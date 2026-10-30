use super::{Arrangement, LivePane, SessionBackend, ShellPane, SpawnSpec, UiBackend, UiPanes};
use anyhow::{bail, Context, Result};
use std::process::{Command, Stdio};

pub const UI_SESSION: &str = "pebl-ui";
pub const BG_SESSION: &str = "pebl-bg";

#[derive(Clone, Debug)]
pub struct Tmux {
    pub socket: String,
}

impl Tmux {
    pub fn new(socket: impl Into<String>) -> Self {
        Tmux {
            socket: socket.into(),
        }
    }

    pub fn command(&self) -> Command {
        let mut c = Command::new("tmux");
        c.arg("-L").arg(&self.socket);
        c.env_remove("TMUX");
        c
    }

    pub fn run<S: AsRef<str>>(&self, args: &[S]) -> Result<String> {
        let mut c = self.command();
        for a in args {
            c.arg(a.as_ref());
        }
        let out = c
            .stdin(Stdio::null())
            .output()
            .context("running tmux (is it installed?)")?;
        if !out.status.success() {
            bail!(
                "tmux {}: {}",
                args.first().map(|a| a.as_ref()).unwrap_or(""),
                String::from_utf8_lossy(&out.stderr).trim()
            );
        }
        Ok(String::from_utf8_lossy(&out.stdout)
            .trim_end_matches('\n')
            .to_string())
    }

    pub fn ok(&self, args: &[&str]) -> bool {
        self.run(args).is_ok()
    }

    pub fn has_session(&self, name: &str) -> bool {
        self.ok(&["has-session", "-t", &format!("={name}")])
    }

    pub fn kill_server(&self) -> Result<()> {
        self.run(&["kill-server"]).map(|_| ())
    }

    pub fn build_ui(&self, pebl: &str, size: (u16, u16), sidebar_w: u16) -> Result<UiPanes> {
        let (w, h) = size;
        let stage = self.run(&[
            "new-session",
            "-d",
            "-s",
            UI_SESSION,
            "-n",
            "pebl",
            "-x",
            &w.to_string(),
            "-y",
            &h.to_string(),
            "-P",
            "-F",
            "#{pane_id}",
            &view_cmd(pebl, "stage"),
        ])?;
        let bar = self.run(&[
            "split-window",
            "-d",
            "-v",
            "-f",
            "-l",
            "1",
            "-t",
            &stage,
            "-P",
            "-F",
            "#{pane_id}",
            &view_cmd(pebl, "bar"),
        ])?;
        let sidebar = self.run(&[
            "split-window",
            "-d",
            "-h",
            "-b",
            "-l",
            &sidebar_w.to_string(),
            "-t",
            &stage,
            "-P",
            "-F",
            "#{pane_id}",
            &view_cmd(pebl, "sidebar"),
        ])?;
        for (pane, role) in [(&sidebar, "sidebar"), (&stage, "stage"), (&bar, "bar")] {
            self.run(&["set", "-p", "-t", pane, "@pebl_role", role])?;
        }
        for (k, v) in [
            ("status", "on"),
            ("status-position", "top"),
            ("status-style", "bg=default"),
        ] {
            self.run(&["set", "-t", UI_SESSION, k, v])?;
        }
        self.run(&["select-pane", "-t", &sidebar])?;
        Ok(UiPanes {
            sidebar,
            stage,
            bar,
        })
    }

    fn pane_with_role(&self, role: &str) -> Result<Option<String>> {
        let out = self.run(&["list-panes", "-a", "-F", "#{pane_id}\t#{@pebl_role}"])?;
        Ok(out
            .lines()
            .filter_map(|l| l.split_once('\t'))
            .find(|(_, r)| *r == role)
            .map(|(p, _)| p.to_string()))
    }

