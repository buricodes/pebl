use crate::model::Status;
use crate::ui::theme::ColorMode;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct View {
    pub projects: Vec<ProjectRow>,
    pub cursor: Option<Cursor>,
    pub active: Option<Active>,
    pub focus: Focus,
    pub mode: Mode,
    pub layout: Layout,
    pub counts: Counts,
    pub path: String,
    pub stage: Stage,
    pub flash: Option<String>,
    pub bell: u64,
    pub osc: Option<(u64, String)>,
    pub color: ColorMode,
    pub filter: String,
    #[serde(default)]
    pub mascot: Option<String>,
    #[serde(default)]
    pub notice: Notice,
    #[serde(default)]
    pub columns: Vec<ColumnTab>,
    #[serde(default)]
    pub col_focus: usize,
    #[serde(default)]
    pub resizing: Option<usize>,
    #[serde(default)]
    pub usage: Option<Usage>,
    #[serde(default)]
    pub limits: Vec<Limit>,
    #[serde(default)]
    pub today: (u64, u64),
    #[serde(default)]
    pub marking: Option<usize>,
    #[serde(default)]
    pub shell: Option<ShellView>,
    #[serde(default)]
    pub runnable: Option<(usize, usize)>,
    #[serde(default)]
    pub update: Option<String>,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct ShellView {
    pub open: bool,
    pub path: String,
    pub shell: String,
    pub running: Option<String>,
    pub done: Option<(String, bool, String)>,
    pub resize: Option<(u16, u16)>,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Usage {
    pub model: String,
    pub pct: Option<u8>,
    pub used: u64,
    pub size: u64,
    pub tok_in: u64,
    pub tok_out: u64,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Limit {
    pub label: String,
    pub pct: u8,
    pub resets: String,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct ColumnTab {
    pub project: String,
    pub session: String,
    pub status: Option<Status>,
    pub width: u16,
    #[serde(default)]
    pub ctx: Option<u8>,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum NoticeKind {
    #[default]
    Idle,
    Working,
    Waiting,
    Error,
    Done,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Notice {
    pub kind: NoticeKind,
    pub session: String,
    pub detail: String,
    pub count: usize,
    pub more: usize,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct ProjectRow {
    pub name: String,
    pub open: bool,
    pub total: usize,
    pub waiting: usize,
    pub sessions: Vec<SessionRow>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct SessionRow {
    pub name: String,
    pub status: Status,
    pub meta: String,
    #[serde(default)]
    pub col: Option<usize>,
    #[serde(default)]
    pub ctx: Option<u8>,
    #[serde(default)]
    pub marked: Option<bool>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Cursor {
    Project(usize),
    Session(usize, usize),
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Active {
    pub project: String,
    pub session: String,
    pub status: Status,
    pub age: String,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum Focus {
    #[default]
    Sidebar,
    Main,
    Bar,
    Shell,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Layout {
    Full(u16),
    Compact(u16),
    Hidden { narrow: bool },
}

impl Default for Layout {
    fn default() -> Self {
        Layout::Full(30)
    }
}

impl Layout {
    pub fn for_width(width: u16, collapsed: bool) -> Layout {
        if collapsed {
            Layout::Hidden {
                narrow: width < 100,
            }
        } else if width >= 100 {
            Layout::Full((width / 4).clamp(24, 36))
        } else if width >= 72 {
            Layout::Compact(22)
        } else {
            Layout::Hidden { narrow: true }
        }
    }

    pub fn sidebar_width(self) -> Option<u16> {
        match self {
            Layout::Full(w) | Layout::Compact(w) => Some(w),
            Layout::Hidden { .. } => None,
        }
    }

    pub fn short_keys(self) -> bool {
        matches!(self, Layout::Compact(_) | Layout::Hidden { narrow: true })
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Counts {
    pub running: usize,
    pub waiting: usize,
    pub idle: usize,
    pub error: usize,
    pub exited: usize,
}

impl Counts {
    pub fn active(&self) -> usize {
        self.running + self.waiting
    }
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub enum Mode {
    #[default]
    Normal,
    NewSession {
        project: usize,
        text: String,
        prompt: String,
        editing_prompt: bool,
        worktree: bool,
    },
    Rename {
        cursor: Cursor,
        text: String,
    },
    NewProject(NewProject),
    ConfirmKill {
        name: String,
    },
    Filter {
        text: String,
    },
    Peek(Peek),
    Finish(Finish),
    Search(SearchView),
    Prompts(PromptsView),
    Inbox(InboxView),
    Mark {
        count: usize,
    },
    Broadcast(Broadcast),
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Broadcast {
    pub targets: Vec<Target>,
    pub more: usize,
    pub text: String,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Target {
    pub project: String,
    pub session: String,
    pub status: Status,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct SearchView {
    pub query: String,
    pub all: bool,
    pub sel: usize,
    pub rows: Vec<SearchRow>,
    pub more: usize,
    pub matches: usize,
    pub convs: usize,
    pub searched: usize,
    pub outside: usize,
    pub project: String,
    pub projects: usize,
    pub indexing: bool,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct InboxView {
    pub rows: Vec<InboxRow>,
    pub sel: usize,
    pub more: usize,
    pub waiting: usize,
    pub total: usize,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct InboxRow {
    pub kind: NoticeKind,
    pub project: String,
    pub session: String,
    pub detail: String,
    pub age: String,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct PromptsView {
    pub query: String,
    pub sel: usize,
    pub rows: Vec<PromptRow>,
    pub total: usize,
    pub target: String,
    pub clip: Clip,
    pub step: PromptStep,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct PromptRow {
    pub name: String,
    pub preview: String,
    pub clipboard: bool,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub enum Clip {
    #[default]
    Unused,
    Empty,
    Text {
        first: String,
        lines: usize,
    },
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub enum PromptStep {
    #[default]
    List,
    Name {
        text: String,
        from: NameFrom,
    },
    ConfirmDelete,
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub enum NameFrom {
    New,
    Last,
    Typed,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct SearchRow {
    pub project: String,
    pub title: String,
    pub snippet: String,
    pub when: String,
    pub src: String,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Finish {
    pub session: String,
    pub branch: String,
    pub into: String,
    pub ahead: usize,
    pub dirty: usize,
    pub gh: bool,
    pub confirm_discard: bool,
    pub busy: Option<String>,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Peek {
    pub project: String,
    pub session: String,
    pub status: Option<Status>,
    pub lines: Vec<String>,
    pub text: String,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct NewProject {
    pub path: String,
    pub candidates: Vec<Candidate>,
    pub sel: usize,
    pub name: String,
    pub then_session: bool,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Candidate {
    pub path: String,
    pub info: String,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub enum Stage {
    Welcome {
        dir: String,
        history: Vec<(String, usize)>,
    },
    NewSession {
        cwd: String,
    },
    Help,
    Exited {
        name: String,
        code: Option<i32>,
    },
    #[default]
    Empty,
}
