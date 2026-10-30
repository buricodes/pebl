# pebl

Run several Claude Code sessions side by side in one terminal window, and see at a glance which ones need you.

pebl is a terminal-native orchestrator for Claude Code: a project sidebar on the left, the real `claude` on the right, a status bar at the bottom. There's no Electron and no browser tab. The main pane is Claude Code itself, untouched.

```
 projects                   3 │ nova-api / auth-refactor                     ● running · 2m
                              │
 ▾ nova-api               ◆ 1 │  (live Claude Code session)
 › ● auth-refactor         2m │
   ◆ migrate-staging    input │
   ○ docs-pass             1h │
                              │
 ▾ billing-svc              3 │
   ● onboarding-flow      14s │
   ✕ e2e-tests         exit 1 │
                              │
 + new session         ctrl+n │
 + new project         ctrl+o │
──────────────────────────────┴──────────────────────────────────────────────────────────────
 ctrl+n new session · ctrl+j/k switch · ctrl+b sidebar · ctrl+q quit   ◆ 1 waiting ctrl+g …
```

## Install

pebl runs on macOS and Linux. Four steps, each one a single command.

**1. Rust**, which builds pebl. Skip this if `cargo --version` already works.

```sh
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
```

Then open a new terminal so `cargo` is on your PATH.

**2. tmux 3.2 or newer**, which pebl uses under the hood. Skip this if `tmux -V` shows 3.2 or higher.

```sh
brew install tmux          # macOS
sudo apt install tmux      # Debian / Ubuntu
```

