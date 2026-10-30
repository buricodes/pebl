mod app;
mod backend;
mod client;
mod complete;
mod config;
mod daemon;
mod diff;
mod gallery;
mod git;
mod history;
mod hotkey;
mod ipc;
mod model;
mod notify;
mod paths;
mod review;
mod search;
mod status;
mod store;
mod telemetry;
mod ui;
mod update;
mod view;

use anyhow::{bail, Context, Result};
use backend::tmux::{Tmux, BG_SESSION, UI_SESSION};
use clap::{Parser, Subcommand};
use std::time::{Duration, Instant};

#[derive(Parser)]
#[command(
    name = "pebl",
    version,
    about = "Run several Claude Code sessions side by side, in your terminal"
)]
struct Cli {
    #[command(subcommand)]
    cmd: Option<Cmd>,
}

#[derive(Subcommand)]
enum Cmd {
    #[command(about = "Add a project (default: current directory)")]
    Add {
        path: Option<String>,
        #[arg(long)]
        name: Option<String>,
    },
    #[command(about = "Start a Claude Code session")]
    New {
        #[arg(help = "Project name or path (default: the selected project)")]
        project: Option<String>,
        #[arg(long, short)]
        name: Option<String>,
        #[arg(long, short, help = "First prompt to send")]
        prompt: Option<String>,
    },
    #[command(about = "List sessions with status (tab separated)")]
    Ls,
    #[command(about = "Check tmux, claude and the hook setup")]
    Doctor,
    #[command(about = "Bring pebl to the front, or open it in a new terminal window")]
    Summon,
    #[command(about = "Global shortcut that summons pebl (macOS)")]
    Hotkey {
        #[command(subcommand)]
        action: HotkeyCmd,
    },
    #[command(
        about = "Restart pebl's own panes and daemon on the installed version (sessions keep running)"
    )]
    Reload,
    #[command(
        about = "Update pebl to the latest version from GitHub, then reload it (sessions keep running)"
    )]
    Update,
    #[command(about = "Stop pebl and every session it runs")]
    Kill {
        #[arg(long)]
        yes: bool,
    },
    #[command(about = "Play pebl's notification sounds so you can hear them")]
    Chime {
        #[arg(help = "waiting, done or error (default: all three)")]
        kind: Option<String>,
    },
    #[command(about = "Pick the sidebar mascot (shows every mascot in every mood)")]
    Mascots {
        #[arg(long, hide = true)]
        svg: Option<u64>,
    },
    #[command(about = "Render a sample screen without starting anything")]
    Preview {
        #[arg(long, default_value_t = 120)]
        width: u16,
        #[arg(long, default_value_t = 40)]
        height: u16,
    },
    #[command(hide = true)]
    Hook { event: String },
    #[command(hide = true)]
    Statusline,
    #[command(hide = true)]
    Review {
        #[arg(long)]
        sid: String,
        #[arg(long)]
        cwd: std::path::PathBuf,
        #[arg(long, default_value = "")]
        title: String,
        #[arg(long, default_value = "truecolor")]
        color: String,
    },
    #[command(hide = true)]
    Daemon,
    #[command(hide = true)]
    View { role: String },
    #[command(hide = true)]
    Ctl { args: Vec<String> },
}

#[derive(Subcommand)]
enum HotkeyCmd {
    #[command(about = "Install the launchd agent")]
    Install {
        #[arg(
            long,
            default_value = "cmd",
            help = "Double-tap ctrl, option, cmd, shift or space; or a combo like cmd+shift+p (needs no permission)"
        )]
        trigger: String,
    },
    Uninstall,
    Status,
}

fn main() {
    let cli = Cli::parse();
    if let Some(Cmd::Hook { event }) = &cli.cmd {
        status::hook::run(event);
        return;
    }
    if let Err(e) = run(cli) {
        eprintln!("pebl: {e:#}");
        std::process::exit(1);
    }
}

