use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::io::{BufRead, BufReader, Read, Write};
use std::os::unix::net::UnixStream;
use std::path::Path;
use std::time::Duration;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Role {
    Sidebar,
    Stage,
    Bar,
    Tabs,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Hello {
    View {
        role: Role,
    },
    Hook {
        sid: String,
        event: String,
        payload: serde_json::Value,
    },
    Ctl {
        args: Vec<String>,
    },
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Input {
    Key(Key),
    Click { x: u16, y: u16 },
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Key {
    pub code: KeyCode,
    #[serde(default)]
    pub ctrl: bool,
    #[serde(default)]
    pub alt: bool,
    #[serde(default)]
    pub shift: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum KeyCode {
    Char(char),
    Enter,
    Esc,
    Tab,
    BackTab,
    Backspace,
    Delete,
    Up,
    Down,
    Left,
    Right,
    Home,
    End,
    Other,
}

impl Key {
    pub fn plain(code: KeyCode) -> Self {
        Key {
            code,
            ctrl: false,
            alt: false,
            shift: false,
        }
    }

    pub fn from_crossterm(k: crossterm::event::KeyEvent) -> Self {
        use crossterm::event::{KeyCode as C, KeyModifiers as M};
        let code = match k.code {
            C::Char(c) => KeyCode::Char(c),
            C::Enter => KeyCode::Enter,
            C::Esc => KeyCode::Esc,
            C::Tab => KeyCode::Tab,
            C::BackTab => KeyCode::BackTab,
            C::Backspace => KeyCode::Backspace,
            C::Delete => KeyCode::Delete,
            C::Up => KeyCode::Up,
            C::Down => KeyCode::Down,
            C::Left => KeyCode::Left,
            C::Right => KeyCode::Right,
            C::Home => KeyCode::Home,
            C::End => KeyCode::End,
            _ => KeyCode::Other,
        };
        Key {
            code,
            ctrl: k.modifiers.contains(M::CONTROL),
            alt: k.modifiers.contains(M::ALT),
            shift: k.modifiers.contains(M::SHIFT),
        }
    }
}

pub fn write_line<T: Serialize>(w: &mut impl Write, msg: &T) -> Result<()> {
    let mut s = serde_json::to_string(msg)?;
    s.push('\n');
    w.write_all(s.as_bytes())?;
    w.flush()?;
    Ok(())
}

pub fn connect(sock: &Path) -> Result<UnixStream> {
    UnixStream::connect(sock).with_context(|| format!("connecting to {}", sock.display()))
}

pub fn ctl(sock: &Path, args: &[String]) -> Result<String> {
    let mut s = connect(sock)?;
    s.set_read_timeout(Some(Duration::from_secs(15)))?;
    write_line(
        &mut s,
        &Hello::Ctl {
            args: args.to_vec(),
        },
    )?;
    s.shutdown(std::net::Shutdown::Write)?;
    let mut out = String::new();
    s.read_to_string(&mut out)?;
    Ok(out)
}

pub fn read_lines<T: for<'de> Deserialize<'de>>(r: impl Read, mut f: impl FnMut(T) -> bool) {
    for line in BufReader::new(r).lines() {
        let Ok(line) = line else { break };
        if let Ok(v) = serde_json::from_str::<T>(&line) {
            if !f(v) {
                break;
            }
        }
    }
}
