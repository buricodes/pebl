use super::theme::Tok;
use super::{keys, len, seg, segb, Painter, Seg};
use crate::view::{Mode, Stage, View};

pub fn render(v: &View, p: &mut Painter) {
    match &v.stage {
        Stage::Welcome { dir, history } => welcome(p, dir, history),
        Stage::NewSession { cwd } => new_session(v, p, cwd),
        Stage::Help => help(p),
        Stage::Exited { name, code } => exited(p, name, *code),
        Stage::Empty => empty(v, p),
    }
}

fn welcome(p: &mut Painter, dir: &str, history: &[(String, usize)]) {
    let x = 1;
    let bw = 62.min(p.w() - 2);
    p.boxed(x, 0, bw, 6, Tok::Acc, true);
    p.segs(
        x + 2,
        1,
        &[
            seg("✻ ", Tok::Acc),
            seg("Welcome to ", Tok::Fg),
            segb("pebl", Tok::Fg),
        ],
    );
    p.t(
        x + 4,
        3,
        "Run several Claude Code sessions side by side and see",
        Tok::Mute,
        false,
    );
    p.t(
        x + 4,
        4,
        "at a glance which ones need you.",
        Tok::Mute,
        false,
    );

    let mut rows: Vec<Vec<Seg>> = vec![
        vec![segb("  Get started", Tok::Fg)],
        vec![],
        vec![
            seg("  enter    ", Tok::Fg),
            seg("add this directory     ", Tok::Mute),
            seg(super::fit(dir, (p.w() - 35).max(8) as usize), Tok::Dim),
        ],
        vec![
            seg("  ctrl+o   ", Tok::Fg),
            seg("add a project by path", Tok::Mute),
        ],
        vec![seg("  ?        ", Tok::Fg), seg("all shortcuts", Tok::Mute)],
    ];
    if !history.is_empty() {
        rows.push(vec![]);
        rows.push(vec![]);
        let n = history.len();
        let folders = if n == 1 { "folder" } else { "folders" };
        rows.push(vec![seg(
            format!("  Found Claude Code history in {n} {folders}"),
            Tok::Mute,
        )]);
        let pw = history.iter().map(|(d, _)| len(d)).max().unwrap_or(0) + 4;
        for (d, count) in history.iter().take(6) {
            let s = if *count == 1 { "session" } else { "sessions" };
            rows.push(vec![
                seg(format!("    {d:<pw$}"), Tok::Fg),
                seg(format!("{count} {s}"), Tok::Dim),
            ]);
        }
        if n > 6 {
            rows.push(vec![seg(format!("    … and {} more", n - 6), Tok::Dim)]);
        }
        let what = match n {
            1 => "it".to_string(),
            2 => "both".to_string(),
            _ => format!("all {n}"),
        };
        rows.push(vec![
            seg("  press ", Tok::Fg),
            segb("i", Tok::Acc),
            seg(format!(" to import {what}"), Tok::Mute),
        ]);
    }
    for (i, r) in rows.iter().enumerate() {
        p.segs(x, 8 + i as i32, r);
    }
}

fn new_session(v: &View, p: &mut Painter, cwd: &str) {
    let x = 1;
    p.boxed(x, 1, 56.min(p.w() - 2), 7, Tok::Line, true);
    p.segs(
        x + 2,
        2,
        &[
            seg("✻ ", Tok::Dim),
            segb("Welcome to Claude Code!", Tok::Dim),
        ],
    );
    p.t(
        x + 4,
        4,
        "/help for help, /status for your current setup",
        Tok::Dim,
        false,
    );
    let room = (56.min(p.w() - 2) - 6).max(8) as usize;
    p.t(
        x + 4,
        6,
        &super::fit(&format!("cwd: {cwd}"), room),
        Tok::Dim,
        false,
    );
    p.segs(
        x,
        9,
        &[
            seg("  Starts here when you press ", Tok::Mute),
            seg("enter", Tok::Fg),
            seg(". ", Tok::Mute),
            seg("tab", Tok::Fg),
            seg(" adds a first prompt to send.", Tok::Mute),
        ],
    );
    if let Mode::NewSession {
        prompt,
        editing_prompt,
        worktree,
        ..
    } = &v.mode
    {
        if *editing_prompt || !prompt.is_empty() {
            let e = p.segs(
                x,
                11,
                &[
                    seg("  first prompt  ", Tok::Mute),
                    seg(prompt.clone(), Tok::Fg),
                ],
            );
            if *editing_prompt {
                p.caret(e, 11);
            }
        }
        if *worktree {
            p.t(
                x,
                13,
                "  in a new git worktree on its own branch",
                Tok::Dim,
                false,
            );
        }
    }
}

