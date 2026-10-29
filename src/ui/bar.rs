use super::theme::Tok;
use super::{keys, len, seg, seg_len, segb, Painter, Seg};
use crate::view::{Broadcast, Finish, Layout, Mode, NewProject, Peek, SearchView, View};

fn shell_line(v: &View) -> bool {
    matches!(v.mode, Mode::Normal)
        && v.shell
            .as_ref()
            .is_some_and(|s| !s.open && (s.running.is_some() || s.done.is_some()))
}

pub fn height(v: &View) -> u16 {
    if shell_line(v) {
        return 2;
    }
    match &v.mode {
        Mode::NewProject(np) => prompt_rows(np, 200).len() as u16 + 2,
        Mode::Peek(pk) => pk.lines.len() as u16 + 5,
        Mode::Broadcast(b) => broadcast_rows(b) as u16 + 2,
        Mode::Search(_) if v.layout.short_keys() => 10,
        Mode::Search(_) => 12,
        Mode::Finish(f) => finish_rows(f, v.flash.as_deref()).len() as u16 + 2,
        _ => 1,
    }
}

pub fn render(v: &View, p: &mut Painter) {
    let h = p.h();
    if let Mode::NewProject(np) = &v.mode {
        let rows = prompt_rows(np, p.w().max(0) as usize);
        for (i, row) in rows.iter().enumerate() {
            p.segs(0, i as i32, row);
        }
        p.caret(10 + seg_len(&rows[2][1..]) as i32, 2);
        if h >= 2 {
            let w = p.w();
            p.hl(0, h - 2, w);
        }
    }
    if let Mode::Finish(f) = &v.mode {
        for (i, row) in finish_rows(f, v.flash.as_deref()).iter().enumerate() {
            p.segs(0, i as i32, row);
        }
        if h >= 2 {
            let w = p.w();
            p.hl(0, h - 2, w);
        }
    }
    if let Mode::Broadcast(b) = &v.mode {
        broadcast(b, p);
        if h >= 2 {
            let w = p.w();
            p.hl(0, h - 2, w);
        }
    }
    if let Mode::Search(sv) = &v.mode {
        search(sv, p, !v.layout.short_keys());
        if h >= 2 {
            let w = p.w();
            p.hl(0, h - 2, w);
        }
    }
    if let Mode::Peek(pk) = &v.mode {
        peek(pk, p);
        if h >= 2 {
            let w = p.w();
            p.hl(0, h - 2, w);
        }
    }
    if shell_line(v) && h >= 2 {
        if let Some(s) = &v.shell {
            hidden_shell(s, p, h - 2);
        }
    }
    status_row(v, p, h - 1);
}

fn hidden_shell(s: &crate::view::ShellView, p: &mut Painter, y: i32) {
    let w = p.w();
    let (row, key): (Vec<Seg>, Vec<Seg>) = match (&s.running, &s.done) {
        (Some(cmd), _) => (
            vec![
                seg(" ● ", Tok::Ok),
                segb("shell", Tok::Fg),
                seg(format!(" · {cmd} · running"), Tok::Mute),
            ],
            vec![seg("ctrl+t", Tok::Fg), seg(" open", Tok::Mute)],
        ),
        (None, Some((cmd, true, detail))) => (
            vec![
                seg(" ✕ ", Tok::Err),
                segb("shell", Tok::Fg),
                seg(format!(" · {cmd} failed · "), Tok::Mute),
                seg(detail.clone(), Tok::Err),
            ],
            vec![seg("alt+c", Tok::Fg), seg(" send to Claude", Tok::Mute)],
        ),
        (None, Some((cmd, false, _))) => (
            vec![
                seg(" ✓ ", Tok::Ok),
                segb("shell", Tok::Fg),
                seg(format!(" · {cmd} finished"), Tok::Mute),
            ],
            vec![seg("alt+c", Tok::Fg), seg(" send to Claude", Tok::Mute)],
        ),
        _ => return,
    };
    let kw = seg_len(&key) as i32;
    let room = (w - kw - 3).max(1) as usize;
    let mut used = 0;
    let mut x = 0;
    for (t, tok, b) in row {
        let t = super::fit(&t, room.saturating_sub(used));
        used += len(&t);
        x = p.t(x, y, &t, tok, b);
    }
    p.segs(w - kw - 1, y, &key);
}

