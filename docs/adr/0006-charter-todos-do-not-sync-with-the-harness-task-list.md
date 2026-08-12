# charter's todos do not sync with Claude Code's task list

Claude Code ships its own per-session task list. charter's todos are a separate,
persistent, workspace-scoped list, and the two are never synchronised in either direction.
*"Why doesn't charter update the task list I can actually see?"* is the most natural
question anyone will ask about this feature, so the answer is recorded rather than left to
look like an omission.

Two live task lists in one session is strictly worse than one. The agent updates whichever
it last thought about, they drift, and both stop being trustworthy — at which point the
durable one is worth less than no list at all, because it is confidently wrong about what
is left. The two are kept honest by giving each exactly one job: the harness's list is
**session scratch** and owns `in_progress`; charter's is **durable intent** across sessions
and knows only open/done.

Mirroring at session start was considered and rejected. Two stores syncing bidirectionally
is a bug factory, and the failure is silent: an item ticked in one place and not the other
produces a list that lies without anyone noticing.

## Consequences

charter's list would be write-only if nothing surfaced it, so SessionStart injects a
**bounded** signal — an open count plus the three oldest titles — into a context budget
that is already defended (`_memory_digest` is commented as "a BOUNDED digest, not the whole
index"). One-way and bounded is the whole compromise. An open count also sits in status-line
zone one, beside the workspace it belongs to.
