use crate::project;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

pub const KEY: &str = "profile";

pub type Profiles = BTreeMap<String, PathBuf>;

pub fn from_config(raw: &BTreeMap<String, String>) -> Profiles {
    raw.iter()
        .map(|(name, dir)| (name.clone(), crate::model::untilde(dir)))
        .collect()
}

pub fn load() -> Profiles {
    crate::config::load(&crate::paths::config())
        .map(|c| from_config(&c.profiles))
        .unwrap_or_default()
}

pub fn default_dir() -> Option<PathBuf> {
    dirs::home_dir().map(|h| h.join(".claude"))
}

pub fn of_project(dir: &Path) -> Option<String> {
    project::section::<String>(&[dir], KEY).map(|(name, _)| name)
}

pub fn config_dir(profiles: &Profiles, name: &str) -> Option<PathBuf> {
    let dir = profiles.get(name)?;
    (Some(dir) != default_dir().as_ref()).then(|| dir.clone())
}

pub fn claude_dirs(profiles: &Profiles) -> Vec<PathBuf> {
    let mut dirs: Vec<PathBuf> = default_dir().into_iter().collect();
    for d in profiles.values() {
        if !dirs.contains(d) {
            dirs.push(d.clone());
        }
    }
    dirs
}

pub fn project_roots(profiles: &Profiles) -> Vec<PathBuf> {
    claude_dirs(profiles)
        .into_iter()
        .map(|d| d.join("projects"))
        .collect()
}

pub const DEFAULT: &str = "default";

#[derive(Clone, Debug, PartialEq)]
pub struct Account {
    pub name: String,
    pub dir: PathBuf,
    pub email: Option<String>,
}

impl Account {
    pub fn label(&self) -> String {
        match &self.email {
            Some(e) => e.clone(),
            None if self.name == DEFAULT => "default account".into(),
            None => format!("{} (not logged in)", self.name),
        }
    }
}

fn email_in(json: &Path) -> Option<String> {
    let v: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(json).ok()?).ok()?;
    v.get("oauthAccount")?
        .get("emailAddress")?
        .as_str()
        .filter(|e| !e.is_empty())
        .map(str::to_string)
}

pub fn accounts(profiles: &Profiles, home: &Path) -> Vec<Account> {
    let default = home.join(".claude");
    let mut out = vec![Account {
        name: DEFAULT.into(),
        dir: default.clone(),
        email: email_in(&home.join(".claude.json")),
    }];
    for (name, dir) in profiles {
        if *dir == default || name == DEFAULT {
            continue;
        }
        out.push(Account {
            name: name.clone(),
            dir: dir.clone(),
            email: email_in(&dir.join(".claude.json")),
        });
    }
    out
}

pub fn switch_arg(prompt: &str) -> Option<&str> {
    let t = prompt.trim();
    let rest = t
        .strip_prefix("/pebl:switch")
        .or_else(|| t.strip_prefix("/switch"))?;
    (rest.is_empty() || rest.starts_with(char::is_whitespace)).then(|| rest.trim())
}

pub fn current_of(list: &[Account], config_dir: Option<&Path>) -> String {
    config_dir
        .and_then(|d| list.iter().find(|a| a.dir == d))
        .map(|a| a.name.clone())
        .unwrap_or_else(|| DEFAULT.into())
}

pub fn switch_target(list: &[Account], current: &str, arg: &str) -> Result<Account, String> {
    if list.len() < 2 {
        return Err(
            "You have one Claude account. Add another in a terminal with: pebl login".into(),
        );
    }
    let found = if arg.is_empty() {
        if list.len() > 2 {
            let rows: Vec<String> = list
                .iter()
                .enumerate()
                .map(|(i, a)| {
                    let here = if a.name == current {
                        "  (this session)"
                    } else {
                        ""
                    };
                    format!("  {}  {}  {}{here}", i + 1, a.name, a.label())
                })
                .collect();
            return Err(format!(
                "Which account? Type /switch and a name or number:\n{}",
                rows.join("\n")
            ));
        }
        list.iter().find(|a| a.name != current)
    } else {
        let n = arg.parse::<usize>().ok().and_then(|n| n.checked_sub(1));
        list.iter().enumerate().find_map(|(i, a)| {
            (Some(i) == n
                || a.name.eq_ignore_ascii_case(arg)
                || a.email
                    .as_deref()
                    .is_some_and(|e| e.eq_ignore_ascii_case(arg)))
            .then_some(a)
        })
    };
    match found {
        None => Err(format!(
            "No account called {arg}. Type /switch to see them."
        )),
        Some(a) if a.name == current => Err(format!("This session already uses {}.", a.label())),
        Some(a) if a.email.is_none() && a.name != DEFAULT => Err(format!(
            "{} isn't logged in yet. Log in first in a terminal: pebl profile",
            a.name
        )),
        Some(a) => Ok(a.clone()),
    }
}

