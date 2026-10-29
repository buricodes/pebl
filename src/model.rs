use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Status {
    Running,
    Waiting,
    Idle,
    Error,
    Exited,
}

impl Status {
    pub fn is_active(self) -> bool {
        matches!(self, Status::Running | Status::Waiting)
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Project {
    pub id: String,
    pub name: String,
    pub path: PathBuf,
    #[serde(default = "yes")]
    pub open: bool,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Session {
    pub id: String,
    pub project: String,
    pub name: String,
    pub claude_id: String,
    #[serde(default)]
    pub pane: Option<String>,
    pub cwd: PathBuf,
    #[serde(default)]
    pub worktree: Option<PathBuf>,
    pub status: Status,
    pub since: u64,
    #[serde(default)]
    pub exit_code: Option<i32>,
    #[serde(default)]
    pub auto_name: bool,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct State {
    #[serde(default)]
    pub projects: Vec<Project>,
    #[serde(default)]
    pub sessions: Vec<Session>,
    #[serde(default)]
    pub next_id: u64,
}

fn yes() -> bool {
    true
}

pub fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

impl State {
    pub fn fresh_id(&mut self, prefix: &str) -> String {
        self.next_id += 1;
        format!("{prefix}{}", self.next_id)
    }

    pub fn project(&self, id: &str) -> Option<&Project> {
        self.projects.iter().find(|p| p.id == id)
    }

    pub fn session(&self, id: &str) -> Option<&Session> {
        self.sessions.iter().find(|s| s.id == id)
    }

    pub fn session_mut(&mut self, id: &str) -> Option<&mut Session> {
        self.sessions.iter_mut().find(|s| s.id == id)
    }

    pub fn sessions_of<'a>(&'a self, project: &'a str) -> impl Iterator<Item = &'a Session> + 'a {
        self.sessions.iter().filter(move |s| s.project == project)
    }

    pub fn project_by_path(&self, path: &std::path::Path) -> Option<&Project> {
        self.projects.iter().find(|p| p.path == path)
    }

    pub fn add_project(&mut self, path: PathBuf, name: Option<String>) -> String {
        if let Some(p) = self.project_by_path(&path) {
            return p.id.clone();
        }
        let name = name.unwrap_or_else(|| folder_name(&path));
        let id = self.fresh_id("p");
        self.projects.push(Project {
            id: id.clone(),
            name,
            path,
            open: true,
        });
        id
    }

    pub fn set_status(&mut self, sid: &str, status: Status) -> bool {
        match self.session_mut(sid) {
            Some(s) if s.status != status => {
                s.status = status;
                s.since = now();
                true
            }
            _ => false,
        }
    }
}

pub fn folder_name(path: &std::path::Path) -> String {
    path.file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| path.display().to_string())
}

pub fn tilde(path: &std::path::Path) -> String {
    if let Some(home) = dirs::home_dir() {
        if let Ok(rest) = path.strip_prefix(&home) {
            if rest.as_os_str().is_empty() {
                return "~".into();
            }
            return format!("~/{}", rest.display());
        }
    }
    path.display().to_string()
}

pub fn utc_date(secs: u64) -> String {
    let days = (secs / 86_400) as i64;
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = yoe + era * 400 + i64::from(m <= 2);
    format!("{y:04}-{m:02}-{d:02}")
}

pub fn short_count(n: u64) -> String {
    match n {
        0..=999 => n.to_string(),
        1_000..=999_999 => format!("{}k", (n + 500) / 1_000),
        _ => format!("{:.1}M", n as f64 / 1_000_000.0).replace(".0M", "M"),
    }
}

pub fn untilde(s: &str) -> PathBuf {
    if s == "~" {
        return dirs::home_dir().unwrap_or_default();
    }
    if let Some(rest) = s.strip_prefix("~/") {
        if let Some(home) = dirs::home_dir() {
            return home.join(rest);
        }
    }
    PathBuf::from(s)
}

pub fn age(secs: u64) -> String {
    match secs {
        0..=59 => format!("{secs}s"),
        60..=3599 => format!("{}m", secs / 60),
        3600..=86399 => format!("{}h", secs / 3600),
        _ => format!("{}d", secs / 86400),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ages() {
        assert_eq!(age(14), "14s");
        assert_eq!(age(120), "2m");
        assert_eq!(age(3700), "1h");
        assert_eq!(age(3 * 86400), "3d");
    }

    #[test]
    fn add_project_dedupes_by_path() {
        let mut st = State::default();
        let a = st.add_project("/x/nova-api".into(), None);
        let b = st.add_project("/x/nova-api".into(), None);
        assert_eq!(a, b);
        assert_eq!(st.projects[0].name, "nova-api");
    }
}
