# pebl: Architecture

## Stack

- **Language:** Rust
- **TUI:** Ratatui + crossterm
- **Session backend:** tmux (one hidden tmux server/socket owned by pebl)
- **Status:** Claude Code hooks → `pebl hook` → local Unix socket → TUI
- **State:** JSON (or SQLite later) in `~/.pebl/`

Why Rust/Ratatui: single static binary, fast redraw, mature widget ecosystem, and the closest prior art (claude-tmux) uses the same stack. Go + Bubble Tea is a fine alternative if preferred; nothing below depends on Rust specifically.

## Process model

Two options were considered.

### Option A: embedded PTY (rejected for v1)

pebl spawns `claude` in a PTY (`portable-pty`), parses output with a `vt100` parser, and renders it inside a Ratatui widget (`tui-term`).

- Pro: one process, total control over layout.
- Con: pebl becomes a terminal emulator. Claude Code is itself a rich Ink TUI, and any gap in the vt100 parser (colors, cursor, alt-screen, wide chars, bracketed paste, mouse) shows up as visual bugs. Sessions die when pebl dies.

### Option B: tmux-backed (chosen)

pebl runs its own tmux server on a private socket (`tmux -L pebl`). Layout:

```
tmux session "pebl-ui"
├── left pane:  pebl sidebar (Ratatui app)
└── right pane: the active Claude Code pane (swapped in/out)

tmux sessions "pebl-s-<id>" (detached, one per Claude session)
└── pane running: claude
```

Switching sessions = `tmux swap-pane` / `join-pane` the target session's pane into the right slot. The user's terminal renders Claude Code natively, so fidelity is perfect.

- Pro: real Claude Code rendering, sessions survive pebl and terminal restarts, detach/reattach for free, battle-tested.
- Con: hard dependency on tmux; must hide tmux's own chrome (status line off, custom border style) so it still feels like one app.

Keep the backend behind a trait so Option A can be added later:

```rust
trait SessionBackend {
    fn spawn(&self, spec: &SessionSpec) -> Result<SessionId>;
    fn focus(&self, id: &SessionId) -> Result<()>;
    fn kill(&self, id: &SessionId) -> Result<()>;
    fn list(&self) -> Result<Vec<BackendSession>>;
}
```

## Status detection

Primary: **Claude Code hooks**. Fallback: pane-content heuristics (only when hooks are missing).

When pebl launches a session it sets `PEBL_SESSION_ID=<id>` and `PEBL_SOCK=~/.pebl/pebl.sock` in the environment and starts `claude` with a pebl-provided settings file that registers hooks:

| Hook event | → pebl status |
|---|---|
| `SessionStart` | idle |
| `UserPromptSubmit` | running |
| `PreToolUse` | running (clears a resolved permission wait) |
| `Notification` | waiting |
| `Stop` | idle |
| `StopFailure` | error |
| `SessionEnd` | exited |

Each hook runs `pebl hook <event>`, which reads the hook JSON from stdin, attaches `PEBL_SESSION_ID`, writes one line to the Unix socket, and exits 0 immediately. It must never block or print to stdout (it must not affect Claude).

If the socket is down (TUI not open), `pebl hook` appends to `~/.pebl/events.log`; the TUI replays it on start.

Process exit is also watched via tmux (`pane_dead`) so a crashed `claude` still shows as exited even if no hook fired.

**To verify early (spike before building UI):**
1. Hooks inherit the env vars set on the `claude` process.
2. Best way to inject hooks per session without touching the user's `~/.claude/settings.json` (e.g. a `--settings` file flag, or a project `.claude/settings.local.json`). Never silently edit user settings.
3. Exact `Notification` payloads for permission prompts vs idle reminders, so "waiting" is accurate.

## Data model

```
~/.pebl/
├── config.toml      # keybindings, theme overrides, prefix key
├── state.json       # projects + sessions
├── pebl.sock       # status socket
├── events.log       # buffered hook events
└── worktrees/       # optional worktree sessions
```

```json
{
  "projects": [
    { "id": "p1", "name": "falsch-parker", "path": "/Users/buri/code/falsch-parker", "color": "coral" }
  ],
  "sessions": [
    { "id": "s1", "project": "p1", "name": "api-refactor", "tmux": "pebl-s-s1",
      "cwd": "/Users/buri/code/falsch-parker", "worktree": null,
      "status": "running", "updated_at": "2026-10-29T10:00:00Z" }
  ]
}
```

