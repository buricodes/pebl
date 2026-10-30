use super::App;
use crate::backend::{SessionBackend, UiBackend};
use crate::checks::{self, Checks, Outcome};
use crate::view::CheckBadge;
use anyhow::{anyhow, Result};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};

pub(super) type Finished = Arc<Mutex<HashMap<String, Report>>>;

#[derive(Clone, Debug)]
pub(super) enum Report {
    Ran(Vec<Outcome>, Option<u64>),
    Unchanged,
}

#[derive(Clone, Debug, Default)]
pub(super) struct CheckState {
    running: bool,
    rerun: bool,
    outcomes: Vec<Outcome>,
    print: Option<u64>,
    fixes: u32,
    auto_sent: bool,
}

impl<B: SessionBackend + UiBackend> App<B> {
    fn checks_for(&self, sid: &str) -> Checks {
        let Some(s) = self.st.session(sid) else {
            return Checks::default();
        };
        let project = self.st.project(&s.project).map(|p| p.path.clone());
        let mut dirs = vec![s.cwd.as_path()];
        if let Some(p) = project.as_deref() {
            dirs.push(p);
        }
        checks::load(&dirs)
    }

    pub(super) fn turn_ended(&mut self, sid: &str) {
        if !self.checks_for(sid).is_empty() {
            self.start_checks(sid, false);
        }
    }

    pub(super) fn prompt_submitted(&mut self, sid: &str) {
        if let Some(c) = self.checks.get_mut(sid) {
            if c.auto_sent {
                c.auto_sent = false;
            } else {
                c.fixes = 0;
            }
        }
    }

    fn start_checks(&mut self, sid: &str, force: bool) {
        let checks = self.checks_for(sid);
        let Some(cwd) = self.st.session(sid).map(|s| s.cwd.clone()) else {
            return;
        };
        let state = self.checks.entry(sid.to_string()).or_default();
        if state.running {
            state.rerun = true;
            return;
        }
        state.running = true;
        let last = if force { None } else { state.print };
        let done = self.checks_done.clone();
        let sid = sid.to_string();
        std::thread::spawn(move || {
            let print = checks::fingerprint(&cwd);
            let report = if print.is_some() && print == last {
                Report::Unchanged
            } else {
                Report::Ran(checks::run(&cwd, &checks), print)
            };
            if let Ok(mut d) = done.lock() {
                d.insert(sid, report);
            }
        });
    }

    pub(super) fn collect_checks(&mut self) {
        let st = &self.st;
        self.checks.retain(|sid, _| st.session(sid).is_some());
        let reports: Vec<(String, Report)> = match self.checks_done.lock() {
            Ok(mut d) => d.drain().collect(),
            Err(_) => return,
        };
        for (sid, report) in reports {
            let auto_fix = self.checks_for(&sid).auto_fix;
            let name = self
                .st
                .session(&sid)
                .map(|s| s.name.clone())
                .unwrap_or_default();
            let Some(state) = self.checks.get_mut(&sid) else {
                continue;
            };
            state.running = false;
            let rerun = std::mem::take(&mut state.rerun);
            if let Report::Ran(outcomes, print) = report {
                state.outcomes = outcomes;
                state.print = print;
                let failed = state.outcomes.iter().any(|o| !o.passed());
                let auto = failed && state.fixes < auto_fix;
                let summary = checks::summary(&state.outcomes);
                if auto {
                    state.fixes += 1;
                    state.auto_sent = true;
                    let prompt = checks::fix_prompt(&state.outcomes);
                    let _ = self.deliver(std::slice::from_ref(&sid), &prompt);
                    self.flash(format!("{name}: {summary}, sent back to Claude to fix"));
                } else if failed {
                    self.flash(format!("{name}: {summary} · t sends it to Claude"));
                }
            }
            if rerun {
                self.start_checks(&sid, false);
            }
        }
    }

    pub(super) fn send_failures(&mut self, sid: &str) -> Result<()> {
        let failed: Vec<Outcome> = self
            .checks
            .get(sid)
            .map(|c| c.outcomes.iter().filter(|o| !o.passed()).cloned().collect())
            .unwrap_or_default();
        if failed.is_empty() {
            return Err(anyhow!("no failing checks for this session"));
        }
        let (now_n, _) = self.deliver(&[sid.to_string()], &checks::fix_prompt(&failed))?;
        self.flash(if now_n == 1 {
            "sent the failing checks to Claude"
        } else {
            "failing checks queued until this turn ends"
        });
        Ok(())
    }

    pub(super) fn run_checks_now(&mut self, sid: &str) -> Result<()> {
        if self.checks_for(sid).is_empty() {
            return Err(anyhow!(
                "no checks for this project: add a [checks] table to pebl.toml"
            ));
        }
        self.start_checks(sid, true);
        self.flash("running checks…");
        Ok(())
    }

    pub(super) fn check_badge(&self, sid: &str) -> Option<CheckBadge> {
        let c = self.checks.get(sid)?;
        if c.running {
            return Some(CheckBadge::Running);
        }
        if c.outcomes.is_empty() {
            return None;
        }
        Some(if c.outcomes.iter().all(Outcome::passed) {
            CheckBadge::Passed
        } else {
            CheckBadge::Failed
        })
    }
}
