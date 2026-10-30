use super::theme::Tok;
use super::{fit, glyph, len, mascot, Painter};
use crate::view::{Cursor, Layout, Mode, View};

#[derive(Clone, Copy, PartialEq)]
enum Pet {
    Card,
    Line,
    None,
}

fn usage_h(v: &View) -> i32 {
    if v.limits.is_empty() && v.today == (0, 0) {
        0
    } else if matches!(v.layout, Layout::Compact(_)) {
        3
    } else {
        2
    }
}

fn bottom(v: &View, h: i32) -> i32 {
    match usage_h(v) {
        0 => h - 3,
        n => h - 2 - n,
    }
}

pub fn pet_rows(v: &View, h: u16) -> Option<(u16, u16)> {
    mascot::current(v)?;
    let b = bottom(v, h as i32);
    let full = !matches!(v.layout, Layout::Compact(_));
    if full && b >= 2 + mascot::CARD_H {
        Some(((b - mascot::CARD_H) as u16, (b - 1) as u16))
    } else {
        let y = (b - 1).max(0) as u16;
        Some((y, y))
    }
}

fn usage_block(v: &View, p: &mut Painter, y: i32, w: i32, compact: bool) {
    use super::{ctx_tok, seg, Seg};
    use crate::model::short_count;
    p.hl(0, y, w);
    let mut lim: Vec<Seg> = Vec::new();
    for (i, l) in v.limits.iter().enumerate() {
        if i > 0 {
            lim.push(seg(" · ", Tok::Dim));
        }
        let label = if compact && l.label == "week" {
            "wk"
        } else {
            l.label.as_str()
        };
        lim.push(seg(format!("{label} "), Tok::Dim));
        lim.push((format!("{}%", l.pct), ctx_tok(l.pct), l.pct > 95));
    }
    let today = vec![
        seg("today", Tok::Mute),
        seg(
            format!("  ↑{} ↓{}", short_count(v.today.0), short_count(v.today.1)),
            Tok::Dim,
        ),
    ];
    let reset = v.limits.first().map(|l| format!("↺ {}", l.resets));
    let line1 = if lim.is_empty() { today.clone() } else { lim };
    if compact {
        p.segs(1, y + 1, &line1);
        if v.limits.is_empty() {
            return;
        }
        if let Some(r) = reset {
            p.t(1, y + 2, &r, Tok::Dim, false);
        }
        return;
    }
    let e = p.segs(1, y + 1, &line1);
    if let Some(r) = reset.filter(|_| !v.limits.is_empty()) {
        if e + len(&r) as i32 + 2 < w {
            p.rt(w - 1, y + 1, &r, Tok::Dim, false);
        }
    }
}

enum Row<'a> {
    Project(usize),
    Session(usize, usize),
    NewSession(&'a str),
    Hint(&'a str),
    Blank,
}

fn rows(v: &View) -> Vec<Row<'_>> {
    let mut rows: Vec<Row> = Vec::new();
    for (pi, pr) in v.projects.iter().enumerate() {
        rows.push(Row::Project(pi));
        if pr.open {
            for si in 0..pr.sessions.len() {
                rows.push(Row::Session(pi, si));
            }
            if let Mode::NewSession { project, text, .. } = &v.mode {
                if *project == pi {
                    rows.push(Row::NewSession(text));
                    rows.push(Row::Hint("enter to start"));
                }
            }
        }
        rows.push(Row::Blank);
    }
    rows
}

