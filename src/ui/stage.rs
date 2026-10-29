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

const HELP: &[(&str, &str)] = &[
    ("ctrl+j / ctrl+k", "next / previous session"),
    ("ctrl+g", "jump to whatever needs you"),
    ("ctrl+n", "new session in current project"),
    ("ctrl+o", "new project"),
    ("ctrl+b", "toggle sidebar"),
    ("ctrl+space", "focus sidebar ↔ main pane"),
    ("ctrl+q", "quit (sessions keep running)"),
    ("ctrl+\\", "open a session in a new column (max 4)"),
    ("alt+1-4 / ctrl+w", "focus / close a column"),
    ("alt+← / →", "resize columns"),
    ("ctrl+t", "shell drawer under Claude"),
    ("alt+r", "type Claude's next command in the shell"),
    ("ctrl+f", "search past conversations"),
    ("ctrl+x", "mark sessions, send one prompt"),
    ("", ""),
    ("j / k", "move in the sidebar"),
    ("enter", "open session / resume exited one"),
    ("space", "expand / collapse project"),
    ("r", "rename"),
    ("x", "kill session (confirm)"),
    ("p", "peek at a session and answer it"),
    ("d", "review its changes, comment, send back"),
    ("e", "open the folder in your editor"),
    ("w", "new worktree session"),
    ("f", "finish a worktree: merge, PR or discard"),
    ("/", "filter"),
    ("m", "mute the mascot and bell"),
];

fn help(p: &mut Painter) {
    p.segs(1, 0, &[seg("✻ ", Tok::Acc), segb("Shortcuts", Tok::Fg)]);
    p.t(
        1,
        1,
        "captured before input is forwarded to Claude Code",
        Tok::Mute,
        false,
    );
    for (i, (k, d)) in HELP.iter().enumerate() {
        p.segs(
            1,
            3 + i as i32,
            &[seg(format!("  {k:<18}"), Tok::Fg), seg(*d, Tok::Mute)],
        );
    }
    p.segs(1, 4 + HELP.len() as i32, &keys(&[("esc", "close")]));
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
