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
| `alt+t` | another shell beside the current one (or click `+ split`); `alt+←/→` moves between them |
| `alt+a` | ask Jel how to do anything in pebl (or click Jel) |
| `ctrl+f` | search every past Claude Code conversation, and resume one |
| `ctrl+x` | mark sessions (`space`, `a` for a whole project), then `enter` to send them all one prompt |
| `ctrl+p` | saved prompts: pick one and send it, with your clipboard filled in |

The bottom bar shows the few keys that matter for what's selected right now (a waiting session offers `enter answer it`, one with failing checks offers `t`, a project offers `q task queue`), and `?` lists every key. With the sidebar focused: `j`/`k` move, `enter` opens (or resumes an exited session), `space` folds a project, `J`/`K` (or `shift+↓/↑`) move a project down or up, `p` peeks, `d` reviews, `R` starts a reviewer agent, `P` queues the tasks from a session's plan, `t` sends failing checks to Claude, `T` runs checks now, `f` finishes a worktree, `r` renames, `x` kills, `w` starts a session in a new git worktree, `e` opens the folder in VS Code, `q` opens the task queue, `H` opens the session's timeline, `N` adds a project note, `m` mutes the mascot and bell, `/` filters, `?` shows all shortcuts.

### Peek and quick reply

`p` opens a strip above the status bar showing the last lines of the oldest session that's waiting and not on screen (or the session under the cursor). Answer it without switching: press a number to pick a permission option, `↑/↓` and `enter` to move and confirm, or type a reply and press `enter` to send it. `tab` moves to the next waiting session, `shift+tab` opens the one you're peeking at, `esc` takes you back to where you were.

### Shell drawer

`ctrl+t` opens a real shell (your `$SHELL`) in the session's folder, under the Claude pane. Each session has its own, and it keeps its history and running processes when you switch sessions or hide it. Press `ctrl+t` from the shell to go back to Claude, and again from Claude to hide it; dragging its border to the bottom hides it too.

Need two at once, like a backend and a frontend? While in a shell, click `+ split` at the bottom right of the status bar (or press `alt+t`) for another shell beside it in the same folder. `× close` closes the one you're in (so does `exit`), and `alt+←/→` or a click moves between them.

When Claude's last reply contains a shell code block, the status bar offers `alt+r`: it opens the drawer and types the first line at the prompt without running it. Press `enter` to run it, `alt+r` again for the next line. While the drawer is hidden, a line above the status bar shows what's running in it; if the command fails, the line turns red with the error, and `alt+c` pastes the end of the output into Claude's input so you can ask about it.

### Search

