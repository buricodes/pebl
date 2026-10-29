use crate::backend::LivePane;
use crate::model::{now, State, Status};
use anyhow::{Context, Result};
use std::path::Path;

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
        }];
        reconcile(&mut st, &live);
        assert_eq!(st.sessions[0].pane.as_deref(), Some("%9"));
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
