use anyhow::{bail, Context, Result};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

pub const SHARED_DIR: &str = ".pebl/recipes";
const CLIPBOARD: &str = "clipboard";

const STARTERS: [(&str, &str); 1] = [(
    "fix-ticket",
    r#"name = "fix ticket"
description = "Investigate, fix, test, review and commit a copied ticket"
steps = [
  "Investigate this ticket before changing anything. Read the code it touches, explain the likely cause and plan the fix. Don't change code yet.\n\n{{clipboard}}",
  "Implement the fix you planned. Keep the change small and follow the code around it.",
  "checks",
  "review",
  "Commit your changes with a clear message that says what was wrong and how you fixed it.",
]
"#,
)];

#[derive(Clone, Debug, PartialEq, serde::Deserialize)]
pub struct Recipe {
    pub name: String,
    #[serde(default)]
    pub description: String,
    pub steps: Vec<String>,
    #[serde(skip)]
    pub path: PathBuf,
}

#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Step {
    Prompt(String),
    Checks,
    Review,
}

impl Step {
    pub fn parse(text: &str) -> Step {
        match text.trim().to_lowercase().as_str() {
            "checks" | "run checks" => Step::Checks,
            "review" => Step::Review,
            "pr" | "open pr" | "open a pr" => Step::Prompt(
                "Commit any remaining changes, push this branch and open a pull request with the gh CLI. Reply with its URL.".into(),
            ),
            _ => Step::Prompt(text.trim().to_string()),
        }
    }

    pub fn label(&self) -> String {
        match self {
            Step::Checks => "checks".into(),
            Step::Review => "review".into(),
            Step::Prompt(p) => crate::model::Task::title_of(p),
        }
    }
}

fn substitute(text: &str, value: &mut dyn FnMut(&str) -> Option<String>) -> String {
    let mut out = String::new();
    let mut rest = text;
    while let Some(i) = rest.find("{{") {
        let after = &rest[i + 2..];
        let Some(j) = after.find("}}") else { break };
        out.push_str(&rest[..i]);
        let name = after[..j].trim();
        match value(name) {
            Some(v) => out.push_str(&v),
            None => out.push_str(&rest[i..i + 2 + j + 2]),
        }
        rest = &after[j + 2..];
    }
    out.push_str(rest);
    out
}

impl Recipe {
    pub fn vars(&self) -> Vec<String> {
        let mut out: Vec<String> = Vec::new();
        for step in &self.steps {
            substitute(step, &mut |name| {
                if !name.is_empty() && name != CLIPBOARD && !out.iter().any(|n| n == name) {
                    out.push(name.to_string());
                }
                None
            });
        }
        out
    }

    pub fn uses_clipboard(&self) -> bool {
        let mut found = false;
        for step in &self.steps {
            substitute(step, &mut |name| {
                found |= name == CLIPBOARD;
                None
            });
        }
        found
    }

    pub fn fill(
        &self,
        values: &BTreeMap<String, String>,
        clipboard: Option<&str>,
    ) -> Result<Vec<Step>> {
        let clip = clipboard.map(str::trim).filter(|c| !c.is_empty());
        if self.uses_clipboard() && clip.is_none() {
            bail!("“{}” needs the clipboard: copy the text first", self.name);
        }
        Ok(self
            .steps
            .iter()
            .map(|s| {
                Step::parse(&substitute(s, &mut |name| {
                    if name == CLIPBOARD {
                        clip.map(str::to_string)
                    } else {
                        values.get(name).cloned()
                    }
                }))
            })
            .collect())
    }
}

pub fn dirs(personal: &Path, project: Option<&Path>) -> Vec<PathBuf> {
    let mut out = vec![personal.to_path_buf()];
    if let Some(p) = project {
        out.push(p.join(SHARED_DIR));
    }
    out
}

