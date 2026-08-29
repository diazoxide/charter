---
version: unreleased
headline: The repo table scrolls, a row can be picked, and the attention bar says what you picked
---

*"I tried to scroll the repo table and nothing happened."*

Nothing was going to. Charter built the whole pointer path earlier in this same release — an
SGR decoder, a dispatcher that translates a click into a component's own cells, a `DELIVERED`
set naming five kinds it can carry — and had **no consumer at all** for it. Every one of the
six components charter draws declared `events = ()`, so the only thing on the machine that
could have received a wheel notch was a component somebody else wrote.

**`repos` declares `scroll` and `click` now**, which makes charter's own panel the worked
example rather than the exception:

```
 ▪ repos 6
   ├─ charter        main                    ✓
   ├─ harness        fix/pane-width   ●⇡2    ✗    !412
   ├─ statusline     main                    ✓                 <- clicked: reversed
   …(+3 more)

 3 todos · ⠙ 1 running · F2 palette · ▪ statusline · main · clean
                                       ^^^^^^^^^^^^^^^^^^^^^^^^^
```

- **The wheel moves a window over the table's own rows.** The table has always shown a
  *ranked* pick when it has more to draw than the pane has room for — the one you are
  standing in, the dirty ones, the ones with a failing pipeline — and the wheel walks down
  that same ranking, then through the worktree rows nested under each clone, which is the
  order the table already spent its budget in. Scroll back to the top and you are on the
  exact table that was there before you touched it.
- **A click selects a row.** The row is drawn in reverse video, and the right-hand end of
  the attention bar reads its state back in words: `▪ statusline · main · dirty · 2 ahead ·
  CI failed · !123`. That is the "tooltip" the table has no columns for, keyed to what you
  picked rather than to where the pointer happens to be — charter asks its terminal for
  press/release reporting and deliberately not for motion, so there is no hover to key it to
  and there will not be one. A click on a worktree row selects its parent clone, including on
  a table scrolled far enough that the clone's own row is off the pane.
- **A click only ever SELECTS.** Nothing charter does from a pointer is irreversible, and
  that is a property rather than caution: a click can reach a pane without its matching
  press — a drag begun on a pane border delivers exactly one release, measured — and a
  gesture that can arrive half-formed is not one to hang an action on. Choosing stays a
  keypress's job.

**The scroll that does nothing is the one worth knowing about, and its condition is counted
in pane rows.** The `repos` pane is sized to its own content, so on most planes it is exactly
tall enough and there is nothing below to scroll to: the wheel moves nothing and repaints
nothing. It starts moving once the table wants **more rows than the pane has room for** —
repo rows and worktree rows together, the same rows the pane draws, and the same condition
that puts `…(+N more)` on screen. That nothing is asserted as a number at the boundary and
one either side, because a wheel that quietly did nothing *because the arithmetic happened to
cancel* is indistinguishable from one that was dropped.

**Rows and not repos, and the difference is worth a paragraph because it very nearly
shipped.** The bound was first computed from the REPO count — the one quantity in that
expression that is not the pane's. The two agree only where no repo has pieces, so on a
many-clones plane the feature worked and on a **one clone with fourteen worktrees** plane it
answered *nothing to scroll* at every pane height there is:

```
 ▪ repos 1
   ├─ charter
   │  ├─ frame-inside-tmux
   │  ├─ guard-all-or-nothing
   │  ├─ opencode-mcp-rule
   │  ├─ op-asks-once
   │  ╰─ reap-live-pid          <- and nine more worktrees, off the pane, unmentioned
```

That shape is the ordinary state of a control plane — including the one this was written on,
which is where it was caught ([#663](https://github.com/diazoxide/charter/issues/663),
reported against [#658](https://github.com/diazoxide/charter/pull/658)). It had been written
down as a stated limit rather than an oversight, with an argument that is sound on its face:
*piece rows appear only in the one-repo shape, where one repo always fits*. What nobody
checked is that the plane in front of us was that shape. **A stated limit that happens to
cover the only configuration you have is not a limit; it is a no-op with documentation.**

Two things a starved one-clone table therefore admits rather than hides. The `…(+N more)`
line is reserved on the same condition every other shape reserves it on, so a table showing
five of fifteen rows says so and says whether the ten it is hiding are clean — counting the
hidden worktrees, not just the hidden clones. And the `⑂14` badge on a clone's row is counted
from the pieces actually drawn rather than from the ones in the cache, so it stops
disappearing exactly when it is most needed. A many-clones table is byte-for-byte what it was
at every offset and every pane height, and a one-clone plane at its natural height draws
exactly what it drew before and still scrolls nothing — because the pieces fit, rather than
because they were never counted.

Verified on a real tmux with a real client on a real pty and charter's own two panels, at
every offset and either side of each boundary.

**You do not need a mouse, and on most planes you do not have one.** `[frame] mouse` ships
`false`, and with it off tmux asks your terminal to report from the *active* pane's request
alone — so with the harness active, a click on a panel is not an event charter drops, it is
an event that never happens. `F2` → `repo: select the next row` (and `previous`) moves the
selection with the arrow keys and Enter you already drive the palette with. Charter will not
bind a bare arrow key to do it: a `bind -n Up` is server-wide in tmux and would take the
arrow before your harness ever saw it.

**To adopt it, set `[frame] mouse = true`** — and read what that costs before you do. From
the moment you attach, your terminal reports the mouse and stops doing its own drag-select
over the window; you get tmux's copy-mode selection instead. There is no state in which
panels are clickable and native selection survives, on any tmux from 3.1c to 3.7c. What it
no longer costs is your keyboard: since charter rebound `MouseDown1Pane` inside its own
server, clicking a panel acts where you pointed and leaves you typing in the harness.

Charter's other three panels declare nothing and are untouched — their panes' terminals are
left in whatever mode they were found in, and they still sleep between repaints rather than
watching their own input.
