use anyhow::{bail, Context, Result};
use std::path::{Path, PathBuf};
use std::process::Command;

pub const CLIPBOARD: &str = "{{clipboard}}";

const STARTER_NAME: &str = "investigate ticket";
const STARTER: &str = "Investigate this ticket before changing any code. Read the code it touches, explain the likely cause, and propose a plan. Wait for my go-ahead before editing anything.\n\n{{clipboard}}\n";
const BLANK: &str = "Write your prompt here. Put {{clipboard}} where pasted text should go, or delete it.\n\n{{clipboard}}\n";

#[derive(Clone, Debug, PartialEq)]
pub struct Prompt {
    pub name: String,
    pub path: PathBuf,
    pub text: String,
}

impl Prompt {
    pub fn uses_clipboard(&self) -> bool {
        self.text.contains(CLIPBOARD)
    }

    pub fn preview(&self) -> String {
        self.text
            .lines()
            .map(str::trim)
            .find(|l| !l.is_empty() && *l != CLIPBOARD)
            .unwrap_or("")
            .to_string()
    }

    pub fn matches(&self, query: &str) -> bool {
        let q = query.trim().to_lowercase();
        q.is_empty()
            || self.name.to_lowercase().contains(&q)
            || self.text.to_lowercase().contains(&q)
    }

    pub fn fill(&self, clipboard: Option<&str>) -> Result<String> {
        let text = if self.uses_clipboard() {
            let clip = clipboard.map(str::trim).filter(|c| !c.is_empty());
            let Some(clip) = clip else {
                bail!("“{}” needs the clipboard: copy the text first", self.name);
            };
            self.text.replace(CLIPBOARD, clip)
        } else {
            self.text.clone()
        };
        Ok(text.trim().to_string())
    }
}

pub fn load(dir: &Path) -> Vec<Prompt> {
    if !dir.exists() && create(dir, STARTER_NAME, STARTER).is_err() {
        return Vec::new();
    }
    let mut out: Vec<Prompt> = std::fs::read_dir(dir)
        .into_iter()
        .flatten()
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|x| x == "md"))
        .filter_map(|path| {
            let text = std::fs::read_to_string(&path).ok()?;
            let name = path.file_stem()?.to_string_lossy().replace('-', " ");
            Some(Prompt { name, path, text })
        })
        .collect();
    out.sort_by_key(|p| p.name.to_lowercase());
    out
}

pub fn create(dir: &Path, name: &str, text: &str) -> Result<PathBuf> {
    let base = slug(name);
    if base.is_empty() {
        bail!("give the prompt a name");
    }
    std::fs::create_dir_all(dir).with_context(|| format!("creating {}", dir.display()))?;
    let path = (1..)
        .map(|n| match n {
            1 => dir.join(format!("{base}.md")),
            n => dir.join(format!("{base}-{n}.md")),
        })
        .find(|p| !p.exists())
        .unwrap_or_else(|| dir.join(format!("{base}.md")));
    std::fs::write(&path, text).with_context(|| format!("writing {}", path.display()))?;
    Ok(path)
}

pub fn create_blank(dir: &Path, name: &str) -> Result<PathBuf> {
    create(dir, name, BLANK)
}

pub fn slug(name: &str) -> String {
    let mut out = String::new();
    for c in name.trim().to_lowercase().chars() {
        if c.is_alphanumeric() {
            out.push(c);
        } else if !out.ends_with('-') {
            out.push('-');
        }
    }
    out.trim_matches('-').chars().take(60).collect()
}

pub fn clipboard() -> Option<String> {
    let tries: &[&[&str]] = if cfg!(target_os = "macos") {
        &[&["pbpaste"]]
    } else {
        &[
            &["wl-paste", "--no-newline"],
            &["xclip", "-selection", "clipboard", "-o"],
            &["xsel", "-ob"],
        ]
    };
    tries.iter().find_map(|argv| {
        let out = Command::new(argv[0]).args(&argv[1..]).output().ok()?;
        out.status
            .success()
            .then(|| String::from_utf8_lossy(&out.stdout).into_owned())
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn prompt(text: &str) -> Prompt {
        Prompt {
            name: "investigate ticket".into(),
            path: PathBuf::from("investigate-ticket.md"),
            text: text.into(),
        }
    }

    #[test]
    fn fills_the_clipboard_placeholder() {
        let p = prompt("Investigate first.\n\n{{clipboard}}\n");
        assert_eq!(
            p.fill(Some("  BUG-12 login loops\n")).unwrap(),
            "Investigate first.\n\nBUG-12 login loops"
        );
        assert!(p.fill(Some("   ")).is_err());
        assert!(p.fill(None).is_err());
        assert_eq!(prompt("Write tests.").fill(None).unwrap(), "Write tests.");
    }

    #[test]
    fn previews_and_matches() {
        let p = prompt("\n{{clipboard}}\nExplain the cause.\n");
        assert_eq!(p.preview(), "Explain the cause.");
        assert!(p.matches("TICKET"));
        assert!(p.matches("cause"));
        assert!(!p.matches("deploy"));
    }

    #[test]
    fn seeds_creates_and_lists_sorted() {
        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path().join("prompts");
        let seeded = load(&dir);
        assert_eq!(seeded.len(), 1);
        assert!(seeded[0].uses_clipboard());
        create(&dir, "Write Tests!", "Write tests for this.").unwrap();
        let again = create(&dir, "write tests", "Again.").unwrap();
        assert!(again.ends_with("write-tests-2.md"));
        let names: Vec<String> = load(&dir).into_iter().map(|p| p.name).collect();
        assert_eq!(
            names,
            vec!["investigate ticket", "write tests", "write tests 2"]
        );
        std::fs::remove_file(dir.join("investigate-ticket.md")).unwrap();
        assert_eq!(load(&dir).len(), 2);
        assert!(create(&dir, " !! ", "x").is_err());
    }
}
