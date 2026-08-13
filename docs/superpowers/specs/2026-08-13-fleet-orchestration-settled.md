# Fleet orchestration — settled ground

**Date:** 2026-08-13 · **Status:** grilled, pre-spec. Input to `/to-spec`.
**Supersedes** [`2026-08-13-fleet-orchestration-handoff.md`](./2026-08-13-fleet-orchestration-handoff.md),
whose open questions are now answered.

The vocabulary is in [`CONTEXT.md`](../../../CONTEXT.md). The decisions that were hard to
reverse are in [ADR 0011](../../adr/0011-the-record-holds-only-what-git-cannot-know.md) and
[ADR 0012](../../adr/0012-the-plan-is-todos-the-lifecycle-is-the-pieces.md). **Those three
files are the authority** — this one carries only what did not belong in them, and does not
restate their reasoning.

---

## The shape, in six lines

A **plan** is workspace todos: piece names, nothing more. A **worker** is one session that
claims a **piece** by creating its worktree, which is how exactly one worker gets it. It
declares `done` or `abandoned` when finished; a hook records that it was alive, so a worker
that dies leaves **silence** with an age rather than looking like success. charter reports
all of it and diagnoses none of it. The agent plans; charter enforces.

## What charter guarantees, and what it does not

Guaranteed: no two workers hold one piece; every claim and declaration is recorded; a piece
that declared nothing is visibly silent, with an age.

**Not** guaranteed, and this is the honest edge of the design: that the plan covers the task.
charter has no model, so it cannot know an eighth piece was needed. ADR 0012 shrinks the gap
— a todo with no piece is computable — but only for plans that were written down. Any spec
language promising that "nothing drifts and nothing is missed" must say *declared* pieces, or
it is over-promising.

## Settled, but not ADR material

**Scope of the first cut.** The outcome spine alone: hook-recorded liveness, `done` /
`abandoned`, the log, the read surface, the SessionStart announcement. **Issue #91 is fixed
first, as a separate ticket** — `workspace remove` destroys worktrees silently, a fleet
multiplies worktrees, and the bug is already real at N=1.

**Second cut:** the todo↔piece linkage, then same-file overlap detection. Overlap started in
the first cut and was moved out during grilling: it needs a merge-base per repo and shares
nothing with the outcome spine.

**Out of scope, deliberately:** fan-in (sequencing merges, rolling N branches into one PR),
and multi-host fleets. If multi-host ever returns, reopen git-refs-as-claims — ADR 0011 says
why it was set aside rather than rejected, so it should be reopened, not reinvented.

**Verbs.** Claiming stays `charter worktree add <repo> <piece>`, which now also writes the
claim event. Declaring is `charter worktree done` and `charter worktree abandon "<reason>"`
with **no piece argument** — the piece comes from cwd via `worktree.locate()`, since every
argument is a chance to name someone else's piece. Reading is the existing
`charter worktree list` plus a history flag; no new command tree, because there is no fleet
noun to hang one on.

**Losing a claim** must be distinguishable without parsing English: a classified error (the
cause *is* recognised — the path or branch exists — so ADR 0009 permits naming it) and a
distinct exit code, so a worker can take the next unclaimed name from the plan.

**Hand-made worktrees** keep working. A tree created with plain `git worktree add` has no
claim event and reports **claimant unknown**. It is never refused — `worktree.py`'s stance
that hand-made trees are first-class is not weakened by this design.

**SessionStart** is where a worker learns it holds a piece and owes a declaration, by the
same mechanism that already announces workspace, persona and todos. State the verbs
literally. No nagging at turn end — liveness is already automatic, and ADR 0008 records what
becomes of a warning you can work through.

**Status line.** Annotate the worktree detail rows it already draws, plus one summary cell on
the workspace line: *"8 pieces · 5 done · 1 abandoned · 2 silent (oldest 3h)"*. No fleet
block. **The join is directory-scan × log, never `git worktree list`** — the status line's
contract forbids a git subprocess, and the obvious implementation reaches for one. Lands on
the `statusline` persona.

**A second session entering a held worktree** is reported, not refused — ADR 0008's trade,
knowingly repeated. A *resumed* session must not warn about itself.

## Still open

Nothing in the design. Two things for the spec to decide, both mechanical:

- The log's file layout. `dispatch.py`'s pattern is the model (`O_APPEND`, host in the
  filename, no locks); what is undecided is one log per workspace versus one per repo.
- Whether the history read is `charter worktree list --history` or its own subcommand.

## Route

Spec for the first cut is filed as **[#95](https://github.com/diazoxide/charter/issues/95)**
(`ready-for-agent`, `gap`), with **#91** as its prerequisite. It is the single source for the
spec — this file is not a second copy of it, deliberately.

`/to-tickets` on #95 next, then `/implement` per ticket with `/clear` between. Keep tickets in
one context window with #95, `CONTEXT.md`, and ADRs 0011–0012 loaded; that is the whole of the
settled ground, and it is deliberately small enough to carry.
