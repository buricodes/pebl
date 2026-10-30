use super::theme::Tok;
use super::{bar, header, sidebar, stage, Painter, Seg};
use crate::view::View;
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;

pub enum Main<'a> {
    Stage,
    #[cfg_attr(not(test), allow(dead_code))]
    Lines(&'a [Vec<Seg>]),
}

pub fn render(v: &View, width: u16, height: u16, main: Main) -> Buffer {
    let mut buf = Buffer::empty(Rect::new(0, 0, width, height));
    let full = Rect::new(0, 0, width, height);
    let bar_h = bar::height(v).min(height.saturating_sub(3));
    let rule = height - bar_h - 1;
    let usage = header::usage_line(v, width as usize);
    let top: u16 = if usage.is_some() { 2 } else { 1 };
    {
        let mut p = Painter::new(&mut buf, full, v.color);
        p.segs(0, 0, &header::segments(v, width as usize));
        if let Some(u) = &usage {
            p.segs(0, 1, u);
        }
        p.hl(0, rule as i32, width as i32);
    }
    let sw = v.layout.sidebar_width().filter(|w| *w + 10 < width);
    let main_x = match sw {
        Some(sw) => {
            let mut p = Painter::new(&mut buf, full, v.color);
            p.vl(sw as i32, top as i32, (rule - top) as i32);
            p.t(sw as i32, rule as i32, "┴", Tok::Line, false);
            let area = Rect::new(0, top, sw, rule - top);
            sidebar::render(v, &mut Painter::new(&mut buf, area, v.color), 1);
            sw + 1
        }
        None => 0,
    };
    let main_area = Rect::new(main_x, top, width - main_x, rule - top);
    match main {
        Main::Stage => stage::render(v, &mut Painter::new(&mut buf, main_area, v.color)),
        Main::Lines(lines) => {
            let mut p = Painter::new(&mut buf, main_area, v.color);
            let top = p.h() - lines.len() as i32;
            for (i, l) in lines.iter().enumerate() {
                p.segs(1, top + i as i32, l);
            }
        }
    }
    let bar_area = Rect::new(0, height - bar_h, width, bar_h);
    bar::render(v, &mut Painter::new(&mut buf, bar_area, v.color));
    buf
}

#[cfg(test)]
pub fn to_text(buf: &Buffer) -> String {
    let a = buf.area;
    let mut out = String::new();
    for y in 0..a.height {
        let mut line = String::new();
        for x in 0..a.width {
            line.push_str(buf[(x, y)].symbol());
        }
        out.push_str(line.trim_end());
        out.push('\n');
    }
    out
}

#[cfg(test)]
pub mod fixtures {
    use super::super::{seg, segb, Seg};
    use crate::model::Status::{self, *};
    use crate::ui::theme::Tok;
    use crate::view::*;

    fn s(name: &str, status: Status, meta: &str) -> SessionRow {
        SessionRow {
            name: name.into(),
            status,
            meta: meta.into(),
            col: None,
            ctx: None,
            marked: None,
        }
    }

    pub fn projects(v2: bool) -> Vec<ProjectRow> {
        let mut infra = vec![
            s("tf-plan", Running, ""),
            s("dns", Idle, ""),
            s("ci-cache", Idle, ""),
        ];
        if v2 {
            infra[1].status = Waiting;
        }
        let rows = vec![
            (
                "nova-api",
                true,
                vec![
                    s("auth-refactor", Running, "2m"),
                    s("migrate-staging", Waiting, "input"),
                    s("docs-pass", Idle, "1h"),
                ],
            ),
            (
                "billing-svc",
                true,
                vec![
                    s("onboarding-flow", Running, "14s"),
                    s("e2e-tests", Error, "exit 1"),
                    s("a11y-audit", Idle, "3h"),
                ],
            ),
            ("infra", false, infra),
        ];
        rows.into_iter()
            .map(|(n, open, ss)| ProjectRow {
                name: n.into(),
                open,
                total: ss.len(),
                waiting: ss.iter().filter(|x| x.status == Waiting).count(),
                sessions: ss,
            })
            .collect()
    }

