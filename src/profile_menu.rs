use crate::model::{tilde, untilde};
use crate::profile::{self, Account, DEFAULT};
use crate::ui::theme::{ColorMode, Tok};
use anyhow::Result;
use std::io::{BufRead, IsTerminal, Write};
use std::path::{Path, PathBuf};

pub struct Ctx<'a> {
    pub config: PathBuf,
    pub home: PathBuf,
    pub here: PathBuf,
    pub projects: Vec<PathBuf>,
    pub input: &'a mut dyn BufRead,
    pub login: &'a dyn Fn(&Path, Option<&str>),
    pub start: bool,
}

fn paint(tok: Tok, text: &str, bold: bool) -> String {
    if std::env::var_os("NO_COLOR").is_some() || !std::io::stdout().is_terminal() {
        return text.to_string();
    }
    let mode = if crate::truecolor() {
        ColorMode::Truecolor
    } else {
        ColorMode::Indexed
    };
    tok.ansi(text, bold, mode)
}

fn fg(t: &str) -> String {
    paint(Tok::Fg, t, false)
}

fn strong(t: &str) -> String {
    paint(Tok::Fg, t, true)
}

fn acc(t: &str) -> String {
    paint(Tok::Acc, t, false)
}

fn mute(t: &str) -> String {
    paint(Tok::Mute, t, false)
}

fn dim(t: &str) -> String {
    paint(Tok::Dim, t, false)
}

fn done(t: &str) {
    println!("  {} {}", paint(Tok::Ok, "✓", false), fg(t));
}

fn note(t: &str) {
    println!("  {}", mute(t));
}

fn warn(t: &str) {
    println!("  {}", paint(Tok::Err, t, false));
}

fn pad(t: &str, w: usize) -> String {
    format!("{t}{}", " ".repeat(w.saturating_sub(t.chars().count())))
}

impl Ctx<'_> {
    fn ask(&mut self, q: &str) -> String {
        match q {
            "" => print!("  {} ", acc("❯")),
            _ => print!("  {} {} ", acc("❯"), fg(q)),
        }
        let _ = std::io::stdout().flush();
        let mut line = String::new();
        match self.input.read_line(&mut line) {
            Ok(0) | Err(_) => "q".into(),
            Ok(_) => line.trim().to_string(),
        }
    }

    fn profiles(&self) -> profile::Profiles {
        crate::config::load(&self.config)
            .map(|c| profile::from_config(&c.profiles))
            .unwrap_or_default()
    }

    fn accounts(&self) -> Vec<Account> {
        profile::accounts(&self.profiles(), &self.home)
    }

    fn current(&self) -> String {
        let profiles = self.profiles();
        crate::config::load(&self.config)
            .ok()
            .and_then(|c| c.profile)
            .filter(|n| profiles.contains_key(n))
            .unwrap_or_else(|| DEFAULT.into())
    }

    fn pick(&mut self, q: &str, allow_default: bool) -> Option<Account> {
        let list = self.accounts();
        let answer = self.ask(q);
        let n: usize = answer.parse().ok()?;
        let a = list.get(n.checked_sub(1)?)?.clone();
        (allow_default || a.name != DEFAULT).then_some(a)
    }
}

fn title(t: &str) {
    println!("\n  {}  {}\n", paint(Tok::Acc, "pebl", true), strong(t));
}

