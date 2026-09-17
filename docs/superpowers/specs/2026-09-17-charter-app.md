# charter-app — one desktop app for running tons of harness sessions in parallel

**Status:** agreed 2026-09-17, amended 2026-09-18 (decisions 14-17 and the milestones: no Python in the app at any milestone) in a grill between the operator and the `steward` persona
(workspace `ide`), and amended the same day when the operator reordered the priorities (below).
The decision and its reasons: `docs/adr/0025-charter-is-rebuilt-as-a-desktop-app-on-a-rust-core.md`.
The evidence: `docs/research/2026-09-17-gui-terminal-embedding.md`.

## The goal

A developer runs dozens of harness sessions (Claude Code, Codex) at once, across workspaces
and repos, and always knows which one needs them. It is one lightweight app on macOS, Linux
and Windows, with no daemon, carrying every charter concept: the plane, workspaces, personas,
todos, memory, vaults, guards.

## Priorities, in order

1. **Development experience.** A change is quick to make, quick to check and pleasant to work on.
2. **Robustness, through standard practice.** Mature, released, widely used tools, the way they
   are meant to be used. Nothing custom where a standard tool exists.
3. **Speed a person can notice.** No lag, no freeze. Speedups nobody can feel never cost 1 or 2.

When two choices conflict, the higher priority wins.

## Why charter is being rebuilt

- **Development is slow.** The tmux frame is 11.5k lines of plumbing, and tests are 3.3× the
  source.
- **Proof is missing.** Nothing tests end to end that a finished change works in the real app.
- **Reach is limited.** POSIX only; the frame does not run on Windows.
- **Scale hurts.** Watching many sessions in tmux panes is impractical.

## Language

- **App**: the desktop GUI. **Core**: the Rust library the app and the CLI share.
  **`charter` binary**: the CLI on PATH, called by hooks, scripts and agents.
- **Session**: one harness process in one PTY, owned by the core. **Chat**: a session as the
  UI shows it: its tab, its workspace, its state.
- **Session state**: `running`, `waiting` (on you), `done`, `failed`, `unknown`. Set only by
  harness hooks, never by reading output.
- **Python charter**: the current implementation, frozen, and the reference for differential
  tests until it is retired.

## Decisions

### Product

1. **One window.**
   - **Left:** a sidebar listing every workspace with its chats, and each chat's live state.
   - **Top:** a global "needs you" queue, plus OS notifications.
   - **Center:** tabs and free split panes.
   - **Right:** panels for the focused workspace: repos, branches, CI, todos, personas.
   - **Palette:** the command palette is the primary input, keyboard first.
2. **No TUI.** In the terminal, charter is the CLI.
3. **Session state comes from hooks only.** A hook calls `charter hook …`, which hands an
   event to the app through a file or socket the app owns. A harness with no such hook shows
   `unknown`.
4. **A worktree per writing chat.** A chat that writes to a repo gets its own git worktree by
   default. The sidebar shows its branch. Merging back is an explicit action.
5. **Lifecycle.** Closing the window hides the app to the tray. Quitting warns if a session is
   mid-turn and then ends every session. On the next launch, chats reopen through each
   harness's own resume.
6. **Harnesses in v1:** Claude Code and Codex. opencode follows.
7. **Remote sessions are not in v1.** Sessions sit behind one interface (spawn, read/write
   bytes, resize, exit) with local PTY as the first implementation, so SSH or devcontainers
   can be added later without a redesign.
8. **Extension points are not in v1.** The tmux frame's component/action seam is redesigned
   for the app later, not carried over.

### Architecture

9. **A Rust core owns every PTY and every terminal's state, headless**, using
   `alacritty_terminal` behind an engine trait. No second engine is built now. This holds
   because web UIs are capped at 16 WebGL contexts (research §5.1).
10. **The UI is Tauri 2 + React + TypeScript, and xterm.js draws only the panes on screen.**
    When a pane becomes visible, the core sends a snapshot of its screen, then streams its
    output. This is the VS Code reconnection pattern, not a hand-written renderer. GPUI is the
    fallback only if the M0 skeleton misses a limit below.
11. **Typed IPC.** Commands and events between core and UI are generated from Rust types
    (`tauri-specta`). No JSON shapes are written by hand on either side.
12. **Nothing in the core parses harness output to decide anything** (ADR 0018, carried over).

### Migration

13. **The plane format does not change.** `docs/plane-format.md` records every file and field
    the Python charter reads or writes (`charter.toml`, `charter.local.toml`, `personas/`,
    `workspaces/*/workspace.md|json`, memory files, `.charter/` state) and marks each one stable
    or internal. Fixture planes are generated from it, and both implementations test against
    them.
14. **The app never calls Python, at any milestone.** Amended 2026-09-18 on the operator's
    instruction: "final app should not use python, fully clean implementation in rust — no need
    to mix languages". The first plan had the app shelling out to Python `charter` for writes
    and hooks until M3. It does not: whatever a feature needs is ported to Rust **before** the
    feature that needs it, so no shipped path ever crosses languages, and no scaffolding is
    written that only exists to be deleted.
15. **Python is the oracle, not a dependency.** Every ported module passes a **differential
    test**: the same fixture plane and the same input give the same output and the same
    resulting plane in both implementations. That runs in CI, where Python is a test fixture —
    it is never in the app, the `charter` binary, or an installer.
