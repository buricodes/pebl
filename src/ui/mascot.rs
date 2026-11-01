use super::theme::Tok;
use super::{fit, len, seg, segb, Painter, Seg};
use crate::view::{Notice, NoticeKind, TipView, View};

pub const CARD_H: i32 = 4;
const TIP_LINES: i32 = 2;

pub fn card_h(v: &View) -> i32 {
    match v.tip {
        Some(_) => CARD_H + TIP_LINES,
        None => CARD_H,
    }
}
pub const TEXT_X: i32 = 11;

pub const MOODS: [NoticeKind; 5] = [
    NoticeKind::Idle,
    NoticeKind::Working,
    NoticeKind::Waiting,
    NoticeKind::Error,
    NoticeKind::Done,
];

pub fn label(k: NoticeKind) -> &'static str {
    match k {
        NoticeKind::Idle => "idle",
        NoticeKind::Working => "working",
        NoticeKind::Waiting => "needs you",
        NoticeKind::Error => "error",
        NoticeKind::Done => "finished",
    }
}

#[derive(Clone, Copy)]
pub struct Pose {
    pub eye: char,
    pub mark: char,
    pub alt: bool,
    pub kind: NoticeKind,
}

pub struct Character {
    pub name: &'static str,
    pub note: &'static str,
    solid: bool,
    art: fn(Pose) -> Vec<String>,
}

pub const ALL: [Character; 6] = [JEL, PEBL, SHROOM, TOAST, NIB, HOOT];
pub const DEFAULT: &str = "jel";

pub fn by_name(name: &str) -> Option<&'static Character> {
    ALL.iter().find(|c| c.name == name)
}

const JEL: Character = Character {
    name: "jel",
    note: "a jellyfish; tentacles drift while it works, stiff on error",
    solid: true,
    art: |p| {
        let (a, b) = if p.alt {
            ("  ▐▌ ▐▌", "   ▘ ▝▘ ▝")
        } else {
            ("  ▌▐ ▌▐", "  ▘ ▝▘ ▝")
        };
        vec![
            format!("  ▄▆██▆▄ {}", p.mark),
            "  █E██E█".into(),
            a.into(),
            b.into(),
        ]
    },
};

const PEBL: Character = Character {
    name: "pebl",
    note: "the namesake pebble; rocks side to side while sessions work",
    solid: true,
    art: |p| {
        let pad = if p.alt { " " } else { "" };
        let gap = if p.alt { "" } else { " " };
        vec![
            format!("{pad}  ▄▄▄▄▄▄{gap}{}", p.mark),
            format!("{pad} ▟█E██E█▙"),
            format!("{pad} ▜██████▛"),
            format!("{pad}   ▀▀▀▀"),
        ]
    },
};

const SHROOM: Character = Character {
    name: "shroom",
    note: "cap spots pulse while it works",
    solid: true,
    art: |p| {
        let spot = if p.alt { '▒' } else { '░' };
        vec![
            format!("  ▄████▄ {}", p.mark),
            format!(" ██{spot}██{spot}██"),
            "   ▐E█E▌".into(),
            "    ▀▀▀".into(),
        ]
    },
};

const TOAST: Character = Character {
    name: "toast",
    note: "a toaster; slices pop when a session finishes, smokes on error",
    solid: true,
    art: |p| {
        let top = match p.kind {
            NoticeKind::Done => "  ██  ██",
            NoticeKind::Error if p.alt => "  ▒▒  ▒▒",
            _ => "  ░░  ░░",
        };
        vec![
            format!("{top} {}", p.mark),
            " ▐██████▌".into(),
            " ▐█E██E█▌▬".into(),
            "  ▀    ▀".into(),
        ]
    },
};

const NIB: Character = Character {
    name: "nib",
    note: "a cat; dozes when idle and stares when it needs you",
    solid: false,
    art: |p| {
        vec![
            format!(" /\\_/\\  {}", p.mark),
            format!("( {e}.{e} )", e = p.eye),
            " > ^ <".into(),
        ]
    },
};

const HOOT: Character = Character {
    name: "hoot",
    note: "an owl that keeps watch over long sessions",
    solid: false,
    art: |p| {
        vec![
            format!(" ,___,  {}", p.mark),
            format!(" ({e},{e})", e = p.eye),
            " /)  )".into(),
            " -\"-\"-".into(),
        ]
    },
};

const SPIN: [char; 6] = ['·', '✢', '✳', '✶', '✻', '✽'];

fn tone(k: NoticeKind) -> Tok {
    match k {
        NoticeKind::Idle => Tok::Mute,
        NoticeKind::Working | NoticeKind::Waiting => Tok::Acc,
        NoticeKind::Error => Tok::Err,
        NoticeKind::Done => Tok::Ok,
    }
}