fn print_accounts(ctx: &Ctx) {
    let list = ctx.accounts();
    let current = ctx.current();
    let name_w = list.iter().map(|a| a.name.len()).max().unwrap_or(0);
    let label_w = list
        .iter()
        .map(|a| account_email(a).chars().count())
        .max()
        .unwrap_or(0);
    title("Claude accounts");
    for (i, a) in list.iter().enumerate() {
        let on = a.name == current;
        let email = pad(&account_email(a), label_w);
        println!(
            "  {} {}  {}  {}  {}{}",
            if on { acc("●") } else { " ".into() },
            if on {
                acc(&(i + 1).to_string())
            } else {
                mute(&(i + 1).to_string())
            },
            strong(&pad(&a.name, name_w)),
            if a.email.is_some() {
                mute(&email)
            } else {
                paint(Tok::Err, &email, false)
            },
            dim(&tilde(&a.dir)),
            if on {
                format!("  {}", acc("new sessions"))
            } else {
                String::new()
            }
        );
    }
    for (i, a) in list.iter().enumerate() {
        if let Some(b) = list[i + 1..]
            .iter()
            .find(|b| b.email.is_some() && b.email == a.email)
        {
            println!(
                "\n  {}",
                paint(
                    Tok::Err,
                    &format!(
                        "{} and {} are both logged in as {}. To fix it type {}, then 1, and log {} in to the other account.",
                        a.name,
                        b.name,
                        account_email(a),
                        i + 1,
                        a.name
                    ),
                    false
                )
            );
        }
    }
    let profiles = ctx.profiles();
    let own: Vec<(String, String)> = ctx
        .projects
        .iter()
        .filter_map(|p| Some((tilde(p), profile::of_project(p)?)))
        .filter(|(_, n)| n == DEFAULT || profiles.contains_key(n))
        .collect();
    if !own.is_empty() {
        let w = own
            .iter()
            .map(|(p, _)| p.chars().count())
            .max()
            .unwrap_or(0);
        println!("\n  {}", mute("Projects with their own account"));
        for (p, n) in own {
            println!("    {}  {}", fg(&pad(&p, w)), acc(&n));
        }
    }
    let here =
        match profile::of_project(&ctx.here).filter(|n| n == DEFAULT || profiles.contains_key(n)) {
            Some(n) => format!("{} {}", acc(&n), dim("(its own)")),
            None => acc(&current),
        };
    println!(
        "\n  {} {} {} {here}",
        mute("This folder"),
        fg(&tilde(&ctx.here)),
        mute("uses")
    );
}

pub fn list(ctx: &mut Ctx) -> Result<bool> {
    print_accounts(ctx);
    println!(
        "\n  {} {} {} {}\n",
        dim("Run"),
        acc("pebl profile"),
        dim("in a terminal to change these, or"),
        acc("pebl login")
    );
    Ok(false)
}

fn account_email(a: &Account) -> String {
    a.email.clone().unwrap_or_else(|| "not logged in".into())
}

pub fn add(ctx: &mut Ctx) -> Result<bool> {
    let profiles = ctx.profiles();
    let suggestion = ["work", "personal"]
        .into_iter()
        .find(|n| !profiles.contains_key(*n) && !ctx.home.join(format!(".claude-{n}")).exists())
        .map(str::to_string)
        .unwrap_or_else(|| profile::next_free(&profiles, &ctx.home).0);
    println!();
    let name = ctx.ask(&format!(
        "Name for the new account {}",
        dim(&format!("like work or personal [{suggestion}]"))
    ));
    if name == "q" {
        return Ok(false);
    }
    let name = if name.is_empty() { suggestion } else { name };
    if !profile::valid_name(&name) || name == DEFAULT {
        warn("A name uses letters, digits, - and _ only, and can't be \"default\".");
        return Ok(false);
    }
    if profiles.contains_key(&name) {
        warn(&format!("There is already an account called {name}."));
        return Ok(false);
    }
    let dir = format!("~/.claude-{name}");
    let path = untilde_in(&ctx.home, &dir);
    std::fs::create_dir_all(&path)?;
    profile::add_to_config(&ctx.config, &name, &dir)?;
    done(&format!("Added {name}. Claude Code keeps it in {dir}."));
    log_in(ctx, &name, &path);
    let yes = ctx.ask(&format!(
        "Use {name} for new sessions from now on? {}",
        dim("[y/N]")
    ));
    if yes.eq_ignore_ascii_case("y") || yes.eq_ignore_ascii_case("yes") {
        profile::set_current(&ctx.config, &name)?;
        done(&format!("New sessions use {name}."));
    }
    Ok(true)
}

fn untilde_in(home: &Path, dir: &str) -> PathBuf {
    match dir.strip_prefix("~/") {
        Some(rest) => home.join(rest),
        None => untilde(dir),
    }
}

fn log_in(ctx: &mut Ctx, name: &str, dir: &Path) {
    let email = ctx.ask(&format!(
        "Email to sign in to {name} with {}",
        dim("(optional, fills in the sign-in page)")
    ));
    let email = email.contains('@').then_some(email);
    note("Your browser opens to sign in. Pick the right account there, then come back here.");
    println!();
    (ctx.login)(dir, email.as_deref());
    println!();
    match ctx.accounts().into_iter().find(|a| a.name == name) {
        Some(Account { email: Some(e), .. }) => done(&format!("{name} is logged in as {e}.")),
        _ => warn(&format!(
            "{name} isn't logged in yet. Run pebl profile, type its number, then 1 to try again."
        )),
    }
}

fn keys(rows: &[&[(&str, &str)]]) {
    println!();
    for row in rows {
        let last = row.len() - 1;
        let line: Vec<String> = row
            .iter()
            .enumerate()
            .map(|(i, (k, label))| {
                let label = if i == last {
                    label.to_string()
                } else {
                    pad(label, 26)
                };
                format!("{} {}", acc(k), mute(&label))
            })
            .collect();
        println!("  {}", line.join(" "));
    }
    println!();
}

