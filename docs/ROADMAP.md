# pebl: Roadmap

## M0: Spikes (1–2 days)
- [x] Private tmux socket, spawn `claude` in a detached session, swap it into a split
- [x] Inject hooks per session without editing `~/.claude/settings.json`
- [x] Confirm hooks see `PEBL_SESSION_ID`; log every event payload to a file
- [x] Map real `Notification` payloads (permission vs idle)

## M1: MVP
- [x] `pebl` opens the two-pane layout, tmux chrome hidden
- [x] Add project (cwd or path), new session, switch, kill
- [x] Sidebar with live status from hooks
- [x] `state.json` persistence + reconcile on start
- [x] Status bar with shortcuts and waiting count
- [x] Claude Code theme
- [x] `pebl doctor`

## M2: Daily driver
- [x] `^g` jump to next waiting session
- [x] Collapsible sidebar, auto-collapse under 100 cols
- [x] Rename, filter
- [x] Reorder projects (`J` / `K`)
- [x] Worktree sessions (`w`)
- [x] Desktop notification / bell when a background session starts waiting
- [x] `config.toml` keybinding overrides

## M3: Polish
- [x] Per-repo `pebl.toml` worktree setup steps (copy `.env`, install deps)
- [x] Session resume via `claude --resume` on exited sessions
- [x] Heuristic status fallback when hooks are missing
- [x] `cargo install --git`, `pebl update`, release notices
- [ ] Homebrew tap and prebuilt binaries

## Extras (not in the original plan)
- [x] Global summon hotkey (`pebl hotkey install`)
- [x] `pebl preview` renders the design screens without starting anything

## M4: Orchestrating (from the Oct 2026 research)
- [x] Mascot notification line (needs you › error › finished › working › idle)
- [x] Split view, up to 4 columns, resize and drag
- [x] Peek and quick reply to waiting sessions
- [x] Review: diff popup, line comments sent back to the session
- [x] Worktree finish flow: `pebl.toml` setup, merge or PR via `gh`, cleanup
- [x] macOS / phone notifications when a session needs you
- [x] Context gauge per session, plan usage (5h / week), token totals
- [x] Search past conversations and resume them
- [x] Send one prompt to several sessions
- [x] Shell drawer per session, run Claude's suggested commands, send output back
- [x] Saved prompts with `{{clipboard}}` (ctrl+p)

## Later
- [ ] Agent adapters (Codex, OpenCode)
- [ ] Embedded-PTY backend (no tmux)
- [ ] Linux packaging, WSL

## PRD v2 (docs/PRD-v2.md)
- [ ] P1 Codex and OpenCode providers (on hold: no accounts to test with)
- [x] P2 `ctrl+g` cycles through what needs you, "all clear" when nothing does
- [x] P2 Inbox (`ctrl+y`): waiting, errored and finished sessions across projects
- [x] P2 Terminal bell and OSC 9 notifications, chosen per event
- [x] P3 Auto-checks after each turn (`[checks]` in pebl.toml, `t` / `T`, `auto_fix`)
- [ ] P3 Budgets, shared project memory
- [ ] P4 Task queue, best-of-N
- [ ] P5 Reviewer agent, planner and workers, merge-conflict helper
- [ ] P6 Triggers, recipes, replay and checkpoints