pub fn count(profiles: &Profiles, home: &Path) -> usize {
    let default = home.join(".claude");
    1 + profiles
        .iter()
        .filter(|(name, dir)| **dir != default && name.as_str() != DEFAULT)
        .count()
}

pub fn next_free(profiles: &Profiles, home: &Path) -> (String, PathBuf) {
    let mut n = 2;
    loop {
        let name = format!("account{n}");
        let dir = home.join(format!(".claude-{n}"));
        if !profiles.contains_key(&name) && !dir.exists() && !profiles.values().any(|d| *d == dir) {
            return (name, dir);
        }
        n += 1;
    }
}

pub fn holding(profiles: &Profiles, path: &Path) -> Option<String> {
    profiles
        .iter()
        .find(|(_, dir)| path.starts_with(dir))
        .map(|(name, _)| name.clone())
}

pub fn set_for_project(dir: &Path, name: Option<&str>) -> anyhow::Result<PathBuf> {
    let repo = dir.join(project::FILE);
    let file = if read_table(&repo).is_some_and(|t| t.contains_key(KEY)) {
        repo
    } else {
        project::private_file(dir)
    };
    let mut table = read_table(&file).unwrap_or_default();
    match name {
        Some(n) => {
            table.insert(KEY.into(), toml::Value::String(n.into()));
        }
        None => {
            table.remove(KEY);
        }
    }
    write_table(&file, table)?;
    Ok(file)
}

pub fn set_current(config: &Path, name: &str) -> anyhow::Result<()> {
    let mut table = read_table(config).unwrap_or_default();
    if name == DEFAULT {
        table.remove(KEY);
    } else {
        table.insert(KEY.into(), toml::Value::String(name.into()));
    }
    write_table(config, table)
}

pub fn read_table(file: &Path) -> Option<toml::Table> {
    toml::from_str(&std::fs::read_to_string(file).ok()?).ok()
}

pub fn add_to_config(config: &Path, name: &str, dir: &str) -> anyhow::Result<()> {
    let mut table = read_table(config).unwrap_or_default();
    let profiles = table
        .entry("profiles")
        .or_insert_with(|| toml::Value::Table(toml::Table::new()));
    if let Some(t) = profiles.as_table_mut() {
        t.insert(name.into(), toml::Value::String(dir.into()));
    }
    write_table(config, table)
}

pub fn remove(config: &Path, name: &str) -> anyhow::Result<String> {
    let mut table = read_table(config).unwrap_or_default();
    let dir = table
        .get_mut("profiles")
        .and_then(|p| p.as_table_mut())
        .and_then(|t| t.remove(name))
        .ok_or_else(|| anyhow::anyhow!("no profile called {name}"))?;
    if table.get(KEY).and_then(|v| v.as_str()) == Some(name) {
        table.remove(KEY);
    }
    write_table(config, table)?;
    Ok(dir.as_str().unwrap_or_default().to_string())
}

pub fn write_table(file: &Path, table: toml::Table) -> anyhow::Result<()> {
    if let Some(parent) = file.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let (plain, tables): (toml::Table, toml::Table) =
        table.into_iter().partition(|(_, v)| !v.is_table());
    let mut text = toml::to_string(&plain)?;
    if !tables.is_empty() {
        if !text.is_empty() {
            text.push('\n');
        }
        text.push_str(&toml::to_string(&tables)?);
    }
    std::fs::write(file, text)?;
    Ok(())
}

pub fn valid_name(name: &str) -> bool {
    !name.is_empty()
        && name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
}

pub fn add(name: &str, dir: Option<&str>, login: bool) -> anyhow::Result<()> {
    if !valid_name(name) {
        anyhow::bail!("a profile name uses letters, digits, - and _ only");
    }
    let dir = dir
        .map(str::to_string)
        .unwrap_or_else(|| format!("~/.claude-{name}"));
    let path = crate::model::untilde(&dir);
    std::fs::create_dir_all(&path)?;
    add_to_config(&crate::paths::config(), name, &dir)?;
    println!("Added profile {name}: Claude Code keeps this account in {dir}");
    if login && Some(&path) != default_dir().as_ref() {
        println!("Your browser opens to sign in with this account.\n");
        let status = std::process::Command::new("claude")
            .args(["auth", "login"])
            .env("CLAUDE_CONFIG_DIR", &path)
            .status();
        if !status.is_ok_and(|s| s.success()) {
            println!("\nTo sign in later: run pebl profile, type its number, then 1.");
        }
    }
    println!("\nRun `pebl reload` to use it. `pebl profile` picks the account new sessions use.");
    println!("To always use it in one project:  pebl profile use {name} ~/code/your-project");
    Ok(())
}

