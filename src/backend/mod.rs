pub mod tmux;

use anyhow::Result;
use std::path::PathBuf;

#[derive(Clone, Debug, PartialEq)]
pub struct LivePane {
    pub id: String,
    pub sid: Option<String>,
    pub dead: bool,
    pub dead_status: Option<i32>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct SpawnSpec {
    pub sid: String,
    pub cwd: PathBuf,
    pub argv: Vec<String>,
    pub env: Vec<(String, String)>,
    pub size: (u16, u16),
}

#[derive(Clone, Debug, PartialEq)]
pub struct UiPanes {
    pub sidebar: String,
    pub stage: String,
    pub bar: String,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Arrangement {
    pub win: (u16, u16),
    pub sidebar: Option<(String, u16)>,
    pub tabs: Option<String>,
    pub cols: Vec<(String, u16)>,
    pub drawer: Option<(String, u16)>,
    pub bar: (String, u16),
}

#[derive(Clone, Debug, PartialEq)]
pub struct ShellPane {
    pub pane: String,
    pub sid: String,
    pub command: String,
    pub pid: u32,
}

impl Arrangement {
    pub fn order(&self) -> Vec<String> {
        let mut o = Vec::new();
        if let Some((s, _)) = &self.sidebar {
            o.push(s.clone());
        }
        if let Some(t) = &self.tabs {
            o.push(t.clone());
        }
        for (i, (p, _)) in self.cols.iter().enumerate() {
            o.push(p.clone());
            if i == 0 {
                if let Some((d, _)) = &self.drawer {
                    o.push(d.clone());
                }
            }
        }
        o.push(self.bar.0.clone());
        o
    }

    pub fn layout(&self) -> String {
        let (w, h) = self.win;
        let bh = self.bar.1.min(h.saturating_sub(4)).max(1);
        let th = h - bh - 1;
        let leaf = |pane: &str, w: u16, h: u16, x: u16, y: u16| {
            format!("{w}x{h},{x},{y},{}", pane.trim_start_matches('%'))
        };
        let (mx, mw) = match &self.sidebar {
            Some((_, sw)) => (sw + 1, w.saturating_sub(sw + 1)),
            None => (0, w),
        };
        let cell = |i: usize, p: &str, cw: u16, rh: u16, x: u16, ry: u16| match (i, &self.drawer) {
            (0, Some((d, dh))) => {
                let dh = (*dh).clamp(1, rh.saturating_sub(7).max(1));
                let ch = rh - dh - 1;
                format!(
                    "{cw}x{rh},{x},{ry}[{},{}]",
                    leaf(p, cw, ch, x, ry),
                    leaf(d, cw, dh, x, ry + ch + 1)
                )
            }
            _ => leaf(p, cw, rh, x, ry),
        };
        let main = if self.cols.len() == 1 && self.tabs.is_none() {
            cell(0, &self.cols[0].0, mw, th, mx, 0)
        } else {
            let ry = if self.tabs.is_some() { 2 } else { 0 };
            let rh = th - ry;
            let mut x = mx;
            let mut cells = Vec::new();
            for (i, (p, cw)) in self.cols.iter().enumerate() {
                let cw = if i + 1 == self.cols.len() {
                    (mx + mw).saturating_sub(x)
                } else {
                    *cw
                };
                cells.push(cell(i, p, cw, rh, x, ry));
                x += cw + 1;
            }
            let row = if cells.len() == 1 {
                cells.remove(0)
            } else {
                format!("{mw}x{rh},{mx},{ry}{{{}}}", cells.join(","))
            };
            match &self.tabs {
                Some(t) => format!("{mw}x{th},{mx},0[{},{row}]", leaf(t, mw, 1, mx, 0)),
                None => row,
            }
        };
        let top = match &self.sidebar {
            Some((s, sw)) => format!("{w}x{th},0,0{{{},{main}}}", leaf(s, *sw, th, 0, 0)),
            None => main,
        };
        let body = format!("{w}x{h},0,0[{top},{}]", leaf(&self.bar.0, w, bh, 0, th + 1));
        format!("{:04x},{body}", checksum(&body))
    }
}

pub fn checksum(s: &str) -> u16 {
    let mut c: u16 = 0;
    for b in s.bytes() {
        c = (c >> 1) | ((c & 1) << 15);
        c = c.wrapping_add(b as u16);
    }
    c
}

pub trait SessionBackend {
    fn spawn(&self, spec: &SpawnSpec) -> Result<String>;
    fn respawn(&self, pane: &str, spec: &SpawnSpec) -> Result<()>;
    fn kill(&self, pane: &str) -> Result<()>;
    fn list(&self) -> Result<Vec<LivePane>>;
    fn capture(&self, pane: &str) -> Result<String>;
    fn spawn_shell(&self, sid: &str, cwd: &std::path::Path, size: (u16, u16)) -> Result<String>;
    fn shells(&self) -> Result<Vec<ShellPane>>;
    fn child_command(&self, pid: u32) -> Option<String>;
    fn send_text(&self, pane: &str, text: &str, enter: bool) -> Result<()>;
    fn send_key(&self, pane: &str, key: &str) -> Result<()>;
}

pub trait UiBackend {
    fn create_ui(&self, pebl: &str, size: (u16, u16), sidebar_w: u16) -> Result<UiPanes>;
    fn ui_panes(&self) -> Result<Option<UiPanes>>;
    fn repair_ui(
        &self,
        pebl: &str,
        main: Option<&str>,
        size: (u16, u16),
        sidebar_w: u16,
    ) -> Result<(UiPanes, String)>;
    fn slot(&self) -> Result<Option<String>>;
    fn window_size(&self) -> Result<(u16, u16)>;
    fn show(&self, pane: &str, current: &str) -> Result<()>;
    fn select(&self, pane: &str) -> Result<()>;
    fn set_header(&self, lines: &[String]) -> Result<()>;
    fn size_parked(&self, size: (u16, u16)) -> Result<()>;
    fn arrange(&self, a: &Arrangement) -> Result<()>;
    fn tabs_pane(&self, pebl: &str) -> Result<String>;
    fn widths(&self) -> Result<Vec<(String, u16)>>;
    fn heights(&self) -> Result<Vec<(String, u16)>>;
    fn set_flag(&self, name: &str, on: bool) -> Result<()>;
    fn set_split(&self, on: bool, accent: Option<&str>) -> Result<()>;
    fn popup(&self, cwd: &std::path::Path, title: &str, cmd: &str) -> Result<()>;
}