fn finish_rows(f: &Finish, flash: Option<&str>) -> Vec<Vec<Seg>> {
    let plural = |n: usize, one: &str, many: &str| {
        if n == 1 {
            format!("1 {one}")
        } else {
            format!("{n} {many}")
        }
    };
    let mut rows = vec![
        vec![
            segb(" Finish  ", Tok::Fg),
            segb(f.branch.clone(), Tok::Acc),
            seg(" → ", Tok::Dim),
            seg(f.into.clone(), Tok::Fg),
            seg(format!("   {}", f.session), Tok::Mute),
        ],
        vec![
            seg("   ", Tok::Fg),
            seg(
                plural(f.ahead, "commit ahead", "commits ahead"),
                if f.ahead > 0 { Tok::Fg } else { Tok::Mute },
            ),
            seg(" · ", Tok::Dim),
            seg(
                plural(f.dirty, "uncommitted file", "uncommitted files"),
                if f.dirty > 0 { Tok::Acc } else { Tok::Mute },
            ),
        ],
        vec![],
    ];
    let opt = |k: &str, text: String, on: bool| {
        vec![
            seg("   ", Tok::Fg),
            segb(k.to_string(), if on { Tok::Acc } else { Tok::Dim }),
            seg(format!("  {text}"), if on { Tok::Fg } else { Tok::Dim }),
        ]
    };
    let clean = f.dirty == 0;
    if f.dirty > 0 {
        rows.push(opt("a", "ask Claude to commit its changes".into(), true));
    }
    rows.push(opt(
        "m",
        format!("merge into {}, then remove the worktree", f.into),
        clean && f.ahead > 0,
    ));
    rows.push(opt(
        "p",
        if f.gh {
            "push and open a pull request (keeps the worktree)".into()
        } else {
            "push and open a pull request (needs gh)".into()
        },
        clean && f.gh,
    ));
    rows.push(opt(
        "d",
        "discard: delete the worktree, the branch and the session".into(),
        true,
    ));
    let note = if let Some(b) = &f.busy {
        Some(seg(format!("   {b}"), Tok::Acc))
    } else if f.confirm_discard {
        Some(segb(
            format!(
                "   press d again to throw away {} and {}",
                plural(f.ahead, "commit", "commits"),
                plural(f.dirty, "uncommitted file", "uncommitted files")
            ),
            Tok::Err,
        ))
    } else {
        flash.map(|m| seg(format!("   {m}"), Tok::Acc))
    };
    rows.push(vec![]);
    rows.push(note.into_iter().collect());
    rows
}

fn broadcast_rows(b: &Broadcast) -> usize {
    b.targets.len() + usize::from(b.more > 0) + 6
}

fn broadcast(b: &Broadcast, p: &mut Painter) {
    let w = p.w();
    let n = b.targets.len() + b.more;
    p.segs(
        1,
        0,
        &[
            segb("Send one prompt to ", Tok::Fg),
            segb(
                if n == 1 {
                    "1 session".to_string()
                } else {
                    format!("{n} sessions")
                },
                Tok::Acc,
            ),
        ],
    );
    p.rt(
        w - 1,
        0,
        "each gets it as its next message",
        Tok::Dim,
        false,
    );
    let mut y = 2;
    for t in &b.targets {
        let (g, gt, gb) = super::glyph(t.status);
        p.t(3, y, g, gt, gb);
        let e = p.t(5, y, &t.project, Tok::Mute, false);
        let e = p.t(e, y, " / ", Tok::Dim, false);
        p.t(
            e,
            y,
            &super::fit(&t.session, 30usize.saturating_sub((e - 5) as usize)),
            Tok::Fg,
            false,
        );
        let (word, what, tok) = match t.status {
            crate::model::Status::Running => (
                "running",
                "queued · sends when its current turn ends",
                Tok::Fg,
            ),
            crate::model::Status::Idle => ("idle", "sends now", Tok::Dim),
            _ => ("ready", "sends now", Tok::Dim),
        };
        p.t(40, y, word, Tok::Mute, false);
        p.t(52, y, what, tok, false);
        y += 1;
    }
    if b.more > 0 {
        p.t(3, y, &format!("and {} more", b.more), Tok::Dim, false);
        y += 1;
    }
    y += 1;
    p.boxed(1, y, w - 2, 3, Tok::Acc, true);
    let e = p.segs(3, y + 1, &[seg("> ", Tok::Fg)]);
    let text = fit_left(&b.text, (w - 8).max(1) as usize);
    let e = p.t(e, y + 1, &text, Tok::Fg, false);
    p.caret(e, y + 1);
}

