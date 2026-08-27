---
version: unreleased
headline: The roster tables size their columns from the names they are about to print, measured in terminal cells
---

`charter persona stats` printed its name column like this.

```python
print(f"{'PERSONA':<28}{'MEM':>5}{'RECENT':>8}{'VERIFY':>8}{'DUP':>6}{'DISP':>6}  STATUS")
...
print(f"{contain.one_line(r['persona']):<28}{r['count']:>5}{rec:>8}...")
```

That constant is two different mistakes wearing one number, and each of them on its own is
enough to stop the report being a table.

**28 is a guess about content.** `:<28` pads a short name and does nothing at all to a long
one — it *pushes*, so MEM, RECENT, VERIFY, DUP, DISP and STATUS land on that row where they
land on no other row, while the header and every other row stay put. A persona is a
DIRECTORY under `personas/`, so the name arrives from whoever last pushed rather than from
the person reading the report, and `contain.one_line` **grows** it on the way to the
screen: a line separator becomes a four-character escape, so a committed 22-character name
renders at 30 without anybody having typed a long one.

**`:<28` counts characters, and a terminal lays out cells.** A CJK name is two cells per
glyph, a combining mark is zero, and an emoji is two cells in one character. So an
eight-glyph name that fits the constant three times over was padded to 28 *characters* and
drawn as 36 *columns*, and its row shifted by eight without ever going near the boundary.
This half is why widening the constant was never the fix: at any width, `str.format` is
still counting the wrong thing.

## Sized from the names, in cells

`tui.column` is the new answer to "how wide does this column have to be": the header, the
widest cell it is about to hold, and the gutter — every measurement `tui.width`, which is
what the terminal actually does. `tui.pad` fills the cells, because a column measured in
cells and padded in characters is arithmetic done twice in two units and thrown away.

Every column is sized this way, not only the one the issue named. MEM, RECENT, VERIFY, DUP
and DISP were the same shape with a threshold nobody had reached yet — a seven-digit
dispatch tally pushes STATUS exactly the way a 29-character name pushed the other six, and
"no plane has that many dispatches" is a fact about today's data rather than a property of
the renderer. The header and the rows are now built by one function called twice, so there
is no second code path left to disagree about a width.

`persona list` had already sized its columns from its names, which is where the fix's shape
comes from — but it measured them with `len`, so it carried the second half of this defect
in full. It measures with `tui.column` now, and its ROLE column, the one holding prose
rather than an identifier, keeps its cap and truncates *inside* the column with an ellipsis
instead of being cut to a character count that was never its width.

## Sized, not clipped — and that is a choice

The other defensible policy is to clip the name to a fixed column, and it is rejected here
for a reason that predates this issue. #472 asks these reports to name each persona in its
bounded spelling, because the steward reading them acts on that name — `persona show`,
`persona retire`. A column capped at some display constant would satisfy every alignment
case and hand back a prefix nobody can look up, which is a worse report than a wide one.
The bound on the name stays `contain.one_line`'s, applied to the value once, the same as
on every other surface; the column honours that bound rather than inventing a second,
smaller one. An ordinary roster now renders *narrower* than before, because 28 was always
more than a roster of real names needs.

## What this does not fix

Alignment is about the cells a value **declares**, and that is all `tui.width` can report.
Whether a cell is legible is a different question, left unanswered: three U+3164 HANGUL
FILLERs measure six cells, the same as `devops`, and render as nothing at all (#498). A persona named with them still gets
a correctly sized, correctly padded, entirely blank name cell. That is the other half, it
belongs to `contain.readable`, and deciding whether the roster tables are identifier
surfaces in that sense is a judgement worth making on its own rather than smuggling in
behind a column-width fix.

The same shape survives in seven other tables, and they were enumerated rather than
assumed. Some of their constants are safe by construction — a column holding
`ask`/`allow`, or `claimed`/`done`/`abandoned`, cannot be pushed by a value that has
nowhere else to come from. Others hold a committed name with no bound on it at all: a
workspace name, a vault name, a git branch, a repo, a worktree piece. Two are already
wrong today rather than one commit away from it — `worktree history` pads a 25-character
ISO timestamp into a 22-wide column, and `status` pads the 13-character stack name
`node-monorepo` into a 12-wide one, so both push every time they are printed. `recall`
sizes its label column from the data and then measures it with `len`, which is this
issue's second half exactly. None of that is changed here; it is written down in the pull
request so the next person starts from a list instead of a grep.
