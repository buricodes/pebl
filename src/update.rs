use crate::paths;
use std::process::Command;

const LATEST: &str = "https://api.github.com/repos/buricodes/pebl/releases/latest";
const DAY: u64 = 24 * 60 * 60;

pub fn current() -> &'static str {
    env!("CARGO_PKG_VERSION")
}

fn parse(v: &str) -> Option<(u64, u64, u64)> {
    let mut it = v.trim().trim_start_matches('v').split(['.', '-']);
    Some((
        it.next()?.parse().ok()?,
        it.next().unwrap_or("0").parse().ok()?,
        it.next().unwrap_or("0").parse().ok()?,
    ))
}

pub fn is_newer(latest: &str, current: &str) -> bool {
    match (parse(latest), parse(current)) {
        (Some(a), Some(b)) => a > b,
        _ => false,
    }
}

fn read_cache() -> Option<(u64, String)> {
    let text = std::fs::read_to_string(paths::update_cache()).ok()?;
    let v: serde_json::Value = serde_json::from_str(&text).ok()?;
    Some((
        v.get("checked")?.as_u64()?,
        v.get("latest")?.as_str()?.to_string(),
    ))
}

pub fn available() -> Option<String> {
    let (_, latest) = read_cache()?;
    is_newer(&latest, current()).then(|| latest.trim_start_matches('v').to_string())
}

pub fn due() -> bool {
    read_cache().is_none_or(|(checked, _)| crate::model::now().saturating_sub(checked) >= DAY)
}

pub fn check_now() -> Option<String> {
    let out = Command::new("curl")
        .args([
            "-fsSL",
            "-m",
            "10",
            "-H",
            "Accept: application/vnd.github+json",
            LATEST,
        ])
        .output()
        .ok()?;
    if !out.status.success() {
        return None;
    }
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).ok()?;
    let tag = v.get("tag_name")?.as_str()?.to_string();
    let doc = serde_json::json!({ "checked": crate::model::now(), "latest": tag });
    let _ = std::fs::create_dir_all(paths::home());
    let _ = std::fs::write(paths::update_cache(), doc.to_string());
    Some(tag.trim_start_matches('v').to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compares_versions() {
        assert!(is_newer("v0.2.0", "0.1.0"));
        assert!(is_newer("0.10.0", "0.9.3"));
        assert!(is_newer("v1.0.0", "0.99.99"));
        assert!(!is_newer("v0.2.0", "0.2.0"));
        assert!(!is_newer("v0.1.9", "0.2.0"));
        assert!(!is_newer("nonsense", "0.2.0"));
    }
}
