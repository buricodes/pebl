use super::theme::Tok;
use super::{fit, glyph, len, Painter};
use crate::view::View;

pub fn render(v: &View, p: &mut Painter) {
    let mut x = 0;
    let last = v.columns.len().saturating_sub(1);
    for (i, c) in v.columns.iter().enumerate() {
        let w = c.width as i32;
        let focused = i == v.col_focus;
        let (cx, cw) = (x + 1, w - 2);
        let num = format!("{} ", i + 1);
        let mut e = p.t(
            cx,
            0,
            &num,
            if focused { Tok::Acc } else { Tok::Dim },
            focused,
        );
        let pc = c.ctx.map(|p| format!("{p}%"));
        let extra = pc.as_ref().map(|s| len(s) as i32 + 3).unwrap_or(0);
        let room = (cw - len(&num) as i32 - 5 - extra).max(1) as usize;
        let full = if c.session.is_empty() {
            c.project.clone()
        } else {
            format!("{} / {}", c.project, c.session)
        };
        let name = fit(&full, room);
        match name.find(" / ") {
            Some(cut) if !c.session.is_empty() => {
                e = p.t(e, 0, &name[..cut], Tok::Mute, false);
                e = p.t(e, 0, " / ", Tok::Dim, false);
                p.t(e, 0, &name[cut + 3..], Tok::Fg, true);
            }
            _ => {
                p.t(e, 0, &name, Tok::Fg, true);
            }
        }
        if let (Some(pct), Some(label)) = (c.ctx, &pc) {
            let rx = x + w - 1;
            let ps = rx - 5 - len(label) as i32;
            let (g, tok, b) = super::gauge(pct);
            p.t(ps - 2, 0, &g, tok, b);
            p.t(ps, 0, label, tok, b);
        }
        if let Some(st) = c.status {
            let (g, tok, bold) = glyph(st);
            p.t(cx + cw - 3, 0, g, tok, bold);
        }
        p.t(cx + cw - 1, 0, "×", Tok::Dim, false);
        if i < last {
            if v.resizing == Some(i) {
                p.t(x + w, 0, "┃", Tok::Acc, true);
            } else {
                p.t(x + w, 0, "│", Tok::Line, false);
            }
        }
        x += w + 1;
    }
}
