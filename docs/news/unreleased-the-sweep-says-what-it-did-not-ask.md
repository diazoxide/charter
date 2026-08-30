---
version: unreleased
headline: The deletion sweep stops asking regex questions that have no answer, and says when it declines one
---

More than half of every regular expression in charter was being asked a mutation question
that could not be answered — and the answer came back as `no verdict`, which is the one
outcome the gate exists to make a reviewer stop on.

## The mutant was not a program

`retune-string` re-spells a string constant: same length, same character classes, a
different value. For a digit that is `0 -> 1` and `9 -> 0`, and **inside a character class
that inverts the range**:

```
retune(r"\$[0-9]+")  ->  r"\$[1-0]+"
re.compile(...)      ->  re.error: bad character range 1-0
```

So the module raises while it is being imported, every selected test module fails to load,
**zero tests run**, and the sweep — correctly — refuses to call that a pin. What a reviewer
sees is `no verdict: 2 not measured` beside a report advising them to re-run on a quieter
machine. No machine and no timeout can ever clear it.

Measured over `charter/` and `tools/`: **56 of the 108** patterns in the tree retune into
something `re.compile` refuses. Not two lines on one branch — every one of them was a
question waiting to be asked the day its line moved.

## The rule it needed was already written, for its neighbour

`retune` has always left the character after a backslash alone, and says why: `\d -> \e`
is `re.error: bad escape`, "a red for a reason that has nothing to do with the property,
which is the one outcome this whole file exists to refuse". A character range is that same
hazard one step over, and so is the letter after `(?` — `(?i) -> (?j)` is `unknown
extension ?j`, four of those in `charter/hooks.py` alone.

All three are one rule: **a character that says what kind of thing comes next is syntax,
and the syntax is not the value this operator asks about.**

Which end of a range moves is the interesting half. Holding both would keep the pattern
valid by making the mutation a no-op — and a character class is the whole of many patterns
here, so that would withdraw the question rather than fix it. The low end moves and the
high end holds: `[0-9] -> [1-9]` is a valid pattern that stops matching `0`, which is a
question a test can answer. With the three rules the 56 become **1**.

## And the one that is left is reported, not dropped

A mutation the tool declines is a question it did not ask, and the one thing it may not do
is decline one in silence — that is the same failure as a shard that never reported,
arriving from inside the plan instead of from a runner.

So the last one (a `\x1f -> \x2g` hex escape in `charter/tui.py`) is still planned, still
sharded, and still reported. It gets a verdict of its own — `withheld` — with the reason
beside it, a row in the gate table, and a line in the check's own name: `no survivors,
1 withheld`.

**Its own bucket and not folded into `unresolved`**, because the two say opposite things
about what the tool knows. `unresolved` is *I looked and could not tell*. `withheld` is *I
decided not to ask, and here is why*. Collapsing them makes a deliberate, bounded,
explained subtraction read as a timeout — and a `no verdict` that cannot be cleared is how
the signal a reviewer must stop on gets spent.

It never fails the gate, for the same reason a question the interpreter puts out of reach
does not. It is loud so that it is not a finding about nothing either.
