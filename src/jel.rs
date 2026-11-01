use anyhow::{bail, Context, Result};
use std::io::Read;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

const GUIDE: &str = include_str!("../docs/JEL.md");

const ACTIONS: [(&str, &str); 9] = [
    ("shell", "opens a shell"),
    ("shell-new", "adds a shell"),
    ("split", "opens a column"),
    ("search", "opens search"),
    ("prompts", "opens saved prompts"),
    ("inbox", "opens the inbox"),
    ("waiting", "jumps there"),
    ("new-session", "starts a chat"),
    ("mark", "starts marking"),
];

#[derive(Clone, Debug, PartialEq)]
pub enum Act {
    Go(String),
    Do(String),
}

impl Act {
    pub fn label(&self) -> String {
        match self {
            Act::Go(_) => "jumps there".into(),
            Act::Do(a) => ACTIONS
                .iter()
                .find(|(k, _)| k == a)
                .map(|(_, l)| l.to_string())
                .unwrap_or_default(),
        }
    }
}

pub fn system_prompt(help: &str, sessions: &str) -> String {
    let actions: Vec<&str> = ACTIONS.iter().map(|(k, _)| *k).collect();
    format!(
        "You are Jel, the jellyfish inside pebl, a terminal app that runs several Claude Code sessions side by side in tmux. \
Answer the user's question about pebl or their sessions in ONE short line, under 90 characters, plain text, no markdown. \
Answer from the pebl guide, the key list and their sessions below. Name the exact key, click or command when there is one. \
If the guide doesn't cover it, say you don't know rather than guessing.\n\
If the answer is something pebl can do right now, end the line with [do:ACTION], ACTION being one of: {}.\n\
If the answer points at one of their sessions, end the line with [go:SESSION NAME].\n\n\
{GUIDE}\n\npebl keys:\n{help}\n\nTheir sessions right now:\n{sessions}",
        actions.join(", ")
    )
}

pub fn parse(answer: &str) -> (String, Option<Act>) {
    let line = answer
        .lines()
        .map(str::trim)
        .find(|l| !l.is_empty())
        .unwrap_or("")
        .to_string();
    let Some(open) = line.rfind('[') else {
        return (line, None);
    };
    let tag = line[open..].trim_start_matches('[').trim_end_matches(']');
    let act = match tag.split_once(':') {
        Some(("go", name)) if !name.trim().is_empty() => Some(Act::Go(name.trim().to_string())),
        Some(("do", a)) if ACTIONS.iter().any(|(k, _)| *k == a.trim()) => {
            Some(Act::Do(a.trim().to_string()))
        }
        _ => None,
    };
    match act {
        Some(a) => (line[..open].trim_end().to_string(), Some(a)),
        None => (line, None),
    }
}

pub fn ask_claude(claude: &str, system: &str, question: &str) -> Result<String> {
    let mut child = Command::new(claude)
        .args([
            "-p",
            "--model",
            "haiku",
            "--tools",
            "",
            "--no-session-persistence",
            "--system-prompt",
            system,
            question,
        ])
        .current_dir(std::env::temp_dir())
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .context("starting claude")?;
    let started = Instant::now();
    loop {
        if let Some(status) = child.try_wait()? {
            let mut out = String::new();
            if let Some(mut o) = child.stdout.take() {
                o.read_to_string(&mut out)?;
            }
            if !status.success() || out.trim().is_empty() {
                let mut err = String::new();
                if let Some(mut e) = child.stderr.take() {
                    let _ = e.read_to_string(&mut err);
                }
                bail!("{}", err.lines().next().unwrap_or("claude gave no answer"));
            }
            return Ok(out);
        }
        if started.elapsed() > Duration::from_secs(60) {
            let _ = child.kill();
            bail!("claude took too long");
        }
        std::thread::sleep(Duration::from_millis(100));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_the_answer_and_its_action() {
        assert_eq!(
            parse("Press ctrl+t, then click + split. [do:shell]\n"),
            (
                "Press ctrl+t, then click + split.".into(),
                Some(Act::Do("shell".into()))
            )
        );
        assert_eq!(
            parse("\nonboarding-flow is waiting on you [go:onboarding-flow]"),
            (
                "onboarding-flow is waiting on you".into(),
                Some(Act::Go("onboarding-flow".into()))
            )
        );
        assert_eq!(parse("try [do:rm -rf]"), ("try [do:rm -rf]".into(), None));
        assert_eq!(parse("plain"), ("plain".into(), None));
        assert_eq!(Act::Do("split".into()).label(), "opens a column");
        let p = system_prompt("k", "s");
        assert!(p.contains("shell-new") && p.contains("ntfy.sh") && p.contains("+ split"));
    }
}