pub fn use_for(name: Option<&str>, dir: &Path) -> anyhow::Result<()> {
    let dir = std::fs::canonicalize(dir)?;
    let profiles = load();
    if let Some(n) = name {
        if !profiles.contains_key(n) {
            anyhow::bail!("no profile called {n}; add it first with `pebl profile add {n}`");
        }
    }
    let file = set_for_project(&dir, name)?;
    match name {
        Some(n) => println!(
            "{} now uses the {n} account (saved in {}).",
            crate::model::tilde(&dir),
            crate::model::tilde(&file)
        ),
        None => println!(
            "{} now uses your default Claude Code account.",
            crate::model::tilde(&dir)
        ),
    }
    println!("New sessions pick it up; running ones keep the account they started with.");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_default_profile_does_not_set_a_config_dir() {
        let home = dirs::home_dir().unwrap();
        let p = from_config(&BTreeMap::from([
            ("personal".into(), "~/.claude".into()),
            ("work".into(), "~/.claude-work".into()),
        ]));
        assert_eq!(config_dir(&p, "personal"), None);
        assert_eq!(config_dir(&p, "work"), Some(home.join(".claude-work")));
        assert_eq!(config_dir(&p, "missing"), None);
        assert_eq!(
            project_roots(&p),
            vec![
                home.join(".claude/projects"),
                home.join(".claude-work/projects")
            ]
        );
    }

    #[test]
    fn a_project_profile_round_trips_and_keeps_other_settings() {
        let tmp = tempfile::tempdir().unwrap();
        let repo = tmp.path().join("repo");
        std::fs::create_dir_all(&repo).unwrap();
        std::fs::write(
            repo.join(project::FILE),
            "profile = \"personal\"\n\n[checks]\ntest = \"cargo test\"\n",
        )
        .unwrap();
        assert_eq!(of_project(&repo).as_deref(), Some("personal"));
        let file = set_for_project(&repo, Some("work")).unwrap();
        assert_eq!(file, repo.join(project::FILE));
        assert_eq!(of_project(&repo).as_deref(), Some("work"));
        let text = std::fs::read_to_string(&file).unwrap();
        assert!(text.contains("test = \"cargo test\""), "{text}");
        set_for_project(&repo, None).unwrap();
        assert_eq!(of_project(&repo), None);
    }

    fn acc(name: &str, email: Option<&str>) -> Account {
        Account {
            name: name.into(),
            dir: PathBuf::from(format!("/h/{name}")),
            email: email.map(Into::into),
        }
    }

    #[test]
    fn switch_reads_the_command_and_its_argument() {
        assert_eq!(switch_arg("/switch"), Some(""));
        assert_eq!(switch_arg("  /switch  work "), Some("work"));
        assert_eq!(switch_arg("/pebl:switch 2"), Some("2"));
        assert_eq!(switch_arg("/switcheroo"), None);
        assert_eq!(switch_arg("please /switch"), None);
    }

    #[test]
    fn switch_picks_the_other_account_or_asks_which() {
        let two = vec![
            acc(DEFAULT, Some("me@home.test")),
            acc("work", Some("me@work.test")),
        ];
        assert_eq!(current_of(&two, Some(Path::new("/h/work"))), "work");
        assert_eq!(current_of(&two, None), DEFAULT);
        assert_eq!(switch_target(&two, DEFAULT, "").unwrap().name, "work");
        assert_eq!(switch_target(&two, "work", "").unwrap().name, DEFAULT);
        assert!(switch_target(&two, "work", "work")
            .unwrap_err()
            .contains("already"));
        assert!(switch_target(&two[..1], DEFAULT, "")
            .unwrap_err()
            .contains("pebl login"));

        let mut three = two.clone();
        three.push(acc("side", None));
        let ask = switch_target(&three, DEFAULT, "").unwrap_err();
        assert!(ask.contains("2  work  me@work.test"), "{ask}");
        assert!(ask.contains("(this session)"), "{ask}");
        assert_eq!(switch_target(&three, DEFAULT, "2").unwrap().name, "work");
        assert_eq!(
            switch_target(&three, DEFAULT, "ME@WORK.TEST").unwrap().name,
            "work"
        );
        assert!(switch_target(&three, DEFAULT, "side")
            .unwrap_err()
            .contains("logged in"));
        assert!(switch_target(&three, DEFAULT, "nope")
            .unwrap_err()
            .contains("No account"));
    }

    #[test]
    fn adding_a_profile_keeps_the_rest_of_the_config() {
        let tmp = tempfile::tempdir().unwrap();
        let cfg = tmp.path().join("config.toml");
        std::fs::write(&cfg, "editor = \"Zed\"\n\n[notify]\nchime = false\n").unwrap();
        add_to_config(&cfg, "work", "~/.claude-work").unwrap();
        let c = crate::config::load(&cfg).unwrap();
        assert_eq!(c.editor.as_deref(), Some("Zed"));
        assert!(!c.notify.chime);
        assert_eq!(
            c.profiles.get("work").map(String::as_str),
            Some("~/.claude-work")
        );
    }
}
