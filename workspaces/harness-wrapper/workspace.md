# harness-wrapper

> **Living charter** for this workspace — its north star and shared context.
> Keep it current as the work evolves (edit this file, or `charter workspace vision "…"`).
> It's committed + shared for LIVE workspaces, and a fork inherits it — so anyone
> can pick up the task with full context. Never put secrets here (vault only).

## Vision

A 'charter <harness>' launcher command (e.g. charter claude) that runs the harness as a child process inside a charter-owned TUI frame: harness on top, charter's plane state in a bottom panel. A harness-agnostic alternative to the status line, which only Claude Code supports. Must not affect the existing status-line path — it is a second way to run charter.

## Context & decisions

<!-- Key facts, constraints, and design/architecture decisions found while working —
     the durable "why", not a chronological log. Grow this as you learn. -->

_Nothing yet._

## Glossary

<!-- Task/domain vocabulary so a teammate or a fork isn't lost: `term` — definition. -->

- `Harness.cli_name` — the word an operator types after `charter` to run this harness in
  a frame (e.g. `claude`). Distinct from `Harness.name`, the harness's identity in
  `$CHARTER_HARNESS` (e.g. `claude-code`).
- `Harness.binary` — the executable `launch_argv` execs. Currently equal to `cli_name`
  for every registered harness, but kept as a separate attribute because they are
  allowed to differ.
- `Harness.launch_argv(extra)` — returns the full argv (`[binary, *extra]`) to hand to
  tmux for starting the harness. Always a `list`, never a joined string: tmux does not
  shell-interpret separate argv (pinned against tmux 3.7c), and a joined string would
  reintroduce command injection from operator-supplied arguments. Task 1 of the plan
  (issue #345); a later task reads these off `harness.all()`/`registry.KINDS` to build
  the `charter <harness>` launcher.

## Log

Chronological "what was done" lives in the task memo — `memory/notes.md`
(append with `charter workspace note "…"`).

## Settled (2026-08-22)

Architecture decided by measurement, not preference: **tmux composes the frame, charter
fills the rectangles and owns no terminal emulation.** Two arms were built and both work —
a Textual+pyte app with the harness as a widget renders claude and opencode correctly —
but it pushes 1.85 MB/s against tmux's 25.2 MB/s, and charter would own a VT emulator
forever on a parser last released in 2023, in a project that ships `dependencies = []`.

- Issue: https://github.com/diazoxide/charter/issues/345
- Spec: `docs/superpowers/specs/2026-08-21-harness-wrapper-design.md` (in the clone)
- Plan: `docs/superpowers/plans/2026-08-22-harness-frame.md`, 11 tasks
- Branch: `harness-frame`, worktree under `.worktrees/charter/harness-frame`
- To record: ADR 0018 — charter may run the harness, but never draws it

Grilled to an empty frontier across four rounds: identity (the frame's panes would share
one `TERM_SESSION_ID` under iTerm2 — the `WINDOWID` failure), liveness (a FIFO would hang
the hook path; a version file cannot), exit codes (an attached tmux returns 0 regardless),
and the injection boundary (names never reach a tmux command string; menu items carry
opaque ids).

The status line path is untouched. This is a second way to run charter.
