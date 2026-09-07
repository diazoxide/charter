---
version: unreleased
headline: The frame says how to close a chat, and not only how to make one
---

*"now for example no way to reactivelly close harness session, we have plus button - that
adding new session tab, but no any option to delete/stop, no any modal/drawer for
confirmation."*

Closing a chat has been possible the whole time. `F2 → chat: close` does it, and so does a
right-click on a tab, and both draw a confirmation naming the chat and what stopping it
costs. The operator could not find either, and concluded from what was on screen that the
system would not let them do it.

**That conclusion was correct given the evidence.** The frame advertises exactly two
things: `F2 palette` on the attention row, and `+` on the chat strip. Not `F3`, not `F12`,
not the right-click menu, not the border drag, not that a tab is clickable at all — and
nothing anywhere about closing. A strip that draws `+` and nothing else says *making is
offered, unmaking is not*.

## The rule this establishes

**A surface that advertises creating a thing must advertise unmaking it, at the same weight
and in the same place.**

An unadvertised create costs an operator a feature they did not know about. An unadvertised
destroy makes them conclude the system will not let them do it, which is strictly worse: the
first is a gap, the second reads as a limitation of the tool.

## Three things changed

**`chat: new` is a palette row.** This was charter's own rule broken by charter:
*"a component whose only route to a piece of state is a click has no route to it on most
planes — give every pointer affordance a key as well"*. `[frame] mouse` ships **off**, and
the palette had no new-chat row, so on a default plane an operator could not make a chat
from the frame at all and the one glyph the frame advertises could not be pressed. It runs
the same `charter frame-new-chat` the `+` does.

**The chat strip draws a `-` beside its `+`.** It closes nothing on the press — it opens the
menu about the chat you are in, the same one a right-click on that tab opens, whose `chat:
close` row draws the warning and hands the decision to a keypress. A pointer opens the
question; the keyboard answers it. Its value is not that it is faster; it is that a strip
drawing `+` and `-` **says closing exists**, which is the whole failure.

```
   api.1  *api.2  + -
```

One glyph at the end of the row, not a `×` on every tab: a per-tab affordance would cost a
column on every tab and re-cut the strip, so the cell you were about to press would hold
another chat's name a moment later. Measured at every width from 0 to 220 and at one, two
and three rows — the tab column map is byte-identical with the `-` and without it.

It is an ASCII `-`, not `−` (U+2212). Both measure one cell and U+2212 is East-Asian
Neutral; a click on this row is resolved by column, and the glyph to draw is the one whose
width no terminal disagrees about. A mathematical minus is a codepoint a terminal font may
not carry, and a fallback into a wide face moves every field after it.

**The confirmation is a drawer.** It used to take the whole window — the overlay pane is
zoomed, so a two-row question about one chat arrived full-screen and read as somewhere else
entirely. It now gives the window back and draws in the five rows at the bottom, with the
frame you were looking at still above it. The palette and the pickers keep the whole pane:
they are lists that scroll and have no length limit, and they earn it.

The pane is still the active one, which is the only thing the zoom was ever load-bearing
for. Measured on tmux 3.7c through a real pty: unzoomed and focused, it is `h=5 active=1`
and the outer terminal still receives that pane's own mouse request.

## The rule is a test

`tests/test_the_palette_advertises_unmaking_what_it_makes.py` asks it of the palette's own
catalogue — the rows are data, and with the pointer off by default the palette is not merely
the primary surface, it is the only one. Every kind of thing the catalogue offers to make is
a kind it offers to unmake, stated as an equality so it fires in both directions.

Against the previous release it fails on the destroy side standing alone, which is exactly
how this shipped:

```
AssertionError: Items in the second set but not the first:
'chat' : the palette makes [] and unmakes ['chat'] — every kind of thing a surface offers
to make must be a kind it offers to unmake, at the same weight and in the same place
```

The day someone adds a create with no destroy, it fails the other way.

`workspace: close` is the next one this rule points at: it is specified, referenced, and
unimplemented, and there is no way to remove a workspace from the frame at all. It is filed
on its own — chats prove the pattern first.
