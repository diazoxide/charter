# charter-app — one desktop app for running tons of harness sessions in parallel

**Status:** agreed 2026-09-17 in a grill between the operator and the `steward` persona
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
14. **Modules move over one at a time.** The app first reads plane files directly and hands
    every write and every hook to Python `charter`. Each module that moves to Rust passes a
    **differential test**: the same fixture plane and the same input give the same output and
    the same resulting plane in both implementations.
15. **Security-critical parts move last:** hooks guard, gitpolicy, vaults and secrets. They
    need the differential proof plus an external review before Python stops answering them.
16. **Python charter is frozen.** It gets bug fixes, security fixes, and plane outputs the app
    needs. New feature ideas go to the `charter-app` backlog.

### Engineering

17. **Repository:** a new repo, `charter-app`, which takes over the `charter` name at M4. The
    product is still called charter. This repo stays the Python implementation and the plane.
18. **Standard tooling, enforced in CI:**
    - **Rust:** the stable toolchain, `rustfmt`, `clippy -D warnings`, `cargo-deny` (licences
      and advisories).
    - **TypeScript:** `strict`, ESLint, Prettier.
    - **Dependencies:** Dependabot (built into GitHub, nothing to install).
    - **Tests:** Vitest for UI units, `cargo test` for the core, and scenario tests through
      Tauri's WebdriverIO service.
    - **Releases:** the Tauri updater with signed artifacts.
19. **Tests:**
    - **Main gate:** scenario tests driving the real app against a **fake harness binary** that
      replays recorded output and fires recorded hooks.
    - **Unit tests** stay small.
    - **Mutation testing** runs nightly on the core crate alone, with `cargo-mutants --in-diff`,
      sharded. It never blocks a PR.
20. **Distribution:** one signed installer per OS. It installs the app and puts `charter` on
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
  locks the stack. GPUI is tried only if a limit is missed.
- **M1: daily driver on macOS.** The app replaces the tmux frame for the operator:
  - workspace sidebar and the "needs you" queue
  - chats with the profile and persona picker
  - a worktree per chat
  - read-only panels and the palette
  - reopen on relaunch

  Writes and hooks still go through Python charter.
- **M2: core in Rust.** Plane read/write, workspaces, todos, personas and recall, each with
  differential tests. The `charter` binary answers these commands.
- **M3: security-critical parts.** Hooks guard, gitpolicy, vaults and secrets, with
  differential tests and an external review. After M3 the app's own commands and hooks no
  longer touch Python.
- **M4: public release.** Linux, then Windows (ConPTY, bundled package), signed installers,
  the final PyPI release pointing to the new install, and Python charter retired.

## Open until M0 reports

- The scrollback cap that the idle-session limit is measured at.
- Whether the file or socket for hook events (decision 3) needs anything beyond Tauri's own
  single-instance and IPC plugins.
