use crate::ui::mascot;
use crate::ui::theme::{ColorMode, Tok};
use crate::ui::Painter;
use crate::{config, ipc, paths};
use anyhow::Result;
use crossterm::event::{self, Event, KeyCode, KeyEventKind};
use crossterm::{cursor, execute, terminal};
use ratatui::backend::CrosstermBackend;
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::Terminal;
use std::time::{Duration, Instant};

const LABEL: i32 = 10;
const COL: i32 = 12;
const ROW: i32 = mascot::CARD_H + 1;

pub fn size() -> (u16, u16) {
    (
        (LABEL + COL * mascot::MOODS.len() as i32 + 2) as u16,
        (5 + ROW * mascot::ALL.len() as i32 + 2) as u16,
    )
}

pub fn draw(p: &mut Painter, f: u64, sel: usize, current: Option<&str>) {
    p.segs(
        1,
        0,
        &[
            crate::ui::seg("✻ ", Tok::Acc),
            crate::ui::segb("pebl mascots", Tok::Fg),
        ],
    );
    p.t(
        1,
        1,
        "↑/↓ choose · enter use it · o turn off · q quit",
        Tok::Mute,
        false,
    );
    p.t(1, 2, mascot::ALL[sel].note, Tok::Dim, false);
    for (mi, m) in mascot::MOODS.iter().enumerate() {
        p.t(
            LABEL + mi as i32 * COL,
            4,
            mascot::label(*m),
            Tok::Dim,
            false,
        );
    }
    for (ci, c) in mascot::ALL.iter().enumerate() {
        let y = 5 + ci as i32 * ROW;
        let picked = ci == sel;
        if picked {
            p.t(1, y + 1, "›", Tok::Acc, true);
        }
        let name_tok = if picked { Tok::Fg } else { Tok::Mute };
        p.t(3, y + 1, c.name, name_tok, picked);
        if current == Some(c.name) {
            p.t(3, y + 2, "in use", Tok::Acc, false);
        }
        for (mi, m) in mascot::MOODS.iter().enumerate() {
            mascot::art(p, c, *m, f + mi as u64 * 3, LABEL + mi as i32 * COL, y);
        }
    }
}

pub fn run() -> Result<()> {
    let cfg = config::load(&paths::config())?;
    let mut current = cfg.mascot();
    let mut sel = current
        .as_deref()
        .and_then(|n| mascot::ALL.iter().position(|c| c.name == n))
        .unwrap_or(0);
    let mode = if crate::truecolor() {
        ColorMode::Truecolor
    } else {
        ColorMode::Indexed
    };

    terminal::enable_raw_mode()?;
    execute!(
        std::io::stdout(),
        terminal::EnterAlternateScreen,
        cursor::Hide
    )?;
    let mut term = Terminal::new(CrosstermBackend::new(std::io::stdout()))?;
    let started = Instant::now();
    let mut message: Option<String> = None;
    let res = (|| -> Result<()> {
        loop {
            let f = (started.elapsed().as_millis() / 250) as u64;
            term.draw(|fr| {
                let area = fr.area();
                let mut p = Painter::new(fr.buffer_mut(), area, mode);
                draw(&mut p, f, sel, current.as_deref());
                if let Some(m) = &message {
                    p.t(1, size().1 as i32 - 1, m, Tok::Acc, false);
                }
            })?;
            if !event::poll(Duration::from_millis(50))? {
                continue;
            }
            let Event::Key(k) = event::read()? else {
                continue;
            };
            if k.kind == KeyEventKind::Release {
                continue;
            }
            match k.code {
                KeyCode::Up | KeyCode::Char('k') => sel = sel.saturating_sub(1),
                KeyCode::Down | KeyCode::Char('j') => sel = (sel + 1).min(mascot::ALL.len() - 1),
                KeyCode::Enter => {
                    let name = mascot::ALL[sel].name;
                    save(name)?;
                    current = Some(name.into());
                    message = Some(format!("{name} is now in your sidebar"));
                }
                KeyCode::Char('o') => {
                    save("off")?;
                    current = None;
                    message = Some("mascot turned off".into());
                }
                KeyCode::Char('q') | KeyCode::Esc => return Ok(()),
                KeyCode::Char('c') if k.modifiers.contains(event::KeyModifiers::CONTROL) => {
                    return Ok(())
                }
                _ => {}
            }
        }
    })();
    let _ = execute!(
        std::io::stdout(),
        cursor::Show,
        terminal::LeaveAlternateScreen
    );
    let _ = terminal::disable_raw_mode();
    res
}

fn save(name: &str) -> Result<()> {
    let path = paths::config();
    let mut table: toml::Table = std::fs::read_to_string(&path)
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or_default();
    table.insert("mascot".into(), toml::Value::String(name.into()));
    std::fs::create_dir_all(paths::home())?;
    std::fs::write(&path, toml::to_string(&table)?)?;
    let _ = ipc::ctl(&paths::sock(), &["mascot".into(), name.into()]);
    Ok(())
}

pub fn svg(f: u64) -> String {
    let (w, h) = size();
    let mut buf = Buffer::empty(Rect::new(0, 0, w, h));
    {
        let mut p = Painter::new(&mut buf, Rect::new(0, 0, w, h), ColorMode::Truecolor);
        draw(&mut p, f, 0, Some(mascot::DEFAULT));
    }
    let (cw, ch) = (9.0, 18.0);
    let hex = |c: ratatui::style::Color| match c {
        ratatui::style::Color::Rgb(r, g, b) => Some(format!("#{r:02x}{g:02x}{b:02x}")),
        _ => None,
    };
    let mut out = format!(
        "<svg xmlns='http://www.w3.org/2000/svg' width='{}' height='{}'><rect width='100%' height='100%' fill='#141413'/>",
        w as f64 * cw,
        h as f64 * ch
    );
    for y in 0..h {
        for x in 0..w {
            let c = &buf[(x, y)];
            let (px, py) = (x as f64 * cw, y as f64 * ch);
            if let Some(bg) = hex(c.bg) {
                out.push_str(&format!(
                    "<rect x='{px}' y='{py}' width='{cw}' height='{ch}' fill='{bg}'/>"
                ));
            }
            let fg = hex(c.fg).unwrap_or_else(|| "#e8e6e1".into());
            match c.symbol() {
                " " => {}
                "█" => out.push_str(&format!(
                    "<rect x='{px}' y='{py}' width='{cw}' height='{ch}' fill='{fg}'/>"
                )),
                "▀" => out.push_str(&format!(
                    "<rect x='{px}' y='{py}' width='{cw}' height='{}' fill='{fg}'/>",
                    ch / 2.0
                )),
                "▄" => out.push_str(&format!(
                    "<rect x='{px}' y='{}' width='{cw}' height='{}' fill='{fg}'/>",
                    py + ch / 2.0,
                    ch / 2.0
                )),
                s => {
                    let s = s.replace('&', "&amp;").replace('<', "&lt;");
                    out.push_str(&format!(
                        "<text x='{px}' y='{}' font-family='Menlo' font-size='14' fill='{fg}'>{s}</text>",
                        py + 14.0
                    ));
                }
            }
        }
    }
    out.push_str("</svg>");
    out
}
