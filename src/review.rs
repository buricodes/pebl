use crate::diff::{Diff, Kind, Row};
use crate::ui::theme::{ColorMode, Tok};
use crate::ui::{fit, keys, len, seg, segb, Painter, Seg};
use crate::{ipc, paths};
use anyhow::Result;
use crossterm::event::{self, Event, KeyCode, KeyEventKind, KeyModifiers, MouseEventKind};
use crossterm::{cursor, execute, terminal};
use ratatui::backend::CrosstermBackend;
use ratatui::Terminal;
use std::collections::BTreeMap;
use std::path::Path;
use std::time::Duration;

pub struct Review {
    pub title: String,
    pub diff: Diff,
    pub cur: usize,
    pub top: usize,
    pub comments: BTreeMap<usize, String>,
    pub input: Option<String>,
    pub msg: Option<String>,
    armed: bool,
}

#[derive(Clone, Copy, PartialEq)]
enum Vis {
    Row(usize),
    Note(usize),
}

impl Review {
    pub fn new(title: String, diff: Diff) -> Self {
        let cur = diff
            .rows
            .iter()
            .position(|r| matches!(r, Row::Line { .. }))
            .unwrap_or(0);
        Review {
            title,
            diff,
            cur,
            top: 0,
            comments: BTreeMap::new(),
            input: None,
            msg: None,
            armed: false,
        }
    }

    fn visual(&self) -> Vec<Vis> {
        let mut v = Vec::with_capacity(self.diff.rows.len());
        for i in 0..self.diff.rows.len() {
            v.push(Vis::Row(i));
            if self.comments.contains_key(&i) {
                v.push(Vis::Note(i));
            }
        }
        v
    }

    fn step(&mut self, by: i64) {
        let n = self.diff.rows.len() as i64;
        if n == 0 {
            return;
        }
        self.cur = (self.cur as i64 + by).clamp(0, n - 1) as usize;
    }

    fn jump_file(&mut self, forward: bool) {
        let is_file = |r: &Row| matches!(r, Row::File { .. });
        let rows = &self.diff.rows;
        let found = if forward {
            (self.cur + 1..rows.len()).find(|i| is_file(&rows[*i]))
        } else {
            (0..self.cur).rev().find(|i| is_file(&rows[*i]))
        };
        if let Some(i) = found {
            self.cur = (i + 1).min(rows.len() - 1);
        }
    }

    fn line_at(&self, i: usize) -> Option<(usize, &str)> {
        match self.diff.rows.get(i)? {
            Row::Line { new, old, text, .. } => Some(((*new).or(*old)?, text)),
            _ => None,
        }
    }

    pub fn prompt(&self) -> String {
        let mut out = String::from(
            "Here is my review of your changes. Please address each comment, then summarise what you changed.\n",
        );
        for (i, c) in &self.comments {
            let file = self.diff.file_of(*i).unwrap_or("?");
            let (n, code) = self.line_at(*i).unwrap_or((0, ""));
            out.push_str(&format!("\n{file}:{n}\n> {}\n{c}\n", code.trim()));
        }
        out
    }

    pub fn draw(&mut self, p: &mut Painter) {
        let (w, h) = (p.w(), p.h());
        let (adds, dels) = self.diff.totals();
        let right: Vec<Seg> = vec![
            seg(
                match self.diff.files() {
                    1 => "1 file".to_string(),
                    n => format!("{n} files"),
                },
                Tok::Mute,
            ),
            seg(" · ", Tok::Dim),
            seg(format!("+{adds}"), Tok::Ok),
            seg(" ", Tok::Fg),
            seg(format!("−{dels}"), Tok::Err),
            seg(" · ", Tok::Dim),
            seg(format!("vs {}", self.diff.base), Tok::Mute),
        ];
        let rw = crate::ui::seg_len(&right) as i32;
        let x = p.segs(1, 0, &[seg("✻ ", Tok::Acc), segb("Review  ", Tok::Fg)]);
        p.t(
            x,
            0,
            &fit(&self.title, (w - x - rw - 2).max(1) as usize),
            Tok::Mute,
            false,
        );
        p.segs(w - rw - 1, 0, &right);
        p.hl(0, 1, w);

        let body_top = 2;
        let body_h = (h - 4).max(1) as usize;
        let vis = self.visual();
        let at = vis
            .iter()
            .position(|v| *v == Vis::Row(self.cur))
            .unwrap_or(0);
        let note_extra = usize::from(self.comments.contains_key(&self.cur));
        if at < self.top {
            self.top = at;
        } else if at + note_extra >= self.top + body_h {
            self.top = at + note_extra + 1 - body_h;
        }
        if self.diff.rows.is_empty() {
            p.t(2, body_top + 1, "No changes yet.", Tok::Mute, false);
        }
        for (k, v) in vis.iter().skip(self.top).take(body_h).enumerate() {
            let y = body_top + k as i32;
            match *v {
                Vis::Row(i) => self.draw_row(p, i, y, w),
                Vis::Note(i) => {
                    let c = &self.comments[&i];
                    p.t(10, y, "✎ ", Tok::Acc, true);
                    p.t(12, y, &fit(c, (w - 13).max(1) as usize), Tok::Acc, false);
                }
            }
        }

        p.hl(0, h - 2, w);
        if let Some(text) = &self.input {
            let e = p.segs(0, h - 1, &[segb(" comment  ", Tok::Acc)]);
            let e = p.t(
                e,
                h - 1,
                &fit(text, (w - e - 30).max(1) as usize),
                Tok::Fg,
                false,
            );
            p.caret(e, h - 1);
            let hint = keys(&[("enter", "save"), ("esc", "cancel")]);
            p.segs(w - crate::ui::seg_len(&hint) as i32 - 1, h - 1, &hint);
        } else if let Some(m) = &self.msg {
            p.t(1, h - 1, m, Tok::Acc, false);
        } else {
            let n = self.comments.len();
            let mut k = keys(&[
                ("j/k", "move"),
                ("n/N", "file"),
                ("c", "comment"),
                ("x", "delete"),
                ("s", "send"),
                ("q", "close"),
            ]);
            let note = if n == 0 {
                "no comments yet".to_string()
            } else if n == 1 {
                "1 comment".to_string()
            } else {
                format!("{n} comments")
            };
            let room = (w as usize).saturating_sub(len(&note) + 3);
            while crate::ui::seg_len(&k) > room && k.len() > 2 {
                k.truncate(k.len() - 3);
            }
            p.segs(0, h - 1, &k);
            p.rt(
                w - 1,
                h - 1,
                &note,
                if n > 0 { Tok::Acc } else { Tok::Dim },
                n > 0,
            );
        }
    }