fn snippet(text: &str, q: &str, w: usize, sel: bool) -> Vec<Seg> {
    let base = if sel { Tok::Fg } else { Tok::Mute };
    let chars: Vec<char> = text.chars().collect();
    let low: Vec<char> = chars
        .iter()
        .map(|c| c.to_lowercase().next().unwrap_or(*c))
        .collect();
    let ql: Vec<char> = q.to_lowercase().chars().collect();
    let at = if ql.is_empty() {
        None
    } else {
        low.windows(ql.len()).position(|win| win == ql.as_slice())
    };
    let Some(i) = at else {
        return vec![seg(super::fit(text, w), base)];
    };
    let st = i.saturating_sub(w.saturating_sub(ql.len()) / 3);
    let pre: String = if st > 0 {
        format!("…{}", chars[st + 1..i].iter().collect::<String>())
    } else {
        chars[..i].iter().collect()
    };
    let m: String = chars[i..i + ql.len()].iter().collect();
    let mut post: String = chars[i + ql.len()..].iter().collect();
    let room = w.saturating_sub(len(&pre) + len(&m));
    if len(&post) > room {
        post = super::fit(&post, room);
    }
    vec![seg(pre, base), segb(m, Tok::Acc), seg(post, base)]
}

fn search(sv: &SearchView, p: &mut Painter, wide: bool) {
    let w = p.w();
    let can_scope = !sv.project.is_empty();
    let scope = vec![
        (
            "this project".to_string(),
            if sv.all { Tok::Mute } else { Tok::Acc },
            !sv.all,
        ),
        seg("  ·  ", Tok::Dim),
        (
            if wide { "all projects" } else { "all" }.to_string(),
            if sv.all { Tok::Acc } else { Tok::Mute },
            sv.all,
        ),
    ];
    let q = vec![seg("/ ", Tok::Acc), seg(sv.query.clone(), Tok::Fg)];
    let summary = if sv.indexing && sv.searched == 0 {
        "indexing conversations…".to_string()
    } else if sv.query.trim().is_empty() {
        format!("{} conversations", sv.searched)
    } else {
        let m = if sv.matches == 1 { "match" } else { "matches" };
        let c = if sv.convs == 1 {
            "conversation"
        } else {
            "conversations"
        };
        format!("{} {m} in {} {c}", sv.matches, sv.convs)
    };
    let mut y = 0;
    if wide {
        p.segs(1, y, &[segb("Search conversations", Tok::Fg)]);
        if can_scope {
            p.segs(w - 1 - seg_len(&scope) as i32, y, &scope);
        }
        y += 1;
        let e = p.segs(3, y, &q);
        p.caret(e, y);
        p.rt(w - 1, y, &summary, Tok::Dim, false);
        y += 2;
    } else {
        let e = p.segs(1, y, &[segb("search ", Tok::Fg)]);
        let e = p.segs(e, y, &q);
        p.caret(e, y);
        if can_scope {
            p.segs(w - 1 - seg_len(&scope) as i32, y, &scope);
        }
        y += 1;
    }
    if sv.rows.is_empty() {
        if sv.query.trim().is_empty() {
            let where_ = if sv.all || sv.project.is_empty() {
                "every project".to_string()
            } else {
                sv.project.clone()
            };
            p.t(
                3,
                y,
                &format!("Type to search past conversations in {where_}."),
                Tok::Mute,
                false,
            );
            return;
        }
        let in_ = if sv.all || sv.project.is_empty() {
            String::new()
        } else {
            format!(" in {}", sv.project)
        };
        let gap = if wide { 0 } else { 1 };
        p.t(
            3,
            y + gap,
            &format!("No conversations{in_} mention “{}”.", sv.query.trim()),
            Tok::Fg,
            false,
        );
        p.t(
            3,
            y + gap + 1,
            &format!(
                "Searched {} conversations, {} of them started outside pebl.",
                sv.searched, sv.outside
            ),
            Tok::Dim,
            false,
        );
        if !sv.all && can_scope {
            p.segs(
                3,
                y + gap + 3,
                &[
                    seg("tab", Tok::Fg),
                    seg(
                        if sv.projects > 1 {
                            format!("  search all {} projects", sv.projects)
                        } else {
                            "  search every folder".to_string()
                        },
                        Tok::Mute,
                    ),
                ],
            );
        }
        return;
    }
    let (pc, tc, sc) = if wide {
        ((3, 12), (16, 30), 48)
    } else {
        ((3, 10), (14, 20), 35)
    };
    let snip_w = if wide {
        (w - 48 - 20).max(10)
    } else {
        (w - 35 - 12).max(8)
    } as usize;
    for (i, r) in sv.rows.iter().enumerate() {
        let yy = y + i as i32;
        let sel = i == sv.sel;
        if sel {
            p.bg(0, yy, w, Tok::Sel);
            p.t(1, yy, "›", Tok::Acc, true);
        }
        p.t(
            pc.0,
            yy,
            &super::fit(&r.project, pc.1),
            if sel { Tok::Fg } else { Tok::Mute },
            false,
        );
        p.t(tc.0, yy, &super::fit(&r.title, tc.1), Tok::Fg, sel);
        p.segs(sc, yy, &snippet(&r.snippet, sv.query.trim(), snip_w, sel));
        if wide && !r.src.is_empty() {
            let tok = if r.src == "live" { Tok::Ok } else { Tok::Dim };
            p.t(w - 18, yy, &r.src, tok, false);
        }
        p.rt(
            w - 1,
            yy,
            &r.when,
            if sel { Tok::Fg } else { Tok::Dim },
            false,
        );
    }
    let foot = y + sv.rows.len() as i32;
    let more = if sv.more > 0 {
        format!("↓ {} more", sv.more)
    } else {
        String::new()
    };
    let line = if wide {
        more
    } else if more.is_empty() {
        summary
    } else {
        format!("{more} · {summary}")
    };
    p.t(3, foot, &line, Tok::Dim, false);
}