    pub fn base() -> View {
        View {
            projects: projects(false),
            cursor: Some(Cursor::Session(0, 0)),
            active: Some(Active {
                project: "nova-api".into(),
                session: "auth-refactor".into(),
                status: Running,
                age: "2m".into(),
            }),
            focus: Focus::Main,
            layout: Layout::Full(30),
            counts: Counts {
                running: 3,
                waiting: 1,
                idle: 4,
                error: 1,
                exited: 0,
            },
            path: "~/code/nova-api".into(),
            ..Default::default()
        }
    }

    pub fn transcript() -> Vec<Vec<Seg>> {
        vec![
            vec![
                seg("> ", Tok::Mute),
                seg(
                    "refactor the auth middleware to use the new session store",
                    Tok::Mute,
                ),
            ],
            vec![],
            vec![
                seg("● ", Tok::Ok),
                segb("Update", Tok::Fg),
                seg("(src/middleware/auth.ts)", Tok::Fg),
            ],
            vec![
                seg("  ⎿  ", Tok::Dim),
                seg("Updated src/middleware/auth.ts with 3 additions", Tok::Mute),
            ],
            vec![],
            vec![
                seg("✻ ", Tok::Acc),
                seg("Refactoring… ", Tok::Acc),
                seg("(47s · esc to interrupt)", Tok::Mute),
            ],
            vec![],
            vec![seg("╭", Tok::Dim), seg("─".repeat(80), Tok::Dim)],
            vec![seg("│ > ", Tok::Dim)],
            vec![seg("╰", Tok::Dim), seg("─".repeat(80), Tok::Dim)],
        ]
    }
}

#[cfg(test)]
mod tests {
    use super::fixtures::*;
    use super::*;
    use crate::model::Status;
    use crate::view::*;

    fn snap(v: &View, w: u16, h: u16, main: Main) -> String {
        to_text(&render(v, w, h, main))
    }

    #[test]
    fn clicks_land_on_the_row_drawn_when_the_list_scrolls() {
        let mut v = base();
        v.cursor = Some(Cursor::Session(1, 2));
        let (w, h) = (30u16, 12u16);
        let area = Rect::new(0, 0, w, h);
        let mut buf = Buffer::empty(area);
        sidebar::render(&v, &mut Painter::new(&mut buf, area, v.color), 1);
        let text = to_text(&buf);
        let lines: Vec<&str> = text.lines().collect();
        assert!(!lines[1].contains("nova-api"), "{text}");
        let mut hits = 0;
        for y in 0..h {
            let Some(c) = sidebar::list_row(&v, w, h, y).and_then(|r| sidebar::hit(&v, r)) else {
                continue;
            };
            let name = match c {
                Cursor::Project(pi) => v.projects[pi].name.clone(),
                Cursor::Session(pi, si) => v.projects[pi].sessions[si].name.clone(),
            };
            assert!(
                lines[y as usize].contains(&name[..6]),
                "row {y} hit {name}\n{text}"
            );
            hits += 1;
        }
        assert!(hits >= 3, "{text}");
    }

    #[test]
    fn s01_default_view() {
        let t = transcript();
        insta::assert_snapshot!(snap(&base(), 120, 40, Main::Lines(&t)));
    }

    #[test]
    fn s02_waiting_for_input() {
        let mut v = base();
        v.projects = projects(true);
        v.cursor = Some(Cursor::Session(0, 1));
        v.active = Some(Active {
            project: "nova-api".into(),
            session: "migrate-staging".into(),
            status: Status::Waiting,
            age: "3m".into(),
        });
        v.counts = Counts {
            running: 3,
            waiting: 2,
            idle: 3,
            error: 1,
            exited: 0,
        };
        insta::assert_snapshot!(snap(&v, 120, 40, Main::Lines(&[])));
    }

    #[test]
    fn s03_sidebar_collapsed() {
        let mut v = base();
        v.layout = Layout::Hidden { narrow: false };
        let t = transcript();
        insta::assert_snapshot!(snap(&v, 120, 40, Main::Lines(&t)));
    }

    #[test]
    fn s04_first_launch() {
        let v = View {
            stage: Stage::Welcome {
                dir: "~/code/nova-api".into(),
                history: vec![
                    ("~/code/nova-api".into(), 12),
                    ("~/code/billing-svc".into(), 4),
                ],
            },
            path: "~/code/nova-api".into(),
            ..Default::default()
        };
        insta::assert_snapshot!(snap(&v, 120, 40, Main::Stage));
    }

