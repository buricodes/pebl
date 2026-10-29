use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::path::Path;

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Config {
    pub keys: Keys,
    pub terminal: Option<String>,
    pub background: Option<String>,
    pub editor: Option<String>,
    pub mascot: Option<String>,
    pub notify: Notify,
    pub statusline: Option<bool>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Notify {
    pub desktop: bool,
    pub chime: bool,
    pub chime_waiting: String,
    pub chime_done: String,
    pub chime_error: String,
    pub phone: String,
    pub phone_after: u64,
}

impl Default for Notify {
    fn default() -> Self {
        Notify {
            desktop: true,
            chime: true,
            chime_waiting: "Glass".into(),
            chime_done: "Pop".into(),
            chime_error: "Basso".into(),
            phone: String::new(),
            phone_after: 120,
        }
    }
}

impl Notify {
    pub fn sound_for(&self, kind: crate::view::NoticeKind) -> Option<String> {
        use crate::view::NoticeKind::*;
        if !self.chime {
            return None;
        }
        let s = match kind {
            Waiting => &self.chime_waiting,
            Done => &self.chime_done,
            Error => &self.chime_error,
            _ => return None,
        };
        (!s.is_empty()).then(|| s.clone())
    }
}

impl Config {
    pub fn mascot(&self) -> Option<String> {
        match self.mascot.as_deref() {
            Some("off") | Some("") => None,
            Some(m) if crate::ui::mascot::by_name(m).is_some() => Some(m.to_string()),
            _ => Some(crate::ui::mascot::DEFAULT.into()),
        }
    }

    pub fn background(&self) -> Option<&str> {
        match self.background.as_deref() {
            Some("default") | Some("") => None,
            Some(c) => Some(c),
            None => None,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Keys {
    pub next: String,
    pub prev: String,
    pub waiting: String,
    pub new_session: String,
    pub new_project: String,
    pub sidebar: String,
    pub focus: String,
    pub quit: String,
    pub split: String,
    pub close_column: String,
    pub search: String,
    pub mark: String,
    pub shell: String,
}

impl Default for Keys {
    fn default() -> Self {
        Keys {
            next: "C-j".into(),
            prev: "C-k".into(),
            waiting: "C-g".into(),
            new_session: "C-n".into(),
            new_project: "C-o".into(),
            sidebar: "C-b".into(),
            focus: "C-Space".into(),
            quit: "C-q".into(),
            split: "C-\\".into(),
            close_column: "C-w".into(),
            search: "C-f".into(),
            mark: "C-x".into(),
            shell: "C-t".into(),
        }
    }
}

impl Keys {
    pub fn bindings(&self) -> Vec<(&str, &'static str)> {
        [
            (self.next.as_str(), "next"),
            (self.prev.as_str(), "prev"),
            (self.waiting.as_str(), "waiting"),
            (self.new_session.as_str(), "new-session"),
            (self.new_project.as_str(), "new-project"),
            (self.sidebar.as_str(), "toggle-sidebar"),
            (self.focus.as_str(), "toggle-focus"),
            (self.quit.as_str(), "quit"),
            (self.split.as_str(), "split"),
            (self.search.as_str(), "search"),
            (self.mark.as_str(), "mark"),
            (self.shell.as_str(), "shell"),
        ]
        .into_iter()
        .filter(|(k, _)| !k.is_empty())
        .collect()
    }
}

pub fn load(path: &Path) -> Result<Config> {
    match std::fs::read_to_string(path) {
        Ok(s) => toml::from_str(&s).with_context(|| format!("parsing {}", path.display())),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Config::default()),
        Err(e) => Err(e).with_context(|| format!("reading {}", path.display())),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn partial_override() {
        let c: Config = toml::from_str("[keys]\nfocus = \"C-]\"\n").unwrap();
        assert_eq!(c.keys.focus, "C-]");
        assert_eq!(c.keys.next, "C-j");
    }
}