fn peek(pk: &Peek, p: &mut Painter) {
    let w = p.w();
    let mut x = p.segs(0, 0, &[segb(" Peek  ", Tok::Fg)]);
    if let Some(st) = pk.status {
        let (g, tok, b) = super::glyph(st);
        x = p.t(x, 0, g, tok, b);
        x = p.t(x, 0, " ", Tok::Fg, false);
    }
    x = p.t(x, 0, &pk.project, Tok::Mute, false);
    x = p.t(x, 0, " / ", Tok::Dim, false);
    p.t(x, 0, &pk.session, Tok::Fg, true);
    let room = (w - 4).max(1) as usize;
    for (i, line) in pk.lines.iter().enumerate() {
        p.t(3, 1 + i as i32, &super::fit(line, room), Tok::Fg, false);
    }
    let y = pk.lines.len() as i32 + 2;
    let e = p.segs(
        0,
        y,
        &[
            seg("   reply  ", Tok::Mute),
            seg(fit_left(&pk.text, (w - 12).max(1) as usize), Tok::Fg),
        ],
    );
    p.caret(e, y);
    if pk.text.is_empty() {
        p.t(
            e + 2,
            y,
            "type to answer, or press a number",
            Tok::Dim,
            false,
        );
    }
}

fn status_row(v: &View, p: &mut Painter, y: i32) {
    let (mut left, mut right) = status(v);
    let over = (seg_len(&left) + seg_len(&right) + 3).saturating_sub(p.w().max(0) as usize);
    if over > 0 {
        if let Some(s) = right.iter_mut().find(|s| s.0 == v.path) {
            s.0 = fit_left(&s.0, len(&s.0).saturating_sub(over));
        }
    }
    let room = (p.w().max(0) as usize).saturating_sub(seg_len(&right) + 3);
    while seg_len(&left) > room && left.len() > 1 {
        while let Some(s) = left.pop() {
            if s.0 == " · " || left.len() <= 1 {
                break;
            }
        }
    }
    let x = p.segs(0, y, &left);
    if let Mode::Filter { .. } = v.mode {
        p.caret(x, y);
    }
    let w = p.w();
    p.segs(w - seg_len(&right) as i32 - 1, y, &right);
}

