---
version: unreleased
headline: the tab strips drop their headings, launch one row deep with F3 to grow them, and every workspace tab says how many chats it holds
---

Four changes to the tab strip, made together because they trade columns against each
other: two free them and one spends them.

```
before
  workspaces   authority-audit   autonomy   charter-update-skill   default   fleet  *harness-wrapper   news-dispatch-guard  +8

after
   authority-audit   autonomy (2)   charter-update-skill   default   fleet  *harness-wrapper (3)   news-dispatch-guard  +8
```

## The headings go

`workspaces` and `chats` were drawn once at the start of each strip. They were mouse
targets for nothing, carried no chrome, and never changed — and a label that never changes
is chrome you stop seeing after a day.

What tells the two strips apart now is where they are and what is on them: they are
adjacent and permanently ordered, the chats strip draws a `+` and a spinner while the
workspaces strip deliberately draws neither, and a chat id ends in an ordinal.

**The saving is 12 columns and 7, not 14 and 9.** The issue attributed the whole lead to
the heading; two of those columns are `slots.INSET`, the left edge every pane's rows start
at, and they are still there. Measured through the ladder itself: the fifteen workspaces on
this repository's own plane needed **274** columns to fit on one row and now need **262**.

## One row on launch, `F3` to grow it

`slots.bar_rows_wanted` composes the strip at 1, 2 … `layout.BAR_MAX_ROWS` and keeps the
tallest height it fills, and `layout._grown` hands those rows out of the harness's spare
budget — so a plane with many names came up two rows deep whether or not its operator
wanted the harness two rows shorter.

Every run now starts at one row. The measured objection to that was a strip drawing
`*harness-wrapper  +14` — one tab, the one you are on, which `switch_to` correctly refuses,
so *"the bar was clickable and reached nothing"*. That objection no longer stands on its
own: **`+N` is clickable and opens the palette**, which lists every name. A collapsed strip
is one press from the complete list, not a dead end.

`F3` cycles a strip 1 → 2 → 3 and back. It **raises a ceiling and does not set a height**:
`bar_rows_wanted` still answers with the rows the names actually fill, so pressing it on a
plane whose names already fit moves nothing — there is nothing to put on a second row.

**The height does not survive a restart.** It is a file in the frame's own state directory
beside the density a palette row chose and the panels a toggle key hid, so charter deletes
it when the frame ends and there is no new kind of state to explain. A plane that always
wants three rows says so once in `[[frame.component]] size` (#687), which is still honoured
as the pinned default.

Two alternatives were weighed. **A tmux drag** is the gesture an operator would reach for
and is recomputed away on the next layout pass, because the layout owns bar heights (both
bars are `Fixed(1)`) — making a drag stick means teaching the layout to tell "the operator
dragged this" from "recompute this". **`charter.toml` alone** already works and is an
edit-and-restart rather than a gesture.

`F3` is the third key charter binds, after `F2` for the palette and `F12` for the escape
hatch. A root-table binding is server-wide and takes the key before your harness sees it,
which is why charter binds no component toggle by default; `F3` is affordable for the same
reason those two are, and a component that commits `key = "F3"` is refused rather than
silently taking it.

## A workspace tab says how many chats it holds

`some-workspace (5)`. A workspace with none draws a blank, so every count you see means
something, and past 99 the field says `(99+)` rather than growing a digit.

**The field is always reserved, whether or not it has a number in it.** Tab widths decide
where the strip cuts its pages, so a suffix that appeared when a chat opened and vanished
when one closed would move every tab to its right — and the cell you were about to press
would hold another workspace's name. charter refused this exact shape once already, for the
tab spinner: *"A spinner drawn beside a name would re-cut the strip the moment a sibling
started thinking."* The spinner had the mark's cell to take; a count has none, so it buys
one on every tab.

That reserve is six columns a tab, which is what item 4 spends of what items 1 and 2 freed:
those fifteen workspaces need **352** columns for one row now.

**One directory scan for the whole strip, not one per workspace.** `chats.of_workspace` is
an `os.scandir` plus two small reads per chat and is uncached by design; asked once per name
it is roughly 15 × 30 × 2 ≈ 900 reads for one repaint of one row. It and the new
`chats.counts_by_workspace` now share one grouped walk, with the same membership rule and
the same order.

The workspaces strip is still not on a clock. A count changes when a chat opens or closes,
which happens to the plane and not to the clock, so it rides the repaint the strip already
had — one `stat`, the same as before.
