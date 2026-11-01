use super::{App, Focus, IMode};
use crate::backend::{SessionBackend, UiBackend};
use crate::ipc::{Key, KeyCode};
use crate::jel::{self, Act};
use crate::view::{AskRow, AskView};
use anyhow::Result;
use std::sync::{Arc, Mutex};

const KEEP: usize = 6;

pub(super) type Answers = Arc<Mutex<Vec<(u64, String)>>>;

#[derive(Clone, Debug)]
pub(super) struct Asked {
    id: u64,
    question: String,
    answer: Option<String>,
    act: Option<Act>,
}

impl<B: SessionBackend + UiBackend> App<B> {
    pub(super) fn start_ask(&mut self) {
        self.cancel_mode();
        self.mode = IMode::Ask {
            text: String::new(),
            back: 0,
        };
        self.set_focus(Focus::Bar);
    }

    pub(super) fn ask_key(&mut self, text: String, back: usize, k: Key) -> Result<()> {
        let mut text = text;
        let mut back = back;
        match k.code {
            KeyCode::Esc => {
                self.mode = IMode::Normal;
                self.set_focus(Focus::Main);
                return Ok(());
            }
            KeyCode::Enter if text.trim().is_empty() => {
                let act = self.asked.last().and_then(|a| a.act.clone());
                if let Some(act) = act {
                    self.mode = IMode::Normal;
                    return self.act_on(act);
                }
            }
            KeyCode::Enter => {
                self.ask(text.trim());
                text.clear();
                back = 0;
            }
            KeyCode::Up => {
                let asked: Vec<&Asked> = self.asked.iter().rev().collect();
                if let Some(a) = asked.get(back) {
                    text = a.question.clone();
                    back += 1;
                }
            }
            KeyCode::Down => {
                back = back.saturating_sub(1);
                text.clear();
            }
            KeyCode::Backspace => {
                text.pop();
            }
            KeyCode::Char('u') if k.ctrl => text.clear(),
            KeyCode::Char(c) if !k.ctrl => text.push(c),
            _ => {}
        }
        self.mode = IMode::Ask { text, back };
        Ok(())
    }

    fn ask(&mut self, question: &str) {
        self.ask_seq += 1;
        let id = self.ask_seq;
        self.asked.push(Asked {
            id,
            question: question.to_string(),
            answer: None,
            act: None,
        });
        let over = self.asked.len().saturating_sub(KEEP);
        self.asked.drain(..over);
        let system = jel::system_prompt(&crate::ui::stage::help_text(), &self.sessions_brief());
        let claude = self.env.claude.clone();
        let ask = self.env.ask;
        let done = self.answers.clone();
        let question = question.to_string();
        std::thread::spawn(move || {
            let reply = match ask(&claude, &system, &question) {
                Ok(r) => r,
                Err(e) => format!("couldn't reach Claude: {e:#}"),
            };
            if let Ok(mut d) = done.lock() {
                d.push((id, reply));
            }
        });
    }

    pub(super) fn collect_answers(&mut self) {
        let replies: Vec<(u64, String)> = match self.answers.lock() {
            Ok(mut d) => d.drain(..).collect(),
            Err(_) => return,
        };
        for (id, reply) in replies {
            if let Some(a) = self.asked.iter_mut().find(|a| a.id == id) {
                let (text, act) = jel::parse(&reply);
                a.answer = Some(text);
                a.act = act;
            }
        }
    }

    fn act_on(&mut self, act: Act) -> Result<()> {
        match act {
            Act::Do(cmd) => {
                self.set_focus(Focus::Main);
                let reply = self.handle_ctl(&[cmd]);
                if !reply.is_empty() && reply != "ok" {
                    self.flash(reply);
                }
                Ok(())
            }
            Act::Go(name) => {
                let want = name.to_lowercase();
                let sid = self
                    .st
                    .sessions
                    .iter()
                    .find(|s| s.name.to_lowercase() == want)
                    .or_else(|| {
                        self.st
                            .sessions
                            .iter()
                            .find(|s| s.name.to_lowercase().contains(&want))
                    })
                    .map(|s| s.id.clone())
                    .ok_or_else(|| anyhow::anyhow!("no session called {name}"))?;
                self.activate(&sid);
                self.set_focus(Focus::Main);
                Ok(())
            }
        }
    }

    fn sessions_brief(&self) -> String {
        let lines: Vec<String> = self
            .st
            .sessions
            .iter()
            .take(40)
            .map(|s| {
                let project = self
                    .st
                    .project(&s.project)
                    .map(|p| p.name.clone())
                    .unwrap_or_default();
                let mut line = format!(
                    "- {} (project {}, {:?}, folder {})",
                    s.name,
                    project,
                    s.status,
                    s.cwd.display()
                );
                if let Some(c) = self.check_summary(&s.id) {
                    line.push_str(&format!(", checks: {c}"));
                }
                if let Some(p) = self.ctx_pct(&s.id) {
                    line.push_str(&format!(", context {p}% used"));
                }
                line
            })
            .collect();
        if lines.is_empty() {
            "none yet".into()
        } else {
            lines.join("\n")
        }
    }

    pub(super) fn ask_view(&self, text: &str) -> AskView {
        AskView {
            rows: self
                .asked
                .iter()
                .map(|a| AskRow {
                    question: a.question.clone(),
                    answer: a.answer.clone(),
                    act: a.act.as_ref().map(Act::label),
                })
                .collect(),
            text: text.to_string(),
        }
    }
}
