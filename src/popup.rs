use crate::ui::theme::ColorMode;
use crate::ui::Painter;
use anyhow::Result;
use crossterm::event::{self, Event, KeyCode, KeyEventKind, KeyModifiers, MouseEventKind};
use crossterm::{cursor, execute, terminal};
use ratatui::backend::CrosstermBackend;
use ratatui::Terminal;
use std::time::Duration;

pub trait Screen {
    fn draw(&mut self, p: &mut Painter);
    fn key(&mut self, code: KeyCode, mods: KeyModifiers, page: i64) -> bool;
    fn scroll(&mut self, by: i64);
}

pub fn run(screen: &mut dyn Screen, color: ColorMode) -> Result<()> {
    terminal::enable_raw_mode()?;
    execute!(
        std::io::stdout(),
        terminal::EnterAlternateScreen,
        event::EnableMouseCapture,
        cursor::Hide
    )?;
    let res = (|| -> Result<()> {
        let mut term = Terminal::new(CrosstermBackend::new(std::io::stdout()))?;
        loop {
            let mut page = 10;
            term.draw(|f| {
                let area = f.area();
                page = area.height.saturating_sub(5).max(1) as i64;
                let mut p = Painter::new(f.buffer_mut(), area, color);
                screen.draw(&mut p);
            })?;
            if !event::poll(Duration::from_millis(500))? {
                continue;
            }
            match event::read()? {
                Event::Key(k) if k.kind != KeyEventKind::Release => {
                    if screen.key(k.code, k.modifiers, page) {
                        return Ok(());
                    }
                }
                Event::Mouse(m) => match m.kind {
                    MouseEventKind::ScrollDown => screen.scroll(3),
                    MouseEventKind::ScrollUp => screen.scroll(-3),
                    _ => {}
                },
                _ => {}
            }
        }
    })();
    let _ = execute!(
        std::io::stdout(),
        cursor::Show,
        event::DisableMouseCapture,
        terminal::LeaveAlternateScreen
    );
    let _ = terminal::disable_raw_mode();
    res
}
