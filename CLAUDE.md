# CLAUDE.md

Instructions for Claude Code working on this repo.

## Project

pebl is a Rust TUI (Ratatui + crossterm) that orchestrates Claude Code sessions via a private tmux server, with status from Claude Code hooks. Read `docs/SPEC.md` and `docs/ARCHITECTURE.md` (especially "As built") before larger changes. The visual source of truth is the maintainer's Claude Design file `Orchestrator TUI.dc.html`, which is not part of this repo; `cargo run -- preview` and the snapshot tests in `src/ui/screen.rs` show every screen as built.

## Commands

```
cargo build
cargo run -- <args>
cargo test                 # includes insta snapshots of every design screen
cargo insta review         # after an intentional UI change
cargo clippy --all-targets -- -D warnings
cargo fmt
cargo run -- preview       # render a sample screen without tmux
```

Manual end-to-end runs without touching your real `~/.pebl`: set `PEBL_HOME`, `PEBL_SOCK` (keep it short, unix socket paths max ~100 chars) and `PEBL_TMUX_SOCKET`.

## Layout

- `src/app.rs` — the state machine (all keys, ctl commands, hook events). Unit-tested against a fake backend.
- `src/daemon.rs` — socket server + main loop; `src/client.rs` — view panes.
- `src/ui/` — pure renderers of `view::View`; `ui/screen.rs` composes them for snapshots.
- `src/backend/tmux.rs` — every tmux call; also generates `~/.pebl/tmux.conf`.
- `src/status/` — hook → status mapping, `pebl hook`, screen heuristic.
- `src/hotkey.rs` — macOS summon agent (Swift source embedded) and `pebl summon`.
- `src/ui/mascot.rs` — sidebar mascots from the design (notification line: art + three message rows, one-line compact form); `src/gallery.rs` is `pebl mascots`.
- `src/search.rs` — background index of `~/.claude/projects/*/*.jsonl` (prompts and replies only, read incrementally); `src/diff.rs` + `src/review.rs` — the review popup; `src/notify.rs` — chimes, macOS and ntfy notifications.
- Split columns: extra Claude panes are joined into the `pebl-ui` window with a one-row `tabs` view pane above them. `App::apply_layout` builds a `backend::Arrangement` and the tmux backend applies it as one layout string (reordering panes first, because tmux assigns layout cells by pane order).

## Rules

- Never edit the user's `~/.claude/settings.json`. Hooks are injected per session with `--settings`.
- All tmux calls go through `backend/tmux.rs` and always use the private socket (`-L pebl`). Never touch the user's default tmux server.
- `pebl hook` must be fast, never print to stdout, and always exit 0. If the socket is unreachable, append to `events.log`.
- No colors outside `ui/theme.rs`. Use theme tokens.
- The UI must render correctly at 80×24. Add an `insta` snapshot test in `ui/screen.rs` for any new screen.
- Tests that touch tmux use a unique temp socket and clean up after themselves.
- No `unwrap()` outside tests; `anyhow` in the binary.
- No code comments of any kind (`//`, `///`, `//!`, `/* */`), including in embedded sources like the Swift agent. CLI help goes in clap `about`/`help` attributes.

## Style

- Look identical to Claude Code / the design: monospace, coral accent, thin borders, no filled backgrounds (except the selected-row tint).
- Keyboard-first. Every action reachable without a mouse.
- Inline prompts (sidebar row or the bar strip), never floating modals.