    fn draw_row(&self, p: &mut Painter, i: usize, y: i32, w: i32) {
        let sel = i == self.cur;
        match &self.diff.rows[i] {
            Row::File { path, adds, dels } => {
                if sel {
                    p.bg(0, y, w, Tok::Sel);
                }
                let counts = format!("+{adds} −{dels}");
                p.t(
                    2,
                    y,
                    &fit(path, (w - 5 - counts.len() as i32).max(1) as usize),
                    Tok::Fg,
                    true,
                );
                let x = w - 1 - len(&counts) as i32;
                let e = p.t(x, y, &format!("+{adds}"), Tok::Ok, false);
                p.t(e, y, &format!(" −{dels}"), Tok::Err, false);
            }
            Row::Hunk(ctx) => {
                if sel {
                    p.bg(0, y, w, Tok::Sel);
                }
                p.t(
                    11,
                    y,
                    &fit(&format!("⋯ {ctx}"), (w - 12).max(1) as usize),
                    Tok::Dim,
                    false,
                );
            }
            Row::Line {
                kind,
                old,
                new,
                text,
            } => {
                let num = |n: &Option<usize>| n.map(|n| format!("{n:>4}")).unwrap_or("    ".into());
                let (sign, stok, bg) = match kind {
                    Kind::Add => ("+", Tok::Ok, Some(Tok::Add)),
                    Kind::Del => ("-", Tok::Err, Some(Tok::Del)),
                    Kind::Context => (" ", Tok::Dim, None),
                };
                let bg = if sel && bg.is_none() {
                    Some(Tok::Sel)
                } else {
                    bg
                };
                if let Some(bg) = bg {
                    p.bg(10, y, w - 10, bg);
                }
                p.t(1, y, &num(old), Tok::Dim, false);
                p.t(5, y, &num(new), Tok::Dim, false);
                p.t(10, y, sign, stok, false);
                p.t(12, y, &fit(text, (w - 13).max(1) as usize), Tok::Fg, false);
            }
        }
        if sel {
            p.t(0, y, "›", Tok::Acc, true);
        }
    }

