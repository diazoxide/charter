# The deletion sweep becomes a thing the repo runs, not a thing an agent promises

## Why

Three rounds of Phase 2 review found the same defect thirty-six times: **a guard with no test
behind it.** Correct code, shipped, that a later refactor could delete in silence.

Every one was found by the same move — *delete the line, run the full suite, see if it stays
green* — and the rule was written into the Phase 2 plan after round two in as many words:

> For every `if` you add that refuses, clamps, contains or falls back, write the test that goes
> RED when that line is deleted. Then run the sweep yourself — delete each new guard in turn,
> run the full suite, and report any that stayed green **before** submitting.

Round three then fixed twelve guards and committed under the message *"Every guard this branch
added is now a line a test goes red without"* — **and that commit added six more unpinned
guards**, in the same files, immediately after writing the rule down. Its report also claimed
the branch was green while CI was red at the same head.

That is the finding. Not "agents are careless" — the rule was known, recent, and explicit.
The sweep is mechanical, slow (86 mutations × a 4-minute suite is the 4½ hours the last round
actually took) and invisible in the diff, so it is the step that gets **reported** rather than
**run**. Nothing in the repository can tell the two apart.

**So the verification has to stop living in the agent's promise and start living in the tree.**
A branch that adds an unpinned guard should not be able to go green. That is the only version
of this rule that has ever held anywhere.

## What already exists, and what does not

- No `tools/` or `scripts/` directory. No mutation infrastructure of any kind.
- 327 test modules, ~6000 tests, run flat: `python -m unittest discover -s tests -v`.
- `dependencies = []` and that is load-bearing — the harness is **stdlib only**, like everything
  else here. `ast`, `sys.settrace`, `difflib`, `subprocess`, `unittest` are the whole toolkit.

## The design

### 1. Test selection is the thing that makes it affordable

A full suite per mutation is why the sweep costs hours. Mutating `charter/frame/overlay.py` and
then running `test_workspace_locking` is pure waste.

Build the map **once**, with `sys.settrace`, by running the suite one module at a time and
recording which source files each test module actually executes:

```
charter/frame/overlay.py  ->  [test_frame_overlay, test_frame_overlay_escape_hatch, …]
```

Cache it keyed by the tree's hash. Rebuild when the map is stale. A mutation then runs only its
mapped modules — seconds, not minutes.

**Selection is an optimisation and must never be the final word.** Any mutation that *survives*
its selected subset is re-run against the **full** suite before being reported. A false
"survivor" costs one full run; a false "pinned" would be the bug this whole document exists to
prevent, so the asymmetry decides the design.

### 2. Mutate statements, by shape — not lines, blindly

Deleting arbitrary lines mostly produces `SyntaxError` and `NameError`, which redden tests for
reasons that have nothing to do with the property. Those are false pins and they are worse than
no signal.

Mutate by statement shape instead, which is exactly what the three rounds did by hand:

| shape | mutation | what it asks |
|---|---|---|
| `if C: return` / `raise` / `continue` | drop the statement | is the refusal pinned? |
| `x = max(a, b)` / `min(…)` | drop to the inner operand | is the clamp pinned? |
| `except E:` | narrow to an exception nothing raises | is the catch pinned? |
| `f(contain.one_line(x))` | `f(x)` | is the containment pinned? |
| `d.get(k) or ()` / `d.get(k, v)` | `d[k]` | is the fallback pinned? |
| `if isinstance(x, str)` | drop the test | is the type filter pinned? |

Each shape is drawn from a guard the sweep actually found. Skip any mutation whose result does
not parse.

### 3. Scope the gate to added lines

Run against the diff with the merge-base, not the whole tree. A PR is answerable for the guards
**it** adds.

Charge the whole tree separately (`--all`), not as a gate but as a number, so the existing debt
on `main` is countable for the first time instead of merely suspected.

### 4. There is no suppression list for equivalent mutants

