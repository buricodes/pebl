use super::auto_checks::Progress;
use super::App;
use crate::backend::{SessionBackend, UiBackend};
use crate::model::{now, Status, Task, TaskState};
use crate::recipes::{Recipe, Step};
use crate::review;
use anyhow::{anyhow, Result};

const FIX_TRIES: u8 = 2;
const PICKUP: std::time::Duration = std::time::Duration::from_secs(60);

impl<B: SessionBackend + UiBackend> App<B> {
    pub(super) fn queue_recipe(
        &mut self,
        pid: &str,
        recipe: &Recipe,
        plan: Vec<Step>,
    ) -> Result<String> {
        let first = match plan.first() {
            Some(Step::Prompt(p)) => p.clone(),
            _ => String::new(),
        };
        let id = self.queue_add(
            pid,
            if first.is_empty() {
                &recipe.name
            } else {
                &first
            },
            1,
        )?;
        let task = self
            .st
            .task_mut(&id)
            .ok_or_else(|| anyhow!("the task vanished"))?;
        let about = Task::title_of(&first);
        task.title = if first.is_empty() || about.is_empty() {
            recipe.name.clone()
        } else {
            format!("{}: {about}", recipe.name)
                .chars()
                .take(60)
                .collect()
        };
        task.plan = plan;
        self.run_queue();
        Ok(id)
    }

    pub(super) fn recipe_from_cli(
        &mut self,
        project: &str,
        name: &str,
        vars: &[&str],
    ) -> Result<String> {
        let pid = self.project_for(project)?;
        let path = self.st.project(&pid).map(|p| p.path.clone());
        let recipe = crate::recipes::load(&self.env.recipes, path.as_deref())
            .into_iter()
            .find(|r| r.name == name || crate::prompts::slug(&r.name) == crate::prompts::slug(name))
            .ok_or_else(|| anyhow!("no recipe called “{name}” (pebl recipe ls lists them)"))?;
        let mut values = std::collections::BTreeMap::new();
        for v in vars {
            let (k, val) = v
                .split_once('=')
                .ok_or_else(|| anyhow!("--var takes name=value, not “{v}”"))?;
            values.insert(k.trim().to_string(), val.to_string());
        }
        if let Some(missing) = recipe.vars().into_iter().find(|v| !values.contains_key(v)) {
            return Err(anyhow!("“{}” needs --var {missing}=…", recipe.name));
        }
        let clip = (self.env.clipboard)();
        let plan = recipe.fill(&values, clip.as_deref())?;
        self.queue_recipe(&pid, &recipe, plan)?;
        Ok(format!("started “{}” in a new worktree", recipe.name))
    }

    pub(super) fn drive_recipes(&mut self) {
        let ids: Vec<String> = self
            .st
            .tasks
            .iter()
            .filter(|t| t.state == TaskState::Running && !t.plan.is_empty())
            .map(|t| t.id.clone())
            .collect();
        for id in ids {
            self.drive_recipe(&id);
        }
    }

    fn drive_recipe(&mut self, id: &str) {
        let Some(t) = self.st.tasks.iter().find(|t| t.id == id).cloned() else {
            return;
        };
        let Some(sid) = t.sessions.first().cloned() else {
            return;
        };
        let Some(status) = self.st.session(&sid).map(|s| s.status) else {
            return self.end_recipe(id, TaskState::Failed, "its session was closed".into());
        };
        match status {
            Status::Running | Status::Waiting => return,
            Status::Error | Status::Exited => {
                return self.end_recipe(id, TaskState::Failed, "the session stopped".into())
            }
            Status::Idle => {}
        }
        let progress = self.check_progress(&sid);
        let picking_up = self.sent.get(&sid).is_some_and(|at| at.elapsed() < PICKUP);
        if progress == Progress::Busy || picking_up {
            return;
        }
        match t.plan.get(t.at) {
            None => self.complete_step(id, &sid),
            Some(Step::Prompt(_)) => self.complete_step(id, &sid),
            Some(Step::Checks) if progress == Progress::Failed && t.fixes < FIX_TRIES => {
                if self.send_failures(&sid).is_ok() {
                    if let Some(task) = self.st.task_mut(id) {
                        task.fixes += 1;
                    }
                }
            }
            Some(Step::Checks) if progress == Progress::Failed => {
                let summary = self.check_summary(&sid).unwrap_or_default();
                self.end_recipe(
                    id,
                    TaskState::Failed,
                    format!("still failing after {FIX_TRIES} fixes: {summary}"),
                );
            }
            Some(Step::Checks) => self.complete_step(id, &sid),
            Some(Step::Review) => self.drive_review(id, &sid, &t),
        }
    }

    fn drive_review(&mut self, id: &str, sid: &str, t: &Task) {
        match &t.reviewer {
            None => {
                let started = self.start_reviewer(sid).ok().and_then(|_| {
                    self.reviewers
                        .iter()
                        .find(|(_, author)| *author == sid)
                        .map(|(r, _)| r.clone())
                });
                match started {
                    Some(r) => {
                        if let Some(task) = self.st.task_mut(id) {
                            task.reviewer = Some(r);
                        }
                    }
                    None => self.complete_step(id, sid),
                }
            }
            Some(r) if self.reviewers.contains_key(r) => {}
            Some(_) if !t.review_sent => {
                let found = review::read_findings(&self.env.reviews, sid);
                if found.is_empty() {
                    return self.complete_step(id, sid);
                }
                let _ = std::fs::remove_file(review::findings_file(&self.env.reviews, sid));
                if self
                    .deliver(&[sid.to_string()], &review::findings_prompt(&found))
                    .is_ok()
                {
                    if let Some(task) = self.st.task_mut(id) {
                        task.review_sent = true;
                    }
                }
            }
            Some(_) => self.complete_step(id, sid),
        }
    }

    fn complete_step(&mut self, id: &str, sid: &str) {
        let next = match self.st.task_mut(id) {
            Some(task) => {
                task.at += 1;
                task.fixes = 0;
                task.reviewer = None;
                task.review_sent = false;
                task.plan.get(task.at).cloned()
            }
            None => return,
        };
        match next {
            None => {
                let summary = self.check_summary(sid);
                let note = match summary {
                    Some(s) => format!("recipe done · {s}"),
                    None => "recipe done".into(),
                };
                self.end_recipe(id, TaskState::Review, note);
            }
            Some(Step::Prompt(p)) => {
                let _ = self.deliver(&[sid.to_string()], &p);
            }
            Some(_) => {}
        }
    }

    fn end_recipe(&mut self, id: &str, state: TaskState, note: String) {
        let title = match self.st.task_mut(id) {
            Some(task) => {
                task.state = state;
                task.note = note;
                task.ended = Some(now());
                task.title.clone()
            }
            None => return,
        };
        self.flash(match state {
            TaskState::Review => format!("“{title}” finished every step · q"),
            _ => format!("“{title}” stopped · q"),
        });
    }
}