16. **Security-critical parts are ported with the most care, not last:** hooks guard,
    gitpolicy, vaults and secrets each need the differential proof plus an external review
    before the Rust one answers for real. Ordering follows what a milestone needs; nothing
    ships on a Python fallback in the meantime.
17. **Python charter is frozen.** It keeps running as today's product until cutover, and gets
    bug fixes and security fixes only. New feature ideas go to the `charter-app` backlog.

### Engineering

18. **Repository:** a new repo, `charter-app`, which takes over the `charter` name at M4. The
    product is still called charter. This repo stays the Python implementation and the plane.
19. **Standard tooling, enforced in CI:**
    - **Rust:** the stable toolchain, `rustfmt`, `clippy -D warnings`, `cargo-deny` (licences
      and advisories).
    - **TypeScript:** `strict`, ESLint, Prettier.
    - **Dependencies:** Dependabot (built into GitHub, nothing to install).
    - **Tests:** Vitest for UI units, `cargo test` for the core, and scenario tests through
      Tauri's WebdriverIO service.
    - **Releases:** the Tauri updater with signed artifacts.
20. **Tests:**
    - **Main gate:** scenario tests driving the real app against a **fake harness binary** that
      replays recorded output and fires recorded hooks.
    - **Unit tests** stay small.
    - **Mutation testing** runs nightly on the core crate alone, with `cargo-mutants --in-diff`,
      sharded. It never blocks a PR.
21. **Distribution:** one signed installer per OS. It installs the app and puts `charter` on
    PATH. The Claude Code plugin keeps calling `charter hook …`. A final PyPI release points to
    the new install.

## Limits (acceptance)

Only what a person would notice. Measured on the operator's machine, in the scenario harness.

| | Limit |
| --- | --- |
| Live sessions | **50**. This is the product's scale, not a speed target |
| 2 MB and 13 MB output bursts | the UI never freezes; input and other panes stay responsive |
| Keystroke to screen | ≤ 50 ms while 49 other sessions stream |
| Tab or pane switch | ≤ 100 ms |
| Hook call (`charter hook …`) | ≤ 50 ms |
| Cold start | ≤ 2 s |
| Idle hidden session | ≤ 50 MB, with scrollback at the shipped cap |
| Synchronized-output animation (`?2026`) | smooth, ≥ 30 fps (xterm.js#6071 is why this is listed) |

tmux's end-to-end throughput is re-measured in M0 on the same machine and recorded as a
**reference**, not a gate.

## Milestones

Each milestone is something the operator actually uses, not a layer.

- **M0: walking skeleton.** Tauri + Rust core + xterm.js, with 50 fake sessions and one
  scenario test green in CI on macOS and Linux. It is measured against the limits above and
  locks the stack. GPUI is tried only if a limit is missed. **Done** — the measurements and
  the lock are ADR 0026, and the benchmark is `node tools/bench.mjs` in charter-app.
- **M1: daily driver on macOS, on Rust alone.** The app replaces the tmux frame for the
  operator, and every part of it is Rust:
  - the plane read *and written* in Rust: workspaces, chats, personas, todos, profiles
  - the workspace sidebar and the "needs you" queue
  - chats with the profile and persona picker
  - a worktree per chat
  - read-only panels and the palette
  - lifecycle: tray, a quit warning mid-turn, reopen on relaunch
  - `charter hook …` answered by the Rust binary, which is also what meets the hook limit
    ADR 0026 measured at 107.6 ms through Python

  Each ported piece carries its differential test against Python.
- **M2: the rest of the CLI.** Every remaining command the plane needs — recall, save, sync,
  clone, doctor, news, report, git-policy — with differential tests, so the Rust `charter` is
  the only one a plane needs.
- **M3: security-critical parts proven.** Hooks guard, gitpolicy, vaults and secrets get their
  external review against the Rust implementation (the pieces themselves land whenever a
  milestone needs them, never on a Python fallback).
- **M4: public release.** Linux, then Windows (ConPTY, bundled package), signed installers,
  the final PyPI release pointing to the new install, and Python charter retired.

## What M0 reported

M0 measured the skeleton against the limits above on the operator's machine and locked the
stack: **ADR 0026**. It answers the first of the questions this section left open.

- **The scrollback cap is 5000 lines**, and a hidden session holding that much at 150 columns
  costs 20.2 MB of the 50 MB the limit allows. Fifty of them add about 1 GB to the app.
- **xterm.js draws with its own DOM renderer.** WebGL was measured beside it: both meet every
  limit, and neither is faster at the same things, so the simpler one wins on priority 1. The
  addon stays behind a switch, because many panes on screen is the one thing it is better at.
- **The hook call is missed**: 107.6 ms through Python charter, against a 1.8 ms start for the
  Rust binary. M3 is where it is met, and it is not the stack's to fix.
- **A `?2026` animation falls to one frame a second** if a writer pauses inside an open update
  (xterm.js#6071), against 52 draws a second when repaints are written whole. The core can
  close it in M1 by never ending a chunk inside an open update, with a deadline of its own.

Still open:

- Whether the file or socket for hook events (decision 3) needs anything beyond Tauri's own
  single-instance and IPC plugins.
