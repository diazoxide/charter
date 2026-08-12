# todos

> **Living charter** for this workspace — its north star and shared context.
> Keep it current as the work evolves (edit this file, or `charter workspace vision "…"`).
> It's committed + shared for LIVE workspaces, and a fork inherits it — so anyone
> can pick up the task with full context. Never put secrets here (vault only).

## Vision

Give charter a third category alongside memory and journal: intent. A workspace keeps a persistent, scoped todo list that survives across sessions — what this task still means to do, as against what it learned (memory) or what already happened (journal). Workspace-scoped only; personas are roles, not workers with queues.

## Context & decisions

<!-- Key facts, constraints, and design/architecture decisions found while working —
     the durable "why", not a chronological log. Grow this as you learn. -->

**Intent is a third store, not a flag on memory.** The distinguishing property is that
intent *expires*: a memory is true forever, a todo stops being true the moment it is done
or abandoned. Mixing something that goes stale into a store whose entire premise is
durability would rot the durable half. Free text in `workspace.md` was rejected for the
opposite reason — a markdown blob gets no search, no dedupe, and no way for an agent to
close one item without rewriting the file.

**Workspace-scoped only. No persona todos.** A persona is a *role* with committed
knowledge and a scoped vault, not a worker with a queue: work is owned by a workspace,
and a persona is who does it. Role-level "get around to it" items are a backlog, which
`charter report gap` and the issue tracker already serve. The cost of a second scope is
not disk, it is coherence — every write would need a "which one?" decision, and an agent
guessing wrong scatters intent across two stores nobody reads together.

**charter's list is the durable one; the harness's stays scratch.** Claude Code ships a
per-session task list, and two live lists in one session is strictly worse than one — the
agent updates whichever it last thought about and both stop being trustworthy. The
differentiator is persistence *across* sessions, which the harness list genuinely lacks.
Bidirectional syncing was rejected outright as a bug factory; a bounded one-way surfacing
at SessionStart is what keeps charter's list from being write-only.

**Either party may add; completion leaves a trace.** An agent that can both create and
silently tick off its own todos has a list that always looks finished. Requiring a human
to close each one was rejected as friction that would kill adoption — most todos are
agent-generated mid-task and closing them is mechanical. Logging the completion into the
existing journal buys the same trust far more cheaply.

**Done means deleted, plus a journal entry.** The journal already *is* the permanent
record of what happened; retaining done todos builds a second one you must then filter
past to see the live list.

**States are open/done only.** `in_progress` is a *session* concept, correctly owned by
the harness's ephemeral list. A durable cross-session list that has marked something "in
progress" for three weeks is simply lying — a failure observed live in the session that
designed this.

**Open todos never expire, but show their age.** The 30-day GC shipped for unsent report
drafts is safe precisely because a draft is a draft — nobody committed to it. An open todo
is a decision a human made, so deleting it silently deletes their intent. Age on the
display gets the same benefit honestly.

**Todos travel with a LIVE workspace, in their own directory.** Nesting them under
`memory/` would ride the existing gitignore mechanism for free, but re-merges on disk the
two things separated above, and the confusion would surface in `charter recall` results.
Always-LOCAL was rejected too: a shared todo list is one of the better arguments for
making a workspace LIVE at all.

**Oldest first, and that is the only ranking.** The three todos surfaced at SessionStart
are the oldest open ones, which makes the injection self-correcting: what shows up is what
is being avoided. An explicit priority field was rejected as something every writer must
decide and most would leave blank — and age is already the staleness signal, so reusing it
avoids inventing a second, competing ranking.

**Plain text, no reference fields.** A todo saying "fix the label failure in
`create_issue`" carries its own pointers well enough for a human and an agent both. Every
ref scheme is a small ontology that must be kept true as paths get renamed and issues get
closed, and a stale ref is worse than no ref because it looks authoritative. Refs can earn
their way in later on evidence from real todos.

**A fork inherits open todos.** `workspace fork` exists so anyone can pick up a task with
full context, and what is still to be done is the most actionable part of that context.
Done todos never arise, since completion deletes.

**Open todos are named at removal, but do not block it.** `workspace remove` refuses to
discard unpushed work; making todos trigger that same refusal would overreach, because a
workspace whose todos are all abandoned is exactly the one worth deleting. Forcing
`--force` for that case trains the habit of reaching for it, which is how a guard stops
protecting the commits it actually exists for. Naming them gives the human the fact
without diluting the guard.

**Duplicate detection is reused from `memstore`, warn-and-skip.** An agent re-reading a
workspace across sessions regenerates near-identical todos, and duplicate intent is worse
than duplicate memory: completing one leaves its twin looking outstanding, so the list
starts lying about what is left. Warning rather than silently merging teaches the writer
the item already exists.

### Constraints found in the code

- **`memstore.py` is already dual-scoped and does most of this.** Its docstring: *"Shared
  by **persona** memory (role knowledge) and **workspace** memory (task journal) so both
  behave the same."* One file per fact, a `MEMORY.md` index, keyword search, near-duplicate
  detection, forget-by-slug, and optional `timestamped` filenames. A third store is close
  to free mechanically — which is an argument for reusing it, not for merging into it.
- **`_live_block` un-ignores exactly four paths per LIVE workspace** (`workspace.py:509`):
  `workspace.json`, `workspace.md`, `memory`, `memory/**`. A `todos/` directory is
  invisible to git until it gains a fifth line there.
- **The SessionStart injection budget is tight and defended.** `charter/hooks.py` already
  stacks version sync, workspace nudge, persona role, memory digest and uncommitted-memory
  nudge, with `_memory_digest` commented as *"a BOUNDED digest, not the whole index"*.
- **The status line's own rule is "a count lives next to what it counts"**, and zone one is
  *where I am* — the active workspace and how many exist.
- **`todo` appears zero times in charter's source today.** Entirely net-new surface.

## Glossary

<!-- Task/domain vocabulary so a teammate or a fork isn't lost: `term` — definition. -->

- `intent` — the category this work adds: what a workspace still means to do. The third
  alongside **memory** (what was learned) and **journal** (what happened), and the only one
  of the three that can be made false by being acted on.
- `todo` — one item of intent, belonging to exactly one workspace. Open or done, nothing
  else.
- `memory` — a durable fact, true indefinitely. Charter's existing store; explicitly *not*
  where todos live.
- `journal` — the timestamped record of what happened (`memory/notes.md`, written with
  `charter workspace note`). Where a completed todo goes to leave its trace.
- **`task` is unavailable.** charter already defines a workspace as an *"isolated per-task
  workspace"*, so "task" means the whole workspace-sized unit of work. A todo is smaller
  and must never be called one — hence `todo` for the item and `intent` for the category.
- `the harness list` — Claude Code's own per-session task list. Ephemeral, session-scoped,
  and deliberately left alone; it owns `in_progress`, charter's list owns persistence.
- `work at risk` — charter's existing term for what `workspace remove` refuses to discard:
  uncommitted or unpushed commits. Open todos are reported alongside it but deliberately do
  not join it, so the guard keeps meaning what it means.

## Log

Chronological "what was done" lives in the task memo — `memory/notes.md`
(append with `charter workspace note "…"`).
