use crate::ipc::{write_line, Hello};
use crate::paths;
use std::io::{Read, Write};
use std::time::Duration;

pub fn statusline() {
    let mut input = String::new();
    let _ = std::io::stdin().take(1 << 20).read_to_string(&mut input);
    let payload: serde_json::Value =
        serde_json::from_str(&input).unwrap_or(serde_json::Value::Null);
    if let Ok(sid) = std::env::var("PEBL_SESSION_ID") {
        let dir = paths::statusline_dir();
        if std::fs::create_dir_all(&dir).is_ok() {
            let tmp = dir.join(format!(".{sid}.tmp"));
            if std::fs::write(&tmp, &input).is_ok() {
                let _ = std::fs::rename(&tmp, dir.join(format!("{sid}.json")));
            }
        }
        let msg = Hello::Hook {
            sid,
            event: "StatusLine".into(),
            payload: payload.clone(),
        };
        let sock = std::env::var_os("PEBL_SOCK")
            .map(Into::into)
            .unwrap_or_else(paths::sock);
        let _ = std::os::unix::net::UnixStream::connect(&sock).and_then(|mut s| {
            s.set_write_timeout(Some(Duration::from_millis(300)))?;
            write_line(&mut s, &msg).map_err(std::io::Error::other)
        });
    }
    let mut parts = Vec::new();
    if let Some(m) = payload
        .pointer("/model/display_name")
        .and_then(|v| v.as_str())
    {
        parts.push(m.to_string());
    }
    if let Some(p) = payload
        .pointer("/context_window/used_percentage")
        .and_then(|v| v.as_f64())
    {
        parts.push(format!("{}% context", p.round()));
    }
    for (key, label) in [("five_hour", "5h"), ("seven_day", "week")] {
        if let Some(p) = payload
            .pointer(&format!("/rate_limits/{key}/used_percentage"))
            .and_then(|v| v.as_f64())
        {
            parts.push(format!("{label} {}%", p.round()));
        }
    }
    if !parts.is_empty() {
        print!("\x1b[38;5;241m{}\x1b[0m", parts.join(" · "));
    }
}

fn switch(arg: &str) -> Result<crate::profile::Account, String> {
    let home = dirs::home_dir().unwrap_or_default();
    let list = crate::profile::accounts(&crate::profile::load(), &home);
    let dir = std::env::var_os("CLAUDE_CONFIG_DIR").map(std::path::PathBuf::from);
    let current = crate::profile::current_of(&list, dir.as_deref());
    crate::profile::switch_target(&list, &current, arg)
}

fn block(reason: String) {
    let out = serde_json::json!({
        "decision": "block",
        "reason": reason,
        "hookSpecificOutput": {
            "hookEventName": "UserPromptSubmit",
            "suppressOriginalPrompt": true,
        },
    });
    print!("{out}");
    let _ = std::io::stdout().flush();
}

pub fn run(event: &str) {
    let mut input = String::new();
    let _ = std::io::stdin().take(1 << 20).read_to_string(&mut input);
    let Ok(sid) = std::env::var("PEBL_SESSION_ID") else {
        return;
    };
    let mut payload: serde_json::Value =
        serde_json::from_str(&input).unwrap_or(serde_json::Value::Null);
    if event == "UserPromptSubmit" {
        let arg = payload
            .get("prompt")
            .and_then(|p| p.as_str())
            .and_then(crate::profile::switch_arg)
            .map(str::to_string);
        if let Some(arg) = arg {
            let target = switch(&arg);
            block(match &target {
                Ok(a) => format!("Switching this session to {}…", a.label()),
                Err(e) => e.clone(),
            });
            let Ok(a) = target else {
                return;
            };
            payload["pebl_switch"] = serde_json::Value::String(a.name);
        }
    }
    let msg = Hello::Hook {
        sid,
        event: event.to_string(),
        payload,
    };
    let sock = std::env::var_os("PEBL_SOCK")
        .map(Into::into)
        .unwrap_or_else(paths::sock);
    let sent = std::os::unix::net::UnixStream::connect(&sock).and_then(|mut s| {
        s.set_write_timeout(Some(Duration::from_millis(500)))?;
        write_line(&mut s, &msg).map_err(std::io::Error::other)
    });
    if sent.is_err() {
        if let Ok(mut f) = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(paths::events_log())
        {
            if let Ok(line) = serde_json::to_string(&msg) {
                let _ = writeln!(f, "{line}");
            }
        }
    }
}
