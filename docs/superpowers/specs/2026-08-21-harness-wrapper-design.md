# charter runs the harness: a composed frame around the agent

Charter's status line exists because a control plane has state a developer needs at a
glance — which workspace is active, what the repos are doing, what is still open. It is
also the one surface charter cannot offer everywhere: Claude Code renders a status line,
Codex and opencode do not, and no amount of care in `statusline.py` changes that.

**So charter stops asking the harness for a surface and brings its own.** `charter claude`
starts the harness inside a frame charter composes, with charter's panels on the edges.
The status line path is untouched; this is a second way to run charter, not a replacement.

## The decision that cost the most to make

The obvious reading of "charter's own frame" is that charter becomes the app: one program,
a component tree, the harness as a widget. It was built and measured rather than argued
about, against a second arm where tmux composes and charter fills the rectangles.

Measured on darwin 25.2.0, Python 3.14.4, textual 8.2.8, pyte 0.8.2, tmux 3.7c, a 150x42
frame, one corpus shaped like an agent streaming (coloured text, full repaint every 40
lines):

| | charter is the app (Textual + pyte) | tmux composes |
| --- | --- | --- |
| end-to-end burst | **1.85 MB/s** | **25.2 MB/s** |
| 2 MB log | 1.08 s frozen | 0.08 s |
| 13 MB build log | ~7 s frozen | 0.51 s |
| VT parse alone | 2.4 MB/s, 0.9 with scrollback | ~37 MB/s |
| screen to pixels | 7.2 ms/frame (138 fps ceiling) | n/a, it is C |

Both arms work: the Textual arm rendered claude's trust prompt and opencode's alt-screen
TUI correctly, so this was never a feasibility question. Rendering was never the
bottleneck either — **parsing is**, and a pure-Python VT emulator is the whole cost.

The maintenance side decided it as firmly as the numbers. The spike's terminal widget was
120 lines and did not draw a cursor. Still owed: mouse forwarding, scrollback, bracketed
paste, focus reporting, OSC titles, wide-character widths, and synchronized output —
`?2026`, which Claude Code was measured emitting. All of it permanent, on `pyte`, whose
last release was 2023-11-12, in a project that ships `dependencies = []`.

**tmux composes the rectangles. Charter fills them, and owns no terminal emulation.** The
only thing given up is drawing on top of the harness pane, which is the one rectangle
charter has no reason to draw. Recorded as ADR 0018, *charter may run the harness, but
never draws it*.

Charter still gets a component model everywhere it actually draws: each panel is a charter
process owning its pane, built on `tui.py`'s existing `Node`/`Row`/`Stack`/`Columns`.

## Command surface

```
charter claude [args…]          # one launcher per harness in registry.KINDS
charter codex [args…]
charter opencode [args…]
charter frame -- <cmd> [args…]  # any harness charter has never met
```

Arguments after the name reach the harness verbatim.

- **Not a TTY, no frame.** `charter claude -p "…" | jq` `execvp`s the harness directly.
  A frame around a pipe is wrong, and `exec` preserves the exit code for free.
- **No tmux, name the gap.** `Deficit` already states the rule — inventing a remedy
  "sends somebody off to configure something that does not exist" — so charter states the
  requirement, prints the install command, and offers `--no-frame`.
- **`Harness` gains two members**, `cli_name` and `launch_argv()`. This widens an
  interface whose docstring calls itself "deliberately three members wide", and it is
  deliberate: charter now needs a fourth fact from a harness — how to start it — and
  anywhere else recreates the hardcoded-literal problem `registry.py` exists to prevent.
- **Core commands win a name collision**, and a colliding `cli_name` fails a test rather
  than someone's terminal.

## Process model

```
charter claude
 ├─ resolve harness → argv, probe `tmux -V` (floor 3.2)
 ├─ write .charter/frame/<frame-id>/tmux.conf   (never ~/.tmux.conf)
 └─ exec tmux -L charter -f <conf> …
      ├─ pane: charter panel top
      ├─ pane: <harness>      ← charter never draws or parses this
      ├─ pane: charter panel left / right
      └─ pane: charter panel bottom
```

Inside an existing tmux (`$TMUX` set): the same layout as **a new window in the user's own
server**. No nesting, no second prefix, their config untouched. Only key policy differs —
prefix-scoped bindings there, because tmux bindings are server-wide and charter must not
take a key from the user's other windows.

Concurrent frames share the server and get one session each, named by workspace and pid.
One session *per workspace* was rejected: two terminals silently sharing one agent is the
same class of bug as the identity collision below.

## Identity

`session.terminal()` prefers `TERM_SESSION_ID` over `TMUX_PANE`, and tmux's
`update-environment` does not scrub it — so under iTerm2 or Terminal.app **every pane in
the frame inherits one terminal id**. That is exactly what `WINDOWID` was removed for:
"two sessions in one window, one runs `charter ws use`, and the other silently moved with
it."

The launcher mints a frame id — the workspace name and its own pid, the same pair the
session is named for — and exports it as `CHARTER_SESSION_ID`; panels take the harness
pane's id as an argument and never resolve identity themselves. `_PANE_ID_VARS` is *not*
reordered — that would change behaviour for every existing session to fix a problem only
the frame has.

`CHARTER_WORKSPACE` is deliberately **not** exported. `statusline._active()` prefers it
over the pointer file, so exporting it would make `charter ws use` inside the frame appear
to do nothing.

## Liveness

Panels do not poll charter's state; they wait on a version file and `stat` it every
~200 ms. `sessionstart`, `userpromptsubmit` and `posttooluse` bump it, debounced to at
most once per 250 ms, written with a single `os.replace` so a torn read is impossible.

