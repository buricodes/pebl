use anyhow::{bail, Context, Result};
use std::io::Write;
use std::path::{Path, PathBuf};

pub const REPO_FILE: &str = "PEBL.md";
const HEADER: &str =
    "# Project notes\n\nNotes kept with pebl for this project. Follow them in every task here.\n\n";

pub fn file_for(store: &Path, project: &Path) -> PathBuf {
    let shared = project.join(REPO_FILE);
    if shared.exists() {
        return shared;
    }
    store.join(format!("{}.md", crate::project::slug(project)))
}

pub fn notes(file: &Path) -> usize {
    std::fs::read_to_string(file)
        .map(|s| s.lines().filter(|l| l.starts_with("- ")).count())
        .unwrap_or(0)
}

pub fn has_content(file: &Path) -> bool {
    std::fs::read_to_string(file)
        .map(|s| !s.trim().is_empty() && s != HEADER)
        .unwrap_or(false)
}

pub fn ensure(file: &Path) -> Result<()> {
    if file.exists() {
        return Ok(());
    }
    if let Some(dir) = file.parent() {
        std::fs::create_dir_all(dir).with_context(|| format!("creating {}", dir.display()))?;
    }
    std::fs::write(file, HEADER).with_context(|| format!("writing {}", file.display()))
}

pub fn add(file: &Path, note: &str) -> Result<()> {
    let note = note.split_whitespace().collect::<Vec<_>>().join(" ");
    if note.is_empty() {
        bail!("type a note first");
    }
    ensure(file)?;
    let text = std::fs::read_to_string(file).unwrap_or_default();
    let gap = if text.is_empty() || text.ends_with('\n') {
        ""
    } else {
        "\n"
    };
    let mut f = std::fs::OpenOptions::new()
        .append(true)
        .open(file)
        .with_context(|| format!("opening {}", file.display()))?;
    writeln!(f, "{gap}- {note}").with_context(|| format!("writing {}", file.display()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keeps_notes_outside_the_repo_unless_it_has_pebl_md() {
        let tmp = tempfile::tempdir().unwrap();
        let store = tmp.path().join("memory");
        let repo = tmp.path().join("code/lief-care");
        std::fs::create_dir_all(&repo).unwrap();
        let file = file_for(&store, &repo);
        assert!(file.starts_with(&store));
        assert!(file.ends_with(format!("{}.md", crate::project::slug(&repo))));
        assert!(!has_content(&file));
        std::fs::write(repo.join(REPO_FILE), "- shared\n").unwrap();
        assert_eq!(file_for(&store, &repo), repo.join(REPO_FILE));
    }

    #[test]
    fn adds_notes_as_a_list() {
        let tmp = tempfile::tempdir().unwrap();
        let file = tmp.path().join("m/notes.md");
        add(&file, "  we use   pnpm ").unwrap();
        add(&file, "don't touch /legacy").unwrap();
        let text = std::fs::read_to_string(&file).unwrap();
        assert!(text.starts_with("# Project notes"));
        assert!(text.ends_with("- we use pnpm\n- don't touch /legacy\n"));
        assert_eq!(notes(&file), 2);
        assert!(has_content(&file));
        assert!(add(&file, "   ").is_err());
        std::fs::write(&file, "edited by hand").unwrap();
        add(&file, "third").unwrap();
        assert_eq!(
            std::fs::read_to_string(&file).unwrap(),
            "edited by hand\n- third\n"
        );
    }
}
