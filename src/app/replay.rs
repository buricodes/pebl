use super::App;
use crate::backend::{SessionBackend, UiBackend};
use crate::model::{now, Status};
use crate::timeline::{self, Event};
use anyhow::{anyhow, bail, Result};
use std::sync::{Arc, Mutex};

pub(super) type Snapshots = Arc<Mutex<Vec<(String, Event)>>>;

impl<B: SessionBackend + UiBackend> App<B> {
    pub(super) fn record(&self, sid: &str, event: Event) {
        let _ = timeline::append(&self.env.timeline, sid, &event);
    }

    pub(super) fn record_hook(&mut self, sid: &str, event: &str, payload: &serde_json::Value) {
        match event {
            "UserPromptSubmit" => {
                if let Some(p) = payload.get("prompt").and_then(|p| p.as_str()) {
                    self.record(
                        sid,
                        Event::Prompt {
                            at: now(),
                            text: p.chars().take(400).collect(),
                        },
                    );
                }
            }
            "PreToolUse" => {
                if let Some(name) = payload.get("tool_name").and_then(|t| t.as_str()) {
                    let detail = payload
                        .get("tool_input")
                        .map(timeline::tool_detail)
                        .unwrap_or_default();
                    self.record(
                        sid,
                        Event::Tool {
                            at: now(),
                            name: name.to_string(),
                            detail,
                        },
                    );
                }
            }
            "Stop" => self.take_checkpoint(sid),
            _ => {}
        }
    }

    fn last_checkpoint(&self, sid: &str) -> Option<String> {
        timeline::read(&self.env.timeline, sid)
            .into_iter()
            .rev()
            .find_map(|e| match e {
                Event::Turn { checkpoint, .. } => checkpoint,
                Event::Restore { to, .. } => Some(to),
                _ => None,
            })
    }

    fn take_checkpoint(&mut self, sid: &str) {
        let Some(s) = self.st.session(sid).cloned() else {
            return;
        };
        if s.reviews.is_some() || !crate::git::is_repo(&s.cwd) {
            return;
        }
        let last = self.last_checkpoint(sid);
        let label = format!("pebl: {} after a turn", s.name);
        let out = self.snapshots.clone();
        let sid = sid.to_string();
        std::thread::spawn(move || {
            let event = match timeline::checkpoint(&s.cwd, &sid, last.as_deref(), &label) {
                Ok(Some(snap)) => Event::Turn {
                    at: now(),
                    checkpoint: Some(snap.commit),
                    files: snap.files,
                    adds: snap.adds,
                    dels: snap.dels,
                },
                _ => Event::Turn {
                    at: now(),
                    checkpoint: None,
                    files: Vec::new(),
                    adds: 0,
                    dels: 0,
                },
            };
            if let Ok(mut o) = out.lock() {
                o.push((sid, event));
            }
        });
    }

    pub(super) fn collect_snapshots(&mut self) {
        let done: Vec<(String, Event)> = match self.snapshots.lock() {
            Ok(mut o) => o.drain(..).collect(),
            Err(_) => return,
        };
        for (sid, event) in done {
            self.record(&sid, event);
        }
    }

    pub(super) fn restore_checkpoint(&mut self, sid: &str, to: &str) -> Result<String> {
        let s = self
            .st
            .session(sid)
            .cloned()
            .ok_or_else(|| anyhow!("no such session"))?;
        if matches!(s.status, Status::Running | Status::Waiting) {
            bail!("{} is busy; wait for its turn to end, then restore", s.name);
        }
        let last = self.last_checkpoint(sid);
        let saved = timeline::restore(&s.cwd, sid, to, last.as_deref())?;
        self.record(
            sid,
            Event::Restore {
                at: now(),
                to: to.to_string(),
                saved,
            },
        );
        let msg = format!(
            "restored {}'s files; its chat still remembers later turns (/rewind in Claude rewinds that)",
            s.name
        );
        self.flash(msg.clone());
        Ok(msg)
    }

    pub(super) fn forget_timeline(&self, sid: &str, cwd: &std::path::Path) {
        let _ = std::fs::remove_file(timeline::file(&self.env.timeline, sid));
        let (cwd, sid) = (cwd.to_path_buf(), sid.to_string());
        std::thread::spawn(move || timeline::forget(&cwd, &sid));
    }

    pub(super) fn open_timeline(&mut self, sid: &str) -> Result<()> {
        let s = self
            .st
            .session(sid)
            .cloned()
            .ok_or_else(|| anyhow!("no such session"))?;
        let project = self
            .st
            .project(&s.project)
            .map(|p| p.name.clone())
            .unwrap_or_default();
        let title = format!("{project} / {}", s.name);
        let q = crate::backend::tmux::sh_quote;
        let cmd = format!(
            "{} timeline --sid {} --title {} --color {}",
            q(&self.env.pebl),
            q(&s.id),
            q(&title),
            if self.color == crate::ui::theme::ColorMode::Indexed {
                "256"
            } else {
                "truecolor"
            }
        );
        self.be.popup(&s.cwd, &format!(" {title} "), &cmd)
    }
}
