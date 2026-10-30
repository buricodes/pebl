use crate::app::{App, Env};
use crate::backend::tmux::Tmux;
use crate::ipc::{read_lines, write_line, Hello, Input, Role};
use crate::{paths, store};
use anyhow::{bail, Context, Result};
use std::io::{BufRead, BufReader, Write};
use std::os::unix::net::{UnixListener, UnixStream};
use std::sync::mpsc::{self, RecvTimeoutError};
use std::time::{Duration, Instant};

enum Msg {
    Subscribe(Role, UnixStream),
    Input(Role, Input),
    Hook(String, String, serde_json::Value),
    Ctl(Vec<String>, UnixStream),
}

pub fn run() -> Result<()> {
    let sock = paths::sock();
    std::fs::create_dir_all(paths::home())?;
    let start = Instant::now();
    while UnixStream::connect(&sock).is_ok() {
        if start.elapsed() > Duration::from_secs(3) {
            bail!("pebl daemon is already running");
        }
        std::thread::sleep(Duration::from_millis(100));
    }
    let _ = std::fs::remove_file(&sock);
    let listener =
        UnixListener::bind(&sock).with_context(|| format!("binding {}", sock.display()))?;

    let pebl = crate::self_exe();
    write_claude_settings(&pebl)?;
    let cfg = crate::config::load(&paths::config()).unwrap_or_default();
    let env = Env {
        notify: cfg.notify.clone(),
        pebl: pebl.clone(),
        claude: find_in_path("claude").unwrap_or_else(|| "claude".into()),
        settings: paths::claude_settings(),
        sock: sock.clone(),
        editor: crate::config::load(&paths::config())
            .ok()
            .and_then(|c| c.editor)
            .unwrap_or_else(|| "Visual Studio Code".into()),
    };

    let (tx, rx) = mpsc::channel::<Msg>();
    std::thread::spawn(move || {
        for stream in listener.incoming().flatten() {
            let tx = tx.clone();
            std::thread::spawn(move || serve(stream, tx));
        }
    });

    let state = store::load(&paths::state())?;
    let mut app = App::new(state, Tmux::new(paths::tmux_socket()), env);
    app.mascot = crate::config::load(&paths::config())
        .ok()
        .and_then(|c| c.mascot());
    replay_events(&mut app);
    app.update = crate::update::available();
    if cfg.update_check.unwrap_or(true) {
        let sock = sock.clone();
        std::thread::spawn(move || loop {
            if crate::update::due() {
                crate::update::check_now();
                let _ = crate::ipc::ctl(&sock, &["update-ready".into()]);
            }
            std::thread::sleep(Duration::from_secs(60 * 60));
        });
    }
    if crate::telemetry::enabled(cfg.telemetry) {
        std::thread::spawn(|| loop {
            if crate::telemetry::due() {
                crate::telemetry::ping();
            }
            std::thread::sleep(Duration::from_secs(60 * 60));
        });
    }
    app.load_statuslines(&paths::statusline_dir());
    app.search_root = crate::search::root();
    app.start_index();

    let mut subs: Vec<(Role, UnixStream)> = Vec::new();
    let mut last_view = String::new();
    let mut last_header: Vec<String> = Vec::new();
    let mut saved = app.st.clone();
    let mut last_tick = Instant::now();
    loop {
        match rx.recv_timeout(Duration::from_millis(500)) {
            Ok(Msg::Subscribe(role, s)) => {
                subs.push((role, s));
                last_view.clear();
            }
            Ok(Msg::Input(role, input)) => app.handle_input(role, input),
            Ok(Msg::Hook(sid, event, payload)) => app.handle_hook(&sid, &event, &payload),
            Ok(Msg::Ctl(args, mut s)) => {
                let reply = app.handle_ctl(&args);
                let _ = s.write_all(reply.as_bytes());
            }
            Err(RecvTimeoutError::Timeout) => {}
            Err(RecvTimeoutError::Disconnected) => break,
        }
        if last_tick.elapsed() >= Duration::from_secs(1) {
            last_tick = Instant::now();
            app.tick();
        }
        if !app.outbox.is_empty() {
            let batch = std::mem::take(&mut app.outbox);
            let cfg = app.env_notify();
            std::thread::spawn(move || crate::notify::deliver(batch, &cfg));
        }

        let view = app.view();
        let json = serde_json::to_string(&view)?;
        if json != last_view {
            subs.retain_mut(|(_, s)| write_line(s, &view).is_ok());
            last_view = json;
        }
        if app.ui.is_some() {
            let header = app.header();
            if header != last_header {
                use crate::backend::UiBackend;
                if Tmux::new(paths::tmux_socket()).set_header(&header).is_ok() {
                    let grew = header.len() != last_header.len() && !last_header.is_empty();
                    last_header = header;
                    if grew {
                        app.handle_ctl(&["resized".to_string()]);
                    }
                }
            }
        }
        if app.st != saved {
            if let Err(e) = store::save(&paths::state(), &app.st) {
                log(&format!("saving state: {e:#}"));
            }
            saved = app.st.clone();
        }
    }
    Ok(())
}