pub fn pose(k: NoticeKind, f: u64) -> Pose {
    let eye = match k {
        NoticeKind::Idle => '-',
        NoticeKind::Working if f % 16 == 15 => '-',
        NoticeKind::Working => '•',
        NoticeKind::Waiting => 'O',
        NoticeKind::Error => 'x',
        NoticeKind::Done => '^',
    };
    let mark = match k {
        NoticeKind::Idle if (f / 4) % 2 == 1 => 'Z',
        NoticeKind::Idle => 'z',
        NoticeKind::Working => SPIN[(f % SPIN.len() as u64) as usize],
        NoticeKind::Waiting if (f / 2) % 2 == 1 => ' ',
        NoticeKind::Waiting => '!',
        _ => ' ',
    };
    let alt = match k {
        NoticeKind::Working => (f / 2) % 2 == 1,
        NoticeKind::Idle => (f / 8) % 2 == 1,
        NoticeKind::Error => (f / 3) % 2 == 1,
        _ => false,
    };
    Pose {
        eye,
        mark,
        alt,
        kind: k,
    }
}

pub fn art(p: &mut Painter, c: &Character, k: NoticeKind, f: u64, x: i32, y: i32) {
    draw(p, c, pose(k, f), tone(k), k == NoticeKind::Waiting, x, y);
}

fn tip_pose(f: u64) -> Pose {
    Pose {
        eye: '•',
        mark: ' ',
        ..pose(NoticeKind::Working, f)
    }
}

fn draw(p: &mut Painter, c: &Character, pose: Pose, tok: Tok, bold: bool, x: i32, y: i32) {
    draw_rows(p, c, pose, tok, bold, (x, y), usize::MAX);
}

pub fn jel_head(p: &mut Painter, f: u64, x: i32, y: i32) {
    let pose = Pose {
        eye: 'O',
        mark: ' ',
        ..pose(NoticeKind::Working, f)
    };
    draw_rows(p, &JEL, pose, Tok::Acc, false, (x - 2, y), 2);
}

fn draw_rows(
    p: &mut Painter,
    c: &Character,
    pose: Pose,
    tok: Tok,
    bold: bool,
    at: (i32, i32),
    rows: usize,
) {
    let (x, y) = at;
    for (i, line) in (c.art)(pose).iter().take(rows).enumerate() {
        let yy = y + i as i32;
        if !c.solid {
            p.t(x, yy, line, tok, bold);
            continue;
        }
        for (j, ch) in line.chars().enumerate() {
            let xx = x + j as i32;
            match ch {
                ' ' => {}
                'E' => {
                    p.t(xx, yy, &pose.eye.to_string(), Tok::Ink, true);
                    p.bg(xx, yy, 1, tok);
                }
                _ => {
                    p.t(xx, yy, &ch.to_string(), tok, bold && !is_block(ch));
                }
            }
        }
    }
}

fn is_block(ch: char) -> bool {
    ('\u{2580}'..='\u{259f}').contains(&ch)
}

pub fn message(n: &Notice, short: bool) -> [Vec<Seg>; 3] {
    let go = if short { "^g" } else { "ctrl+g" };
    match n.kind {
        NoticeKind::Idle => [
            vec![segb("all quiet", Tok::Fg)],
            vec![seg(
                match n.count {
                    0 => "no sessions yet".to_string(),
                    1 => "1 session idle".to_string(),
                    c => format!("{c} sessions idle"),
                },
                Tok::Mute,
            )],
            vec![
                seg(if short { "^n" } else { "ctrl+n" }, Tok::Fg),
                seg(" new", Tok::Dim),
            ],
        ],
        NoticeKind::Working => [
            vec![segb(format!("{} running", n.count), Tok::Fg)],
            vec![seg(n.session.clone(), Tok::Mute)],
            vec![seg(format!("› {}", n.detail), Tok::Dim)],
        ],
        NoticeKind::Waiting => {
            let mut last = vec![seg(go, Tok::Fg)];
            if n.more > 0 {
                last.push(seg(format!(" · {} more", n.more), Tok::Dim));
            }
            [
                vec![segb(format!("◆ {}", n.session), Tok::Acc)],
                vec![seg(n.detail.clone(), Tok::Mute)],
                last,
            ]
        }
        NoticeKind::Error => [
            vec![segb(format!("✕ {}", n.session), Tok::Err)],
            vec![seg(n.detail.clone(), Tok::Mute)],
            vec![seg(go, Tok::Fg), seg(" open", Tok::Dim)],
        ],
        NoticeKind::Done => [
            vec![segb(format!("✓ {}", n.session), Tok::Ok)],
            vec![seg(n.detail.clone(), Tok::Mute)],
            vec![seg(go, Tok::Fg), seg(" review", Tok::Dim)],
        ],
    }
}

fn fit_segs(segs: &[Seg], w: usize) -> Vec<Seg> {
    let mut out = Vec::new();
    let mut left = w;
    for (t, tok, b) in segs {
        if left == 0 {
            break;
        }
        let s = fit(t, left);
        left = left.saturating_sub(len(&s));
        out.push((s, *tok, *b));
    }
    out
}

