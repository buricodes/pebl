# pebl: Research Notes

Snapshot as of Oct 2026.

## Prior art

### T3 Code (desktop/web, the thing we're not)
Open-source, MIT, by Theo / T3 Tools. Electron desktop app plus web and mobile, wraps Claude Code, Codex, Cursor, OpenCode, Grok and others behind one UI, uses your existing logins, has a built-in git/PR workflow.
**Takeaway:** strong product, but it's a GUI. Our reason to exist is staying in the terminal and showing real Claude Code instead of a re-rendered chat UI.

### claude-squad (closest competitor, Go)
Terminal app managing multiple Claude Code / Codex / Gemini / Aider instances. Uses tmux for isolated sessions and git worktrees so each session gets its own branch. Has a fork, Loom, that has since diverged.
**Takeaway:** proves tmux + worktrees works. It's task/instance-centric; pebl is project-centric with a persistent sidebar and a Claude-Code-native look.

### claude-tmux (Rust/Ratatui)
TUI over tmux that lists Claude Code sessions, detects idle / working / waiting **by scraping pane content**, supports worktrees and PRs, opens as a tmux popup.
**Takeaway:** good reference code for the tmux wrapper. We improve on status accuracy by using hooks instead of scraping.

### ccmux (Python)
TUI with a visual sidebar; red highlight when Claude needs attention; CLI to create/list/attach; worktree support with a per-repo toml for setup steps.
**Takeaway:** the per-repo worktree setup file is a nice idea (copy `.env`, run install). Steal for v1.1.

### stapler-squad (Go, web UI)
Web dashboard with xterm.js streaming, approval rules, review queue, SQLite state.
**Takeaway:** out of scope (web), but its isolated tmux sockets in tests are worth copying.

## Technical findings

### Embedding a terminal in Ratatui
`tui-term` provides a pseudoterminal widget for Ratatui backed by the `vt100` parser and `portable-pty`. Works, but makes pebl a terminal emulator. Chosen: tmux-backed instead (see ARCHITECTURE.md), keep this as a possible future backend.

### Claude Code hooks
Configured in `~/.claude/settings.json`, `.claude/settings.json`, or `.claude/settings.local.json`. Lifecycle: `SessionStart` → per turn `UserPromptSubmit` → `PreToolUse`/`PostToolUse` → `Stop` (or `StopFailure` on API error) → `SessionEnd`. `Notification` fires when Claude Code sends a notification (e.g. needs permission). `Stop` and `UserPromptSubmit` don't take matchers.
**Takeaway:** enough events to drive an exact status machine without parsing the screen.

## Differentiation summary

| | pebl | claude-squad | claude-tmux | T3 Code |
|---|---|---|---|---|
| Terminal-native | ✓ | ✓ | ✓ | ✗ |
| Persistent project sidebar | ✓ | ~ | popup | ✓ |
| Real Claude Code rendering | ✓ | ✓ | ✓ | ✗ |
| Status via hooks | ✓ | ✗ | ✗ (scrape) | n/a |
| Claude Code look | ✓ | ✗ | ✗ | ✗ |
| Multi-agent | later | ✓ | ✗ | ✓ |

## Open questions

- Prefix key: `ctrl+space` vs `ctrl+a`. Must not clash with Claude Code or common tmux configs.
- Should pebl reuse the user's tmux config or ship a minimal isolated one? (Leaning isolated.)
- Notifications when not focused: terminal bell, OSC 9 desktop notification, or both?
