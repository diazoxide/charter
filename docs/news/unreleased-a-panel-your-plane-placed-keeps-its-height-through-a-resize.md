---
version: unreleased
headline: A panel your plane placed keeps its height through a window resize, instead of taking the repo table's rows
---

*"I put the chat bar on my frame, dragged the terminal, and the repo table vanished."*

It did not vanish — it was squeezed to one row while the one-line bar grew to seven, and it
stayed that way until the frame was relaunched.

Charter re-applies every pane's size after a window resize, because tmux redistributes all of
them proportionally on its own. **Which axis re-asserts which panel was the last per-slot
fact charter had not derived**: it was a literal in `commands_frame`, keyed by the four
committed slot names —

```python
{"top": "-y", "bottom": "-y", "right": "-x"}
```

— while every other such fact (which splits cost columns, which strips are a fixed height,
which one takes what its content needs) is built at import from what each component
*declares*, precisely so that a component charter did not write is answered rather than
missing from a list.

A panel placed by a `[[frame.component]]` table travels under its own id — `chats`,
`workspaces`, or a component someone else's package supplies. None of those is in that
literal. So the arithmetic sized the panel, the arithmetic charged the harness for its rows,
and then **nothing told tmux about it**. The harness's own explicit height took those rows out
of whichever neighbour tmux picked, and the one pane charter deliberately never asserts — the
repo table, which is the stack's remainder by design — absorbed the entire error.

Measured against real tmux 3.7c: a frame with `chats` placed at `size = 1`, grown from 200x40
to 200x90 and put through the real `window-resized` handler.

```
want    top 1    chats 1    harness 76    repos 7    bottom 1
got     top 1    chats 7    harness 76    repos 1    bottom 1
```

That is charter's own two-sizes-swapping-panes failure — the one the three-strip frame was
fixed for — in the one place that fix could not reach, because at the time there was nothing
a plane could place that was not already in the list. It is stable rather than transient:
three further resizes with no window change reproduced it exactly.

**The axis is derived now**, from the same edge the splitter already reads: columns for a
side panel, rows for a horizontal strip, and nothing at all for the repo table, which stays
the dependent pane it has always been (in a stack of N panes only N-1 heights are free, and
asserting all N is what makes the outcome depend on the order). A component charter does not
know about still gets no axis rather than a default, which is the same
filter-don't-refuse degrade the rest of the frame's geometry already makes.

Nothing moves for a frame that places only charter's own four panels — the derivation answers
exactly what the literal did for `top`, `bottom`, `right` and `repos`.

**If you had placed a bar or a provider's panel, this is why your frame drifted.** The
symptom is always the same shape: the panel with no `-y` grows to whatever the proportional
redistribution gave it, and the repo table pays.