fn run(cli: Cli) -> Result<()> {
    match cli.cmd {
        None => open(),
        Some(Cmd::Add { path, name }) => {
            let path = match path {
                Some(p) => p,
                None => std::env::current_dir()?.display().to_string(),
            };
            let mut args = vec!["add".to_string(), path];
            args.extend(name);
            print_reply(&ctl_started(&args)?)
        }
        Some(Cmd::New {
            project,
            name,
            prompt,
        }) => print_reply(&ctl_started(&[
            "new".into(),
            project.unwrap_or_default(),
            name.unwrap_or_default(),
            prompt.unwrap_or_default(),
        ])?),
        Some(Cmd::Ls) => {
            let out = match ipc::ctl(&paths::sock(), &["ls".into()]) {
                Ok(s) => s,
                Err(_) => offline_ls()?,
            };
            print!("{out}");
            Ok(())
        }
        Some(Cmd::Doctor) => doctor(),
        Some(Cmd::Summon) => hotkey::summon(config::load(&paths::config())?.terminal.as_deref()),
        Some(Cmd::Hotkey { action }) => match action {
            HotkeyCmd::Install { trigger } => {
                hotkey::install(&trigger)?;
                if trigger.contains('+') {
                    println!("Hotkey installed: press {trigger} to summon pebl.");
                } else {
                    println!("Hotkey installed: double-tap {trigger} to summon pebl.");
                    println!("Allow \"pebl-hotkey\" under Privacy & Security > Input Monitoring, then run:");
                    println!("  launchctl kickstart -k gui/$(id -u)/com.pebl.hotkey");
                }
                Ok(())
            }
            HotkeyCmd::Uninstall => {
                hotkey::uninstall()?;
                println!("Hotkey removed.");
                Ok(())
            }
            HotkeyCmd::Status => {
                println!("{}", hotkey::status());
                Ok(())
            }
        },
        Some(Cmd::Reload) => reload(),
        Some(Cmd::Update) => update(),
        Some(Cmd::Kill { yes }) => {
            if !yes {
                bail!("this stops every Claude Code session pebl is running; rerun with --yes");
            }
            let _ = Tmux::new(paths::tmux_socket()).kill_server();
            let _ = std::fs::remove_file(paths::sock());
            Ok(())
        }
        Some(Cmd::Mascots { svg: Some(f) }) => {
            println!("{}", gallery::svg(f));
            Ok(())
        }
        Some(Cmd::Mascots { svg: None }) => gallery::run(),
        Some(Cmd::Preview { width, height }) => preview(width, height),
        Some(Cmd::Chime { kind }) => {
            use crate::view::NoticeKind;
            let cfg = config::load(&paths::config())?.notify;
            if !cfg.chime {
                println!("chimes are off (chime = false in ~/.pebl/config.toml)");
            }
            let all = [
                ("waiting", NoticeKind::Waiting, "a session needs you"),
                ("done", NoticeKind::Done, "a session finished"),
                ("error", NoticeKind::Error, "a session hit an error"),
            ];
            for (name, k, what) in all {
                if kind.as_deref().is_some_and(|x| x != name) {
                    continue;
                }
                let sound = match k {
                    NoticeKind::Waiting => &cfg.chime_waiting,
                    NoticeKind::Done => &cfg.chime_done,
                    _ => &cfg.chime_error,
                };
                if sound.is_empty() {
                    println!("{name:<8} off");
                    continue;
                }
                println!("{name:<8} {sound:<10} {what}");
                if !notify::sound_path(sound).exists() {
                    println!(
                        "         (no such sound: {})",
                        notify::sound_path(sound).display()
                    );
                    continue;
                }
                notify::chime(sound);
            }
            Ok(())
        }
        Some(Cmd::Review {
            sid,
            cwd,
            title,
            color,
        }) => {
            let mode = if color == "256" {
                ui::theme::ColorMode::Indexed
            } else {
                ui::theme::ColorMode::Truecolor
            };
            review::run(&sid, &cwd, &title, mode)
        }
        Some(Cmd::Hook { .. }) => Ok(()),
        Some(Cmd::Statusline) => {
            status::hook::statusline();
            Ok(())
        }
        Some(Cmd::Daemon) => daemon::run(),
        Some(Cmd::View { role }) => {
            let role = match role.as_str() {
                "sidebar" => ipc::Role::Sidebar,
                "stage" => ipc::Role::Stage,
                "bar" => ipc::Role::Bar,
                "tabs" => ipc::Role::Tabs,
                _ => bail!("unknown view {role}"),
            };
            client::run(role)
        }
        Some(Cmd::Ctl { args }) => {
            if let Ok(out) = ipc::ctl(&paths::sock(), &args) {
                print!("{}", out);
            }
            Ok(())
        }
    }
}

pub fn self_exe() -> String {
    std::env::current_exe()
        .and_then(std::fs::canonicalize)
        .map(|p| p.display().to_string())
        .unwrap_or_else(|_| "pebl".into())
}

fn print_reply(s: &str) -> Result<()> {
    if let Some(e) = s.strip_prefix("error: ") {
        bail!("{e}");
    }
    if !s.is_empty() {
        println!("{s}");
    }
    Ok(())
}