fn serve(stream: UnixStream, tx: mpsc::Sender<Msg>) {
    let Ok(read) = stream.try_clone() else { return };
    let mut reader = BufReader::new(read);
    let mut first = String::new();
    if reader.read_line(&mut first).is_err() {
        return;
    }
    match serde_json::from_str::<Hello>(&first) {
        Ok(Hello::View { role }) => {
            if tx.send(Msg::Subscribe(role, stream)).is_err() {
                return;
            }
            read_lines::<Input>(reader, |i| tx.send(Msg::Input(role, i)).is_ok());
        }
        Ok(Hello::Hook {
            sid,
            event,
            payload,
        }) => {
            let _ = tx.send(Msg::Hook(sid, event, payload));
        }
        Ok(Hello::Ctl { args }) => {
            let _ = tx.send(Msg::Ctl(args, stream));
        }
        Err(_) if first.trim().is_empty() => {}
        Err(e) => log(&format!("bad hello: {e}")),
    }
}

fn replay_events(app: &mut App<Tmux>) {
    let path = paths::events_log();
    let Ok(text) = std::fs::read_to_string(&path) else {
        return;
    };
    for line in text.lines() {
        if let Ok(Hello::Hook {
            sid,
            event,
            payload,
        }) = serde_json::from_str(line)
        {
            app.handle_hook(&sid, &event, &payload);
        }
    }
    let _ = std::fs::remove_file(path);
}

pub fn write_claude_settings(pebl: &str) -> Result<()> {
    let events = [
        "SessionStart",
        "UserPromptSubmit",
        "PreToolUse",
        "PostToolUse",
        "PermissionRequest",
        "Notification",
        "Stop",
        "StopFailure",
        "SessionEnd",
    ];
    let mut hooks = serde_json::Map::new();
    for e in events {
        let cmd = format!("{} hook {e}", crate::backend::tmux::sh_quote(pebl));
        hooks.insert(
            e.into(),
            serde_json::json!([{ "hooks": [{ "type": "command", "command": cmd, "timeout": 5 }] }]),
        );
    }
    let mut doc = serde_json::json!({ "hooks": hooks });
    let statusline = crate::config::load(&paths::config())
        .ok()
        .and_then(|c| c.statusline)
        .unwrap_or(true);
    if statusline {
        doc["statusLine"] = serde_json::json!({
            "type": "command",
            "command": format!("{} statusline", crate::backend::tmux::sh_quote(pebl)),
        });
    }
    std::fs::write(paths::claude_settings(), serde_json::to_vec_pretty(&doc)?)?;
    Ok(())
}

pub fn find_in_path(bin: &str) -> Option<String> {
    let path = std::env::var_os("PATH")?;
    std::env::split_paths(&path)
        .map(|d| d.join(bin))
        .find(|p| p.is_file())
        .map(|p| p.display().to_string())
}

pub fn log(msg: &str) {
    if let Ok(mut f) = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(paths::daemon_log())
    {
        let _ = writeln!(f, "[{}] {msg}", crate::model::now());
    }
}
