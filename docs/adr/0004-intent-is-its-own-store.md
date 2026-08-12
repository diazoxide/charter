# Intent is its own store, not a state flag on memory

A workspace's todos live in `todos/`, beside `memory/` rather than inside it — a third
category alongside **memory** (what was learned) and the **journal** (what happened). The
obvious cheaper designs were both rejected.

The distinguishing property is that intent **expires**. A memory is true indefinitely; a
todo stops being true the moment it is done or abandoned. `memstore` exists precisely
because durable facts are worth keeping, indexing and searching forever, and putting
something that goes stale inside it would rot the half that must not go stale — a
`charter recall` hit on a todo somebody completed last month is worse than no hit, because
it reads with the same authority as a fact.

Free text in `workspace.md` was rejected for the opposite reason: a markdown blob gets no
search, no duplicate detection, and no way for an agent to close a single item without
rewriting the whole file.

## Consequences

`todos/` is a third instance of the `memstore` pattern, so it inherits the index, keyword
search, near-duplicate detection and forget-by-slug essentially free. It also needs its own
line in `workspace._live_block`, which currently un-ignores only `workspace.json`,
`workspace.md`, `memory` and `memory/**` — without it, todos would be invisible to git even
in a LIVE workspace.
