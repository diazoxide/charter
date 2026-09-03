---
version: unreleased
headline: the plane is recorded as it changes, not only when you quit — and bare `charter` puts it back
---

*"We need auto-save. Now it only saves state when exiting charter from the F2 menu, and only
restores when running `reopen`. Make this configurable — and by default always save state
and always restore automatically when running just `charter`."*

The plane was written down at exactly one moment: `F2 → charter: quit`. Everything between
that moment and a crash, a killed terminal or a laptop that never comes back was gone, and
the way back was a command (`charter reopen`) you had to already know about.

It is written down as it changes now, and bare `charter` puts it back.

```
$ charter                      # after a restart, nothing running
✓ charter: restored the 4 chat(s) this plane last recorded
```

## The word is *record*, and that is not a style choice

`charter save` commits and pushes a tree. This writes one local file. If both were called
"save", every message about either would be ambiguous about which one did not happen — and
the vocabulary already existed: `reopen.write` is what writes it, and 0.55.0's own entry
says *"quitting charter records the plane"*.

## When it writes: debounced on the bumps that were already there

`state.bump` is charter's own "something changed" — every writer in the frame moves it and
every panel already polls it. Reusing it means there is no second notion of change to keep
in step with the first.

**Not on every bump.** `cmd_launch` bumps immediately after it claims a chat id and before
tmux has made the window, so a record taken there names a chat that has no window and no
harness — a plane that never existed. **Not on a timer** either: a tick can land in that same
window, and it spends work on a plane that did nothing, which is most of the time. The
record is taken when the plane has been still for two seconds, which is the consistency a
quit got for free by stopping the world first.

## Who writes: the frame process, and it cost a watch it did not have

Panels are separate processes. Six of them notice one bump, so "record where the bump was
noticed" is six processes racing one file — and a designated panel is worse, because it
stops recording the moment that panel dies, which is exactly when the record matters.

So the writer is the process holding your terminal: the launcher that attached, and
`charter reopen`'s own attach. **That process did not watch bumps before this** — only
panels did (`frame/panel.py`) — so it gains a poll it never carried, one `stat`-sized read
per chat twice a second on a thread. That cost is paid rather than avoided by moving the
writer somewhere cheaper, and there is nothing cheaper available: a bump is a file written
by another process and the standard library has no file-change notification to subscribe to,
which is why the panels poll too.

## How it writes: atomically, and now with a temp file of its own

`reopen.write` already wrote a temp file and renamed it, so a crash mid-write leaves the
previous record whole rather than a truncated one. At a hundred times the write rate that
matters more, and it exposed a second half that was missing: **`os.replace` is atomic, the
file it renames is not.** Two writers sharing one temp NAME wrote into the same bytes, and
the first to rename put the *second* writer's content in place while reporting to the first
that its own record had landed. The temp file is `tempfile.mkstemp`'s now — unique across
processes and threads by construction — and it is removed when the rename does not happen,
because a name nothing can predict is a name nothing can collect.

## What it records: what a quit records, and not one field more

Which chats existed, in which workspace, with which harness, persona and directory, and
which one was on screen. **Not panel view state** — a wrong selected row or scroll offset
fails silently and looks like a working frame, and every field added is a field that can be
stale or unrestorable.

It does not capture scrollback either: that is one `capture-pane` per chat per write, for
the one restore item that is *offered and never replayed*. What it does do is name a capture
a quit already left on disk, so a record taken while you are working never withdraws an
offer a quit made.

## What the restore says: one line, and never silence

`charter reopen` is a deliberate act, so its per-chat report is read and is unchanged. The
automatic restore happens because you typed `charter` and wanted a terminal, so it is one
line — and when part of the plane cannot come back, the rest is named on that same line:

```
✓ charter: restored 3 of the 4 chats this plane last recorded — beta.1 did not come back
```

**It deliberately does not offer `charter reopen` as a retry.** `_consume` does leave
exactly the chats still owed in the record, exactly as before — but this process is now
recording as well, so about two seconds later the record is the plane that is running and
the leftovers are gone. An operator who has just been attached to the frame this restore
built cannot type anything in that window, so a line pointing there would point at
something that is no longer there by the time it is read. The names are what the line can
carry truthfully.

Silence was never an option — that is a frame quietly drawing less than it should — and
neither was refusing the whole restore because one chat could not start, which makes one
dead workspace cost the other five.

**It fires only when there is nothing to join.** With the record now current rather than a
relic of the last quit, a second terminal typing `charter` on a running plane would
otherwise reopen every chat it can already see, and a reopened chat gets a fresh ordinal, so
nothing on screen would tell the duplicates from the originals. If anything is running
anywhere on this plane, `charter` focuses or opens exactly as it always did.

## Two keys, not one

```toml
[frame]
record  = true
restore = true
```

They are separable and the asymmetry is real: recording costs a little I/O and changes
nothing you see, restoring changes what happens when you type `charter`. Somebody may
reasonably want the plane recorded — so `charter reopen` has something to act on when they
ask — without `charter` alone resurrecting yesterday's six chats. One key makes that
position unreachable. Both ship on.

## `charter --fresh` does not participate, in both directions

Once restore is automatic, a plane you wanted to abandon comes back every time, and the only
escape would be deleting a file you have to know about first. `--fresh` is that escape: no
restore, **and no recording over the record it skipped**. Skipping only the restore is not a
smaller version of this — it is destructive, because the same run would overwrite the
skipped record two seconds later and nothing would say so until you asked for the plane back
and got the wrong one. `charter claude --fresh` says the same thing about a named harness.

## `charter reopen` now refuses a plane that is already running

This is a footgun the feature creates, closed where it is created. The record used to exist
only after a quit, so `charter reopen` never had a live plane to describe. Now it does — so
typing it out of habit after closing a terminal (which only *detaches*) would put a second
copy of every running chat on the plane, each with a fresh ordinal so nothing on screen
tells the copies apart, and for Claude Code both copies resuming one conversation.

```
✗ charter reopen: this plane is already running — reopening it would open a second copy
  of every chat, and a reopened chat gets a new id, so nothing on screen would tell the
  copies apart. Attach to what is there (`tmux -L charter attach`), or quit it first
  (`F2 → charter: quit`). Nothing was reopened, and the record is left in place.
```

It is the same rule bare `charter`'s restore already follows, read a second time rather than
a second rule — and it asks the disk before it asks tmux, because a live chat always has a
directory (`state.new_chat_id` claims its ordinal with the `mkdir`). A server that will not
answer reads as *nothing live*, which is the opposite of what a quit assumes and is right
for the opposite reason: a reopen is wanted precisely when there is no server at all.

## One sentence changed its words

The line the launcher prints when it hands your shell back said *"this plane was quit"*. It
was gated on the record naming the chat that had just ended, and while a quit was the only
writer that reading WAS a quit. It is not any more, so the sentence says what is still true
and still actionable — the plane is recorded, and one command puts it back — and it is now
also gated on the chat actually being over, so a detach from a live harness is not told its
plane is finished one line above "detached — the harness is still running".
