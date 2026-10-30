use crate::model::{age, now};
use crate::timeline::Event;
use crate::ui::theme::{ColorMode, Tok};
use crate::ui::{fit, keys, seg, segb, Painter};
use crate::{ipc, paths};
use anyhow::Result;
use crossterm::event::{KeyCode, KeyModifiers};

const TOOLS_SHOWN: usize = 5;

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Group {
    pub turn: Option<Turn>,
    pub prompts: Vec<String>,
    pub tools: Vec<(String, String)>,
    pub checks: Option<(bool, String)>,
    pub restored: Option<usize>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Turn {
    pub n: usize,
    pub at: u64,
    pub checkpoint: Option<String>,
    pub files: usize,
    pub adds: usize,
    pub dels: usize,
}

pub fn group(events: &[Event]) -> Vec<Group> {
    let mut out: Vec<Group> = Vec::new();
    let mut open = Group::default();
    let mut n = 0;
    for e in events {
        match e {
            Event::Prompt { text, .. } => open.prompts.push(text.clone()),
            Event::Tool { name, detail, .. } => open.tools.push((name.clone(), detail.clone())),
            Event::Checks {
                passed, summary, ..
            } => {
                let quiet = open.prompts.is_empty() && open.tools.is_empty();
                match out.last_mut() {
                    Some(last) if quiet && last.checks.is_none() => {
                        last.checks = Some((*passed, summary.clone()))
                    }
                    _ => open.checks = Some((*passed, summary.clone())),
                }
            }
            Event::Turn {
                at,
                checkpoint,
                files,
                adds,
                dels,
            } => {
                n += 1;
                open.turn = Some(Turn {
                    n,
                    at: *at,
                    checkpoint: checkpoint.clone(),
                    files: files.len(),
                    adds: *adds,
                    dels: *dels,
                });
                out.push(std::mem::take(&mut open));
            }
            Event::Restore { to, .. } => {
                let target = out.iter().find_map(|g| {
                    g.turn
                        .as_ref()
                        .filter(|t| t.checkpoint.as_deref() == Some(to.as_str()))
                        .map(|t| t.n)
                });
                out.push(Group {
                    restored: Some(target.unwrap_or(0)),
                    ..Default::default()
                });
            }
        }
    }
    if open != Group::default() {
        out.push(open);
    }
    out
}

pub struct Timeline {
    title: String,
    groups: Vec<Group>,
    cur: usize,
    top: usize,
    armed: bool,
    msg: Option<String>,
    now: u64,
}

impl Timeline {
    pub fn new(title: String, events: &[Event], now: u64) -> Self {
        let groups = group(events);
        let cur = groups.len().saturating_sub(1);
        Timeline {
            title,
            groups,
            cur,
            top: 0,
            armed: false,
            msg: None,
            now,
        }
    }

    fn lines(&self, i: usize) -> usize {
        let g = &self.groups[i];
        1 + g.prompts.len().min(2) + usize::from(!g.tools.is_empty())
    }

    pub fn draw(&mut self, p: &mut Painter) {
        let (w, h) = (p.w(), p.h());
        let x = p.segs(1, 0, &[seg("✻ ", Tok::Acc), segb("Timeline  ", Tok::Fg)]);
        p.t(
            x,
            0,
            &fit(&self.title, (w - x - 2).max(1) as usize),
            Tok::Mute,
            false,
        );
        let turns = self.groups.iter().filter(|g| g.turn.is_some()).count();
        p.rt(w - 1, 0, &format!("{turns} turns"), Tok::Dim, false);
        p.hl(0, 1, w);
        let body = (h - 4).max(1) as usize;
        if self.groups.is_empty() {
            p.t(
                2,
                3,
                "Nothing recorded yet: the timeline starts with this session's next prompt.",
                Tok::Mute,
                false,
            );
        }
        let mut used = 0;
        for i in self.top..=self.cur.min(self.groups.len().saturating_sub(1)) {
            used += self.lines(i);
        }
        while used > body && self.top < self.cur {
            used -= self.lines(self.top);
            self.top += 1;
        }
        if self.cur < self.top {
            self.top = self.cur;
        }
        let mut y = 2;
        for i in self.top..self.groups.len() {
            if y + self.lines(i) as i32 > h - 2 {
                break;
            }
            y = self.draw_group(p, i, y, w);
        }
        p.hl(0, h - 2, w);
        if let Some(m) = &self.msg {
            p.t(1, h - 1, &fit(m, (w - 2).max(1) as usize), Tok::Acc, false);
        } else {
            p.segs(
                0,
                h - 1,
                &keys(&[
                    ("j/k", "move"),
                    ("r", "restore files to this turn"),
                    ("q", "close"),
                ]),
            );
        }
    }

    fn draw_group(&self, p: &mut Painter, i: usize, mut y: i32, w: i32) -> i32 {
        let g = &self.groups[i];
        let sel = i == self.cur;
        if sel {
            p.bg(0, y, w, Tok::Sel);
            p.t(0, y, "›", Tok::Acc, true);
        }
        match (&g.turn, g.restored) {
            (_, Some(n)) => {
                let to = if n == 0 {
                    "an earlier turn".to_string()
                } else {
                    format!("turn {n}")
                };
                p.t(
                    2,
                    y,
                    &format!("↺ restored the files to {to}"),
                    Tok::Acc,
                    false,
                );
            }
            (Some(t), _) => {
                let (g_, tok) = if t.checkpoint.is_some() {
                    ("●", Tok::Ok)
                } else {
                    ("○", Tok::Dim)
                };
                p.t(2, y, g_, tok, false);
                let e = p.segs(
                    4,
                    y,
                    &[
                        segb(format!("Turn {}", t.n), Tok::Fg),
                        seg(
                            format!("  {} ago", age(self.now.saturating_sub(t.at))),
                            Tok::Dim,
                        ),
                    ],
                );
                let change = if t.checkpoint.is_some() {
                    vec![
                        seg(
                            match t.files {
                                1 => "   1 file ".to_string(),
                                n => format!("   {n} files "),
                            },
                            Tok::Mute,
                        ),
                        seg(format!("+{}", t.adds), Tok::Ok),
                        seg(format!(" −{}", t.dels), Tok::Err),
                    ]
                } else {
                    vec![seg("   no file changes", Tok::Dim)]
                };
                let e = p.segs(e, y, &change);
                if let Some((ok, s)) = &g.checks {
                    let (mark, tok) = if *ok {
                        ("✓", Tok::Ok)
                    } else {
                        ("✕", Tok::Err)
                    };
                    p.t(e + 3, y, &format!("{mark} {s}"), tok, false);
                }
            }
            (None, None) => {
                p.t(2, y, "…", Tok::Acc, false);
                p.t(4, y, "in progress", Tok::Mute, false);
            }
        }
        y += 1;
        for prompt in g.prompts.iter().take(2) {
            let line = prompt.lines().find(|l| !l.trim().is_empty()).unwrap_or("");
            p.t(4, y, "›", Tok::Dim, false);
            p.t(
                6,
                y,
                &fit(line.trim(), (w - 7).max(1) as usize),
                Tok::Fg,
                false,
            );
            y += 1;
        }
        if !g.tools.is_empty() {
            let mut text: Vec<String> = g
                .tools
                .iter()
                .take(TOOLS_SHOWN)
                .map(|(n, d)| {
                    if d.is_empty() {
                        n.clone()
                    } else {
                        format!("{n} {d}")
                    }
                })
                .collect();
            if g.tools.len() > TOOLS_SHOWN {
                text.push(format!("+{} more", g.tools.len() - TOOLS_SHOWN));
            }
            p.t(
                6,
                y,
                &fit(&text.join(" · "), (w - 7).max(1) as usize),
                Tok::Dim,
                false,
            );
            y += 1;
        }
        y
    }

    fn step(&mut self, by: i64) {
        let n = self.groups.len() as i64;
        if n > 0 {
            self.cur = (self.cur as i64 + by).clamp(0, n - 1) as usize;
        }
    }

    fn key(&mut self, code: KeyCode, sid: &str) -> bool {
        self.msg = None;
        let armed = std::mem::take(&mut self.armed);
        match code {
            KeyCode::Char('q') | KeyCode::Esc => return true,
            KeyCode::Char('j') | KeyCode::Down => self.step(1),
            KeyCode::Char('k') | KeyCode::Up => self.step(-1),
            KeyCode::Char('g') | KeyCode::Home => self.cur = 0,
            KeyCode::Char('G') | KeyCode::End => self.step(i64::MAX / 2),
            KeyCode::Char('r') => {
                let turn = self.groups.get(self.cur).and_then(|g| g.turn.clone());
                match turn {
                    Some(Turn {
                        checkpoint: Some(c),
                        n,
                        ..
                    }) if armed => {
                        let reply = ipc::ctl(&paths::sock(), &["restore".into(), sid.into(), c]);
                        self.msg = Some(match reply {
                            Ok(r) if r.starts_with("error") => r,
                            Ok(_) => format!("restored the files to turn {n}; the state before is saved as a turn too"),
                            Err(e) => format!("{e:#}"),
                        });
                    }
                    Some(Turn {
                        checkpoint: Some(_),
                        n,
                        ..
                    }) => {
                        self.armed = true;
                        self.msg = Some(format!(
                            "r again puts the files back to how they were after turn {n} (what's there now is saved first)"
                        ));
                    }
                    _ => self.msg = Some("pick a turn that changed files".into()),
                }
            }
            _ => {}
        }
        false
    }
}

struct TimelineScreen {
    timeline: Timeline,
    sid: String,
}

impl crate::popup::Screen for TimelineScreen {
    fn draw(&mut self, p: &mut Painter) {
        self.timeline.draw(p);
    }

    fn key(&mut self, code: KeyCode, _: KeyModifiers, _: i64) -> bool {
        let done = self.timeline.key(code, &self.sid);
        if !done
            && self
                .timeline
                .msg
                .as_deref()
                .is_some_and(|m| m.starts_with("restored"))
        {
            let events = crate::timeline::read(&paths::timeline(), &self.sid);
            let cur = self.timeline.cur;
            let msg = self.timeline.msg.take();
            self.timeline = Timeline::new(self.timeline.title.clone(), &events, now());
            self.timeline.cur = self.timeline.cur.max(cur);
            self.timeline.msg = msg;
        }
        done
    }

    fn scroll(&mut self, by: i64) {
        self.timeline.step(by.signum());
    }
}

pub fn run(sid: &str, title: &str, color: ColorMode) -> Result<()> {
    let events = crate::timeline::read(&paths::timeline(), sid);
    let mut screen = TimelineScreen {
        timeline: Timeline::new(title.to_string(), &events, now()),
        sid: sid.to_string(),
    };
    crate::popup::run(&mut screen, color)
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::buffer::Buffer;
    use ratatui::layout::Rect;

    fn events() -> Vec<Event> {
        let tool = |at, name: &str, detail: &str| Event::Tool {
            at,
            name: name.into(),
            detail: detail.into(),
        };
        vec![
            Event::Prompt {
                at: 100,
                text: "Fix the rota export for night shifts".into(),
            },
            tool(110, "Read", "utils/exports/rota.js"),
            tool(120, "Edit", "utils/exports/rota.js"),
            tool(130, "Bash", "npm test"),
            Event::Turn {
                at: 200,
                checkpoint: Some("c1".into()),
                files: vec!["utils/exports/rota.js".into()],
                adds: 18,
                dels: 4,
            },
            Event::Checks {
                at: 210,
                passed: false,
                summary: "test failed".into(),
            },
            Event::Prompt {
                at: 300,
                text: "The project's checks failed after your last change.".into(),
            },
            tool(310, "Edit", "utils/exports/rota.test.js"),
            Event::Turn {
                at: 400,
                checkpoint: Some("c2".into()),
                files: vec!["a".into(), "b".into()],
                adds: 30,
                dels: 6,
            },
            Event::Checks {
                at: 410,
                passed: true,
                summary: "test passed".into(),
            },
            Event::Prompt {
                at: 500,
                text: "What does this file do?".into(),
            },
            Event::Turn {
                at: 520,
                checkpoint: None,
                files: vec![],
                adds: 0,
                dels: 0,
            },
            Event::Restore {
                at: 600,
                to: "c1".into(),
                saved: "c3".into(),
            },
            Event::Prompt {
                at: 700,
                text: "Try a different approach".into(),
            },
            tool(710, "Grep", "nightShift"),
        ]
    }

    #[test]
    fn groups_events_into_turns() {
        let g = group(&events());
        assert_eq!(g.len(), 5);
        assert_eq!(g[0].turn.as_ref().unwrap().n, 1);
        assert_eq!(g[0].checks, Some((false, "test failed".into())));
        assert_eq!(g[1].checks, Some((true, "test passed".into())));
        assert!(g[2].turn.as_ref().unwrap().checkpoint.is_none());
        assert_eq!(g[3].restored, Some(1));
        assert!(g[4].turn.is_none() && g[4].tools.len() == 1);
    }

    #[test]
    fn timeline_screen() {
        let mut t = Timeline::new("nova-api / rota-export".into(), &events(), 1000);
        t.cur = 1;
        let area = Rect::new(0, 0, 100, 18);
        let mut buf = Buffer::empty(area);
        t.draw(&mut Painter::new(&mut buf, area, ColorMode::Truecolor));
        insta::assert_snapshot!(crate::ui::screen::to_text(&buf));
    }
}
