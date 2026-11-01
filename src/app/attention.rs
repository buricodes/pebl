use super::{App, IMode};
use crate::backend::{SessionBackend, UiBackend};
use crate::ipc::{Key, KeyCode};
use crate::model::{age, now, Status};
use crate::view::{Focus, InboxRow, InboxView, NoticeKind};
use anyhow::{anyhow, bail, Result};

const ROWS: usize = 8;

#[derive(Clone, Debug, PartialEq)]
pub(super) struct Item {
    pub sid: String,
    pub kind: NoticeKind,
    pub since: u64,
}

impl<B: SessionBackend + UiBackend> App<B> {
    pub(super) fn attention(&self) -> Vec<Item> {
        let mut waiting: Vec<Item> = self
            .st
            .sessions
            .iter()
            .filter(|s| s.status == Status::Waiting)
            .map(|s| Item {
                sid: s.id.clone(),
                kind: NoticeKind::Waiting,
                since: s.since,
            })
            .collect();
        waiting.sort_by_key(|i| i.since);
        let alerts = |kind: NoticeKind| {
            let mut v: Vec<Item> = self
                .alerts
                .iter()
                .filter(|(_, (k, _))| *k == kind)
                .map(|(sid, (k, at))| Item {
                    sid: sid.clone(),
                    kind: *k,
                    since: *at,
                })
                .collect();
            v.sort_by_key(|i| i.since);
            v
        };
        waiting.extend(alerts(NoticeKind::Error));
        waiting.extend(alerts(NoticeKind::Done));
        waiting
    }

    pub(super) fn waiting_detail(&self, sid: &str) -> String {
        match self.tool.get(sid).map(String::as_str) {
            Some("thinking") | None => "waiting for input".to_string(),
            Some(t) => format!("wants to run {t}"),
        }
    }

    pub(super) fn error_detail(&self, sid: &str) -> String {
        self.st
            .session(sid)
            .and_then(|s| s.exit_code)
            .map(|c| format!("exit {c}"))
            .unwrap_or_else(|| "stopped with an error".into())
    }

    pub(super) fn jump_waiting(&mut self) -> Result<()> {
        let items = self.attention();
        if items.is_empty() {
            self.flash("all clear");
            return Ok(());
        }
        let here = self
            .focused_sid()
            .and_then(|a| items.iter().position(|i| i.sid == a));
        let next = here.map(|i| (i + 1) % items.len()).unwrap_or(0);
        let sid = items[next].sid.clone();
        self.open_attention(&sid);
        Ok(())
    }

    fn open_attention(&mut self, sid: &str) {
        self.cancel_mode();
        self.activate(sid);
        self.sync_main();
        self.set_focus(Focus::Main);
    }

    pub(super) fn start_inbox(&mut self) {
        self.cancel_mode();
        self.mode = IMode::Inbox { sel: 0 };
        self.apply_layout();
        self.set_focus(Focus::Bar);
    }

    pub(super) fn inbox_key(&mut self, mut sel: usize, k: Key) -> Result<()> {
        let items = self.attention();
        let picked = items.get(sel).cloned();
        match k.code {
            KeyCode::Esc | KeyCode::Char('q') => {
                self.cancel_mode();
                let back = if self.active.is_some() {
                    Focus::Main
                } else {
                    Focus::Sidebar
                };
                self.set_focus(back);
                return Ok(());
            }
            KeyCode::Up | KeyCode::Char('k') => sel = sel.saturating_sub(1),
            KeyCode::Down | KeyCode::Char('j') => {
                sel = (sel + 1).min(items.len().saturating_sub(1))
            }
            KeyCode::Enter => {
                let item = picked.ok_or_else(|| anyhow!("nothing needs you"))?;
                self.open_attention(&item.sid);
                return Ok(());
            }
            KeyCode::Char('p') => {
                let item = picked.ok_or_else(|| anyhow!("nothing needs you"))?;
                self.cancel_mode();
                return self.start_peek(&item.sid);
            }
            KeyCode::Char('x') => {
                let item = picked.ok_or_else(|| anyhow!("nothing to dismiss"))?;
                if item.kind == NoticeKind::Waiting {
                    self.mode = IMode::Inbox { sel };
                    bail!("answer it to clear it: enter opens it, p peeks");
                }
                self.alerts.remove(&item.sid);
                sel = sel.min(self.attention().len().saturating_sub(1));
            }
            _ => {}
        }
        self.mode = IMode::Inbox { sel };
        Ok(())
    }

    pub(super) fn inbox_view(&self, sel: usize) -> InboxView {
        let items = self.attention();
        let sel = sel.min(items.len().saturating_sub(1));
        let t = now();
        let start = (sel + 1).saturating_sub(ROWS);
        let rows = items
            .iter()
            .skip(start)
            .take(ROWS)
            .filter_map(|i| {
                let s = self.st.session(&i.sid)?;
                let detail = match i.kind {
                    NoticeKind::Waiting => self.waiting_detail(&i.sid),
                    NoticeKind::Error => self.error_detail(&i.sid),
                    _ => "finished".to_string(),
                };
                Some(InboxRow {
                    kind: i.kind,
                    project: self
                        .st
                        .project(&s.project)
                        .map(|p| p.name.clone())
                        .unwrap_or_default(),
                    session: s.name.clone(),
                    detail,
                    age: age(t.saturating_sub(i.since)),
                })
            })
            .collect();
        InboxView {
            rows,
            sel: sel.saturating_sub(start),
            more: items.len().saturating_sub(start + ROWS),
            waiting: items
                .iter()
                .filter(|i| i.kind == NoticeKind::Waiting)
                .count(),
            total: items.len(),
        }
    }
}