pub fn menu(ctx: &mut Ctx) -> Result<bool> {
    let mut changed = false;
    loop {
        print_accounts(ctx);
        keys(&[
            &[
                ("a", "add an account"),
                ("d", "default for new sessions"),
                ("p", "give a project an account"),
            ],
            &[
                ("i", "log in again"),
                ("r", "remove an account"),
                ("s", "start pebl"),
            ],
            &[("q", "done")],
        ]);
        let choice = ctx.ask("");
        match choice.as_str() {
            "a" => changed |= add(ctx)?,
            "d" => {
                if let Some(a) = ctx.pick("Number of the account new sessions use:", true) {
                    profile::set_current(&ctx.config, &a.name)?;
                    done(&format!("New sessions use {}.", a.label()));
                    changed = true;
                }
            }
            "p" => changed |= project(ctx)?,
            "i" | "l" => {
                if let Some(a) = ctx.pick("Number of the account to log in to:", true) {
                    let dir = a.dir.clone();
                    log_in(ctx, &a.name, &dir);
                }
            }
            "r" => {
                if let Some(a) = ctx.pick("Number of the account to remove:", false) {
                    profile::remove(&ctx.config, &a.name)?;
                    done(&format!("Removed {}.", a.name));
                    note(&format!(
                        "Its login and history stay in {}; delete that folder to get rid of them.",
                        tilde(&a.dir)
                    ));
                    changed = true;
                }
            }
            "s" => {
                ctx.start = true;
                return Ok(changed);
            }
            "q" | "" => return Ok(changed),
            n => match n
                .parse::<usize>()
                .ok()
                .and_then(|i| ctx.accounts().get(i.checked_sub(1)?).cloned())
            {
                Some(a) => changed |= one_account(ctx, &a)?,
                None => warn("Type one of the letters above, or an account's number."),
            },
        }
    }
}

fn project_account(ctx: &Ctx, dir: &Path) -> String {
    let profiles = ctx.profiles();
    match profile::of_project(dir).filter(|n| n == DEFAULT || profiles.contains_key(n)) {
        Some(n) => acc(&n),
        None => dim(&format!("follows the default ({})", ctx.current())),
    }
}

fn pick_folder(ctx: &mut Ctx) -> Option<PathBuf> {
    let mut known: Vec<PathBuf> = ctx.projects.clone();
    known.sort_by_key(|p| tilde(p).to_lowercase());
    known.dedup();
    let here = std::fs::canonicalize(&ctx.here).unwrap_or(ctx.here.clone());
    let default = known
        .iter()
        .position(|p| *p == here)
        .map(|i| (i + 1).to_string());
    if !known.is_empty() {
        println!("\n  {}", mute("Which project?"));
        let w = known
            .iter()
            .map(|p| tilde(p).chars().count())
            .max()
            .unwrap_or(0);
        for (i, p) in known.iter().enumerate() {
            println!(
                "    {}  {}  {}",
                acc(&format!("{:>2}", i + 1)),
                fg(&pad(&tilde(p), w)),
                project_account(ctx, p)
            );
        }
    }
    let hint = match (&default, known.is_empty()) {
        (Some(n), _) => format!("Number or folder [{n}]"),
        (None, true) => format!("Project folder [{}]", tilde(&ctx.here)),
        (None, false) => "Number or folder".into(),
    };
    let answer = ctx.ask(&hint);
    if answer == "q" {
        return None;
    }
    let answer = match (answer.is_empty(), &default) {
        (true, Some(n)) => n.clone(),
        _ => answer,
    };
    let folder = if answer.is_empty() {
        if !known.is_empty() {
            return None;
        }
        ctx.here.clone()
    } else if let Some(p) = answer
        .parse::<usize>()
        .ok()
        .and_then(|n| known.get(n.checked_sub(1)?))
    {
        p.clone()
    } else {
        untilde_in(&ctx.home, &answer)
    };
    match std::fs::canonicalize(&folder) {
        Ok(f) => Some(f),
        Err(_) => {
            warn(&format!("There is no folder {}.", folder.display()));
            None
        }
    }
}

#[derive(Clone, Copy, PartialEq)]
enum AccountAct {
    LogIn,
    Default,
    Remove,
}