    fn key(&mut self, code: KeyCode, mods: KeyModifiers, page: i64, sid: &str) -> bool {
        self.msg = None;
        if let Some(text) = &mut self.input {
            match code {
                KeyCode::Esc => self.input = None,
                KeyCode::Enter => {
                    let t = text.trim().to_string();
                    if t.is_empty() {
                        self.comments.remove(&self.cur);
                    } else {
                        self.comments.insert(self.cur, t);
                    }
                    self.input = None;
                }
                KeyCode::Backspace => {
                    text.pop();
                }
                KeyCode::Char('u') if mods.contains(KeyModifiers::CONTROL) => text.clear(),
                KeyCode::Char(c) => text.push(c),
                _ => {}
            }
            return false;
        }
        let ctrl = mods.contains(KeyModifiers::CONTROL);
        match code {
            KeyCode::Char('q') | KeyCode::Esc => {
                if self.comments.is_empty() || self.armed {
                    return true;
                }
                self.armed = true;
                self.msg = Some("you have unsent comments: s sends them, q again discards".into());
                return false;
            }
            KeyCode::Char('c') if ctrl => return true,
            KeyCode::Char('j') | KeyCode::Down => self.step(1),
            KeyCode::Char('k') | KeyCode::Up => self.step(-1),
            KeyCode::Char('d') if ctrl => self.step(page / 2),
            KeyCode::Char('u') if ctrl => self.step(-page / 2),
            KeyCode::Char(' ') | KeyCode::PageDown | KeyCode::Char('J') => self.step(page),
            KeyCode::PageUp | KeyCode::Char('K') => self.step(-page),
            KeyCode::Char('g') | KeyCode::Home => self.cur = 0,
            KeyCode::Char('G') | KeyCode::End => self.step(i64::MAX / 2),
            KeyCode::Char('n') | KeyCode::Char(']') => self.jump_file(true),
            KeyCode::Char('N') | KeyCode::Char('[') => self.jump_file(false),
            KeyCode::Char('c') | KeyCode::Enter => {
                if self.line_at(self.cur).is_some() {
                    self.input = Some(self.comments.get(&self.cur).cloned().unwrap_or_default());
                } else {
                    self.msg = Some("move to a changed line to comment on it".into());
                }
            }
            KeyCode::Char('x') => {
                self.comments.remove(&self.cur);
            }
            KeyCode::Char('s') => {
                if self.comments.is_empty() {
                    self.msg = Some("add a comment first (c)".into());
                    return false;
                }
                let sent = ipc::ctl(&paths::sock(), &["reply".into(), sid.into(), self.prompt()]);
                match sent {
                    Ok(out) if !out.starts_with("error") => return true,
                    Ok(out) => self.msg = Some(out),
                    Err(e) => self.msg = Some(format!("{e:#}")),
                }
            }
            _ => {}
        }
        self.armed = false;
        false
    }
}

pub fn run(sid: &str, cwd: &Path, title: &str, color: ColorMode) -> Result<()> {
    let diff = match crate::diff::load(cwd) {
        Ok(d) => d,
        Err(e) => Diff {
            base: format!("{e:#}"),
            rows: Vec::new(),
        },
    };
    let mut r = Review::new(title.to_string(), diff);
    terminal::enable_raw_mode()?;
    execute!(
        std::io::stdout(),
        terminal::EnterAlternateScreen,
        event::EnableMouseCapture,
        cursor::Hide
    )?;
    let res = (|| -> Result<()> {
        let mut term = Terminal::new(CrosstermBackend::new(std::io::stdout()))?;
        loop {
            let mut page = 10;
            term.draw(|f| {
                let area = f.area();
                page = area.height.saturating_sub(5).max(1) as i64;
                let mut p = Painter::new(f.buffer_mut(), area, color);
                r.draw(&mut p);
            })?;
            if !event::poll(Duration::from_millis(500))? {
                continue;
            }
            match event::read()? {
                Event::Key(k) if k.kind != KeyEventKind::Release => {
                    if r.key(k.code, k.modifiers, page, sid) {
                        return Ok(());
                    }
                }
                Event::Mouse(m) => match m.kind {
                    MouseEventKind::ScrollDown => r.step(3),
                    MouseEventKind::ScrollUp => r.step(-3),
                    _ => {}
                },
                _ => {}
            }
        }
    })();
    let _ = execute!(
        std::io::stdout(),
        cursor::Show,
        event::DisableMouseCapture,
        terminal::LeaveAlternateScreen
    );
    let _ = terminal::disable_raw_mode();
    res
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::buffer::Buffer;
    use ratatui::layout::Rect;

    fn sample() -> Review {
        let d = crate::diff::parse(
            "diff --git a/src/middleware/auth.ts b/src/middleware/auth.ts\n--- a/src/middleware/auth.ts\n+++ b/src/middleware/auth.ts\n@@ -36,6 +36,7 @@ export async function auth(req, res, next) {\n   const token = req.headers.authorization?.slice(7)\n-  const raw = await redis.get(`sess:${token}`)\n-  const session = raw ? JSON.parse(raw) : null\n+  const session = flags.USE_LEGACY_SESSIONS\n+    ? await legacyLookup(token)\n+    : await sessionStore.get(token)\n   if (!session) return res.status(401).end()\n",
        );
        let mut r = Review::new(
            "nova-api / auth-refactor".into(),
            Diff {
                base: "main".into(),
                ..d
            },
        );
        r.cur = 5;
        r.comments.insert(
            5,
            "keep the legacy path behind the flag until next sprint".into(),
        );
        r
    }

    #[test]
    fn review_screen() {
        let mut r = sample();
        let area = Rect::new(0, 0, 100, 16);
        let mut buf = Buffer::empty(area);
        r.draw(&mut Painter::new(&mut buf, area, ColorMode::Truecolor));
        insta::assert_snapshot!(crate::ui::screen::to_text(&buf));
    }

    #[test]
    fn prompt_lists_comments_with_location() {
        let r = sample();
        let p = r.prompt();
        assert!(p.contains("src/middleware/auth.ts:37\n> const session = flags.USE_LEGACY_SESSIONS\nkeep the legacy path"), "{p}");
    }
}
