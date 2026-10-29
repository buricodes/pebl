use ratatui::style::Color;
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Tok {
    Fg,
    Mute,
    Dim,
    Line,
    Acc,
    Ok,
    Err,
    Sel,
    Ink,
    AccDim,
    AccLight,
    Add,
    Del,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ColorMode {
    #[default]
    Truecolor,
    Indexed,
}

impl Tok {
    pub fn rgb(self) -> (u8, u8, u8) {
        match self {
            Tok::Fg => (0xe8, 0xe6, 0xe1),
            Tok::Mute => (0x95, 0x90, 0x89),
            Tok::Dim => (0x64, 0x60, 0x5b),
            Tok::Line => (0x3b, 0x39, 0x36),
            Tok::Acc => (0xd7, 0x77, 0x57),
            Tok::Ok => (0x5f, 0xb8, 0x6f),
            Tok::Err => (0xf0, 0x67, 0x7b),
            Tok::Sel => (0x2b, 0x24, 0x21),
            Tok::Ink => (0x14, 0x14, 0x13),
            Tok::AccDim => (0xa3, 0x56, 0x3d),
            Tok::AccLight => (0xe8, 0x95, 0x7a),
            Tok::Add => (0x1e, 0x33, 0x24),
            Tok::Del => (0x3d, 0x1f, 0x25),
        }
    }

    pub fn idx(self) -> u8 {
        match self {
            Tok::Fg => 254,
            Tok::Mute => 246,
            Tok::Dim => 241,
            Tok::Line => 237,
            Tok::Acc => 173,
            Tok::Ok => 71,
            Tok::Err => 204,
            Tok::Sel => 235,
            Tok::Ink => 233,
            Tok::AccDim => 131,
            Tok::AccLight => 174,
            Tok::Add => 22,
            Tok::Del => 52,
        }
    }

    pub fn color(self, mode: ColorMode) -> Color {
        match mode {
            ColorMode::Truecolor => {
                let (r, g, b) = self.rgb();
                Color::Rgb(r, g, b)
            }
            ColorMode::Indexed => Color::Indexed(self.idx()),
        }
    }

    pub fn tmux(self, mode: ColorMode) -> String {
        match mode {
            ColorMode::Truecolor => {
                let (r, g, b) = self.rgb();
                format!("#{r:02x}{g:02x}{b:02x}")
            }
            ColorMode::Indexed => format!("colour{}", self.idx()),
        }
    }
}
