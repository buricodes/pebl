use super::{App, Place};
use crate::backend::{SessionBackend, UiBackend};
use crate::model::Status;
use crate::review;
use anyhow::{anyhow, bail, Result};

fn reviewer_prompt(task: &str, diff: &str) -> String {
    format!(
        "You are reviewing another agent's work in this folder. Don't change, create or delete any files, and don't commit.\n\nThe task it was given:\n\n{task}\n\nSee its changes with `{diff}` and `git status` (read new files directly). Check that it does what the task asks, and look for bugs, missed edge cases, risky changes and missing tests.\n\nEnd your reply with your findings in a fenced block marked review, one per line as `path:line: what is wrong and what to do`, using line numbers from the new version of the file:\n\n```review\nsrc/example.ts:42: this returns undefined when the list is empty; return [] instead\n```\n\nIf nothing needs changing, end with an empty review block."
    )
}

impl<B: SessionBackend + UiBackend> App<B> {
    pub(super) fn start_reviewer(&mut self, author: &str) -> Result<()> {
        let s = self
            .st
            .session(author)
            .cloned()
            .ok_or_else(|| anyhow!("no such session"))?;
        if !crate::git::is_repo(&s.cwd) {
            bail!("reviews need a git repository to diff against");
        }
        if self.reviewers.values().any(|a| a == author) {
            bail!("a reviewer is already looking at {}", s.name);
        }
        let task = self
            .st
            .tasks
            .iter()
            .find(|t| t.sessions.iter().any(|x| x == author))
            .map(|t| t.prompt.clone())
            .or_else(|| self.last_prompt.get(author).cloned())
            .unwrap_or_else(|| {
                "(not recorded: work it out from the diff and the commit messages)".into()
            });
        let diff = crate::diff::base(&s.cwd)
            .map(|(rev, _)| format!("git diff {rev}"))
            .unwrap_or_else(|_| "git diff HEAD".into());
        let name = format!("review-{}", s.name);
        let rsid = self.create_session(
            &s.project,
            &name,
            &reviewer_prompt(&task, &diff),
            Place::In(s.cwd.clone()),
        )?;
        self.reviewers.insert(rsid, author.to_string());
        self.flash(format!(
            "{name} is reviewing; its comments open with d on {}",
            s.name
        ));
        Ok(())
    }

    pub(super) fn collect_reviews(&mut self) {
        let st = &self.st;
        self.reviewers
            .retain(|r, a| st.session(r).is_some() && st.session(a).is_some());
        let ready: Vec<(String, String)> = self
            .reviewers
            .iter()
            .filter(|(r, _)| self.st.session(r).is_some_and(|s| s.status == Status::Idle))
            .map(|(r, a)| (r.clone(), a.clone()))
            .collect();
        for (r, author) in ready {
            let text = self
                .tokens
                .get(&r)
                .map(|(_, _, t)| t.last_text.clone())
                .unwrap_or_default();
            let Some(found) = review::parse_findings(&text) else {
                continue;
            };
            self.reviewers.remove(&r);
            let name = self
                .st
                .session(&author)
                .map(|s| s.name.clone())
                .unwrap_or_default();
            if found.is_empty() {
                self.flash(format!("the reviewer found nothing to change in {name}"));
                continue;
            }
            match review::save_findings(&self.env.reviews, &author, &found) {
                Ok(()) => self.flash(format!(
                    "review of {name}: {} comments · d on {name} to edit and send them",
                    found.len()
                )),
                Err(e) => self.flash(format!("saving the review: {e:#}")),
            }
        }
    }
}
