use super::{edit, App, IMode};
use crate::backend::{SessionBackend, UiBackend};
use crate::ipc::{Key, KeyCode};
use crate::prompts::{self, Prompt};
use crate::view::{Clip, Focus, PromptRow, PromptStep, PromptsView};
use anyhow::{anyhow, bail, Result};

const ROWS: usize = 6;

#[derive(Clone, Debug, PartialEq)]
pub(super) struct Picker {
    query: String,
    sel: usize,
    targets: Vec<String>,
    marked: bool,
    step: PromptStep,
}

impl<B: SessionBackend + UiBackend> App<B> {
    pub(super) fn start_prompts(&mut self) -> Result<()> {
        let (targets, marked) = match &self.mode {
            IMode::Mark { marks } | IMode::Broadcast { marks, .. } if !marks.is_empty() => {
                (marks.clone(), true)
            }
            _ => {
                let sid = self
                    .active
                    .clone()
                    .filter(|sid| self.st.session(sid).is_some_and(|s| s.pane.is_some()))
                    .ok_or_else(|| anyhow!("open a session first, or mark several with ctrl+x"))?;
                (vec![sid], false)
            }
        };
        self.cancel_mode();
        self.mode = IMode::Prompts(Picker {
            query: String::new(),
            sel: 0,
            targets,
            marked,
            step: PromptStep::List,
        });
        self.refresh_prompts();
        self.apply_layout();
        self.set_focus(Focus::Bar);
        Ok(())
    }

    pub(super) fn refresh_prompts(&mut self) {
        self.saved = prompts::load(&self.env.prompts);
        self.clip = (self.env.clipboard)();
    }

    fn shown(&self, query: &str) -> Vec<&Prompt> {
        self.saved.iter().filter(|p| p.matches(query)).collect()
    }

    fn selected(&self, pk: &Picker) -> Option<Prompt> {
        self.shown(&pk.query).get(pk.sel).map(|p| (*p).clone())
    }

    pub(super) fn prompts_key(&mut self, mut pk: Picker, k: Key) -> Result<()> {
        let res = match std::mem::take(&mut pk.step) {
            PromptStep::List => self.list_key(&mut pk, k),
            PromptStep::Name { text, last } => self.name_key(&mut pk, text, last, k),
            PromptStep::ConfirmDelete => self.delete_key(&mut pk, k),
        };
        match res {
            Ok(true) => Ok(()),
            other => {
                self.mode = IMode::Prompts(pk);
                other.map(|_| ())
            }
        }
    }

    fn list_key(&mut self, pk: &mut Picker, k: Key) -> Result<bool> {
        match k.code {
            KeyCode::Esc => {
                self.close_prompts(pk);
                return Ok(true);
            }
            KeyCode::Up => pk.sel = pk.sel.saturating_sub(1),
            KeyCode::Down => {
                pk.sel = (pk.sel + 1).min(self.shown(&pk.query).len().saturating_sub(1))
            }
            KeyCode::Enter | KeyCode::Tab => {
                let p = self
                    .selected(pk)
                    .ok_or_else(|| anyhow!("no prompt to send; ctrl+a adds one"))?;
                let text = p.fill(self.clip.as_deref())?;
                if k.code == KeyCode::Tab {
                    self.type_prompt(pk, &text)?;
                } else {
                    self.send_prompt(pk, &p.name, &text)?;
                }
                return Ok(true);
            }
            KeyCode::Char('a') if k.ctrl => {
                pk.step = PromptStep::Name {
                    text: String::new(),
                    last: false,
                }
            }
            KeyCode::Char('s') if k.ctrl => {
                if !self.last_prompt.contains_key(&pk.targets[0]) {
                    bail!("send a prompt in this session first, then save it here");
                }
                pk.step = PromptStep::Name {
                    text: String::new(),
                    last: true,
                }
            }
            KeyCode::Char('e') if k.ctrl => {
                let p = self
                    .selected(pk)
                    .ok_or_else(|| anyhow!("no prompt to edit"))?;
                self.launch_editor(&p.path)?;
                self.flash(format!(
                    "editing “{}”: the list updates when you save",
                    p.name
                ));
            }
            KeyCode::Char('d') if k.ctrl => {
                self.selected(pk)
                    .ok_or_else(|| anyhow!("no prompt to delete"))?;
                pk.step = PromptStep::ConfirmDelete;
            }
            KeyCode::Char('u') if k.ctrl => {
                pk.query.clear();
                pk.sel = 0;
            }
            _ if k.ctrl => {}
            _ => {
                edit(&mut pk.query, &k, false);
                pk.sel = 0;
            }
        }
        Ok(false)
    }

