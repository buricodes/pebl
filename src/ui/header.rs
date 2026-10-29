use super::theme::{ColorMode, Tok};
use super::{glyph, len, seg, seg_len, segb, Seg};
use crate::model::Status;
use crate::view::{Layout, Mode, View};

pub fn segments(v: &View, width: usize) -> Vec<Seg> {
    let mut line: Vec<Seg> = Vec::new();
    let (x, hw) = match v.layout.sidebar_width() {
        Some(sw) => {
            let sw = sw as usize;
            match v.marking {
                Some(m) => {
                    let n = m.to_string();
                    line.push(segb(" marking", Tok::Acc));
                    pad(&mut line, sw.saturating_sub(1 + len(&n)));
                    line.push(segb(n, Tok::Acc));
                }
                None => {
                    let n = v.projects.len().to_string();
                    line.push(seg(" projects", Tok::Mute));
                    pad(&mut line, sw.saturating_sub(1 + len(&n)));
                    line.push(seg(n, Tok::Dim));
                }
            }
            pad(&mut line, sw);
            line.push(seg("│", Tok::Line));
            (sw + 2, width.saturating_sub(sw + 4))
        }
        None => (2, width.saturating_sub(4)),
    };
    pad(&mut line, x);

    let (left, right) = main_header(v);
    let rl = seg_len(&right);
    let room = hw.saturating_sub(rl + 2);
    let mut used = 0;
    for (t, tok, b) in left {
        let take = room.saturating_sub(used);
        if take == 0 {
            break;
        }
        let t = super::fit(&t, take);
        used += len(&t);
        line.push((t, tok, b));
    }
    pad(&mut line, x + hw - rl);
    line.extend(right);
    pad(&mut line, width);
    line
}

pub fn usage_line(v: &View, width: usize) -> Option<Vec<Seg>> {
    use crate::model::short_count;
    let u = v.usage.as_ref()?;
    let mut line: Vec<Seg> = Vec::new();
    let (x, hw) = match v.layout.sidebar_width() {
        Some(sw) => {
            let sw = sw as usize;
            pad(&mut line, sw);
            line.push(seg("│", Tok::Line));
            (sw + 2, width.saturating_sub(sw + 4))
        }
        None => (2, width.saturating_sub(4)),
    };
    pad(&mut line, x);
    let compact = v.layout.short_keys();
    let mut left: Vec<Seg> = Vec::new();
    if !u.model.is_empty() {
        left.push(seg(u.model.clone(), Tok::Fg));
    }
    let mut right: Vec<Seg> = Vec::new();
    if let Some(p) = u.pct {
        let (g, tok, b) = super::gauge(p);
        if compact {
            left.push(seg("  ", Tok::Fg));
        } else {
            left.push(seg("  ctx ", Tok::Dim));
        }
        left.push((g, tok, b));
        if !compact && u.size > 0 {
            left.push((
                format!(" {}/{}", short_count(u.used), short_count(u.size)),
                tok,
                false,
            ));
        }
        left.push((format!(" {p}%"), tok, true));
        if p > 95 {
            right.push(seg(
                if compact {
                    "run /compact"
                } else {
                    "nearly full · /compact"
                },
                Tok::Err,
            ));
        } else if p > 80 && !compact {
            right.push(seg("getting full", Tok::Acc));
        }
    }
    if !compact && u.tok_in + u.tok_out > 0 {
        left.push(seg(
            format!(
                "   ↑{} in · ↓{} out",
                short_count(u.tok_in),
                short_count(u.tok_out)
            ),
            Tok::Mute,
        ));
    }
    if left.is_empty() {
        return None;
    }
    let rl = seg_len(&right);
    let room = hw.saturating_sub(rl + 2);
    let mut used = 0;
    for (t, tok, b) in left {
        let take = room.saturating_sub(used);
        if take == 0 {
            break;
        }
        let t = super::fit(&t, take);
        used += len(&t);
        line.push((t, tok, b));
    }
    pad(&mut line, x + hw - rl);
    line.extend(right);
    pad(&mut line, width);
    Some(line)
}

fn main_header(v: &View) -> (Vec<Seg>, Vec<Seg>) {
    let compact = v.layout.short_keys();
    if let Mode::NewSession { project, text, .. } = &v.mode {
        let pname = v
            .projects
            .get(*project)
            .map(|p| p.name.clone())
            .unwrap_or_default();
        let sname = if text.is_empty() {
            "new session".to_string()
        } else {
            text.clone()
        };
        return (
            vec![
                seg(pname, Tok::Mute),
                seg(" / ", Tok::Dim),
                segb(sname, Tok::Fg),
            ],
            vec![seg("new session", Tok::Dim)],
        );
    }
    let Some(a) = &v.active else {
        return (Vec::new(), tally(v));
    };
    let left = vec![
        seg(a.project.clone(), Tok::Mute),
        seg(" / ", Tok::Dim),
        segb(a.session.clone(), Tok::Fg),
    ];
    if matches!(v.layout, Layout::Hidden { .. }) {
        return (left, tally(v));
    }
    let (g, gt, gb) = glyph(a.status);
    let label = match a.status {
        Status::Running => "running",
        Status::Waiting => "waiting for input",
        Status::Idle => "idle",
        Status::Error => "error",
        Status::Exited => "exited",
    };
    let text = if compact || a.age.is_empty() {
        label.to_string()
    } else {
        format!("{label} · {}", a.age)
    };
    let tt = if a.status == Status::Waiting {
        Tok::Acc
    } else {
        Tok::Mute
    };
    (left, vec![(format!("{g} "), gt, gb), seg(text, tt)])
}

fn tally(v: &View) -> Vec<Seg> {
    let c = v.counts;
    let mut out: Vec<Seg> = Vec::new();
    let items = [
        (Status::Running, c.running),
        (Status::Waiting, c.waiting),
        (Status::Error, c.error),
        (Status::Idle, c.idle + c.exited),
    ];
    for (st, n) in items {
        if n == 0 {
            continue;
        }
        if !out.is_empty() {
            out.push(seg("  ", Tok::Fg));
        }
        let (g, tok, b) = glyph(st);
        out.push((format!("{g} {n}"), tok, b));
    }
    out
}

fn pad(line: &mut Vec<Seg>, to: usize) {
    let cur = seg_len(line);
    if to > cur {
        line.push(seg(" ".repeat(to - cur), Tok::Fg));
    }
}

pub fn tmux_format(segs: &[Seg], mode: ColorMode) -> String {
    let mut out = String::new();
    for (t, tok, b) in segs {
        out.push_str(&format!(
            "#[fg={},{}]",
            tok.tmux(mode),
            if *b { "bold" } else { "nobold" }
        ));
        out.push_str(&t.replace('#', "##").replace('%', "%%"));
    }
    out.push_str("#[default]");
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn escapes_hash() {
        let f = tmux_format(&[seg("a#b", Tok::Fg)], ColorMode::Truecolor);
        assert!(f.contains("a##b"));
        assert!(f.starts_with("#[fg=#e8e6e1,nobold]"));
    }

    #[test]
    fn exact_width() {
        let v = View::default();
        assert_eq!(seg_len(&segments(&v, 120)), 120);
    }
}
