use super::auto_checks::Progress;
use super::{App, CurRef};
use crate::backend::{SessionBackend, UiBackend};
use crate::model::Status;
use crate::view::Focus;

const MAX: usize = 5;

#[derive(Default)]
struct Hints(Vec<(String, String)>);

impl Hints {
    fn add(&mut self, key: &str, label: impl Into<String>) {
        if self.0.len() < MAX && !self.0.iter().any(|(k, _)| k == key) {
            self.0.push((key.to_string(), label.into()));
        }
    }

    fn done(mut self) -> Vec<(String, String)> {
        self.0.push(("?".into(), "all keys".into()));
        self.0
    }
}

impl<B: SessionBackend + UiBackend> App<B> {
    pub(super) fn hints(&self) -> Vec<(String, String)> {
        match (self.focus, &self.cursor) {
            (Focus::Sidebar, Some(CurRef::Session(sid))) => self.session_hints(sid),
            (Focus::Sidebar, Some(CurRef::Project(pid))) => self.project_hints(pid),
            (Focus::Main, _) => self.claude_hints(),
            _ => Vec::new(),
        }
    }

    fn session_hints(&self, sid: &str) -> Vec<(String, String)> {
        let Some(s) = self.st.session(sid) else {
            return Vec::new();
        };
        let mut h = Hints::default();
        match s.status {
            Status::Waiting => {
                h.add("enter", "answer it");
                h.add("p", "peek and answer");
            }
            Status::Exited | Status::Error => h.add("enter", "resume"),
            _ => {}
        }
        if self.check_progress(sid) == Progress::Failed {
            h.add("t", "send failing checks to Claude");
        }
        if crate::review::findings_file(&self.env.reviews, sid).exists() {
            h.add("d", "see the reviewer's comments");
        }
        let plan = self
            .tokens
            .get(sid)
            .is_some_and(|(_, _, t)| !crate::queue::from_markdown(&t.last_text).is_empty());
        if plan && s.status == Status::Idle {
            h.add("P", "queue its plan");
        }
        if s.worktree.is_some() && s.status == Status::Idle {
            h.add("f", "finish: merge or PR");
        }
        if !self.borrows_folder(sid) {
            h.add("d", "review changes");
            h.add("R", "ask a reviewer");
        }
        h.add("c", "open in a column");
        h.done()
    }

    fn project_hints(&self, pid: &str) -> Vec<(String, String)> {
        let mut h = Hints::default();
        h.add("ctrl+n", "new session");
        let tasks = self
            .st
            .tasks
            .iter()
            .filter(|t| t.project == pid && t.state != crate::model::TaskState::Done)
            .count();
        h.add(
            "q",
            match tasks {
                0 => "task queue".to_string(),
                n => format!("task queue ({n})"),
            },
        );
        h.add("N", "add a note");
        h.add("space", "fold");
        h.done()
    }

    fn claude_hints(&self) -> Vec<(String, String)> {
        let mut h = Hints::default();
        h.add("ctrl+p", "prompts");
        h.add("ctrl+y", "inbox");
        h.add("ctrl+t", "shell");
        h.add("ctrl+space", "sidebar");
        h.add("ctrl+j/k", "switch");
        h.0
    }
}
