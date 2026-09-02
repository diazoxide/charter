---
version: unreleased
headline: The chat and workspace strips have a keyboard route — F2, next tab, Enter
---

The tab bars became clickable, and clicking was the only way to use them. That is backwards
for charter's own rule, which the frame states to every provider author who might add a
pointer affordance of their own:

> A component whose only route to a piece of state is a click has no route to it on most
> planes. **Give every pointer affordance a key as well.**

`[frame] mouse` is off by default, and a bar can never grow a keyboard of its own: tmux
routes your typing to the *active* pane, and that pane is the harness the frame exists to
protect. So the keyboard route is where every other one is — the palette.

```
charter · 20 to choose from
>   workspace: alpha — pick another
    detach — leave the harness running
    repo: select the next row
    repo: select the previous row
    chat: the next tab                 <- new
    chat: the previous tab             <- new
    workspace: the next tab            <- new
    workspace: the previous tab        <- new
```

`F2`, type `next`, Enter. It walks the strip **one step in the order the strip is drawn**,
running the same command a click on that tab runs — the same switch, the same refusals, the
same sentence on your own screen when one fires. It wraps, so every press moves something.

**Why it beats the picker that was already there.** `F2` → `chat` → a name reaches any chat
at any width, and is what a narrow frame has instead of a bar at all. What it is not is *the
next one*: cycling between two agents cost a pane cycle, a list and a choice each time, to
move one step along a strip you can see.

**They do not leave the palette open, unlike the repo table's own next/previous.** Those
move a selection and write two files. These move your *terminal* to another window, and the
palette's pane is in the window you are leaving — a row that stayed open would be staying
open somewhere you no longer are.

**A strip of one tells you when you press it, not before.** Opening the palette reads no
roster at all — that is a promise charter keeps so `F2` on a plane with forty workspaces
costs what it costs with none — and "is there anywhere else to go" cannot be answered
without reading one. So the row is always offered, and a press on a plane with a single chat
or a single workspace says so and starts nothing, rather than spending a pane cycle to
arrive where you already were. It is the same rule a click on the tab you are on already
follows.

One thing this is *not*: a bare key. Charter will not bind `Alt-]` or an arrow to this, for
the reason the repo table's own rows already give — a `bind -n` is server-wide in tmux, with
no per-window form, so it would take that key before your harness ever sees it.
