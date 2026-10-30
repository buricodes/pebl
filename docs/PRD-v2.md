# Pebble PRD v2: From Orchestrator to Agent Workbench

Status: Draft · Owner: Buri · License: open source, free forever

## 1. Where we are

PRD v1 is fully shipped and in use by early testers. Pebble today has:

- Project sidebar with sessions, mascot, default view
- Split column views (multiple sessions side by side)
- Chime sound when a session needs attention
- Command suggestions + command drawer (pre-filled, resizable)
- Send one prompt to many / marked sessions
- Conversation search
- Usage view
- Git diff with comments + raise PR (diff UI redesign owned by Buri, out of scope here)

## 2. Goal of v2

Move Pebble from "a nice way to run several Claude Codes" to "the place agents get real work done while you're not watching". Three themes:

1. **More agents**: Codex and OpenCode alongside Claude Code
2. **Less babysitting**: Pebble tells you exactly what needs you and checks the work for you
3. **Agents working together**: queues, parallel attempts, planners, reviewers

## 3. Non-goals (v2)

- Phone / remote access (revisit in v3)
- Paid tiers, accounts, telemetry
- Diff view redesign (separate track)
- Reimplementing any agent's UI; each agent always runs as its real CLI

## 4. Phases

| Phase | Theme | Features |
|---|---|---|
| P1 | Providers | Codex + OpenCode, one provider per session |
| P2 | Attention | Jump-to-attention key, attention inbox |
| P3 | Trust the output | Auto-checks, budgets, shared project memory |
| P4 | Parallel work | Task queue, best-of-N |
| P5 | Agents together | Reviewer agent, planner/worker, merge-conflict helper |
| P6 | Automation | Triggers, recipes, replay + checkpoints |
| Later | | Mid-thread provider handoff, remote access |

---

## P1: Multi-provider support

### Scope
Each session runs exactly one provider: Claude Code, Codex, or OpenCode. No switching inside a session in this phase.

### Requirements
- **Provider adapter interface** so adding a provider = one new module:
  ```
  Provider {
    id, displayName, glyph, color
    detect()          -> installed? logged in? version
    launchCommand(cwd, opts) -> argv + env
    statusSource()    -> hooks | events | heuristic
    resumeCommand(sessionRef)
    usage()           -> optional
  }
  ```
- **New session flow** asks for provider (remember last choice per project). Default provider configurable globally and per project.
- **Sidebar** shows a small provider badge per session.
- **Status detection per provider**: use the provider's native hook/event mechanism where one exists, fall back to pane heuristics. Same status set across providers: running, waiting, idle, error, exited.
- **Doctor**: `pebble doctor` reports each provider's install + login state.
- **Features that must work across providers:** broadcast prompt, search, chime, split views, diff/PR. Usage view shows "n/a" where a provider exposes nothing.

### Spike first
- Confirm each provider's status mechanism (Codex and OpenCode event/notify options) and map them to Pebble statuses.
- Confirm resume commands for each.

### Done when
A user can open one project with a Claude Code, a Codex and an OpenCode session side by side, and all three show correct live status.

---

## P2: Attention

### Jump to attention
- One global key (default `^g`) jumps to the next session in `waiting` state, across all projects. Press again for the next one. Ordered by longest waiting first.
- If nothing is waiting, flash the status bar "all clear".

### Attention inbox
- Status bar shows `! 2 waiting`. A key opens a small inline list of everything waiting / errored / finished-since-you-looked, each with project, session, and how long ago.
- Chime stays; add optional terminal bell and OSC desktop notification, configurable per event type.

### Done when
With 6+ sessions across projects, a user never has to scan the sidebar to find who needs them.

---

## P3: Trust the output

### Auto-checks after each turn
- Per project `pebble.toml` defines checks: `test`, `lint`, `typecheck`, any shell command.
- When a session's turn ends (idle), Pebble runs checks in that session's worktree in the background.
- Sidebar shows a check badge: ✓ pass, ✕ fail, … running.
- One key sends the failing output back to the agent as a prompt ("tests failed, fix them").
- Option: auto-send failures up to N times.