    pub fn respawn_views(&self, pebl: &str) -> Result<()> {
        let out = self.run(&["list-panes", "-a", "-F", "#{pane_id}\t#{@pebl_role}"])?;
        for (pane, role) in out.lines().filter_map(|l| l.split_once('\t')) {
            if matches!(role, "sidebar" | "stage" | "bar" | "tabs") {
                self.run(&["respawn-pane", "-k", "-t", pane, &view_cmd(pebl, role)])?;
            }
        }
        Ok(())
    }

    pub fn client_ttys(&self) -> Vec<String> {
        self.run(&["list-clients", "-F", "#{client_tty}"])
            .map(|s| s.lines().map(str::to_string).collect())
            .unwrap_or_default()
    }
}

impl SessionBackend for Tmux {
    fn spawn(&self, spec: &SpawnSpec) -> Result<String> {
        let name = format!("pebl-s-{}", spec.sid);
        let _ = self.run(&["kill-session", "-t", &format!("={name}")]);
        let mut args: Vec<String> = vec![
            "new-session".into(),
            "-d".into(),
            "-s".into(),
            name,
            "-x".into(),
            spec.size.0.to_string(),
            "-y".into(),
            spec.size.1.to_string(),
            "-c".into(),
            spec.cwd.display().to_string(),
            "-P".into(),
            "-F".into(),
            "#{pane_id}".into(),
        ];
        for (k, v) in &spec.env {
            args.push("-e".into());
            args.push(format!("{k}={v}"));
        }
        args.push(shell_line(&spec.argv));
        let pane = self.run(&args)?;
        self.run(&["set", "-p", "-t", &pane, "@pebl_sid", &spec.sid])?;
        self.run(&["set", "-p", "-t", &pane, "remain-on-exit", "on"])?;
        Ok(pane)
    }

    fn respawn(&self, pane: &str, spec: &SpawnSpec) -> Result<()> {
        let mut args: Vec<String> = vec![
            "respawn-pane".into(),
            "-k".into(),
            "-t".into(),
            pane.into(),
            "-c".into(),
            spec.cwd.display().to_string(),
        ];
        for (k, v) in &spec.env {
            args.push("-e".into());
            args.push(format!("{k}={v}"));
        }
        args.push(shell_line(&spec.argv));
        self.run(&args).map(|_| ())
    }

    fn kill(&self, pane: &str) -> Result<()> {
        self.run(&["kill-pane", "-t", pane]).map(|_| ())
    }

    fn list(&self) -> Result<Vec<LivePane>> {
        let out = self.run(&[
            "list-panes",
            "-a",
            "-F",
            "#{pane_id}\t#{@pebl_sid}\t#{pane_dead}\t#{pane_dead_status}",
        ])?;
        Ok(out.lines().filter_map(parse_pane).collect())
    }

    fn capture(&self, pane: &str) -> Result<String> {
        self.run(&["capture-pane", "-p", "-J", "-t", pane, "-S", "-25"])
    }

    fn send_text(&self, pane: &str, text: &str, enter: bool) -> Result<()> {
        if !text.is_empty() {
            self.run(&["set-buffer", "-b", "pebl-reply", "--", text])?;
            self.run(&["paste-buffer", "-p", "-d", "-b", "pebl-reply", "-t", pane])?;
        }
        if enter {
            std::thread::sleep(std::time::Duration::from_millis(60));
            self.run(&["send-keys", "-t", pane, "Enter"])?;
        }
        Ok(())
    }

    fn spawn_shell(&self, sid: &str, cwd: &std::path::Path, size: (u16, u16)) -> Result<String> {
        let name = format!("pebl-sh-{sid}");
        let _ = self.run(&["kill-session", "-t", &format!("={name}")]);
        let pane = self.run(&[
            "new-session",
            "-d",
            "-s",
            &name,
            "-x",
            &size.0.to_string(),
            "-y",
            &size.1.to_string(),
            "-c",
            &cwd.display().to_string(),
            "-P",
            "-F",
            "#{pane_id}",
        ])?;
        self.run(&["set", "-p", "-t", &pane, "@pebl_shell", sid])?;
        Ok(pane)
    }

