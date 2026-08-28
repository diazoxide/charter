---
version: unreleased
headline: Tables measure their columns in terminal cells, and everything inside a frame answers for the frame's workspace
---

Two unrelated properties, fixed together because each one had shipped in more than one
place and neither was going to be finished by patching a single call site.

## A column is measured in cells, from the values it holds

`charter worktree history` printed its timestamp into `{ts:<22}`. The timestamp charter
writes is `2026-08-28T01:09:13+00:00` — twenty-five characters, on every event, forever.
`str.format` pads and never truncates, so the field did nothing at all and the repo name
that follows it ran into a single space. `charter status` printed its stack into
`{:<12}`; `node-monorepo` is thirteen, and it is one of charter's own stack names, so on
any plane holding a node monorepo that row's branch column sat one column right of every
other row's — and one column right of the header above it.

`charter recall` had already got the harder half right: it sized its label column from the
data rather than from a constant. It then measured that column with `len`, which counts
*characters* while a terminal lays out *cells*. A CJK persona name is two cells per glyph
and a combining mark is zero, so a label well inside any budget still shifted its row —
and no constant, however large, could have fixed that half.

All three now go through `tui.column`, which landed for `charter persona stats` and does
both things at once: it takes the width from the values about to be printed, and it
measures them with `tui.width`. `tui.pad` fills, because sizing and padding have to be the
same unit or the arithmetic was for nothing.

One thing worth stating because it is counter-intuitive, and because it decided how this
was tested: **"do the columns line up" cannot see a mis-measured column.** `tui.pad`
truncates, so putting a too-small width back leaves every row perfectly aligned with the
value quietly cut off inside one of them. The probe that catches it is whether the value
is still readable off its row, and both probes are now on every table here.

## A repo row narrower than the table gives up a column, not the end of one

The status line composed each repo row at ninety-five columns and let the final crop trim
it to whatever the terminal actually had. Every cell after the branch sits at a fixed
offset, so on an eighty-column terminal — which is ordinary, and `$COLUMNS` is the status
line *pane's* width, so any split reaches it — the last two cells were what got cut:

```
w= 80   ├─ charter    main*    ✗ fa…
w= 70   ├─ charter    main*          …
```

The CI mark and the open change went off the right-hand end. The dirty marker rides on the
branch cell and survived, so a dirty, CI-failing, unpushed repo with an open merge request
read as one that was merely dirty. `✗ fa…` is worse than no CI cell at all: it is a
false-clean reading wearing a real one's clothes.

The row now decides its own shape from the columns it will actually get, and gives them up
in a written-down order: the branch text narrows first (the widest column and the least
urgent — which branch is a fact you look up, a red pipeline is one you act on), then the
repo name, and only then whole cells, change first and the CI mark last. Nothing is ever
drawn as a fragment. At eighty columns you now get the repo, `main*↑2`, `✗ failed` and
`#500`, all four whole, with the branch column paying for it.

The dirty/ahead/behind markers are the branch cell's floor rather than its first casualty:
they are true of the tree whatever it is called.

## Everything inside a frame answers for the frame's workspace

A frame is launched *for* a workspace, and nothing inside it can re-derive which one —
`$CHARTER_WORKSPACE` arrives empty by design so a second frame cannot inherit the first
one's pin, the working directory is the plane root, the per-session pointer is keyed on
the frame's own id, and the per-terminal pointer is keyed on a pane charter created. So
the launcher writes the answer down and the panels read it back. Two things were still
asking for themselves.

**The sidebar's todos.** On a cold cache the todo section fell through to a live gather
that resolved a workspace from the panel process, so a frame's very first paint could list
*another* workspace's open todos under this one's `todos N` heading. That is worse than the
blank repo table it sits beside: a populated list reads as an answer, and three todos you
have never seen read as three todos you have. It is handed the frame's workspace now. The
cold-cache scan stays, deliberately — the repo strip dropped its own because it repaints
five times a second while work is in flight, and the sidebar repaints once.

**The pane you type in.** The agent's shell reached the same dead rungs, so the frame could
correctly draw `harness-wrapper` in its header while every command typed inside it acted on
`default`. `charter` now consults the frame you are running inside as a rung of its own,
between the per-session pointer and the per-terminal one. The order is the answer to which
of the two wins:

* **`charter workspace use <name>` typed inside the frame still wins**, and still moves the
  panels. The launch is a seed, never a pin — handing the harness `CHARTER_WORKSPACE`
  would have worked and would also have taken `ws use` away from every framed session.
* **The frame outranks the pointer for the pane and the declared default**, because inside
  a frame those two answer for the asking process and the asking process is not the
  operator's terminal.

Deliberately a rung rather than a session-start hook: a harness that is not Claude Code has
no such hook, and neither does a bare `charter ws current` typed into the frame's shell.
`charter workspace current` says `frame` when that is what decided.
