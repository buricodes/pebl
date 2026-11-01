# pebl guide for Jel

Jel answers questions about pebl from this file, the key list and the user's live sessions. Keep it current when a feature changes.

## What pebl is

A terminal app that runs several Claude Code sessions ("chats") side by side. Left: a sidebar of projects and their chats. Right: the real Claude Code. Bottom: a status bar with the keys that matter right now. It runs on its own private tmux server, so chats keep running after the terminal window closes; typing `pebl` again reattaches. Start it with `pebl` in a project folder. The first time, enter adds the current folder; `i` imports folders already used with Claude Code.

Status glyphs in the sidebar: ● running, ◆ waiting for you, ○ idle, ✕ error, · exited. The little gauge after a chat shows how full its context is (orange over 80%, red over 95%: type /compact in that chat).

## Chats and projects

- New chat: ctrl+n (tab adds a first prompt). New project: ctrl+o (tab completes the path). From a shell: `pebl add <folder>`, `pebl new <project> -n <name> -p "prompt"`.
- Switch chats: ctrl+j / ctrl+k, or click one in the sidebar.
- Sidebar focus: ctrl+space moves between the sidebar and Claude. ctrl+b hides or shows the sidebar.
- In the sidebar: j/k move, enter opens (or resumes an exited chat), space folds a project, J/K move a project up or down, r renames, x kills (asks first), / filters, e opens the folder in the editor (VS Code by default, `editor` in config), m mutes the mascot and sounds.
- Quit with ctrl+q: chats keep running. `pebl kill --yes` stops everything.
- If a project folder was renamed while a chat ran in it, pebl follows the new folder automatically.

## Something needs you

- ctrl+g jumps to the chat that has waited longest; press again for the next. "all clear" when nothing waits.
- ctrl+y opens the inbox: waiting chats first (with what they want, like "wants to run Bash"), then errors, then chats that finished while you weren't looking. enter opens, p peeks, x dismisses.
- p in the sidebar peeks: shows the end of a waiting chat in a strip so you can answer without switching (a number picks a permission option, or type a reply and press enter; tab goes to the next waiting chat).
- Sounds and notifications: pebl chimes (Glass waiting, Pop done, Basso error) and shows a macOS notification for chats you're not looking at, and also for the chat on screen when the pebl window isn't in front. `pebl chime` plays them. Settings live under `[notify]` in `~/.pebl/config.toml`: `desktop`, `chime`, `chime_waiting` / `chime_done` / `chime_error` (a name from /System/Library/Sounds, a file path, or "" for none), `bell`, `osc`.
- Phone alerts: set `phone = "<ntfy.sh topic>"` under `[notify]`, install the ntfy app and subscribe to that topic; `phone_after = 120` is how many seconds a chat must wait first.

## Shells

- ctrl+t opens a shell under the chat, in the chat's folder. ctrl+t again goes back to Claude, again hides it. Each chat keeps its own shells and their running processes.
- Several shells side by side (backend and frontend): click "+ split" at the bottom right of the status bar while in a shell, or press alt+t. "× close" closes the shell you're in, or type exit. alt+left / alt+right move between shells, or click one.
- alt+up / alt+down resize the drawer; dragging its border works too.
- alt+r types the next command from Claude's last code block into the shell (you press enter).
- alt+c sends the end of the shell's output to Claude, handy when a command fails.
- While the drawer is hidden, a line above the status bar shows what's running and turns red if it fails.

## Columns

- ctrl+\ opens a chat in a new column, side by side, up to 4 depending on width. c in the sidebar opens the selected chat in a column.
- alt+1 to alt+4 focus a column, alt+left / alt+right resize, ctrl+w closes the focused column (the chat keeps running). Click a column's tab to focus it, its × to close it.

## Sending prompts

- ctrl+p: saved prompts and recipes. Type to filter, enter sends to the chat you're in, tab types it into Claude's input to edit first. If nothing matches, enter saves what you typed as a new prompt. ctrl+a adds one in your editor, ctrl+s saves the last prompt you sent, ctrl+e edits, ctrl+d deletes. Prompts are Markdown files in `~/.pebl/prompts/`.
- `{{clipboard}}` in a saved prompt is replaced by what you copied, e.g. copy a ticket, ctrl+p, "investigate ticket", enter.
- ctrl+x marks chats (space marks one, a marks a whole project), then enter sends them all one prompt. Busy chats get it when their turn ends.
- ctrl+f searches every past Claude Code conversation (also ones started outside pebl); enter resumes it as a pebl chat. tab switches between this project and all.

## Checks

After each turn that changed files, pebl runs the project's own tests and linters in the background and shows … then ✓ or ✕ next to the chat. It finds them itself: package.json scripts test / lint / typecheck (npm, pnpm, yarn or bun), cargo test, go test, pytest, make test. t sends the failures to Claude to fix, T runs checks now. `pebl checks` shows what was found and where the private settings file is. To choose checks yourself, add a `[checks]` table (for example `test = "npm test"`, `auto_fix = 2` to send failures back automatically) to the private file under `~/.pebl/projects/`, or to a `pebl.toml` in the repo to share it. An empty `[checks]` table turns them off for a project; `checks = false` in config turns detection off everywhere.