    fn shells(&self) -> Result<Vec<ShellPane>> {
        let out = self.run(&[
            "list-panes",
            "-a",
            "-F",
            "#{pane_id}\t#{@pebl_shell}\t#{pane_current_command}\t#{pane_pid}",
        ])?;
        Ok(out
            .lines()
            .filter_map(|l| {
                let mut f = l.split('\t');
                let pane = f.next()?.to_string();
                let sid = f.next().filter(|s| !s.is_empty())?.to_string();
                let command = f.next().unwrap_or("").to_string();
                let pid = f.next().and_then(|p| p.parse().ok()).unwrap_or(0);
                Some(ShellPane {
                    pane,
                    sid,
                    command,
                    pid,
                })
            })
            .collect())
    }

    fn child_command(&self, pid: u32) -> Option<String> {
        let kids = Command::new("pgrep")
            .args(["-P", &pid.to_string()])
            .output()
            .ok()?;
        let child = String::from_utf8_lossy(&kids.stdout)
            .lines()
            .next()?
            .trim()
            .to_string();
        let out = Command::new("ps")
            .args(["-o", "args=", "-p", &child])
            .output()
            .ok()?;
        let args = String::from_utf8_lossy(&out.stdout).trim().to_string();
        (!args.is_empty()).then_some(args)
    }

    fn send_key(&self, pane: &str, key: &str) -> Result<()> {
        self.run(&["send-keys", "-t", pane, key]).map(|_| ())
    }
}

impl UiBackend for Tmux {
    fn create_ui(&self, pebl: &str, size: (u16, u16), sidebar_w: u16) -> Result<UiPanes> {
        self.build_ui(pebl, size, sidebar_w)
    }

    fn ui_panes(&self) -> Result<Option<UiPanes>> {
        if !self.has_session(UI_SESSION) {
            return Ok(None);
        }
        let (Some(sidebar), Some(stage), Some(bar)) = (
            self.pane_with_role("sidebar")?,
            self.pane_with_role("stage")?,
            self.pane_with_role("bar")?,
        ) else {
            return Ok(None);
        };
        Ok(Some(UiPanes {
            sidebar,
            stage,
            bar,
        }))
    }

    fn repair_ui(
        &self,
        pebl: &str,
        main: Option<&str>,
        size: (u16, u16),
        sidebar_w: u16,
    ) -> Result<(UiPanes, String)> {
        if !self.has_session(UI_SESSION) {
            let ui = self.build_ui(pebl, size, sidebar_w)?;
            let stage = ui.stage.clone();
            return Ok((ui, stage));
        }
        let listing = self.run(&[
            "list-panes",
            "-t",
            &format!("={UI_SESSION}:"),
            "-F",
            "#{pane_id}\t#{@pebl_role}",
        ])?;
        let in_ui: Vec<(String, String)> = listing
            .lines()
            .filter_map(|l| l.split_once('\t'))
            .map(|(a, b)| (a.to_string(), b.to_string()))
            .collect();
        let mut main = main
            .filter(|m| in_ui.iter().any(|(p, _)| p == m))
            .map(str::to_string)
            .or_else(|| {
                in_ui
                    .iter()
                    .find(|(_, r)| r.is_empty() || r == "stage")
                    .map(|(p, _)| p.clone())
            });
        let tag = |pane: &str, role: &str| {
            self.run(&["set", "-p", "-t", pane, "@pebl_role", role])
                .map(|_| ())
        };
        let stage = match self.pane_with_role("stage")? {
            Some(s) => s,
            None => {
                let s = match (&main, in_ui.first()) {
                    (None, Some((any, _))) => self.run(&[
                        "split-window",
                        "-d",
                        "-h",
                        "-t",
                        any,
                        "-P",
                        "-F",
                        "#{pane_id}",
                        &view_cmd(pebl, "stage"),
                    ])?,
                    _ => self.run(&[
                        "new-window",
                        "-d",
                        "-t",
                        &format!("={BG_SESSION}:"),
                        "-P",
                        "-F",
                        "#{pane_id}",
                        &view_cmd(pebl, "stage"),
                    ])?,
                };
                tag(&s, "stage")?;
                if main.is_none() {
                    main = Some(s.clone());
                }
                s
            }
        };
        let main = main.unwrap_or_else(|| stage.clone());
        let sidebar = match self.pane_with_role("sidebar")? {
            Some(s) => s,
            None => {
                let s = self.run(&[
                    "split-window",
                    "-d",
                    "-h",
                    "-b",
                    "-l",
                    &sidebar_w.to_string(),
                    "-t",
                    &main,
                    "-P",
                    "-F",
                    "#{pane_id}",
                    &view_cmd(pebl, "sidebar"),
                ])?;
                tag(&s, "sidebar")?;
                s
            }
        };
        let bar = match self.pane_with_role("bar")? {
            Some(s) => s,
            None => {
                let s = self.run(&[
                    "split-window",
                    "-d",
                    "-v",
                    "-f",
                    "-l",
                    "1",
                    "-t",
                    &main,
                    "-P",
                    "-F",
                    "#{pane_id}",
                    &view_cmd(pebl, "bar"),
                ])?;
                tag(&s, "bar")?;
                s
            }
        };
        Ok((
            UiPanes {
                sidebar,
                stage,
                bar,
            },
            main,
        ))
    }