struct Fit {
    pet: Option<(&'static mascot::Character, Pet)>,
    room: usize,
    scroll: usize,
}

fn fit_rows(v: &View, rows: &[Row], w: i32, h: i32) -> Fit {
    let compact = matches!(v.layout, Layout::Compact(_));
    let top = 1;
    let base = bottom(v, h);
    let pet = mascot::current(v).map(|c| {
        let need = top + rows.len() as i32;
        if !compact && w >= 24 && need < base - mascot::CARD_H {
            (c, Pet::Card)
        } else if need < base {
            (c, Pet::Line)
        } else {
            (c, Pet::None)
        }
    });
    let limit = match pet {
        Some((_, Pet::Card)) => base - mascot::CARD_H - 1,
        Some((_, Pet::Line)) => base - 1,
        _ => base,
    };
    let room = (limit - top).max(1) as usize;
    let focus_row = rows
        .iter()
        .position(|r| match (r, v.cursor) {
            (Row::NewSession(_), _) => true,
            (Row::Project(a), Some(Cursor::Project(b))) => *a == b,
            (Row::Session(a, c), Some(Cursor::Session(b, d))) => *a == b && *c == d,
            _ => false,
        })
        .unwrap_or(0);
    let scroll = (focus_row + 2).saturating_sub(room);
    Fit { pet, room, scroll }
}

pub fn list_row(v: &View, w: u16, h: u16, y: u16) -> Option<u16> {
    let f = fit_rows(v, &rows(v), w as i32, h as i32);
    (y >= 1 && (y as usize) <= f.room).then_some(y + f.scroll as u16)
}

pub fn render(v: &View, p: &mut Painter, frame: u64) {
    let w = p.w();
    let h = p.h();
    let compact = matches!(v.layout, Layout::Compact(_));
    let rows = rows(v);
    let top = 1;
    let base = bottom(v, h);
    let Fit { pet, room, scroll } = fit_rows(v, &rows, w, h);

    if v.projects.is_empty() {
        let msg = if v.filter.is_empty() {
            "No projects yet."
        } else {
            "No matches."
        };
        p.t(1, top, msg, Tok::Mute, false);
    }

    for (i, row) in rows.iter().enumerate().skip(scroll) {
        let y = top + (i - scroll) as i32;
        if y >= top + room as i32 {
            break;
        }
        match *row {
            Row::Project(pi) => {
                let pr = &v.projects[pi];
                let sel = v.cursor == Some(Cursor::Project(pi));
                if sel {
                    p.bg(0, y, w, Tok::Sel);
                }
                p.t(
                    1,
                    y,
                    if pr.open { "▾" } else { "▸" },
                    if sel { Tok::Acc } else { Tok::Mute },
                    sel,
                );
                p.t(3, y, &fit(&pr.name, (w - 9).max(1) as usize), Tok::Fg, true);
                if pr.waiting > 0 {
                    p.rt(w - 1, y, &format!("◆ {}", pr.waiting), Tok::Acc, true);
                } else {
                    p.rt(w - 1, y, &pr.total.to_string(), Tok::Dim, false);
                }
            }
            Row::Session(pi, si) => {
                let s = &v.projects[pi].sessions[si];
                let sel = v.cursor == Some(Cursor::Session(pi, si));
                let wt = s.status == crate::model::Status::Waiting;
                if sel {
                    p.bg(0, y, w, Tok::Sel);
                    p.t(1, y, "›", Tok::Acc, true);
                }
                let xo = if let Some(m) = s.marked {
                    if m {
                        p.t(3, y, "✓", Tok::Acc, true);
                    } else {
                        p.t(3, y, "·", Tok::Dim, false);
                    }
                    2
                } else {
                    0
                };
                let (g, gt, gb) = glyph(s.status);
                p.t(3 + xo, y, g, gt, gb);
                let renaming = match &v.mode {
                    Mode::Rename {
                        cursor: Cursor::Session(a, b),
                        text,
                    } if *a == pi && *b == si => Some(text),
                    _ => None,
                };
                if let Some(text) = renaming {
                    let e = p.t(5 + xo, y, text, Tok::Fg, true);
                    p.caret(e, y);
                    continue;
                }
                let g = s.ctx.is_some();
                let mw = if s.col.is_some() {
                    4
                } else if compact {
                    0
                } else if g {
                    7
                } else {
                    (len(&s.meta) + 1).max(6) as i32
                };
                let mw = mw + if g { 2 } else { 0 };
                if let Some(c) = s.ctx {
                    let gx = match (compact, s.col.is_some()) {
                        (true, true) => w - 6,
                        (true, false) => w - 2,
                        _ => w - 9,
                    };
                    let (gc, gt, gb) = super::gauge(c);
                    p.t(gx, y, &gc, gt, gb);
                }
                let idle = matches!(
                    s.status,
                    crate::model::Status::Idle | crate::model::Status::Exited
                );
                let tok = if idle && !sel { Tok::Mute } else { Tok::Fg };
                p.t(
                    5 + xo,
                    y,
                    &fit(&s.name, (w - 6 - mw - xo).max(1) as usize),
                    tok,
                    sel || wt,
                );
                if let Some(n) = s.col {
                    let lit = wt || n == v.col_focus + 1;
                    let tok = if lit { Tok::Acc } else { Tok::Mute };
                    p.rt(w - 1, y, &format!("[{n}]"), tok, lit);
                } else if !compact && !s.meta.is_empty() {
                    let mt = match s.status {
                        crate::model::Status::Waiting => Tok::Acc,
                        crate::model::Status::Error => Tok::Err,
                        _ => Tok::Dim,
                    };
                    p.rt(w - 1, y, &s.meta, mt, wt);
                }
            }
            Row::NewSession(text) => {
                p.bg(0, y, w, Tok::Sel);
                p.t(1, y, "›", Tok::Acc, true);
                p.t(3, y, "+", Tok::Acc, true);
                let editing_name = !matches!(
                    &v.mode,
                    Mode::NewSession {
                        editing_prompt: true,
                        ..
                    }
                );
                let e = p.t(5, y, &fit(text, (w - 7).max(1) as usize), Tok::Fg, true);
                if editing_name {
                    p.caret(e, y);
                }
            }
            Row::Hint(t) => {
                p.t(5, y, t, Tok::Dim, false);
            }
            Row::Blank => {}
        }
    }

    match pet {
        Some((c, Pet::Card)) => {
            let ry = base - mascot::CARD_H - 1;
            p.hl(0, ry, w);
            mascot::card(p, c, &v.notice, frame, 1, ry + 1, w);
        }
        Some((c, Pet::Line)) => mascot::compact(p, c, &v.notice, frame, 1, base - 1, w),
        _ => {}
    }
    if usage_h(v) > 0 {
        usage_block(v, p, base, w, compact);
    }

    let short = compact;
    if !v.projects.is_empty() || !v.filter.is_empty() {
        p.t(1, h - 2, "+ new session", Tok::Mute, false);
        p.rt(
            w - 1,
            h - 2,
            if short { "^n" } else { "ctrl+n" },
            Tok::Dim,
            false,
        );
    }
    p.t(1, h - 1, "+ new project", Tok::Mute, false);
    p.rt(
        w - 1,
        h - 1,
        if short { "^o" } else { "ctrl+o" },
        Tok::Dim,
        false,
    );
}

pub fn hit(v: &View, y: u16) -> Option<Cursor> {
    let mut row = 1u16;
    for (pi, pr) in v.projects.iter().enumerate() {
        if row == y {
            return Some(Cursor::Project(pi));
        }
        row += 1;
        if pr.open {
            for si in 0..pr.sessions.len() {
                if row == y {
                    return Some(Cursor::Session(pi, si));
                }
                row += 1;
            }
        }
        row += 1;
    }
    None
}
