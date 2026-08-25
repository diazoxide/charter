---
version: unreleased
headline: A clone and a gl-refresh move the frame's spinner too, because records now say what they are
---

The frame's one moving thing was promised for "a dispatch, clone or `gl-refresh`" and
delivered dispatches. Not an oversight: `inflight.start` had exactly one caller, and the
reason nobody added the other two is worth reading, because it is the whole of this
change.

The same records feed the **dispatch-overlap nudge** — the one that tells you a peer
persona is already editing the tree you are about to hand to another one. That nudge reads
its answer back as a sentence. A record named `clone` dropped into the same tracker would
have produced *"`x` writes code and `clone` are already running"*: wrong, and wrong in the
confident, human-readable way that is worse than saying nothing.

**So every record now carries a kind, and every reader says which kinds it means.** A
`charter clone` records one per repo (eight parallel clones read as eight); a `gl-refresh`
records one for the workspace it is fetching, in the detached child that does the work
rather than the parent that starts it. The frame's bottom row counts records and never
names them, so all three are simply `⠙ 3 running` there.

**The default is dispatches, and the default is the guard.** The readers that put a name
in front of you — the overlap nudge, the `⚡` badge on a persona's chip, this session's own
`⚡ N` — get dispatches by *not asking*. The frame's spinner is the single caller that opts
into "anything live". That way the next kind of work charter learns to record cannot leak
into a sentence by being forgotten at one call site; it would have to be let in
deliberately.

Two smaller things fell out of it, both about records that outlive the charter that wrote
them — the tracker keeps one for a day, so an upgrade mid-dispatch is ordinary rather than
exotic:

- A record with no kind reads as a dispatch, which is what it was. Anything else would
  drop a genuinely running peer out of the nudge that exists to catch it.
- `finish` matches on the kind as well as the name. The file name carries only the agent,
  so a clone of a repo called `steward` and a dispatch to a persona called `steward` were
  indistinguishable — whichever ended first would have retired the other's record, clearing
  a true one and leaving a false live one behind.

Nothing to adopt, and nothing to clean up: records already on disk keep working in both
directions — this charter can still find one an older charter wrote, and an older charter
can still find one this charter writes.
