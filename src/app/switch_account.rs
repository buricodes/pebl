use super::App;
use crate::backend::{SessionBackend, UiBackend};
use crate::profile::DEFAULT;
use anyhow::{anyhow, Result};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

const SETTLE: Duration = Duration::from_millis(400);

pub const COMMAND: &str = "---
description: Switch this session to another Claude account (pebl)
argument-hint: [account name, email or number]
---
pebl switches this session's Claude account before this message reaches you. If you can read it, the switch did not run: reply only with \"pebl couldn't switch accounts. Try /switch again.\"
";

pub fn write_plugin(dir: &Path) -> Result<()> {
    let manifest = dir.join(".claude-plugin");
    let commands = dir.join("commands");
    std::fs::create_dir_all(&manifest)?;
    std::fs::create_dir_all(&commands)?;
    std::fs::write(
        manifest.join("plugin.json"),
        r#"{"name":"pebl","description":"Commands pebl adds to its Claude sessions"}"#,
    )?;
    std::fs::write(commands.join("switch.md"), COMMAND)?;
    Ok(())
}

fn copy_tree(from: &Path, to: &Path) -> std::io::Result<()> {
    if from.is_dir() {
        std::fs::create_dir_all(to)?;
        for e in std::fs::read_dir(from)?.flatten() {
            copy_tree(&e.path(), &to.join(e.file_name()))?;
        }
        Ok(())
    } else {
        if let Some(dir) = to.parent() {
            std::fs::create_dir_all(dir)?;
        }
        std::fs::copy(from, to).map(|_| ())
    }
}

impl<B: SessionBackend + UiBackend> App<B> {
    pub(super) fn queue_switch(&mut self, sid: &str, account: &str) {
        self.switching.retain(|(s, _, _)| s.as_str() != sid);
        self.switching
            .push((sid.to_string(), account.to_string(), Instant::now()));
    }

    pub(super) fn switch_tick(&mut self) {
        let (due, wait): (Vec<_>, Vec<_>) = std::mem::take(&mut self.switching)
            .into_iter()
            .partition(|(_, _, at)| at.elapsed() >= SETTLE);
        self.switching = wait;
        for (sid, account, _) in due {
            if let Err(e) = self.switch_now(&sid, &account) {
                self.flash(format!("couldn't switch accounts: {e:#}"));
            }
        }
    }

    fn account_dir(&self, name: &str) -> Option<PathBuf> {
        if name == DEFAULT {
            return Some(self.env.home.join(".claude"));
        }
        self.env.profiles.get(name).cloned()
    }

    fn switch_now(&mut self, sid: &str, account: &str) -> Result<()> {
        let s = self
            .st
            .session(sid)
            .cloned()
            .ok_or_else(|| anyhow!("that session is gone"))?;
        let to = self
            .account_dir(account)
            .ok_or_else(|| anyhow!("no account called {account}; run pebl reload"))?;
        let from = self
            .claude_config_dir(&s)
            .unwrap_or_else(|| self.env.home.join(".claude"));
        let old = crate::history::transcript_in(&[from.join("projects")], &s.claude_id);
        if let Some(parent) = old.as_deref().and_then(Path::parent) {
            if let Some(folder) = parent.file_name() {
                let dest = to.join("projects").join(folder);
                for name in [format!("{}.jsonl", s.claude_id), s.claude_id.clone()] {
                    if parent.join(&name).exists() {
                        copy_tree(&parent.join(&name), &dest.join(&name))?;
                    }
                }
            }
        }
        if let Some(m) = self.st.session_mut(sid) {
            m.profile = Some(account.to_string());
        }
        self.resume(sid)?;
        self.sync_main();
        let label = self
            .accounts()
            .into_iter()
            .find(|a| a.name == account)
            .map(|a| a.label())
            .unwrap_or_else(|| account.to_string());
        self.flash(format!("{} now uses {label}", s.name));
        Ok(())
    }
}
