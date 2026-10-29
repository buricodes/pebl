use anyhow::{bail, Context, Result};
use std::path::Path;
use std::process::Command;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Context,
    Add,
    Del,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Row {
    File {
        path: String,
        adds: usize,
        dels: usize,
    },
    Hunk(String),
    Line {
        kind: Kind,
        old: Option<usize>,
        new: Option<usize>,
        text: String,
    },
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Diff {
    pub base: String,
    pub rows: Vec<Row>,
}

impl Diff {
    pub fn files(&self) -> usize {
        self.rows
            .iter()
            .filter(|r| matches!(r, Row::File { .. }))
            .count()
    }

    pub fn totals(&self) -> (usize, usize) {
        self.rows.iter().fold((0, 0), |(a, d), r| match r {
            Row::File { adds, dels, .. } => (a + adds, d + dels),
            _ => (a, d),
        })
    }

    pub fn file_of(&self, i: usize) -> Option<&str> {
        self.rows[..=i.min(self.rows.len().saturating_sub(1))]
            .iter()
            .rev()
            .find_map(|r| match r {
                Row::File { path, .. } => Some(path.as_str()),
                _ => None,
            })
    }
}

fn git(dir: &Path, args: &[&str]) -> Result<String> {
    let out = Command::new("git")
        .arg("-C")
        .arg(dir)
        .args(args)
        .output()
        .context("running git")?;
    if !out.status.success() {
        bail!(
            "git {}: {}",
            args.first().copied().unwrap_or(""),
            String::from_utf8_lossy(&out.stderr).trim()
        );
    }
    Ok(String::from_utf8_lossy(&out.stdout).into_owned())
}

pub fn base(dir: &Path) -> Result<(String, String)> {
    let branch = git(dir, &["rev-parse", "--abbrev-ref", "HEAD"])?
        .trim()
        .to_string();
    let default = git(
        dir,
        &["symbolic-ref", "--short", "refs/remotes/origin/HEAD"],
    )
    .ok()
    .map(|s| s.trim().to_string())
    .or_else(|| {
        ["main", "master"]
            .iter()
            .find(|b| git(dir, &["rev-parse", "--verify", "--quiet", b]).is_ok())
            .map(|b| b.to_string())
    });
    if let Some(d) = default {
        let short = d.trim_start_matches("origin/");
        if short != branch && d != branch {
            if let Ok(mb) = git(dir, &["merge-base", "HEAD", &d]) {
                return Ok((mb.trim().to_string(), d));
            }
        }
    }
    if git(dir, &["rev-parse", "--verify", "--quiet", "HEAD"]).is_ok() {
        Ok(("HEAD".into(), "uncommitted".into()))
    } else {
        Ok((
            "4b825dc642cb6eb9a060e54bf8d69288fbee4904".into(),
            "empty".into(),
        ))
    }
}

pub fn load(dir: &Path) -> Result<Diff> {
    if !crate::git::is_repo(dir) {
        bail!("{} is not a git repository", dir.display());
    }
    let (rev, label) = base(dir)?;
    let text = git(
        dir,
        &[
            "diff",
            "--no-color",
            "--no-ext-diff",
            "-U3",
            &rev,
            "--",
            ".",
        ],
    )?;
    let mut d = parse(&text);
    let untracked = git(dir, &["ls-files", "--others", "--exclude-standard"])?;
    let root = git(dir, &["rev-parse", "--show-toplevel"])?;
    let root = Path::new(root.trim());
    for path in untracked.lines().filter(|l| !l.is_empty()) {
        let full = dir.join(path);
        let full = if full.exists() { full } else { root.join(path) };
        let Ok(meta) = full.metadata() else { continue };
        if meta.len() > 256 * 1024 {
            continue;
        }
        let Ok(body) = std::fs::read_to_string(&full) else {
            continue;
        };
        let lines: Vec<&str> = body.lines().collect();
        d.rows.push(Row::File {
            path: path.to_string(),
            adds: lines.len(),
            dels: 0,
        });
        d.rows.push(Row::Hunk(match lines.len() {
            1 => "new file, 1 line".to_string(),
            n => format!("new file, {n} lines"),
        }));
        for (i, l) in lines.iter().enumerate() {
            d.rows.push(Row::Line {
                kind: Kind::Add,
                old: None,
                new: Some(i + 1),
                text: l.to_string(),
            });
        }
    }
    d.base = label;
    Ok(d)
}

pub fn parse(text: &str) -> Diff {
    let mut d = Diff::default();
    let mut file: Option<usize> = None;
    let (mut old, mut new) = (0usize, 0usize);
    let mut in_hunk = false;
    for line in text.lines() {
        if let Some(rest) = line.strip_prefix("diff --git ") {
            let path = rest
                .rsplit_once(" b/")
                .map(|(_, b)| b.to_string())
                .unwrap_or_else(|| rest.to_string());
            d.rows.push(Row::File {
                path,
                adds: 0,
                dels: 0,
            });
            file = Some(d.rows.len() - 1);
            in_hunk = false;
            continue;
        }
        if let Some(h) = line.strip_prefix("@@ ") {
            let mut nums = h.split_whitespace();
            let o = nums.next().unwrap_or("-0");
            let n = nums.next().unwrap_or("+0");
            let start = |s: &str| {
                s[1..]
                    .split(',')
                    .next()
                    .and_then(|x| x.parse().ok())
                    .unwrap_or(0)
            };
            old = start(o);
            new = start(n);
            let ctx = h.split_once(" @@").map(|(_, c)| c.trim()).unwrap_or("");
            d.rows.push(Row::Hunk(ctx.to_string()));
            in_hunk = true;
            continue;
        }
        if !in_hunk {
            if line.starts_with("Binary files") {
                d.rows.push(Row::Hunk("binary file".into()));
            }
            continue;
        }
        let (kind, body) = match line.chars().next() {
            Some('+') => (Kind::Add, &line[1..]),
            Some('-') => (Kind::Del, &line[1..]),
            Some(' ') => (Kind::Context, &line[1..]),
            Some('\\') => continue,
            None => (Kind::Context, ""),
            _ => continue,
        };
        let (o, n) = match kind {
            Kind::Add => {
                new += 1;
                (None, Some(new - 1))
            }
            Kind::Del => {
                old += 1;
                (Some(old - 1), None)
            }
            Kind::Context => {
                old += 1;
                new += 1;
                (Some(old - 1), Some(new - 1))
            }
        };
        if let Some(Row::File { adds, dels, .. }) = file.and_then(|i| d.rows.get_mut(i)) {
            match kind {
                Kind::Add => *adds += 1,
                Kind::Del => *dels += 1,
                Kind::Context => {}
            }
        }
        d.rows.push(Row::Line {
            kind,
            old: o,
            new: n,
            text: body.replace('\t', "    "),
        });
    }
    d
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_unified_diff() {
        let d = parse(
            "diff --git a/src/a.rs b/src/a.rs\nindex 1..2 100644\n--- a/src/a.rs\n+++ b/src/a.rs\n@@ -10,3 +10,3 @@ fn main() {\n ctx\n-old\n+new\n ctx2\n",
        );
        assert_eq!(d.files(), 1);
        assert_eq!(d.totals(), (1, 1));
        assert_eq!(d.rows[1], Row::Hunk("fn main() {".into()));
        assert_eq!(
            d.rows[4],
            Row::Line {
                kind: Kind::Add,
                old: None,
                new: Some(11),
                text: "new".into()
            }
        );
        assert_eq!(d.file_of(5), Some("src/a.rs"));
    }
}