fn wrap(text: &str, w: usize, lines: usize) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for word in text.split_whitespace() {
        let join = out.last().is_some_and(|l| len(l) + 1 + len(word) <= w) || out.len() == lines;
        match out.last_mut() {
            Some(l) if join => {
                l.push(' ');
                l.push_str(word);
            }
            _ => out.push(word.to_string()),
        }
    }
    out.iter().map(|l| fit(l, w)).collect()
}

fn ask_hint() -> Vec<Seg> {
    vec![segb("click me", Tok::Fg), seg(" to ask", Tok::Dim)]
}

fn tip_card(p: &mut Painter, c: &Character, t: &TipView, f: u64, at: (i32, i32), w: i32, h: i32) {
    let (x, y) = at;
    draw(p, c, tip_pose(f), Tok::Acc, false, x, y);
    let tw = (w - x - TEXT_X).max(0) as usize;
    let head = vec![
        segb(t.key.clone(), Tok::Acc),
        seg(format!("  tip {}/{}", t.at, t.total), Tok::Dim),
    ];
    let tip_lines = |width: usize| -> Vec<Vec<Seg>> {
        wrap(&t.text, width.saturating_sub(2).max(1), TIP_LINES as usize)
            .into_iter()
            .enumerate()
            .map(|(i, l)| {
                let mark = if i == 0 { "✦ " } else { "  " };
                vec![seg(mark, Tok::Acc), seg(l, Tok::Fg)]
            })
            .collect()
    };
    let mut beside = vec![ask_hint(), Vec::new(), head];
    if h > CARD_H {
        let bw = (w - x - 1).max(1) as usize;
        for (i, line) in tip_lines(bw).iter().enumerate() {
            p.segs(x, y + CARD_H + i as i32, line);
        }
    } else {
        beside.remove(1);
        beside.extend(tip_lines(tw));
    }
    for (i, line) in beside.iter().enumerate() {
        p.segs(x + TEXT_X, y + i as i32, &fit_segs(line, tw));
    }
}

pub fn card(p: &mut Painter, c: &Character, v: &View, f: u64, at: (i32, i32), w: i32, h: i32) {
    match &v.tip {
        Some(t) => tip_card(p, c, t, f, at, w, h),
        None => notice_card(p, c, &v.notice, f, at.0, at.1, w),
    }
}

fn notice_card(p: &mut Painter, c: &Character, n: &Notice, f: u64, x: i32, y: i32, w: i32) {
    art(p, c, n.kind, f, x, y);
    let tw = (w - x - TEXT_X).max(0) as usize;
    for (i, line) in message(n, false).iter().enumerate() {
        p.segs(x + TEXT_X, y + i as i32, &fit_segs(line, tw));
    }
}

pub fn compact(p: &mut Painter, c: &Character, v: &View, f: u64, x: i32, y: i32, w: i32) {
    let n = &v.notice;
    let (pose, tok) = match v.tip {
        Some(_) => (tip_pose(f), Tok::Acc),
        None => (pose(n.kind, f), tone(n.kind)),
    };
    let mut xx = x;
    if c.solid {
        p.t(xx, y, "▐", tok, false);
        p.t(xx + 1, y, &pose.eye.to_string(), Tok::Ink, true);
        p.bg(xx + 1, y, 1, tok);
        p.t(xx + 2, y, "▌", tok, false);
        xx += 3;
    } else {
        let face = (c.art)(pose)[1].trim().to_string();
        xx = p.t(xx, y, &face, tok, true);
    }
    if let Some(t) = &v.tip {
        let segs = [
            seg(" ", Tok::Fg),
            segb(t.key.clone(), Tok::Acc),
            seg(format!(" {}", t.text), Tok::Mute),
        ];
        p.segs(xx, y, &fit_segs(&segs, (w - xx).max(0) as usize));
        return;
    }
    let segs: Vec<Seg> = match n.kind {
        NoticeKind::Idle => vec![seg(" all quiet", Tok::Mute)],
        NoticeKind::Working => vec![seg(format!(" {} running", n.count), Tok::Mute)],
        NoticeKind::Waiting => vec![segb(format!(" ◆ {}", n.session), Tok::Acc)],
        NoticeKind::Error => vec![segb(format!(" ✕ {}", n.session), Tok::Err)],
        NoticeKind::Done => vec![segb(format!(" ✓ {}", n.session), Tok::Ok)],
    };
    let key = matches!(
        n.kind,
        NoticeKind::Waiting | NoticeKind::Error | NoticeKind::Done
    );
    let room = (w - xx - if key { 4 } else { 0 }).max(0) as usize;
    let e = p.segs(xx, y, &fit_segs(&segs, room));
    if key {
        p.t(e, y, " ^g", Tok::Fg, false);
    }
}

pub fn current(v: &View) -> Option<&'static Character> {
    v.mascot.as_deref().and_then(by_name)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn art_fits_the_card() {
        for c in ALL {
            for k in MOODS {
                for f in 0..16 {
                    let lines = (c.art)(pose(k, f));
                    assert!(lines.len() as i32 <= CARD_H, "{}", c.name);
                    for l in lines {
                        assert!((len(&l) as i32) < TEXT_X, "{} {l:?}", c.name);
                    }
                }
            }
        }
    }
}