    #[test]
    fn s05a_new_project() {
        let mut v = base();
        v.mode = Mode::NewProject(NewProject {
            path: "~/code/billing".into(),
            candidates: vec![
                Candidate {
                    path: "~/code/billing-app".into(),
                    info: "git · main · 2d ago".into(),
                },
                Candidate {
                    path: "~/code/billing-app-legacy".into(),
                    info: "git · legacy · 8mo ago".into(),
                },
            ],
            sel: 0,
            name: "billing-app".into(),
            then_session: true,
        });
        v.focus = Focus::Bar;
        let t = transcript();
        insta::assert_snapshot!(snap(&v, 120, 40, Main::Lines(&t)));
    }

    #[test]
    fn s05b_new_session() {
        let mut v = base();
        v.mode = Mode::NewSession {
            project: 1,
            text: "fix-flaky-tests".into(),
            prompt: String::new(),
            editing_prompt: false,
            worktree: false,
        };
        v.cursor = None;
        v.stage = Stage::NewSession {
            cwd: "/Users/maya/code/billing-svc".into(),
        };
        v.path = "~/code/billing-svc".into();
        insta::assert_snapshot!(snap(&v, 120, 40, Main::Stage));
    }

    #[test]
    fn s06_minimum_size() {
        let mut v = base();
        v.layout = Layout::for_width(80, false);
        let t = transcript();
        insta::assert_snapshot!(snap(&v, 80, 24, Main::Lines(&t)));
    }

    #[test]
    fn help_screen() {
        let mut v = base();
        v.stage = Stage::Help;
        v.focus = Focus::Sidebar;
        insta::assert_snapshot!(snap(&v, 120, 40, Main::Stage));
    }

    #[test]
    fn mascot_in_sidebar() {
        let mut v = base();
        v.mascot = Some("jel".into());
        v.notice = Notice {
            kind: NoticeKind::Waiting,
            session: "migrate-staging".into(),
            detail: "wants to run Bash".into(),
            count: 2,
            more: 1,
        };
        let t = transcript();
        insta::assert_snapshot!(snap(&v, 120, 40, Main::Lines(&t)));
    }

    #[test]
    fn mascot_compact() {
        let mut v = base();
        v.layout = Layout::for_width(80, false);
        v.projects.truncate(1);
        v.mascot = Some("jel".into());
        v.notice = Notice {
            kind: NoticeKind::Waiting,
            session: "migrate-staging".into(),
            detail: "wants to run Bash".into(),
            count: 1,
            more: 0,
        };
        let t = transcript();
        insta::assert_snapshot!(snap(&v, 80, 24, Main::Lines(&t)));
    }

    #[test]
    fn split_tabs_and_resize_bar() {
        let mut v = base();
        let tab = |p: &str, s: &str, st, w| ColumnTab {
            project: p.into(),
            session: s.into(),
            status: Some(st),
            width: w,
            ctx: Some(if w == 53 { 97 } else { 42 }),
        };
        v.columns = vec![
            tab("nova-api", "auth-refactor", Status::Running, 51),
            tab("nova-api", "migrate-staging", Status::Waiting, 51),
            tab("billing-svc", "onboarding-flow", Status::Running, 51),
            tab("billing-svc", "e2e-tests", Status::Error, 53),
        ];
        v.focus = Focus::Main;
        let mut out = String::new();
        for (w, resizing) in [(209u16, None), (209, Some(0))] {
            v.resizing = resizing;
            let area = Rect::new(0, 0, w, 1);
            let mut buf = Buffer::empty(area);
            super::super::tabs::render(&v, &mut Painter::new(&mut buf, area, v.color));
            out.push_str(&to_text(&buf));
            let area = Rect::new(0, 0, 240, 1);
            let mut buf = Buffer::empty(area);
            bar::render(&v, &mut Painter::new(&mut buf, area, v.color));
            out.push_str(&to_text(&buf));
        }
        insta::assert_snapshot!(out);
    }

    #[test]
    fn finish_strip() {
        let mut v = base();
        v.mode = Mode::Finish(Finish {
            session: "auth-refactor".into(),
            branch: "pebl/auth-refactor".into(),
            into: "main".into(),
            ahead: 3,
            dirty: 2,
            gh: true,
            confirm_discard: false,
            busy: None,
        });
        v.focus = Focus::Bar;
        let t = transcript();
        insta::assert_snapshot!(snap(&v, 120, 40, Main::Lines(&t)));
    }

