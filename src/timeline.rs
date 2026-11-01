use anyhow::{bail, Context, Result};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::Command;

#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(tag = "kind", rename_all = "lowercase")]
pub enum Event {
    Prompt {
        at: u64,
        text: String,
    },
    Tool {
        at: u64,
        name: String,
        detail: String,
    },
    Turn {
        at: u64,
        checkpoint: Option<String>,
        files: Vec<String>,
        adds: usize,
        dels: usize,
    },
    Checks {
        at: u64,
        passed: bool,
        summary: String,
    },
    Restore {
        at: u64,
        to: String,
        saved: String,
    },
}

pub fn file(dir: &Path, sid: &str) -> PathBuf {
    dir.join(format!("{sid}.jsonl"))
}

pub fn append(dir: &Path, sid: &str, event: &Event) -> Result<()> {
    std::fs::create_dir_all(dir)?;
    let mut f = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(file(dir, sid))?;
    writeln!(f, "{}", serde_json::to_string(event)?)?;
    Ok(())
}

pub fn read(dir: &Path, sid: &str) -> Vec<Event> {
    std::fs::read_to_string(file(dir, sid))
        .unwrap_or_default()
        .lines()
        .filter_map(|l| serde_json::from_str(l).ok())
        .collect()
}

pub fn tool_detail(input: &serde_json::Value) -> String {
    let text = [
        "file_path",
        "command",
        "pattern",
        "url",
        "path",
        "description",
    ]
    .iter()
    .find_map(|k| input.get(k).and_then(|v| v.as_str()))
    .unwrap_or("");
    let line = text.lines().next().unwrap_or("").trim();
    line.chars().take(120).collect()
}

fn git(cwd: &Path, index: Option<&Path>, args: &[&str]) -> Result<String> {
    let mut cmd = Command::new("git");
    cmd.arg("-C").arg(cwd).args(args);
    if let Some(i) = index {
        cmd.env("GIT_INDEX_FILE", i);
    }
    let out = cmd.output().context("running git")?;
    if !out.status.success() {
        bail!(
            "git {}: {}",
            args.first().unwrap_or(&""),
            String::from_utf8_lossy(&out.stderr).trim()
        );
    }
    Ok(String::from_utf8_lossy(&out.stdout).trim().to_string())
}

fn snapshot_tree(cwd: &Path) -> Result<String> {
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let index = std::env::temp_dir().join(format!("pebl-index-{}-{stamp}", std::process::id()));
    let tree = (|| {
        if git(cwd, None, &["rev-parse", "--verify", "--quiet", "HEAD"]).is_ok() {
            git(cwd, Some(&index), &["read-tree", "HEAD"])?;
        }
        git(cwd, Some(&index), &["add", "-A", "--", "."])?;
        git(cwd, Some(&index), &["write-tree"])
    })();
    let _ = std::fs::remove_file(&index);
    tree
}

pub fn tree_of(cwd: &Path, commit: &str) -> Result<String> {
    git(cwd, None, &["rev-parse", &format!("{commit}^{{tree}}")])
}

pub struct Snapshot {
    pub commit: String,
    pub files: Vec<String>,
    pub adds: usize,
    pub dels: usize,
}

pub fn checkpoint(
    cwd: &Path,
    sid: &str,
    last: Option<&str>,
    label: &str,
) -> Result<Option<Snapshot>> {
    let tree = snapshot_tree(cwd)?;
    let base = match last {
        Some(c) => tree_of(cwd, c).ok(),
        None => git(cwd, None, &["rev-parse", "HEAD^{tree}"]).ok(),
    };
    if base.as_deref() == Some(tree.as_str()) {
        return Ok(None);
    }
    let mut args = vec!["commit-tree", tree.as_str(), "-m", label];
    let parent = last
        .map(str::to_string)
        .or_else(|| git(cwd, None, &["rev-parse", "HEAD"]).ok());
    if let Some(p) = parent.as_deref() {
        args.extend(["-p", p]);
    }
    let commit = git(cwd, None, &args)?;
    let n = git(
        cwd,
        None,
        &["for-each-ref", "--format=x", &format!("refs/pebl/{sid}/")],
    )?
    .lines()
    .count();
    git(
        cwd,
        None,
        &["update-ref", &format!("refs/pebl/{sid}/{n}"), &commit],
    )?;
    let (files, adds, dels) = match base {
        Some(b) => changes(cwd, &b, &tree)?,
        None => (Vec::new(), 0, 0),
    };
    Ok(Some(Snapshot {
        commit,
        files,
        adds,
        dels,
    }))
}

fn changes(cwd: &Path, from: &str, to: &str) -> Result<(Vec<String>, usize, usize)> {
    let out = git(cwd, None, &["diff", "--numstat", from, to])?;
    let mut files = Vec::new();
    let (mut adds, mut dels) = (0, 0);
    for line in out.lines() {
        let mut parts = line.split('\t');
        adds += parts
            .next()
            .and_then(|n| n.parse::<usize>().ok())
            .unwrap_or(0);
        dels += parts
            .next()
            .and_then(|n| n.parse::<usize>().ok())
            .unwrap_or(0);
        if let Some(f) = parts.next() {
            files.push(f.to_string());
        }
    }
    Ok((files, adds, dels))
}

