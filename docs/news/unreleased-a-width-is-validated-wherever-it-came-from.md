---
version: unreleased
headline: a terminal that reports no size no longer squeezes charter into one column, and four more tables are measured from the values they hold
---

Two things a width can be wrong about, and charter had one of each left.

## A tty that says zero columns

`tui.term_width` asks two sources in order: `$COLUMNS`, then the tty. The first was
guarded, and correctly — a real environment in this project exports `COLUMNS=0`, `int("0")`
parses happily, and `max(floor, 0)` used to turn that meaningless value into a
plausible-looking floor-width render. The guard was written like this:

```python
w = int(os.environ["COLUMNS"])
if w <= 0:
    raise ValueError(w)          # the environment's zero IS guarded
```

One rung further down the same function, the identical value walked straight through:

```python
w = os.get_terminal_size().columns   # the syscall's zero is NOT
```

so a terminal reporting zero columns produced `max(1, 0)` — **one column** — and charter
drew every table, row and panel into it.

That terminal is ordinary. A pty created without a window size reports zero until somebody
calls `TIOCSWINSZ`, which is exactly what `openpty` gives you: tooling that starts a pty,
some CI shells, a terminal attached before its size is negotiated. The suite has been
carrying the evidence for a while — run under a real pty, **28 failures across 8 modules
were all terminal-size**, and giving that pty a size made all 28 pass. Nothing had ever
asked.

The fix is not a second zero-check beside the first. Each source now answers with a number
or with nothing, the **answer** is what gets judged, and a source with no usable answer is
indistinguishable from a source with no answer at all — so a third source would be one
entry in a tuple rather than a fourth place to remember the zero. A width nobody could
measure falls to the caller's stated default (80 unless they say otherwise), which is what
that parameter was always for.

This is the sixth instance this month of a guard matching a spelling rather than a
property — [#547](https://github.com/diazoxide/charter/issues/547) (a flag's spelling, not
its parse), [#558](https://github.com/diazoxide/charter/issues/558) (the ref's spelling,
not whether the check ran), [#537](https://github.com/diazoxide/charter/issues/537),
[#498](https://github.com/diazoxide/charter/issues/498),
[#577](https://github.com/diazoxide/charter/issues/577) — and the first where the right
idea was already sitting at the top of the very ladder that leaked at the bottom.

## Four more tables sized from a constant

`tui.column()`/`tui.pad` measured `persona stats`
([#508](https://github.com/diazoxide/charter/issues/508)), then `worktree history`,
`status` and `recall` ([#592](https://github.com/diazoxide/charter/issues/592)). The last
four now go the same way:

* **`worktree list`** — `{branch:<28}`, and this one was wrong *today*: a branch name past
  28 characters is a Tuesday, and `:<n` pads a short value while doing nothing at all to a
  long one. It pushes, so that row's state, claimant and outcome land where no other row's
  do. A worktree on a detached HEAD writes its whole path into that cell, so it did not
  even need an unusual branch name.
* **`vault list`** — `{:<18} {:<16} {:<12}` over a vault name the operator typed, a
  provider name a plugin supplies and a persona name that is a committed directory. The
  rule under the header was those same constants written a *second* time, which is how it
  went on agreeing with the header while both sat left of the rows they described.
* **`workspace list`** — `{:<22}` over a name the operator minted. `list_workspaces` lists
  every directory under `workspaces/`, not only the ones `workspace create` would accept,
  so the values this has to draw are bounded by what the *listing* can return.
* **`charter doctor`** — `{name:<16}`, with three checks (`credential paths`, `workspace
  clones`, `plane-root guard`) sitting exactly on it, so they got a single word space where
  every shorter name got a column.

`doctor` is the one of the four that cannot size from its rows: it prints each row as its
check lands, deliberately, so that a preflight killed by its hook timeout says where it
stopped instead of printing nothing at all. Its NAME column is therefore sized from the
names the checks *carry*, before the run — asked of `declared_or_default_forges` for the
`gh`/`glab` pair, and pinned by equality against what `run_all()` actually produces, so a
check renamed or added fails on the commit that does it. And the width is a **floor**: a
name the column did not know about pushes its own row rather than being cut, because
`tui.pad` truncates and a check name cut in half is a failure nobody can go and look up.

## What the tests had to avoid

Three traps, all measured, all recorded in the cases rather than in a commit message.

**A precomposed accent is a silent control.** `équipe-mémoire` is 14 characters and 14
cells — it tests nothing at all. The fixtures use a base letter with a combining acute
that Unicode has no precomposed form for, checked as it came back *off the filesystem*,
because a filesystem that composes the pair hands the renderer a value `len` and
`tui.width` agree about.

**A CJK name alone does not catch `len`-sizing**, because `tui.pad` measures in cells and
the table stays aligned while the value is cut. What catches it is a **pair** — one value
widest in characters, another widest in cells, alone in the table — and a fixture guard
keeps it a pair.

**And an alignment assertion cannot see a mis-measured column at all.** Every alignment
test in #508's own suite passed against a restored constant. The probe that works is
whether the value is still readable off its row.

`charter doctor`'s NAME column is one cell wider than it was; nothing else about these
tables changes for values that already fit. Nothing to adopt.