**3. Claude Code**, installed and logged in. Follow [the Claude Code setup guide](https://docs.claude.com/en/docs/claude-code), then run `claude` once to log in.

**4. pebl**

```sh
cargo install --git https://github.com/buricodes/pebl
pebl doctor
```

The build takes about a minute. `pebl doctor` checks that tmux, Claude Code and the hooks are all set up, and tells you what's missing. If your shell says `pebl: command not found`, `~/.cargo/bin` isn't on your PATH yet: open a new terminal, or run `source ~/.cargo/env`.

**Updating:** run `pebl update`. It installs the latest release and switches the running pebl over without stopping your sessions. pebl checks GitHub once a day and shows `pebl 0.x.y is out · pebl update` in the status bar when there's a new release; `pebl doctor` reports it too. Set `update_check = false` in `~/.pebl/config.toml` to turn the check off. To hear about releases by email, use **Watch → Custom → Releases** on GitHub.

**Usage ping:** once a day pebl sends a random install id (made on first run, stored in `~/.pebl/install_id`) and its version number to `getpebl.vercel.app`, so the website can show how many people used pebl this week. Nothing else is sent: no names, paths, projects, prompts or anything from Claude, and the server keeps only a hash of the id. Turn it off with `telemetry = false` in `~/.pebl/config.toml`, or set `DO_NOT_TRACK=1` or `PEBL_NO_TELEMETRY=1`. `pebl doctor` shows whether it's on.

**From a clone:** `cargo install --path .`

**Optional (macOS):** `pebl hotkey install` adds a global shortcut that brings pebl to the front from any app. See [Summon from anywhere](#summon-from-anywhere-macos).

## Use

```sh
cd ~/code/my-project
pebl
```

The first time, press **enter** to add the current folder and start a session. pebl also finds folders you already used with Claude Code; press **i** to import them.

| Key | Action |
|---|---|
| `ctrl+j` / `ctrl+k` | next / previous session |
| `ctrl+g` | jump to whatever needs you, longest waiting first; press again for the next ("all clear" when nothing does) |
| `ctrl+y` | the inbox: everything waiting, errored or finished since you looked |
| `ctrl+n` | new session in the current project (`tab` adds a first prompt) |
| `ctrl+o` | new project (path completes with `tab`) |
| `ctrl+b` | hide or show the sidebar |
| `ctrl+space` | move focus between the sidebar and Claude Code |
| `ctrl+q` | quit; sessions keep running, and `pebl` reattaches |
| `ctrl+\` | open a session in a new column, side by side (up to 4) |
| `alt+1`…`alt+4` | focus a column |
| `alt+←` / `alt+→` | resize columns (then `←/→`, `shift` for 5, `=` to equalise, `enter` when done); dragging a border works too |
| `ctrl+w` | close the focused column; the session keeps running |
| `ctrl+t` | open the shell drawer under Claude; again to go back to Claude, again to hide it |
| `alt+r` | type the next command from Claude's last code block into the shell (you press enter) |
| `alt+↑` / `alt+↓` | resize the shell drawer (then `↑/↓`, `shift` for 5, `=` for half, `enter`) |
| `alt+c` | paste the end of the shell's output into Claude's input |
| `ctrl+f` | search every past Claude Code conversation, and resume one |
| `ctrl+x` | mark sessions (`space`, `a` for a whole project), then `enter` to send them all one prompt |
| `ctrl+p` | saved prompts: pick one and send it, with your clipboard filled in |

With the sidebar focused: `j`/`k` move, `enter` opens (or resumes an exited session), `space` folds a project, `J`/`K` (or `shift+↓/↑`) move a project down or up, `p` peeks, `d` reviews, `f` finishes a worktree, `r` renames, `x` kills, `w` starts a session in a new git worktree, `e` opens the folder in VS Code, `m` mutes the mascot and bell, `/` filters, `?` shows all shortcuts.

### Peek and quick reply

`p` opens a strip above the status bar showing the last lines of the oldest session that's waiting and not on screen (or the session under the cursor). Answer it without switching: press a number to pick a permission option, `↑/↓` and `enter` to move and confirm, or type a reply and press `enter` to send it. `tab` moves to the next waiting session, `shift+tab` opens the one you're peeking at, `esc` takes you back to where you were.

### Shell drawer

`ctrl+t` opens a real shell (your `$SHELL`) in the session's folder, under the Claude pane. Each session has its own, and it keeps its history and running processes when you switch sessions or hide it. Press `ctrl+t` from the shell to go back to Claude, and again from Claude to hide it; dragging its border to the bottom hides it too.

When Claude's last reply contains a shell code block, the status bar offers `alt+r`: it opens the drawer and types the first line at the prompt without running it. Press `enter` to run it, `alt+r` again for the next line. While the drawer is hidden, a line above the status bar shows what's running in it; if the command fails, the line turns red with the error, and `alt+c` pastes the end of the output into Claude's input so you can ask about it.

### Search

`ctrl+f` searches every Claude Code conversation on disk, including ones you started outside pebl (tagged `cli`); ones open in pebl are tagged `live`. Type to search, `↑/↓` to choose, `tab` to switch between this project and all projects, `enter` to resume the conversation as a pebl session (or jump to it if it's already open). pebl indexes your history in the background when it starts, reading only prompts and replies.

### Send one prompt to several sessions

`ctrl+x` puts the sidebar in mark mode: `space` marks a session, `a` marks every session in its project, and marks can span projects. Sessions waiting for you can't be marked until you answer them. `enter` opens a strip listing the targets; type the prompt and press `enter`. Idle sessions get it now; running ones get it when their current turn ends.

### Inbox

`ctrl+y` lists every session that needs you, across all projects: waiting ones first (longest waiting at the top, with what they want, like `wants to run Bash`), then ones that stopped with an error, then ones that finished while you weren't looking. `enter` opens one, `p` peeks and answers it without switching, `x` dismisses an error or finished one, `esc` closes. Finished sessions drop off on their own once you've looked at them, or after 15 minutes. With two or more waiting, the status bar shows `ctrl+y list`.

### Saved prompts

`ctrl+p` lists your saved prompts above the status bar. Type to filter (if nothing matches, `enter` saves what you typed as a new prompt), `↑/↓` to choose, `enter` to send it to the session you're in, or `tab` to type it into Claude's input so you can edit it first. Running sessions get it when their turn ends. In mark mode (`ctrl+x`), `p` sends a saved prompt to every marked session.

Put `{{clipboard}}` in a prompt where pasted text should go. Copy a ticket, press `ctrl+p`, pick "investigate ticket" (pebl adds that one for you) and press `enter`: the ticket lands where `{{clipboard}}` was. The strip shows the clipboard's first line so you know what will be sent.

`ctrl+a` adds a prompt and opens it in your editor, `ctrl+s` saves the last prompt you sent in this session, `ctrl+e` edits the selected one, `ctrl+d` deletes it. Prompts are plain Markdown files in `~/.pebl/prompts/`, one per prompt, named after the file.

### Worktrees

`w` starts a session in its own git worktree on a `pebl/<name>` branch. If the repo has a `pebl.toml`, pebl copies the listed files into the new worktree and runs the setup commands before Claude starts:

```toml
[worktree]
copy = [".env", ".env.local"]
setup = ["npm install"]
```

When the work is done, `f` on that session opens the finish strip: `a` asks Claude to commit, `m` merges the branch into the repo's current branch and removes the worktree, branch and session, `p` pushes and opens a pull request with `gh` (keeping the worktree for review fixes), and `d` twice discards everything.

### Review

`d` opens a full screen diff of the session's folder in a popup over pebl: everything changed since the branch left `main` (or uncommitted changes when you're on `main`), new untracked files included. `j`/`k` move, `n`/`N` jump between files, `c` comments on a line, `x` deletes a comment, and `s` sends all your comments back to that session as one prompt with file and line references. `q` closes.

`ctrl+\` opens the session under the cursor in a new column, or the one that most needs you if the cursor is on something already showing. In the sidebar, `c` opens the session under the cursor in a new column and puts the session you were looking at back in column 1, so you can pick exactly which sessions sit side by side. Each column gets a tab with its number, project, session and state; click a tab to focus it or its `×` to close it. Columns are at least 30 cells wide, so how many fit depends on your window (hide the sidebar with `ctrl+b` for more room). `alt+number`, `alt+arrows` and `ctrl+w` only act while columns are open; otherwise they go to Claude Code as normal.

Status glyphs: `●` running · `◆` waiting for you · `○` idle · `✕` error · `·` exited. Status comes from Claude Code hooks, passed to each session with `--settings`. pebl never edits your `~/.claude/settings.json`.

### From scripts

```sh
pebl add ~/code/api          # add a project
pebl new api -n fix-login -p "fix the login redirect"
pebl ls                      # project, session, status, age (tab separated)
pebl update                  # get the latest pebl from GitHub; sessions keep running
pebl reload                  # restart pebl's own panes on the installed version
pebl chime                   # hear the notification sounds
pebl kill --yes              # stop pebl and every session it runs
```

## Summon from anywhere (macOS)

```sh
pebl hotkey install                  # double-tap cmd (⌘)
pebl hotkey install --trigger option # or ctrl / shift / space
pebl hotkey install --trigger cmd+shift+p  # a combo: needs no permission
pebl hotkey uninstall
```

A double-tap (or the combo) brings the terminal window running pebl to the front, or opens a new window running `pebl`. Double-tap triggers need **pebl-hotkey** allowed under System Settings → Privacy & Security → Input Monitoring (then `launchctl kickstart -k gui/$(id -u)/com.pebl.hotkey`). Combos are ordinary system hotkeys and need no permission. Bringing an existing window forward also needs a one-time Automation approval for Terminal or iTerm.

The agent only listens; it can't block or change keystrokes. That's why `--trigger space` is not the default: both spaces still get typed into whatever app you're in, and a fast double space while typing will fire it.

Set `terminal = "iTerm"` (or `"Ghostty"`) in `~/.pebl/config.toml` to choose which app opens.

## Usage and context

Each session row in the sidebar has a one cell gauge (▁ to █) for how full its context window is: grey normally, orange over 80%, red and bold over 95%. The focused session gets a second header line with the model, context used, and tokens in and out. The bottom of the sidebar shows your plan usage, the 5 hour and weekly windows with when the 5 hour one resets, or today's token totals if your plan doesn't report limits. Column tabs show the gauge too.

The numbers come from a small statusline that pebl gives each session (`pebl statusline`), which also prints a grey line under Claude's input box with the same figures. Set `statusline = false` in `~/.pebl/config.toml` to turn it off; the gauges and plan usage go with it.

## Mascot

The bottom of the sidebar holds a mascot that works as a notification line. Its face and colour show the most urgent state across all sessions (needs you › error › finished › working › idle) and the text beside it names the session, what it wants, and the key to act on it. `ctrl+g` (or clicking it) goes straight there. In a short or narrow sidebar it shrinks to a one-line face; `m` mutes it along with the bell.

The default is Jel the jellyfish. Run `pebl mascots` to see the others (pebl, shroom, toast, nib, hoot) in every mood and pick one, or `o` there to turn it off.

## Configure

`~/.pebl/config.toml`:

```toml
terminal = "Terminal"
statusline = true              # feeds context and plan usage to pebl
mascot = "jel"                 # jel, pebl, shroom, toast, nib, hoot or off
editor = "Visual Studio Code" # what `e` opens: an app name, or a command like "zed"
# background = "#141413" # paint pebl in the design's near-black instead of your terminal's colour

[notify]
desktop = true        # macOS notification when a session needs you, errors or finishes, unless pebl is the window you're looking at
bell = false          # terminal bell; true, false, or a list of events: ["waiting", "error", "done"] (on Linux the default is ["waiting"])
osc = false           # notification sent through the terminal (Ghostty, iTerm2, Kitty, WezTerm); same values
chime = true          # play a sound for sessions you're not looking at, even when Terminal is in front
chime_waiting = "Glass"  # any name in /System/Library/Sounds, a path to a sound file, or "" for none
chime_done = "Pop"
chime_error = "Basso"
phone = ""            # an ntfy.sh topic (or full URL) to push to your phone
phone_after = 120     # seconds a session must stay unanswered before the phone push

[keys]          # tmux key names; "" unbinds
next = "C-j"
prev = "C-k"
waiting = "C-g"
new_session = "C-n"
new_project = "C-o"
sidebar = "C-b"
focus = "C-Space"
quit = "C-q"
split = "C-\\"
close_column = "C-w"  # only while columns are open
search = "C-f"
mark = "C-x"
shell = "C-t"         # Claude Code uses ctrl+t to show its task list; rebind if you rely on it
prompts = "C-p"       # saved prompts; inside Claude ctrl+p is otherwise "previous line"
inbox = "C-y"         # everything that needs you
```

The defaults follow the design and override some Claude Code shortcuts while pebl is open: `ctrl+b` (background a bash command), `ctrl+o` (transcript), `ctrl+g` (open the prompt in your editor), and `ctrl+j` (newline). While columns are open, `ctrl+w` closes a column instead of deleting a word. Rebind any of these if you rely on them. On macOS, `ctrl+space` may also be the input-source switch.

For a new line in Claude's prompt, `shift+enter` works in terminals that send it (Warp, iTerm2, Ghostty, Kitty, WezTerm), and pebl passes it through to Claude. macOS Terminal.app sends it as a plain `enter`, so there use `\` then `enter`, or `option+enter` with "Use Option as Meta key" on. `pebl doctor` tells you which applies.

## How it works

pebl runs its own tmux server (`tmux -L pebl`, separate from yours). The sidebar, stage, and status bar are small pebl panes driven by a daemon, and each Claude Code session lives in its own hidden tmux session that gets swapped into the main pane. Sessions therefore survive closing the terminal. See [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md).

State lives in `~/.pebl/`: `state.json`, `config.toml`, the socket, logs, and worktrees.

## Docs

| File | What's in it |
|---|---|
| `docs/SPEC.md` | Product spec: goals, non-goals, UX, keybindings |
| `docs/ARCHITECTURE.md` | Stack, process model, status detection, and how it was built |
| `docs/RESEARCH.md` | Prior art |
| `docs/ROADMAP.md` | Milestones and what's done |

## License

MIT
