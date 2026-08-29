---
version: unreleased
headline: The repo strip can have a height you chose, and the `size` key that used to do nothing on it now does
---

The repo table is the one panel charter sizes to its content: one row per clone, plus its
heading. Two clones, two rows. That was a deliberate decision — a fourteen-row strip padded
with blanks is worse than a two-row one — and it is still the default.

It is the wrong default for a plane whose clone count moves. You add a repo and the pane
grows; you remove one and it shrinks; your session shuffles up and down underneath it. What
was missing was a way to say *stop*:

```toml
[[frame.component]]
use  = "repos"
size = 15
```

Fifteen rows with one clone, fifteen with thirty.

**The key was already in the form, and on this component it did nothing.** `size` on a
`[[frame.component]]` table could only echo a number charter had already declared — and the
repo table declares no number at all, it declares "as tall as its content". So *any* `size`
on it was a value charter could not honour, which takes the whole arrangement out of play
(#535): the four tables dropped back to `slots` and you got a different frame with nothing
said. An operator writing `size = 15` today gets today's content-sized strip and no error.

## What charter can honour, and why the other three panels are different

The three fixed panels — identity, the attention strip, the sidebar — have their heights
turned into a table once, when charter starts. Nothing between your `charter.toml` and
`split-window` carries a per-plane override for them, so a different number there could only
be read, validated, stored and then ignored. They still only echo, and any other number
still takes the arrangement out of play.

The repo table's height never enters that table. It is recomputed from your resolved
arrangement at every launch and again on every terminal resize, which is what a content-sized
pane requires — and it is what gives your number somewhere to be read. One rule, not a
carve-out: a committed value is honoured exactly where something reads it.

It is a whole number of cells, at least 1. `0`, a negative, a `true`, a string and a float
are refused the way every other unusable value in that form is.

## Your session still keeps its twelve rows

tmux does not refuse an over-large pane height. It grants it, and takes the difference out
of the neighbour — measured on 3.7c, `resize-pane -y 40` in a 20-row window left the harness
pane **one row tall**. A `size = 40` in a committed file is that command with a config file
in front of it, and a plane's frame is shared: it arrives on a laptop whose terminal its
author never saw.

So a pin says what the strip *wants*. What the window can spare is decided afterwards, by
exactly the same arithmetic that already caps a fourteen-repo plane on a short terminal. On a
terminal with room you get your number; on one without you get what is left after the harness
has its floor; below the 95 columns the table needs, the strip is dropped entirely, as before.

## What did not change

The repo pane is still the one pane tmux is never told the height of. In a stack of N panes
only N-1 boundaries can be asserted — assert them all and the result depends on the order,
which is how a re-assertion once came out with the table one row tall and the attention strip
six. Everything around the strip is asserted and the strip lands on its number, whether that
number came from your clone count or from your file. A pin changes the number, not the
mechanism.

A plane that writes no `size` — which is every plane today, charter's own included — draws
exactly the frame it drew before.
