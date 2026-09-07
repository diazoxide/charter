---
version: unreleased
headline: The quit confirmation takes the pane again, because its blast radius is the whole plane
---

`chat: close` opens a drawer — five rows at the bottom of the window, the frame you were
looking at still above them. That was the right change and it stays.

**`charter: quit` shared that surface, and should not have.** It stops every harness on the
plane. Its confirmation is the one place an operator sees the full blast radius: every chat,
in every workspace, and which of them can resume the conversation. In a five-row drawer that
is two of them at a time, under a heading saying how many there are, scrolled.

So it takes the whole pane again, and close still does not.

## The rule

**The size of the surface matches the size of the consequence.**

A confirmation about one chat is a drawer. A confirmation about every chat on the plane
takes the pane, because the operator has to be able to read the whole list before answering
— and **scrolling a destructive list is how you answer it without reading it.**

This is charter's existing guard at a second scale. The destructive rows are already placed
LAST in the palette's catalogue, *"because the palette's cursor starts on the first row that
can run, and a destructive row at the top would be one `F2 Enter` away."* Placement and
presentation are one rule: put distance between the operator and a destructive answer in
proportion to what the answer costs, and never ask them to scroll past the thing they are
agreeing to.

## What it cost

One expression. `commands_frame._as_a_drawer` reads the verb off the surface's own label,
which both routes into it already spell — so the condition arrives with the surface, and the
rule lives in one place rather than as an `if` at two call sites that could come to
disagree. The tab-strip route only ever opens `chat: close`, so it only ever gets a drawer,
and it gets one by inheriting the rule rather than by holding a copy.

That the change was one expression is not luck. The agent that built the drawer chose
one-surface-one-rule over two surfaces that read differently by doorway, wrote down that it
was a trade rather than a fact, and said where to change it if the trade turned out wrong.
It turned out wrong; the note was accurate, and the fix was where it said it would be.

## Both directions are pinned

A test that `chat: close` is a drawer, and a test that `charter: quit` is not. A suite
asserting only one of them leaves the other free to drift into it with nothing to say so —
which is exactly what happened here: the previous behaviour had a passing test asserting
quit was a drawer too, and that assertion was the thing this had to change.

A third pins the discriminator: a plane with one chat on it does not make quit a drawer. The
rule is what the verb *can* reach, not what it happens to reach at this moment — an operator
who learns that quit takes the pane has learned something true every time, and a surface
that changed shape with the plane's size would teach them nothing they could rely on.
