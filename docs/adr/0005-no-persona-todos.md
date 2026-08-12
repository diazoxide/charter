# Todos are workspace-scoped only — personas do not get them

Personas have their own committed memory, their own vault and their own charter, so the
symmetry is tempting: if a workspace has a todo list, why not a persona? Deliberately not,
and this is written down because the symmetry argument will be made again.

A persona is a **role** — Release Engineer, Forge Integration Engineer — not a worker with
a queue. Work is owned by a workspace, which charter already defines as an *isolated
per-task* workspace; a persona is *who does it*. Role-level "should get around to it" items
are a backlog, and charter already has two homes for those: `charter report gap` and the
issue tracker.

The cost of a second scope is not disk, it is coherence. Two scopes means every write needs
a "which one?" decision, and an agent that guesses wrong scatters intent across two stores
that are never read together — so neither list is trustworthy, which defeats the point of
having one.

## Consequences

Reversible if evidence appears: the bar is three real persona todos that are not workspace
todos and not issues. Until then the absence is a decision, not an oversight.