A FIFO was designed and rejected: opening one for write **blocks until a reader exists**,
which would put a hang inside the hook path — and a hook may cost a session its briefing,
never its turn.

The frame therefore updates because the agent did something, costs a `stat` per panel at
idle, and behaves identically on all three harnesses. This is why the wrapper is better
than a status line rather than merely more portable.

## Slots

`top` is identity — workspace, pin marker, plane version, persona. `bottom` is alerts, the
todo count and the hotkey hint. `left` is repo rows and `right` is todos and memory when
they ship. The split follows the zones `statusline.py` already argues for, so the frame
inherits a layout that has been thought about rather than inventing a second one. Content
comes from the existing renderers — `_repo_rows`, `_persona_chips`, `_todo_count`,
`_alerts` — composed, never rewritten.

Panels repaint fully; a five-row pane is a few hundred cells and diffing would optimise
something already free. No alt-screen in a panel: nothing there scrolls.

## Interaction, and the boundary that keeps it safe

Keyboard focus is pinned to the harness. Typing must always reach the agent; a mis-click
that swallows half a prompt is the worst bug this feature could ship. Interaction is a
single configurable hotkey (`bind -n` only in charter's own server) opening a
`display-menu`, with `display-popup -E` for anything charter must render itself.

**Names never cross into a tmux command string.** `display-menu` and `display-popup -E`
take commands tmux parses and runs, and workspace names, repo names, branch names and
persona names are all read from committed files or `.git/HEAD`. Charter shipped a fix for
this exact shape one release ago — a branch name reaching `gh -F` and making it read a
file, where "checking out a branch from someone else's pull request was enough" — and its
conclusion was that the fix is the mechanism, not the value.

So menu items invoke `charter frame action <opaque-id>`, where the id indexes a table
charter holds in its own state and resolves in-process. The same rule covers the launch
path: the harness is spawned as **separate argv, never a joined string**, with tmux's
actual behaviour pinned by probing the binary the way `codex.py` pinned its config shapes.

This is also an argument for panes over `status-format`: text a panel writes to its own
pane is inert, where a status format would expand `#{}` inside a workspace name.

Mouse is **off by default**. `set -g mouse on` takes over drag-select, and breaking the
user's copy to enable a feature v1 does not ship is a bad trade; it turns on with the
release that makes panels clickable.

## Degradation

Below `min-cols`/`min-rows`, side panels drop, then the top, then the frame falls back to
a bare harness with a one-line note. `statusline.render` already budgets a width and
truncates rather than wrapping; the frame degrades the same way rather than differently.

Scrollback inside the frame is tmux copy-mode, not the terminal's, capped at 2000 lines by
default. The frame raises `history-limit` to 50 000 and binds the wheel to copy-mode, and
the docs name the difference regardless — it is the most likely "the frame broke my
terminal" report.

## Lifecycle

- **Exit code.** `charter claude` must be a transparent substitute for `claude`, so the
  harness's status is re-raised via `remain-on-exit` and `#{pane_dead_status}`.
- **Detach** is allowed and prints how to reattach; an agent surviving a closed lid is a
  feature, and returning silently to a shell with it still running is not.
- **A dead panel** stays visible with its error, respawns with backoff, and gives up after
  3 attempts. A panel must never be able to take the agent down with it.
- **Teardown** removes the frame's own directory; launch reaps directories whose tmux
  session is gone. Never by age — a long-lived frame is what an age heuristic would eat.
- **Version skew** is named, not fixed: panels compare their version against the CLI's and
  show "frame is stale — restart" rather than restarting under the user's agent.

## Config

```toml
[frame]
slots = ["top", "bottom"]
mouse = false
hotkey = "F2"
history-limit = 50000
min-cols = 100
min-rows = 20
```

Plane-level only, with `--slots` and `--no-frame` overrides. No per-workspace override
until something asks for one.

## Layout

| file | job |
| --- | --- |
| `charter/frame/layout.py` | pure: (slots, size, argv) → list of tmux argv. No tmux to test. |
| `charter/frame/tmuxctl.py` | the only module that shells out to tmux; version probing |
| `charter/frame/panel.py` | panel runtime — draw, wait, resize |
| `charter/frame/slots.py` | per-slot renderers composed from statusline parts |
| `charter/commands_frame.py` | CLI wiring, matching the `commands_*.py` convention |

## v1

Launchers plus `charter frame --`, private server, non-TTY bypass, missing-tmux message,
**top and bottom panels**, version-file liveness, and one hotkey opening a menu. Left and
right follow immediately — they are the same `split-window` and one config line, so
shipping them in v1 proves nothing while delaying what can actually go wrong.

Build order, test-first throughout: `layout.py` → `tmuxctl.py` → launcher → `panel.py` →
slots → menu → docs/news/ADR. Layout being a pure function means the frame's whole shape
is under test before a pane is ever created. The end-to-end test skips when tmux is absent
rather than failing, and asserts on probed capability rather than a version string.

Ships with `docs/frame.md` (force-included in `pyproject.toml`, which is not optional —
`tests/test_docs_show.py` fails otherwise) and a news entry whose `check:` probe is
read-only, uses charter's own argv, and cannot hang.

## Not pinned yet

Two facts are assumed and must be probed before the code depends on them, because this
repo's rule is that only a rejection is evidence:

1. Whether `tmux new-session` given separate argv ever hands the command to a shell.
2. Whether `remain-on-exit` plus `#{pane_dead_status}` reliably carries the exit code out
   through an attached client. Neither could be tested in a sandbox without a terminal.

If either fails, the harness gets spawned through a small charter shim that owns both
answers itself.
