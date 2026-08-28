---
version: unreleased
headline: The deletion sweep runs on every pull request, and it can now see a string
---

For three review rounds the same defect kept arriving: **a guard with no test behind it.**
Correct code, shipped, that a later refactor could delete in silence. Every one was found
by deleting the line and running the suite — a step that is mechanical, slow and invisible
in the diff, which is why it kept being *reported* rather than *run*.

`tools/sweep.py` made that mechanical in August. **Now CI runs it**, on every pull request,
scoped to the lines the branch added.

```
Deletion sweep — 2 actionable survivor(s)

| outcome            |  n | what it means                                        |
| pinned             | 34 | a test goes red without the line                     |
| unpinned           |  1 | a guard with no test behind it                       |
| masked cluster     |  0 | two or more in one function; none is safe alone      |
| platform-deferred  |  1 | may be unreachable on linux; never fails this gate   |
| unresolved         |  0 | no verdict — timed out, not measured                 |
```

**It blocks nothing yet, and that is the design.** A gate whose numbers nobody has read
gets switched off the first time it is inconvenient, so it reports first and says on the
page what it *would* have done. One flag turns it on, once the numbers have been believed.

**A survivor comes with the assertions that were supposed to hold it.** Not "line 291 is
unpinned" but the covering tests, by name, and what each one actually asserts about the
symbol. That field is the difference between a report somebody triages and a report
somebody mutes — because most "that mutant is equivalent" claims turn out to be "the test
asserts too little", and you cannot tell which from a line number.

**And it keeps three kinds of survivor apart.** A *masked cluster* is two survivors inside
one function: two guards in sequence hide each other, so neither looks like it matters on
its own and neither is tested. *Platform-deferred* is a catch on an error the operating
system decides — `except OSError` around a pty read is dead code on macOS and live on
Linux — and the gate never fails on one, because a check that fails a branch for a clause
the runner's kernel cannot reach is a check people are right to disable. *Unresolved* is
its own answer too: a run that timed out is not a pass and not a failure, it is no verdict,
and machine load must never be allowed to certify a guard as tested.

## Two shapes it could not see before

The sweep mutates by statement shape, and two families had no shape at all — between them,
nearly every finding the last round had to check by hand.

**A string is now a value the sweep can move.** The old objection was that strings have no
honest perturbation: picking `1003` over `1000` for a mouse-tracking escape is fitting the
answer key, not recognising a shape. The answer is to move the *value* while holding
everything structural — same length, same character classes, escapes untouched. So
`"\x1b[?1000h"` becomes `"\x1b[?2111i"` and is still an escape sequence, `{:<28}` becomes
`{:<39}` and is still a format spec, and a regex stays a regex. It is derived from the
constant and nothing else, so it cannot fit an answer key it never reads.

It applies where the program *reads* the string — a key, a comparison, a pattern, a
separator, a width — and not to prose. That line is drawn from a measurement rather than a
preference: mutating every string in `charter/` took the tree from 7,006 mutations to
14,801 and spent the difference on log lines.

**And a near-synonym is now a mutation.** `<` against `<=`, so a guard written one notch
out is caught by something other than luck; `.lower()` against `.upper()`,
`startswith` against `endswith`, `sorted` against `list`; and `p.resolve()` dropped to `p`
— which is the exact shape of the bug that made the sweep itself unusable on macOS, where
one path was normalised and the one it was compared against was not.

Both were checked against a defect already known and already fixed: run over the commit
before the roster-column fix, the sweep now finds the `{:<28}` width literal on its own —
the finding that had to be hand-written last time because no operator could see it.

## The sweep swept itself again

It found 123 of its own 198 mutations survived, deleted two of its own lines that turned
out to guard nothing, and the operator table — the half that decides every verdict — now
holds. The report renderer is still thin, and the number in the pull request says so
rather than rounding it up.
