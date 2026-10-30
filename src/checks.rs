use std::collections::BTreeMap;
use std::hash::{Hash, Hasher};
use std::io::Read;
use std::path::Path;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

const TIMEOUT: Duration = Duration::from_secs(10 * 60);
const TAIL_LINES: usize = 60;
const TAIL_CHARS: usize = 6000;

#[derive(Clone, Debug, Default, PartialEq, serde::Deserialize)]
#[serde(default)]
pub struct Checks {
    pub auto_fix: u32,
    #[serde(flatten)]
    pub commands: BTreeMap<String, String>,
}

impl Checks {
    pub fn is_empty(&self) -> bool {
        self.commands.is_empty()
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Outcome {
    pub name: String,
    pub command: String,
    pub code: Option<i32>,
    pub tail: String,
}

impl Outcome {
    pub fn passed(&self) -> bool {
        self.code == Some(0)
    }
}

pub fn load(dirs: &[&Path]) -> Checks {
    #[derive(serde::Deserialize, Default)]
    struct File {
        #[serde(default)]
        checks: Checks,
    }
    dirs.iter()
        .filter_map(|d| std::fs::read_to_string(d.join("pebl.toml")).ok())
        .find_map(|s| toml::from_str::<File>(&s).ok())
        .map(|f| f.checks)
        .unwrap_or_default()
}

pub fn fingerprint(cwd: &Path) -> Option<u64> {
    let git = |args: &[&str]| {
        Command::new("git")
            .args(args)
            .current_dir(cwd)
            .stdin(Stdio::null())
            .stderr(Stdio::null())
            .output()
            .ok()
            .filter(|o| o.status.success())
            .map(|o| o.stdout)
    };
    let status = git(&["status", "--porcelain=v1", "-uall"])?;
    let diff = git(&["diff", "HEAD"]).unwrap_or_default();
    let mut h = std::collections::hash_map::DefaultHasher::new();
    status.hash(&mut h);
    diff.hash(&mut h);
    for line in String::from_utf8_lossy(&status).lines() {
        if let Some(path) = line.strip_prefix("?? ") {
            if let Ok(meta) = std::fs::metadata(cwd.join(path)) {
                meta.len().hash(&mut h);
                meta.modified().ok().hash(&mut h);
            }
        }
    }
    Some(h.finish())
}

pub fn run(cwd: &Path, checks: &Checks) -> Vec<Outcome> {
    checks
        .commands
        .iter()
        .map(|(name, command)| run_one(cwd, name, command))
        .collect()
}

fn run_one(cwd: &Path, name: &str, command: &str) -> Outcome {
    let outcome = |code, tail| Outcome {
        name: name.to_string(),
        command: command.to_string(),
        code,
        tail,
    };
    let child = Command::new("sh")
        .args(["-c", &format!("exec 2>&1\n{command}")])
        .current_dir(cwd)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn();
    let mut child = match child {
        Ok(c) => c,
        Err(e) => return outcome(None, format!("couldn't start: {e}")),
    };
    let mut out = child.stdout.take();
    let reader = std::thread::spawn(move || {
        let mut buf = Vec::new();
        if let Some(o) = out.as_mut() {
            let _ = o.read_to_end(&mut buf);
        }
        buf
    });
    let started = Instant::now();
    let code = loop {
        match child.try_wait() {
            Ok(Some(status)) => break status.code(),
            Ok(None) if started.elapsed() > TIMEOUT => {
                let _ = child.kill();
                let _ = child.wait();
                break None;
            }
            Ok(None) => std::thread::sleep(Duration::from_millis(100)),
            Err(_) => break None,
        }
    };
    let text = String::from_utf8_lossy(&reader.join().unwrap_or_default()).into_owned();
    let mut tail = tail(&text);
    if code.is_none() && started.elapsed() > TIMEOUT {
        tail.push_str("\n(stopped after 10 minutes)");
    }
    outcome(code, tail)
}

fn tail(text: &str) -> String {
    let lines: Vec<&str> = text.trim_end().lines().collect();
    let kept = lines[lines.len().saturating_sub(TAIL_LINES)..].join("\n");
    let n = kept.chars().count();
    if n <= TAIL_CHARS {
        return kept;
    }
    kept.chars().skip(n - TAIL_CHARS).collect()
}

pub fn fix_prompt(outcomes: &[Outcome]) -> String {
    let mut out = String::from(
        "The project's checks failed after your last change. Fix them, run them again, then summarise what you changed.\n",
    );
    for o in outcomes.iter().filter(|o| !o.passed()) {
        let code = o
            .code
            .map(|c| format!("exit {c}"))
            .unwrap_or_else(|| "did not finish".into());
        out.push_str(&format!(
            "\n{}: `{}` ({code})\n```\n{}\n```\n",
            o.name, o.command, o.tail
        ));
    }
    out
}

pub fn summary(outcomes: &[Outcome]) -> String {
    let failed: Vec<&str> = outcomes
        .iter()
        .filter(|o| !o.passed())
        .map(|o| o.name.as_str())
        .collect();
    match failed.as_slice() {
        [] => format!(
            "{} passed",
            outcomes
                .iter()
                .map(|o| o.name.as_str())
                .collect::<Vec<_>>()
                .join(", ")
        ),
        names => format!("{} failed", names.join(", ")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_checks_from_the_first_pebl_toml() {
        let tmp = tempfile::tempdir().unwrap();
        let (wt, repo) = (tmp.path().join("wt"), tmp.path().join("repo"));
        std::fs::create_dir_all(&wt).unwrap();
        std::fs::create_dir_all(&repo).unwrap();
        std::fs::write(
            repo.join("pebl.toml"),
            "[worktree]\ncopy = [\".env\"]\n\n[checks]\ntest = \"npm test\"\nlint = \"npm run lint\"\nauto_fix = 2\n",
        )
        .unwrap();
        let c = load(&[&wt, &repo]);
        assert_eq!(c.auto_fix, 2);
        assert_eq!(c.commands.keys().collect::<Vec<_>>(), vec!["lint", "test"]);
        assert!(load(&[&wt]).is_empty());
    }

    #[test]
    fn runs_commands_and_builds_the_fix_prompt() {
        let tmp = tempfile::tempdir().unwrap();
        let mut c = Checks::default();
        c.commands.insert("lint".into(), "echo all good".into());
        c.commands.insert(
            "test".into(),
            "seq 1 100; echo '1 failing' >&2; exit 3".into(),
        );
        let out = run(tmp.path(), &c);
        assert!(out[0].passed());
        assert_eq!(out[1].code, Some(3));
        assert!(out[1].tail.ends_with("100\n1 failing"));
        assert!(!out[1].tail.contains("\n40\n"));
        assert_eq!(summary(&out), "test failed");
        let p = fix_prompt(&out);
        assert!(p.contains("test: `seq 1 100; echo '1 failing' >&2; exit 3` (exit 3)"));
        assert!(!p.contains("lint"));
        assert_eq!(summary(&out[..1]), "lint passed");
    }

    #[test]
    fn fingerprint_changes_with_the_working_tree() {
        let tmp = tempfile::tempdir().unwrap();
        let d = tmp.path();
        let git = |args: &[&str]| {
            Command::new("git")
                .args(args)
                .current_dir(d)
                .output()
                .unwrap()
        };
        git(&["init", "-q"]);
        git(&[
            "-c",
            "user.email=t@t",
            "-c",
            "user.name=t",
            "commit",
            "-q",
            "--allow-empty",
            "-m",
            "x",
        ]);
        let a = fingerprint(d).unwrap();
        assert_eq!(fingerprint(d), Some(a));
        std::fs::write(d.join("new.txt"), "hi").unwrap();
        let b = fingerprint(d).unwrap();
        assert_ne!(a, b);
        assert!(fingerprint(&d.join("missing")).is_none());
    }
}
