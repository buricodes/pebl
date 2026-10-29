use crate::backend::{Arrangement, SessionBackend, SpawnSpec, UiBackend, UiPanes};
use crate::ipc::{Input, Key, KeyCode, Role};
use crate::model::{age, now, tilde, untilde, Session, State, Status};
use crate::ui::theme::{ColorMode, Tok};
use crate::view::{
    Active, ColumnTab, Counts, Cursor, Focus, Layout, Limit, Mode, NewProject, Notice, NoticeKind,
    ProjectRow, SessionRow, Stage, Usage, View,
};
use crate::{complete, git, history, paths, status};
use anyhow::{anyhow, bail, Result};
use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::time::{Duration, Instant};

const FLASH_FOR: Duration = Duration::from_secs(4);
const CANDIDATES: usize = 4;
const DONE_FOR: u64 = 15 * 60;
pub const MAX_COLS: usize = 4;
pub const MIN_COL: u16 = 30;
const PEEK_LINES: usize = 10;
const SEARCH_ROWS: usize = 6;

#[derive(Clone, Debug, PartialEq)]
pub enum CurRef {
    Project(String),
    Session(String),
}

#[derive(Clone, Debug, PartialEq)]
enum IMode {
    Normal,
    NewSession {
        pid: String,
        text: String,
        prompt: String,
        editing_prompt: bool,
        worktree: bool,
    },
    Rename {
        target: CurRef,
        text: String,
    },
    NewProject(NewProject),
    ConfirmKill {
        target: CurRef,
    },
    Filter {
        text: String,
    },
    Resize,
    Peek {
        sid: String,
        text: String,
    },
    Finish {
        sid: String,
        info: crate::view::Finish,
    },
    Search {
        query: String,
        all: bool,
        sel: usize,
        scope: Option<PathBuf>,
    },
    Mark {
        marks: Vec<String>,
    },
    Broadcast {
        marks: Vec<String>,
        text: String,
    },
    DrawerResize,
}

#[derive(Clone, Debug, Default)]
struct ShellRun {
    running: Option<String>,
    done: Option<(String, bool, String)>,
}

const SHELLS: [&str; 10] = [
    "zsh", "bash", "fish", "sh", "dash", "ksh", "tcsh", "nu", "login", "xonsh",
];
const MIN_DRAWER: u16 = 6;

#[derive(Clone, Debug)]
pub struct Env {
    pub pebl: String,
    pub claude: String,
    pub settings: PathBuf,
    pub sock: PathBuf,
    pub editor: String,
    pub notify: crate::config::Notify,
}

pub struct App<B: SessionBackend + UiBackend> {
    pub st: State,
    be: B,
    env: Env,
    pub ui: Option<UiPanes>,
    main: Option<String>,
    active: Option<String>,
    cursor: Option<CurRef>,
    focus: Focus,
    mode: IMode,
    filter: String,
    collapsed: bool,
    win: (u16, u16),
    help: bool,
    launch_dir: PathBuf,
    history: Vec<(PathBuf, usize)>,
    flash: Option<(String, Instant)>,
    bell: u64,
    pub color: ColorMode,
    dead: HashSet<String>,
    stale: HashMap<String, (Status, u8)>,
    ticks: u64,
    pub mascot: Option<String>,
    titles: HashMap<String, (PathBuf, u64)>,
    alerts: HashMap<String, (NoticeKind, u64)>,
    last: HashMap<String, Status>,
    tool: HashMap<String, String>,
    muted: bool,
    cols: Vec<String>,
    col: usize,
    col_w: Vec<u16>,
    side_w: Option<u16>,
    peek: Vec<String>,
    peek_from: Focus,
    pending: Vec<Pending>,
    pub outbox: Vec<Outgoing>,
    ctx: HashMap<String, Ctx>,
    hooked: HashSet<String>,
    limits: Vec<(String, f64, u64)>,
    tokens: HashMap<String, (PathBuf, u64, history::Tokens)>,
    index: crate::search::Shared,
    indexing: std::sync::Arc<std::sync::atomic::AtomicBool>,
    found: Option<crate::search::Results>,
    queued: HashMap<String, Vec<String>>,
    pub search_root: Option<PathBuf>,
    shells: HashMap<String, String>,
    drawer: HashSet<String>,
    drawer_h: Option<u16>,
    drawer_shown: Option<String>,
    shell_run: HashMap<String, ShellRun>,
    runnable: HashMap<String, (Vec<String>, usize)>,
    flags: HashMap<&'static str, bool>,
    bar_h: u16,
}

#[derive(Clone, Debug, Default)]
struct Ctx {
    model: String,
    pct: Option<f64>,
    used: u64,
    size: u64,
}