fn one_account(ctx: &mut Ctx, a: &Account) -> Result<bool> {
    println!("\n  {}  {}", strong(&a.name), mute(&account_email(a)));
    let mut options = vec![(AccountAct::LogIn, "l", "log in again (or to another login)")];
    if a.name != ctx.current() {
        options.push((AccountAct::Default, "d", "use it for new sessions"));
    }
    if a.name != DEFAULT {
        options.push((AccountAct::Remove, "r", "remove it"));
    }
    for (i, (_, _, label)) in options.iter().enumerate() {
        println!("    {} {}", acc(&(i + 1).to_string()), mute(label));
    }
    let answer = ctx.ask(&dim("number, or enter to go back"));
    let picked = answer
        .parse::<usize>()
        .ok()
        .and_then(|n| options.get(n.checked_sub(1)?))
        .or_else(|| options.iter().find(|(_, k, _)| *k == answer))
        .map(|(act, _, _)| *act);
    match picked {
        None => Ok(false),
        Some(AccountAct::LogIn) => {
            log_in(ctx, &a.name, &a.dir);
            Ok(false)
        }
        Some(AccountAct::Default) => {
            profile::set_current(&ctx.config, &a.name)?;
            done(&format!("New sessions use {}.", a.label()));
            Ok(true)
        }
        Some(AccountAct::Remove) => {
            profile::remove(&ctx.config, &a.name)?;
            done(&format!("Removed {}.", a.name));
            note(&format!(
                "Its login and history stay in {}; delete that folder to get rid of them.",
                tilde(&a.dir)
            ));
            Ok(true)
        }
    }
}

fn project(ctx: &mut Ctx) -> Result<bool> {
    let Some(folder) = pick_folder(ctx) else {
        return Ok(false);
    };
    let list = ctx.accounts();
    let name_w = list.iter().map(|a| a.name.len()).max().unwrap_or(0);
    println!(
        "\n  {} {}",
        mute("Which account should"),
        fg(&tilde(&folder))
    );
    println!(
        "    {}  {}",
        acc(" 0"),
        dim(&format!("follow the default ({})", ctx.current()))
    );
    for (i, a) in list.iter().enumerate() {
        println!(
            "    {}  {}  {}",
            acc(&format!("{:>2}", i + 1)),
            strong(&pad(&a.name, name_w)),
            mute(&account_email(a))
        );
    }
    let answer = ctx.ask("Number");
    let name = match answer.as_str() {
        "0" => None,
        _ => {
            let Some(a) = answer
                .parse::<usize>()
                .ok()
                .and_then(|n| list.get(n.checked_sub(1)?))
            else {
                return Ok(false);
            };
            Some(a.name.clone())
        }
    };
    profile::set_for_project(&folder, name.as_deref())?;
    match name {
        Some(n) => done(&format!("{} now always uses {n}.", tilde(&folder))),
        None => done(&format!(
            "{} now follows the default account.",
            tilde(&folder)
        )),
    }
    Ok(true)
}