On start, pebl reconciles `state.json` against live tmux sessions: missing tmux session → mark exited; orphan `pebl-s-*` session → adopt it.

## Theme

Defined once in `theme.rs`, all colors as truecolor with a 256-color fallback.

| Token | Use |
|---|---|
| `bg` | terminal default (don't paint a background) |
| `fg` | primary text |
| `muted` | secondary text, hints, borders |
| `accent` | Claude coral: selection, focus, running |
| `warn` | waiting |
| `error` | error |

Borders: thin box-drawing only. Selection = accent foreground + `▌` gutter, not a filled bar.

## Module layout

```
src/
├── main.rs          # CLI entry (clap)
├── app.rs           # TUI state + event loop
├── ui/              # sidebar, statusbar, prompts, theme
├── backend/
│   ├── mod.rs       # SessionBackend trait
│   └── tmux.rs      # tmux implementation
├── status/
│   ├── hook.rs      # `pebl hook` subcommand
│   ├── socket.rs    # Unix socket listener
│   └── heuristic.rs # fallback pane scraping
├── store.rs         # state.json load/save/reconcile
└── git.rs           # worktree helpers
```

## As built (v0.1)

What the implementation settled on where the plan above left options open.

### Processes

```
tmux -L pebl
├── session pebl-bg      window "daemon": `pebl daemon` (state, socket, tmux driver)
│                        + parked panes (collapsed sidebar, idle stage)
├── session pebl-ui      the window you see
│   ├── status line      header row (rendered by tmux, so it spans the border)
│   ├── %sidebar         `pebl view sidebar`
│   ├── %main            the stage (`pebl view stage`) or a Claude Code pane
│   └── %bar             `pebl view bar` (1 row; grows for the new-project prompt)
└── sessions pebl-s-<id> one Claude Code pane each while not on screen
```

- **The daemon owns everything.** View panes are dumb: they subscribe over
  `~/.pebl/pebl.sock`, get the whole `View` as a JSON line on every change,
  render their slice, and send keys/clicks back. They reconnect if the daemon
  restarts, and each runs in a `while :; do pebl view …; done` loop. The daemon
  recreates any pebl pane that disappears.
- **Global keys** are tmux root-table bindings (`bind -n C-j run-shell "pebl ctl next"`),
  so they work no matter which pane has focus. Keys are generated into
  `~/.pebl/tmux.conf` from `~/.pebl/config.toml` on every launch.
- **Switching** is `swap-pane` between the main slot and the target pane, tracked
  by pane id (`@pebl_sid` / `@pebl_role` pane options make it recoverable after a
  daemon restart). Parked session windows are kept at the main slot's size so
  Claude Code doesn't reflow on every switch.
- **Header** is tmux's status line (`status-position top`) with a format string
  produced from the same segments the snapshot tests render.
- **Exited sessions** keep their pane (`remain-on-exit on`); the stage shows
  "enter to resume", which runs `claude --resume <id>` (or a fresh
  `--session-id` if no transcript was written yet).

### Status

Hooks are injected with `claude --settings ~/.pebl/claude-settings.json`
(verified: hooks from `--settings` fire and inherit `PEBL_SESSION_ID`). Events:
`SessionStart, UserPromptSubmit, PreToolUse, PostToolUse, PermissionRequest,
Notification, Stop, StopFailure, SessionEnd`. `Notification` only means
"waiting" for `permission_prompt` / `elicitation_dialog` (or a message about
permission), never for the idle reminder.

Fallback: a session that has said "running" or "waiting" for 10s+ gets its
screen checked every 3s; two agreeing checks that disagree with the hook status
win (covers esc-interrupted turns, which fire no `Stop`).

### Summon hotkey (macOS)

`pebl hotkey install` compiles a ~100-line Swift agent (`~/.pebl/bin/pebl-hotkey`),
registers it as a LaunchAgent, and it runs `pebl summon` on a double-tap of the
chosen key. It's a listen-only event tap (needs Input Monitoring permission; it
can't block or change keystrokes). `pebl summon` focuses the Terminal/iTerm tab
already attached to pebl, or opens a new window running `pebl`.
