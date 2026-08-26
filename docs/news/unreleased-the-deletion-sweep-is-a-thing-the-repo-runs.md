---
version: unreleased
headline: The deletion sweep stops being a thing an agent promises and becomes a thing the repo runs
---

*"Every guard this branch added is now a line a test goes red without."*

That was a commit message. The same commit added six more guards with no test behind
them, in the same two files, immediately after writing the rule down.

Three rounds of review found that defect thirty-six times: a guard that refuses, clamps,
contains or falls back, shipped correct, with nothing in the suite that would notice if a
later refactor deleted it. Every one was found the same way — delete the line, run the
whole suite, see whether it stays green. And every one was found by hand, which is the
problem. Eighty-six mutations against a four-minute suite is four and a half hours, it
leaves no trace in the diff, and nothing in the repository could tell a sweep that was
**run** from a sweep that was **reported**.

**`tools/sweep.py` runs it.**

```
$ python3 tools/sweep.py
sweeping 5b02b3f6dd42 (paths: charter)
  diff against 97163fb4c5c7: 8 file(s), 524 added line(s)
  selection map: cached
  57 mutations across 8 file(s)

charter/frame/layout.py:291  in `_placed_here`   [drop-conjunct] — is the half pinned?
    shipped : isinstance(name, str) and name not in SLOT_SIZE
    mutant  : isinstance(name, str)
    full    : Ran 6002 tests — OK, with the line gone
    NOTE    : 4 survivors sit in `_placed_here`. Two guards in sequence
              mask each other, so none of them is safe to call equivalent on its own.
    covered : 31 module(s) execute this file and NOT ONE names `_placed_here`
```

It reads the diff against the merge-base, so a branch answers for the guards **it** adds.
It mutates by statement shape rather than by line — deleting arbitrary lines mostly
produces `SyntaxError` and `NameError`, which redden tests for reasons that have nothing
to do with the property, and a red for the wrong reason is worse than no signal. Every
shape in the table is one a review round actually found: a refusal dropped, a clamp
dropped to the value it clamps, a broad `except` narrowed to something nothing raises, a
containment call removed, a `.get(k) or ()` turned back into `d[k]`.

**What makes it affordable is that it does not run the suite.** It runs the suite once,
under `sys.settrace`, one test module at a time, and records which *function* in which
file each module actually executes. A mutation then runs only the modules that reach it:
the median subset on charter is seven modules and two seconds, against 322 modules and
four minutes. The map is cached against a hash of the tree it was measured on.

**And selection is never allowed the last word.** Any mutation that survives its subset
is re-run against the FULL suite before it is reported. The two mistakes are not
symmetrical: a false survivor costs one full run and a reviewer's minute, while a false
*pin* is a guard the repository has quietly certified as tested when it is not — which is
the entire defect this exists to stop. So a subset that goes green is never an answer, a
file no traced module was seen executing is not pinned by that silence, and a run that
*hangs* has not passed.

**There is deliberately no suppression list.** The usual escape hatch is to mark a mutant
"equivalent" and move on, and that is how this kind of gate becomes a rubber stamp —
charter already refuses the analogous thing, because there is no config key that lifts a
guard denial (#370), since one charter could read is one a committed file could flip. The
reasoning is even simpler here:

> If deleting a line genuinely changes nothing observable, the line should be deleted.

"Equivalent mutant" and "dead code" are the same finding. The sweep reporting it *is* the
sweep working.

Two things push back on the temptation to write a survivor off anyway. First, a survivor
is printed next to **what the covering tests actually assert** about the mutated symbol —
because most "equivalent mutant" claims turn out to be "the test asserts too little".
Measured on the release workflow's version check: deleting its `-z "$claimed"` refusal
left the run still exiting 1, because the check below it caught the empty string instead.
Same exit code, a different reason, and a test asserting only the exit code stays green
over a real deletion. Second, `--second-order` applies survivors that share an enclosing
function *together*, because two guards in sequence hide behind each other and each looks
harmless alone.

It is not wired into CI yet, and that is on purpose: a gate whose baseline nobody has seen
gets disabled the first time it is inconvenient. `--all` charges the whole tree instead of
a diff, which is how that baseline gets counted for the first time.

Nothing to adopt — it is a tool in the repository, not a change to charter. Run it on a
branch before you ask anyone to read it:

```
python3 tools/sweep.py                  # this branch, against its merge-base
python3 tools/sweep.py --second-order 24
python3 tools/sweep.py --all            # the standing debt, as a number
```
