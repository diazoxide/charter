# Charter is rebuilt as a desktop app on a Rust core

The tmux frame was supposed to be how charter scales to many harness sessions. Six weeks in, it
is what stops charter from scaling. The operator named the pain in a grill on 2026-09-17:
development has become very slow, the tests and the mutation sweep are very slow, the frame
has no room to bend, and nothing proves end to end that finished work actually works. The
measurements behind that:

| | |
| --- | --- |
| Source | 67k lines of stdlib Python, first commit 2026-08-06 |
| Tests | 222k lines, 13.6k test functions in 510 files, **3.3× the source**. 134 files touch tmux |
| Frame | `charter/commands_frame.py` alone is 11.5k lines, with about 950 references to tmux |
| Suite | about four minutes at 6k tests (`tools/sweep.py`). The suite has more than doubled since |
| Mutation | a full deletion sweep: "four and a half hours" (`tools/sweep.py`) |
| Hooks | a Python start of 50–130 ms on every `pretooluse` |
| Reach | POSIX only (`pyproject.toml`). tmux does not run natively on Windows |

What settled the grill is below. The spec it produced, with the milestones and the limits the
app is held to, is `docs/superpowers/specs/2026-09-17-charter-app.md`. The evidence for the
stack is `docs/research/2026-09-17-gui-terminal-embedding.md`.

## The decision

**Charter is rebuilt from scratch as one cross-platform desktop app plus one `charter` binary,
both on a Rust core.** The concepts carry over word for word: the plane, workspaces, personas,
todos, memory, vaults, guards, and every ADR that is not about tmux. The Python code does not.

- **The plane on disk does not change.** The new charter reads and writes the plane the Python
  one does. The format is written down as a spec with fixture planes before any module is
  ported, so "the same plane" is something tested rather than hoped for.
- **Modules move over one at a time, and Python is the reference.** The app ships first as a
  GUI plus session manager. It reads plane files directly and hands every write and every hook
  to the Python `charter`. Each module that moves to Rust passes a differential test: the same
  plane and the same input give the same result in both. The security-critical parts (hooks
  guard, gitpolicy, vaults) move last, with the Python behaviour as the proof they did not
  regress.
- **There is no terminal frontend.** The terminal story is the `charter` CLI, for hooks,
  scripts and agents. A TUI would be a terminal emulator inside a terminal again, which is
  the problem ADR 0018 measured, and it would be a second UI to keep in step.
- **No daemons.** Every session is a child of the app. Quitting ends them all, the way PTYs
  work on every OS (SIGHUP on POSIX, `CTRL_CLOSE_EVENT` under ConPTY). Closing the window
  hides the app to the tray. When the app starts again, it reopens chats through each
  harness's own resume, not through reattach.
- **The Python repo is frozen from this decision on.** It gets bug fixes, security fixes, and
  the plane outputs the new app needs during migration. Every new feature idea goes to the new
  app's backlog.

## Why a rewrite, and what it will not fix

A rewrite fixes what the language and tmux cause: the frame's plumbing and the 134 fragile
test files it brings, the missing Windows support, the Python start paid on every hook, and
managing dozens of PTYs from a Python process.

It does **not** fix what comes from how charter has been run. Tests at 3.3× the source and a
mutation run on every change would be *slower* in Rust, because cargo-mutants does an
incremental build plus the full test suite for every mutant and cannot pick which tests to run
per mutant (research note §8). So the testing approach changes with the rewrite, and that
change is part of this decision, not a detail left to it:

- **The main gate is scenario tests driving the real app**: open a workspace, start a chat, a
  hook fires, a panel updates, quit, reopen. They run against a fake harness binary that
  replays recorded Claude Code output, so they are fast and deterministic.
- **Unit tests stay small** and cover the core crate.
- **Mutation testing is scoped and nightly**: the core crate on its own, `--in-diff`, sharded.
  It no longer blocks every PR.

Rewriting only the frontend was weighed and rejected. A desktop GUI over the Python core
through a child process would be cheaper, and it would keep the start cost on every hook, the
test weight and the POSIX-only reach.

## Why a Rust core that owns every terminal

The research ruled out the obvious web shape before any drawing layer was chosen. WebKit, the
webview Tauri and Wails use on macOS and Linux, hard-caps a page at 16 WebGL contexts, and
Chromium does the same by default. Past the cap, the oldest context is lost. A UI that keeps a
live xterm.js per session cannot hold 50 sessions. xterm.js is also only at tmux's speed, not
above it: its maintainers measured 28–33 MB/s with no rendering.

So, whatever draws the screen, **the Rust core owns every PTY and every terminal's state,
headless**, behind one trait. `alacritty_terminal` (Apache-2.0) is the first engine, and
libghostty-vt, which measures far higher but whose API "is definitely going to change", is
the challenger it is benchmarked against. The UI draws only the panes on screen.

One compiled binary also puts hook calls near zero and covers PTYs on all three OSes, ConPTY
included.

## What decides the drawing layer

Two candidates remain, and **a measurement decides between them, not an argument**. That is
the same discipline ADR 0018 used:

- **GPUI (native, GPU-drawn):** one process, no context cap, the smallest footprint, and
  deterministic in-process tests. It is pre-1.0 and pinned to Zed's git. Zed's own terminal
  crate is GPL-3.0, so charter writes its own terminal view.
- **Tauri 2:** the webview draws the visible panes from grid updates the core sends, and the
  rest of the UI is React and TypeScript. It is quicker to build a modern UI this way, but
  there is the 16-context cap, WebKitGTK risk on Linux, and the terminal renderer is custom
  anyway.

Milestone M0 builds both on the same Rust core, which is kept whichever option wins. The
fastest option that meets the spec's limits wins, and a tie goes to GPUI. Electron was
rejected before the bake-off: its 120–160 MB runtime breaks the minimal-footprint requirement.

## What this replaces

For the new app, this supersedes **ADR 0018** (charter may run the harness but never draws
it), **ADR 0019** (the frame owns the surface) and **ADR 0023** (one tmux server per plane).
Their reasons stay true for the tmux frame, which keeps them until it is retired.

ADR 0018's core refusal carries over unchanged: **charter never parses a harness's output to
decide anything.** It draws the terminal, and it learns a session's state (running, waiting
for you, done) only from the harness's hooks. A harness with no such hook shows "unknown",
labelled as unknown, not guessed.