    fn name_key(&mut self, pk: &mut Picker, mut text: String, last: bool, k: Key) -> Result<bool> {
        match k.code {
            KeyCode::Esc => {}
            KeyCode::Enter => {
                pk.step = PromptStep::Name {
                    text: text.clone(),
                    last,
                };
                let path = if last {
                    let body = self
                        .last_prompt
                        .get(&pk.targets[0])
                        .cloned()
                        .ok_or_else(|| anyhow!("no prompt sent in this session yet"))?;
                    prompts::create(&self.env.prompts, &text, &body)?
                } else {
                    prompts::create_blank(&self.env.prompts, &text)?
                };
                self.refresh_prompts();
                pk.query.clear();
                pk.sel = self.saved.iter().position(|p| p.path == path).unwrap_or(0);
                pk.step = PromptStep::List;
                if last {
                    self.flash(format!("saved “{}”: ctrl+e to edit it", text.trim()));
                } else {
                    self.launch_editor(&path)?;
                    self.flash(format!("write “{}” in the editor, then save", text.trim()));
                }
            }
            _ => {
                edit(&mut text, &k, false);
                pk.step = PromptStep::Name { text, last };
            }
        }
        Ok(false)
    }

    fn delete_key(&mut self, pk: &mut Picker, k: Key) -> Result<bool> {
        if k.code == KeyCode::Char('y') {
            if let Some(p) = self.selected(pk) {
                std::fs::remove_file(&p.path)
                    .map_err(|e| anyhow!("deleting {}: {e}", p.path.display()))?;
                self.refresh_prompts();
                pk.sel = pk.sel.min(self.shown(&pk.query).len().saturating_sub(1));
                self.flash(format!("deleted “{}”", p.name));
            }
        }
        Ok(false)
    }

    fn send_prompt(&mut self, pk: &Picker, name: &str, text: &str) -> Result<()> {
        let (now_n, later) = self.deliver(&pk.targets, text)?;
        self.finish_prompts(pk);
        self.flash(match (pk.targets.len(), later) {
            (1, 0) => format!("sent “{name}”"),
            (1, _) => format!("“{name}” queued until this turn ends"),
            (_, 0) => format!("sent “{name}” to {now_n} sessions"),
            _ => format!("sent “{name}” to {now_n}, {later} queued until their turn ends"),
        });
        Ok(())
    }

    fn type_prompt(&mut self, pk: &Picker, text: &str) -> Result<()> {
        let [sid] = pk.targets.as_slice() else {
            bail!("tab types into one session; enter sends to all of them");
        };
        let pane = self
            .st
            .session(sid)
            .and_then(|s| s.pane.clone())
            .ok_or_else(|| anyhow!("that session isn't running"))?;
        self.be.send_text(&pane, text, false)?;
        self.finish_prompts(pk);
        Ok(())
    }

    fn finish_prompts(&mut self, pk: &Picker) {
        self.cancel_mode();
        let back = if pk.marked || self.active.is_none() {
            Focus::Sidebar
        } else {
            Focus::Main
        };
        self.set_focus(back);
    }

    fn close_prompts(&mut self, pk: &Picker) {
        if pk.marked {
            self.cancel_mode();
            self.mode = IMode::Mark {
                marks: pk.targets.clone(),
            };
            self.set_focus(Focus::Sidebar);
        } else {
            self.finish_prompts(pk);
        }
    }

    pub(super) fn prompts_view(&self, pk: &Picker) -> PromptsView {
        let shown = self.shown(&pk.query);
        let start = (pk.sel + 1).saturating_sub(ROWS);
        let rows = shown
            .iter()
            .skip(start)
            .take(ROWS)
            .map(|p| PromptRow {
                name: p.name.clone(),
                preview: p.preview(),
                clipboard: p.uses_clipboard(),
            })
            .collect();
        let clip = match shown.get(pk.sel) {
            Some(p) if p.uses_clipboard() => clip_summary(self.clip.as_deref()),
            _ => Clip::Unused,
        };
        let target = match pk.targets.as_slice() {
            [sid] => self
                .st
                .session(sid)
                .map(|s| {
                    let project = self
                        .st
                        .project(&s.project)
                        .map(|p| p.name.as_str())
                        .unwrap_or("");
                    format!("{project} / {}", s.name)
                })
                .unwrap_or_default(),
            many => format!("{} sessions", many.len()),
        };
        PromptsView {
            query: pk.query.clone(),
            sel: pk.sel - start,
            rows,
            total: self.saved.len(),
            target,
            clip,
            step: pk.step.clone(),
        }
    }
}

fn clip_summary(clip: Option<&str>) -> Clip {
    let text = clip.map(str::trim).unwrap_or("");
    match text.lines().map(str::trim).find(|l| !l.is_empty()) {
        Some(first) => Clip::Text {
            first: first.to_string(),
            lines: text.lines().count(),
        },
        None => Clip::Empty,
    }
}
