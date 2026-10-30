use super::{edit, App, IMode};
use crate::backend::{SessionBackend, UiBackend};
use crate::ipc::{Key, KeyCode};
use crate::memory;
use crate::view::Focus;
use anyhow::{anyhow, Result};
use std::path::PathBuf;

impl<B: SessionBackend + UiBackend> App<B> {
    pub(super) fn memory_file(&self, pid: &str) -> Option<PathBuf> {
        let project = self.st.project(pid)?;
        Some(memory::file_for(&self.env.memory, &project.path))
    }

    pub(super) fn memory_args(&self, pid: &str) -> Vec<String> {
        match self.memory_file(pid) {
            Some(f) if memory::has_content(&f) => vec![
                "--append-system-prompt-file".into(),
                f.display().to_string(),
            ],
            _ => Vec::new(),
        }
    }

    pub(super) fn start_note(&mut self) -> Result<()> {
        let pid = self
            .current_project()
            .ok_or_else(|| anyhow!("move to a project first"))?;
        self.cancel_mode();
        self.mode = IMode::Note {
            pid,
            text: String::new(),
        };
        self.set_focus(Focus::Sidebar);
        Ok(())
    }

    pub(super) fn note_key(&mut self, pid: String, mut text: String, k: Key) -> Result<()> {
        let file = self
            .memory_file(&pid)
            .ok_or_else(|| anyhow!("that project is gone"))?;
        match k.code {
            KeyCode::Esc => {
                self.mode = IMode::Normal;
                return Ok(());
            }
            KeyCode::Enter => {
                self.mode = IMode::Note {
                    pid: pid.clone(),
                    text: text.clone(),
                };
                memory::add(&file, &text)?;
                self.mode = IMode::Normal;
                let name = self
                    .st
                    .project(&pid)
                    .map(|p| p.name.clone())
                    .unwrap_or_default();
                let n = memory::notes(&file);
                self.flash(format!(
                    "noted for {name} ({n} in all) · new sessions start with these notes"
                ));
                return Ok(());
            }
            KeyCode::Char('e') if k.ctrl => {
                memory::ensure(&file)?;
                self.launch_editor(&file)?;
                self.mode = IMode::Normal;
                self.flash(format!("opened {} in {}", file.display(), self.env.editor));
                return Ok(());
            }
            KeyCode::Char('u') if k.ctrl => text.clear(),
            _ if k.ctrl => {}
            _ => edit(&mut text, &k, false),
        }
        self.mode = IMode::Note { pid, text };
        Ok(())
    }
}