    fn usage_view(width: u16) -> View {
        let mut v = base();
        v.layout = Layout::for_width(width, false);
        let pcts = [42u8, 61, 84, 97, 31, 8];
        let mut i = 0;
        for p in v.projects.iter_mut() {
            for s in p.sessions.iter_mut() {
                s.ctx = Some(pcts[i % pcts.len()]);
                i += 1;
            }
        }
        v.cursor = Some(Cursor::Session(1, 0));
        v.active = Some(Active {
            project: "billing-svc".into(),
            session: "onboarding-flow".into(),
            status: Status::Running,
            age: "14s".into(),
        });
        v.usage = Some(Usage {
            model: "Sonnet".into(),
            pct: Some(97),
            used: 194_000,
            size: 200_000,
            tok_in: 1_100_000,
            tok_out: 61_000,
        });
        v.limits = vec![
            Limit {
                label: "5h".into(),
                pct: 23,
                resets: "2h14m".into(),
            },
            Limit {
                label: "week".into(),
                pct: 84,
                resets: "3d4h".into(),
            },
        ];
        v.today = (3_200_000, 214_000);
        v.mascot = Some("jel".into());
        v
    }

    #[test]
    fn usage_and_context() {
        let t = transcript();
        insta::assert_snapshot!(snap(&usage_view(120), 120, 40, Main::Lines(&t)));
    }

    #[test]
    fn usage_and_context_minimum() {
        let t = transcript();
        insta::assert_snapshot!(snap(&usage_view(80), 80, 24, Main::Lines(&t)));
    }

    #[test]
    fn usage_without_plan_limits() {
        let mut v = usage_view(120);
        v.limits.clear();
        let t = transcript();
        insta::assert_snapshot!(snap(&v, 120, 40, Main::Lines(&t)));
    }

    fn search_view(empty: bool) -> SearchView {
        let rows = [
            (
                "nova-api",
                "Auth middleware to session store",
                "2h",
                "live",
                "keep the old redis path behind a flag",
            ),
            (
                "nova-api",
                "Rate limiter rewrite",
                "3d",
                "",
                "move the token bucket into redis with a Lua script so it stays atomic",
            ),
            (
                "infra",
                "ElastiCache upgrade plan",
                "6d",
                "cli",
                "the redis 7.2 cluster mode needs a new parameter group",
            ),
            (
                "billing-svc",
                "Session timeout bug",
                "Oct 12",
                "cli",
                "the portal never refreshes the redis TTL when the user is active",
            ),
            (
                "nova-api",
                "Local dev setup",
                "Oct 3",
                "",
                "run docker compose up redis postgres before starting the API",
            ),
            (
                "infra",
                "CI cache flakiness",
                "Sep 28",
                "cli",
                "calling redis-cli FLUSHALL in the teardown step wipes the shared cache",
            ),
        ];
        SearchView {
            query: if empty { "kubernetes" } else { "redis" }.into(),
            all: !empty,
            sel: 2,
            rows: if empty {
                Vec::new()
            } else {
                rows.iter()
                    .map(|r| SearchRow {
                        project: r.0.into(),
                        title: r.1.into(),
                        when: r.2.into(),
                        src: r.3.into(),
                        snippet: r.4.into(),
                    })
                    .collect()
            },
            more: if empty { 0 } else { 3 },
            matches: if empty { 0 } else { 14 },
            convs: if empty { 0 } else { 9 },
            searched: 23,
            outside: 9,
            project: "nova-api".into(),
            projects: 3,
            indexing: false,
        }
    }

    #[test]
    fn search_strip() {
        let t = transcript();
        let mut v = base();
        v.mode = Mode::Search(search_view(false));
        v.focus = Focus::Bar;
        insta::assert_snapshot!(snap(&v, 120, 40, Main::Lines(&t)));
    }

