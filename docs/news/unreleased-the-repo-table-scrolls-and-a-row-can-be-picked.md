---
version: unreleased
headline: The repo table scrolls, a row can be picked, and the attention bar says what you picked
---

*"I tried to scroll the repo table and nothing happened."*

Nothing was going to. Charter built the whole pointer path a release ago — an SGR decoder,
a dispatcher that translates a click into a component's own cells, a `DELIVERED` set naming
five kinds it can carry — and shipped it with **no consumer at all**. Every one of the six
components charter draws declared `events = ()`, so the only thing on the machine that could
have received a wheel notch was a component somebody else wrote.

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

- **The wheel moves a window over the list.** The table has always shown a *ranked* pick
  when there are more repos than rows — the one you are standing in, the dirty ones, the
  ones with a failing pipeline — and the wheel now walks down that same ranking. Scroll back
  to the top and you are on the exact table that was there before you touched it.
- **A click selects a row.** The row is drawn in reverse video, and the right-hand end of
  the attention bar reads its state back in words: `▪ statusline · main · dirty · 2 ahead ·
  CI failed · !123`. That is the "tooltip" the table has no columns for, keyed to what you
  picked rather than to where the pointer happens to be — charter asks its terminal for
  press/release reporting and deliberately not for motion, so there is no hover to key it to
  and there will not be one.
- **A click only ever SELECTS.** Nothing charter does from a pointer is irreversible, and
  that is a property rather than caution: a click can reach a pane without its matching
  press — a drag begun on a pane border delivers exactly one release, measured — and a
  gesture that can arrive half-formed is not one to hang an action on. Choosing stays a
  keypress's job.

**The scroll that does nothing is the one worth knowing about.** The `repos` pane is sized
to its own content, so on most planes it is exactly tall enough and there is nothing below
to scroll to: the wheel moves nothing and repaints nothing. It starts moving on a plane with
more clones than the pane has rows — the same plane that already shows `…(+3 more)`. That is
tested as hard as the scrolling is, because a wheel that quietly did nothing *because the
arithmetic happened to cancel* is indistinguishable from one that was dropped.

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