pub fn status(v: &View) -> (Vec<Seg>, Vec<Seg>) {
    let short = v.layout.short_keys();
    let k = |long: &'static str, sh: &'static str| if short { sh } else { long };

    if let Some((c, s)) = v.shell.as_ref().and_then(|s| s.resize) {
        let mut left = vec![
            seg(" ", Tok::Fg),
            segb("RESIZE", Tok::Acc),
            seg("  ", Tok::Fg),
        ];
        left.extend(
            keys(&[
                ("↑/↓", "1 row"),
                ("shift+↑/↓", "5 rows"),
                ("=", "half"),
                ("enter", "done"),
            ])
            .into_iter()
            .skip(1),
        );
        let right = vec![
            seg(format!("claude {c}"), Tok::Fg),
            seg(" ━ ", Tok::Dim),
            segb(format!("shell {s}"), Tok::Acc),
            seg("   min 6 · drag to the bottom to hide", Tok::Dim),
        ];
        return (left, right);
    }
    if v.focus == crate::view::Focus::Shell && matches!(v.mode, Mode::Normal) {
        let next = v.runnable.map(|(i, n)| format!("next line ({i} of {n})"));
        let mut list: Vec<(&str, &str)> = vec![("enter", "run")];
        if let Some(n) = &next {
            list.push(("alt+r", n.as_str()));
        }
        list.push(("alt+↑/↓", "resize"));
        list.push(("ctrl+t", "back to Claude"));
        let left = keys(&list);
        let right = match &v.shell {
            Some(s) => vec![
                segb("shell", Tok::Fg),
                seg(" · ", Tok::Dim),
                seg(v.path.clone(), Tok::Mute),
                seg(format!(" · {}", s.shell), Tok::Mute),
            ],
            None => Vec::new(),
        };
        return (left, right);
    }
    if let Some(d) = v.resizing {
        let mut left = vec![
            seg(" ", Tok::Fg),
            segb("RESIZE", Tok::Acc),
            seg("  ", Tok::Fg),
        ];
        left.extend(
            keys(&[
                ("←/→", "1 col"),
                ("shift+←/→", "5 cols"),
                ("=", "equalize"),
                ("enter", "done"),
            ])
            .into_iter()
            .skip(1),
        );
        let mut right = Vec::new();
        for (i, c) in v.columns.iter().enumerate() {
            if i > 0 {
                right.push(seg(" │ ", Tok::Dim));
            }
            let lit = i == d || i == d + 1;
            if i == v.col_focus {
                right.push(segb(c.width.to_string(), Tok::Acc));
            } else {
                right.push(seg(
                    c.width.to_string(),
                    if lit { Tok::Fg } else { Tok::Mute },
                ));
            }
        }
        right.push(seg(format!("   min {}", crate::app::MIN_COL), Tok::Dim));
        return (left, right);
    }
    let split = !v.columns.is_empty();
    let left = match &v.mode {
        Mode::NewSession {
            editing_prompt: false,
            ..
        } => keys(&[
            ("enter", "start"),
            ("tab", "first prompt"),
            ("esc", "cancel"),
        ]),
        Mode::NewSession { .. } => keys(&[
            ("enter", "start"),
            ("tab", "back to name"),
            ("esc", "cancel"),
        ]),
        Mode::NewProject(_) => keys(&[
            ("enter", "create"),
            ("tab", "complete"),
            ("↑/↓", "choose"),
            ("esc", "cancel"),
        ]),
        Mode::Rename { .. } => keys(&[("enter", "save"), ("esc", "cancel")]),
        Mode::Peek(pk) if pk.text.is_empty() => keys(&[
            ("1-9", "choose"),
            ("↑/↓", "move"),
            ("enter", "confirm"),
            ("tab", "next waiting"),
            ("shift+tab", "open"),
            ("esc", "close"),
        ]),
        Mode::Peek(_) => keys(&[("enter", "send"), ("esc", "close")]),
        Mode::Mark { .. } => keys(&[
            ("space", "mark"),
            ("j/k", "move"),
            ("a", "mark project"),
            ("enter", "write prompt"),
            ("esc", "cancel"),
        ]),
        Mode::Broadcast(b) => {
            let send = format!("send to {}", b.targets.len() + b.more);
            keys(&[
                ("enter", send.as_str()),
                ("esc", "back to marking"),
                ("ctrl+u", "clear"),
            ])
        }
        Mode::Search(sv) if !sv.query.trim().is_empty() && sv.rows.is_empty() => {
            keys(&[("tab", "all projects"), ("esc", "close")])
        }
        Mode::Search(_) => keys(&[
            ("enter", "resume"),
            ("↑/↓", "choose"),
            ("tab", "scope"),
            ("esc", "close"),
        ]),
        Mode::Finish(_) => keys(&[
            ("a", "commit"),
            ("m", "merge"),
            ("p", "pull request"),
            ("d", "discard"),
            ("esc", "cancel"),
        ]),
        Mode::ConfirmKill { name } => {
            let mut l = vec![
                seg(" Kill ", Tok::Fg),
                segb(name.clone(), Tok::Fg),
                seg("?   ", Tok::Fg),
            ];
            l.extend(keys(&[("y", "kill"), ("n", "keep")]).into_iter().skip(1));
            l
        }
        Mode::Filter { text } => vec![seg(" / ", Tok::Acc), seg(text.clone(), Tok::Fg)],
        Mode::Normal if v.flash.is_some() => vec![seg(
            format!(" {}", v.flash.clone().unwrap_or_default()),
            Tok::Acc,
        )],
        Mode::Normal
            if shell_line(v) && v.shell.as_ref().is_some_and(|s| s.done.is_some()) && !short =>
        {
            keys(&[
                ("alt+c", "send output to Claude"),
                ("ctrl+t", "open shell"),
                ("ctrl+j/k", "switch"),
                ("ctrl+q", "quit"),
            ])
        }
        Mode::Normal
            if v.runnable.is_some() && v.focus == crate::view::Focus::Main && !split && !short =>
        {
            keys(&[
                ("alt+r", "run in shell"),
                ("ctrl+t", "shell"),
                ("ctrl+j/k", "switch"),
                ("ctrl+b", "sidebar"),
                ("ctrl+q", "quit"),
            ])
        }
        Mode::Normal if split && v.focus != crate::view::Focus::Sidebar && !short => keys(&[
            ("alt+1-4", "focus column"),
            ("ctrl+\\", "split"),
            ("ctrl+w", "close column"),
            ("alt+←/→", "resize"),
            ("ctrl+j/k", "switch"),
            ("ctrl+b", "sidebar"),
            ("ctrl+q", "quit"),
        ]),
        Mode::Normal if v.projects.is_empty() && v.filter.is_empty() => keys(&[
            ("enter", "add this directory"),
            (k("ctrl+o", "^o"), "new project"),
            ("i", "import"),
            (k("ctrl+q", "^q"), "quit"),
        ]),
        Mode::Normal if matches!(v.layout, Layout::Hidden { .. }) => keys(&[
            (k("ctrl+b", "^b"), "show sidebar"),
            (k("ctrl+j/k", "^j/k"), "switch"),
            (k("ctrl+g", "^g"), "go to waiting"),
            (k("ctrl+q", "^q"), "quit"),
        ]),
        Mode::Normal if v.focus == crate::view::Focus::Sidebar && !short => keys(&[
            ("enter", "open"),
            ("p", "peek"),
            ("d", "review"),
            ("f", "finish"),
            ("r", "rename"),
            ("x", "kill"),
            ("w", "worktree"),
            ("e", "editor"),
            ("/", "filter"),
            ("?", "help"),
        ]),
        Mode::Normal if short => keys(&[
            ("^n", "new"),
            ("^j/k", "switch"),
            ("^b", "sidebar"),
            ("^q", "quit"),
        ]),
        Mode::Normal => keys(&[
            ("ctrl+n", "new session"),
            ("ctrl+t", "shell"),
            ("ctrl+j/k", "switch"),
            ("ctrl+b", "sidebar"),
            ("ctrl+q", "quit"),
        ]),
    };

    let mut right: Vec<Seg> = Vec::new();
    if let Mode::Filter { .. } = v.mode {
        right.extend(
            keys(&[("enter", "keep"), ("esc", "clear")])
                .into_iter()
                .skip(1),
        );
        return (left, right);
    }
    if let Some(m) = v.marking {
        right.push(segb(format!("{m} marked"), Tok::Acc));
        right.push(seg("   ", Tok::Fg));
    }
    let waiting = v.counts.waiting;
    if short {
        if waiting > 0 {
            right.push(segb(format!("◆ {waiting}"), Tok::Acc));
            right.push(seg("  ", Tok::Fg));
        }
        right.push(seg(format!("{} active", v.counts.active()), Tok::Mute));
        return (left, right);
    }
    let hidden = matches!(v.layout, Layout::Hidden { .. });
    if waiting > 0 && !hidden && matches!(v.mode, Mode::Normal) {
        right.push(segb(format!("◆ {waiting} waiting"), Tok::Acc));
        right.push(seg(" ctrl+g", Tok::Fg));
        right.push(seg("   ", Tok::Fg));
    }
    if hidden && !v.limits.is_empty() {
        for (i, l) in v.limits.iter().enumerate() {
            if i > 0 {
                right.push(seg(" · ", Tok::Dim));
            }
            right.push(seg(format!("{} ", l.label), Tok::Dim));
            right.push((format!("{}%", l.pct), super::ctx_tok(l.pct), l.pct > 95));
        }
        right.push(seg("   ", Tok::Fg));
    }
    right.push(seg(v.path.clone(), Tok::Mute));
    right.push(seg(" · ", Tok::Dim));
    if split {
        right.push(seg(format!("{} columns", v.columns.len()), Tok::Mute));
        right.push(seg(" · ", Tok::Dim));
    }
    right.push(seg(format!("{} active", v.counts.active()), Tok::Mute));
    (left, right)
}

