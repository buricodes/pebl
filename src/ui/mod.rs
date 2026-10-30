pub mod bar;
pub mod header;
pub mod mascot;
pub mod screen;
pub mod sidebar;
pub mod stage;
pub mod tabs;
pub mod theme;

use crate::model::Status;
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::Modifier;
use theme::{ColorMode, Tok};

pub type Seg = (String, Tok, bool);

pub fn seg(t: impl Into<String>, tok: Tok) -> Seg {
    (t.into(), tok, false)
}

pub fn segb(t: impl Into<String>, tok: Tok) -> Seg {
    (t.into(), tok, true)
}

pub fn seg_len(segs: &[Seg]) -> usize {
    segs.iter().map(|s| len(&s.0)).sum()
}

pub fn len(s: &str) -> usize {
    s.chars().count()
}

pub fn gauge(pct: u8) -> (String, Tok, bool) {
    const G: [char; 9] = [' ', '▁', '▂', '▃', '▄', '▅', '▆', '▇', '█'];
    let i = ((pct as f32) / 12.5).ceil().clamp(1.0, 8.0) as usize;
    (G[i].to_string(), ctx_tok(pct), pct > 95)
}

pub fn ctx_tok(pct: u8) -> Tok {
    if pct > 95 {
        Tok::Err
    } else if pct > 80 {
        Tok::Acc
    } else {
        Tok::Mute
    }
}

pub fn fit(s: &str, n: usize) -> String {
    if len(s) <= n {
        return s.to_string();
    }
    if n == 0 {
        return String::new();
    }
    let mut out: String = s.chars().take(n - 1).collect();
    out.push('…');
    out
}

pub fn fit_tail(s: &str, n: usize) -> String {
    let l = len(s);
    if l <= n {
        return s.to_string();
    }
    if n == 0 {
        return String::new();
    }
    let mut out = String::from("…");
    out.extend(s.chars().skip(l - (n - 1)));
    out
}

pub fn glyph(st: Status) -> (&'static str, Tok, bool) {
    match st {
        Status::Running => ("●", Tok::Ok, false),
        Status::Idle => ("○", Tok::Mute, false),
        Status::Waiting => ("◆", Tok::Acc, true),
        Status::Error => ("✕", Tok::Err, false),
        Status::Exited => ("·", Tok::Dim, false),
    }
}

pub struct Painter<'a> {
    pub buf: &'a mut Buffer,
    pub area: Rect,
    pub mode: ColorMode,
}

impl<'a> Painter<'a> {
    pub fn new(buf: &'a mut Buffer, area: Rect, mode: ColorMode) -> Self {
        Painter { buf, area, mode }
    }

    pub fn w(&self) -> i32 {
        self.area.width as i32
    }

    pub fn h(&self) -> i32 {
        self.area.height as i32
    }

    fn cell(&mut self, x: i32, y: i32) -> Option<&mut ratatui::buffer::Cell> {
        if x < 0 || y < 0 || x >= self.w() || y >= self.h() {
            return None;
        }
        self.buf
            .cell_mut((self.area.x + x as u16, self.area.y + y as u16))
    }

    pub fn t(&mut self, x: i32, y: i32, s: &str, tok: Tok, bold: bool) -> i32 {
        let fg = tok.color(self.mode);
        let mut xx = x;
        for ch in s.chars() {
            if let Some(c) = self.cell(xx, y) {
                c.set_char(ch);
                c.fg = fg;
                if bold {
                    c.modifier.insert(Modifier::BOLD);
                } else {
                    c.modifier.remove(Modifier::BOLD);
                }
            }
            xx += 1;
        }
        xx
    }

    pub fn rt(&mut self, xe: i32, y: i32, s: &str, tok: Tok, bold: bool) -> i32 {
        self.t(xe - len(s) as i32, y, s, tok, bold)
    }

    pub fn segs(&mut self, mut x: i32, y: i32, segs: &[Seg]) -> i32 {
        for (t, tok, b) in segs {
            x = self.t(x, y, t, *tok, *b);
        }
        x
    }

    pub fn bg(&mut self, x: i32, y: i32, w: i32, tok: Tok) {
        let bg = tok.color(self.mode);
        for i in 0..w {
            if let Some(c) = self.cell(x + i, y) {
                c.bg = bg;
            }
        }
    }

    pub fn caret(&mut self, x: i32, y: i32) {
        let fg = Tok::Fg.color(self.mode);
        if let Some(c) = self.cell(x, y) {
            c.set_char(' ');
            c.bg = fg;
        }
    }

    pub fn hl(&mut self, x: i32, y: i32, w: i32) {
        self.t(x, y, &"─".repeat(w.max(0) as usize), Tok::Line, false);
    }

    pub fn vl(&mut self, x: i32, y: i32, h: i32) {
        for i in 0..h {
            self.t(x, y + i, "│", Tok::Line, false);
        }
    }

    pub fn boxed(&mut self, x: i32, y: i32, w: i32, h: i32, tok: Tok, round: bool) {
        let c = if round {
            ['╭', '╮', '╰', '╯']
        } else {
            ['┌', '┐', '└', '┘']
        };
        let inner = "─".repeat((w - 2).max(0) as usize);
        self.t(x, y, &format!("{}{inner}{}", c[0], c[1]), tok, false);
        for i in 1..h - 1 {
            self.t(x, y + i, "│", tok, false);
            self.t(x + w - 1, y + i, "│", tok, false);
        }
        self.t(
            x,
            y + h - 1,
            &format!("{}{inner}{}", c[2], c[3]),
            tok,
            false,
        );
    }
}

pub fn keys(list: &[(&str, &str)]) -> Vec<Seg> {
    let mut o = vec![seg(" ", Tok::Fg)];
    for (i, (k, l)) in list.iter().enumerate() {
        if i > 0 {
            o.push(seg(" · ", Tok::Dim));
        }
        o.push(seg(*k, Tok::Fg));
        if !l.is_empty() {
            o.push(seg(format!(" {l}"), Tok::Mute));
        }
    }
    o
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fit_truncates_with_ellipsis() {
        assert_eq!(fit("onboarding-flow", 8), "onboard…");
        assert_eq!(fit("dns", 8), "dns");
    }

    #[test]
    fn fit_tail_keeps_the_end() {
        assert_eq!(fit_tail("hey what's up so how", 8), "… so how");
        assert_eq!(fit_tail("dns", 8), "dns");
    }
}