    fn slot(&self) -> Result<Option<String>> {
        let listing = self.run(&[
            "list-panes",
            "-t",
            &format!("={UI_SESSION}:"),
            "-F",
            "#{pane_id}\t#{@pebl_role}\t#{pane_left}",
        ])?;
        Ok(listing
            .lines()
            .filter_map(|l| {
                let mut f = l.split('\t');
                Some((f.next()?, f.next()?, f.next()?.parse::<u32>().ok()?))
            })
            .filter(|(_, r, _)| !matches!(*r, "sidebar" | "bar" | "tabs"))
            .min_by_key(|(_, _, x)| *x)
            .map(|(p, _, _)| p.to_string()))
    }

    fn window_size(&self) -> Result<(u16, u16)> {
        let out = self.run(&[
            "display-message",
            "-p",
            "-t",
            &format!("={UI_SESSION}:"),
            "#{window_width} #{window_height}",
        ])?;
        let mut it = out
            .split_whitespace()
            .map(|n| n.parse::<u16>().unwrap_or(0));
        Ok((it.next().unwrap_or(120), it.next().unwrap_or(40)))
    }

    fn show(&self, pane: &str, current: &str) -> Result<()> {
        if pane == current {
            return Ok(());
        }
        self.run(&["swap-pane", "-d", "-s", pane, "-t", current])
            .map(|_| ())
    }

    fn select(&self, pane: &str) -> Result<()> {
        self.run(&["select-pane", "-t", pane]).map(|_| ())
    }

    fn set_header(&self, lines: &[String]) -> Result<()> {
        for (i, l) in lines.iter().enumerate() {
            self.run(&["set", "-t", UI_SESSION, &format!("status-format[{i}]"), l])?;
        }
        let n = if lines.len() > 1 { "2" } else { "on" };
        self.run(&["set", "-t", UI_SESSION, "status", n])?;
        self.run(&["refresh-client", "-S"]).map(|_| ()).or(Ok(()))
    }

    fn arrange(&self, a: &Arrangement) -> Result<()> {
        self.do_arrange(a)
    }

    fn tabs_pane(&self, pebl: &str) -> Result<String> {
        if let Some(p) = self.pane_with_role("tabs")? {
            return Ok(p);
        }
        let p = self.run(&[
            "new-window",
            "-d",
            "-t",
            &format!("={BG_SESSION}:"),
            "-P",
            "-F",
            "#{pane_id}",
            &view_cmd(pebl, "tabs"),
        ])?;
        self.run(&["set", "-p", "-t", &p, "@pebl_role", "tabs"])?;
        Ok(p)
    }