fn fit_left(s: &str, n: usize) -> String {
    let l = len(s);
    if l <= n {
        return s.to_string();
    }
    if n == 0 {
        return String::new();
    }
    let tail: String = s.chars().skip(l - (n - 1)).collect();
    format!("…{tail}")
}

const MAX_CANDIDATES: usize = 4;

fn prompt_rows(np: &NewProject, width: usize) -> Vec<Vec<Seg>> {
    let mut rows = vec![vec![segb(" New project", Tok::Fg)], vec![]];
    let room = width.saturating_sub(12).max(10);
    rows.push(vec![
        seg("   path   ", Tok::Mute),
        seg(fit_left(&np.path, room), Tok::Fg),
    ]);
    let shown: Vec<_> = np.candidates.iter().take(MAX_CANDIDATES).collect();
    let info_w = shown.iter().map(|c| len(&c.info)).max().unwrap_or(0);
    let path_room = width.saturating_sub(14 + info_w).max(10);
    let pw = (shown
        .iter()
        .map(|c| len(&c.path))
        .max()
        .unwrap_or(0)
        .max(25)
        + 2)
    .min(path_room);
    let shown: Vec<_> = shown
        .into_iter()
        .map(|c| crate::view::Candidate {
            path: fit_left(&c.path, pw - 2),
            info: c.info.clone(),
        })
        .collect();
    if shown.is_empty() {
        rows.push(vec![seg(
            "            no matching folders · enter adds the path as typed",
            Tok::Dim,
        )]);
    }
    for (i, c) in shown.iter().enumerate() {
        let padded = format!("{:<pw$}", c.path);
        if i == np.sel {
            rows.push(vec![
                seg("          ", Tok::Fg),
                seg("❯ ", Tok::Acc),
                seg(padded, Tok::Acc),
                seg(c.info.clone(), Tok::Dim),
            ]);
        } else {
            rows.push(vec![
                seg(format!("            {padded}"), Tok::Mute),
                seg(c.info.clone(), Tok::Dim),
            ]);
        }
    }
    rows.push(vec![]);
    rows.push(vec![
        seg("   name   ", Tok::Mute),
        seg(np.name.clone(), Tok::Fg),
        seg("   from folder", Tok::Dim),
    ]);
    let then = if np.then_session {
        "start a session"
    } else {
        "just add it"
    };
    let hint = if np.then_session {
        "   space to skip"
    } else {
        "   space to start one"
    };
    rows.push(vec![
        seg("   then   ", Tok::Mute),
        seg(then, Tok::Fg),
        seg(hint, Tok::Dim),
    ]);
    rows
}
