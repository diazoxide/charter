---
version: unreleased
headline: A chat keeps the harness session id a resume will need, without the context gauge ever reading a stale one
---

Charter records the harness's own session id at `.charter/frame/<fid>/session`, written by
Claude Code's `statusLine` hook. It is the only thing charter records that could ever ask a
harness for its conversation back — and `state.clear_shape` deletes it on every launch that
claims a chat id, with its reason written at the line: *"a gauge reading somebody else's 78%
is worse than either."*

**Both requirements are right, and one file was serving two purposes.** The usage history
the context gauge reads is keyed by that session id and lives *outside* the frame directory,
so deleting the mapping is the only thing suppressing a stale gauge today — that argument is
about a number drawn on screen and it stands. It is not an argument about the identifier.

So the id is written twice and deleted once. `record_harness_session` writes a durable
sibling, `session.durable`, from the same value on the same branch; `clear_shape` does not
list it. The two can only ever disagree by the sibling being *absent* — never by holding a
different id.

**Nothing reads it yet, and that is the stage rather than an omission.** Reopening a chat
into its own existing directory is what reads it, and the id cannot be written retroactively
for a chat whose `session` a launch has already cleared — so this has to land first, on its
own, ahead of everything that uses it.

**The ordering hazard, stated here so the next person does not have to find it again.**
Keeping the id is "no behaviour change" only while nothing relaunches into an existing chat
directory. The moment a reopen does, `session` comes back and the gauge starts reading a
history that belongs to a run that is over. The gauge's gate — `state.exit_code(fid) is
None`, one `stat`, already written and already correct — **must ship before reopen, not with
it**. It is deliberately not in this change: a gate is a behaviour change and this is not.

One thing that is neither back-filled nor guessed at: a chat whose harness was already
running when this charter was installed keeps a `session` written by the previous version,
so the sibling first appears when that harness's own session id next changes. Reading
`session` as a stand-in would be reading exactly the file whose deletion this pair exists to
survive.

Nothing to adopt — no production behaviour changed, and no surface draws anything new.
