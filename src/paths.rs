use std::path::PathBuf;

pub fn home() -> PathBuf {
    if let Some(h) = std::env::var_os("PEBL_HOME") {
        return PathBuf::from(h);
    }
    dirs::home_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join(".pebl")
}

pub fn state() -> PathBuf {
    home().join("state.json")
}

pub fn config() -> PathBuf {
    home().join("config.toml")
}

pub fn sock() -> PathBuf {
    if let Some(s) = std::env::var_os("PEBL_SOCK") {
        return PathBuf::from(s);
    }
    home().join("pebl.sock")
}

pub fn statusline_dir() -> PathBuf {
    home().join("statusline")
}

pub fn events_log() -> PathBuf {
    home().join("events.log")
}

pub fn daemon_log() -> PathBuf {
    home().join("daemon.log")
}

pub fn claude_settings() -> PathBuf {
    home().join("claude-settings.json")
}

pub fn tmux_conf() -> PathBuf {
    home().join("tmux.conf")
}

pub fn worktrees() -> PathBuf {
    home().join("worktrees")
}

pub fn tmux_socket() -> String {
    std::env::var("PEBL_TMUX_SOCKET").unwrap_or_else(|_| "pebl".into())
}
