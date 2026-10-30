use crate::model::Task;
use std::path::Path;

pub const DEFAULT_PARALLEL: usize = 2;

pub fn max_parallel(project: &Path) -> usize {
    #[derive(serde::Deserialize, Default)]
    struct Queue {
        max_parallel: Option<usize>,
    }
    #[derive(serde::Deserialize, Default)]
    struct File {
        #[serde(default)]
        queue: Queue,
    }
    std::fs::read_to_string(project.join("pebl.toml"))
        .ok()
        .and_then(|s| toml::from_str::<File>(&s).ok())
        .and_then(|f| f.queue.max_parallel)
        .unwrap_or(DEFAULT_PARALLEL)
        .clamp(1, 8)
}

pub fn auto_review(project: &Path) -> bool {
    #[derive(serde::Deserialize, Default)]
    struct Review {
        auto: Option<bool>,
    }
    #[derive(serde::Deserialize, Default)]
    struct File {
        #[serde(default)]
        review: Review,
    }
    std::fs::read_to_string(project.join("pebl.toml"))
        .ok()
        .and_then(|s| toml::from_str::<File>(&s).ok())
        .and_then(|f| f.review.auto)
        .unwrap_or(false)
}

pub fn from_markdown(text: &str) -> Vec<String> {
    let mut tasks: Vec<String> = Vec::new();
    let mut open = false;
    for line in text.lines() {
        let t = line.trim_start();
        if let Some(rest) = t
            .strip_prefix("- [ ] ")
            .or_else(|| t.strip_prefix("* [ ] "))
        {
            tasks.push(rest.trim().to_string());
            open = true;
        } else if open && line.starts_with([' ', '\t']) && !t.is_empty() && !t.starts_with("- [") {
            if let Some(last) = tasks.last_mut() {
                last.push('\n');
                last.push_str(t);
            }
        } else {
            open = false;
        }
    }
    tasks.retain(|t| !t.trim().is_empty());
    tasks
}

pub fn session_name(task: &Task, attempt: u8) -> String {
    let base: String = crate::prompts::slug(&task.title).chars().take(24).collect();
    let base = base.trim_matches('-');
    let base = if base.is_empty() { "task" } else { base };
    let run = match task.runs {
        0 | 1 => String::new(),
        n => format!("r{n}"),
    };
    match task.attempts {
        1 => format!("{base}-{}{run}", task.id),
        _ => format!("{base}-{}{run}-{attempt}", task.id),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::State;

    #[test]
    fn reads_open_tasks_from_markdown() {
        let md = "# Sprint\n\n- [ ] Fix the rota export\n  only night shifts are missing\n- [x] Done already\n- [ ] Add CSV download\n\nNotes:\n- plain bullet\n";
        assert_eq!(
            from_markdown(md),
            vec![
                "Fix the rota export\nonly night shifts are missing",
                "Add CSV download"
            ]
        );
    }

    #[test]
    fn names_sessions_per_attempt() {
        let mut st = State::default();
        st.add_task("p1", "Fix the rota export for night shifts please", 1);
        st.add_task("p1", "!!!", 3);
        assert_eq!(session_name(&st.tasks[0], 1), "fix-the-rota-export-for-t1");
        assert_eq!(session_name(&st.tasks[1], 2), "task-t2-2");
        st.tasks[0].runs = 2;
        assert_eq!(
            session_name(&st.tasks[0], 1),
            "fix-the-rota-export-for-t1r2"
        );
    }

    #[test]
    fn parallel_limit_comes_from_pebl_toml() {
        let tmp = tempfile::tempdir().unwrap();
        assert_eq!(max_parallel(tmp.path()), DEFAULT_PARALLEL);
        std::fs::write(tmp.path().join("pebl.toml"), "[queue]\nmax_parallel = 3\n").unwrap();
        assert_eq!(max_parallel(tmp.path()), 3);
    }
}