`ctrl+f` searches every Claude Code conversation on disk, including ones you started outside pebl (tagged `cli`); ones open in pebl are tagged `live`. Type to search, `↑/↓` to choose, `tab` to switch between this project and all projects, `enter` to resume the conversation as a pebl session (or jump to it if it's already open). pebl indexes your history in the background when it starts, reading only prompts and replies.

### Send one prompt to several sessions

`ctrl+x` puts the sidebar in mark mode: `space` marks a session, `a` marks every session in its project, and marks can span projects. Sessions waiting for you can't be marked until you answer them. `enter` opens a strip listing the targets; type the prompt and press `enter`. Idle sessions get it now; running ones get it when their current turn ends.

### Inbox

`ctrl+y` lists every session that needs you, across all projects: waiting ones first (longest waiting at the top, with what they want, like `wants to run Bash`), then ones that stopped with an error, then ones that finished while you weren't looking. `enter` opens one, `p` peeks and answers it without switching, `x` dismisses an error or finished one, `esc` closes. Finished sessions drop off on their own once you've looked at them, or after 15 minutes. With two or more waiting, the status bar shows `ctrl+y list`.

### Checks after every turn

Each time a session finishes a turn that changed files (turns that only answer questions cost nothing), pebl runs the project's own checks in the background, in that session's folder, and shows `…` while they run, then `✓` or `✕` next to the session. There's nothing to set up: pebl finds them itself.

| Project has | pebl runs |
|---|---|
| `package.json` with `test`, `lint` or `typecheck` scripts | them, with npm, pnpm, yarn or bun (from the lockfile); skipped if `node_modules` is missing |
| `Cargo.toml` | `cargo test --quiet` |
| `go.mod` | `go test ./...` |
| pytest config | `python -m pytest -q` |
| a `Makefile` with a `test` target | `make test` |

`pebl checks` (in a project folder) shows what pebl found. When a check fails, `t` on the session sends Claude the failing commands with the end of their output and asks it to fix them; `T` runs the checks again now. Each command has 10 minutes.

To choose the checks yourself without touching the repo, put a `[checks]` table in the private settings file `pebl checks` prints (under `~/.pebl/projects/`), or in a `pebl.toml` in the repo to share it with your team:

```toml
[checks]
test = "npm test"
lint = "npx eslint src"
auto_fix = 2      # send failures back to Claude by itself, up to twice per task; resets when you send the next prompt yourself
```

An empty `[checks]` table turns them off for that project; `checks = false` in `~/.pebl/config.toml` turns detection off everywhere.

### Inbox

`ctrl+y` lists every session that needs you, across all projects: waiting ones first (longest waiting at the top, with what they want, like `wants to run Bash`), then ones that stopped with an error, then ones that finished while you weren't looking. `enter` opens one, `p` peeks and answers it without switching, `x` dismisses an error or finished one, `esc` closes. Finished sessions drop off on their own once you've looked at them, or after 15 minutes. With two or more waiting, the status bar shows `ctrl+y list`.

### Task queue and best of N

`q` in the sidebar opens the selected project's queue. `a` adds a task (type it, `enter`); pebl starts it in a fresh git worktree session as soon as a slot is free, without switching your screen. When its turn ends and the project's checks pass, it moves to "needs review" and the next task starts. Two run at a time by default; to change that, add this to the project's settings (`~/.pebl/projects/…` or `pebl.toml`):

```toml
[queue]
max_parallel = 3
```

In the queue, `enter` opens a task's session, `d` reviews its diff, `D` marks it done, `r` retries a failed one in a new worktree, `x` removes it (twice for a running task: that discards its sessions and worktrees). Merging it with `f`, or closing its session, marks the task done.

**Best of N:** while typing a task, `tab` sets how many attempts to run (up to 4). Each gets its own worktree. When they're done, `enter` on the task opens a compare view with each attempt's checks, files changed, lines added and removed, tokens and time; `d` shows an attempt's diff, `enter` opens it, and `w` twice keeps it and discards the others.

From the command line:

```sh
pebl queue add "Fix the rota export for night shifts"
pebl queue add --attempts 3 "Make the dashboard query faster"
pebl queue add --file tasks.md        # every open "- [ ]" item; indented lines below one join its prompt
pebl queue ls
```

### Agents working together

**Reviewer.** `R` on a session starts a second Claude in the same folder that reads the first one's diff (and its task, if it came from the queue) without changing anything, then lists what it found. When it's done, `d` on the original session opens the review with those comments already on the right lines: edit or delete them, then `s` sends them back. Add this to the project's settings (its file under `~/.pebl/projects/`, or `pebl.toml`) to review every queued task automatically once its checks pass:

```toml
[review]
auto = true
```

**Planner.** Give a session a big goal with the "plan into tasks" saved prompt (it asks for a `- [ ]` checklist and changes nothing). Then `P` on that session lists the tasks: `space` unticks any you don't want, `enter` queues the rest, each in its own worktree.

**Merge conflicts.** If `m` in the finish strip hits conflicts, pebl backs the merge out, lists the conflicting files, and `c` asks the session to rebase onto the target branch and resolve them. pebl never commits for you; press `f` then `m` again once it's done.

### Recipes

A recipe is a list of steps pebl walks through on its own, each starting when the one before has finished. Recipes show up in the `ctrl+p` list (tagged `recipe · N steps`); `enter` runs one as a queued task in its own worktree, and `q` shows which step it's on. pebl adds a "fix ticket" recipe to start from: copy a ticket, `ctrl+p`, pick it, and it investigates, implements, gets the checks green, has a reviewer look, applies the review and commits.

```toml
name = "fix ticket"
description = "Investigate, fix, test, review and commit a copied ticket"
steps = [
  "Investigate this ticket and plan the fix. Don't change code yet.\n\n{{clipboard}}",
  "Implement the fix you planned.",
  "checks",
  "review",
  "Commit your changes with a clear message.",
]
```

A step is a prompt, or one of:

| Step | What pebl does |
|---|---|
| `checks` | waits for the project's checks; if they fail, sends the failures to Claude, up to twice, then stops the recipe |
| `review` | starts a reviewer and sends its findings back to Claude |
| `open PR` | asks Claude to commit, push and open a pull request with `gh` |

`{{clipboard}}` is replaced by what you copied; any other `{{name}}` is asked for when the recipe starts. Your recipes live in `~/.pebl/recipes/`; a repo can share some in `.pebl/recipes/`. From the command line: `pebl recipe ls`, `pebl recipe add <file or URL>`, `pebl recipe run "fix ticket" --var ticket=BUG-12`.

### Timeline and rollback

pebl keeps a timeline of every session: your prompts, the tools Claude used (which files it edited, which commands it ran), what each turn changed and how the checks went. At the end of each turn that changed files it also takes a snapshot of the folder, stored as a hidden git ref (`refs/pebl/…`), without touching your branch, commits or staging area.

`H` on a session opens the timeline, so you can see what an agent did while you were away. `r` twice on a turn puts the files back to how they were after it; what's there now is snapshotted first, so a rollback can itself be rolled back. Only files are restored: Claude's chat still remembers the later turns (`/rewind` in Claude Code rewinds that). Snapshots and the timeline are deleted with the session.

### Project notes

`N` in the sidebar adds a one-line note to the selected project, like "we use pnpm, never npm" or "don't touch /legacy". Every new or resumed session in that project starts with the notes in its system prompt, so you never repeat them. `ctrl+e` while typing a note opens all of them in your editor.

Notes live in `~/.pebl/memory/`, outside your repo. To share them with your team instead, create a `PEBL.md` in the project folder: pebl then reads and adds to that file. Sessions already running keep the notes they started with.

### Saved prompts

`ctrl+p` lists your saved prompts above the status bar. Type to filter (if nothing matches, `enter` saves what you typed as a new prompt), `↑/↓` to choose, `enter` to send it to the session you're in, or `tab` to type it into Claude's input so you can edit it first. Running sessions get it when their turn ends. In mark mode (`ctrl+x`), `p` sends a saved prompt to every marked session.

Put `{{clipboard}}` in a prompt where pasted text should go. Copy a ticket, press `ctrl+p`, pick "investigate ticket" (pebl adds that one for you) and press `enter`: the ticket lands where `{{clipboard}}` was. The strip shows the clipboard's first line so you know what will be sent.

`ctrl+a` adds a prompt and opens it in your editor, `ctrl+s` saves the last prompt you sent in this session, `ctrl+e` edits the selected one, `ctrl+d` deletes it. Prompts are plain Markdown files in `~/.pebl/prompts/`, one per prompt, named after the file.

### Worktrees

`w` starts a session in its own git worktree on a `pebl/<name>` branch. If the project's settings (its file under `~/.pebl/projects/`, or a `pebl.toml` in the repo) have a `[worktree]` table, pebl copies the listed files into the new worktree and runs the setup commands before Claude starts:

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

When every session is idle, Jel shows a tip every 10 seconds, starting with features you haven't used yet.

**Ask Jel.** Click Jel (or `ask jel` in the status bar, or press `alt+a`) and ask anything about pebl or your sessions: "how do I run two terminals?", "which chat is waiting on me?". It answers in one line using your own Claude Code (`claude -p` with Haiku, so no API key, and a little of your plan per question), from a guide built into pebl ([docs/JEL.md](docs/JEL.md)) and your live sessions. When the answer is something pebl can do, `enter` does it.

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
ask = "M-a"           # ask Jel
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
