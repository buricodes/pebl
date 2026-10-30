use crate::ipc::{connect, read_lines, write_line, Hello, Input, Key, KeyCode, Role};
use crate::paths;
use crate::ui::theme::Tok;
use crate::ui::{bar, sidebar, stage, tabs, Painter};
use crate::view::View;
use anyhow::Result;
use crossterm::event::{self, Event, KeyEventKind, MouseButton, MouseEventKind};
use crossterm::{cursor, execute, terminal};
use ratatui::backend::CrosstermBackend;
use ratatui::Terminal;
use std::io::Write;
use std::os::unix::net::UnixStream;
use std::sync::mpsc;
use std::sync::{Arc, Mutex};
use std::time::Duration;

type Writer = Arc<Mutex<Option<UnixStream>>>;

pub fn run(role: Role) -> Result<()> {
    terminal::enable_raw_mode()?;
    let mut out = std::io::stdout();
    execute!(out, terminal::EnterAlternateScreen, cursor::Hide)?;
    let mouse = matches!(role, Role::Sidebar | Role::Tabs);
    if mouse {
        execute!(out, event::EnableMouseCapture)?;
    }
    let res = run_loop(role);
    if mouse {
        let _ = execute!(std::io::stdout(), event::DisableMouseCapture);
    }
    let _ = execute!(
        std::io::stdout(),
        cursor::Show,
        terminal::LeaveAlternateScreen
    );
    let _ = terminal::disable_raw_mode();
    res
}

fn run_loop(role: Role) -> Result<()> {
    let mut term = Terminal::new(CrosstermBackend::new(std::io::stdout()))?;
    let (tx, rx) = mpsc::channel::<Option<View>>();
    let writer: Writer = Arc::new(Mutex::new(None));
    {
        let writer = writer.clone();
        std::thread::spawn(move || loop {
            if let Ok(mut s) = connect(&paths::sock()) {
                if write_line(&mut s, &Hello::View { role }).is_ok() {
                    if let Ok(w) = s.try_clone() {
                        *writer.lock().unwrap_or_else(|e| e.into_inner()) = Some(w);
                    }
                    read_lines::<View>(s, |v| tx.send(Some(v)).is_ok());
                }
                *writer.lock().unwrap_or_else(|e| e.into_inner()) = None;
                if tx.send(None).is_err() {
                    return;
                }
            }
            std::thread::sleep(Duration::from_millis(400));
        });
    }

    let mut view: Option<View> = None;
    let mut bell = None;
    let mut osc: Option<Option<u64>> = None;
    let mut redraw = true;
    let started = std::time::Instant::now();
    let mut last_frame = u64::MAX;
    loop {
        while let Ok(v) = rx.try_recv() {
            if let (Some(v), Role::Sidebar) = (&v, role) {
                if bell.is_some_and(|b| v.bell > b) {
                    let _ = std::io::stdout().write_all(b"\x07");
                }
                bell = Some(v.bell);
                let seq = v.osc.as_ref().map(|(n, _)| *n);
                if let (Some(prev), Some((n, text))) = (osc, &v.osc) {
                    if prev != Some(*n) {
                        let _ = std::io::stdout().write_all(osc_notification(text).as_bytes());
                    }
                }
                osc = Some(seq);
            }
            view = v;
            redraw = true;
        }
        if redraw {
            let frame = (started.elapsed().as_millis() / 250) as u64;
            last_frame = frame;
            term.draw(|f| {
                let area = f.area();
                let buf = f.buffer_mut();
                match &view {
                    Some(v) => {
                        let mut p = Painter::new(buf, area, v.color);
                        match role {
                            Role::Sidebar => sidebar::render(v, &mut p, frame),
                            Role::Stage => stage::render(v, &mut p),
                            Role::Bar => bar::render(v, &mut p),
                            Role::Tabs => tabs::render(v, &mut p),
                        }
                    }
                    None => {
                        let mut p = Painter::new(buf, area, Default::default());
                        if matches!(role, Role::Sidebar | Role::Stage) {
                            p.t(1, 1, "connecting to pebl…", Tok::Dim, false);
                        }
                    }
                }
            })?;
            redraw = false;
        }
        let animated = role == Role::Sidebar && view.as_ref().is_some_and(|v| v.mascot.is_some());
        if animated && (started.elapsed().as_millis() / 250) as u64 != last_frame {
            redraw = true;
        }
        if !event::poll(Duration::from_millis(30))? {
            continue;
        }
        let input = match event::read()? {
            Event::Key(k) if k.kind != KeyEventKind::Release => {
                Some(Input::Key(Key::from_crossterm(k)))
            }
            Event::Mouse(m) if m.kind == MouseEventKind::Down(MouseButton::Left) => {
                let h = term.size().map(|s| s.height).unwrap_or(0);
                let pet = view
                    .as_ref()
                    .and_then(|v| sidebar::pet_rows(v, h))
                    .is_some_and(|(a, b)| m.row >= a && m.row <= b);
                if role == Role::Tabs {
                    Some(Input::Click {
                        x: m.column,
                        y: m.row,
                    })
                } else if pet {
                    let _ = crate::ipc::ctl(&paths::sock(), &["waiting".into()]);
                    None
                } else if m.row + 2 == h {
                    Some(Input::Key(Key::plain(KeyCode::Char('n'))))
                } else if m.row + 1 == h {
                    Some(Input::Key(Key::plain(KeyCode::Char('o'))))
                } else {
                    let w = term.size().map(|s| s.width).unwrap_or(0);
                    view.as_ref()
                        .and_then(|v| sidebar::list_row(v, w, h, m.row))
                        .map(|y| Input::Click { x: m.column, y })
                }
            }
            Event::Resize(..) => {
                redraw = true;
                None
            }
            _ => None,
        };
        if let Some(i) = input {
            if let Some(w) = writer.lock().unwrap_or_else(|e| e.into_inner()).as_mut() {
                let _ = write_line(w, &i);
            }
        }
    }
}

fn osc_notification(text: &str) -> String {
    let clean: String = text.chars().filter(|c| !c.is_control()).collect();
    format!("\x1bPtmux;\x1b\x1b]9;{clean}\x07\x1b\\")
}
