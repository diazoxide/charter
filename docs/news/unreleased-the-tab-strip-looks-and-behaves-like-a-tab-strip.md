---
version: unreleased
headline: The tab bars look like tabs, the +N opens the palette, and rules put a seam between them
---

*"now we are showing plain text, no colors, no active tab/session color, `+N` in tabs —
but user can't click and see other workspaces, we don't have any separator borders. All
together we need to have one intuitive and harmonic design."*

Three of those, and the `+N` is the one with real evidence behind it: **the operator
clicked it.** That is the strongest thing anyone can know about what a field looks like it
does, and what it did was nothing at all.

## The tab you are on is drawn as one

```
  workspaces   authority-audit   autonomy   default   fleet  ██*harness-wrapper██   showcase  +9
```

Reverse video — your own terminal's foreground and background exchanged — and not a colour
charter picked. That is the same decision the sidebar's active persona row and the repo
table's selected row already run on, and it is why the block is right on a Solarized
palette, on a light theme, and on a client tmux has downsampled to sixteen colours. It is
also unrelated to `[frame] chrome`, which paints a pane's *background* through a tmux
option and has nothing to say about what a renderer writes into its own row — so the
shipped `chrome = "off"` still gets the block.

**The `*` stays.** With `NO_COLOR` set, on a Linux console, or with a panel's output
redirected to a file, every escape charter writes is stripped — so a strip whose only
answer to *which tab am I on* was the highlight would have no answer at all there. The
block goes on top of the mark, never instead of it.

The paint costs the row no column, which is why it could be added to a strip that is
already short of them.

## `+N` opens the palette

```
  workspaces  +5   fleet  ██*harness-wrapper██   news-dispatch-guard  +8
              ^^^                                                     ^^
```

Both counts, and the `n/N` that a very narrow bar draws instead of names — the rung where
the strip reaches nothing at all and the hand-off is worth the most.

**Not a page turn, and the reason is the one that made the pages safe to click.** The bar
cuts a long list into pages decided by the names and the width alone, never by history: no
remembered window means a tab cannot slide out from under your pointer between two presses,
which is what stops a double-click switching twice. A `+N` that paged would need exactly
that memory, and a panel does not survive a respawn, a density change re-splits the panes,
and two frames on one plane at one width would then disagree about where a tab is.

So the count hands off to the mechanism instead. The bar has always been a readout; `F2`
has always been what reaches every chat and every workspace at every width. Pressing the
count is that sentence made clickable at exactly the point where the readout ran out of
room.

**One honest cost.** It opens the top-level palette, not the picker for that bar's noun —
`charter frame-palette` has no way to be told which picker to land in yet. What you get is
the doorway row for your noun with the name you are on already in it, one keystroke from
the list. One keystroke more than it should be; infinitely fewer than a field that does
nothing.

Everything else on the row still does nothing, and that is the half a feature like this
gets wrong: the heading, the gap between two names, the space past the last tab, and the
tab you are already on.

## `rules = "visible"` puts a seam between tabs

```
  workspaces   authority-audit |  autonomy |  default |  fleet | ██*harness-wrapper██ | +9
```

The key that already decides whether you see the rule between two panes now decides whether
you see one between two tabs. At the shipped `hidden` the row is **byte for byte what it
was** — this costs the default plane nothing — and at `visible` it spends one column per
seam out of the names it can draw, which the ladder pays for by dropping a whole name
exactly as it does for a narrower pane.

**It is an ASCII `|` and not the `│` an IDE would draw, and that is not an aesthetic
choice.** The box-drawing vertical is East-Asian *Ambiguous*: charter measures it as one
cell and a terminal may draw it as two. The repo table draws box glyphs happily, because a
click there is resolved by *row* — a wide glyph makes a row ragged and moves no row index.
A tab bar's clicks are resolved by *column*. One separator a cell wider than it was measured
shifts every tab to its right by one; ten shift the tenth by ten, and you press `fleet` and
land on `default`. Charter's rule for pointer affordances is that they may fail to fire and
must never fire wrongly, so the strip is held to the one glyph no terminal disagrees about —
asserted over the whole row at every width from 0 to 260, not just on the constant.

## Verification

Real tmux, **3.7c and the 3.2 floor**: a real 120-column client on a real pty, fifteen real
workspace names, the count pressed through the client's own terminal, and a real pane carved
off the harness by a real detached `charter frame-palette` — with the heading beside it still
opening nothing. The click map is re-asserted column by column on a painted row and on a
ruled one, at four widths each.

Every new case was checked against a mutant that should break it: dropping the block,
dropping the separator, spelling the separator `│`, publishing the click map with the old
gap width, moving the trailing count's cells by one, leaving the narrow rung inert, and
removing the handler branch — each turns the suite red.

Nothing to adopt. Both bars are still off unless your plane places them with a
`[[frame.component]]` table, and clicking any of it still needs `[frame] mouse = true`.
