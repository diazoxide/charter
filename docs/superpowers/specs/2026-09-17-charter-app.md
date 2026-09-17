# charter-app — one desktop app for running tons of harness sessions in parallel

**Status:** agreed 2026-09-17 in a grill between the operator and the `steward` persona
(workspace `ide`). The decision and its reasons: `docs/adr/0025-charter-is-rebuilt-as-a-desktop-app-on-a-rust-core.md`.
The evidence: `docs/research/2026-09-17-gui-terminal-embedding.md`.

## The goal

A developer runs dozens of harness sessions (Claude Code, Codex) at once, across workspaces
and repos, and always knows which one needs them. It is one lightweight app on macOS, Linux
and Windows, with no lag and no daemon, carrying every charter concept: the plane, workspaces,
personas, todos, memory, vaults, guards.

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

9. **A Rust core owns every PTY and every terminal's state, headless.** The terminal engine
   sits behind a trait: `alacritty_terminal` first, benchmarked against libghostty-vt. The UI
   draws only the panes on screen. This holds whatever draws them, because web UIs are capped
   at 16 WebGL contexts (research §5.1).
10. **The drawing layer is decided by M0's bake-off:** GPUI or Tauri 2 (React + TS), on the same
    core. The fastest option that meets the limits below wins, and a tie goes to GPUI.
    Electron is out on footprint.
11. **Nothing in the core parses harness output to decide anything** (ADR 0018, carried over).

### Migration

12. **The plane format does not change.** `docs/plane-format.md` records every file and field
    the Python charter reads or writes (`charter.toml`, `charter.local.toml`, `personas/`,
    `workspaces/*/workspace.md|json`, memory files, `.charter/` state) and marks each one stable
    or internal. Fixture planes are generated from it, and both implementations test against
    them.
13. **Modules move over one at a time.** The app first reads plane files directly and hands
    every write and every hook to Python `charter`. Each module that moves to Rust passes a
    **differential test**: the same fixture plane and the same input give the same output and
    the same resulting plane in both implementations.
14. **Security-critical parts move last:** hooks guard, gitpolicy, vaults and secrets. They
    need the differential proof plus an external review before Python stops answering them.
15. **Python charter is frozen.** It gets bug fixes, security fixes, and plane outputs the app
    needs. New feature ideas go to the `charter-app` backlog.

### Engineering

16. **Repository:** a new repo, `charter-app`, which takes over the `charter` name at M4. The
    product is still called charter. This repo stays the Python implementation and the plane.
17. **Tests:**
    - **Main gate:** scenario tests driving the real app against a **fake harness binary** that
      replays recorded output and fires recorded hooks.
    - **Unit tests** stay small and cover the core crate.
    - **Mutation testing** runs nightly on the core crate alone, with `cargo-mutants --in-diff`,
      sharded. It never blocks a PR.
18. **Distribution:** one signed installer per OS. It installs the app and puts `charter` on
    PATH. The Claude Code plugin keeps calling `charter hook …`. A final PyPI release points to
    the new install.

## Limits (acceptance)

These are measured with the method in research §9.3, on one machine, in one run:

| | Limit |
| --- | --- |
| Live sessions | 50 |
| Visible-pane throughput | **≥ tmux**, re-measured on the same machine against newly recorded Claude Code output |
| Stall under the 2 MB and 13 MB bursts | no multi-second main-thread stall |
| Keystroke to screen | ≤ 16 ms while 49 other sessions stream |
| Tab switch | ≤ 50 ms |
| Idle hidden session | ≤ 30 MB, with scrollback at the shipped cap |
| Hook call (`charter hook …`) | ≤ 10 ms |
| Cold start | ≤ 1 s |
| Synchronized-output animation (`?2026`) | > 1 fps, continuous (xterm.js#6071 is why) |

## Milestones

Each milestone is something the operator actually uses, not a layer.

- **M0: bake-off.**
  - Re-record the corpus and re-measure tmux.
  - Build the headless session core with the engine trait.
  - Run GPUI and Tauri arms against the limits above; alacritty_terminal against libghostty-vt.
  - Output: the numbers, the stack locked in an ADR, the prototype kept on a `prototype/`
    branch.
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

- The drawing layer (decision 10), and whether React is used at all.
- Which terminal engine ships.
- The scrollback cap that the idle-session limit is measured at.
