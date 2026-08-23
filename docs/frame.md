# The frame

`charter claude` runs the harness inside a frame charter composes: the harness in the
middle, charter's own panels on the edges. It works on every harness, which is the point —
a status line is Claude Code's own surface, and Codex and opencode have none at all.

    charter claude               # or codex, opencode
    charter frame -- <cmd>       # anything charter has never met
    charter claude --no-frame    # bare, no frame at all

`claude`, `codex` and `opencode` are top-level commands (`charter claude`), never nested
under `frame` (`charter frame claude`) — `charter frame` is its own, separate escape hatch
for a command charter has no launcher for at all. That is not just naming: once `charter
frame` is on the command line, everything after it is grafted onto the harness's own argv
verbatim, before charter's own argument parser ever sees it, so `frame claude -p hi` would
hand `claude -p hi` to the `frame --` mechanism rather than route anywhere. The same reason
keeps `frame-menu`, `frame-action` and `frame-probe` — one command tmux's hotkey calls
back into, one every menu action calls back into, and the read-only probe — as top-level
names rather than nested under `frame` too.

## What it needs

tmux composes the rectangles and does every part of terminal emulation; charter fills the
edges and never draws or parses the harness's own pane (ADR 0018). Only tmux being
*missing* stops a launch. tmux 3.2 is the version charter has checked its own
requirements against — the hotkey's `display-menu`, and the pane-scoped hooks that carry
the harness's exit code back out. Below 3.2 `charter <harness>` still starts and nothing
is switched off: the hotkey stays bound but may open nothing, and if the exit-code hooks
fail to install charter says so and declines to attach rather than risk a session nothing
can end. The resize-recovery hook needs a further 3.3; below that a resize still works,
panels can just drift out of their fixed size until the next one.

`charter <harness> --probe` (or the standalone `charter frame-probe`) answers "can a frame
run here, and what will it not be able to do" without starting anything: exit 0 if a frame
can run, non-zero if tmux is missing entirely, plus a line for each standing limit — a
tmux below 3.2, and any `[frame] slots` entry charter sizes but has no renderer for.
`charter doctor` carries the same facts as its own `frame` row.

Those two limits are deliberately **not** printed when a frame launches. A warning
written to your terminal microseconds before tmux switches to the alternate screen is not
readable — measured at 86 bytes ahead of the switch — and it comes back into view only
once the frame exits. Both are standing properties of this machine and this plane rather
than news about one launch, so they live on the two surfaces you can ask on demand.

## What changes inside the frame

**Scrollback is tmux's own copy-mode, not your terminal's — the difference people notice
first.** The frame raises tmux's `history-limit` (50 000 lines, configurable) and binds the
mouse wheel to enter copy-mode, but it is tmux's buffer you are scrolling, not the
terminal emulator's.

**Mouse is off by default.** `set -g mouse on` takes over drag-select, so turning it on
trades your terminal's own text-selection for clickable panels — a trade this release does
not make for you. `[frame] mouse = true` opts in.

**Panels repaint when charter's own hooks say something changed**, not by reaching out for
anything themselves: every `posttooluse*` hook bumps a version marker charter already
writes, and each panel notices the bump with a cheap local check (a `stat`, a few times a
second) — never a poll of anything outside the plane, and never work while nothing has
changed.

**Panels measure their own pane**, never `$COLUMNS` — a tmux pane inherits the *launching*
shell's environment whole, so trusting `$COLUMNS` there would lay a panel out at the outer
terminal's width and wrap inside its own much narrower one.

Charter never touches `~/.tmux.conf` — the frame's settings go into a private server of
charter's own (`tmux -L charter`), one server shared by every frame on the machine, with
each frame a session on it.

**Run from inside an existing tmux session, the frame nests.** charter starts its own
server regardless, so you end up with two tmux layers and two prefix keys — charter's
frame does not currently read `$TMUX` or open a window in your own server instead. That
non-nesting path is not built. If you already live in tmux, `charter <harness> --no-frame`
is the honest answer for now.

## Exit codes

The launcher does not `exec` tmux — an attached `tmux new-session` reports 0 regardless of
what actually ran inside it (measured against tmux 3.7c), so a frame's exit code would
otherwise always read as success. Instead charter waits for the session, then reads back
the harness's real status and exits with that. `--no-frame` and the automatic bypass when
stdout is not a terminal both skip the frame entirely and `exec` straight into the harness,
so a pipe (`charter claude -p … | jq`) carries the real exit code with no help needed.

## When the terminal is too small

Below `[frame]`'s `min-cols`/`min-rows`, the side panels (`left`/`right`) are the first to
drop — any shortage costs them, since neither can spare its own divider. A further shortage
in rows drops `top` too. Below half of either floor, every panel drops and the harness
simply gets the whole terminal, the same choice `charter`'s own status line makes when it
runs out of width. `left`/`right` are accepted in configuration and sized, but nothing
renders in them yet — asking for one leaves the harness pane holding that space instead of
a dead, unwritten-to pane, and `charter frame-probe`/`charter doctor` name it.

## Configuring it

```toml
[frame]
slots = ["top", "bottom"]
mouse = false
hotkey = "F2"
history-limit = 50000
min-cols = 100
min-rows = 20
```

`hotkey` is checked against the shape of a tmux key name — optional `C-`/`M-`/`S-`
modifiers and then a key (`F2`, `Up`, `PPage`, `a`, `/`). Anything else falls back to
`F2`, the same way every other key in `[frame]` falls back to its default when charter
cannot make sense of it. That check is not cosmetic: this value is interpolated into tmux
configuration that `source-file` *executes*, and `charter.toml` is a committed, shared
file that arrives from someone else's machine.

`slots`/`mouse`/`hotkey` are spelled the same on both sides. `history-limit`, `min-cols`
and `min-rows` are the three that are not: charter.toml spells them with a hyphen: the
resolved settings charter's own code reads back use an underscore
(`config.FRAME["history_limit"]`) instead. A key typed the underscored way in charter.toml
is silently not recognized — the hyphenated spelling above is the one that is read.

The hotkey (`F2` by default) opens a small menu on whichever frame you are attached to —
today, a single "Detach" entry. However you detach — that entry, or tmux's own prefix key
— charter notices the session is still running and prints how to get back in
(`tmux -L charter attach -t <frame-id>`) rather than leaving you to remember the flags.