    fn prompts_view(step: PromptStep) -> PromptsView {
        let row = |name: &str, preview: &str, clipboard: bool| PromptRow {
            name: name.into(),
            preview: preview.into(),
            clipboard,
        };
        PromptsView {
            query: String::new(),
            sel: 0,
            rows: vec![
                row(
                    "investigate ticket",
                    "Investigate this ticket before changing any code. Read the code it touches.",
                    true,
                ),
                row(
                    "review diff",
                    "Review this diff for edge cases and missing tests.",
                    false,
                ),
                row(
                    "write tests",
                    "Write tests for what you just changed.",
                    false,
                ),
            ],
            total: 3,
            target: "nova-api / auth-refactor".into(),
            clip: Clip::Text {
                first: "BUG-412: password reset email links expire immediately".into(),
                lines: 14,
            },
            step,
        }
    }

    fn inbox_view() -> InboxView {
        let row = |kind, project: &str, session: &str, detail: &str, age: &str| InboxRow {
            kind,
            project: project.into(),
            session: session.into(),
            detail: detail.into(),
            age: age.into(),
        };
        InboxView {
            rows: vec![
                row(
                    NoticeKind::Waiting,
                    "nova-api",
                    "migrate-staging",
                    "wants to run Bash",
                    "4m",
                ),
                row(
                    NoticeKind::Waiting,
                    "infra",
                    "dns-cutover",
                    "waiting for input",
                    "1m",
                ),
                row(
                    NoticeKind::Error,
                    "billing-svc",
                    "e2e-tests",
                    "exit 1",
                    "12m",
                ),
                row(NoticeKind::Done, "nova-api", "docs-pass", "finished", "2m"),
            ],
            sel: 0,
            more: 0,
            waiting: 2,
            total: 4,
        }
    }

    #[test]
    fn inbox_strip() {
        let t = transcript();
        let mut v = base();
        v.mode = Mode::Inbox(inbox_view());
        v.focus = Focus::Bar;
        insta::assert_snapshot!(snap(&v, 120, 40, Main::Lines(&t)));
    }

    #[test]
    fn inbox_strip_minimum() {
        let t = transcript();
        let mut v = base();
        v.layout = Layout::for_width(80, false);
        v.mode = Mode::Inbox(inbox_view());
        v.focus = Focus::Bar;
        insta::assert_snapshot!(snap(&v, 80, 24, Main::Lines(&t)));
    }

    #[test]
    fn prompts_strip() {
        let t = transcript();
        let mut v = base();
        v.mode = Mode::Prompts(prompts_view(PromptStep::List));
        v.focus = Focus::Bar;
        insta::assert_snapshot!(snap(&v, 120, 40, Main::Lines(&t)));
    }

    #[test]
    fn prompts_strip_typed_text_matches_nothing() {
        let t = transcript();
        let mut v = base();
        let mut pv = prompts_view(PromptStep::List);
        pv.query = "hey so how I work is I ask every agent to look into the project and check the structure of the codebase first".into();
        pv.rows.clear();
        pv.total = 1;
        pv.clip = Clip::Unused;
        v.mode = Mode::Prompts(pv);
        v.focus = Focus::Bar;
        insta::assert_snapshot!(snap(&v, 120, 40, Main::Lines(&t)));
    }

    #[test]
    fn prompts_strip_minimum_naming() {
        let t = transcript();
        let mut v = base();
        v.layout = Layout::for_width(80, false);
        v.mode = Mode::Prompts(prompts_view(PromptStep::Name {
            text: "triage".into(),
            from: NameFrom::Last,
        }));
        v.focus = Focus::Bar;
        insta::assert_snapshot!(snap(&v, 80, 24, Main::Lines(&t)));
    }

    #[test]
    fn search_strip_minimum() {
        let t = transcript();
        let mut v = base();
        v.layout = Layout::for_width(80, false);
        v.mode = Mode::Search(search_view(false));
        v.focus = Focus::Bar;
        insta::assert_snapshot!(snap(&v, 80, 24, Main::Lines(&t)));
    }

    #[test]
    fn search_no_matches() {
        let t = transcript();
        let mut v = base();
        v.mode = Mode::Search(search_view(true));
        v.focus = Focus::Bar;
        insta::assert_snapshot!(snap(&v, 120, 40, Main::Lines(&t)));
    }

