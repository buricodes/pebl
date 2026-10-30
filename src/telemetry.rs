use crate::paths;
use std::process::Command;

const URL: &str = "https://getpebl.vercel.app/api/ping";

pub fn enabled(setting: Option<bool>) -> bool {
    let off = |k: &str| std::env::var(k).is_ok_and(|v| !v.is_empty() && v != "0");
    setting.unwrap_or(true)
        && !off("DO_NOT_TRACK")
        && !off("PEBL_NO_TELEMETRY")
        && std::env::var_os("PEBL_HOME").is_none()
}

fn install_id() -> Option<String> {
    let path = paths::home().join("install_id");
    if let Ok(id) = std::fs::read_to_string(&path) {
        let id = id.trim();
        if uuid::Uuid::parse_str(id).is_ok() {
            return Some(id.to_string());
        }
    }
    let id = uuid::Uuid::new_v4().to_string();
    std::fs::create_dir_all(paths::home()).ok()?;
    std::fs::write(&path, &id).ok()?;
    Some(id)
}

fn today() -> u64 {
    crate::model::now() / (24 * 60 * 60)
}

pub fn due() -> bool {
    std::fs::read_to_string(paths::home().join("ping"))
        .ok()
        .and_then(|s| s.trim().parse::<u64>().ok())
        != Some(today())
}

pub fn ping() {
    let Some(id) = install_id() else { return };
    let body = serde_json::json!({ "id": id, "version": crate::update::current() }).to_string();
    let sent = Command::new("curl")
        .args([
            "-fsS",
            "-m",
            "10",
            "-X",
            "POST",
            "-H",
            "Content-Type: application/json",
            "-d",
            &body,
            URL,
        ])
        .output()
        .is_ok_and(|o| o.status.success());
    if sent {
        let _ = std::fs::write(paths::home().join("ping"), today().to_string());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn off_when_disabled_in_config() {
        assert!(!enabled(Some(false)));
    }
}
