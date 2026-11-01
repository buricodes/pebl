use crate::backend::LivePane;
use crate::model::{now, State, Status};
use anyhow::{Context, Result};
use std::path::{Path, PathBuf};

pub fn load(path: &Path) -> Result<State> {
    match std::fs::read_to_string(path) {
        Ok(s) => serde_json::from_str(&s).with_context(|| format!("parsing {}", path.display())),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(State::default()),
        Err(e) => Err(e).with_context(|| format!("reading {}", path.display())),
    }
}

pub fn save(path: &Path, state: &State) -> Result<()> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let tmp = path.with_extension("json.tmp");
    std::fs::write(&tmp, serde_json::to_vec_pretty(state)?)?;
    std::fs::rename(&tmp, path)?;
    Ok(())
}

pub fn reconcile(state: &mut State, live: &[LivePane]) -> bool {
    let mut changed = false;
    for s in &mut state.sessions {
        let found = live
            .iter()
            .find(|p| p.sid.as_deref() == Some(s.id.as_str()) || Some(&p.id) == s.pane.as_ref());
        match found {
            Some(p) => {
                if s.pane.as_ref() != Some(&p.id) {
                    s.pane = Some(p.id.clone());
                    changed = true;
                }
                if !p.dead && s.status == Status::Exited {
                    s.status = Status::Idle;
                    s.since = now();
                    changed = true;
                }
                if p.dead && s.status != Status::Exited && s.status != Status::Error {
                    let code = p.dead_status.unwrap_or(0);
                    s.status = if code == 0 {
                        Status::Exited
                    } else {
                        Status::Error
                    };
                    s.exit_code = Some(code);
                    s.since = now();
                    changed = true;
                }
            }
            None => {
                if s.pane.is_some() || !matches!(s.status, Status::Exited | Status::Error) {
                    s.pane = None;
                    if !matches!(s.status, Status::Exited | Status::Error) {
                        s.status = Status::Exited;
                        s.since = now();
                    }
                    changed = true;
                }
            }
        }
    }
    changed
}

pub fn follow_moves(
    state: &mut State,
    live: &[LivePane],
    exists: impl Fn(&Path) -> bool,
) -> Vec<(PathBuf, PathBuf)> {
    let mut moves: Vec<(PathBuf, PathBuf)> = Vec::new();
    for s in &state.sessions {
        let Some(now_at) = live
            .iter()
            .find(|p| p.sid.as_deref() == Some(s.id.as_str()) && !p.dead)
            .and_then(|p| p.path.clone())
        else {
            continue;
        };
        if now_at != s.cwd
            && !exists(&s.cwd)
            && exists(&now_at)
            && !moves.iter().any(|(from, _)| *from == s.cwd)
        {
            moves.push((s.cwd.clone(), now_at));
        }
    }
    let base = |p: &Path| p.file_name().map(|n| n.to_string_lossy().to_string());
    for (from, to) in &moves {
        for s in state.sessions.iter_mut().filter(|s| s.cwd == *from) {
            s.cwd = to.clone();
        }
        for p in state.projects.iter_mut().filter(|p| p.path == *from) {
            if base(from).as_deref() == Some(p.name.as_str()) {
                p.name = base(to).unwrap_or_else(|| p.name.clone());
            }
            p.path = to.clone();
        }
    }
    moves
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::Session;

    fn sess(id: &str, pane: Option<&str>, status: Status) -> Session {
        Session {
            id: id.into(),
            project: "p1".into(),
            name: id.into(),
            claude_id: "u".into(),
            pane: pane.map(Into::into),
            cwd: "/tmp".into(),
            worktree: None,
            status,
            since: 0,
            exit_code: None,
            auto_name: false,
            reviews: None,
        }
    }

    #[test]
    fn missing_pane_marks_exited() {
        let mut st = State {
            sessions: vec![sess("s1", Some("%1"), Status::Running)],
            ..Default::default()
        };
        assert!(reconcile(&mut st, &[]));
        assert_eq!(st.sessions[0].status, Status::Exited);
        assert_eq!(st.sessions[0].pane, None);
    }

    #[test]
    fn dead_pane_nonzero_is_error() {
        let mut st = State {
            sessions: vec![sess("s1", Some("%1"), Status::Idle)],
            ..Default::default()
        };
        let live = [LivePane {
            id: "%1".into(),
            sid: Some("s1".into()),
            dead: true,
            dead_status: Some(1),
            path: None,
        }];
        reconcile(&mut st, &live);
        assert_eq!(st.sessions[0].status, Status::Error);
        assert_eq!(st.sessions[0].exit_code, Some(1));
    }

    #[test]
    fn adopts_pane_by_session_tag() {
        let mut st = State {
            sessions: vec![sess("s1", None, Status::Exited)],
            ..Default::default()
        };
        let live = [LivePane {
            id: "%9".into(),
            sid: Some("s1".into()),
            dead: false,
            dead_status: None,
            path: None,
        }];
        reconcile(&mut st, &live);
        assert_eq!(st.sessions[0].pane.as_deref(), Some("%9"));
    }

    #[test]
    fn follows_a_renamed_project_folder() {
        let at = |id: &str, cwd: &str| Session {
            cwd: cwd.into(),
            ..sess(id, Some(&format!("%{id}")), Status::Idle)
        };
        let mut st = State {
            projects: vec![crate::model::Project {
                id: "p1".into(),
                name: "scroll".into(),
                path: "/code/scroll".into(),
                open: true,
            }],
            sessions: vec![
                at("s1", "/code/scroll"),
                at("s2", "/code/scroll"),
                at("s3", "/gone"),
            ],
            ..Default::default()
        };
        let pane = |sid: &str, path: &str| LivePane {
            id: format!("%{sid}"),
            sid: Some(sid.into()),
            dead: false,
            dead_status: None,
            path: Some(path.into()),
        };
        let live = [pane("s1", "/code/algo"), pane("s3", "/elsewhere")];
        let exists = |p: &Path| p == Path::new("/code/algo");
        let moves = follow_moves(&mut st, &live, exists);
        assert_eq!(moves, vec![("/code/scroll".into(), "/code/algo".into())]);
        assert_eq!(st.projects[0].name, "algo");
        assert_eq!(st.projects[0].path, PathBuf::from("/code/algo"));
        assert!(st.sessions[..2]
            .iter()
            .all(|s| s.cwd == Path::new("/code/algo")));
        assert_eq!(st.sessions[2].cwd, PathBuf::from("/gone"));
        assert!(follow_moves(&mut st, &live, exists).is_empty());
    }

    #[test]
    fn roundtrip() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("state.json");
        let mut st = State::default();
        st.add_project("/x/a".into(), None);
        save(&p, &st).unwrap();
        assert_eq!(load(&p).unwrap(), st);
    }
}