pub fn truecolor() -> bool {
    let ct = std::env::var("COLORTERM").unwrap_or_default();
    if ct == "truecolor" || ct == "24bit" {
        return true;
    }
    matches!(
        std::env::var("TERM_PROGRAM").unwrap_or_default().as_str(),
        "iTerm.app" | "WezTerm" | "ghostty" | "vscode" | "WarpTerminal"
    )
}

fn daemon_cmd(pebl: &str) -> String {
    format!(
        "{} daemon 2>>{}",
        backend::tmux::sh_quote(pebl),
        backend::tmux::sh_quote(&paths::daemon_log().display().to_string())
    )
}

const REPO: &str = "https://github.com/buricodes/pebl";

fn update() -> Result<()> {
    which("cargo").context("cargo isn't installed; get it from https://rustup.rs")?;
    let latest = update::check_now();
    if let Some(v) = &latest {
        if !update::is_newer(v, update::current()) {
            println!("pebl {} is the latest version.", update::current());
            return Ok(());
        }
        println!("Updating pebl {} to {v}", update::current());
    } else {
        println!("Updating pebl from {REPO}");
    }
    let mut args = vec!["install", "--git", REPO, "--locked", "--force"];
    let tag = latest.map(|v| format!("v{v}"));
    if let Some(t) = &tag {
        args.extend(["--tag", t.as_str()]);
    }
    let ok = std::process::Command::new("cargo")
        .args(&args)
        .status()
        .context("running cargo")?
        .success();
    if !ok {
        bail!("the update didn't build; your current pebl is unchanged");
    }
    println!("pebl is up to date.");
    reload()
}

fn reload() -> Result<()> {
    let t = Tmux::new(paths::tmux_socket());
    if !t.has_session(BG_SESSION) {
        println!("pebl isn't running; nothing to reload.");
        return Ok(());
    }
    let pebl = self_exe();
    t.run(&[
        "respawn-pane",
        "-k",
        "-t",
        &format!("={BG_SESSION}:daemon"),
        &daemon_cmd(&pebl),
    ])?;
    let start = Instant::now();
    while ipc::ctl(&paths::sock(), &["ls".into()]).is_err()
        && start.elapsed() < Duration::from_secs(5)
    {
        std::thread::sleep(Duration::from_millis(100));
    }
    ensure_daemon()?;
    t.respawn_views(&pebl)?;
    println!("pebl reloaded; sessions kept running.");
    Ok(())
}

