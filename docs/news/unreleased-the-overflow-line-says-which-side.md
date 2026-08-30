---
version: unreleased
headline: The repo table says whether the rows you cannot see are above you or below you
---

Scroll the repo table to the bottom and the last line said:

```
  …(+8 more, all clean)
```

There is nothing below it. All eight are above. The count was right — charter works it out
as "every row that is not on screen", which is true wherever the window sits — but `more`
asserts a direction, and at the bottom of the table it was pointing at empty screen.

Worse than wrong in one place: it was *the same sentence everywhere*. Twelve repos in a
five-row pane, at three scroll positions:

```
offset 0   …(+8 more, all clean)      <- 8 below
offset 4   …(+8 more, all clean)      <- 4 above, 4 below
offset 8   …(+8 more, all clean)      <- 8 above
```

Three different places to be standing, one string. Scrolling moved a number that never
changed.

**It now says which side.**

```
  …(8 below, all clean)
  …(4 above, 4 below, all clean)
  …(8 above, all clean)
```

The table is one window over one list — repo rows, then the worktree rows beneath them — so
the first number is simply how far down you have scrolled. A side with nothing on it is not
drawn, because `0 above` is a field that is always false and always there.

**Why it is not an arrow, and not a second row.** An arrow was the obvious thing and it is a
trap on this surface: `●`, `◆` and the pointing triangles are East-Asian *Ambiguous* width,
and they have broken this layout twice — the mark on the tab bars is an ASCII `*` for exactly
that reason. A marker row along the top was the other obvious thing, and it costs a **repo
row**: this table's budget is measured in rows, and it would spend one at precisely the
budgets short enough to need scrolling in the first place. The line at the bottom was already
reserved out of that budget, so putting both numbers on it costs nothing at all.

Clicking it still does nothing, which is the point of it: it stands for rows that are not on
screen, and picking one of them because you clicked the line saying they exist is a question
nobody asked.

This is the same defect the tab bars were fixed for one release earlier — an overflow marker
that describes a window but is computed as though the window always starts at the top. The
bars would have produced a wrong *number* and ship two counts to avoid it; the table's number
was right and its *sentence* was false. They share no code, so this is the same rule applied
twice rather than one fix in two places.
