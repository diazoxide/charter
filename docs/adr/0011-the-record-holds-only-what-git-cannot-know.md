# The record holds only what git cannot know

Parallel work needs two things charter does not have: a **claim**, so two workers cannot
take the same piece, and an **outcome**, so a fleet that completes seven of eight cannot
report success. The obvious design for both is a ledger in workspace state. `worktree.py`
forbids exactly that, in its opening docstring, on the grounds that a marker which can
disagree with reality is a failure mode this repo has already paid for.

The decision is a split, not an exception:

- **A claim is the piece's worktree existing.** A piece *is* a worktree, and `git worktree
  add` already fails for the loser when two workers race the same path or branch. Assignment
  is therefore atomic with no new state and no new command — git's own lock is the mutex.
- **An outcome is written down**, because it is a fact git has no opinion on. A branch with
  three commits and no further activity looks identical whether the worker declared itself
  finished, hit a permission denial, or was killed. There is no reality for that record to
  disagree with, because nothing else records it.

So the rule survives in a sharper form: **git is the only registry of what exists; the
record holds only a worker's declaration.**

## The claimant is the creator, which forbids pre-assignment

The atomicity above is only real if the worker that will do the work is the process that
creates the worktree. An orchestrator that pre-creates eight worktrees and then starts eight
workers inside them has won a race that was never run — one process, creating sequentially —
while the race that matters, two workers entering the same tree, goes unguarded. The mutex
would be decorative.

So the plan declares piece *names* (ADR 0012) and each worker creates its own worktree,
taking the name or losing it. An orchestrator writes the plan and starts workers; it does
not hand out assignments. Nothing enforces this — it is a property of how fleets are driven,
not of the code — which is exactly why it is written here rather than left as an assumption
inside a design that silently depends on it.

Two sessions entering one existing worktree remain possible and are **reported, not
refused** — the same trade ADR 0008 made for the plane root, for the same reason, with the
same known cost.

## What this forbids

The discipline is the whole value, and it is invisible in the code that follows it — a
record with one extra field looks like a convenience, not a regression. The record must
never store a fact that is derivable: which worktrees exist, which branch a piece is on,
whether it is pushed, whether it is dirty. All of it is read from git at read time, every
time, exactly as `worktree.py` already does. The moment a derivable fact is cached for
convenience, this ADR has been reversed whether or not anyone says so.

## The record is events, not state

What makes the identity of a claim recordable at all is that the record is an **append-only
log of things that happened**, never a description of how things are. *"Piece P claimed at T
by X"* is a fact about the past and cannot become false; *"X holds P"* can, the moment the
worktree is removed by hand. Only the first form is written.

The present tense is reconstructed at read time: git answers which worktrees exist, and the
log is joined to that answer to say who took each one and what they declared. Nothing is
believed about a piece git no longer sees. That join is what allows a written record to
carry identity without becoming the marker the docstring forbids, and it is the reason the
log may never gain a "current status" field, however convenient.

The corollary is that the log and git answer **different questions** and must not be made to
answer each other's — *what is running here* comes from git, *what happened here* comes from
the log. ADR 0010 is the same lesson learned the expensive way with the manifest and the
directory scan.

That corollary immediately splits the record in two. Declarations are rare — at most three
lines per piece — and belong in the log forever. Liveness is not: it is written by a hook
that fires every turn, so appending it would bury three meaningful lines per piece under
thousands of "still alive" ones and make the log unbounded. **Liveness therefore lives in a
small per-piece file that is overwritten, not in the log.** Overwriting does not breach the
rule above: *"last seen at T"* is a past observation, not a cached derivable fact, and there
is no reality it can contradict. What it discards is heartbeat history, which nothing needs.
Two files, two questions, once again.

## Considered options

**A claims ledger with leases** (append-only records, reconciled against a plan) was the
leading design until the docstring was read. It is the marker the module forbids, and it
fails the same way: a lease outlives the worktree it describes.

**Deriving everything from git**, with nothing written at all, cannot represent a
declaration. It was rejected for the case it cannot see — a dead worker and a slow one are
identical — which is precisely the failure the work exists to fix.

**Git refs as claims** (`git update-ref refs/charter/claim/<piece>` for compare-and-swap)
was the most promising option before the split above. It is not wrong; it is redundant.
Once a claim *is* a worktree, refs add a second namespace to buy atomicity git already
provided. Its real advantage — a pushed ref is visible on every machine — only pays off
across hosts, which is out of scope for now. **If multi-host fleets are ever in scope, this
is the option to reopen**, and it should be reopened rather than reinvented.

## Consequences

**A piece nobody started is invisible to charter.** Claims are worktrees, so a piece that
was planned and never begun has nothing to observe. "Nothing will drift and be missed" is
therefore true of *declared* pieces only; coverage of the plan is not something a program
with no model can check, and this ADR does not pretend otherwise.

**The absence of an outcome is not a state.** There is no `failed`, no `timed-out`, no
`blocked` — a worker that dies declares nothing, and inventing a state for it would mean
charter asserting a cause it never verified, which ADR 0009 rules out. What charter can say
is that a claim exists and nothing was declared. Naming that condition *silence* is what
makes it reportable instead of indistinguishable from success.

That extends to elapsed time: **no threshold ever converts silence into a verdict.** charter
reports how long a piece has been claimed and how long since its worker was last observed,
and stops there. A piece marked `failed` that was in fact running a forty-minute test suite
is ADR 0009's exact failure one level up — a confident wrong answer tells the reader to stop
looking. Somebody will propose a staleness threshold; this paragraph is the answer.

**An abandoned piece keeps its name.** Freeing a piece for a retry would mean removing its
worktree, and `charter worktree remove` deliberately refuses to lose uncommitted or unpushed
work — so an abandoning worker often *cannot* remove it, and forcing the removal would
destroy the evidence of why it gave up, which is the most useful thing it produced. Handing
the tree to a second worker is worse: it inherits a state nobody characterised. So an
abandoned piece stays on disk carrying its declaration, and a retry is a new piece. The cost
is worktree sprawl needing curation, which is issue #93 rather than a new problem.

**The claim carries no identity on its own.** A worktree's existence proves someone took the
piece and says nothing about who, since git records an author only once a commit lands.
Visibility therefore depends on the written record covering the claim as well as the
outcome — the same record, under the same restriction.
