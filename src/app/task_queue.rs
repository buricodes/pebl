use super::auto_checks::Progress;
use super::{edit, App, CurRef, IMode, Place};
use crate::backend::{SessionBackend, UiBackend};
use crate::ipc::{Key, KeyCode};
use crate::model::{age, now, short_count, Status, Task, TaskState};
use crate::view::{CompareRow, CompareView, Focus, PlanView, QueueRow, QueueView};
use crate::{git, queue};
use anyhow::{anyhow, bail, Result};

const ROWS: usize = 8;

#[derive(Clone, Debug, PartialEq)]
pub(super) struct Stat {
    sid: String,
    files: usize,
    adds: usize,
    dels: usize,
}

impl<B: SessionBackend + UiBackend> App<B> {
    pub(super) fn queue_add(&mut self, pid: &str, prompt: &str, attempts: u8) -> Result<String> {
        if prompt.trim().is_empty() {
            bail!("type the task first");
        }
        let p = self
            .st
            .project(pid)
            .ok_or_else(|| anyhow!("no such project"))?;
        if !git::is_repo(&p.path) {
            bail!(
                "queued tasks run in their own git worktree, and {} isn't a git repository",
                p.name
            );
        }
        Ok(self.st.add_task(pid, prompt, attempts))
    }

    pub(super) fn run_queue(&mut self) {
        self.drive_recipes();
        self.update_tasks();
        self.start_tasks();
    }

    fn update_tasks(&mut self) {
        let st = &self.st;
        let gone: Vec<String> = st
            .tasks
            .iter()
            .filter(|t| st.project(&t.project).is_none())
            .map(|t| t.id.clone())
            .collect();
        self.st.tasks.retain(|t| !gone.contains(&t.id));
        let changes: Vec<(String, TaskState, String)> = self
            .st
            .tasks
            .iter()
            .filter_map(|t| {
                self.task_progress(t)
                    .map(|(state, note)| (t.id.clone(), state, note))
            })
            .collect();
        for (id, state, note) in changes {
            let title = match self.st.task_mut(&id) {
                Some(t) => {
                    t.state = state;
                    t.note = note;
                    t.ended = Some(now());
                    t.title.clone()
                }
                None => continue,
            };
            let auto = self
                .st
                .tasks
                .iter()
                .find(|t| t.id == id)
                .filter(|t| state == TaskState::Review && t.sessions.len() == 1)
                .and_then(|t| {
                    let path = self.st.project(&t.project)?.path.clone();
                    queue::auto_review(&path).then(|| t.sessions[0].clone())
                });
            if let Some(sid) = auto {
                let _ = self.start_reviewer(&sid);
                continue;
            }
            match state {
                TaskState::Review => self.flash(format!("“{title}” is ready for review · q")),
                TaskState::Failed => self.flash(format!("“{title}” failed · q")),
                _ => {}
            }
        }
    }

    fn task_progress(&self, t: &Task) -> Option<(TaskState, String)> {
        if !t.plan.is_empty() && t.state == TaskState::Running {
            return None;
        }
        let live: Vec<_> = t
            .sessions
            .iter()
            .filter_map(|s| self.st.session(s))
            .collect();
        match t.state {
            TaskState::Review if live.is_empty() => {
                Some((TaskState::Done, "merged or closed".into()))
            }
            TaskState::Running if live.is_empty() => {
                Some((TaskState::Failed, "its sessions were closed".into()))
            }
            TaskState::Running => {
                let (mut ready, mut failed) = (Vec::new(), Vec::new());
                for s in &live {
                    match (s.status, self.check_progress(&s.id)) {
                        (Status::Running | Status::Waiting, _) | (_, Progress::Busy) => {
                            return None
                        }
                        (Status::Error | Status::Exited, _) => failed.push(s.id.clone()),
                        (_, Progress::Failed) => failed.push(s.id.clone()),
                        _ => ready.push(s.id.clone()),
                    }
                }
                if t.attempts > 1 {
                    return Some(match ready.len() {
                        0 => (TaskState::Failed, "every attempt failed".into()),
                        n => (
                            TaskState::Review,
                            format!("{n} of {} attempts ready to compare", live.len()),
                        ),
                    });
                }
                let sid = &live[0].id;
                let checks = self.check_summary(sid);
                Some(match (failed.is_empty(), checks) {
                    (true, Some(c)) => (TaskState::Review, format!("ready · {c}")),
                    (true, None) => (TaskState::Review, "ready for review".into()),
                    (false, Some(c)) => (TaskState::Failed, c),
                    (false, None) => (TaskState::Failed, "the session stopped".into()),
                })
            }
            _ => None,
        }
    }

