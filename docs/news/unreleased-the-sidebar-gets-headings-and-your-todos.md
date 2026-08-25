---
version: unreleased
headline: The frame's sidebar gets headings, a badge column, and this workspace's todos
---

The right sidebar was a bare column of names with glyphs trailing them. Nothing said what
the column was, the badges started wherever each name happened to end, and the todos you
had actually written down were visible nowhere in the frame — `bottom` printed `3 todos`
and that was the whole of it.

**Two headed sections now.** `personas`, then `todos` beneath it:

```
▪ personas 6
▸ steward          ✎47
▫ forge ◦          ✎9
▫ reddit           ✎7
▫ release          ✎38
▫ statusline       ✎14
▫ 測試員測試員     ✎3

▪ todos 10
- wire the sidebar heading into the frame
- decide what a done todo looks like
- chase the flaky tmux integration module
  …(+7 more)
```

The headings are the status line's own — same marker, same indent, same lower-case word —
so the two surfaces cannot drift into two different ideas of what a column header looks
like. The persona count is every persona this plane has, not the number of rows that fit:
it and the `…(+N more)` line under it come from the same number, so they cannot contradict
each other.

**The badges are a column now, not a tail.** They used to start wherever the name ended, so
a longer name pushed its own badge past every other and the column read as ragged. The
column's width is the widest badge on screen, measured in terminal cells rather than
characters — `⚡` is two cells wide and `✎ ◌ ⚑ ✗` are the ambiguous kind a font gets a vote
on, which is what had broken this layout twice before. A name too long for what is left
loses its own tail to a `…` and moves nothing else.

The vault dot stays with the name rather than joining the badges, and that is a choice
worth stating: it appears only when a vault *cannot be used*, so it is absent on almost
every row. In the badge column its width would be paid by every persona for a fact about
one of them, and the whole column would shift the day somebody registers a vault.

There is a floor under the name: one persona holding three dispatches (`⚡3 2h?`) will not
take twelve columns off every name in a 22-column sidebar. Past that point the badge
column is what gives way.

**The todos come from the workspace, and never from a per-repaint read.** `charter ws todo`
is still where the whole list lives; this is the frame's reminder that it does. Open todos
only — oldest first, which is `charter ws todo`'s own ordering and the point of it: what
surfaces is what is being avoided, not what you already have in mind. Done todos never
appear, because a list you read past is not a list you act on. A workspace with nothing
open gets **no section at all** rather than a `todos 0` heading over an empty space; the
attention row at the bottom of the frame keeps its unconditional count, so the number is
still somewhere.

How many rows you get is what the pane has. The personas are served first — `right` is the
persona column everywhere else charter names it — and the todos take what is left, capped
at eight rows so a forty-row terminal does not turn into a todo list. A pane with room for
only one row draws no section: a heading with nothing under it claims this workspace has no
todos, which is exactly the false-clean reading the frame refuses everywhere else. At
`minimal` density the section shrinks to four rows, the same as the persona list.

**It costs nothing at idle, and that was a constraint rather than a hope.** Reading todos
means opening and parsing one file per todo, and a panel repaints whenever the plane's
state moves. So the todos are gathered once, into the same cache the repo table already
draws from, and the sidebar reads that — the panel never touches the todo directory. The
cache carries at most twenty titles and the true count beside them, unclipped, so a
workspace with four hundred open todos is told it has four hundred rather than twenty.

A todo's title is a committed value — someone else's machine wrote it into this plane's
repo — so it is bounded before any column arithmetic touches it. A newline in a title
shows as `\x0a` on one row instead of writing a second row that looks exactly as much like
charter's own output as the first.

**And the charter version moved to the top bar's right-hand end.**

```
 ⬢ demo*  ◆ steward · ◇ personas forge · reddit · release        charter 0.53.0
```

The left of that bar answers *where am I and who am I being*; the version answers *which
build is saying so*, which is not part of the same sentence. At the far edge it reads as
the bar's signature rather than as a fourth field of your identity. The `dev` chip travels
with it, as it always has. Nothing is cut to make room: on a bar too narrow for both, the
version is dropped whole — the same field `minimal` already drops, so a starved bar and a
terse bar lose the same thing rather than a narrow terminal inventing a third way for the
row to degrade.

Nothing to adopt. Upgrading is the whole of it.

One thing to know if you are watching a frame come up: a launch deliberately starts with no
gather cache — a recycled pid must not inherit another frame's repos — and the sidebar's
first paint falls through to a live gather rather than drawing an empty list. The cache is
what makes every repaint *after* that one free; it is not what fills the first one.
