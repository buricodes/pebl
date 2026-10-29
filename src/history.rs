use std::collections::HashMap;
use std::io::{Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

const RECENT: Duration = Duration::from_secs(60 * 86400);
const MAX: usize = 12;

pub fn scan() -> Vec<(PathBuf, usize)> {
    let Some(home) = dirs::home_dir() else {
        return Vec::new();
    };
    scan_dir(&home.join(".claude/projects"), &home)
        .into_iter()
        .map(|(p, n, _)| (p, n))
        .collect()
}

pub fn scan_dir(root: &Path, home: &Path) -> Vec<(PathBuf, usize, SystemTime)> {
    let mut found = Vec::new();
    let Ok(entries) = std::fs::read_dir(root) else {
        return Vec::new();
    };
    let cutoff = SystemTime::now() - RECENT;
    for e in entries.flatten() {
        let mut logs: Vec<(PathBuf, SystemTime)> = std::fs::read_dir(e.path())
            .into_iter()
            .flatten()
            .flatten()
            .filter(|f| f.path().extension().is_some_and(|x| x == "jsonl"))
            .filter_map(|f| Some((f.path(), f.metadata().ok()?.modified().ok()?)))
            .collect();
        if logs.is_empty() {
            continue;
        }
        logs.sort_by_key(|b| std::cmp::Reverse(b.1));
        let newest = logs[0].1;
        if newest < cutoff {
            continue;
        }
        let Some(cwd) = logs.iter().take(3).find_map(|(p, _)| cwd_of(p)) else {
            continue;
        };
        if !keep(&cwd, home) {
            continue;
        }
        found.push((cwd, logs.len(), newest));
    }
    found.sort_by_key(|b| std::cmp::Reverse(b.2));
    found.dedup_by(|a, b| a.0 == b.0);
    found.truncate(MAX);
    found
}

fn keep(cwd: &Path, home: &Path) -> bool {
    let s = cwd.to_string_lossy();
    cwd.is_dir()
        && cwd != home
        && cwd.starts_with(home)
        && !s.contains("/.pebl/")
        && !s.contains("/.claude/")
        && !s.contains("/Library/")
}

fn cwd_of(path: &Path) -> Option<PathBuf> {
    use std::io::{BufRead, BufReader};
    let f = std::fs::File::open(path).ok()?;
    for line in BufReader::new(f).lines().take(30).map_while(Result::ok) {
        if let Ok(v) = serde_json::from_str::<serde_json::Value>(&line) {
            if let Some(c) = v.get("cwd").and_then(|c| c.as_str()) {
                return Some(PathBuf::from(c));
            }
        }
    }
    None
}

pub fn has_transcript(claude_id: &str) -> bool {
    transcript(claude_id).is_some()
}

pub fn transcript(claude_id: &str) -> Option<PathBuf> {
    let home = dirs::home_dir()?;
    let name = format!("{claude_id}.jsonl");
    std::fs::read_dir(home.join(".claude/projects"))
        .into_iter()
        .flatten()
        .flatten()
        .map(|d| d.path().join(&name))
        .find(|p| p.exists())
}

pub fn read_title(path: &Path, offset: u64) -> (Option<String>, u64) {
    let Ok(mut f) = std::fs::File::open(path) else {
        return (None, offset);
    };
    if f.seek(SeekFrom::Start(offset)).is_err() {
        return (None, offset);
    }
    let mut buf = Vec::new();
    if f.read_to_end(&mut buf).is_err() {
        return (None, offset);
    }
    let Some(end) = buf.iter().rposition(|b| *b == b'\n') else {
        return (None, offset);
    };
    let (mut ai, mut custom) = (None, None);
    for line in String::from_utf8_lossy(&buf[..end]).lines() {
        if !line.contains("Title\"") {
            continue;
        }
        let Ok(v) = serde_json::from_str::<serde_json::Value>(line) else {
            continue;
        };
        if let Some(t) = v.get("aiTitle").and_then(|t| t.as_str()) {
            ai = Some(t.to_string());
        }
        if let Some(t) = v.get("customTitle").and_then(|t| t.as_str()) {
            custom = Some(t.to_string());
        }
    }
    (custom.or(ai), offset + end as u64 + 1)
}

#[derive(Clone, Debug, Default)]
pub struct Tokens {
    pub messages: HashMap<String, (u64, u64, String)>,
    pub by_day: HashMap<String, (u64, u64)>,
    pub last_text: String,
}

impl Tokens {
    pub fn total(&self) -> (u64, u64) {
        self.by_day
            .values()
            .fold((0, 0), |(a, b), (i, o)| (a + i, b + o))
    }

    pub fn on(&self, day: &str) -> (u64, u64) {
        self.by_day.get(day).copied().unwrap_or((0, 0))
    }
}

pub fn runnable(text: &str) -> Vec<String> {
    let shellish = [
        "", "bash", "sh", "zsh", "shell", "console", "terminal", "fish",
    ];
    let mut blocks: Vec<Vec<String>> = Vec::new();
    let mut cur: Option<(String, Vec<String>)> = None;
    for line in text.lines() {
        let t = line.trim_start();
        if let Some(rest) = t.strip_prefix("```") {
            match cur.take() {
                Some((lang, lines)) => {
                    if shellish.contains(&lang.as_str()) {
                        blocks.push(lines);
                    }
                }
                None => cur = Some((rest.trim().to_lowercase(), Vec::new())),
            }
            continue;
        }
        if let Some((_, lines)) = &mut cur {
            let l = line.trim_end();
            let l = l.strip_prefix("$ ").unwrap_or(l);
            if !l.trim().is_empty() && !l.trim_start().starts_with('#') {
                lines.push(l.to_string());
            }
        }
    }
    blocks
        .into_iter()
        .rev()
        .find(|b| !b.is_empty())
        .unwrap_or_default()
}

pub fn read_tokens(path: &Path, offset: u64, t: &mut Tokens) -> u64 {
    let Ok(mut f) = std::fs::File::open(path) else {
        return offset;
    };
    if f.seek(SeekFrom::Start(offset)).is_err() {
        return offset;
    }
    let mut buf = Vec::new();
    if f.read_to_end(&mut buf).is_err() {
        return offset;
    }
    let Some(end) = buf.iter().rposition(|b| *b == b'\n') else {
        return offset;
    };
    for line in String::from_utf8_lossy(&buf[..end]).lines() {
        if !line.contains("\"usage\"") || !line.contains("\"assistant\"") {
            continue;
        }
        let Ok(v) = serde_json::from_str::<serde_json::Value>(line) else {
            continue;
        };
        let Some(msg) = v.get("message") else {
            continue;
        };
        if let Some(parts) = msg.get("content").and_then(|c| c.as_array()) {
            let text: Vec<&str> = parts
                .iter()
                .filter(|p| p.get("type").and_then(|t| t.as_str()) == Some("text"))
                .filter_map(|p| p.get("text").and_then(|t| t.as_str()))
                .collect();
            if !text.is_empty() {
                t.last_text = text.join("\n");
            }
        }
        let Some(u) = msg.get("usage") else { continue };
        let n = |k: &str| u.get(k).and_then(|x| x.as_u64()).unwrap_or(0);
        let input =
            n("input_tokens") + n("cache_creation_input_tokens") + n("cache_read_input_tokens");
        let output = n("output_tokens");
        let id = msg
            .get("id")
            .and_then(|x| x.as_str())
            .map(str::to_string)
            .unwrap_or_else(|| format!("line{}", t.messages.len()));
        let day = v
            .get("timestamp")
            .and_then(|x| x.as_str())
            .and_then(|s| s.get(..10))
            .unwrap_or("")
            .to_string();
        let (pi, po, pday) = t.messages.get(&id).cloned().unwrap_or((0, 0, day.clone()));
        let e = t.by_day.entry(pday).or_default();
        e.0 = e.0.saturating_sub(pi);
        e.1 = e.1.saturating_sub(po);
        let e = t.by_day.entry(day.clone()).or_default();
        e.0 += input;
        e.1 += output;
        t.messages.insert(id, (input, output, day));
    }
    offset + end as u64 + 1
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_recent_folders_with_counts() {
        let tmp = tempfile::tempdir().unwrap();
        let home = tmp.path();
        let proj = home.join("code/nova-api");
        std::fs::create_dir_all(&proj).unwrap();
        let logs = home.join(".claude/projects/-code-nova-api");
        std::fs::create_dir_all(&logs).unwrap();
        let line = format!("{{\"type\":\"user\",\"cwd\":\"{}\"}}\n", proj.display());
        std::fs::write(logs.join("a.jsonl"), &line).unwrap();
        std::fs::write(logs.join("b.jsonl"), &line).unwrap();
        let found = scan_dir(&home.join(".claude/projects"), home);
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].0, proj);
        assert_eq!(found[0].1, 2);
    }

    #[test]
    fn finds_the_last_shell_block() {
        let text = "Run this:\n\n```ts\nconst a = 1\n```\n\n```bash\n$ docker compose up -d redis\n# comment\nnpm run dev   # watches src\n\ncurl localhost:4000\n```\nThen check.";
        assert_eq!(
            runnable(text),
            vec![
                "docker compose up -d redis",
                "npm run dev   # watches src",
                "curl localhost:4000"
            ]
        );
        assert!(runnable("no code here").is_empty());
        assert!(runnable("```json\n{}\n```").is_empty());
    }

    #[test]
    fn counts_tokens_once_per_message() {
        let tmp = tempfile::tempdir().unwrap();
        let p = tmp.path().join("t.jsonl");
        let msg = |id: &str, out: u64, day: &str| {
            format!("{{\"type\":\"assistant\",\"timestamp\":\"{day}T10:00:00Z\",\"message\":{{\"id\":\"{id}\",\"usage\":{{\"input_tokens\":10,\"cache_read_input_tokens\":90,\"output_tokens\":{out}}}}}}}\n")
        };
        std::fs::write(&p, msg("a", 5, "2026-10-28") + &msg("a", 20, "2026-10-28")).unwrap();
        let mut t = Tokens::default();
        let off = read_tokens(&p, 0, &mut t);
        assert_eq!(t.total(), (100, 20));
        let mut s = std::fs::read_to_string(&p).unwrap();
        s.push_str(&msg("b", 7, "2026-10-29"));
        std::fs::write(&p, s).unwrap();
        read_tokens(&p, off, &mut t);
        assert_eq!(t.total(), (200, 27));
        assert_eq!(t.on("2026-10-29"), (100, 7));
    }

    #[test]
    fn reads_titles_incrementally() {
        let tmp = tempfile::tempdir().unwrap();
        let p = tmp.path().join("t.jsonl");
        std::fs::write(
            &p,
            "{\"type\":\"user\"}\n{\"type\":\"ai-title\",\"aiTitle\":\"Fix login\"}\n{\"part",
        )
        .unwrap();
        let (t, off) = read_title(&p, 0);
        assert_eq!(t.as_deref(), Some("Fix login"));
        let (t2, off2) = read_title(&p, off);
        assert_eq!((t2, off2), (None, off));
        let mut s = std::fs::read_to_string(&p).unwrap();
        s.push_str("ial\":1}\n{\"type\":\"custom-title\",\"customTitle\":\"login\"}\n");
        std::fs::write(&p, s).unwrap();
        assert_eq!(read_title(&p, off).0.as_deref(), Some("login"));
    }
}