    fn start_tasks(&mut self) {
        let mut pids: Vec<String> = self
            .st
            .tasks
            .iter()
            .filter(|t| t.state == TaskState::Pending)
            .map(|t| t.project.clone())
            .collect();
        pids.dedup();
        for pid in pids {
            let Some(path) = self.st.project(&pid).map(|p| p.path.clone()) else {
                continue;
            };
            let limit = queue::max_parallel(&path);
            loop {
                let tasks = self.st.tasks.iter().filter(|t| t.project == pid);
                let running = tasks
                    .clone()
                    .filter(|t| t.state == TaskState::Running)
                    .count();
                let next = tasks
                    .clone()
                    .find(|t| t.state == TaskState::Pending)
                    .map(|t| t.id.clone());
                match next {
                    Some(id) if running < limit => self.start_task(&id),
                    _ => break,
                }
            }
        }
    }

    fn start_task(&mut self, id: &str) {
        let Some(t) = self.st.task_mut(id).map(|t| {
            t.runs += 1;
            t.clone()
        }) else {
            return;
        };
        let mut sids = Vec::new();
        let mut err = None;
        for n in 1..=t.attempts {
            let name = queue::session_name(&t, n);
            match self.create_session(&t.project, &name, &t.prompt, Place::Worktree) {
                Ok(sid) => sids.push(sid),
                Err(e) => {
                    err = Some(format!("{e:#}"));
                    break;
                }
            }
        }
        let Some(task) = self.st.task_mut(id) else {
            return;
        };
        task.started = Some(now());
        task.state = if sids.is_empty() {
            TaskState::Failed
        } else {
            TaskState::Running
        };
        task.note = err.unwrap_or_default();
        task.sessions = sids;
    }

    fn discard_session(&mut self, sid: &str) -> Result<()> {
        let Some(s) = self.st.session(sid).cloned() else {
            return Ok(());
        };
        let repo = self
            .st
            .project(&s.project)
            .map(|p| p.path.clone())
            .ok_or_else(|| anyhow!("no such project"))?;
        let branch = s.worktree.as_deref().and_then(|w| git::branch(w).ok());
        self.kill(&CurRef::Session(sid.to_string()))?;
        if let (Some(wt), Some(branch)) = (s.worktree, branch) {
            git::remove_worktree(&repo, &wt, &branch, true)?;
        }
        Ok(())
    }

    fn project_tasks(&self, pid: &str) -> Vec<Task> {
        self.st
            .tasks
            .iter()
            .filter(|t| t.project == pid)
            .cloned()
            .collect()
    }

    pub(super) fn start_queue(&mut self) -> Result<()> {
        let pid = self
            .current_project()
            .ok_or_else(|| anyhow!("add a project first"))?;
        self.cancel_mode();
        self.mode = IMode::Queue {
            pid,
            sel: 0,
            input: None,
            confirm: false,
        };
        self.apply_layout();
        self.set_focus(Focus::Bar);
        Ok(())
    }