    fn widths(&self) -> Result<Vec<(String, u16)>> {
        Ok(self
            .run(&[
                "list-panes",
                "-t",
                &format!("={UI_SESSION}:"),
                "-F",
                "#{pane_id}\t#{pane_width}",
            ])?
            .lines()
            .filter_map(|l| l.split_once('\t'))
            .map(|(p, w)| (p.to_string(), w.parse().unwrap_or(0)))
            .collect())
    }

    fn heights(&self) -> Result<Vec<(String, u16)>> {
        Ok(self
            .run(&[
                "list-panes",
                "-t",
                &format!("={UI_SESSION}:"),
                "-F",
                "#{pane_id}\t#{pane_height}",
            ])?
            .lines()
            .filter_map(|l| l.split_once('\t'))
            .map(|(p, h)| (p.to_string(), h.parse().unwrap_or(0)))
            .collect())
    }

    fn set_flag(&self, name: &str, on: bool) -> Result<()> {
        self.run(&[
            "set",
            "-g",
            &format!("@pebl_{name}"),
            if on { "1" } else { "0" },
        ])
        .map(|_| ())
    }

    fn set_split(&self, on: bool, accent: Option<&str>) -> Result<()> {
        self.run(&["set", "-g", "@pebl_split", if on { "1" } else { "0" }])?;
        let style = match accent {
            Some(c) => format!("fg={c}"),
            None => "fg=#3b3936".into(),
        };
        let _ = self.run(&[
            "set",
            "-w",
            "-t",
            &format!("={UI_SESSION}:"),
            "pane-active-border-style",
            &style,
        ]);
        Ok(())
    }

    fn popup(&self, cwd: &std::path::Path, title: &str, cmd: &str) -> Result<()> {
        let client = self
            .client_ttys()
            .into_iter()
            .next()
            .context("pebl isn't open in a terminal")?;
        self.command()
            .args([
                "display-popup",
                "-E",
                "-w",
                "94%",
                "-h",
                "90%",
                "-b",
                "rounded",
            ])
            .args(["-S", "fg=#3b3936", "-c", &client, "-T", title, "-d"])
            .arg(cwd)
            .arg(cmd)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .context("opening the review popup")?;
        Ok(())
    }

    fn size_parked(&self, size: (u16, u16)) -> Result<()> {
        let out = self.run(&["list-sessions", "-F", "#{session_name}"])?;
        for s in out.lines().filter(|s| s.starts_with("pebl-s-")) {
            let _ = self.run(&[
                "resize-window",
                "-t",
                &format!("={s}:"),
                "-x",
                &size.0.to_string(),
                "-y",
                &size.1.to_string(),
            ]);
        }
        let bg = self
            .run(&[
                "list-panes",
                "-s",
                "-t",
                &format!("={BG_SESSION}"),
                "-F",
                "#{window_id}\t#{@pebl_sid}",
            ])
            .unwrap_or_default();
        for (win, _) in bg
            .lines()
            .filter_map(|l| l.split_once('\t'))
            .filter(|(_, sid)| !sid.is_empty())
        {
            let _ = self.run(&[
                "resize-window",
                "-t",
                win,
                "-x",
                &size.0.to_string(),
                "-y",
                &size.1.to_string(),
            ]);
        }
        let _ = self.run(&[
            "set",
            "-g",
            "default-size",
            &format!("{}x{}", size.0, size.1),
        ]);
        Ok(())
    }
}

impl Tmux {
    fn ui_order(&self) -> Result<Vec<String>> {
        Ok(self
            .run(&[
                "list-panes",
                "-t",
                &format!("={UI_SESSION}:"),
                "-F",
                "#{pane_id}",
            ])?
            .lines()
            .map(str::to_string)
            .collect())
    }

