# pebl: Product Spec

## One-liner

A terminal UI that orchestrates multiple Claude Code sessions across projects, with a sidebar, and looks like Claude Code itself.

## Goals

1. **Terminal only.** Runs in any modern terminal. No GUI, no web server, no Electron.
2. **Feels like home.** Visual language copied from Claude Code: dark, monospace, coral accent, minimal borders. A Claude Code user should feel zero friction.
3. **Real Claude Code.** We never re-render or reimplement Claude Code's UI. The main pane is the actual `claude` process.
4. **Know who needs you.** Accurate, instant per-session status, driven by Claude Code hooks rather than screen scraping.
5. **Never lose work.** Sessions keep running if pebl quits or the terminal closes.
6. **Local and simple.** Single binary, config in `~/.pebl/`, no accounts, no telemetry.

## Non-goals (for v1)

- Supporting other agents (Codex, Cursor, OpenCode). Claude Code first; design the adapter seam, don't build it.
- Remote / mobile access.
- Built-in diff viewer or PR UI (Claude Code and `gh` already do this).
- Windows support (tmux dependency). Revisit later via WSL.
- Auto-accepting permission prompts.

## Core concepts

- **Project**: a directory on disk (usually a git repo). Has a name, path, and color.
- **Session**: one running `claude` process inside a project. A project can have many sessions.
- **Worktree session** (optional): a session started in a fresh git worktree on its own branch, so parallel sessions don't step on each other.

## Layout

```
┌ pebl ─────────────────┬───────────────────────────────────────────────┐
│ ▾ falsch-parker        │                                               │
│   ● api-refactor       │   (live Claude Code session)                  │
│   ◆ fix-login    !     │                                               │
│ ▸ bui                  │                                               │
│ ▾ space-crew           │                                               │
│   ○ main               │                                               │
│                        │                                               │
│ + new project          │                                               │
│ + new session          │                                               │
├────────────────────────┴───────────────────────────────────────────────┤
│ ^n new  ^j/^k switch  ^b sidebar  ^q quit   ~/code/falsch-parker  3/5  │
└────────────────────────────────────────────────────────────────────────┘
```

- **Sidebar** (~25% width, min 24 cols, collapsible): projects, expandable to sessions.
- **Main pane**: the selected session's Claude Code, full fidelity.
- **Status bar**: one line, shortcuts, current path, active/total sessions.

## Session status

| Status | Glyph | Color | Meaning |
|---|---|---|---|
| running | `●` | coral | Claude is working on a turn |
| waiting | `◆` + `!` | yellow, bold | Needs you: permission prompt or question |
| idle | `○` | grey | Turn finished, ready for next prompt |
| error | `✕` | red | Turn failed (API error etc.) |
| exited | `·` | dim | Process gone, can be restarted |

Waiting sessions bubble attention: the parent project row also shows `!` when collapsed, and the status bar shows a count.

## Keybindings (defaults, configurable)

All global keys use a prefix so they never collide with Claude Code's own input. Default prefix: `ctrl+space`. Direct shortcuts listed as `^x` work when the sidebar has focus.

| Key | Action |
|---|---|
| `^n` | New session in selected project |
| `^p` | New project (pick a directory) |
| `^j` / `^k` | Next / previous session |
| `^g` | Jump to next session that is waiting |
| `^b` | Toggle sidebar |
| `tab` | Move focus sidebar ↔ main pane |
| `enter` | Open selected session |
| `r` | Rename |
| `x` | Kill session (confirm) |
| `w` | New worktree session |
| `/` | Filter |
| `^q` | Quit pebl (sessions keep running) |

## Screens

1. **Default**: projects and sessions, one session open.
2. **Attention**: a session waiting, highlighted.
3. **Collapsed sidebar**: main pane full width, waiting count in status bar.
4. **First launch**: empty state, "add a project" prompt, detects if cwd is a git repo and offers it.
5. **Inline prompts**: new project / new session / confirm kill, rendered inline at the bottom, never as a floating window.

## Sizing

Must look right at 120×40, must remain usable at 80×24 (sidebar auto-collapses below 100 cols).

## CLI

```
pebl                 # open the TUI (attach if already running)
pebl add [path]      # add a project (default: cwd)
pebl new [project]   # start a session
pebl ls              # list sessions with status (scriptable)
pebl hook <event>    # internal: called by Claude Code hooks
pebl doctor          # check tmux, claude, hooks install
```