    pub(super) fn queue_key(
        &mut self,
        pid: String,
        mut sel: usize,
        mut input: Option<(String, u8)>,
        k: Key,
    ) -> Result<()> {
        let tasks = self.project_tasks(&pid);
        let mut confirm = false;
        if let Some((text, attempts)) = input.as_mut() {
            match k.code {
                KeyCode::Esc => input = None,
                KeyCode::Tab => *attempts = *attempts % 4 + 1,
                KeyCode::Enter => {
                    let (text, attempts) = (text.clone(), *attempts);
                    self.mode = IMode::Queue {
                        pid: pid.clone(),
                        sel,
                        input: input.clone(),
                        confirm,
                    };
                    self.queue_add(&pid, &text, attempts)?;
                    input = None;
                    sel = tasks.len();
                    self.run_queue();
                }
                KeyCode::Char('u') if k.ctrl => text.clear(),
                _ if k.ctrl => {}
                _ => edit(text, &k, false),
            }
            self.mode = IMode::Queue {
                pid,
                sel,
                input,
                confirm,
            };
            return Ok(());
        }
        let picked = tasks.get(sel).cloned();
        let was_confirming = matches!(self.mode, IMode::Queue { confirm: true, .. });
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
                sel = (sel + 1).min(tasks.len().saturating_sub(1))
            }
            KeyCode::Char('a') => input = Some((String::new(), 1)),
            KeyCode::Enter => {
                let t = picked.ok_or_else(|| anyhow!("no tasks yet: a adds one"))?;
                return self.open_task(&t);
            }
            KeyCode::Char('d') => {
                let t = picked.ok_or_else(|| anyhow!("no task here"))?;
                let sid = t
                    .sessions
                    .first()
                    .filter(|_| t.sessions.len() == 1)
                    .ok_or_else(|| anyhow!("open the task to compare its attempts"))?;
                self.review(sid)?;
            }
            KeyCode::Char('D') => {
                let t = picked.ok_or_else(|| anyhow!("no task here"))?;
                if t.state != TaskState::Review {
                    bail!("only a task waiting for review can be marked done");
                }
                if let Some(task) = self.st.task_mut(&t.id) {
                    task.state = TaskState::Done;
                    task.ended = Some(now());
                }
            }
            KeyCode::Char('r') => {
                let t = picked.ok_or_else(|| anyhow!("no task here"))?;
                if t.state != TaskState::Failed {
                    bail!("only a failed task can be retried");
                }
                if let Some(task) = self.st.task_mut(&t.id) {
                    task.state = TaskState::Pending;
                    task.sessions.clear();
                    task.note.clear();
                }
                self.run_queue();
            }
            KeyCode::Char('x') => {
                let t = picked.ok_or_else(|| anyhow!("no task here"))?;
                let live = t.sessions.iter().any(|s| self.st.session(s).is_some());
                let busy = matches!(t.state, TaskState::Running | TaskState::Review) && live;
                if busy && !was_confirming {
                    confirm = true;
                } else {
                    if busy {
                        for sid in &t.sessions {
                            self.discard_session(sid)?;
                        }
                    }
                    self.st.tasks.retain(|x| x.id != t.id);
                    sel = sel.min(tasks.len().saturating_sub(2));
                }
            }
            _ => {}
        }
        self.mode = IMode::Queue {
            pid,
            sel,
            input,
            confirm,
        };
        Ok(())
    }

    fn open_task(&mut self, t: &Task) -> Result<()> {
        if t.state == TaskState::Pending {
            bail!("it starts when a slot frees up");
        }
        let live: Vec<String> = t
            .sessions
            .iter()
            .filter(|s| self.st.session(s).is_some())
            .cloned()
            .collect();
        if live.len() > 1 {
            return self.start_compare(&t.id);
        }
        let sid = live.first().ok_or_else(|| anyhow!("its session is gone"))?;
        self.cancel_mode();
        self.activate(sid);
        self.sync_main();
        self.set_focus(Focus::Main);
        Ok(())
    }

    fn start_compare(&mut self, id: &str) -> Result<()> {
        let t = self
            .st
            .tasks
            .iter()
            .find(|t| t.id == id)
            .cloned()
            .ok_or_else(|| anyhow!("no such task"))?;
        let stats = t
            .sessions
            .iter()
            .filter_map(|sid| {
                let s = self.st.session(sid)?;
                let d = crate::diff::load(&s.cwd).ok();
                let (adds, dels) = d.as_ref().map(|d| d.totals()).unwrap_or((0, 0));
                Some(Stat {
                    sid: sid.clone(),
                    files: d.map(|d| d.files()).unwrap_or(0),
                    adds,
                    dels,
                })
            })
            .collect();
        self.cancel_mode();
        self.mode = IMode::Compare {
            task: id.to_string(),
            sel: 0,
            stats,
            confirm: false,
        };
        self.apply_layout();
        self.set_focus(Focus::Bar);
        Ok(())
    }

    pub(super) fn compare_key(
        &mut self,
        task: String,
        mut sel: usize,
        stats: Vec<Stat>,
        confirm: bool,
        k: Key,
    ) -> Result<()> {
        let picked = stats.get(sel).map(|s| s.sid.clone());
        let mut next_confirm = false;
        match k.code {
            KeyCode::Esc | KeyCode::Char('q') => {
                let pid = self
                    .st
                    .tasks
                    .iter()
                    .find(|t| t.id == task)
                    .map(|t| t.project.clone());
                self.cancel_mode();
                if let Some(pid) = pid {
                    self.mode = IMode::Queue {
                        pid,
                        sel: 0,
                        input: None,
                        confirm: false,
                    };
                    self.apply_layout();
                }
                return Ok(());
            }
            KeyCode::Up | KeyCode::Char('k') => sel = sel.saturating_sub(1),
            KeyCode::Down | KeyCode::Char('j') => {
                sel = (sel + 1).min(stats.len().saturating_sub(1))
            }
            KeyCode::Enter => {
                let sid = picked.ok_or_else(|| anyhow!("no attempt here"))?;
                self.cancel_mode();
                self.activate(&sid);
                self.sync_main();
                self.set_focus(Focus::Main);
                return Ok(());
            }
            KeyCode::Char('d') => {
                let sid = picked.ok_or_else(|| anyhow!("no attempt here"))?;
                self.review(&sid)?;
            }
            KeyCode::Char('w') if confirm => {
                let sid = picked.ok_or_else(|| anyhow!("no attempt here"))?;
                return self.pick_winner(&task, &sid);
            }
            KeyCode::Char('w') => next_confirm = true,
            _ => {}
        }
        self.mode = IMode::Compare {
            task,
            sel,
            stats,
            confirm: next_confirm,
        };
        Ok(())
    }

    fn pick_winner(&mut self, task: &str, winner: &str) -> Result<()> {
        let t = self
            .st
            .tasks
            .iter()
            .find(|t| t.id == task)
            .cloned()
            .ok_or_else(|| anyhow!("no such task"))?;
        for sid in t.sessions.iter().filter(|s| *s != winner) {
            self.discard_session(sid)?;
        }
        let name = self
            .st
            .session(winner)
            .map(|s| s.name.clone())
            .unwrap_or_default();
        if let Some(t) = self.st.task_mut(task) {
            t.sessions = vec![winner.to_string()];
            t.state = TaskState::Review;
            t.note = format!("picked {name}");
        }
        self.cancel_mode();
        self.activate(winner);
        self.sync_main();
        self.set_focus(Focus::Main);
        self.flash(format!(
            "kept {name}, discarded the other attempts · f finishes it"
        ));
        Ok(())
    }

    pub(super) fn start_plan(&mut self, sid: &str) -> Result<()> {
        self.follow_tokens();
        let text = self
            .tokens
            .get(sid)
            .map(|(_, _, t)| t.last_text.clone())
            .unwrap_or_default();
        let tasks = queue::from_markdown(&text);
        if tasks.is_empty() {
            bail!("its last reply has no \"- [ ]\" checklist; ask it to plan as one (ctrl+p, plan into tasks)");
        }
        self.cancel_mode();
        self.mode = IMode::Plan {
            sid: sid.to_string(),
            tasks: tasks.into_iter().map(|t| (t, true)).collect(),
            sel: 0,
        };
        self.apply_layout();
        self.set_focus(Focus::Bar);
        Ok(())
    }

    pub(super) fn plan_key(
        &mut self,
        sid: String,
        mut tasks: Vec<(String, bool)>,
        mut sel: usize,
        k: Key,
    ) -> Result<()> {
        match k.code {
            KeyCode::Esc | KeyCode::Char('q') => {
                self.cancel_mode();
                self.set_focus(Focus::Sidebar);
                return Ok(());
            }
            KeyCode::Up | KeyCode::Char('k') => sel = sel.saturating_sub(1),
            KeyCode::Down | KeyCode::Char('j') => {
                sel = (sel + 1).min(tasks.len().saturating_sub(1))
            }
            KeyCode::Char(' ') => {
                if let Some(t) = tasks.get_mut(sel) {
                    t.1 = !t.1;
                }
            }
            KeyCode::Char('a') => {
                let all = tasks.iter().all(|t| t.1);
                tasks.iter_mut().for_each(|t| t.1 = !all);
            }
            KeyCode::Enter => {
                let pid = self
                    .st
                    .session(&sid)
                    .map(|s| s.project.clone())
                    .ok_or_else(|| anyhow!("that session is gone"))?;
                let picked: Vec<String> =
                    tasks.iter().filter(|t| t.1).map(|t| t.0.clone()).collect();
                if picked.is_empty() {
                    self.mode = IMode::Plan { sid, tasks, sel };
                    bail!("tick at least one task with space");
                }
                for t in &picked {
                    self.mode = IMode::Plan {
                        sid: sid.clone(),
                        tasks: tasks.clone(),
                        sel,
                    };
                    self.queue_add(&pid, t, 1)?;
                }
                self.run_queue();
                self.cancel_mode();
                self.mode = IMode::Queue {
                    pid,
                    sel: 0,
                    input: None,
                    confirm: false,
                };
                self.apply_layout();
                self.flash(format!("queued {} tasks from the plan", picked.len()));
                return Ok(());
            }
            _ => {}
        }
        self.mode = IMode::Plan { sid, tasks, sel };
        Ok(())
    }

    pub(super) fn plan_view(&self, sid: &str, tasks: &[(String, bool)], sel: usize) -> PlanView {
        let start = (sel + 1).saturating_sub(ROWS);
        PlanView {
            session: self
                .st
                .session(sid)
                .map(|s| s.name.clone())
                .unwrap_or_default(),
            rows: tasks
                .iter()
                .skip(start)
                .take(ROWS)
                .map(|(t, on)| (Task::title_of(t), *on))
                .collect(),
            sel: sel - start,
            more: tasks.len().saturating_sub(start + ROWS),
            picked: tasks.iter().filter(|t| t.1).count(),
            total: tasks.len(),
        }
    }

    pub(super) fn queue_view(
        &self,
        pid: &str,
        sel: usize,
        input: &Option<(String, u8)>,
        confirm: bool,
    ) -> QueueView {
        let tasks = self.project_tasks(pid);
        let sel = sel.min(tasks.len().saturating_sub(1));
        let start = (sel + 1).saturating_sub(ROWS);
        let t0 = now();
        let mut line = 0;
        let rows = tasks
            .iter()
            .map(|t| {
                let detail = match t.state {
                    TaskState::Pending => {
                        line += 1;
                        format!("waiting · #{line} in line")
                    }
                    TaskState::Running => self.running_detail(t),
                    TaskState::Done if t.note.is_empty() => "done".into(),
                    _ => t.note.clone(),
                };
                let at = t.ended.or(t.started).unwrap_or(t.created);
                QueueRow {
                    state: t.state,
                    title: t.title.clone(),
                    attempts: t.attempts,
                    detail,
                    age: age(t0.saturating_sub(at)),
                }
            })
            .skip(start)
            .take(ROWS)
            .collect();
        let path = self.st.project(pid).map(|p| p.path.clone());
        QueueView {
            project: self
                .st
                .project(pid)
                .map(|p| p.name.clone())
                .unwrap_or_default(),
            rows,
            sel: sel - start,
            more: tasks.len().saturating_sub(start + ROWS),
            limit: path
                .map(|p| queue::max_parallel(&p))
                .unwrap_or(queue::DEFAULT_PARALLEL),
            input: input.clone(),
            confirm,
        }
    }

    fn running_detail(&self, t: &Task) -> String {
        if let Some(step) = t.plan.get(t.at) {
            return format!("step {} of {}: {}", t.at + 1, t.plan.len(), step.label());
        }
        let live: Vec<_> = t
            .sessions
            .iter()
            .filter_map(|s| self.st.session(s))
            .collect();
        if t.attempts > 1 {
            let done = live.iter().filter(|s| s.status == Status::Idle).count();
            return format!("{done} of {} attempts done", live.len());
        }
        match live.first() {
            Some(s) if s.status == Status::Waiting => "needs you".into(),
            Some(s) if self.check_progress(&s.id) == Progress::Busy => "running checks".into(),
            Some(_) => "working".into(),
            None => "starting".into(),
        }
    }

    pub(super) fn compare_view(
        &self,
        task: &str,
        sel: usize,
        stats: &[Stat],
        confirm: bool,
    ) -> CompareView {
        let t = self.st.tasks.iter().find(|t| t.id == task);
        let started = t.and_then(|t| t.started).unwrap_or(0);
        let rows = stats
            .iter()
            .filter_map(|st| {
                let s = self.st.session(&st.sid)?;
                let tokens = self
                    .tokens
                    .get(&st.sid)
                    .map(|(_, _, t)| {
                        let (i, o) = t.total();
                        short_count(i + o)
                    })
                    .unwrap_or_else(|| "–".into());
                Some(CompareRow {
                    name: s.name.clone(),
                    status: s.status,
                    check: self.check_badge(&st.sid),
                    files: st.files,
                    adds: st.adds,
                    dels: st.dels,
                    tokens,
                    time: if s.status == Status::Running {
                        "…".into()
                    } else {
                        age(s.since.saturating_sub(started))
                    },
                })
            })
            .collect();
        CompareView {
            title: t.map(|t| t.title.clone()).unwrap_or_default(),
            rows,
            sel,
            confirm,
        }
    }
}