### Cost and token budgets
- Budget per session, per task, or per day (tokens or $ where the provider exposes it).
- At 80%: warning badge. At 100%: Pebble pauses (stops sending queued prompts, marks session `budget`). User can raise or override.

### Shared project memory
- One `PEBBLE.md` (or configurable) per project, injected into every new session regardless of provider (via each provider's native memory file or first prompt).
- Quick-add from any session: a key to append a note ("we use pnpm", "don't touch /legacy").
- Shows in the drawer for editing.

---

## P4: Parallel work

### Task queue
- Per project queue of tasks (title + prompt + optional provider + optional checks).
- Pebble starts up to `max_parallel` sessions, each in its own git worktree/branch, and feeds the next task when one finishes and passes checks.
- Queue view: pending, running, needs review, done, failed.
- Add tasks from the drawer, a file (`tasks.md`), or the CLI: `pebble queue add "..."`.

### Best-of-N
- Send one prompt to N sessions (same or different providers/models), each in its own worktree.
- Compare view: diffs side by side, check results, tokens used, time taken.
- Pick a winner → keep its branch, discard the rest (with confirm).
- Builds on existing broadcast + split views.

---

## P5: Agents together

### Reviewer agent
- When a session finishes (and checks pass), optionally spawn a reviewer session (configurable provider) with the diff and the original task.
- Reviewer writes comments into Pebble's existing diff comments.
- One key: "send review to author" feeds comments back to the original session.

### Planner / worker mode
- User gives a big goal to a planner session.
- Planner outputs a structured task list (Pebble defines the format, e.g. JSON block or `tasks.md`).
- User approves/edits the plan, then tasks go into the task queue (P4) as workers.
- Planner gets a summary when workers finish and can add follow-ups.

### Merge-conflict helper
- When merging worktree branches conflicts, Pebble opens a session with the conflict context and asks the agent to resolve.
- Result shown as a diff for approval. Never auto-commits.

---

## P6: Automation

### Triggers
- Start a task automatically from:
  - Failing CI run (GitHub Actions via `gh`)
  - New GitHub issue with a label (e.g. `pebble`)
  - Cron schedule ("every morning: update deps, run tests")
- Triggered tasks go into the queue, never run unbounded. Budgets apply.

### Recipes
- Saved multi-step workflows as plain files, e.g.:
  ```toml
  name = "fix-issue"
  steps = [
    "read issue {{issue}} and plan the fix",
    "implement it",
    "run checks",
    "review",
    "open PR",
  ]
  ```
- Run from drawer or CLI. Shareable: drop a file in `.pebble/recipes/` or install from a URL.

### Session replay + checkpoints
- Timeline per session: prompts, tool calls, file changes, check results.
- Checkpoint = git snapshot at each turn end. Roll back to any checkpoint (restores files; conversation rollback where the provider supports resume).
- Scrub through the timeline to see what the agent did while you were away.

---

## Later

### Mid-thread provider handoff
True switching isn't possible (each provider owns its own history format). Plan: **handoff** = summarize current thread + current diff + PEBBLE.md, start a new session with another provider seeded with that context, link the two sessions in the sidebar. Revisit after P1 ships and providers are well understood.

### Remote access
Approve prompts and peek at sessions from a phone. Deferred.

## 5. Success signals

- Testers run 3+ sessions at once as a habit
- Time from "session starts waiting" to "user responds" drops (measure locally, opt-in only)
- At least one tester uses the queue or best-of-N weekly
- Outside contributors add a provider or recipe

## 6. Open questions

- Config format: one `pebble.toml` per project for checks, budgets, memory, recipes, or split files?
- Where do worktrees live: inside repo (`.pebble/worktrees`) or global (`~/.pebble/worktrees`)?
- Reviewer default: same provider as author, or always a different one for a second opinion?
- How to expose budgets when a provider gives no token data?