    pub fn do_arrange(&self, a: &Arrangement) -> Result<()> {
        let want = a.order();
        let have = self.ui_order()?;
        let anchor = a
            .cols
            .first()
            .map(|(p, _)| p.clone())
            .unwrap_or_else(|| a.bar.0.clone());
        for p in want.iter().filter(|p| !have.contains(p)) {
            self.run(&["join-pane", "-d", "-h", "-l", "1", "-s", p, "-t", &anchor])?;
        }
        for p in have.iter().filter(|p| !want.contains(p)) {
            self.run(&[
                "break-pane",
                "-d",
                "-s",
                p,
                "-t",
                &format!("={BG_SESSION}:"),
            ])?;
        }
        let mut cur = self.ui_order()?;
        if cur.len() != want.len() {
            bail!("layout has {} panes, expected {}", cur.len(), want.len());
        }
        for i in 0..want.len() {
            if cur[i] == want[i] {
                continue;
            }
            let j = cur
                .iter()
                .position(|p| *p == want[i])
                .context("pane vanished")?;
            self.run(&["swap-pane", "-d", "-s", &want[i], "-t", &cur[i]])?;
            cur.swap(i, j);
        }
        self.run(&[
            "select-layout",
            "-t",
            &format!("={UI_SESSION}:"),
            &a.layout(),
        ])
        .map(|_| ())
    }
}

fn view_cmd(pebl: &str, role: &str) -> String {
    format!("while :; do {} view {role}; sleep 1; done", sh_quote(pebl))
}

fn parse_pane(line: &str) -> Option<LivePane> {
    let mut f = line.split('\t');
    let id = f.next()?.to_string();
    let sid = f.next().filter(|s| !s.is_empty()).map(str::to_string);
    let dead = f.next() == Some("1");
    let dead_status = f.next().and_then(|s| s.parse().ok());
    Some(LivePane {
        id,
        sid,
        dead,
        dead_status,
    })
}

pub fn sh_quote(s: &str) -> String {
    if !s.is_empty()
        && s.chars()
            .all(|c| c.is_ascii_alphanumeric() || "-_./:=@%+,".contains(c))
    {
        return s.to_string();
    }
    format!("'{}'", s.replace('\'', r"'\''"))
}

fn shell_line(argv: &[String]) -> String {
    let words: Vec<String> = argv.iter().map(|a| sh_quote(a)).collect();
    format!("exec {}", words.join(" "))
}

