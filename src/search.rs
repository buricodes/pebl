use std::collections::HashMap;
use std::io::{Seek, SeekFrom};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::UNIX_EPOCH;

const MAX_LINE: usize = 200_000;
const MAX_TEXT: usize = 2_000;

#[derive(Clone, Debug, Default)]
pub struct Conv {
    pub id: String,
    pub cwd: Option<PathBuf>,
    pub title: String,
    pub custom: bool,
    pub mtime: u64,
    pub texts: Vec<String>,
    offset: u64,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Hit {
    pub id: String,
    pub cwd: Option<PathBuf>,
    pub title: String,
    pub mtime: u64,
    pub snippet: String,
}

#[derive(Default)]
pub struct Index {
    pub convs: HashMap<PathBuf, Conv>,
}

pub type Shared = Arc<Mutex<Index>>;

#[cfg(test)]
pub fn conv(id: &str, cwd: &Path, title: &str, texts: &[&str]) -> Conv {
    Conv {
        id: id.into(),
        cwd: Some(cwd.to_path_buf()),
        title: title.into(),
        texts: texts.iter().map(|t| t.to_string()).collect(),
        mtime: 1,
        ..Default::default()
    }
}

pub fn root() -> Option<PathBuf> {
    dirs::home_dir().map(|h| h.join(".claude/projects"))
}

fn files(root: &Path) -> Vec<(PathBuf, u64, u64)> {
    let mut out = Vec::new();
    let Ok(dirs) = std::fs::read_dir(root) else {
        return out;
    };
    for d in dirs.flatten() {
        let Ok(entries) = std::fs::read_dir(d.path()) else {
            continue;
        };
        for e in entries.flatten() {
            let p = e.path();
            if p.extension().and_then(|x| x.to_str()) != Some("jsonl") {
                continue;
            }
            let Ok(m) = e.metadata() else { continue };
            let mtime = m
                .modified()
                .ok()
                .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
                .map(|d| d.as_secs())
                .unwrap_or(0);
            out.push((p, m.len(), mtime));
        }
    }
    out
}

pub fn refresh(shared: &Shared, root: &Path) {
    let found = files(root);
    {
        let Ok(mut idx) = shared.lock() else { return };
        idx.convs
            .retain(|p, _| found.iter().any(|(f, _, _)| f == p));
    }
    for (path, size, mtime) in found {
        let prev = shared
            .lock()
            .ok()
            .and_then(|idx| idx.convs.get(&path).cloned());
        let mut conv = match prev {
            Some(c) if c.offset == size => continue,
            Some(c) if c.offset < size => c,
            _ => Conv {
                id: path
                    .file_stem()
                    .map(|s| s.to_string_lossy().to_string())
                    .unwrap_or_default(),
                ..Default::default()
            },
        };
        conv.mtime = mtime;
        read_more(&path, &mut conv);
        if let Ok(mut idx) = shared.lock() {
            idx.convs.insert(path, conv);
        }
    }
}

fn read_more(path: &Path, c: &mut Conv) {
    use std::io::BufRead;
    let Ok(mut f) = std::fs::File::open(path) else {
        return;
    };
    if f.seek(SeekFrom::Start(c.offset)).is_err() {
        return;
    }
    let mut r = std::io::BufReader::with_capacity(1 << 16, f);
    let mut raw: Vec<u8> = Vec::new();
    loop {
        raw.clear();
        let Ok(n) = r.read_until(b'\n', &mut raw) else {
            break;
        };
        if n == 0 || raw.last() != Some(&b'\n') {
            break;
        }
        c.offset += n as u64;
        if raw.len() <= MAX_LINE {
            take_line(&String::from_utf8_lossy(&raw), c);
        }
        if raw.capacity() > MAX_LINE * 2 {
            raw = Vec::new();
        }
    }
    c.texts.shrink_to_fit();
}

fn take_line(line: &str, c: &mut Conv) {
    if line.contains("Title\"") {
        if let Ok(v) = serde_json::from_str::<serde_json::Value>(line) {
            if let Some(t) = v.get("customTitle").and_then(|t| t.as_str()) {
                c.title = t.to_string();
                c.custom = true;
            } else if let Some(t) = v.get("aiTitle").and_then(|t| t.as_str()) {
                if !c.custom {
                    c.title = t.to_string();
                }
            }
        }
        return;
    }
    let user = line.contains("\"type\":\"user\"");
    if !user && !line.contains("\"type\":\"assistant\"") {
        return;
    }
    let Ok(v) = serde_json::from_str::<serde_json::Value>(line) else {
        return;
    };
    if v.get("isSidechain").and_then(|x| x.as_bool()) == Some(true) {
        return;
    }
    if c.cwd.is_none() {
        c.cwd = v.get("cwd").and_then(|x| x.as_str()).map(PathBuf::from);
    }
    let Some(content) = v.pointer("/message/content") else {
        return;
    };
    let text = match content {
        serde_json::Value::String(s) => s.clone(),
        serde_json::Value::Array(parts) => parts
            .iter()
            .filter(|p| p.get("type").and_then(|t| t.as_str()) == Some("text"))
            .filter_map(|p| p.get("text").and_then(|t| t.as_str()))
            .collect::<Vec<_>>()
            .join(" "),
        _ => return,
    };
    let flat: String = text.split_whitespace().collect::<Vec<_>>().join(" ");
    if flat.is_empty() || flat.starts_with('<') {
        return;
    }
    let short: String = flat.chars().take(MAX_TEXT).collect();
    if c.title.is_empty() && user {
        c.title = short.chars().take(60).collect();
    }
    c.texts.push(short);
}

pub struct Results {
    pub hits: Vec<Hit>,
    pub matches: usize,
    pub searched: usize,
    pub outside: usize,
}

pub fn search(
    idx: &Index,
    query: &str,
    scope: Option<&Path>,
    known: &dyn Fn(&str) -> bool,
) -> Results {
    let q = query.trim().to_lowercase();
    let mut hits = Vec::new();
    let (mut matches, mut searched, mut outside) = (0, 0, 0);
    for c in idx.convs.values() {
        if c.texts.is_empty() {
            continue;
        }
        if let Some(s) = scope {
            if !c.cwd.as_deref().is_some_and(|cwd| cwd.starts_with(s)) {
                continue;
            }
        }
        searched += 1;
        if !known(&c.id) {
            outside += 1;
        }
        if q.is_empty() {
            continue;
        }
        let mut first: Option<&String> = None;
        let mut n = 0;
        for t in &c.texts {
            let k = t.to_lowercase().matches(&q).count();
            if k > 0 {
                n += k;
                first.get_or_insert(t);
            }
        }
        if c.title.to_lowercase().contains(&q) {
            n += 1;
        }
        if n == 0 {
            continue;
        }
        matches += n;
        hits.push(Hit {
            id: c.id.clone(),
            cwd: c.cwd.clone(),
            title: if c.title.is_empty() {
                "untitled".into()
            } else {
                c.title.clone()
            },
            mtime: c.mtime,
            snippet: first.cloned().unwrap_or_else(|| c.title.clone()),
        });
    }
    hits.sort_by_key(|h| std::cmp::Reverse(h.mtime));
    Results {
        hits,
        matches,
        searched,
        outside,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn indexes_and_searches_incrementally() {
        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path().join("-code-api");
        std::fs::create_dir_all(&dir).unwrap();
        let f = dir.join("abc.jsonl");
        let user = |t: &str| {
            format!("{{\"type\":\"user\",\"cwd\":\"/code/api\",\"message\":{{\"role\":\"user\",\"content\":\"{t}\"}}}}\n")
        };
        std::fs::write(
            &f,
            user("move the rate limiter into redis")
                + "{\"type\":\"ai-title\",\"aiTitle\":\"Rate limiter rewrite\"}\n"
                + "{\"type\":\"assistant\",\"message\":{\"content\":[{\"type\":\"text\",\"text\":\"Redis it is.\"},{\"type\":\"tool_use\",\"name\":\"Bash\"}]}}\n",
        )
        .unwrap();
        let shared: Shared = Default::default();
        refresh(&shared, tmp.path());
        let idx = shared.lock().unwrap();
        let r = search(&idx, "REDIS", Some(Path::new("/code")), &|_| false);
        assert_eq!(r.hits.len(), 1);
        assert_eq!(r.matches, 2);
        assert_eq!(r.outside, 1);
        assert_eq!(r.hits[0].title, "Rate limiter rewrite");
        assert_eq!(r.hits[0].snippet, "move the rate limiter into redis");
        assert!(search(&idx, "redis", Some(Path::new("/other")), &|_| false)
            .hits
            .is_empty());
        drop(idx);
        let mut s = std::fs::read_to_string(&f).unwrap();
        s.push_str(&user("now add kubernetes"));
        std::fs::write(&f, s).unwrap();
        refresh(&shared, tmp.path());
        let idx = shared.lock().unwrap();
        assert_eq!(search(&idx, "kubernetes", None, &|_| true).hits.len(), 1);
    }
}
