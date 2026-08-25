---
version: unreleased
headline: The frame's rules are one colour, and charter picks it
---

The frame's borders came out in two colours — the outer rules one shade, the separator
under them another. Nothing in charter chose either: tmux ships
`pane-active-border-style` as **green** and `pane-border-style` as the terminal default,
and it picks between them **per border cell**. So a single horizontal rule running past
the active pane's corner changed colour halfway along it. Measured on tmux 3.7c in a
100-column frame: `\033[32m` for the 77 cells over the agent session, `\033[39m` for the
22 over the persona strip beside it, in one line.

Charter now sets both, to the same thing: the terminal's own foreground, dimmed — the
same `\033[2m` the status line's own box has always used. No hue is imposed on your
theme; a rule is one shade of whatever your palette already is, and a terminal too old to
honour dim draws both in the plain default, which is still one colour.

**Three more settings came with it, and they matter most in the tmux you already have.**
On charter's private server the borders were tmux's defaults; inside your own server they
are whatever your `.tmux.conf` says, and charter was inheriting all of it. Measured
against a real config:

- `pane-border-indicators arrows` marks the **active** pane's borders with `←`/`↓` and
  leaves its neighbours plain — one rule carrying a glyph the next one does not.
- `pane-border-lines double` (or `heavy`, or `number`) redraws every rule in a different
  weight; `number` writes pane numbers into them.
- `pane-border-status top` is the loud one: it turns every border into a title bar
  carrying `#{pane_title}` — your machine's hostname, by default — **and** adds a border
  row above the topmost pane, a row the frame's own height arithmetic never budgeted for.

All five are pinned on charter's own window and nowhere else. They are window options, so
the scope is exactly the window charter opened: a window you already had still resolves to
your own values, and your server's globals are untouched. Verified on a real 3.7c with a
hostile config in place, and pinned by a test that renders the frame through a nested tmux
client and reads the border cells' colours back off the screen.

**One place sets them, for both servers.** The defect was chrome decided in two places
that could never agree; a fix that put charter's private server's answer in its config file
and your server's answer somewhere else would have rebuilt that shape one layer down. The
settings are issued from the single funnel every panel pane charter creates comes out of —
both launch paths, and every density change.

The other candidate cause was checked and is not real: no panel draws its own box. The
only thing in charter that draws one is the status line's frame, and inside a frame the
status line is suppressed outright. There is now a test holding that line — a panel's
output may never be a line enclosed at both edges, which is what a box is and what the
repo table's `├─` tree markers are not.

Nothing to adopt: upgrading is the whole of it, and it applies to frames you launch after
upgrading rather than to one already running.

One neighbour was found on the way and is left for its own fix: sixteen tests fail when
the suite is run from a shell that is itself inside a charter frame, and pass with
`$CHARTER_SESSION_ID` and `$TMUX` cleared. They read the developer's own terminal instead
of stating what they assume about it — the same ambient-environment shape the frame suite
already isolates against in one place and not the others. That is
[#521](https://github.com/diazoxide/charter/issues/521); it predates this work and is
unchanged by it.