pub fn restore(cwd: &Path, sid: &str, to: &str, last: Option<&str>) -> Result<String> {
    let target = tree_of(cwd, to)?;
    let saved = match checkpoint(cwd, sid, last, "pebl: before restoring")? {
        Some(s) => s.commit,
        None => last.map(str::to_string).unwrap_or_else(|| to.to_string()),
    };
    let now = snapshot_tree(cwd)?;
    let added = git(
        cwd,
        None,
        &["diff", "--name-only", "--diff-filter=A", &target, &now],
    )?;
    for f in added.lines().filter(|l| !l.is_empty()) {
        let path = cwd.join(f);
        if path.starts_with(cwd) {
            let _ = std::fs::remove_file(path);
        }
    }
    git(
        cwd,
        None,
        &[
            "restore",
            &format!("--source={to}"),
            "--worktree",
            "--",
            ".",
        ],
    )?;
    Ok(saved)
}

pub fn forget(cwd: &Path, sid: &str) {
    if let Ok(refs) = git(
        cwd,
        None,
        &[
            "for-each-ref",
            "--format=%(refname)",
            &format!("refs/pebl/{sid}/"),
        ],
    ) {
        for r in refs.lines() {
            let _ = git(cwd, None, &["update-ref", "-d", r]);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn repo() -> tempfile::TempDir {
        let tmp = tempfile::tempdir().unwrap();
        let d = tmp.path();
        for args in [
            vec!["init", "-q", "-b", "main"],
            vec![
                "-c",
                "user.email=t@t",
                "-c",
                "user.name=t",
                "commit",
                "-q",
                "--allow-empty",
                "-m",
                "start",
            ],
        ] {
            Command::new("git")
                .args(&args)
                .current_dir(d)
                .output()
                .unwrap();
        }
        std::fs::write(d.join("a.txt"), "one\n").unwrap();
        Command::new("git")
            .args(["add", "."])
            .current_dir(d)
            .output()
            .unwrap();
        Command::new("git")
            .args([
                "-c",
                "user.email=t@t",
                "-c",
                "user.name=t",
                "commit",
                "-qm",
                "a",
            ])
            .current_dir(d)
            .output()
            .unwrap();
        tmp
    }

    #[test]
    fn checkpoints_and_restores_without_touching_head_or_index() {
        let tmp = repo();
        let d = tmp.path();
        let head = git(d, None, &["rev-parse", "HEAD"]).unwrap();
        assert!(checkpoint(d, "s1", None, "turn 1").unwrap().is_none());

        std::fs::write(d.join("a.txt"), "two\n").unwrap();
        std::fs::write(d.join("b.txt"), "new\n").unwrap();
        let one = checkpoint(d, "s1", None, "turn 1").unwrap().unwrap();
        assert_eq!(one.files, vec!["a.txt", "b.txt"]);
        assert_eq!((one.adds, one.dels), (2, 1));
        assert!(checkpoint(d, "s1", Some(&one.commit), "same")
            .unwrap()
            .is_none());

        std::fs::write(d.join("a.txt"), "three\n").unwrap();
        std::fs::remove_file(d.join("b.txt")).unwrap();
        std::fs::write(d.join("c.txt"), "later\n").unwrap();
        let two = checkpoint(d, "s1", Some(&one.commit), "turn 2")
            .unwrap()
            .unwrap();
        assert_eq!(two.files, vec!["a.txt", "b.txt", "c.txt"]);

        let saved = restore(d, "s1", &one.commit, Some(&two.commit)).unwrap();
        assert_eq!(saved, two.commit);
        assert_eq!(std::fs::read_to_string(d.join("a.txt")).unwrap(), "two\n");
        assert_eq!(std::fs::read_to_string(d.join("b.txt")).unwrap(), "new\n");
        assert!(!d.join("c.txt").exists());
        assert_eq!(git(d, None, &["rev-parse", "HEAD"]).unwrap(), head);
        assert_eq!(
            git(d, None, &["diff", "--cached", "--name-only"]).unwrap(),
            ""
        );

        restore(d, "s1", &two.commit, Some(&two.commit)).unwrap();
        assert!(d.join("c.txt").exists());
        assert!(!d.join("b.txt").exists());

        let refs = git(
            d,
            None,
            &["for-each-ref", "--format=%(refname)", "refs/pebl/s1/"],
        )
        .unwrap();
        assert_eq!(refs.lines().count(), 3);
        forget(d, "s1");
        assert_eq!(git(d, None, &["for-each-ref", "refs/pebl/"]).unwrap(), "");
    }

    #[test]
    fn records_and_reads_events() {
        let tmp = tempfile::tempdir().unwrap();
        append(
            tmp.path(),
            "s1",
            &Event::Prompt {
                at: 1,
                text: "fix it".into(),
            },
        )
        .unwrap();
        append(
            tmp.path(),
            "s1",
            &Event::Tool {
                at: 2,
                name: "Edit".into(),
                detail: tool_detail(
                    &serde_json::json!({"file_path": "src/a.ts", "old_string": "x"}),
                ),
            },
        )
        .unwrap();
        let events = read(tmp.path(), "s1");
        assert_eq!(events.len(), 2);
        assert!(matches!(&events[1], Event::Tool { detail, .. } if detail == "src/a.ts"));
        assert_eq!(
            tool_detail(&serde_json::json!({"command": "npm test\nnpm run lint"})),
            "npm test"
        );
        assert!(read(tmp.path(), "none").is_empty());
    }
}