pub fn config(pebl: &str, truecolor: bool, cfg: &crate::config::Config) -> String {
    let keys = &cfg.keys;
    let p = sh_quote(pebl);
    let ctl = |action: &str| format!("run-shell -b \"{} ctl {action}\"", p.replace('"', "\\\""));
    let mut c = String::from(
        "# Generated by pebl on every launch. Edit ~/.pebl/config.toml instead.\n\
         set -g prefix None\n\
         set -g prefix2 None\n\
         set -g escape-time 0\n\
         set -g mouse on\n\
         set -g focus-events on\n\
         set -g extended-keys on\n\
         set -g default-terminal tmux-256color\n\
         set -g history-limit 50000\n\
         set -g status off\n\
         set -g status-style bg=default\n\
         set -g pane-border-style fg=#3b3936\n\
         set -g pane-active-border-style fg=#3b3936\n\
         set -g pane-border-lines single\n\
         set -g pane-border-indicators off\n\
         set -g allow-passthrough on\n\
         set -g set-clipboard on\n\
         set -g bell-action any\n\
         set -g visual-bell off\n\
         set -g set-titles on\n\
         set -g set-titles-string pebl\n\
         set -g window-size latest\n\
         set -g detach-on-destroy off\n",
    );
    c.push_str("set -su terminal-features\nset -as terminal-features ',*:extkeys'\n");
    if truecolor {
        c.push_str("set -as terminal-features ',*:RGB'\n");
    }
    c.push_str("bind -n S-Enter if -F '#{@pebl_sid}' 'send-keys M-Enter' 'send-keys Enter'\n");
    if let Some(bg) = cfg.background() {
        let bg = sh_quote(bg);
        c.push_str(&format!(
            "set -g window-style bg={bg}\nset -g window-active-style bg={bg}\nset -g status-style bg={bg}\n\
             set -g pane-border-style fg=#3b3936,bg={bg}\nset -g pane-active-border-style fg=#3b3936,bg={bg}\n"
        ));
    }
    c.push_str(&format!(
        "set-hook -g client-resized '{}'\n",
        ctl("resized")
    ));
    c.push_str(&format!(
        "set-hook -g client-attached '{}'\n",
        ctl("resized")
    ));
    c.push_str(&format!(
        "set-hook -g pane-focus-in '{}'\n",
        ctl("focused #{pane_id}")
    ));
    c.push_str(&format!("set-hook -g pane-died '{}'\n", ctl("pane-died")));
    c.push_str(&format!(
        "set-hook -g after-resize-pane '{}'\n",
        ctl("dragged")
    ));
    let key = |k: &str| format!("'{}'", k.replace('\'', ""));
    for (k, action) in keys.bindings() {
        if action == "quit" {
            c.push_str(&format!("bind -n {} detach-client\n", key(k)));
        } else {
            c.push_str(&format!("bind -n {} {}\n", key(k), ctl(action)));
        }
    }
    let mut only: Vec<(String, String, &str)> = vec![
        ("M-Left".into(), "resize left".into(), "split"),
        ("M-Right".into(), "resize right".into(), "split"),
        ("M-Up".into(), "drawer-resize up".into(), "drawer"),
        ("M-Down".into(), "drawer-resize down".into(), "drawer"),
        ("M-r".into(), "run-next".into(), "runnable"),
        ("M-c".into(), "shell-to-claude".into(), "shellout"),
    ];
    if !keys.close_column.is_empty() {
        only.push((keys.close_column.clone(), "close-column".into(), "split"));
    }
    for n in 1..=4 {
        only.push((format!("M-{n}"), format!("column {n}"), "split"));
    }
    for (k, action, flag) in only {
        c.push_str(&format!(
            "bind -n {} if -F '#{{==:#{{@pebl_{flag}}},1}}' '{}' 'send-keys {}'\n",
            key(&k),
            ctl(&action),
            k.replace(['\'', '"', '\\'], "")
        ));
    }
    c
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn quoting() {
        assert_eq!(sh_quote("claude"), "claude");
        assert_eq!(sh_quote("fix it"), "'fix it'");
        assert_eq!(sh_quote("it's"), r"'it'\''s'");
        assert_eq!(
            shell_line(&["claude".into(), "say hi".into()]),
            "exec claude 'say hi'"
        );
    }

    #[test]
    fn parses_panes() {
        let p = parse_pane("%3\ts1\t1\t2").unwrap();
        assert_eq!(
            p,
            LivePane {
                id: "%3".into(),
                sid: Some("s1".into()),
                dead: true,
                dead_status: Some(2)
            }
        );
        assert_eq!(parse_pane("%4\t\t0\t").unwrap().sid, None);
    }

    #[test]
    fn spawn_list_kill_on_private_socket() {
        if Command::new("tmux").arg("-V").output().is_err() {
            return;
        }
        let t = Tmux::new(format!("pebl-test-{}", std::process::id()));
        let spec = SpawnSpec {
            sid: "s1".into(),
            cwd: std::env::temp_dir(),
            argv: vec!["sh".into(), "-c".into(), "sleep 30".into()],
            env: vec![("PEBL_SESSION_ID".into(), "s1".into())],
            size: (80, 24),
        };
        let res = (|| -> Result<()> {
            let pane = t.spawn(&spec)?;
            let live = t.list()?;
            assert!(live
                .iter()
                .any(|p| p.id == pane && p.sid.as_deref() == Some("s1") && !p.dead));
            t.kill(&pane)?;
            Ok(())
        })();
        let _ = t.kill_server();
        res.unwrap();
    }

    #[test]
    fn config_passes_shift_enter_through_and_survives_reloads() {
        if Command::new("tmux").arg("-V").output().is_err() {
            return;
        }
        let t = Tmux::new(format!("pebl-test-cfg-{}", std::process::id()));
        let dir = tempfile::tempdir().unwrap();
        let conf = dir.path().join("tmux.conf");
        std::fs::write(&conf, config("pebl", true, &Default::default())).unwrap();
        let conf = conf.display().to_string();
        let res = (|| -> Result<()> {
            t.run(&["new-session", "-d", "-s", "x", "sleep 30"])?;
            t.run(&["source-file", &conf])?;
            t.run(&["source-file", &conf])?;
            let features = t.run(&["show", "-s", "terminal-features"])?;
            assert_eq!(features.matches("RGB").count(), 1, "{features}");
            assert_eq!(features.matches("extkeys").count(), 1, "{features}");
            let keys = t.run(&["list-keys", "-T", "root", "S-Enter"])?;
            assert!(keys.contains("send-keys M-Enter"), "{keys}");
            Ok(())
        })();
        let _ = t.kill_server();
        res.unwrap();
    }

    #[test]
    fn arranges_columns_with_a_layout_string() {
        if Command::new("tmux").arg("-V").output().is_err() {
            return;
        }
        let t = Tmux::new(format!("pebl-test-arr-{}", std::process::id()));
        let res = (|| -> Result<()> {
            let ui = t.build_ui("true", (120, 40), 30)?;
            t.run(&["new-session", "-d", "-s", BG_SESSION, "sleep 60"])?;
            let extra = t.run(&[
                "new-session",
                "-d",
                "-s",
                "pebl-s-x",
                "-P",
                "-F",
                "#{pane_id}",
                "sleep 60",
            ])?;
            let tabs = t.tabs_pane("true")?;
            let (w, h) = t.window_size()?;
            let a = Arrangement {
                win: (w, h),
                sidebar: Some((ui.sidebar.clone(), 30)),
                tabs: Some(tabs.clone()),
                cols: vec![(ui.stage.clone(), 44), (extra.clone(), 44)],
                drawer: None,
                bar: (ui.bar.clone(), 1),
            };
            t.arrange(&a)?;
            let got = t.widths()?;
            assert_eq!(got.len(), 5, "{got:?}");
            let wid = |p: &str| got.iter().find(|(x, _)| x == p).map(|(_, w)| *w);
            assert_eq!(wid(&ui.sidebar), Some(30));
            assert_eq!(wid(&ui.stage), Some(44));
            assert_eq!(wid(&extra), Some(w - 30 - 1 - 44 - 1));
            assert_eq!(wid(&tabs), Some(w - 31));
            assert_eq!(t.slot()?, Some(ui.stage.clone()));
            let single = Arrangement {
                win: (w, h),
                sidebar: None,
                tabs: None,
                cols: vec![(ui.stage.clone(), w)],
                drawer: None,
                bar: (ui.bar.clone(), 1),
            };
            t.arrange(&single)?;
            assert_eq!(t.widths()?.len(), 2);
            let shell = t.spawn_shell("s9", &std::env::temp_dir(), (80, 20))?;
            let with_drawer = Arrangement {
                drawer: Some((shell.clone(), 10)),
                ..single.clone()
            };
            t.arrange(&with_drawer)?;
            let hs = t.heights()?;
            let ht = |p: &str| hs.iter().find(|(x, _)| x == p).map(|(_, h)| *h);
            assert_eq!(ht(&shell), Some(10));
            assert_eq!(ht(&ui.stage), Some(h - 1 - 1 - 10 - 1));
            assert!(t.shells()?.iter().any(|s| s.pane == shell && s.sid == "s9"));
            t.arrange(&single)?;
            assert_eq!(t.widths()?.len(), 2);
            Ok(())
        })();
        let _ = t.kill_server();
        res.unwrap();
    }
}