#[derive(Clone, Debug)]
struct Pending {
    sid: String,
    kind: NoticeKind,
    at: u64,
    desktop: bool,
    phone: bool,
    chimed: bool,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Channel {
    Desktop,
    Phone,
    Chime,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Outgoing {
    pub title: String,
    pub body: String,
    pub channel: Channel,
}

impl<B: SessionBackend + UiBackend> App<B> {
    pub fn new(st: State, be: B, env: Env) -> Self {
        let mut app = App {
            st,
            be,
            env,
            ui: None,
            main: None,
            active: None,
            cursor: None,
            focus: Focus::Sidebar,
            mode: IMode::Normal,
            filter: String::new(),
            collapsed: false,
            win: (120, 40),
            help: false,
            launch_dir: std::env::current_dir().unwrap_or_default(),
            history: Vec::new(),
            flash: None,
            bell: 0,
            color: ColorMode::Truecolor,
            dead: HashSet::new(),
            stale: HashMap::new(),
            titles: HashMap::new(),
            mascot: None,
            ticks: 0,
            alerts: HashMap::new(),
            last: HashMap::new(),
            tool: HashMap::new(),
            muted: false,
            cols: Vec::new(),
            col: 0,
            col_w: Vec::new(),
            side_w: None,
            peek: Vec::new(),
            peek_from: Focus::Sidebar,
            pending: Vec::new(),
            outbox: Vec::new(),
            ctx: HashMap::new(),
            hooked: HashSet::new(),
            limits: Vec::new(),
            tokens: HashMap::new(),
            index: Default::default(),
            indexing: Default::default(),
            found: None,
            queued: HashMap::new(),
            search_root: None,
            shells: HashMap::new(),
            drawer: HashSet::new(),
            drawer_h: None,
            drawer_shown: None,
            shell_run: HashMap::new(),
            runnable: HashMap::new(),
            flags: HashMap::new(),
            bar_h: 1,
        };
        app.hooked = app.st.sessions.iter().map(|s| s.id.clone()).collect();
        if let Ok(list) = app.be.shells() {
            for sp in list {
                app.shells.insert(sp.sid, sp.pane);
            }
        }
        app.adopt_ui();
        app.refresh_panes();
        if let Ok(ws) = app.be.widths() {
            let main = app.main.clone();
            for (pane, w) in ws {
                if Some(&pane) == main.as_ref() {
                    app.col_w.insert(0, w);
                    continue;
                }
                if let Some(s) = app
                    .st
                    .sessions
                    .iter()
                    .find(|s| s.pane.as_ref() == Some(&pane))
                {
                    app.cols.push(s.id.clone());
                    app.col_w.push(w);
                }
            }
            app.cols.truncate(MAX_COLS - 1);
            app.style_split();
        }
        let in_slot = app
            .st
            .sessions
            .iter()
            .find(|s| s.pane.is_some() && s.pane == app.main);
        if let Some(s) = in_slot.or_else(|| app.st.sessions.iter().find(|s| s.pane.is_some())) {
            let id = s.id.clone();
            app.activate(&id);
        }
        app
    }

    fn adopt_ui(&mut self) {
        if let Ok(Some(ui)) = self.be.ui_panes() {
            if let Ok(win) = self.be.window_size() {
                self.win = win;
            }
            self.main = self.find_main(&ui);
            self.ui = Some(ui);
        }
    }

    fn find_main(&self, ui: &UiPanes) -> Option<String> {
        self.be
            .slot()
            .ok()
            .flatten()
            .or_else(|| Some(ui.stage.clone()))
    }

    pub fn handle_ctl(&mut self, args: &[String]) -> String {
        let a: Vec<&str> = args.iter().map(String::as_str).collect();
        let res = match a.as_slice() {
            ["ensure-ui", cwd, w, h, color] => {
                self.ensure_ui(cwd, w, h, color).map(|_| "ok".into())
            }
            ["launched", cwd] => {
                self.launched(PathBuf::from(cwd));
                Ok("ok".into())
            }
            ["resized"] => {
                if let Ok(win) = self.be.window_size() {
                    self.win = win;
                }
                self.apply_layout();
                Ok(String::new())
            }
            ["dragged"] => {
                self.dragged();
                Ok(String::new())
            }
            ["focused", pane] => {
                self.focused(pane);
                Ok(String::new())
            }
            ["pane-died", ..] => {
                self.refresh_panes();
                Ok(String::new())
            }
            ["next"] => self.cycle(1).map(|_| String::new()),
            ["prev"] => self.cycle(-1).map(|_| String::new()),
            ["waiting"] => self.jump_waiting().map(|_| String::new()),
            ["new-session"] => self.start_new_session(false).map(|_| String::new()),
            ["new-project"] => {
                self.start_new_project();
                Ok(String::new())
            }
            ["toggle-sidebar"] => {
                self.collapsed = !self.collapsed;
                if self.collapsed && self.focus == Focus::Sidebar {
                    self.set_focus(Focus::Main);
                }
                self.apply_layout();
                Ok(String::new())
            }
            ["toggle-focus"] => {
                let f = if self.focus == Focus::Sidebar {
                    Focus::Main
                } else {
                    Focus::Sidebar
                };
                if f == Focus::Sidebar && self.collapsed {
                    self.collapsed = false;
                    self.apply_layout();
                }
                self.set_focus(f);
                Ok(String::new())
            }
            ["add", path, rest @ ..] => {
                let p = std::fs::canonicalize(untilde(path)).map_err(|e| anyhow!("{path}: {e}"));
                p.map(|p| {
                    let id = self.st.add_project(p, rest.first().map(|s| s.to_string()));
                    self.cursor = Some(CurRef::Project(id.clone()));
                    id
                })
            }
            ["new", project, name, prompt] => self.new_from_cli(project, name, prompt),
            ["resume", sid] => self.resume(sid).map(|_| "ok".into()),
            ["ls"] => Ok(self.ls()),
            ["split"] => self.open_column().map(|_| String::new()),
            ["reply", sid, text] => {
                let s = self.st.session(sid).cloned();
                let s = s.ok_or_else(|| anyhow!("no such session"));
                s.and_then(|s| {
                    let pane = s
                        .pane
                        .clone()
                        .ok_or_else(|| anyhow!("{} isn't running", s.name))?;
                    self.be.send_text(&pane, text, true)?;
                    self.flash(format!("review sent to {}", s.name));
                    Ok("ok".to_string())
                })
            }
            ["review", sid] => self.review(sid).map(|_| String::new()),
            ["finish", sid] => self.start_finish(sid).map(|_| String::new()),
            ["shell"] => self.toggle_shell().map(|_| String::new()),
            ["run-next"] => self.run_next().map(|_| String::new()),
            ["shell-to-claude"] => self.shell_to_claude().map(|_| String::new()),
            ["drawer-resize", dir] => {
                if self.drawer_shown.is_none() {
                    Err(anyhow!("open the shell first (ctrl+t)"))
                } else {
                    self.mode = IMode::DrawerResize;
                    self.set_focus(Focus::Bar);
                    self.resize_drawer(if *dir == "up" { 1 } else { -1 });
                    Ok(String::new())
                }
            }
            ["search"] => {
                self.start_search();
                Ok(String::new())
            }
            ["mark"] => {
                self.cancel_mode();
                if self.collapsed {
                    self.collapsed = false;
                    self.apply_layout();
                }
                self.mode = IMode::Mark { marks: Vec::new() };
                self.set_focus(Focus::Sidebar);
                Ok(String::new())
            }
            ["search-ready"] => {
                self.run_search();
                Ok(String::new())
            }
            ["flash", msg] => {
                if let IMode::Finish { info, .. } = &mut self.mode {
                    info.busy = None;
                }
                self.flash(msg.to_string());
                Ok(String::new())
            }
            ["close-column"] => {
                self.close_column();
                Ok(String::new())
            }
            ["column", n] => {
                self.focus_column(n.parse::<usize>().unwrap_or(1).saturating_sub(1));
                Ok(String::new())
            }
            ["resize", dir] => {
                if self.cols.is_empty() {
                    Err(anyhow!("resizing needs two columns; ctrl+\\ opens one"))
                } else {
                    self.mode = IMode::Resize;
                    self.set_focus(Focus::Bar);
                    self.resize_col(if *dir == "left" { -1 } else { 1 });
                    Ok(String::new())
                }
            }
            ["mascot", name] => {
                self.mascot = (*name != "off").then(|| name.to_string());
                Ok(String::new())
            }
            ["view"] => Ok(serde_json::to_string_pretty(&self.view()).unwrap_or_default()),
            _ => Err(anyhow!("unknown command: {}", a.join(" "))),
        };
        self.sync_main();
        match res {
            Ok(s) => s,
            Err(e) => {
                self.flash(format!("{e:#}"));
                format!("error: {e:#}")
            }
        }
    }

    fn statusline(&mut self, sid: &str, p: &serde_json::Value) {
        let num = |ptr: &str| p.pointer(ptr).and_then(|v| v.as_f64());
        let c = self.ctx.entry(sid.to_string()).or_default();
        if let Some(m) = p.pointer("/model/display_name").and_then(|v| v.as_str()) {
            c.model = m.to_string();
        }
        c.pct = num("/context_window/used_percentage");
        if let Some(u) = num("/context_window/total_input_tokens") {
            c.used = u as u64;
        }
        if let Some(s) = num("/context_window/context_window_size") {
            c.size = s as u64;
        }
        let mut limits = Vec::new();
        for (key, label) in [
            ("five_hour", "5h"),
            ("seven_day", "week"),
            ("spend_limit", "limit"),
        ] {
            if let (Some(pct), Some(at)) = (
                num(&format!("/rate_limits/{key}/used_percentage")),
                num(&format!("/rate_limits/{key}/resets_at")),
            ) {
                limits.push((label.to_string(), pct, at as u64));
            }
        }
        if !limits.is_empty() {
            self.limits = limits;
        }
    }

    fn follow_session_id(&mut self, sid: &str, payload: &serde_json::Value) {
        let Some(id) = payload.get("session_id").and_then(|v| v.as_str()) else {
            return;
        };
        if id.is_empty() {
            return;
        }
        let Some(s) = self.st.session_mut(sid) else {
            return;
        };
        if s.claude_id == id {
            return;
        }
        s.claude_id = id.to_string();
        self.titles.remove(sid);
        self.tokens.remove(sid);
    }

    pub fn load_statuslines(&mut self, dir: &std::path::Path) {
        let ids: Vec<String> = self.st.sessions.iter().map(|s| s.id.clone()).collect();
        for sid in ids {
            let Ok(text) = std::fs::read_to_string(dir.join(format!("{sid}.json"))) else {
                continue;
            };
            if let Ok(v) = serde_json::from_str::<serde_json::Value>(&text) {
                self.statusline(&sid, &v);
            }
        }
        if let Ok(entries) = std::fs::read_dir(dir) {
            for e in entries.flatten() {
                let name = e.file_name().to_string_lossy().to_string();
                let sid = name.trim_end_matches(".json");
                if self.st.session(sid).is_none() {
                    let _ = std::fs::remove_file(e.path());
                }
            }
        }
    }

    fn follow_tokens(&mut self) {
        let ids: Vec<(String, String)> = self
            .st
            .sessions
            .iter()
            .map(|s| (s.id.clone(), s.claude_id.clone()))
            .collect();
        self.tokens
            .retain(|sid, _| ids.iter().any(|(s, _)| s == sid));
        for (sid, cid) in ids {
            if !self.tokens.contains_key(&sid) {
                let Some(p) = history::transcript(&cid) else {
                    continue;
                };
                self.tokens
                    .insert(sid.clone(), (p, 0, history::Tokens::default()));
            }
            if let Some((path, off, t)) = self.tokens.get_mut(&sid) {
                *off = history::read_tokens(path, *off, t);
                let lines = history::runnable(&t.last_text);
                if self.runnable.get(&sid).map(|(l, _)| l) != Some(&lines) {
                    self.runnable.insert(sid.clone(), (lines, 0));
                }
            }
        }
    }

    pub fn handle_hook(&mut self, sid: &str, event: &str, payload: &serde_json::Value) {
        self.hooked.insert(sid.to_string());
        self.follow_session_id(sid, payload);
        if event == "StatusLine" {
            self.statusline(sid, payload);
            return;
        }
        let Some(cur) = self.st.session(sid).map(|s| s.status) else {
            return;
        };
        match event {
            "PreToolUse" | "PermissionRequest" => {
                if let Some(t) = payload.get("tool_name").and_then(|t| t.as_str()) {
                    self.tool.insert(sid.into(), t.into());
                }
            }
            "UserPromptSubmit" => {
                self.tool.insert(sid.into(), "thinking".into());
            }
            "PostToolUse" => {
                self.tool.insert(sid.into(), "thinking".into());
            }
            "Stop" | "StopFailure" | "SessionEnd" => {
                self.tool.remove(sid);
            }
            _ => {}
        }
        if let Some(new) = status::from_hook(event, payload, cur) {
            if self.st.set_status(sid, new) {
                self.stale.remove(sid);
                if new == Status::Waiting && !self.watching(sid) && !self.muted {
                    self.bell += 1;
                }
            }
        }
        if event == "SessionEnd" {
            self.refresh_panes();
        }
        self.sync_main();
    }

    pub fn handle_input(&mut self, role: Role, input: Input) {
        let res = match input {
            Input::Key(k) => self.key(role, k),
            Input::Click { x, .. } if role == Role::Tabs => {
                self.click_tab(x);
                Ok(())
            }
            Input::Click { x: _, y } => self.click(y),
        };
        if let Err(e) = res {
            self.flash(format!("{e:#}"));
        }
        self.sync_main();
    }

    pub fn tick(&mut self) {
        self.ticks += 1;
        if self
            .flash
            .as_ref()
            .is_some_and(|(_, t)| t.elapsed() > FLASH_FOR)
        {
            self.flash = None;
        }
        if self.ticks.is_multiple_of(2) {
            self.refresh_panes();
            self.check_ui();
        }
        self.refresh_peek();
        self.refresh_shells();
        self.deliver_queued();
        self.notify();
        if self.ticks.is_multiple_of(3) {
            self.follow_titles();
            self.follow_tokens();
            self.check_screens();
        }
        self.sync_main();
    }

    fn flash(&mut self, msg: impl Into<String>) {
        self.flash = Some((msg.into(), Instant::now()));
    }

    fn ensure_ui(&mut self, cwd: &str, w: &str, h: &str, color: &str) -> Result<()> {
        self.color = if color == "256" {
            ColorMode::Indexed
        } else {
            ColorMode::Truecolor
        };
        self.launch_dir = PathBuf::from(cwd);
        if self.ui.is_none() {
            self.adopt_ui();
        }
        if self.ui.is_none() {
            let size = (w.parse().unwrap_or(120), h.parse().unwrap_or(40));
            self.win = size;
            let sw = Layout::for_width(size.0, false)
                .sidebar_width()
                .unwrap_or(30);
            let ui = self.be.create_ui(&self.env.pebl, size, sw)?;
            self.main = Some(ui.stage.clone());
            self.ui = Some(ui);
            self.apply_layout();
            let f = if self.st.projects.is_empty() {
                Focus::Main
            } else {
                Focus::Sidebar
            };
            self.set_focus(f);
        }
        self.launched(PathBuf::from(cwd));
        Ok(())
    }

    fn launched(&mut self, cwd: PathBuf) {
        self.launch_dir = cwd.clone();
        if self.st.projects.is_empty() {
            self.history = history::scan();
            self.set_focus(Focus::Main);
            return;
        }
        if let Some(p) = self
            .st
            .projects
            .iter()
            .find(|p| cwd.starts_with(&p.path))
            .cloned()
        {
            let first = self.st.sessions_of(&p.id).next().map(|s| s.id.clone());
            match first {
                Some(sid) => self.activate(&sid),
                None => self.cursor = Some(CurRef::Project(p.id)),
            }
        }
    }

    fn focused(&mut self, pane: &str) {
        let Some(ui) = &self.ui else { return };
        let was = self.focus;
        self.focus = if pane == ui.sidebar {
            Focus::Sidebar
        } else if pane == ui.bar {
            Focus::Bar
        } else {
            Focus::Main
        };
        if self.shells.values().any(|p| p == pane) {
            self.focus = Focus::Shell;
            self.col = 0;
        }
        if self.focus == Focus::Main && !self.cols.is_empty() {
            let col = if Some(pane) == self.main.as_deref() {
                Some(0)
            } else {
                self.col_panes()
                    .iter()
                    .position(|p| p == pane)
                    .map(|i| i + 1)
            };
            if let Some(c) = col {
                self.col = c;
                if let Some(sid) = self.focused_sid() {
                    self.cursor = Some(CurRef::Session(sid));
                }
            }
        }
        if was != self.focus && !self.cols.is_empty() {
            self.style_split();
        }
    }

    fn set_focus(&mut self, f: Focus) {
        self.focus = f;
        let Some(ui) = &self.ui else { return };
        let target = match f {
            Focus::Sidebar => Some(ui.sidebar.clone()),
            Focus::Bar => Some(ui.bar.clone()),
            Focus::Main => self.focused_pane(),
            Focus::Shell => self.drawer_shown.clone(),
        };
        if let Some(t) = target {
            let _ = self.be.select(&t);
        }
    }

    fn layout(&self) -> Layout {
        Layout::for_width(self.win.0, self.collapsed)
    }

    fn main_size(&self) -> (u16, u16) {
        let split = !self.cols.is_empty();
        let w = if split {
            self.col_w.first().copied().unwrap_or(self.main_width())
        } else {
            self.main_width()
        };
        let h = self.win.1.saturating_sub(if split { 5 } else { 3 });
        (w.max(20), h.max(5))
    }

    fn sidebar_w(&self) -> Option<u16> {
        let w = self.layout().sidebar_width()?;
        Some(
            self.side_w
                .filter(|s| *s >= 16 && *s <= self.win.0 / 2)
                .unwrap_or(w),
        )
    }

    fn main_width(&self) -> u16 {
        let sw = self.sidebar_w().map(|w| w + 1).unwrap_or(0);
        self.win.0.saturating_sub(sw)
    }

    fn dragged(&mut self) {
        if let Some(d) = self.drawer_shown.clone() {
            if let Ok(hs) = self.be.heights() {
                if let Some((_, h)) = hs.iter().find(|(p, _)| *p == d) {
                    if *h <= 2 {
                        if let Some(sid) = self.active.clone() {
                            self.drawer.remove(&sid);
                        }
                        self.drawer_h = None;
                        self.apply_layout();
                        return;
                    }
                    self.drawer_h = Some(*h);
                }
            }
        }
        let Some(ui) = self.ui.clone() else { return };
        let Ok(ws) = self.be.widths() else { return };
        let get = |p: &str| ws.iter().find(|(x, _)| x == p).map(|(_, w)| *w);
        if self.layout().sidebar_width().is_some() {
            if let Some(w) = get(&ui.sidebar) {
                self.side_w = Some(w);
            }
        }
        let mut cw = Vec::new();
        if let Some(w) = self.main.as_deref().and_then(get) {
            cw.push(w);
        }
        cw.extend(self.col_panes().iter().filter_map(|p| get(p)));
        if cw.len() == self.cols.len() + 1 {
            self.col_w = cw;
        }
    }

    fn col_panes(&self) -> Vec<String> {
        self.cols
            .iter()
            .filter_map(|sid| self.st.session(sid)?.pane.clone())
            .collect()
    }

    fn col_widths(&self, n: usize) -> Vec<u16> {
        if n == 0 {
            return Vec::new();
        }
        let room = self.main_width().saturating_sub(n as u16 - 1);
        if self.col_w.len() == n {
            let sum: u32 = self.col_w.iter().map(|w| *w as u32).sum();
            if sum == room as u32 {
                return self.col_w.clone();
            }
            if sum > 0 {
                let mut out: Vec<u16> = self
                    .col_w
                    .iter()
                    .map(|w| ((*w as u32 * room as u32) / sum) as u16)
                    .collect();
                let used: u16 = out[..n - 1].iter().sum();
                out[n - 1] = room.saturating_sub(used);
                return out;
            }
        }
        let each = room / n as u16;
        let mut out = vec![each; n];
        out[n - 1] = room - each * (n as u16 - 1);
        out
    }

    fn focused_sid(&self) -> Option<String> {
        if self.col > 0 {
            if let Some(s) = self.cols.get(self.col - 1) {
                return Some(s.clone());
            }
        }
        self.active.clone()
    }

    fn focused_pane(&self) -> Option<String> {
        if self.col > 0 {
            if let Some(p) = self.col_panes().get(self.col - 1) {
                return Some(p.clone());
            }
        }
        self.main.clone()
    }

    fn apply_layout(&mut self) {
        let Some(ui) = self.ui.clone() else { return };
        let Some(main) = self.main.clone() else {
            return;
        };
        let panes = self.col_panes();
        let tabs = if panes.is_empty() {
            None
        } else {
            match self.be.tabs_pane(&self.env.pebl) {
                Ok(t) => Some(t),
                Err(e) => {
                    self.flash(format!("{e:#}"));
                    None
                }
            }
        };
        let widths = self.col_widths(panes.len() + 1);
        self.col_w = widths.clone();
        let mut cols = vec![(main, widths[0])];
        for (p, w) in panes.iter().zip(widths.iter().skip(1)) {
            cols.push((p.clone(), *w));
        }
        let drawer = self.drawer_pane();
        let a = Arrangement {
            win: self.win,
            sidebar: self.sidebar_w().map(|w| (ui.sidebar.clone(), w)),
            tabs: tabs.filter(|_| cols.len() > 1),
            cols,
            drawer: drawer.clone().map(|p| (p, self.drawer_height())),
            bar: (ui.bar.clone(), {
                self.bar_h = crate::ui::bar::height(&self.view());
                self.bar_h
            }),
        };
        match self.be.arrange(&a) {
            Ok(()) => self.drawer_shown = drawer,
            Err(e) => self.flash(format!("layout: {e:#}")),
        }
        let _ = self.be.size_parked(self.main_size());
        self.style_split();
    }

    fn style_split(&self) {
        let on = !self.cols.is_empty();
        let accent = Tok::Acc.tmux(self.color);
        let lit = on && self.focus == Focus::Main && !matches!(self.mode, IMode::Resize);
        let _ = self.be.set_split(on, lit.then_some(accent.as_str()));
    }

    fn prune_cols(&mut self) -> bool {
        let before = self.cols.clone();
        let active = self.active.clone();
        let mut seen = HashSet::new();
        let st = &self.st;
        self.cols.retain(|sid| {
            Some(sid) != active.as_ref()
                && seen.insert(sid.clone())
                && st.session(sid).is_some_and(|s| s.pane.is_some())
        });
        if self.cols == before {
            return false;
        }
        self.col = self.col.min(self.cols.len());
        self.col_w.clear();
        true
    }

    fn open_column(&mut self) -> Result<()> {
        if self.cols.len() + 1 >= MAX_COLS {
            bail!("already {MAX_COLS} columns; ctrl+w closes one");
        }
        let n = self.cols.len() as u16 + 2;
        if self.main_width() < MIN_COL * n + n - 1 {
            bail!("no room for another column; widen the window or hide the sidebar");
        }
        let visible =
            |sid: &str| self.active.as_deref() == Some(sid) || self.cols.iter().any(|c| c == sid);
        let picked = match &self.cursor {
            Some(CurRef::Session(sid)) if !visible(sid) => Some(sid.clone()),
            _ => None,
        };
        let picked = picked.or_else(|| {
            let mut c: Vec<&Session> = self
                .st
                .sessions
                .iter()
                .filter(|s| s.pane.is_some() && !visible(&s.id))
                .collect();
            c.sort_by_key(|s| {
                let rank = match s.status {
                    Status::Waiting => 0,
                    Status::Running => 1,
                    Status::Idle => 2,
                    _ => 3,
                };
                (rank, std::cmp::Reverse(s.since))
            });
            c.first().map(|s| s.id.clone())
        });
        let Some(sid) = picked else {
            bail!("every session is already on screen");
        };
        if self.active.is_none() {
            self.activate(&sid);
            return Ok(());
        }
        if self.st.session(&sid).and_then(|s| s.pane.clone()).is_none() {
            bail!("that session isn't running");
        }
        self.cancel_mode();
        self.cols.push(sid.clone());
        self.col_w.clear();
        self.col = self.cols.len();
        self.cursor = Some(CurRef::Session(sid));
        self.apply_layout();
        self.set_focus(Focus::Main);
        Ok(())
    }

    fn close_column(&mut self) {
        if self.cols.is_empty() {
            return;
        }
        if self.col == 0 {
            let next = self.cols.remove(0);
            self.active = Some(next);
            self.sync_main();
        } else {
            self.cols.remove(self.col - 1);
            self.col = self.col.min(self.cols.len());
        }
        self.mode = IMode::Normal;
        self.col_w.clear();
        self.apply_layout();
        if let Some(sid) = self.focused_sid() {
            self.cursor = Some(CurRef::Session(sid));
        }
        self.set_focus(Focus::Main);
    }

    fn focus_column(&mut self, n: usize) {
        if n > self.cols.len() {
            return;
        }
        self.col = n;
        if let Some(sid) = self.focused_sid() {
            self.cursor = Some(CurRef::Session(sid));
        }
        self.cancel_mode();
        self.set_focus(Focus::Main);
        self.style_split();
    }

    fn resize_col(&mut self, delta: i32) {
        let n = self.cols.len() + 1;
        if n < 2 {
            return;
        }
        let mut w = self.col_widths(n);
        let d = self.col.min(n - 2);
        let min = MIN_COL as i32;
        let total = w[d] as i32 + w[d + 1] as i32;
        let a = (w[d] as i32 + delta).clamp(min.min(total - min), (total - min).max(min));
        w[d] = a as u16;
        w[d + 1] = (total - a) as u16;
        self.col_w = w;
        self.apply_layout();
    }

    fn resize_divider(&self) -> Option<usize> {
        matches!(self.mode, IMode::Resize).then(|| self.col.min(self.cols.len().saturating_sub(1)))
    }

    fn watching(&self, sid: &str) -> bool {
        self.focus == Focus::Main
            && (self.active.as_deref() == Some(sid) || self.cols.iter().any(|c| c == sid))
    }

    fn track(&mut self) {
        let t = now();
        let mut seen = HashMap::new();
        for s in &self.st.sessions {
            let prev = self.last.get(&s.id).copied();
            seen.insert(s.id.clone(), s.status);
            if prev == Some(s.status) {
                continue;
            }
            let watched = self.focus == Focus::Main
                && (self.active.as_deref() == Some(s.id.as_str()) || self.cols.contains(&s.id));
            let note = |kind| Pending {
                sid: s.id.clone(),
                kind,
                at: t,
                desktop: false,
                phone: false,
                chimed: false,
            };
            match (prev, s.status) {
                (Some(_), Status::Waiting) if !watched => {
                    self.pending.push(note(NoticeKind::Waiting));
                }
                (Some(Status::Running | Status::Waiting), Status::Idle) if !watched => {
                    self.alerts.insert(s.id.clone(), (NoticeKind::Done, t));
                    self.pending.push(note(NoticeKind::Done));
                }
                (Some(_), Status::Error) if !watched => {
                    self.alerts.insert(s.id.clone(), (NoticeKind::Error, t));
                    self.pending.push(note(NoticeKind::Error));
                }
                (_, Status::Running | Status::Waiting) => {
                    self.alerts.remove(&s.id);
                }
                _ => {}
            }
        }
        self.alerts.retain(|sid, (k, at)| {
            seen.contains_key(sid) && (*k != NoticeKind::Done || t.saturating_sub(*at) < DONE_FOR)
        });
        self.tool.retain(|sid, _| seen.contains_key(sid));
        self.last = seen;
    }

    pub fn env_notify(&self) -> crate::config::Notify {
        self.env.notify.clone()
    }

    fn notify(&mut self) {
        let cfg = self.env.notify.clone();
        if self.muted || (!cfg.desktop && !cfg.chime && cfg.phone.is_empty()) {
            self.pending.clear();
            return;
        }
        let t = now();
        let mut out = Vec::new();
        let watched: HashSet<String> = self
            .st
            .sessions
            .iter()
            .filter(|s| self.watching(&s.id))
            .map(|s| s.id.clone())
            .collect();
        let st = &self.st;
        let alerts = &self.alerts;
        self.pending.retain_mut(|p| {
            let Some(s) = st.session(&p.sid) else {
                return false;
            };
            if watched.contains(&p.sid) {
                return false;
            }
            let live = match p.kind {
                NoticeKind::Waiting => s.status == Status::Waiting,
                _ => alerts.contains_key(&p.sid),
            };
            if !live {
                return false;
            }
            let project = st
                .project(&s.project)
                .map(|p| p.name.clone())
                .unwrap_or_default();
            let body = match p.kind {
                NoticeKind::Waiting => format!("{} needs you", s.name),
                NoticeKind::Error => format!("{} stopped with an error", s.name),
                _ => format!("{} finished", s.name),
            };
            let title = format!("pebl · {project}");
            let sound = cfg.sound_for(p.kind);
            if let (Some(sound), false) = (&sound, p.chimed) {
                p.chimed = true;
                out.push(Outgoing {
                    title: sound.clone(),
                    body: String::new(),
                    channel: Channel::Chime,
                });
            }
            if cfg.desktop && !p.desktop {
                p.desktop = true;
                out.push(Outgoing {
                    title: title.clone(),
                    body: body.clone(),
                    channel: Channel::Desktop,
                });
            }
            let phone_due = !cfg.phone.is_empty()
                && p.kind != NoticeKind::Done
                && t.saturating_sub(p.at) >= cfg.phone_after;
            if phone_due && !p.phone {
                p.phone = true;
                out.push(Outgoing {
                    title,
                    body,
                    channel: Channel::Phone,
                });
            }
            let phone_left = !cfg.phone.is_empty() && p.kind != NoticeKind::Done && !p.phone;
            (cfg.desktop && !p.desktop) || phone_left
        });
        self.outbox.extend(out);
    }

    fn notice(&self, counts: &Counts) -> Notice {
        let name = |sid: &str| {
            self.st
                .session(sid)
                .map(|s| s.name.clone())
                .unwrap_or_default()
        };
        let mut waiting: Vec<&Session> = self
            .st
            .sessions
            .iter()
            .filter(|s| s.status == Status::Waiting)
            .collect();
        waiting.sort_by_key(|s| s.since);
        if let Some(s) = waiting.first() {
            let detail = match self.tool.get(&s.id).map(String::as_str) {
                Some("thinking") | None => "waiting for input".to_string(),
                Some(t) => format!("wants to run {t}"),
            };
            return Notice {
                kind: NoticeKind::Waiting,
                session: s.name.clone(),
                detail,
                count: waiting.len(),
                more: waiting.len() - 1,
            };
        }
        let newest = |k: NoticeKind| {
            self.alerts
                .iter()
                .filter(|(_, (kk, _))| *kk == k)
                .max_by_key(|(_, (_, at))| *at)
                .map(|(sid, (_, at))| (sid.clone(), *at))
        };
        if let Some((sid, _)) = newest(NoticeKind::Error) {
            let detail = self
                .st
                .session(&sid)
                .and_then(|s| s.exit_code)
                .map(|c| format!("exit {c}"))
                .unwrap_or_else(|| "stopped with an error".into());
            return Notice {
                kind: NoticeKind::Error,
                session: name(&sid),
                detail,
                count: 1,
                more: 0,
            };
        }
        if let Some((sid, at)) = newest(NoticeKind::Done) {
            return Notice {
                kind: NoticeKind::Done,
                session: name(&sid),
                detail: format!("done · {} ago", age(now().saturating_sub(at))),
                count: 1,
                more: 0,
            };
        }
        if counts.running > 0 {
            let s = self
                .st
                .sessions
                .iter()
                .filter(|s| s.status == Status::Running)
                .max_by_key(|s| s.since);
            let detail = s
                .and_then(|s| self.tool.get(&s.id))
                .map(|t| match t.as_str() {
                    "thinking" => "thinking".to_string(),
                    t => format!("running {t}"),
                })
                .unwrap_or_else(|| "working".into());
            return Notice {
                kind: NoticeKind::Working,
                session: s.map(|s| s.name.clone()).unwrap_or_default(),
                detail,
                count: counts.running,
                more: 0,
            };
        }
        Notice {
            kind: NoticeKind::Idle,
            session: String::new(),
            detail: String::new(),
            count: counts.idle + counts.exited + counts.error,
            more: 0,
        }
    }

    fn sync_main(&mut self) {
        self.sync_flags();
        self.track();
        if self.prune_cols() {
            self.apply_layout();
        }
        let Some(ui) = self.ui.clone() else { return };
        let Some(cur) = self.main.clone() else { return };
        let want = match self.stage_kind() {
            Some(_) => ui.stage.clone(),
            None => match self.active_pane() {
                Some(p) => p,
                None => ui.stage.clone(),
            },
        };
        if want != cur {
            match self.be.show(&want, &cur) {
                Ok(()) => {
                    self.main = Some(want.clone());
                    if self.focus == Focus::Main && self.col == 0 {
                        let _ = self.be.select(&want);
                    }
                }
                Err(e) => self.flash(format!("{e:#}")),
            }
        }
        if self.drawer_pane() != self.drawer_shown {
            self.apply_layout();
        }
    }

    fn active_pane(&self) -> Option<String> {
        let s = self.st.session(self.active.as_deref()?)?;
        let p = s.pane.clone()?;
        (!self.dead.contains(&p)).then_some(p)
    }

    fn stage_kind(&self) -> Option<Stage> {
        if let IMode::NewSession { pid, .. } = &self.mode {
            let cwd = self
                .st
                .project(pid)
                .map(|p| p.path.display().to_string())
                .unwrap_or_default();
            return Some(Stage::NewSession { cwd });
        }
        if self.help {
            return Some(Stage::Help);
        }
        if self.st.projects.is_empty() {
            let history = self.history.iter().map(|(p, n)| (tilde(p), *n)).collect();
            return Some(Stage::Welcome {
                dir: tilde(&self.launch_dir),
                history,
            });
        }
        match self.active.as_deref().and_then(|a| self.st.session(a)) {
            Some(s) if self.active_pane().is_none() => Some(Stage::Exited {
                name: s.name.clone(),
                code: s.exit_code,
            }),
            Some(_) => None,
            None => Some(Stage::Empty),
        }
    }

    fn check_ui(&mut self) {
        let Some(ui) = self.ui.clone() else { return };
        if matches!(self.be.ui_panes(), Ok(Some(ref now)) if *now == ui) {
            return;
        }
        let sw = Layout::for_width(self.win.0, false)
            .sidebar_width()
            .unwrap_or(22);
        match self
            .be
            .repair_ui(&self.env.pebl, self.main.as_deref(), self.win, sw)
        {
            Ok((ui, main)) => {
                self.ui = Some(ui);
                self.main = Some(main);
                self.apply_layout();
                self.set_focus(self.focus);
            }
            Err(e) => self.flash(format!("repairing layout: {e:#}")),
        }
    }

    fn follow_titles(&mut self) {
        let auto: Vec<(String, String)> = self
            .st
            .sessions
            .iter()
            .filter(|s| s.auto_name)
            .map(|s| (s.id.clone(), s.claude_id.clone()))
            .collect();
        for (sid, cid) in auto {
            if !self.titles.contains_key(&sid) {
                match history::transcript(&cid) {
                    Some(p) => {
                        self.titles.insert(sid.clone(), (p, 0));
                    }
                    None => continue,
                }
            }
            let Some((path, off)) = self.titles.get(&sid).cloned() else {
                continue;
            };
            let (title, next) = history::read_title(&path, off);
            self.titles.insert(sid.clone(), (path, next));
            if let (Some(t), Some(s)) = (title, self.st.session_mut(&sid)) {
                s.name = t;
            }
        }
    }

    fn refresh_panes(&mut self) {
        let Ok(live) = self.be.list() else { return };
        crate::store::reconcile(&mut self.st, &live);
        self.dead = live
            .iter()
            .filter(|p| p.dead)
            .map(|p| p.id.clone())
            .collect();
    }

    fn check_screens(&mut self) {
        let cutoff = now().saturating_sub(10);
        let stale: Vec<(String, String, Status)> = self
            .st
            .sessions
            .iter()
            .filter(|s| {
                (s.status.is_active() && s.since <= cutoff)
                    || (!self.hooked.contains(&s.id) && s.since + 3 <= now())
            })
            .filter_map(|s| Some((s.id.clone(), s.pane.clone()?, s.status)))
            .collect();
        for (sid, pane, cur) in stale {
            let Ok(text) = self.be.capture(&pane) else {
                continue;
            };
            let seen = status::from_screen(&text);
            if seen == cur {
                self.stale.remove(&sid);
                continue;
            }
            let e = self.stale.entry(sid.clone()).or_insert((seen, 0));
            if e.0 == seen {
                e.1 += 1;
            } else {
                *e = (seen, 1);
            }
            if e.1 >= 2 {
                self.stale.remove(&sid);
                if seen == Status::Waiting || !self.hooked.contains(&sid) {
                    self.st.set_status(&sid, seen);
                }
            }
        }
    }

    fn spec(&self, s: &Session, argv: Vec<String>) -> SpawnSpec {
        SpawnSpec {
            sid: s.id.clone(),
            cwd: s.cwd.clone(),
            argv,
            env: vec![
                ("PEBL_SESSION_ID".into(), s.id.clone()),
                ("PEBL_SOCK".into(), self.env.sock.display().to_string()),
            ],
            size: self.main_size(),
        }
    }

    fn claude_argv(&self, s: &Session, prompt: &str, resume: bool) -> Vec<String> {
        let mut argv = vec![
            self.env.claude.clone(),
            "--settings".into(),
            self.env.settings.display().to_string(),
        ];
        if resume {
            argv.extend(["--resume".into(), s.claude_id.clone()]);
        } else {
            argv.extend(["--session-id".into(), s.claude_id.clone()]);
            if !s.auto_name {
                argv.extend(["-n".into(), s.name.clone()]);
            }
        }
        if !prompt.is_empty() {
            argv.push(prompt.into());
        }
        argv
    }

    fn spawn_session(
        &mut self,
        pid: &str,
        name: &str,
        prompt: &str,
        worktree: bool,
    ) -> Result<String> {
        let p = self
            .st
            .project(pid)
            .cloned()
            .ok_or_else(|| anyhow!("no such project"))?;
        let auto_name = name.trim().is_empty();
        let name = if auto_name {
            "new session".to_string()
        } else {
            name.trim().to_string()
        };
        let wt = if worktree {
            let dir = if auto_name {
                format!("session-{}", self.st.sessions_of(pid).count() + 1)
            } else {
                name.clone()
            };
            Some(git::add_worktree(
                &p.path,
                &paths::worktrees(),
                &p.name,
                &dir,
            )?)
        } else {
            None
        };
        let sid = self.st.fresh_id("s");
        let mut s = Session {
            id: sid.clone(),
            project: pid.into(),
            name,
            claude_id: uuid::Uuid::new_v4().to_string(),
            pane: None,
            cwd: wt.clone().unwrap_or(p.path.clone()),
            worktree: wt,
            status: Status::Idle,
            since: now(),
            exit_code: None,
            auto_name,
        };
        let mut argv = self.claude_argv(&s, prompt, false);
        if let Some(w) = &s.worktree {
            let cfg = git::setup_for(&p.path);
            git::copy_into(&p.path, w, &cfg.copy);
            if !cfg.setup.is_empty() {
                let q = crate::backend::tmux::sh_quote;
                let claude: Vec<String> = argv.iter().map(|a| q(a)).collect();
                let script = format!(
                    "printf '\\033[2m pebl: running worktree setup from pebl.toml\\033[0m\\n'; {}; exec {}",
                    cfg.setup.join(" && "),
                    claude.join(" ")
                );
                argv = vec!["sh".into(), "-c".into(), script];
            }
        }
        s.pane = Some(self.be.spawn(&self.spec(&s, argv))?);
        if !prompt.is_empty() {
            s.status = Status::Running;
        }
        self.st.sessions.push(s);
        self.activate(&sid);
        Ok(sid)
    }

    fn resume(&mut self, sid: &str) -> Result<()> {
        let s = self
            .st
            .session(sid)
            .cloned()
            .ok_or_else(|| anyhow!("no such session"))?;
        let argv = self.claude_argv(&s, "", history::has_transcript(&s.claude_id));
        let spec = self.spec(&s, argv);
        let pane = match &s.pane {
            Some(p) => {
                self.be.respawn(p, &spec)?;
                p.clone()
            }
            None => self.be.spawn(&spec)?,
        };
        self.dead.remove(&pane);
        if let Some(m) = self.st.session_mut(sid) {
            m.pane = Some(pane);
            m.status = Status::Idle;
            m.since = now();
            m.exit_code = None;
        }
        self.activate(sid);
        Ok(())
    }

    fn kill(&mut self, target: &CurRef) -> Result<()> {
        match target {
            CurRef::Session(sid) => {
                let s = self
                    .st
                    .session(sid)
                    .cloned()
                    .ok_or_else(|| anyhow!("no such session"))?;
                let order = self.session_order();
                let pos = order.iter().position(|x| x == sid);
                if self.active.as_deref() == Some(sid.as_str()) {
                    self.active = None;
                    self.sync_main();
                }
                if let Some(p) = &s.pane {
                    let _ = self.be.kill(p);
                }
                if let Some(p) = self.shells.remove(sid.as_str()) {
                    let _ = self.be.kill(&p);
                }
                self.drawer.remove(sid.as_str());
                self.st.sessions.retain(|x| x.id != *sid);
                let next = pos
                    .and_then(|i| {
                        order
                            .get(i + 1)
                            .or_else(|| i.checked_sub(1).and_then(|j| order.get(j)))
                    })
                    .filter(|n| *n != sid)
                    .cloned();
                match next {
                    Some(n) => self.activate(&n),
                    None => self.cursor = Some(CurRef::Project(s.project.clone())),
                }
                if let Some(w) = s.worktree {
                    self.flash(format!("worktree kept at {}", tilde(&w)));
                }
            }
            CurRef::Project(pid) => {
                if self
                    .st
                    .sessions_of(pid)
                    .any(|s| s.pane.is_some() && !matches!(s.status, Status::Exited))
                {
                    bail!("kill its sessions first");
                }
                self.st.sessions.retain(|s| s.project != *pid);
                self.st.projects.retain(|p| p.id != *pid);
                self.cursor = self
                    .st
                    .projects
                    .first()
                    .map(|p| CurRef::Project(p.id.clone()));
            }
        }
        Ok(())
    }

    fn activate(&mut self, sid: &str) {
        let Some(s) = self.st.session(sid) else {
            return;
        };
        let pid = s.project.clone();
        if let Some(p) = self.st.projects.iter_mut().find(|p| p.id == pid) {
            p.open = true;
        }
        self.cursor = Some(CurRef::Session(sid.into()));
        self.help = false;
        self.alerts.remove(sid);
        if let Some(i) = self.cols.iter().position(|c| c == sid) {
            self.col = i + 1;
            if self.focus == Focus::Main {
                self.set_focus(Focus::Main);
            }
            return;
        }
        self.active = Some(sid.into());
        self.col = 0;
    }

    fn new_from_cli(&mut self, project: &str, name: &str, prompt: &str) -> Result<String> {
        let pid = if project.is_empty() {
            self.current_project()
                .ok_or_else(|| anyhow!("no project; run `pebl add` first"))?
        } else if let Some(p) = self.st.projects.iter().find(|p| p.name == project) {
            p.id.clone()
        } else {
            let path =
                std::fs::canonicalize(untilde(project)).map_err(|e| anyhow!("{project}: {e}"))?;
            self.st.add_project(path, None)
        };
        self.spawn_session(&pid, name, prompt, false)
    }

    fn ls(&self) -> String {
        let mut out = String::new();
        for p in &self.st.projects {
            for s in self.st.sessions_of(&p.id) {
                let st = serde_json::to_value(s.status)
                    .ok()
                    .and_then(|v| v.as_str().map(str::to_string))
                    .unwrap_or_default();
                out.push_str(&format!("{}\t{}\t{}\t{}\n", p.name, s.name, st, meta(s)));
            }
        }
        out
    }

    fn current_project(&self) -> Option<String> {
        match &self.cursor {
            Some(CurRef::Project(p)) => Some(p.clone()),
            Some(CurRef::Session(s)) => self.st.session(s).map(|s| s.project.clone()),
            None => self.st.projects.first().map(|p| p.id.clone()),
        }
    }

    fn session_order(&self) -> Vec<String> {
        self.st
            .projects
            .iter()
            .flat_map(|p| {
                self.st
                    .sessions_of(&p.id)
                    .map(|s| s.id.clone())
                    .collect::<Vec<_>>()
            })
            .collect()
    }

    fn cycle(&mut self, dir: i32) -> Result<()> {
        let order = self.session_order();
        if order.is_empty() {
            return Ok(());
        }
        let cur = self
            .focused_sid()
            .and_then(|a| order.iter().position(|x| *x == a));
        let n = order.len() as i32;
        let i = match cur {
            Some(i) => (i as i32 + dir).rem_euclid(n),
            None if dir > 0 => 0,
            None => n - 1,
        };
        self.cancel_mode();
        self.activate(&order[i as usize].clone());
        Ok(())
    }

    fn jump_waiting(&mut self) -> Result<()> {
        let alert = |k: NoticeKind| {
            self.alerts
                .iter()
                .filter(|(_, (kk, _))| *kk == k)
                .max_by_key(|(_, (_, at))| *at)
                .map(|(sid, _)| sid.clone())
        };
        let oldest = self
            .st
            .sessions
            .iter()
            .filter(|s| s.status == Status::Waiting)
            .min_by_key(|s| s.since)
            .map(|s| s.id.clone())
            .or_else(|| alert(NoticeKind::Error))
            .or_else(|| alert(NoticeKind::Done));
        match oldest {
            Some(sid) => {
                self.cancel_mode();
                self.activate(&sid);
                self.sync_main();
                self.set_focus(Focus::Main);
            }
            None => self.flash("nothing needs you"),
        }
        Ok(())
    }

    fn visible(&self) -> Vec<(usize, Vec<usize>)> {
        let f = match &self.mode {
            IMode::Filter { text } => text.to_lowercase(),
            _ => self.filter.to_lowercase(),
        };
        let mut out = Vec::new();
        for (pi, p) in self.st.projects.iter().enumerate() {
            let all: Vec<usize> = self
                .st
                .sessions
                .iter()
                .enumerate()
                .filter(|(_, s)| s.project == p.id)
                .map(|(i, _)| i)
                .collect();
            if f.is_empty() || p.name.to_lowercase().contains(&f) {
                out.push((pi, all));
                continue;
            }
            let hits: Vec<usize> = all
                .into_iter()
                .filter(|i| self.st.sessions[*i].name.to_lowercase().contains(&f))
                .collect();
            if !hits.is_empty() {
                out.push((pi, hits));
            }
        }
        out
    }

    fn filtering(&self) -> bool {
        !self.filter.is_empty() || matches!(&self.mode, IMode::Filter { text } if !text.is_empty())
    }

    fn rows(&self) -> Vec<CurRef> {
        let mut rows = Vec::new();
        for (pi, ss) in self.visible() {
            let p = &self.st.projects[pi];
            rows.push(CurRef::Project(p.id.clone()));
            if p.open || self.filtering() {
                rows.extend(
                    ss.iter()
                        .map(|i| CurRef::Session(self.st.sessions[*i].id.clone())),
                );
            }
        }
        rows
    }

    fn move_cursor(&mut self, delta: i32) {
        let rows = self.rows();
        if rows.is_empty() {
            return;
        }
        let cur = self
            .cursor
            .as_ref()
            .and_then(|c| rows.iter().position(|r| r == c));
        let i = match cur {
            Some(i) => (i as i32 + delta).clamp(0, rows.len() as i32 - 1) as usize,
            None => 0,
        };
        self.set_cursor(rows[i].clone());
    }

    fn set_cursor(&mut self, c: CurRef) {
        if let CurRef::Session(sid) = &c {
            self.activate(&sid.clone());
        } else {
            self.cursor = Some(c);
        }
    }

    fn toggle_open(&mut self, pid: &str, open: Option<bool>) {
        if let Some(p) = self.st.projects.iter_mut().find(|p| p.id == pid) {
            p.open = open.unwrap_or(!p.open);
        }
    }

    fn cancel_mode(&mut self) {
        if matches!(
            self.mode,
            IMode::NewProject(_)
                | IMode::Peek { .. }
                | IMode::Finish { .. }
                | IMode::Search { .. }
                | IMode::Broadcast { .. }
        ) {
            self.mode = IMode::Normal;
            self.apply_layout();
        }
        self.mode = IMode::Normal;
    }

    fn start_peek(&mut self, sid: &str) -> Result<()> {
        let s = self
            .st
            .session(sid)
            .ok_or_else(|| anyhow!("no such session"))?;
        if s.pane.is_none() {
            bail!("{} isn't running", s.name);
        }
        if !matches!(self.mode, IMode::Peek { .. }) {
            self.peek_from = if self.focus == Focus::Bar {
                Focus::Sidebar
            } else {
                self.focus
            };
        }
        self.mode = IMode::Peek {
            sid: sid.into(),
            text: String::new(),
        };
        self.refresh_peek();
        self.apply_layout();
        self.set_focus(Focus::Bar);
        Ok(())
    }

    fn refresh_peek(&mut self) {
        let IMode::Peek { sid, .. } = &self.mode else {
            return;
        };
        let Some(pane) = self.st.session(sid).and_then(|s| s.pane.clone()) else {
            return;
        };
        let Ok(text) = self.be.capture(&pane) else {
            return;
        };
        let lines: Vec<String> = text.lines().map(|l| l.trim_end().to_string()).collect();
        let end = lines
            .iter()
            .rposition(|l| !l.is_empty())
            .map(|i| i + 1)
            .unwrap_or(0);
        let start = end.saturating_sub(PEEK_LINES);
        self.peek = lines[start..end].to_vec();
    }

    fn peek_send(&mut self, sid: &str, text: &str, key: Option<&str>) -> Result<()> {
        let s = self
            .st
            .session(sid)
            .ok_or_else(|| anyhow!("no such session"))?;
        let pane = s
            .pane
            .clone()
            .ok_or_else(|| anyhow!("{} isn't running", s.name))?;
        let name = s.name.clone();
        match key {
            Some(k) => self.be.send_key(&pane, k)?,
            None => self.be.send_text(&pane, text, true)?,
        }
        self.flash(format!("sent to {name}"));
        self.refresh_peek();
        Ok(())
    }

    fn drawer_pane(&self) -> Option<String> {
        if self.stage_kind().is_some() {
            return None;
        }
        let sid = self.active.as_ref()?;
        if !self.drawer.contains(sid) {
            return None;
        }
        self.shells.get(sid).cloned()
    }

    fn column_rows(&self) -> u16 {
        let bar = self.bar_h;
        let tabs = if self.cols.is_empty() { 0 } else { 2 };
        self.win.1.saturating_sub(bar + 1 + tabs)
    }

    fn drawer_height(&self) -> u16 {
        let rows = self.column_rows();
        let max = rows.saturating_sub(9).max(MIN_DRAWER);
        let default = (rows / 3).max(MIN_DRAWER + 2);
        self.drawer_h.unwrap_or(default).clamp(MIN_DRAWER, max)
    }

    fn ensure_shell(&mut self, sid: &str) -> Result<String> {
        if let Some(p) = self.shells.get(sid) {
            return Ok(p.clone());
        }
        let s = self
            .st
            .session(sid)
            .ok_or_else(|| anyhow!("no such session"))?;
        let w = self.col_w.first().copied().unwrap_or(self.main_width());
        let pane = self
            .be
            .spawn_shell(sid, &s.cwd, (w, self.drawer_height()))?;
        self.shells.insert(sid.to_string(), pane.clone());
        Ok(pane)
    }

    fn open_drawer(&mut self) -> Result<String> {
        let sid = self
            .active
            .clone()
            .ok_or_else(|| anyhow!("open a session first"))?;
        let pane = self.ensure_shell(&sid)?;
        self.help = false;
        self.drawer.insert(sid.clone());
        if let Some(r) = self.shell_run.get_mut(&sid) {
            r.done = None;
        }
        self.col = 0;
        self.apply_layout();
        Ok(pane)
    }

    fn toggle_shell(&mut self) -> Result<()> {
        let sid = self
            .active
            .clone()
            .ok_or_else(|| anyhow!("open a session first"))?;
        let open = self.drawer.contains(&sid) && self.drawer_shown.is_some();
        if !open {
            self.cancel_mode();
            self.open_drawer()?;
            self.set_focus(Focus::Shell);
        } else if self.focus == Focus::Shell {
            self.set_focus(Focus::Main);
        } else {
            self.drawer.remove(&sid);
            self.apply_layout();
            self.set_focus(Focus::Main);
        }
        Ok(())
    }

    fn run_next(&mut self) -> Result<()> {
        let sid = self
            .active
            .clone()
            .ok_or_else(|| anyhow!("open a session first"))?;
        let (lines, idx) = self
            .runnable
            .get(&sid)
            .cloned()
            .filter(|(l, _)| !l.is_empty())
            .ok_or_else(|| anyhow!("Claude's last reply has no commands to run"))?;
        let pane = self.open_drawer()?;
        let line = lines[idx % lines.len()].clone();
        self.be.send_key(&pane, "C-u")?;
        self.be.send_text(&pane, &line, false)?;
        self.runnable
            .insert(sid, (lines.clone(), (idx + 1) % lines.len()));
        self.set_focus(Focus::Shell);
        Ok(())
    }

    fn shell_to_claude(&mut self) -> Result<()> {
        let sid = self
            .active
            .clone()
            .ok_or_else(|| anyhow!("open a session first"))?;
        let shell = self
            .shells
            .get(&sid)
            .cloned()
            .ok_or_else(|| anyhow!("this session has no shell yet (ctrl+t)"))?;
        let claude = self
            .st
            .session(&sid)
            .and_then(|s| s.pane.clone())
            .ok_or_else(|| anyhow!("Claude isn't running in this session"))?;
        let text = self.be.capture(&shell)?;
        let lines: Vec<&str> = text.lines().map(str::trim_end).collect();
        let end = lines
            .iter()
            .rposition(|l| !l.is_empty())
            .map(|i| i + 1)
            .unwrap_or(0);
        let tail = lines[end.saturating_sub(40)..end].join("\n");
        let cmd = self
            .shell_run
            .get(&sid)
            .and_then(|r| r.done.as_ref().map(|d| d.0.clone()))
            .unwrap_or_else(|| "my shell".into());
        let msg = format!("Here's the output of `{cmd}` from my shell:\n```\n{tail}\n```\n");
        self.be.send_text(&claude, &msg, false)?;
        if let Some(r) = self.shell_run.get_mut(&sid) {
            r.done = None;
        }
        self.col = 0;
        self.set_focus(Focus::Main);
        Ok(())
    }

    fn resize_drawer(&mut self, delta: i32) {
        let rows = self.column_rows() as i32;
        let h = (self.drawer_height() as i32 + delta)
            .clamp(MIN_DRAWER as i32, (rows - 9).max(MIN_DRAWER as i32));
        self.drawer_h = Some(h as u16);
        self.apply_layout();
    }

    fn refresh_shells(&mut self) {
        let Ok(list) = self.be.shells() else { return };
        let mut seen = HashMap::new();
        for sp in &list {
            seen.insert(sp.sid.clone(), sp.pane.clone());
        }
        self.shells = seen;
        let shells = self.shells.clone();
        self.drawer.retain(|sid| shells.contains_key(sid));
        self.shell_run.retain(|sid, _| shells.contains_key(sid));
        for sp in list {
            let name = sp.command.trim_start_matches('-');
            let idle = SHELLS.contains(&name) || sp.command.is_empty();
            let was = self.shell_run.get(&sp.sid).and_then(|r| r.running.clone());
            match (idle, was) {
                (false, None) => {
                    let cmd = self.be.child_command(sp.pid).unwrap_or(sp.command.clone());
                    let r = self.shell_run.entry(sp.sid.clone()).or_default();
                    r.running = Some(cmd);
                    r.done = None;
                }
                (true, Some(cmd)) => {
                    let tail = self.be.capture(&sp.pane).unwrap_or_default();
                    let (failed, detail) = classify(&tail);
                    let r = self.shell_run.entry(sp.sid.clone()).or_default();
                    r.running = None;
                    r.done = Some((cmd, failed, detail));
                }
                _ => {}
            }
        }
        if self.drawer_pane() != self.drawer_shown
            || crate::ui::bar::height(&self.view()) != self.bar_h
        {
            self.apply_layout();
        }
    }

    fn sync_flags(&mut self) {
        let sid = self.active.clone().unwrap_or_default();
        let runnable = self.focus != Focus::Sidebar
            && self.runnable.get(&sid).is_some_and(|(l, _)| !l.is_empty());
        let drawer = self.drawer_shown.is_some();
        let shellout = self.shells.contains_key(&sid)
            && (drawer || self.shell_run.get(&sid).is_some_and(|r| r.done.is_some()));
        for (name, on) in [
            ("runnable", runnable),
            ("drawer", drawer),
            ("shellout", shellout),
        ] {
            if self.flags.get(name) != Some(&on) && self.be.set_flag(name, on).is_ok() {
                self.flags.insert(name, on);
            }
        }
    }

    fn markable(&self, sid: &str) -> Result<()> {
        let s = self
            .st
            .session(sid)
            .ok_or_else(|| anyhow!("no such session"))?;
        if s.status == Status::Waiting {
            bail!("{} is waiting for you; answer it first", s.name);
        }
        if !s.pane.as_ref().is_some_and(|p| !self.dead.contains(p)) {
            bail!("{} isn't running", s.name);
        }
        Ok(())
    }

    fn mark_key(&mut self, mut marks: Vec<String>, k: Key) -> Result<()> {
        use KeyCode::*;
        let res = match k.code {
            Esc => {
                self.mode = IMode::Normal;
                return Ok(());
            }
            Char('j') | Down => {
                self.move_cursor(1);
                Ok(())
            }
            Char('k') | Up => {
                self.move_cursor(-1);
                Ok(())
            }
            Char(' ') => match &self.cursor {
                Some(CurRef::Session(sid)) => {
                    let sid = sid.clone();
                    if let Some(i) = marks.iter().position(|m| *m == sid) {
                        marks.remove(i);
                        Ok(())
                    } else {
                        self.markable(&sid).map(|_| marks.push(sid))
                    }
                }
                _ => Err(anyhow!("move to a session to mark it")),
            },
            Char('a') => {
                let pid = self.current_project();
                let ids: Vec<String> = pid
                    .map(|p| self.st.sessions_of(&p).map(|s| s.id.clone()).collect())
                    .unwrap_or_default();
                let ok: Vec<String> = ids
                    .into_iter()
                    .filter(|s| self.markable(s).is_ok())
                    .collect();
                if !ok.is_empty() && ok.iter().all(|s| marks.contains(s)) {
                    marks.retain(|m| !ok.contains(m));
                } else {
                    for s in ok {
                        if !marks.contains(&s) {
                            marks.push(s);
                        }
                    }
                }
                Ok(())
            }
            Enter if marks.is_empty() => Err(anyhow!("mark sessions with space first")),
            Enter => {
                self.mode = IMode::Broadcast {
                    marks,
                    text: String::new(),
                };
                self.apply_layout();
                self.set_focus(Focus::Bar);
                return Ok(());
            }
            _ => Ok(()),
        };
        self.mode = IMode::Mark { marks };
        res
    }

    fn broadcast_key(&mut self, marks: Vec<String>, mut text: String, k: Key) -> Result<()> {
        match k.code {
            KeyCode::Esc => {
                self.mode = IMode::Mark { marks };
                self.apply_layout();
                self.set_focus(Focus::Sidebar);
                return Ok(());
            }
            KeyCode::Char('u') if k.ctrl => text.clear(),
            KeyCode::Enter if !text.trim().is_empty() => {
                let (mut now_n, mut later) = (0, 0);
                for sid in &marks {
                    let Some(s) = self.st.session(sid).cloned() else {
                        continue;
                    };
                    let Some(pane) = s.pane.clone() else { continue };
                    if s.status == Status::Running {
                        self.queued
                            .entry(sid.clone())
                            .or_default()
                            .push(text.trim().to_string());
                        later += 1;
                    } else {
                        self.be.send_text(&pane, text.trim(), true)?;
                        now_n += 1;
                    }
                }
                self.cancel_mode();
                self.set_focus(Focus::Sidebar);
                self.flash(match later {
                    0 => format!("sent to {now_n} sessions"),
                    _ => format!("sent to {now_n}, {later} queued until their turn ends"),
                });
                return Ok(());
            }
            _ => edit(&mut text, &k, false),
        }
        self.mode = IMode::Broadcast { marks, text };
        Ok(())
    }

    fn deliver_queued(&mut self) {
        let ready: Vec<String> = self
            .queued
            .keys()
            .filter(|sid| {
                self.st
                    .session(sid)
                    .is_some_and(|s| s.status == Status::Idle && s.pane.is_some())
            })
            .cloned()
            .collect();
        for sid in ready {
            let Some(msgs) = self.queued.remove(&sid) else {
                continue;
            };
            let Some(pane) = self.st.session(&sid).and_then(|s| s.pane.clone()) else {
                continue;
            };
            let _ = self.be.send_text(&pane, &msgs.join("\n\n"), true);
        }
        self.queued
            .retain(|sid, _| self.st.session(sid).is_some_and(|s| s.pane.is_some()));
    }

    pub fn start_index(&self) {
        use std::sync::atomic::Ordering;
        let Some(root) = self.search_root.clone() else {
            return;
        };
        if self.indexing.swap(true, Ordering::SeqCst) {
            return;
        }
        let shared = self.index.clone();
        let flag = self.indexing.clone();
        let sock = self.env.sock.clone();
        std::thread::spawn(move || {
            crate::search::refresh(&shared, &root);
            flag.store(false, Ordering::SeqCst);
            let _ = crate::ipc::ctl(&sock, &["search-ready".into()]);
        });
    }

    fn start_search(&mut self) {
        let scope = self
            .current_project()
            .and_then(|pid| self.st.project(&pid).map(|p| p.path.clone()));
        if !matches!(self.mode, IMode::Search { .. }) {
            self.cancel_mode();
        }
        self.mode = IMode::Search {
            query: String::new(),
            all: scope.is_none(),
            sel: 0,
            scope,
        };
        self.found = None;
        self.start_index();
        self.apply_layout();
        self.set_focus(Focus::Bar);
    }

    fn run_search(&mut self) {
        let IMode::Search {
            query, all, scope, ..
        } = &self.mode
        else {
            return;
        };
        let scope = if *all { None } else { scope.clone() };
        let known: HashSet<String> = self
            .st
            .sessions
            .iter()
            .map(|s| s.claude_id.clone())
            .collect();
        let r = match self.index.lock() {
            Ok(idx) => {
                crate::search::search(&idx, query, scope.as_deref(), &|id| known.contains(id))
            }
            Err(_) => return,
        };
        self.found = Some(r);
    }

    fn search_key(
        &mut self,
        mut query: String,
        mut all: bool,
        mut sel: usize,
        scope: Option<PathBuf>,
        k: Key,
    ) -> Result<()> {
        let n = self.found.as_ref().map(|r| r.hits.len()).unwrap_or(0);
        match k.code {
            KeyCode::Esc => {
                self.cancel_mode();
                self.set_focus(if self.active.is_some() {
                    Focus::Main
                } else {
                    Focus::Sidebar
                });
                return Ok(());
            }
            KeyCode::Up => sel = sel.saturating_sub(1),
            KeyCode::Down => sel = (sel + 1).min(n.saturating_sub(1)),
            KeyCode::Tab | KeyCode::BackTab => {
                if scope.is_some() {
                    all = !all;
                    sel = 0;
                }
            }
            KeyCode::Enter => {
                let hit = self.found.as_ref().and_then(|r| r.hits.get(sel).cloned());
                let Some(hit) = hit else { return Ok(()) };
                self.cancel_mode();
                return self.open_hit(&hit);
            }
            _ => {
                edit(&mut query, &k, false);
                sel = 0;
            }
        }
        self.mode = IMode::Search {
            query,
            all,
            sel,
            scope,
        };
        self.run_search();
        Ok(())
    }

    fn open_hit(&mut self, hit: &crate::search::Hit) -> Result<()> {
        if let Some(s) = self
            .st
            .sessions
            .iter()
            .find(|s| s.claude_id == hit.id)
            .cloned()
        {
            let live = s.pane.as_ref().is_some_and(|p| !self.dead.contains(p));
            if live {
                self.activate(&s.id);
            } else {
                self.resume(&s.id)?;
            }
            self.sync_main();
            self.set_focus(Focus::Main);
            return Ok(());
        }
        let cwd = hit
            .cwd
            .clone()
            .filter(|c| c.is_dir())
            .ok_or_else(|| anyhow!("that conversation's folder no longer exists"))?;
        let pid = match self.st.projects.iter().find(|p| cwd.starts_with(&p.path)) {
            Some(p) => p.id.clone(),
            None => self.st.add_project(cwd.clone(), None),
        };
        let sid = self.st.fresh_id("s");
        self.st.sessions.push(Session {
            id: sid.clone(),
            project: pid,
            name: hit.title.chars().take(40).collect(),
            claude_id: hit.id.clone(),
            pane: None,
            cwd,
            worktree: None,
            status: Status::Idle,
            since: now(),
            exit_code: None,
            auto_name: false,
        });
        self.resume(&sid)?;
        self.sync_main();
        self.set_focus(Focus::Main);
        Ok(())
    }

    fn search_view(
        &self,
        query: &str,
        all: bool,
        sel: usize,
        scope: &Option<PathBuf>,
    ) -> crate::view::SearchView {
        use crate::view::{SearchRow, SearchView};
        let pname = |cwd: &Option<PathBuf>| {
            let cwd = cwd.as_deref();
            self.st
                .projects
                .iter()
                .find(|p| cwd.is_some_and(|c| c.starts_with(&p.path)))
                .map(|p| p.name.clone())
                .or_else(|| {
                    cwd.and_then(|c| c.file_name())
                        .map(|n| n.to_string_lossy().to_string())
                })
                .unwrap_or_default()
        };
        let mut v = SearchView {
            query: query.into(),
            all,
            projects: self.st.projects.len(),
            project: scope
                .as_ref()
                .and_then(|s| self.st.projects.iter().find(|p| p.path == *s))
                .map(|p| p.name.clone())
                .unwrap_or_default(),
            indexing: self.indexing.load(std::sync::atomic::Ordering::SeqCst),
            ..Default::default()
        };
        let Some(r) = &self.found else { return v };
        v.matches = r.matches;
        v.convs = r.hits.len();
        v.searched = r.searched;
        v.outside = r.outside;
        let start = (sel + 1).saturating_sub(SEARCH_ROWS);
        v.sel = sel - start;
        for h in r.hits.iter().skip(start).take(SEARCH_ROWS) {
            let s = self.st.sessions.iter().find(|s| s.claude_id == h.id);
            let src = match s {
                Some(s) if s.pane.as_ref().is_some_and(|p| !self.dead.contains(p)) => "live",
                Some(_) => "",
                None => "cli",
            };
            v.rows.push(SearchRow {
                project: pname(&h.cwd),
                title: h.title.clone(),
                snippet: h.snippet.clone(),
                when: when(h.mtime),
                src: src.into(),
            });
        }
        v.more = r.hits.len().saturating_sub(start + SEARCH_ROWS);
        v
    }

    fn start_finish(&mut self, sid: &str) -> Result<()> {
        let s = self
            .st
            .session(sid)
            .cloned()
            .ok_or_else(|| anyhow!("no such session"))?;
        let Some(wt) = &s.worktree else {
            bail!("only worktree sessions can be finished; start one with w");
        };
        let repo = self
            .st
            .project(&s.project)
            .map(|p| p.path.clone())
            .ok_or_else(|| anyhow!("no such project"))?;
        let branch = git::branch(wt)?;
        let into = git::branch(&repo)?;
        let info = crate::view::Finish {
            session: s.name.clone(),
            ahead: git::ahead(&repo, &into, &branch),
            dirty: git::dirty(wt),
            branch,
            into,
            gh: git::has_gh(),
            confirm_discard: false,
            busy: None,
        };
        self.mode = IMode::Finish {
            sid: sid.into(),
            info,
        };
        self.apply_layout();
        self.set_focus(Focus::Bar);
        Ok(())
    }

    fn finish_key(&mut self, sid: String, mut info: crate::view::Finish, k: Key) -> Result<()> {
        if info.busy.is_some() {
            if k.code == KeyCode::Esc {
                self.cancel_mode();
                self.set_focus(Focus::Sidebar);
            }
            return Ok(());
        }
        let s = self
            .st
            .session(&sid)
            .cloned()
            .ok_or_else(|| anyhow!("no such session"))?;
        let repo = self
            .st
            .project(&s.project)
            .map(|p| p.path.clone())
            .ok_or_else(|| anyhow!("no such project"))?;
        let wt = s.worktree.clone().unwrap_or_default();
        let discard = matches!(k.code, KeyCode::Char('d'));
        if !discard {
            info.confirm_discard = false;
        }
        match k.code {
            KeyCode::Esc => {
                self.cancel_mode();
                self.set_focus(Focus::Sidebar);
                return Ok(());
            }
            KeyCode::Char('a') => {
                let pane = s
                    .pane
                    .clone()
                    .ok_or_else(|| anyhow!("{} isn't running", s.name))?;
                self.be.send_text(
                    &pane,
                    "Please commit all your changes on this branch with a clear commit message.",
                    true,
                )?;
                self.cancel_mode();
                self.set_focus(Focus::Sidebar);
                self.flash(format!("asked {} to commit", s.name));
                return Ok(());
            }
            KeyCode::Char('m') => {
                if info.dirty > 0 {
                    self.mode = IMode::Finish { sid, info };
                    bail!("the worktree has uncommitted changes; press a to ask Claude to commit");
                }
                if info.ahead == 0 {
                    self.mode = IMode::Finish { sid, info };
                    bail!("nothing to merge: the branch has no new commits");
                }
                git::merge(&repo, &info.branch)?;
                self.cancel_mode();
                self.kill(&CurRef::Session(sid))?;
                git::remove_worktree(&repo, &wt, &info.branch, false)?;
                self.flash(format!(
                    "merged {} into {} and removed the worktree",
                    info.branch, info.into
                ));
                return Ok(());
            }
            KeyCode::Char('p') => {
                if !info.gh {
                    self.mode = IMode::Finish { sid, info };
                    bail!("pull requests need the GitHub CLI: brew install gh, then gh auth login");
                }
                if info.dirty > 0 {
                    self.mode = IMode::Finish { sid, info };
                    bail!("the worktree has uncommitted changes; press a to ask Claude to commit");
                }
                info.busy = Some(format!(
                    "pushing {} and opening a pull request…",
                    info.branch
                ));
                let sock = self.env.sock.clone();
                let branch = info.branch.clone();
                std::thread::spawn(move || {
                    let msg = match git::push_and_pr(&wt, &branch) {
                        Ok(url) => {
                            let _ = std::process::Command::new("open").arg(&url).spawn();
                            format!("pull request: {url}")
                        }
                        Err(e) => format!("{e:#}"),
                    };
                    let _ = crate::ipc::ctl(&sock, &["flash".into(), msg]);
                });
            }
            KeyCode::Char('d') if info.confirm_discard => {
                self.cancel_mode();
                self.kill(&CurRef::Session(sid))?;
                git::remove_worktree(&repo, &wt, &info.branch, true)?;
                self.flash(format!("discarded {} and its worktree", info.branch));
                return Ok(());
            }
            KeyCode::Char('d') => info.confirm_discard = true,
            _ => {}
        }
        self.mode = IMode::Finish { sid, info };
        Ok(())
    }

    fn review(&mut self, sid: &str) -> Result<()> {
        let s = self
            .st
            .session(sid)
            .cloned()
            .ok_or_else(|| anyhow!("no such session"))?;
        if !git::is_repo(&s.cwd) {
            bail!(
                "{} isn't a git repository, so there's nothing to diff",
                tilde(&s.cwd)
            );
        }
        let project = self
            .st
            .project(&s.project)
            .map(|p| p.name.clone())
            .unwrap_or_default();
        let title = format!("{project} / {}", s.name);
        let q = crate::backend::tmux::sh_quote;
        let cmd = format!(
            "{} review --sid {} --cwd {} --title {} --color {}",
            q(&self.env.pebl),
            q(&s.id),
            q(&s.cwd.display().to_string()),
            q(&title),
            if self.color == ColorMode::Indexed {
                "256"
            } else {
                "truecolor"
            }
        );
        self.alerts.remove(sid);
        self.be.popup(&s.cwd, &format!(" {title} "), &cmd)
    }

    fn on_screen(&self, sid: &str) -> bool {
        self.active.as_deref() == Some(sid) || self.cols.iter().any(|c| c == sid)
    }

    fn next_waiting(&self, after: &str) -> Option<String> {
        let order = self.session_order();
        let start = order
            .iter()
            .position(|s| s == after)
            .map(|i| i + 1)
            .unwrap_or(0);
        let n = order.len();
        (0..n)
            .map(|i| &order[(start + i) % n])
            .find(|sid| {
                *sid != after
                    && self
                        .st
                        .session(sid)
                        .is_some_and(|s| s.status == Status::Waiting)
            })
            .cloned()
    }

    fn start_new_session(&mut self, worktree: bool) -> Result<()> {
        let Some(pid) = self.current_project() else {
            self.start_new_project();
            return Ok(());
        };
        self.cancel_mode();
        self.filter.clear();
        self.toggle_open(&pid, Some(true));
        self.mode = IMode::NewSession {
            pid,
            text: String::new(),
            prompt: String::new(),
            editing_prompt: false,
            worktree,
        };
        if self.collapsed {
            self.collapsed = false;
            self.apply_layout();
        }
        self.set_focus(Focus::Sidebar);
        Ok(())
    }

    fn start_new_project(&mut self) {
        self.cancel_mode();
        let base = self
            .launch_dir
            .parent()
            .map(tilde)
            .unwrap_or_else(|| "~".into());
        let path = format!("{}/", base.trim_end_matches('/'));
        self.mode = IMode::NewProject(self.project_prompt(path, true));
        self.apply_layout();
        self.set_focus(Focus::Bar);
    }

    fn project_prompt(&self, path: String, then_session: bool) -> NewProject {
        let candidates = complete::candidates(&path, CANDIDATES);
        let name = candidates
            .first()
            .map(|c| complete::name_for(&c.path))
            .unwrap_or_else(|| complete::name_for(&path));
        NewProject {
            path,
            candidates,
            sel: 0,
            name,
            then_session,
        }
    }

    fn import_history(&mut self) {
        if self.history.is_empty() {
            self.history = history::scan();
        }
        if self.history.is_empty() {
            self.flash("no Claude Code history found");
            return;
        }
        for (p, _) in std::mem::take(&mut self.history) {
            self.st.add_project(p, None);
        }
        self.cursor = self
            .st
            .projects
            .first()
            .map(|p| CurRef::Project(p.id.clone()));
        self.set_focus(Focus::Sidebar);
    }

    fn add_launch_dir(&mut self) -> Result<()> {
        let pid = self.st.add_project(self.launch_dir.clone(), None);
        self.cursor = Some(CurRef::Project(pid.clone()));
        self.mode = IMode::NewSession {
            pid,
            text: "main".into(),
            prompt: String::new(),
            editing_prompt: false,
            worktree: false,
        };
        self.set_focus(Focus::Sidebar);
        Ok(())
    }

    fn click(&mut self, y: u16) -> Result<()> {
        if !matches!(self.mode, IMode::Normal) {
            return Ok(());
        }
        let v = self.view();
        let rows = self.rows();
        let Some(hit) = crate::ui::sidebar::hit(&v, y) else {
            return Ok(());
        };
        let target = match hit {
            Cursor::Project(pi) => rows
                .iter()
                .filter(|r| matches!(r, CurRef::Project(_)))
                .nth(pi)
                .cloned(),
            Cursor::Session(pi, si) => {
                let (_, ss) = self.visible().get(pi).cloned().unwrap_or_default();
                ss.get(si)
                    .map(|i| CurRef::Session(self.st.sessions[*i].id.clone()))
            }
        };
        match target {
            Some(CurRef::Project(pid)) => {
                self.toggle_open(&pid, None);
                self.cursor = Some(CurRef::Project(pid));
            }
            Some(c) => self.set_cursor(c),
            None => {}
        }
        Ok(())
    }

    fn click_tab(&mut self, x: u16) {
        let n = self.cols.len() + 1;
        let mut start = 0u16;
        for (i, w) in self.col_widths(n).into_iter().enumerate() {
            if x < start + w {
                self.focus_column(i);
                if x + 2 == start + w {
                    self.close_column();
                }
                return;
            }
            start += w + 1;
        }
    }

    fn key(&mut self, role: Role, k: Key) -> Result<()> {
        if let IMode::Broadcast { marks, text } = self.mode.clone() {
            return self.broadcast_key(marks, text, k);
        }
        if k.ctrl {
            return Ok(());
        }
        match self.mode.clone() {
            IMode::Mark { marks } => self.mark_key(marks, k),
            IMode::DrawerResize => {
                let step = if k.shift { 5 } else { 1 };
                match k.code {
                    KeyCode::Up | KeyCode::Char('k') => self.resize_drawer(step),
                    KeyCode::Down | KeyCode::Char('j') => self.resize_drawer(-step),
                    KeyCode::Char('K') => self.resize_drawer(5),
                    KeyCode::Char('J') => self.resize_drawer(-5),
                    KeyCode::Char('=') => {
                        self.drawer_h = Some(self.column_rows() / 2);
                        self.apply_layout();
                    }
                    KeyCode::Enter | KeyCode::Esc | KeyCode::Char('q') => {
                        self.mode = IMode::Normal;
                        self.set_focus(Focus::Shell);
                    }
                    _ => {}
                }
                Ok(())
            }
            IMode::Broadcast { .. } => Ok(()),
            IMode::Search {
                query,
                all,
                sel,
                scope,
            } => self.search_key(query, all, sel, scope, k),
            IMode::Finish { sid, info } => self.finish_key(sid, info, k),
            IMode::Peek { sid, mut text } => {
                match k.code {
                    KeyCode::Esc => {
                        self.cancel_mode();
                        let back = self.peek_from;
                        self.set_focus(back);
                    }
                    KeyCode::Enter if text.trim().is_empty() => {
                        self.peek_send(&sid, "", Some("Enter"))?;
                    }
                    KeyCode::Enter => {
                        self.peek_send(&sid, text.trim(), None)?;
                        self.mode = IMode::Peek {
                            sid,
                            text: String::new(),
                        };
                    }
                    KeyCode::Up | KeyCode::Down if text.is_empty() => {
                        let key = if k.code == KeyCode::Up { "Up" } else { "Down" };
                        self.peek_send(&sid, "", Some(key))?;
                    }
                    KeyCode::Char(c) if text.is_empty() && c.is_ascii_digit() && c != '0' => {
                        self.peek_send(&sid, "", Some(&c.to_string()))?;
                    }
                    KeyCode::Tab => match self.next_waiting(&sid) {
                        Some(next) => {
                            self.mode = IMode::Peek {
                                sid: next,
                                text: String::new(),
                            };
                            self.refresh_peek();
                        }
                        None => self.flash("nothing else is waiting"),
                    },
                    KeyCode::BackTab => {
                        self.cancel_mode();
                        self.activate(&sid);
                        self.sync_main();
                        self.set_focus(Focus::Main);
                    }
                    _ => {
                        edit(&mut text, &k, false);
                        self.mode = IMode::Peek { sid, text };
                    }
                }
                Ok(())
            }
            IMode::Resize => {
                let step = if k.shift { 5 } else { 1 };
                match k.code {
                    KeyCode::Left | KeyCode::Char('h') => self.resize_col(-step),
                    KeyCode::Right | KeyCode::Char('l') => self.resize_col(step),
                    KeyCode::Char('H') => self.resize_col(-5),
                    KeyCode::Char('L') => self.resize_col(5),
                    KeyCode::Char('=') => {
                        self.col_w.clear();
                        self.apply_layout();
                    }
                    KeyCode::Enter | KeyCode::Esc | KeyCode::Char('q') => {
                        self.mode = IMode::Normal;
                        self.set_focus(Focus::Main);
                        self.style_split();
                    }
                    _ => {}
                }
                Ok(())
            }
            IMode::Normal => self.key_normal(role, k),
            IMode::NewSession {
                pid,
                mut text,
                mut prompt,
                editing_prompt,
                worktree,
            } => {
                match k.code {
                    KeyCode::Esc => {
                        self.mode = IMode::Normal;
                        if self.active.is_some() {
                            self.set_focus(Focus::Main);
                        }
                        return Ok(());
                    }
                    KeyCode::Enter => {
                        self.mode = IMode::Normal;
                        self.spawn_session(&pid, &text, &prompt, worktree)?;
                        self.sync_main();
                        self.set_focus(Focus::Main);
                        return Ok(());
                    }
                    KeyCode::Tab | KeyCode::BackTab => {
                        self.mode = IMode::NewSession {
                            pid,
                            text,
                            prompt,
                            editing_prompt: !editing_prompt,
                            worktree,
                        };
                        return Ok(());
                    }
                    _ => {}
                }
                let field = if editing_prompt {
                    &mut prompt
                } else {
                    &mut text
                };
                edit(field, &k, !editing_prompt);
                self.mode = IMode::NewSession {
                    pid,
                    text,
                    prompt,
                    editing_prompt,
                    worktree,
                };
                Ok(())
            }
            IMode::Rename { target, mut text } => {
                match k.code {
                    KeyCode::Esc => self.mode = IMode::Normal,
                    KeyCode::Enter => {
                        let name = text.trim().to_string();
                        if !name.is_empty() {
                            match &target {
                                CurRef::Session(sid) => {
                                    if let Some(s) = self.st.session_mut(sid) {
                                        s.name = name;
                                        s.auto_name = false;
                                    }
                                }
                                CurRef::Project(pid) => {
                                    if let Some(p) =
                                        self.st.projects.iter_mut().find(|p| p.id == *pid)
                                    {
                                        p.name = name;
                                    }
                                }
                            }
                        }
                        self.mode = IMode::Normal;
                    }
                    _ => {
                        edit(&mut text, &k, true);
                        self.mode = IMode::Rename { target, text };
                    }
                }
                Ok(())
            }
            IMode::NewProject(np) => self.key_new_project(np, k),
            IMode::ConfirmKill { target } => {
                self.mode = IMode::Normal;
                if matches!(k.code, KeyCode::Char('y') | KeyCode::Char('Y')) {
                    self.kill(&target)?;
                }
                Ok(())
            }
            IMode::Filter { mut text } => {
                match k.code {
                    KeyCode::Esc => {
                        self.filter.clear();
                        self.mode = IMode::Normal;
                    }
                    KeyCode::Enter => {
                        self.filter = text;
                        self.mode = IMode::Normal;
                        if let Some(first) = self
                            .rows()
                            .into_iter()
                            .find(|r| matches!(r, CurRef::Session(_)))
                        {
                            self.set_cursor(first);
                        }
                    }
                    KeyCode::Backspace if text.is_empty() => self.mode = IMode::Normal,
                    _ => {
                        edit(&mut text, &k, false);
                        self.mode = IMode::Filter { text };
                    }
                }
                Ok(())
            }
        }
    }

    fn key_normal(&mut self, role: Role, k: Key) -> Result<()> {
        use KeyCode::*;
        if self.help {
            self.help = false;
            return Ok(());
        }
        if role == Role::Stage {
            return match (self.stage_kind(), k.code) {
                (Some(Stage::Welcome { .. }), Enter) => self.add_launch_dir(),
                (Some(Stage::Welcome { .. }), Char('i')) => {
                    self.import_history();
                    Ok(())
                }
                (Some(Stage::Exited { .. }), Enter) => {
                    let sid = self.active.clone().unwrap_or_default();
                    self.resume(&sid)?;
                    self.sync_main();
                    self.set_focus(Focus::Main);
                    Ok(())
                }
                (Some(Stage::Exited { .. }), Char('x')) => {
                    if let Some(sid) = self.active.clone() {
                        self.mode = IMode::ConfirmKill {
                            target: CurRef::Session(sid),
                        };
                    }
                    Ok(())
                }
                (Some(Stage::Empty), Enter | Char('n')) => self.start_new_session(false),
                (_, Char('o')) => {
                    self.start_new_project();
                    Ok(())
                }
                (_, Char('?')) => {
                    self.help = true;
                    Ok(())
                }
                (_, Tab | Esc) => {
                    self.set_focus(Focus::Sidebar);
                    Ok(())
                }
                _ => Ok(()),
            };
        }
        if role == Role::Bar {
            if k.code == Esc {
                self.set_focus(Focus::Main);
            }
            return Ok(());
        }
        match k.code {
            Char('j') | Down => self.move_cursor(1),
            Char('k') | Up => self.move_cursor(-1),
            Char('g') | Home => self.move_cursor(-10_000),
            Char('G') | End => self.move_cursor(10_000),
            Enter | Char('l') | Right => match self.cursor.clone() {
                Some(CurRef::Session(sid)) => {
                    let alive = self
                        .st
                        .session(&sid)
                        .and_then(|s| s.pane.clone())
                        .is_some_and(|p| !self.dead.contains(&p));
                    if !alive {
                        self.resume(&sid)?;
                    }
                    self.sync_main();
                    self.set_focus(Focus::Main);
                }
                Some(CurRef::Project(pid)) => {
                    self.toggle_open(&pid, Some(k.code == Enter && !self.is_open(&pid)))
                }
                None if self.st.projects.is_empty() => self.add_launch_dir()?,
                None => self.move_cursor(0),
            },
            Char('h') | Left => {
                if let Some(pid) = self.current_project() {
                    self.toggle_open(&pid, Some(false));
                    self.cursor = Some(CurRef::Project(pid));
                }
            }
            Char(' ') => {
                if let Some(pid) = self.current_project() {
                    self.toggle_open(&pid, None);
                    self.cursor = Some(CurRef::Project(pid));
                }
            }
            Char('r') => {
                let text = match &self.cursor {
                    Some(CurRef::Session(s)) => self.st.session(s).map(|s| s.name.clone()),
                    Some(CurRef::Project(p)) => self.st.project(p).map(|p| p.name.clone()),
                    None => None,
                };
                if let (Some(text), Some(target)) = (text, self.cursor.clone()) {
                    self.mode = IMode::Rename { target, text };
                }
            }
            Char('x') | Delete => {
                if let Some(target) = self.cursor.clone() {
                    self.mode = IMode::ConfirmKill { target };
                }
            }
            Char('n') => self.start_new_session(false)?,
            Char('w') => self.start_new_session(true)?,
            Char('o') => self.start_new_project(),
            Char('/') => {
                self.mode = IMode::Filter {
                    text: self.filter.clone(),
                }
            }
            Char('i') => self.import_history(),
            Char('e') => self.open_editor()?,
            Char('f') => {
                let sid = match &self.cursor {
                    Some(CurRef::Session(sid)) => sid.clone(),
                    _ => bail!("move to a worktree session to finish it"),
                };
                self.start_finish(&sid)?;
            }
            Char('d') => {
                let sid = match &self.cursor {
                    Some(CurRef::Session(sid)) => sid.clone(),
                    _ => bail!("move to a session to review its changes"),
                };
                self.review(&sid)?;
            }
            Char('p') => {
                let hidden_waiting = self
                    .st
                    .sessions
                    .iter()
                    .filter(|s| s.status == Status::Waiting && !self.on_screen(&s.id))
                    .min_by_key(|s| s.since)
                    .map(|s| s.id.clone());
                let cursor = match &self.cursor {
                    Some(CurRef::Session(sid)) => Some(sid.clone()),
                    _ => None,
                };
                let sid = hidden_waiting
                    .or(cursor)
                    .ok_or_else(|| anyhow!("move to a session to peek at it"))?;
                self.start_peek(&sid)?;
            }
            Char('m') => {
                self.muted = !self.muted;
                self.flash(if self.muted {
                    "mascot and bell muted · m to unmute"
                } else {
                    "mascot back on"
                });
            }
            Char('?') => self.help = true,
            Tab | Esc => {
                if !self.filter.is_empty() && k.code == Esc {
                    self.filter.clear();
                } else if self.active.is_some() {
                    self.set_focus(Focus::Main);
                }
            }
            _ => {}
        }
        Ok(())
    }

    fn open_editor(&mut self) -> Result<()> {
        let dir = match &self.cursor {
            Some(CurRef::Session(sid)) => self.st.session(sid).map(|s| s.cwd.clone()),
            Some(CurRef::Project(pid)) => self.st.project(pid).map(|p| p.path.clone()),
            None => None,
        }
        .ok_or_else(|| anyhow!("select a project first"))?;
        let ed = &self.env.editor;
        let mut cmd = if crate::daemon::find_in_path(ed).is_some() {
            std::process::Command::new(ed)
        } else {
            let mut c = std::process::Command::new("open");
            c.args(["-a", ed]);
            c
        };
        let mut child = cmd
            .arg(&dir)
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn()
            .map_err(|e| anyhow!("opening {ed}: {e}"))?;
        std::thread::spawn(move || {
            if !child.wait().is_ok_and(|s| s.success()) {
                crate::daemon::log("editor exited with an error");
            }
        });
        self.flash(format!("opened {} in {ed}", tilde(&dir)));
        Ok(())
    }

    fn is_open(&self, pid: &str) -> bool {
        self.st.project(pid).is_some_and(|p| p.open)
    }

    fn key_new_project(&mut self, mut np: NewProject, k: Key) -> Result<()> {
        use KeyCode::*;
        match k.code {
            Esc => {
                self.cancel_mode();
                self.set_focus(if self.active.is_some() {
                    Focus::Main
                } else {
                    Focus::Sidebar
                });
                return Ok(());
            }
            Enter => {
                let chosen = np
                    .candidates
                    .get(np.sel)
                    .map(|c| c.path.clone())
                    .unwrap_or_else(|| np.path.clone());
                let path = untilde(chosen.trim_end_matches('/'));
                if !path.is_dir() {
                    bail!("no such folder: {chosen}");
                }
                let path = std::fs::canonicalize(&path)?;
                let pid = self
                    .st
                    .add_project(path, Some(np.name.clone()).filter(|n| !n.is_empty()));
                self.cancel_mode();
                self.cursor = Some(CurRef::Project(pid));
                if np.then_session {
                    self.start_new_session(false)?;
                } else {
                    self.set_focus(Focus::Sidebar);
                }
                return Ok(());
            }
            Tab => {
                if let Some(c) = np.candidates.get(np.sel) {
                    let path = format!("{}/", c.path.trim_end_matches('/'));
                    np = self.project_prompt(path, np.then_session);
                }
            }
            Down => np.sel = (np.sel + 1).min(np.candidates.len().saturating_sub(1)),
            Up => np.sel = np.sel.saturating_sub(1),
            Char(' ') => np.then_session = !np.then_session,
            _ => {
                let mut path = np.path.clone();
                edit(&mut path, &k, false);
                np = self.project_prompt(path, np.then_session);
            }
        }
        if let Some(c) = np.candidates.get(np.sel) {
            np.name = complete::name_for(&c.path);
        }
        self.mode = IMode::NewProject(np);
        Ok(())
    }

    pub fn view(&self) -> View {
        let vis = self.visible();
        let mut projects = Vec::new();
        let mut cursor = None;
        let mut counts = Counts::default();
        for s in &self.st.sessions {
            match s.status {
                Status::Running => counts.running += 1,
                Status::Waiting => counts.waiting += 1,
                Status::Idle => counts.idle += 1,
                Status::Error => counts.error += 1,
                Status::Exited => counts.exited += 1,
            }
        }
        for (vi, (pi, ss)) in vis.iter().enumerate() {
            let p = &self.st.projects[*pi];
            if self.cursor == Some(CurRef::Project(p.id.clone())) {
                cursor = Some(Cursor::Project(vi));
            }
            let all: Vec<&Session> = self.st.sessions_of(&p.id).collect();
            let mut rows = Vec::new();
            for (si, i) in ss.iter().enumerate() {
                let s = &self.st.sessions[*i];
                if self.cursor == Some(CurRef::Session(s.id.clone())) {
                    cursor = Some(Cursor::Session(vi, si));
                }
                rows.push(SessionRow {
                    name: s.name.clone(),
                    status: s.status,
                    meta: meta(s),
                    col: self.column_of(&s.id),
                    ctx: self.ctx_pct(&s.id),
                    marked: match &self.mode {
                        IMode::Mark { marks } | IMode::Broadcast { marks, .. } => {
                            Some(marks.contains(&s.id))
                        }
                        _ => None,
                    },
                });
            }
            projects.push(ProjectRow {
                name: p.name.clone(),
                open: p.open || self.filtering(),
                total: all.len(),
                waiting: all.iter().filter(|s| s.status == Status::Waiting).count(),
                sessions: rows,
            });
        }
        let pidx = |pid: &str| {
            vis.iter()
                .position(|(pi, _)| self.st.projects[*pi].id == pid)
                .unwrap_or(0)
        };
        let mode = match &self.mode {
            IMode::Normal | IMode::Resize | IMode::DrawerResize => Mode::Normal,
            IMode::NewSession {
                pid,
                text,
                prompt,
                editing_prompt,
                worktree,
            } => Mode::NewSession {
                project: pidx(pid),
                text: text.clone(),
                prompt: prompt.clone(),
                editing_prompt: *editing_prompt,
                worktree: *worktree,
            },
            IMode::Rename { text, .. } => Mode::Rename {
                cursor: cursor.unwrap_or(Cursor::Project(0)),
                text: text.clone(),
            },
            IMode::NewProject(np) => Mode::NewProject(np.clone()),
            IMode::ConfirmKill { target } => Mode::ConfirmKill {
                name: match target {
                    CurRef::Session(s) => self
                        .st
                        .session(s)
                        .map(|s| s.name.clone())
                        .unwrap_or_default(),
                    CurRef::Project(p) => self
                        .st
                        .project(p)
                        .map(|p| p.name.clone())
                        .unwrap_or_default(),
                },
            },
            IMode::Filter { text } => Mode::Filter { text: text.clone() },
            IMode::Finish { info, .. } => Mode::Finish(info.clone()),
            IMode::Search {
                query,
                all,
                sel,
                scope,
            } => Mode::Search(self.search_view(query, *all, *sel, scope)),
            IMode::Mark { marks } => Mode::Mark { count: marks.len() },
            IMode::Broadcast { marks, text } => {
                let targets: Vec<crate::view::Target> = marks
                    .iter()
                    .filter_map(|sid| self.st.session(sid))
                    .map(|s| crate::view::Target {
                        project: self
                            .st
                            .project(&s.project)
                            .map(|p| p.name.clone())
                            .unwrap_or_default(),
                        session: s.name.clone(),
                        status: s.status,
                    })
                    .collect();
                let more = targets.len().saturating_sub(5);
                Mode::Broadcast(crate::view::Broadcast {
                    targets: targets.into_iter().take(5).collect(),
                    more,
                    text: text.clone(),
                })
            }
            IMode::Peek { sid, text } => {
                let s = self.st.session(sid);
                Mode::Peek(crate::view::Peek {
                    project: s
                        .and_then(|s| self.st.project(&s.project))
                        .map(|p| p.name.clone())
                        .unwrap_or_default(),
                    session: s.map(|s| s.name.clone()).unwrap_or_default(),
                    status: s.map(|s| s.status),
                    lines: self.peek.clone(),
                    text: text.clone(),
                })
            }
        };
        if matches!(mode, Mode::NewSession { .. }) {
            cursor = None;
        }
        let focused = self.focused_sid();
        let active = focused
            .as_deref()
            .and_then(|a| self.st.session(a))
            .map(|s| Active {
                project: self
                    .st
                    .project(&s.project)
                    .map(|p| p.name.clone())
                    .unwrap_or_default(),
                session: s.name.clone(),
                status: s.status,
                age: age(now().saturating_sub(s.since)),
            });
        let path_of = |pid: &str| self.st.project(pid).map(|p| tilde(&p.path));
        let path = match &self.mode {
            IMode::NewSession { pid, .. } => path_of(pid),
            _ => focused
                .as_deref()
                .and_then(|a| self.st.session(a))
                .map(|s| tilde(&s.cwd)),
        }
        .unwrap_or_else(|| tilde(&self.launch_dir));
        View {
            projects,
            cursor,
            active,
            focus: self.focus,
            mode,
            layout: self.layout(),
            counts,
            path,
            stage: self.stage_kind().unwrap_or(Stage::Empty),
            flash: self.flash.as_ref().map(|(m, _)| m.clone()),
            bell: self.bell,
            color: self.color,
            filter: self.filter.clone(),
            mascot: if self.muted {
                None
            } else {
                self.mascot.clone()
            },
            notice: self.notice(&counts),
            columns: self.tabs(),
            col_focus: self.col,
            resizing: self.resize_divider(),
            usage: focused.as_deref().and_then(|sid| self.usage_of(sid)),
            limits: self.limit_rows(),
            today: self.today(),
            shell: self.shell_view(),
            runnable: self
                .active
                .as_ref()
                .and_then(|sid| self.runnable.get(sid))
                .filter(|(l, _)| !l.is_empty())
                .map(|(l, i)| (i + 1, l.len())),
            marking: match &self.mode {
                IMode::Mark { marks } | IMode::Broadcast { marks, .. } => Some(marks.len()),
                _ => None,
            },
        }
    }

    fn shell_view(&self) -> Option<crate::view::ShellView> {
        let sid = self.active.as_ref()?;
        let s = self.st.session(sid)?;
        let has = self.shells.contains_key(sid);
        let run = self.shell_run.get(sid).cloned().unwrap_or_default();
        let open = self.drawer_shown.is_some();
        if !has && !open {
            return None;
        }
        let shell = std::env::var("SHELL")
            .ok()
            .and_then(|p| p.rsplit('/').next().map(str::to_string))
            .unwrap_or_else(|| "sh".into());
        let resize = matches!(self.mode, IMode::DrawerResize).then(|| {
            let d = self.drawer_height();
            (self.column_rows().saturating_sub(d + 1), d)
        });
        Some(crate::view::ShellView {
            open,
            path: tilde(&s.cwd),
            shell,
            running: run.running,
            done: run.done,
            resize,
        })
    }

    fn ctx_pct(&self, sid: &str) -> Option<u8> {
        self.ctx
            .get(sid)
            .and_then(|c| c.pct)
            .map(|p| p.round().clamp(0.0, 100.0) as u8)
    }

    fn usage_of(&self, sid: &str) -> Option<Usage> {
        let c = self.ctx.get(sid);
        let (tok_in, tok_out) = self
            .tokens
            .get(sid)
            .map(|(_, _, t)| t.total())
            .unwrap_or((0, 0));
        if c.is_none() && tok_in + tok_out == 0 {
            return None;
        }
        let c = c.cloned().unwrap_or_default();
        Some(Usage {
            model: c.model,
            pct: self.ctx_pct(sid),
            used: c.used,
            size: c.size,
            tok_in,
            tok_out,
        })
    }

    fn limit_rows(&self) -> Vec<Limit> {
        let t = now();
        self.limits
            .iter()
            .filter(|(_, _, at)| *at > t)
            .map(|(label, pct, at)| Limit {
                label: label.clone(),
                pct: pct.round().clamp(0.0, 255.0) as u8,
                resets: until(*at - t),
            })
            .collect()
    }

    fn today(&self) -> (u64, u64) {
        let day = crate::model::utc_date(now());
        self.tokens
            .values()
            .map(|(_, _, t)| t.on(&day))
            .fold((0, 0), |(a, b), (i, o)| (a + i, b + o))
    }

    fn column_of(&self, sid: &str) -> Option<usize> {
        if self.cols.is_empty() {
            return None;
        }
        if self.active.as_deref() == Some(sid) {
            return Some(1);
        }
        self.cols.iter().position(|c| c == sid).map(|i| i + 2)
    }

    fn tabs(&self) -> Vec<ColumnTab> {
        if self.cols.is_empty() {
            return Vec::new();
        }
        let tab = |sid: Option<&String>| {
            sid.and_then(|sid| self.st.session(sid))
                .map(|s| ColumnTab {
                    project: self
                        .st
                        .project(&s.project)
                        .map(|p| p.name.clone())
                        .unwrap_or_default(),
                    session: s.name.clone(),
                    status: Some(s.status),
                    width: 0,
                    ctx: self.ctx_pct(&s.id),
                })
                .unwrap_or_else(|| ColumnTab {
                    project: "pebl".into(),
                    session: String::new(),
                    status: None,
                    width: 0,
                    ctx: None,
                })
        };
        let mut out = vec![tab(self.active.as_ref())];
        out.extend(self.cols.iter().map(|c| tab(Some(c))));
        for (t, w) in out.iter_mut().zip(self.col_widths(self.cols.len() + 1)) {
            t.width = w;
        }
        out
    }

    pub fn header(&self) -> Vec<String> {
        let v = self.view();
        let w = self.win.0 as usize;
        let mut out = vec![crate::ui::header::tmux_format(
            &crate::ui::header::segments(&v, w),
            self.color,
        )];
        if let Some(l) = crate::ui::header::usage_line(&v, w) {
            out.push(crate::ui::header::tmux_format(&l, self.color));
        }
        out
    }
}

fn classify(tail: &str) -> (bool, String) {
    let lines: Vec<&str> = tail
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty())
        .collect();
    let recent = &lines[lines.len().saturating_sub(12)..];
    let markers = [
        "error",
        "failed",
        "fatal",
        "exception",
        "panic",
        "err!",
        "exit code",
        "command not found",
        "no such file",
        "econnrefused",
    ];
    match recent
        .iter()
        .rev()
        .find(|l| markers.iter().any(|m| l.to_lowercase().contains(m)))
    {
        Some(l) => (true, l.to_string()),
        None => (false, String::new()),
    }
}

fn when(mtime: u64) -> String {
    let ago = now().saturating_sub(mtime);
    if ago < 7 * 86_400 {
        return age(ago);
    }
    let d = crate::model::utc_date(mtime);
    let months = [
        "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
    ];
    let m: usize = d.get(5..7).and_then(|m| m.parse().ok()).unwrap_or(1);
    let day: u32 = d.get(8..10).and_then(|x| x.parse().ok()).unwrap_or(1);
    format!("{} {day}", months[m.clamp(1, 12) - 1])
}

fn until(secs: u64) -> String {
    let (d, h, m) = (secs / 86_400, (secs % 86_400) / 3600, (secs % 3600) / 60);
    if d > 0 {
        format!("{d}d{h}h")
    } else if h > 0 {
        format!("{h}h{m:02}m")
    } else {
        format!("{m}m")
    }
}

fn meta(s: &Session) -> String {
    match s.status {
        Status::Running | Status::Idle => age(now().saturating_sub(s.since)),
        Status::Waiting => "input".into(),
        Status::Error => s
            .exit_code
            .map(|c| format!("exit {c}"))
            .unwrap_or_else(|| "error".into()),
        Status::Exited => "exited".into(),
    }
}

fn edit(field: &mut String, k: &Key, name_like: bool) {
    match k.code {
        KeyCode::Backspace => {
            field.pop();
        }
        KeyCode::Char(c) if !k.alt => {
            let c = if name_like && c == ' ' { '-' } else { c };
            if name_like && field.chars().count() >= 40 {
                return;
            }
            field.push(c);
        }
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::backend::LivePane;
    use std::cell::RefCell;

    #[derive(Default)]
    struct Fake {
        log: RefCell<Vec<String>>,
        shell_panes: RefCell<Vec<(String, String)>>,
        panes: RefCell<Vec<LivePane>>,
        next: RefCell<u32>,
    }

    impl SessionBackend for Fake {
        fn spawn(&self, spec: &SpawnSpec) -> Result<String> {
            *self.next.borrow_mut() += 1;
            let id = format!("%{}", 10 + *self.next.borrow());
            self.log
                .borrow_mut()
                .push(format!("spawn {} {}", spec.sid, spec.argv.join(" ")));
            self.panes.borrow_mut().push(LivePane {
                id: id.clone(),
                sid: Some(spec.sid.clone()),
                dead: false,
                dead_status: None,
            });
            Ok(id)
        }
        fn respawn(&self, pane: &str, _: &SpawnSpec) -> Result<()> {
            self.log.borrow_mut().push(format!("respawn {pane}"));
            Ok(())
        }
        fn kill(&self, pane: &str) -> Result<()> {
            self.log.borrow_mut().push(format!("kill {pane}"));
            self.panes.borrow_mut().retain(|p| p.id != pane);
            Ok(())
        }
        fn list(&self) -> Result<Vec<LivePane>> {
            Ok(self.panes.borrow().clone())
        }
        fn capture(&self, _: &str) -> Result<String> {
            Ok("one\n\n❯ 1. Yes\n  2. No\n\n\n".into())
        }
        fn send_text(&self, pane: &str, text: &str, enter: bool) -> Result<()> {
            self.log
                .borrow_mut()
                .push(format!("send {pane} {text:?} {enter}"));
            Ok(())
        }
        fn send_key(&self, pane: &str, key: &str) -> Result<()> {
            self.log.borrow_mut().push(format!("key {pane} {key}"));
            Ok(())
        }
        fn spawn_shell(&self, sid: &str, _: &std::path::Path, _: (u16, u16)) -> Result<String> {
            let id = format!("%sh-{sid}");
            self.shell_panes
                .borrow_mut()
                .push((id.clone(), sid.to_string()));
            Ok(id)
        }
        fn shells(&self) -> Result<Vec<crate::backend::ShellPane>> {
            Ok(self
                .shell_panes
                .borrow()
                .iter()
                .map(|(p, s)| crate::backend::ShellPane {
                    pane: p.clone(),
                    sid: s.clone(),
                    command: "zsh".into(),
                    pid: 1,
                })
                .collect())
        }
        fn child_command(&self, _: u32) -> Option<String> {
            None
        }
    }

    impl UiBackend for Fake {
        fn create_ui(&self, _: &str, _: (u16, u16), _: u16) -> Result<UiPanes> {
            Ok(UiPanes {
                sidebar: "%1".into(),
                stage: "%2".into(),
                bar: "%3".into(),
            })
        }
        fn ui_panes(&self) -> Result<Option<UiPanes>> {
            Ok(Some(UiPanes {
                sidebar: "%1".into(),
                stage: "%2".into(),
                bar: "%3".into(),
            }))
        }
        fn repair_ui(
            &self,
            _: &str,
            _: Option<&str>,
            _: (u16, u16),
            _: u16,
        ) -> Result<(UiPanes, String)> {
            unreachable!("ui never breaks in tests")
        }
        fn slot(&self) -> Result<Option<String>> {
            Ok(Some("%2".into()))
        }
        fn window_size(&self) -> Result<(u16, u16)> {
            Ok((120, 40))
        }
        fn show(&self, pane: &str, cur: &str) -> Result<()> {
            self.log
                .borrow_mut()
                .push(format!("show {pane} over {cur}"));
            Ok(())
        }
        fn select(&self, _: &str) -> Result<()> {
            Ok(())
        }
        fn set_header(&self, _: &[String]) -> Result<()> {
            Ok(())
        }
        fn size_parked(&self, _: (u16, u16)) -> Result<()> {
            Ok(())
        }
        fn arrange(&self, a: &crate::backend::Arrangement) -> Result<()> {
            self.log.borrow_mut().push(format!(
                "arrange {}",
                a.cols
                    .iter()
                    .map(|(p, _)| p.as_str())
                    .collect::<Vec<_>>()
                    .join(",")
            ));
            Ok(())
        }
        fn tabs_pane(&self, _: &str) -> Result<String> {
            Ok("%9".into())
        }
        fn widths(&self) -> Result<Vec<(String, u16)>> {
            Ok(Vec::new())
        }
        fn set_split(&self, _: bool, _: Option<&str>) -> Result<()> {
            Ok(())
        }
        fn heights(&self) -> Result<Vec<(String, u16)>> {
            Ok(Vec::new())
        }
        fn set_flag(&self, name: &str, on: bool) -> Result<()> {
            self.log.borrow_mut().push(format!("flag {name} {on}"));
            Ok(())
        }
        fn popup(&self, _: &std::path::Path, title: &str, cmd: &str) -> Result<()> {
            self.log.borrow_mut().push(format!("popup {title} {cmd}"));
            Ok(())
        }
    }

    fn app() -> (App<Fake>, tempfile::TempDir) {
        let dir = tempfile::tempdir().unwrap();
        let env = Env {
            pebl: "pebl".into(),
            claude: "claude".into(),
            settings: "/s.json".into(),
            sock: "/x.sock".into(),
            editor: "true".into(),
            notify: crate::config::Notify {
                phone: "test-topic".into(),
                phone_after: 0,
                ..Default::default()
            },
        };
        let mut a = App::new(State::default(), Fake::default(), env);
        a.handle_ctl(
            &[
                "ensure-ui",
                dir.path().to_str().unwrap(),
                "120",
                "40",
                "truecolor",
            ]
            .map(String::from),
        );
        (a, dir)
    }

    fn key(a: &mut App<Fake>, role: Role, c: KeyCode) {
        a.handle_input(role, Input::Key(Key::plain(c)));
    }

    fn typ_role(a: &mut App<Fake>, role: Role, s: &str) {
        for c in s.chars() {
            key(a, role, KeyCode::Char(c));
        }
    }

    fn typ(a: &mut App<Fake>, s: &str) {
        for c in s.chars() {
            key(a, Role::Sidebar, KeyCode::Char(c));
        }
    }

    #[test]
    fn first_launch_enter_adds_dir_then_starts_session() {
        let (mut a, _d) = app();
        assert!(matches!(a.view().stage, Stage::Welcome { .. }));
        key(&mut a, Role::Stage, KeyCode::Enter);
        assert_eq!(a.st.projects.len(), 1);
        assert!(matches!(a.view().mode, Mode::NewSession { ref text, .. } if text == "main"));
        key(&mut a, Role::Sidebar, KeyCode::Enter);
        assert_eq!(a.st.sessions.len(), 1);
        let log = a.be.log.borrow().join("\n");
        assert!(
            log.contains("spawn s2 claude --settings /s.json --session-id"),
            "{log}"
        );
        assert!(log.contains("show %11 over %2"), "{log}");
        assert_eq!(a.view().focus, Focus::Main);
    }

    #[test]
    fn new_session_with_typed_name_and_prompt() {
        let (mut a, d) = app();
        a.handle_ctl(&["add".into(), d.path().display().to_string()]);
        a.handle_ctl(&["new-session".into()]);
        typ(&mut a, "fix flaky");
        key(&mut a, Role::Sidebar, KeyCode::Tab);
        typ(&mut a, "run the tests");
        key(&mut a, Role::Sidebar, KeyCode::Enter);
        let s = &a.st.sessions[0];
        assert_eq!(s.name, "fix-flaky");
        assert_eq!(s.status, Status::Running);
        assert!(a
            .be
            .log
            .borrow()
            .iter()
            .any(|l| l.ends_with("-n fix-flaky run the tests")));
    }

    #[test]
    fn hooks_drive_status_and_ctrl_g_finds_oldest_waiting() {
        let (mut a, d) = app();
        a.handle_ctl(&["add".into(), d.path().display().to_string()]);
        for n in ["a", "b", "c"] {
            a.handle_ctl(&["new".into(), String::new(), n.into(), String::new()]);
        }
        let ids: Vec<String> = a.st.sessions.iter().map(|s| s.id.clone()).collect();
        let perm = serde_json::json!({"notification_type": "permission_prompt"});
        a.handle_hook(&ids[1], "Notification", &perm);
        a.st.session_mut(&ids[1]).unwrap().since = 100;
        a.handle_hook(&ids[0], "PermissionRequest", &serde_json::Value::Null);
        assert_eq!(a.view().counts.waiting, 2);
        assert!(a.view().bell >= 1);
        a.handle_ctl(&["waiting".into()]);
        assert_eq!(a.view().active.unwrap().session, "b");
        a.handle_hook(&ids[1], "PostToolUse", &serde_json::Value::Null);
        assert_eq!(a.st.session(&ids[1]).unwrap().status, Status::Running);
    }

    #[test]
    fn columns_open_focus_resize_and_close() {
        let (mut a, d) = app();
        a.handle_ctl(&["add".into(), d.path().display().to_string()]);
        for n in ["a", "b", "c"] {
            a.handle_ctl(&["new".into(), String::new(), n.into(), String::new()]);
        }
        a.handle_ctl(&["split".into()]);
        let out = a.handle_ctl(&["split".into()]);
        assert!(out.contains("no room"), "{out}");
        a.handle_ctl(&["toggle-sidebar".into()]);
        a.handle_ctl(&["split".into()]);
        let v = a.view();
        assert_eq!(v.columns.len(), 3);
        assert_eq!(v.col_focus, 2);
        let total: u16 = v.columns.iter().map(|c| c.width).sum();
        assert_eq!(total + 2, 120);
        let mut marks: Vec<Option<usize>> = v.projects[0].sessions.iter().map(|s| s.col).collect();
        marks.sort();
        assert_eq!(marks, vec![Some(1), Some(2), Some(3)]);
        let out = a.handle_ctl(&["split".into()]);
        assert!(out.starts_with("error"), "{out}");

        a.handle_ctl(&["column".into(), "1".into()]);
        assert_eq!(a.view().active.unwrap().session, "c");
        let before = a.view().columns[0].width;
        a.handle_ctl(&["resize".into(), "right".into()]);
        assert_eq!(a.view().resizing, Some(0));
        a.handle_input(
            Role::Bar,
            Input::Key(Key {
                shift: true,
                ..Key::plain(KeyCode::Right)
            }),
        );
        let v = a.view();
        assert_eq!(v.columns[0].width, before + 6);
        assert_eq!(v.columns.iter().map(|c| c.width).sum::<u16>(), total);
        key(&mut a, Role::Bar, KeyCode::Enter);
        assert_eq!(a.view().resizing, None);

        a.handle_ctl(&["close-column".into()]);
        let v = a.view();
        assert_eq!(v.columns.len(), 2);
        assert_ne!(v.active.unwrap().session, "c");
        a.handle_ctl(&["close-column".into()]);
        assert!(a.view().columns.is_empty());
    }

    #[test]
    fn peek_answers_without_switching() {
        let (mut a, d) = app();
        a.handle_ctl(&["add".into(), d.path().display().to_string()]);
        for n in ["a", "b"] {
            a.handle_ctl(&["new".into(), String::new(), n.into(), String::new()]);
        }
        let ids: Vec<String> = a.st.sessions.iter().map(|s| s.id.clone()).collect();
        a.handle_hook(&ids[0], "PermissionRequest", &serde_json::Value::Null);
        key(&mut a, Role::Sidebar, KeyCode::Char('p'));
        let Mode::Peek(pk) = a.view().mode else {
            panic!("not peeking")
        };
        assert_eq!(pk.session, "a");
        assert_eq!(pk.lines, vec!["one", "", "❯ 1. Yes", "  2. No"]);
        key(&mut a, Role::Bar, KeyCode::Tab);
        let Mode::Peek(pk) = a.view().mode else {
            panic!("not peeking")
        };
        assert_eq!(pk.session, "a");
        key(&mut a, Role::Bar, KeyCode::Char('1'));
        typ_role(&mut a, Role::Bar, "use the staging db");
        key(&mut a, Role::Bar, KeyCode::Enter);
        let pane = a.st.session(&ids[0]).unwrap().pane.clone().unwrap();
        let log = a.be.log.borrow().join("\n");
        assert!(log.contains(&format!("key {pane} 1")), "{log}");
        assert!(
            log.contains(&format!("send {pane} \"use the staging db\" true")),
            "{log}"
        );
        assert_eq!(a.view().active.unwrap().session, "b");
        key(&mut a, Role::Bar, KeyCode::Esc);
        assert_eq!(a.view().mode, Mode::Normal);
    }

    #[test]
    fn notifies_once_when_a_hidden_session_needs_you() {
        let (mut a, d) = app();
        a.handle_ctl(&["add".into(), d.path().display().to_string()]);
        for n in ["a", "b"] {
            a.handle_ctl(&["new".into(), String::new(), n.into(), String::new()]);
        }
        let ids: Vec<String> = a.st.sessions.iter().map(|s| s.id.clone()).collect();
        a.handle_hook(&ids[0], "PermissionRequest", &serde_json::Value::Null);
        a.tick();
        let sent: Vec<(Channel, String)> = a
            .outbox
            .drain(..)
            .map(|o| {
                let what = if o.channel == Channel::Chime {
                    o.title
                } else {
                    o.body
                };
                (o.channel, what)
            })
            .collect();
        assert_eq!(
            sent,
            vec![
                (Channel::Chime, "Glass".into()),
                (Channel::Desktop, "a needs you".into()),
                (Channel::Phone, "a needs you".into())
            ]
        );
        a.tick();
        assert!(a.outbox.is_empty());
        a.handle_hook(&ids[0], "PostToolUse", &serde_json::Value::Null);
        a.handle_hook(&ids[0], "PermissionRequest", &serde_json::Value::Null);
        a.handle_ctl(&["waiting".into()]);
        a.tick();
        assert!(a.outbox.is_empty(), "{:?}", a.outbox);
    }

    #[test]
    fn search_resumes_a_conversation_from_outside_pebl() {
        let (mut a, d) = app();
        a.handle_ctl(&["add".into(), d.path().display().to_string()]);
        a.handle_ctl(&["new".into(), String::new(), "a".into(), String::new()]);
        a.index.lock().unwrap().convs.insert(
            PathBuf::from("/x/old.jsonl"),
            crate::search::conv(
                "old-uuid",
                &std::fs::canonicalize(d.path()).unwrap(),
                "Rate limiter rewrite",
                &["move it into redis"],
            ),
        );
        a.handle_ctl(&["search".into()]);
        typ_role(&mut a, Role::Bar, "redis");
        let Mode::Search(sv) = a.view().mode else {
            panic!("not searching")
        };
        assert_eq!(sv.rows.len(), 1);
        assert_eq!(sv.rows[0].src, "cli");
        key(&mut a, Role::Bar, KeyCode::Enter);
        let s =
            a.st.sessions
                .iter()
                .find(|s| s.claude_id == "old-uuid")
                .unwrap();
        assert_eq!(s.name, "Rate limiter rewrite");
        assert_eq!(a.view().active.unwrap().session, "Rate limiter rewrite");
        assert_eq!(a.view().mode, Mode::Normal);
    }

    #[test]
    fn marks_and_sends_one_prompt_to_many() {
        let (mut a, d) = app();
        a.handle_ctl(&["add".into(), d.path().display().to_string()]);
        for n in ["a", "b", "c"] {
            a.handle_ctl(&["new".into(), String::new(), n.into(), String::new()]);
        }
        let ids: Vec<String> = a.st.sessions.iter().map(|s| s.id.clone()).collect();
        a.handle_hook(&ids[0], "UserPromptSubmit", &serde_json::Value::Null);
        a.handle_hook(&ids[2], "PermissionRequest", &serde_json::Value::Null);
        a.handle_ctl(&["mark".into()]);
        key(&mut a, Role::Sidebar, KeyCode::Char('a'));
        assert_eq!(a.view().marking, Some(2));
        key(&mut a, Role::Sidebar, KeyCode::Enter);
        let Mode::Broadcast(b) = a.view().mode else {
            panic!("not composing")
        };
        assert_eq!(b.targets.len(), 2);
        typ_role(&mut a, Role::Bar, "update the readme");
        key(&mut a, Role::Bar, KeyCode::Enter);
        let panes: Vec<String> =
            a.st.sessions
                .iter()
                .map(|s| s.pane.clone().unwrap())
                .collect();
        let log = a.be.log.borrow().join("\n");
        assert!(
            log.contains(&format!("send {} \"update the readme\"", panes[1])),
            "{log}"
        );
        assert!(
            !log.contains(&format!("send {} \"update", panes[0])),
            "{log}"
        );
        a.handle_hook(&ids[0], "Stop", &serde_json::Value::Null);
        a.tick();
        let log = a.be.log.borrow().join("\n");
        assert!(
            log.contains(&format!("send {} \"update the readme\"", panes[0])),
            "{log}"
        );
    }

    #[test]
    fn follows_claude_when_it_switches_conversation() {
        let (mut a, d) = app();
        a.handle_ctl(&["add".into(), d.path().display().to_string()]);
        a.handle_ctl(&["new".into(), String::new(), "a".into(), String::new()]);
        let sid = a.st.sessions[0].id.clone();
        let p = serde_json::json!({"session_id": "new-conversation"});
        a.handle_hook(&sid, "UserPromptSubmit", &p);
        assert_eq!(a.st.sessions[0].claude_id, "new-conversation");
        a.handle_hook(
            &sid,
            "StatusLine",
            &serde_json::json!({"session_id": "later"}),
        );
        assert_eq!(a.st.sessions[0].claude_id, "later");
    }

    #[test]
    fn shell_drawer_toggles_and_runs_claudes_commands() {
        let (mut a, d) = app();
        a.handle_ctl(&["add".into(), d.path().display().to_string()]);
        a.handle_ctl(&["new".into(), String::new(), "a".into(), String::new()]);
        let sid = a.st.sessions[0].id.clone();
        a.handle_ctl(&["shell".into()]);
        let shell = format!("%sh-{sid}");
        assert_eq!(a.drawer_shown.as_deref(), Some(shell.as_str()));
        assert_eq!(a.view().focus, Focus::Shell);
        assert!(a.be.log.borrow().iter().any(|l| l.contains("arrange")));
        a.handle_ctl(&["shell".into()]);
        assert_eq!(a.view().focus, Focus::Main);
        assert!(a.drawer_shown.is_some());
        a.handle_ctl(&["shell".into()]);
        assert!(a.drawer_shown.is_none());

        a.runnable.insert(
            sid.clone(),
            (
                vec!["docker compose up -d redis".into(), "npm run dev".into()],
                0,
            ),
        );
        assert_eq!(a.view().runnable, Some((1, 2)));
        a.handle_ctl(&["run-next".into()]);
        assert_eq!(a.drawer_shown.as_deref(), Some(shell.as_str()));
        let log = a.be.log.borrow().join("\n");
        assert!(
            log.contains(&format!(
                "send {shell} \"docker compose up -d redis\" false"
            )),
            "{log}"
        );
        assert_eq!(a.view().runnable, Some((2, 2)));
        a.handle_ctl(&["drawer-resize".into(), "up".into()]);
        let before = a.view().shell.unwrap().resize.unwrap().1;
        a.handle_input(
            Role::Bar,
            Input::Key(Key {
                shift: true,
                ..Key::plain(KeyCode::Up)
            }),
        );
        assert_eq!(a.view().shell.unwrap().resize.unwrap().1, before + 5);
        key(&mut a, Role::Bar, KeyCode::Enter);
        assert_eq!(a.view().focus, Focus::Shell);
    }

    #[test]
    fn mascot_notice_follows_sessions() {
        let (mut a, d) = app();
        a.handle_ctl(&["add".into(), d.path().display().to_string()]);
        for n in ["a", "b"] {
            a.handle_ctl(&["new".into(), String::new(), n.into(), String::new()]);
        }
        let ids: Vec<String> = a.st.sessions.iter().map(|s| s.id.clone()).collect();
        assert_eq!(a.view().notice.kind, NoticeKind::Idle);
        let bash = serde_json::json!({"tool_name": "Bash"});
        a.handle_hook(&ids[0], "PreToolUse", &bash);
        let n = a.view().notice;
        assert_eq!(
            (n.kind, n.detail.as_str()),
            (NoticeKind::Working, "running Bash")
        );
        a.handle_hook(&ids[0], "Stop", &serde_json::Value::Null);
        let n = a.view().notice;
        assert_eq!((n.kind, n.session.as_str()), (NoticeKind::Done, "a"));
        a.handle_hook(&ids[1], "PermissionRequest", &bash);
        let n = a.view().notice;
        assert_eq!(
            (n.kind, n.detail.as_str()),
            (NoticeKind::Waiting, "wants to run Bash")
        );
        a.handle_ctl(&["waiting".into()]);
        a.handle_hook(&ids[1], "PostToolUse", &serde_json::Value::Null);
        a.handle_hook(&ids[1], "Stop", &serde_json::Value::Null);
        assert_eq!(a.view().notice.kind, NoticeKind::Done);
        a.handle_ctl(&["waiting".into()]);
        assert_eq!(a.view().active.unwrap().session, "a");
        assert_eq!(a.view().notice.kind, NoticeKind::Idle);
    }

    #[test]
    fn kill_confirms_and_moves_to_neighbor() {
        let (mut a, d) = app();
        a.handle_ctl(&["add".into(), d.path().display().to_string()]);
        a.handle_ctl(&["new".into(), String::new(), "a".into(), String::new()]);
        a.handle_ctl(&["new".into(), String::new(), "b".into(), String::new()]);
        key(&mut a, Role::Sidebar, KeyCode::Char('x'));
        assert!(matches!(a.view().mode, Mode::ConfirmKill { ref name } if name == "b"));
        key(&mut a, Role::Sidebar, KeyCode::Char('y'));
        assert_eq!(a.st.sessions.len(), 1);
        assert_eq!(a.view().active.unwrap().session, "a");
    }

    #[test]
    fn exited_session_shows_stage_and_enter_resumes() {
        let (mut a, d) = app();
        a.handle_ctl(&["add".into(), d.path().display().to_string()]);
        a.handle_ctl(&["new".into(), String::new(), "a".into(), String::new()]);
        a.be.panes.borrow_mut()[0].dead = true;
        a.handle_ctl(&["pane-died".into()]);
        assert!(matches!(a.view().stage, Stage::Exited { .. }));
        key(&mut a, Role::Stage, KeyCode::Enter);
        assert!(a.be.log.borrow().iter().any(|l| l.starts_with("respawn")));
    }

    #[test]
    fn filter_narrows_tree() {
        let (mut a, d) = app();
        a.handle_ctl(&["add".into(), d.path().display().to_string()]);
        a.handle_ctl(&["new".into(), String::new(), "auth".into(), String::new()]);
        a.handle_ctl(&["new".into(), String::new(), "docs".into(), String::new()]);
        key(&mut a, Role::Sidebar, KeyCode::Char('/'));
        typ(&mut a, "doc");
        let v = a.view();
        assert_eq!(v.projects[0].sessions.len(), 1);
        assert_eq!(v.projects[0].sessions[0].name, "docs");
    }

    #[test]
    fn ctrl_j_cycles_and_wraps() {
        let (mut a, d) = app();
        a.handle_ctl(&["add".into(), d.path().display().to_string()]);
        a.handle_ctl(&["new".into(), String::new(), "a".into(), String::new()]);
        a.handle_ctl(&["new".into(), String::new(), "b".into(), String::new()]);
        a.handle_ctl(&["next".into()]);
        assert_eq!(a.view().active.unwrap().session, "a");
        a.handle_ctl(&["prev".into()]);
        assert_eq!(a.view().active.unwrap().session, "b");
    }
}
