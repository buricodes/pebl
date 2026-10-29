use anyhow::{bail, Context, Result};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::SystemTime;

pub fn describe(dir: &Path) -> String {
    let git = dir.join(".git");
    let age = |p: &Path| {
        p.metadata()
            .and_then(|m| m.modified())
            .ok()
            .and_then(|t| SystemTime::now().duration_since(t).ok())
            .map(|d| format!("{} ago", crate::model::age(d.as_secs())))
            .unwrap_or_default()
    };
    if git.is_dir() {
        let branch = std::fs::read_to_string(git.join("HEAD"))
            .ok()
            .and_then(|h| {
                h.trim()
                    .strip_prefix("ref: refs/heads/")
                    .map(str::to_string)
            })
            .unwrap_or_else(|| "detached".into());
        let logs = git.join("logs/HEAD");
        let when = if logs.exists() { age(&logs) } else { age(&git) };
        format!("git · {branch} · {when}")
    } else if git.is_file() {
        format!("worktree · {}", age(dir))
    } else {
        format!("folder · {}", age(dir))
    }
}

pub fn is_repo(dir: &Path) -> bool {
    Command::new("git")
        .arg("-C")
        .arg(dir)
        .args(["rev-parse", "--is-inside-work-tree"])
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

pub fn add_worktree(repo: &Path, root: &Path, project: &str, name: &str) -> Result<PathBuf> {
    if !is_repo(repo) {
        bail!("{} is not a git repository", repo.display());
    }
    std::fs::create_dir_all(root)?;
    let dir = root.join(format!("{project}-{name}"));
    let out = Command::new("git")
        .arg("-C")
        .arg(repo)
        .args(["worktree", "add", "-b", &format!("pebl/{name}")])
        .arg(&dir)
        .output()
        .context("running git")?;
    if !out.status.success() {
        bail!(
            "git worktree: {}",
            String::from_utf8_lossy(&out.stderr).trim()
        );
    }
    Ok(dir)
}

fn run(dir: &Path, args: &[&str]) -> Result<String> {
    let out = Command::new("git")
        .arg("-C")
        .arg(dir)
        .args(args)
        .output()
        .context("running git")?;
    if !out.status.success() {
        let err = String::from_utf8_lossy(&out.stderr);
        let msg = err
            .lines()
            .find(|l| !l.trim().is_empty())
            .unwrap_or("failed")
            .trim()
            .trim_start_matches("error: ")
            .trim_start_matches("fatal: ")
            .to_string();
        bail!("{msg}");
    }
    Ok(String::from_utf8_lossy(&out.stdout).trim().to_string())
}

#[derive(Clone, Debug, Default, PartialEq, serde::Deserialize)]
#[serde(default)]
pub struct WorktreeSetup {
    pub copy: Vec<String>,
    pub setup: Vec<String>,
}

pub fn setup_for(repo: &Path) -> WorktreeSetup {
    #[derive(serde::Deserialize, Default)]
    struct File {
        #[serde(default)]
        worktree: WorktreeSetup,
    }
    std::fs::read_to_string(repo.join("pebl.toml"))
        .ok()
        .and_then(|s| toml::from_str::<File>(&s).ok())
        .map(|f| f.worktree)
        .unwrap_or_default()
}

pub fn copy_into(repo: &Path, wt: &Path, items: &[String]) -> Vec<String> {
    let mut copied = Vec::new();
    for item in items {
        let from = repo.join(item);
        let to = wt.join(item);
        if copy_path(&from, &to).is_ok() {
            copied.push(item.clone());
        }
    }
    copied
}

fn copy_path(from: &Path, to: &Path) -> std::io::Result<()> {
    if from.is_dir() {
        std::fs::create_dir_all(to)?;
        for e in std::fs::read_dir(from)? {
            let e = e?;
            copy_path(&e.path(), &to.join(e.file_name()))?;
        }
        Ok(())
    } else {
        if let Some(parent) = to.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::copy(from, to).map(|_| ())
    }
}

pub fn branch(dir: &Path) -> Result<String> {
    run(dir, &["rev-parse", "--abbrev-ref", "HEAD"])
}

pub fn dirty(dir: &Path) -> usize {
    run(dir, &["status", "--porcelain"])
        .map(|s| s.lines().filter(|l| !l.is_empty()).count())
        .unwrap_or(0)
}

pub fn ahead(dir: &Path, base: &str, branch: &str) -> usize {
    run(dir, &["rev-list", "--count", &format!("{base}..{branch}")])
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(0)
}

pub fn merge(repo: &Path, branch: &str) -> Result<()> {
    let n = dirty(repo);
    if n > 0 {
        bail!(
            "{} has {n} uncommitted changes; commit or stash them first",
            crate::model::tilde(repo)
        );
    }
    if let Err(e) = run(repo, &["merge", "--no-edit", branch]) {
        let _ = run(repo, &["merge", "--abort"]);
        bail!("merge stopped ({e}); ask the session to rebase onto the main branch");
    }
    Ok(())
}

pub fn remove_worktree(repo: &Path, wt: &Path, branch: &str, force: bool) -> Result<()> {
    let mut args = vec!["worktree", "remove"];
    if force {
        args.push("--force");
    }
    let wt = wt.display().to_string();
    args.push(&wt);
    run(repo, &args)?;
    if branch.starts_with("pebl/") {
        run(repo, &["branch", if force { "-D" } else { "-d" }, branch])?;
    }
    Ok(())
}

pub fn push_and_pr(wt: &Path, branch: &str) -> Result<String> {
    run(wt, &["push", "-u", "origin", branch])?;
    let out = Command::new("gh")
        .current_dir(wt)
        .args(["pr", "create", "--fill", "--head", branch])
        .output()
        .context("running gh (brew install gh)")?;
    let text = String::from_utf8_lossy(&out.stdout).trim().to_string();
    if !out.status.success() {
        let err = String::from_utf8_lossy(&out.stderr);
        if let Some(url) = err.split_whitespace().find(|w| w.starts_with("https://")) {
            return Ok(url.to_string());
        }
        bail!(
            "gh: {}",
            err.lines()
                .next()
                .unwrap_or("failed to open the pull request")
        );
    }
    Ok(text
        .split_whitespace()
        .find(|w| w.starts_with("https://"))
        .unwrap_or(&text)
        .to_string())
}

pub fn has_gh() -> bool {
    Command::new("gh")
        .arg("--version")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sh(dir: &Path, args: &[&str]) {
        let ok = Command::new("git")
            .arg("-C")
            .arg(dir)
            .args(["-c", "user.email=t@t", "-c", "user.name=t"])
            .args(args)
            .output()
            .unwrap();
        assert!(
            ok.status.success(),
            "{}",
            String::from_utf8_lossy(&ok.stderr)
        );
    }

    #[test]
    fn worktree_setup_merge_and_cleanup() {
        let tmp = tempfile::tempdir().unwrap();
        let repo = tmp.path().join("api");
        std::fs::create_dir_all(&repo).unwrap();
        sh(&repo, &["init", "-q", "-b", "main"]);
        std::fs::write(repo.join("a.txt"), "one\n").unwrap();
        std::fs::write(repo.join(".env"), "SECRET=1\n").unwrap();
        std::fs::write(
            repo.join("pebl.toml"),
            "[worktree]\ncopy = [\".env\", \"missing\"]\nsetup = [\"true\"]\n",
        )
        .unwrap();
        std::fs::write(repo.join(".gitignore"), ".env\n").unwrap();
        sh(&repo, &["add", "."]);
        sh(&repo, &["commit", "-qm", "init"]);

        let wt = add_worktree(&repo, &tmp.path().join("wts"), "api", "fix").unwrap();
        let cfg = setup_for(&repo);
        assert_eq!(cfg.setup, vec!["true"]);
        assert_eq!(copy_into(&repo, &wt, &cfg.copy), vec![".env"]);
        assert!(wt.join(".env").exists());

        std::fs::write(wt.join("a.txt"), "two\n").unwrap();
        assert_eq!(dirty(&wt), 1);
        sh(&wt, &["commit", "-qam", "change"]);
        assert_eq!(dirty(&wt), 0);
        assert_eq!(branch(&wt).unwrap(), "pebl/fix");
        assert_eq!(ahead(&repo, "main", "pebl/fix"), 1);

        merge(&repo, "pebl/fix").unwrap();
        assert_eq!(
            std::fs::read_to_string(repo.join("a.txt")).unwrap(),
            "two\n"
        );
        remove_worktree(&repo, &wt, "pebl/fix", false).unwrap();
        assert!(!wt.exists());
        assert!(run(&repo, &["rev-parse", "--verify", "--quiet", "pebl/fix"]).is_err());
    }
}
