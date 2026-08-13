# charter

charter is a control plane for agent development: it holds the personas, workspaces and
repos a team works through, and enforces the invariants that keep parallel work from
colliding. It has no model and makes no judgements about the *content* of work — every
term below describes something charter can observe or enforce without understanding it.

## Language

### The plane

**Plane**:
The control plane — the directory holding `charter.toml`, its personas, inventory,
workspaces and config. Not a place work happens (ADR 0008).
_Avoid_: root, repo, home

**Workspace**:
An isolated per-task working context, owning clones of the repos that task touches. The
task is the unit: one workspace, one task — so a workspace running N pieces in parallel is
what "a fleet" describes, and charter stores no such thing separately.
_Avoid_: project, session, environment, fleet

**Clone**:
A repo a workspace owns, on disk under that workspace.
_Avoid_: checkout, copy

**Worktree**:
An additional working tree over a clone's object store, living outside every clone so that
build tools cannot recurse into it and `git clean` inside the clone cannot destroy it.
_Avoid_: branch dir, tree

**Persona**:
A role — Release Engineer, Forge Integration Engineer — with its own charter, memory and
vault. A persona is *who does the work*, never a queue of work (ADR 0005).
_Avoid_: agent, bot, owner, worker

**Todo**:
Workspace-scoped intent that expires: it stops being true the moment it is done or
abandoned, which is why it lives beside memory rather than inside it (ADR 0004).
_Avoid_: task, ticket, item

### Parallel work

**Plan**:
The set of piece names a task was divided into, recorded as the workspace's todos. charter
holds no other representation of it, and never judges whether it covers the task (ADR 0012).
_Avoid_: breakdown, decomposition, backlog, assignment

**Piece**:
A unit of one task that exactly one worker owns, and the worktree that holds it. A piece
*is* a worktree — `workspaces/<ws>/.worktrees/<repo>/<piece>` — not a separate record that
points at one. Scoped to a single repo: a task spanning two repos is two pieces.
_Avoid_: task, chunk, shard, work item, unit

**Claim**:
One worker's exclusive ownership of a piece, established by that worker creating the piece's
worktree. Git is what makes it exclusive, and the claimant is always the creator — a piece
created for somebody else is not claimed by anyone.
_Avoid_: lock, lease, reservation, assignment

**Declaration**:
A statement only the worker can make, because it is a judgement rather than a fact on disk.
An outcome is the only declaration there is.
_Avoid_: report, update, signal

**Observation**:
Something charter can see without being told — that a piece's worktree exists, that a
worker was alive at some moment. Observations are never judgements, which is why charter
may record one about a worker that never speaks to it, and why it may not conclude anything
from their absence (ADR 0009).
_Avoid_: check, probe, ping

**Worker**:
One live session holding one piece. Distinct from a persona (the *role* it may be acting
as) and from an agent (a *generated sub-agent definition*, which `charter report` counts by
name) — three things the same word used to cover.
_Avoid_: agent, sub-agent, runner, bot

**Outcome**:
What a worker declares became of its piece: `done`, or `abandoned` with a reason. An outcome
is a statement git cannot make on its own — commits and branches show what a piece *left
behind*, never whether the worker considered itself finished.
_Avoid_: result, status, state, report

**Silence**:
A piece with a claim and no outcome. Not a third outcome but the absence of one, and the
shape every undeclared failure takes — denial, timeout, or a killed session alike. Silence
has an *age*, which is the only thing charter says about it: the cause is never inferred.
_Avoid_: stuck, failed, timed-out, orphaned, dead