## Worktrees, review and finishing

- w starts a chat in its own git worktree on a `pebl/<name>` branch, so parallel work doesn't collide. A `[worktree]` table can copy files (`copy = [".env"]`) and run setup (`setup = ["npm install"]`) first.
- d opens a full-screen diff of the chat's changes: j/k move, n/N jump files, c comments on a line, s sends all comments back to the chat, q closes.
- R starts a reviewer: a second Claude reads the diff without changing anything; then d shows its comments on the right lines. `[review] auto = true` reviews every queued task automatically.
- f finishes a worktree chat: a asks Claude to commit, m merges into the current branch and cleans up, p pushes and opens a pull request with gh, d twice discards. On merge conflicts pebl backs out, lists the files, and c asks Claude to rebase and fix them.

## Task queue, plans and recipes

- q opens the project's task queue. a adds a task; each runs in its own worktree, two at a time (`[queue] max_parallel = 3` changes it), and moves to "needs review" when its checks pass. In the queue: enter opens, d diff, D done, r retry, x remove.
- Best of N: while adding a task, tab sets attempts (up to 4); then compare them and w twice keeps the winner.
- From a shell: `pebl queue add "task"`, `pebl queue add --attempts 3 "task"`, `pebl queue add --file tasks.md`, `pebl queue ls`.
- Plans: ask a chat to plan with the "plan into tasks" saved prompt, then P on it queues the checklist's tasks.
- Recipes: multi-step workflows in ctrl+p (tagged recipe). Steps are prompts or `checks`, `review`, `open PR`. A "fix ticket" recipe comes built in. Files live in `~/.pebl/recipes/` or a repo's `.pebl/recipes/`. `pebl recipe ls`, `pebl recipe add <file or URL>`, `pebl recipe run "fix ticket" --var ticket=BUG-12`.

## Timeline and rollback

H on a chat opens its timeline: prompts, tools Claude used, files each turn changed, check results. r twice on a turn puts the files back to how they were after it (the current state is saved first, so it can be undone). Only files go back; Claude's chat still remembers later turns (/rewind in Claude Code handles that).

## Project notes

N adds a one-line note to a project, like "we use pnpm". Every new chat in that project starts with the notes. ctrl+e while typing a note opens them all. Notes live in `~/.pebl/memory/`, or in a `PEBL.md` in the repo to share with a team.

## Jel

Jel is the jellyfish at the bottom of the sidebar. It shows the most urgent thing (needs you, error, finished, working) and, when every chat is idle, a tip every 10 seconds, favouring features not used yet. Click Jel, click "ask jel" in the status bar, or press alt+a to ask a question. `pebl mascots` shows other mascots (pebl, shroom, toast, nib, hoot); `mascot = "off"` in config hides it.

## Usage

The bottom of the sidebar shows plan usage: the 5 hour and weekly windows and when the 5 hour one resets. The focused chat's header shows model, context and tokens. `statusline = false` in config turns this off.

## Setup, updates and fixes

- `pebl doctor` checks tmux, Claude Code and hooks, and explains what's missing.
- `pebl update` installs the latest release without stopping chats. `pebl reload` restarts pebl's own panes on the installed version.
- Global shortcut on macOS: `pebl hotkey install` (double-tap cmd), `--trigger option` / ctrl / shift, or a combo like `--trigger cmd+shift+p` (no permission needed). Double-tap needs pebl-hotkey allowed in System Settings › Privacy & Security › Input Monitoring. `pebl hotkey uninstall` removes it. `terminal = "iTerm"` or "Ghostty" in config picks the app.
- alt keys do nothing on macOS: turn on "Use Option as Meta key" in Terminal › Settings › Profiles › Keyboard (iTerm: Profiles › Keys › Option key acts as Esc+).
- New line in Claude's prompt: shift+enter in Warp, iTerm2, Ghostty, Kitty, WezTerm. Terminal.app can't send it: use \ then enter, or option+enter with Option as Meta on.
- Keys can be changed under `[keys]` in `~/.pebl/config.toml` (tmux key names, "" unbinds): next, prev, waiting, new_session, new_project, sidebar, focus, quit, split, close_column, search, mark, shell, prompts, inbox, ask. pebl's defaults override some Claude Code keys (ctrl+b, ctrl+o, ctrl+g, ctrl+j, ctrl+t, ctrl+p).
- Everything pebl stores is in `~/.pebl/`. pebl never edits `~/.claude/settings.json`; it passes its hooks to each chat with --settings.
- Anonymous usage ping once a day (random id and version only): `telemetry = false` in config or `DO_NOT_TRACK=1` turns it off.