The usual escape hatch — mark a mutant "equivalent" and move on — is how this kind of gate turns
into a rubber stamp. charter already refuses the analogous thing: **there is deliberately no
config key that lifts a guard denial** (#370), because one charter could read is one a committed
file could flip.

The same reasoning applies, and here it is even simpler:

> **If deleting a line genuinely changes nothing observable, the line should be deleted.**

"Equivalent mutant" and "dead code" are the same finding. The sweep reporting it is the sweep
working.

The one real exception is a deliberately redundant defence-in-depth line, and it gets a marker
that must **name what pins the property elsewhere** — a test id, not prose. A marker naming
nothing fails the same as an unpinned guard.

### 5. Assert the reason, not just the refusal

Measured on `release.yml`'s version check (#558) while fixing it: deleting the `-z "$claimed"`
refusal leaves the run **still exiting 1**, because the mismatch check below it catches the empty
string instead.

```
shipped:  rc=1  this run did not say which version it publishes (the version input <none>)
mutant:   rc=1  this run names  (the version input <none>) but pyproject.toml says 0.53.0
```

Same exit code, different reason. **Two guards in sequence mask each other**, so a test asserting
only the exit code stays green over a real deletion.

This shape has now appeared three times, in three different spellings, so it is the norm rather
than the curiosity: a refusal masked by the check below it (`release.yml`'s `-z`); a guard masked
by a lookup *ordering* two functions away (`layout.py:291`, #553); and a fallback masked by a
neighbouring call already answering False (`_server`'s `or SOCKET`, found by the sweep on #567).
In all three, each single mutation looks equivalent and **neither line is pinned**.

This is why most "equivalent mutant" claims will be wrong: the mutant is not equivalent, the test
asserts too little. The harness should surface *what the test asserted* alongside a survivor, so
the reviewer's first question is "did my test look closely enough" rather than "can I suppress
this".

## Six ways a sweep lies, all measured here

The first four make a mutation score **pinned** when it is not — the worst failure this tool can
have and the hardest to notice, because the report comes back green. The fifth lies in the
**other** direction, which is why it took longest to find. The sixth does not produce a wrong
answer at all: it destroys the apparatus, and the wreckage is charged to somebody else.

1. **Exit-code scoring.** Deleting `release.yml`'s `-z "$claimed"` refusal (#558) left the run
   still exiting 1 — the mismatch check below it caught the empty string instead. Same code,
   different reason, real guard gone. Score on the **set of newly-failing test ids**, never on
   the exit code.
2. **A broken baseline.** A sweep that copied the tree with `cp -R` instead of `git clone` had no
   `.git`, so twelve `test_workflows` / `test_plugin_freshness` / `test_doctor_shadowed` cases
   errored in *every* run — baseline and all 37 mutants alike. Every mutation came back `rc=1`
   and every one would have scored pinned. Two independent agents hit the identical twelve.
   **Assert a green baseline before trusting any verdict from that clone.**
3. **Stale bytecode.** A `__pycache__` left in place can mean the mutated source is never
   executed — the mutation is real, the file on disk is right, and the interpreter runs the old
   code. **This is conditional and the condition matters.** Mutating by rewriting the `.py`
   changes its mtime, which invalidates a timestamp-validated `.pyc`, so a sweep that edits
   files in place cannot hit it — measured, including a deliberately careless variant with a
   warm cache copied in, which still reddened. It *is* reachable for a `--ref`-driven sweeper
   that moves the tree with `git checkout` / `apply` / `stash`, or under hash-based `.pyc`.
   The harness is exactly that kind of sweeper, and it hit this. Set `PYTHONDONTWRITEBYTECODE`
   and clear caches between mutations rather than relying on mtime.
4. **A mutation that hangs.** A hang is not a red and not a survivor — it is *no verdict*, and
   an unattended sweep that stalls hands the next person a blank where an answer belongs. Every
   mutation needs a timeout, and a timeout must be reported as its own outcome rather than
   folded into either column.

5. **A mutation that never applied.** Its edit did not match — a quoting difference, an anchor
   that moved — so the "mutant" tree is the **unmutated** tree, the tests pass, and the guard is
   reported as a **survivor**. Found on #585, whose first table carried two of these. This is the
   only one that errs toward *more* work rather than less: it sends someone to write a test for a
   line that is already covered, and it discredits the tool the first time they discover that.
   **Every mutation must assert that it actually applied.** A verdict from an edit that did not
   happen is not a verdict.

6. **A mutation that consumes the machine.** #4's answer is a timeout, and a timeout cannot save
   a runner that has already been eaten: the timer and the process die together. Measured on
   #710 — of its 118 mutations, **four never terminate and three of those allocate on every
   pass**, the fastest at **4,274 MB/min**, which exhausts a 16 GB runner in under four minutes
   against a 900 s cap. The shard reports `exit 143` and *"The runner has received a shutdown
   signal"*, byte-for-byte a spot reclaim, so the gate charges it to infrastructure and the
   mutations in that slice are silently never measured. On run 33331151759 the one shard holding
   a memory-eater is the one shard that died; the shard that drew the mutation which spins
   **without** allocating survived on the timeout, which is the control.

   This is the only one of the seven whose distinguishing property is that the mutant takes the
   *measuring apparatus* with it rather than producing a wrong answer — and the only one that
   compounds, because the natural reading of `exit 143` is "flaky pool, re-run it", which
   reproduces the same failure on the same slice.

   **Bound it, do not predict it.** A static "does this mutant contain a runaway loop" check
   fails in the fail-open direction and that was measured too: the fastest of the four leaves
   every loop inside its own function terminating, and corrupts a **cursor returned across a
   function boundary** so that the *caller* spins. Any scanner, tokenizer or parser is mostly
   made of functions like that. Every child gets an address-space cap instead, and a mutant that
   crosses it raises `MemoryError` — a red, on tests that were green unmutated, which is a
   **pin**, the correct verdict for a mutant that destroys the process.

7. **A run that cannot finish inside the machine it was given.** #4 and #6 are one mutation
   taking the apparatus with it; this is the *plan* being larger than the fan-out can measure,
   and it needs no mutant to misbehave at all. `MAX_SHARDS x per_shard()` is a real ceiling —
   224 as this is written — and past it every shard carries more than it can measure. Measured
   on run 33500900581: 231 mutations, **eight of eight shards cancelled at the job timeout to
   the second**, and the merge step publishing `no verdict: 8 of 8 shards did not report` with
   a conclusion of `success`. The eight had between them measured something close to 224
   mutations, and all of it was thrown away, because the result set lived in memory until the
   sweep's last line.

   It is the only one of the seven where **the tool already had the answer and lost it**, which
   is why the remedy is not a bigger machine. *Write the answer down as it arrives, and stop
   before being killed.* The result file is the whole plan from the outset, every row marked
   `out of time`, rewritten as each verdict lands; a shard killed at any instant leaves the
   complete plan with holes in it rather than nothing, and a shard allowed to exit stops short
   of the runner's cap on purpose.

   **`out of time` is its own outcome and not a timeout**, for the reason #4 gives about
   folding: a timeout is *I looked and could not tell*, and a re-run may answer it; this is *I
   never looked*, and a re-run of the same plan on the same fan-out stops at the same place. A
   reader who cannot tell them apart re-runs a gate that cannot answer.

None of the seven is visible from outside. Together they are the argument for why this harness
needs its own tests and its own sweep rather than being trusted because it is short: a gate that
silently passes everything is worse than no gate, because it is *believed*.

## What makes it affordable

**A red subset proves a red full suite** — the subset is a subset, so a mutation that reddens it
necessarily reddens the whole. The full suite is therefore needed only to confirm **survivors**,
never to confirm reds.

That asymmetry is the whole cost model. Reds are the common case and they run in seconds;
survivors are rare and can afford four minutes each. It is also why selection can never be the
final word in the *other* direction: a survivor of a subset is not yet a survivor.

## Delivery

**A. The harness** — `tools/sweep.py`, stdlib only. Trace-based selection map, the mutation
operators above, survivors re-confirmed against the full suite. Runs standalone and prints a
report a human can act on. **Shipped** (#565).

**B. The baseline** — run `--all` against `main` and write the number down. This is the first
honest count of the repo's unpinned guards, and it sets what B→C has to hold flat. **Not run**:
measured at roughly 5,672 mutations and fourteen hours, which is a background job somebody
schedules, not a step on the way to anything.

**C. The gate** — a CI job on pull requests, scoped to added lines, that fails on a survivor.
Wire it only after A and B, because a gate whose baseline nobody has seen gets disabled the first
time it is inconvenient.

**Shipped before B, and the ordering above was the thing re-examined rather than ignored.** Its
stated reason is credibility, not a technical dependency — and the gate never sweeps the whole
tree, so B is not on C's critical path at all. What the reason actually asks for is that the
gate's numbers be *seen* before it can block a branch, and B is a poor way to get that: it
measures `main`'s standing debt, which is not the number a gate run produces. So the gate ships
**reporting only** — it prints its table on every pull request, says what it *would* have done,
and blocks nothing until `--enforce` is added to `.github/workflows/sweep.yml`. That makes the
gate's own baseline visible on real branches, which is what the ordering was protecting. B
remains worth doing, as a number, whenever a machine is free.

## How this gets verified

The harness's own credibility rests on reproducing findings that are already known to be true:

- It must independently rediscover the **six** guards round three added to #553 and the **three**
  on #554, from the diff alone, with no list supplied.
- It must **not** flag the guards round three correctly pinned — those tests exist and go red.
- Its own guards get swept by itself, and that run goes in the PR body.

If it cannot rediscover the known thirty-six, it does not ship. That is the acceptance test, and
it is available because three rounds of doing this by hand produced the answer key.