const HELP: &[(&str, &[(&str, &str)])] = &[
    (
        "anywhere, even inside Claude",
        &[
            ("ctrl+j / ctrl+k", "next / previous session"),
            ("ctrl+g", "jump to whatever needs you"),
            ("ctrl+n", "new session in this project"),
            ("ctrl+o", "new project"),
            ("ctrl+b", "hide / show the sidebar"),
            ("ctrl+space", "sidebar ↔ Claude"),
            ("ctrl+f", "search past conversations"),
            ("ctrl+x", "mark sessions, send one prompt"),
            ("ctrl+q", "quit (sessions keep running)"),
        ],
    ),
    (
        "columns",
        &[
            ("ctrl+\\", "open a session in a new column"),
            ("alt+1 … alt+4", "focus a column"),
            ("alt+← / alt+→", "resize columns"),
            ("ctrl+w", "close the focused column"),
        ],
    ),
    (
        "shell drawer",
        &[
            ("ctrl+t", "open shell / back to Claude / hide"),
            ("alt+r", "type Claude's next command"),
            ("alt+↑ / alt+↓", "resize the drawer"),
            ("alt+c", "send shell output to Claude"),
        ],
    ),
    (
        "in the sidebar",
        &[
            ("j / k · enter", "move · open or resume"),
            ("space", "fold a project"),
            ("p", "peek at a session and answer it"),
            ("d", "review its changes"),
            ("f", "finish a worktree"),
            ("c", "open it in a new column"),
            ("w", "new worktree session"),
            ("r · x", "rename · kill"),
            ("e", "open the folder in your editor"),
            ("/", "filter"),
            ("m", "mute mascot, chimes, alerts"),
        ],
    ),
];

fn help_block(p: &mut Painter, x: i32, y: i32, title: &str, rows: &[(&str, &str)]) -> i32 {
    p.t(x, y, title, Tok::Acc, false);
    for (i, (k, d)) in rows.iter().enumerate() {
        p.segs(
            x,
            y + 1 + i as i32,
            &[seg(format!("  {k:<17}"), Tok::Fg), seg(*d, Tok::Mute)],
        );
    }
    y + rows.len() as i32 + 1
}

fn help(p: &mut Painter) {
    p.segs(1, 0, &[seg("✻ ", Tok::Acc), segb("Shortcuts", Tok::Fg)]);
    let close = keys(&[("esc", "close")]);
    let w = p.w();
    p.segs(w - crate::ui::seg_len(&close) as i32 - 1, 0, &close);
    p.t(
        1,
        1,
        "pebl catches these before they reach Claude Code",
        Tok::Mute,
        false,
    );
    let total: i32 = HELP.iter().map(|(_, r)| r.len() as i32 + 1).sum();
    let two = 3 + total + 3 > p.h() && p.w() >= 100;
    let mut y = 3;
    let mut x = 1;
    let split = if two { total / 2 } else { i32::MAX };
    let mut used = 0;
    for (title, rows) in HELP {
        if used >= split && x == 1 {
            x = p.w() / 2 + 1;
            y = 3;
        }
        y = help_block(p, x, y, title, rows);
        used += rows.len() as i32 + 1;
    }
    let bottom = if two { 3 + split + 1 } else { y };
    let note = if cfg!(target_os = "macos") {
        "alt keys need Option as Meta: Terminal › Settings › Profiles › Keyboard"
    } else {
        ""
    };
    if !note.is_empty() {
        p.t(1, bottom, note, Tok::Dim, false);
    }
}

fn exited(p: &mut Painter, name: &str, code: Option<i32>) {
    let what = match code {
        Some(c) if c != 0 => format!("exited with code {c}"),
        _ => "is not running".to_string(),
    };
    p.segs(
        1,
        1,
        &[segb(name, Tok::Fg), seg(format!(" {what}."), Tok::Mute)],
    );
    p.segs(
        1,
        3,
        &[
            seg("  enter", Tok::Fg),
            seg("  resume the conversation", Tok::Mute),
        ],
    );
    p.segs(
        1,
        4,
        &[seg("  x    ", Tok::Fg), seg("  remove it", Tok::Mute)],
    );
}

fn empty(v: &View, p: &mut Painter) {
    p.t(1, 1, "No session open.", Tok::Mute, false);
    let k = if v.layout.short_keys() {
        "^n"
    } else {
        "ctrl+n"
    };
    p.segs(
        1,
        3,
        &[
            seg(format!("  {k}"), Tok::Fg),
            seg("  start a Claude Code session here", Tok::Mute),
        ],
    );
}