fn ensure_daemon() -> Result<Tmux> {
    which("tmux").with_context(|| format!("tmux is not installed ({})", tmux_hint()))?;
    std::fs::create_dir_all(paths::home())?;
    let cfg = config::load(&paths::config())?;
    let pebl = self_exe();
    std::fs::write(
        paths::tmux_conf(),
        backend::tmux::config(&pebl, truecolor(), &cfg),
    )?;
    let conf = paths::tmux_conf().display().to_string();
    let t = Tmux::new(paths::tmux_socket());

    if ipc::ctl(&paths::sock(), &["ls".into()]).is_ok() {
        let _ = t.run(&["source-file", &conf]);
        return Ok(t);
    }
    let cmd = daemon_cmd(&pebl);
    if t.has_session(BG_SESSION) {
        let _ = t.run(&["source-file", &conf]);
        t.run(&[
            "new-window",
            "-d",
            "-t",
            &format!("={BG_SESSION}:"),
            "-n",
            "daemon",
            &cmd,
        ])?;
    } else {
        let mut c = t.command();
        c.args([
            "-f",
            &conf,
            "new-session",
            "-d",
            "-s",
            BG_SESSION,
            "-n",
            "daemon",
        ]);
        let out = c.output().context("starting tmux")?;
        if !out.status.success() {
            bail!(
                "starting tmux: {}",
                String::from_utf8_lossy(&out.stderr).trim()
            );
        }
        let _ = t.run(&["source-file", &conf]);
        t.run(&[
            "respawn-pane",
            "-k",
            "-t",
            &format!("={BG_SESSION}:daemon"),
            &cmd,
        ])?;
    }
    let start = Instant::now();
    while ipc::ctl(&paths::sock(), &["ls".into()]).is_err() {
        if start.elapsed() > Duration::from_secs(5) {
            bail!(
                "the pebl daemon did not start; see {}",
                paths::daemon_log().display()
            );
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    Ok(t)
}

fn ctl_started(args: &[String]) -> Result<String> {
    ensure_daemon()?;
    ipc::ctl(&paths::sock(), args)
}

fn tmux_hint() -> &'static str {
    if cfg!(target_os = "macos") {
        "brew install tmux"
    } else {
        "sudo apt install tmux   (or your distro's package manager)"
    }
}

fn tmux_version() -> Option<(u32, u32)> {
    let out = std::process::Command::new("tmux").arg("-V").output().ok()?;
    let text = String::from_utf8_lossy(&out.stdout);
    let v = text.split_whitespace().nth(1)?;
    let mut parts = v
        .split(|c: char| !c.is_ascii_digit())
        .filter(|p| !p.is_empty());
    Some((
        parts.next()?.parse().ok()?,
        parts.next().unwrap_or("0").parse().ok()?,
    ))
}

fn preflight() -> Result<()> {
    let mut missing: Vec<String> = Vec::new();
    match which("tmux") {
        None => missing.push(format!(
            "tmux is not installed. Install it with:\n      {}",
            tmux_hint()
        )),
        Some(_) => {
            if let Some((major, minor)) = tmux_version() {
                if (major, minor) < (3, 2) {
                    missing.push(format!(
                        "tmux {major}.{minor} is too old; pebl needs 3.2 or newer. Update it with:\n      {}",
                        tmux_hint()
                    ));
                }
            }
        }
    }
    if which("claude").is_none() {
        missing.push(
            "Claude Code is not installed (or `claude` is not on your PATH). Install it from:\n      https://docs.claude.com/en/docs/claude-code\n    then run `claude` once to log in."
                .to_string(),
        );
    }
    if missing.is_empty() {
        return Ok(());
    }
    eprintln!(
        "pebl needs {} first:\n",
        if missing.len() == 1 {
            "one thing"
        } else {
            "a few things"
        }
    );
    for (i, m) in missing.iter().enumerate() {
        eprintln!("  {}. {m}\n", i + 1);
    }
    eprintln!("Then run `pebl` again. `pebl doctor` checks everything, and the full setup is at\nhttps://github.com/buricodes/pebl#install");
    std::process::exit(1);
}

fn open() -> Result<()> {
    preflight()?;
    let t = ensure_daemon()?;
    let (w, h) = crossterm::terminal::size().unwrap_or((120, 40));
    let cwd = std::env::current_dir()?.display().to_string();
    let color = if truecolor() { "truecolor" } else { "256" };
    let reply = ipc::ctl(
        &paths::sock(),
        &[
            "ensure-ui".into(),
            cwd,
            w.to_string(),
            h.to_string(),
            color.into(),
        ],
    )?;
    if let Some(e) = reply.strip_prefix("error: ") {
        bail!("{e}");
    }
    let mut c = t.command();
    c.args(["attach-session", "-t", &format!("={UI_SESSION}")]);
    if std::env::var_os("TMUX").is_some() {
        eprintln!("pebl: running inside tmux; nesting a second tmux client");
    }
    c.status().context("attaching to tmux")?;
    Ok(())
}

fn which(bin: &str) -> Option<String> {
    daemon::find_in_path(bin)
}

fn offline_ls() -> Result<String> {
    let st = store::load(&paths::state())?;
    let mut out = String::new();
    for p in &st.projects {
        for s in st.sessions_of(&p.id) {
            out.push_str(&format!("{}\t{}\t{:?}\n", p.name, s.name, s.status).to_lowercase());
        }
    }
    Ok(out)
}

fn doctor() -> Result<()> {
    let mut ok = true;
    let mut check = |name: &str, res: Result<String, String>| match res {
        Ok(s) => println!("  ✓ {name}  {s}"),
        Err(s) => {
            ok = false;
            println!("  ✕ {name}  {s}");
        }
    };
    let tmux = std::process::Command::new("tmux").arg("-V").output();
    check(
        "tmux",
        match tmux {
            Ok(o) if o.status.success() => {
                let v = String::from_utf8_lossy(&o.stdout).trim().to_string();
                let num: f32 = v
                    .trim_start_matches("tmux ")
                    .trim_end_matches(char::is_alphabetic)
                    .parse()
                    .unwrap_or(0.0);
                if num >= 3.2 {
                    Ok(v)
                } else {
                    Err(format!("{v} (need >= 3.2)"))
                }
            }
            _ => Err("not installed (brew install tmux)".into()),
        },
    );
    check(
        "claude",
        which("claude").ok_or_else(|| "not on PATH".to_string()),
    );
    let creds = dirs::home_dir()
        .map(|h| h.join(".claude"))
        .filter(|p| p.exists());
    check(
        "claude config",
        creds
            .map(|p| p.display().to_string())
            .ok_or_else(|| "~/.claude missing; run `claude` once".into()),
    );
    check("pebl home", Ok(paths::home().display().to_string()));
    let daemon = ipc::ctl(&paths::sock(), &["ls".into()]).is_ok();
    println!(
        "  · daemon  {}",
        if daemon {
            "running"
        } else {
            "not running (starts with `pebl`)"
        }
    );
    let hooks = paths::claude_settings();
    if hooks.exists() {
        println!(
            "  · hooks  {} (passed per session with --settings)",
            hooks.display()
        );
    } else {
        println!("  · hooks  written on first launch");
    }
    println!("  · hotkey  {}", hotkey::status());
    match update::check_now() {
        Some(latest) if update::is_newer(&latest, update::current()) => println!(
            "  · version  {} (pebl {latest} is out: run `pebl update`)",
            update::current()
        ),
        Some(_) => println!("  · version  {} (latest)", update::current()),
        None => println!(
            "  · version  {} (couldn't check GitHub for a newer one)",
            update::current()
        ),
    }
    let tele = crate::config::load(&paths::config())
        .ok()
        .and_then(|c| c.telemetry);
    if telemetry::enabled(tele) {
        println!("  · usage ping  on: an anonymous id and the version, once a day (telemetry = false turns it off)");
    } else {
        println!("  · usage ping  off");
    }
    if !ok {
        bail!("some checks failed");
    }
    Ok(())
}

fn preview(width: u16, height: u16) -> Result<()> {
    use model::Status::*;
    use view::*;
    let row = |n: &str, s, m: &str| SessionRow {
        name: n.into(),
        status: s,
        meta: m.into(),
        col: None,
        ctx: None,
        marked: None,
    };
    let projects = vec![
        ProjectRow {
            name: "nova-api".into(),
            open: true,
            total: 3,
            waiting: 1,
            sessions: vec![
                row("auth-refactor", Running, "2m"),
                row("migrate-staging", Waiting, "input"),
                row("docs-pass", Idle, "1h"),
            ],
        },
        ProjectRow {
            name: "billing-svc".into(),
            open: true,
            total: 3,
            waiting: 0,
            sessions: vec![
                row("onboarding-flow", Running, "14s"),
                row("e2e-tests", Error, "exit 1"),
                row("a11y-audit", Idle, "3h"),
            ],
        },
        ProjectRow {
            name: "infra".into(),
            open: false,
            total: 3,
            waiting: 0,
            sessions: vec![],
        },
    ];
    let v = View {
        projects,
        cursor: Some(Cursor::Session(0, 0)),
        active: Some(Active {
            project: "nova-api".into(),
            session: "auth-refactor".into(),
            status: Running,
            age: "2m".into(),
        }),
        focus: Focus::Main,
        layout: Layout::for_width(width, false),
        counts: Counts {
            running: 3,
            waiting: 1,
            idle: 4,
            error: 1,
            exited: 0,
        },
        path: "~/code/nova-api".into(),
        stage: Stage::Help,
        color: if truecolor() {
            ui::theme::ColorMode::Truecolor
        } else {
            ui::theme::ColorMode::Indexed
        },
        ..Default::default()
    };
    let buf = ui::screen::render(&v, width, height, ui::screen::Main::Stage);
    let mut out = String::new();
    let mut last = ratatui::style::Style::default();
    for y in 0..height {
        for x in 0..width {
            let c = &buf[(x, y)];
            let st = c.style();
            if st != last {
                out.push_str("\x1b[0m");
                out.push_str(&ansi(&st));
                last = st;
            }
            out.push_str(c.symbol());
        }
        out.push_str("\x1b[0m\n");
        last = ratatui::style::Style::default();
    }
    print!("{out}");
    Ok(())
}

fn ansi(st: &ratatui::style::Style) -> String {
    use ratatui::style::{Color, Modifier};
    let mut s = String::new();
    let col = |c: Color, base: u8| match c {
        Color::Rgb(r, g, b) => format!("\x1b[{base};2;{r};{g};{b}m"),
        Color::Indexed(i) => format!("\x1b[{base};5;{i}m"),
        _ => String::new(),
    };
    if let Some(fg) = st.fg {
        s.push_str(&col(fg, 38));
    }
    if let Some(bg) = st.bg {
        s.push_str(&col(bg, 48));
    }
    if st.add_modifier.contains(Modifier::BOLD) {
        s.push_str("\x1b[1m");
    }
    s
}