pub fn onboarding(ctx: &mut Ctx) -> Result<()> {
    let me = ctx
        .accounts()
        .into_iter()
        .next()
        .and_then(|a| a.email)
        .map(|e| format!("You're logged in to Claude as {e}."))
        .unwrap_or_default();
    title("Welcome");
    if !me.is_empty() {
        note(&me);
    }
    let yes = ctx.ask(&format!(
        "Do you use more than one Claude account, like work and personal? {}",
        dim("[y/N]")
    ));
    if yes.eq_ignore_ascii_case("y") || yes.eq_ignore_ascii_case("yes") {
        note("Your current login stays as the default account. Let's add the other one.");
        add(ctx)?;
        println!();
        note("You can change this any time with pebl profile. Starting pebl…");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(home: &Path, here: &Path, input: &str, f: fn(&mut Ctx) -> Result<bool>) -> bool {
        run_in(home, here, vec![], input, f)
    }

    fn run_in(
        home: &Path,
        here: &Path,
        projects: Vec<PathBuf>,
        input: &str,
        f: fn(&mut Ctx) -> Result<bool>,
    ) -> bool {
        let mut input = std::io::Cursor::new(input.as_bytes().to_vec());
        let login = |dir: &Path, _: Option<&str>| {
            std::fs::write(
                dir.join(".claude.json"),
                "{\"oauthAccount\":{\"emailAddress\":\"me@work.test\"}}",
            )
            .unwrap();
        };
        let mut ctx = Ctx {
            config: home.join("config.toml"),
            home: home.to_path_buf(),
            here: here.to_path_buf(),
            projects,
            input: &mut input,
            login: &login,
            start: false,
        };
        f(&mut ctx).unwrap()
    }

    #[test]
    fn adding_an_account_logs_in_and_can_make_it_the_default() {
        let tmp = tempfile::tempdir().unwrap();
        let home = tmp.path();
        assert!(run(home, home, "\n\ny\n", add));
        let cfg = crate::config::load(&home.join("config.toml")).unwrap();
        assert_eq!(
            cfg.profiles.get("work").map(String::as_str),
            Some("~/.claude-work")
        );
        assert_eq!(cfg.profile.as_deref(), Some("work"));
        assert!(home.join(".claude-work/.claude.json").exists());

        assert!(run(home, home, "personal\n\nn\n", add));
        let cfg = crate::config::load(&home.join("config.toml")).unwrap();
        assert!(cfg.profiles.contains_key("personal"));
        assert_eq!(cfg.profile.as_deref(), Some("work"));
    }

    #[test]
    fn the_menu_picks_the_default_sets_a_project_and_removes_an_account() {
        let tmp = tempfile::tempdir().unwrap();
        let home = tmp.path();
        let repo = home.join("repo");
        std::fs::create_dir_all(&repo).unwrap();
        std::fs::write(repo.join(crate::project::FILE), "profile = \"default\"\n").unwrap();
        run(home, home, "work\n\ny\n", add);
        assert!(run(home, &repo, "d\n1\np\n\n2\nq\n", menu));
        let cfg = crate::config::load(&home.join("config.toml")).unwrap();
        assert_eq!(cfg.profile, None);
        assert_eq!(profile::of_project(&repo).as_deref(), Some("work"));

        assert!(run(home, &repo, "r\n1\nr\n2\n", menu));
        let cfg = crate::config::load(&home.join("config.toml")).unwrap();
        assert!(cfg.profiles.is_empty());
        assert!(home.join(".claude-work").is_dir());
    }

    #[test]
    fn a_project_is_picked_by_number_from_the_ones_pebl_knows() {
        let tmp = tempfile::tempdir().unwrap();
        let home = tmp.path().canonicalize().unwrap();
        let repos: Vec<PathBuf> = ["a-api", "b-web"]
            .iter()
            .map(|n| {
                let r = home.join(n);
                std::fs::create_dir_all(&r).unwrap();
                std::fs::write(r.join(crate::project::FILE), "profile = \"default\"\n").unwrap();
                r
            })
            .collect();
        run(&home, &home, "work\n\nn\n", add);
        assert!(run_in(&home, &home, repos.clone(), "p\n2\n2\nq\n", menu));
        assert_eq!(profile::of_project(&repos[1]).as_deref(), Some("work"));
        assert_eq!(profile::of_project(&repos[0]).as_deref(), Some("default"));

        assert!(run_in(&home, &repos[0], repos.clone(), "p\n\n2\nq\n", menu));
        assert_eq!(profile::of_project(&repos[0]).as_deref(), Some("work"));

        assert!(!run_in(&home, &home, repos, "p\n\nq\n", menu));
    }

    #[test]
    fn s_leaves_the_menu_to_start_pebl() {
        let tmp = tempfile::tempdir().unwrap();
        let home = tmp.path();
        let mut input = std::io::Cursor::new(b"s\n".to_vec());
        let login = |_: &Path, _: Option<&str>| {};
        let mut ctx = Ctx {
            config: home.join("config.toml"),
            home: home.to_path_buf(),
            here: home.to_path_buf(),
            projects: vec![],
            input: &mut input,
            login: &login,
            start: false,
        };
        assert!(!menu(&mut ctx).unwrap());
        assert!(ctx.start);
    }

    #[test]
    fn an_account_number_opens_its_actions() {
        let tmp = tempfile::tempdir().unwrap();
        let home = tmp.path();
        run(home, home, "work\n\nn\n", add);
        assert!(run(home, home, "2\n2\nq\n", menu));
        let cfg = crate::config::load(&home.join("config.toml")).unwrap();
        assert_eq!(cfg.profile.as_deref(), Some("work"));
        assert!(!run(home, home, "1\n3\nq\n", menu));
        assert!(run(home, home, "2\nr\nq\n", menu));
        let cfg = crate::config::load(&home.join("config.toml")).unwrap();
        assert!(cfg.profiles.is_empty());
    }

    #[test]
    fn bad_input_changes_nothing() {
        let tmp = tempfile::tempdir().unwrap();
        let home = tmp.path();
        assert!(!run(home, home, "default\n", add));
        assert!(!run(home, home, "x\nd\n9\nq\n", menu));
        assert!(!home.join("config.toml").exists());
    }
}