    #[test]
    fn mark_and_send() {
        let t = transcript();
        let mut v = base();
        v.marking = Some(3);
        for (pi, si) in [(0, 0), (0, 2), (1, 2)] {
            v.projects[pi].sessions[si].marked = Some(true);
        }
        for p in v.projects.iter_mut() {
            for s in p.sessions.iter_mut() {
                s.marked.get_or_insert(false);
            }
        }
        v.mode = Mode::Mark { count: 3 };
        let marking = snap(&v, 120, 40, Main::Lines(&t));
        v.mode = Mode::Broadcast(Broadcast {
            targets: vec![
                Target {
                    project: "nova-api".into(),
                    session: "auth-refactor".into(),
                    status: Status::Running,
                },
                Target {
                    project: "nova-api".into(),
                    session: "docs-pass".into(),
                    status: Status::Idle,
                },
                Target {
                    project: "billing-svc".into(),
                    session: "a11y-audit".into(),
                    status: Status::Idle,
                },
            ],
            more: 0,
            text: "update the README env var table to match .env.example".into(),
        });
        v.focus = Focus::Bar;
        let sending = snap(&v, 120, 40, Main::Lines(&t));
        insta::assert_snapshot!(format!("{marking}\n{sending}"));
    }

    #[test]
    fn shell_drawer_bars() {
        let mut v = base();
        let sh =
            |open, running: Option<&str>, done: Option<(&str, bool, &str)>, resize| ShellView {
                open,
                path: "~/code/nova-api".into(),
                shell: "zsh".into(),
                running: running.map(String::from),
                done: done.map(|(a, b, c)| (a.to_string(), b, c.to_string())),
                resize,
            };
        let mut out = String::new();
        let mut line = |v: &View, w: u16| {
            let h = bar::height(v);
            let area = Rect::new(0, 0, w, h);
            let mut buf = Buffer::empty(area);
            bar::render(v, &mut Painter::new(&mut buf, area, v.color));
            out.push_str(&to_text(&buf));
        };
        v.focus = Focus::Main;
        v.runnable = Some((1, 3));
        line(&v, 120);
        v.focus = Focus::Shell;
        v.runnable = Some((2, 3));
        v.shell = Some(sh(true, None, None, None));
        line(&v, 120);
        v.shell = Some(sh(true, None, None, Some((11, 23))));
        line(&v, 120);
        v.focus = Focus::Main;
        v.runnable = None;
        v.shell = Some(sh(false, Some("npm run dev"), None, None));
        line(&v, 120);
        v.shell = Some(sh(
            false,
            None,
            Some((
                "npm run dev",
                true,
                "Error: connect ECONNREFUSED 127.0.0.1:6379",
            )),
            None,
        ));
        line(&v, 120);
        insta::assert_snapshot!(out);
    }

    #[test]
    fn update_notice_in_status_bar() {
        let mut v = base();
        v.update = Some("0.2.0".into());
        let area = Rect::new(0, 0, 120, 1);
        let mut buf = Buffer::empty(area);
        bar::render(&v, &mut Painter::new(&mut buf, area, v.color));
        let text = to_text(&buf);
        assert!(text.contains("pebl 0.2.0 is out · pebl update"), "{text}");
    }

    #[test]
    fn peek_strip() {
        let mut v = base();
        v.mode = Mode::Peek(Peek {
            project: "nova-api".into(),
            session: "migrate-staging".into(),
            status: Some(Status::Waiting),
            lines: vec![
                "╭──────────────────────────────────────────╮".into(),
                "│ Bash command                             │".into(),
                "│   npm run migrate -- --env=staging       │".into(),
                "│ Do you want to proceed?                  │".into(),
                "│ ❯ 1. Yes                                 │".into(),
                "│   2. No, and tell Claude what to do      │".into(),
                "╰──────────────────────────────────────────╯".into(),
            ],
            text: String::new(),
        });
        v.focus = Focus::Bar;
        let t = transcript();
        insta::assert_snapshot!(snap(&v, 120, 40, Main::Lines(&t)));
    }

    #[test]
    fn layout_breakpoints() {
        assert_eq!(Layout::for_width(120, false), Layout::Full(30));
        assert_eq!(Layout::for_width(200, false), Layout::Full(36));
        assert_eq!(Layout::for_width(100, false), Layout::Full(25));
        assert_eq!(Layout::for_width(80, false), Layout::Compact(22));
        assert_eq!(
            Layout::for_width(70, false),
            Layout::Hidden { narrow: true }
        );
    }
}