pub fn load(personal: &Path, project: Option<&Path>) -> Vec<Recipe> {
    if !personal.exists() && seed(personal).is_err() {
        return Vec::new();
    }
    let mut out: Vec<Recipe> = Vec::new();
    for dir in dirs(personal, project) {
        let Ok(entries) = std::fs::read_dir(&dir) else {
            continue;
        };
        let mut found: Vec<Recipe> = entries
            .flatten()
            .map(|e| e.path())
            .filter(|p| p.extension().is_some_and(|x| x == "toml"))
            .filter_map(|path| {
                let text = std::fs::read_to_string(&path).ok()?;
                let mut r: Recipe = toml::from_str(&text).ok()?;
                r.path = path;
                (!r.steps.is_empty()).then_some(r)
            })
            .collect();
        found.sort_by_key(|r| r.name.to_lowercase());
        out.extend(found);
    }
    out
}

fn seed(dir: &Path) -> Result<()> {
    std::fs::create_dir_all(dir).with_context(|| format!("creating {}", dir.display()))?;
    for (file, text) in STARTERS {
        std::fs::write(dir.join(format!("{file}.toml")), text)?;
    }
    Ok(())
}

pub fn install(dir: &Path, text: &str) -> Result<PathBuf> {
    let r: Recipe =
        toml::from_str(text).context("that isn't a recipe (it needs name and steps)")?;
    if r.steps.is_empty() {
        bail!("the recipe has no steps");
    }
    let file = crate::prompts::slug(&r.name);
    if file.is_empty() {
        bail!("the recipe needs a name");
    }
    std::fs::create_dir_all(dir)?;
    let path = dir.join(format!("{file}.toml"));
    std::fs::write(&path, text)?;
    Ok(path)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn recipe() -> Recipe {
        toml::from_str(
            r#"name = "fix issue"
steps = ["read issue {{issue}} in {{ repo }} and plan", "implement it", "run checks", "Review", "open PR", "{{clipboard}} again {{issue}}"]"#,
        )
        .unwrap()
    }

    #[test]
    fn finds_variables_and_fills_steps() {
        let r = recipe();
        assert_eq!(r.vars(), vec!["issue", "repo"]);
        assert!(r.uses_clipboard());
        assert!(r.fill(&BTreeMap::new(), Some("  ")).is_err());
        let mut v = BTreeMap::new();
        v.insert("issue".to_string(), "#12".to_string());
        v.insert("repo".to_string(), "api".to_string());
        let steps = r.fill(&v, Some("notes")).unwrap();
        assert_eq!(
            steps[0],
            Step::Prompt("read issue #12 in api and plan".into())
        );
        assert_eq!(steps[2], Step::Checks);
        assert_eq!(steps[3], Step::Review);
        assert!(matches!(&steps[4], Step::Prompt(p) if p.contains("pull request")));
        assert_eq!(steps[5], Step::Prompt("notes again #12".into()));
    }

    #[test]
    fn seeds_loads_and_installs() {
        let tmp = tempfile::tempdir().unwrap();
        let mine = tmp.path().join("recipes");
        let repo = tmp.path().join("repo");
        std::fs::create_dir_all(repo.join(SHARED_DIR)).unwrap();
        std::fs::write(
            repo.join(SHARED_DIR).join("deploy.toml"),
            "name = \"deploy check\"\nsteps = [\"check the deploy\"]\n",
        )
        .unwrap();
        std::fs::write(repo.join(SHARED_DIR).join("broken.toml"), "steps = 3").unwrap();
        let all = load(&mine, Some(&repo));
        let names: Vec<&str> = all.iter().map(|r| r.name.as_str()).collect();
        assert_eq!(names, vec!["fix ticket", "deploy check"]);
        assert_eq!(all[0].steps.len(), 5);
        let p = install(&mine, "name = \"Write Docs\"\nsteps = [\"write docs\"]\n").unwrap();
        assert!(p.ends_with("write-docs.toml"));
        assert!(install(&mine, "nothing = 1").is_err());
        assert_eq!(load(&mine, None).len(), 2);
    }
}
