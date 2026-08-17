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

**Harness**:
The agent runtime charter runs inside — Claude Code, opencode, Codex. Charter enforces the
same invariants on every harness; what differs is what it can *offer*, and `charter doctor`
names each gap rather than leaving it to be found (ADR 0015).
_Avoid_: host, client, runner, IDE, platform

**Host**:
A forge host — `github.com`, a self-hosted GitLab. Never the agent runtime: both senses of
this word were load-bearing at once until ADR 0015 split them.
_Avoid_: harness, server, remote

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

## Prose

The glossary governs words; this governs sentences. It is written down because docs drift
gradually — no single paragraph looks wrong, and a year later the README sells instead of
explaining. Every rule below is a pattern already in these docs, recorded so the next
person or session can follow it without having read all of them first.

**Name the failure, not the feature**:
A heading says what went wrong often enough to get built around — "Two sub-agents that need
the same repo" — and the body answers it. A reader recognises their own bad afternoon in a
failure; nobody recognises themselves in "Worktrees".
_Avoid_: Features, Key benefits, Capabilities, any heading that is only a noun

**Make every claim checkable**:
Reach for the number, the path, the flag, the file mode: "plaintext JSON at file mode 0600"
carries what "stored securely" does not. A sentence that could move to another project's
README unchanged is carrying no information.
_Avoid_: secure, robust, powerful, seamless, blazing, simply, just, easily

**State the limit at full volume**:
What a thing does not do belongs in the same breath as what it does. "The vault is not a
password manager" is the sentence that earns the rest of that section its trust. A
limitation the reader discovers alone was concealed, however honestly it was omitted.
_Avoid_: note that, please be aware, a caveats section at the end

**Explain the why — the what is already on screen**:
A paragraph beside a command earns its place by naming the failure that command prevents,
never by restating it. The same rule governs code comments, which is why the good ones read
as records of things that went wrong.
_Avoid_: This command will, As you can see, In other words

**Describe the reader's day, not the abstraction**:
"Two features and a hotfix means three sets of branches, and if they share a checkout you
spend the day stashing" — second person, concrete nouns, an afternoon the reader has had.
An abstraction says the same thing while being impossible to disagree with.
_Avoid_: workflow, productivity, overhead, friction, streamline, leverage, empower

**A refusal is the rule working**:
Where charter denies something on purpose, say so plainly and name the fix in the same
breath. A reader who does not know the rule reads the denial as a bug and files it.
_Avoid_: error, failure, blocked — for anything charter did deliberately

**Never tell the reader how to feel**:
The evidence goes on the page and the reader draws the conclusion. No exclamation marks, no
promise that something is easy.
_Avoid_: !, amazing, incredible, you will love, it is that simple

One rewrite, for calibration:

> **✗** charter provides powerful workspace isolation, seamlessly enabling developers to
> effortlessly manage concurrent tasks and boost productivity.

> **✓** Two features and a hotfix means three sets of branches across a shifting set of
> repos, and if they share a checkout you spend the day stashing. A workspace is one
> directory of clones per task, each repo on its own branch.
