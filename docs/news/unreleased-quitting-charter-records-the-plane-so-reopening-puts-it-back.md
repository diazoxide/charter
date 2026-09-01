---
version: unreleased
headline: Quitting charter writes the plane down first, so reopening puts it back — and says, per chat, what it cannot
---

*"when user closing charter session we need to show warning, that all harness sessions will
be closed, and when opening again we need to resume harness sessions as well. so for user it
should be 'less invasive' — to not lose sessions, state etc."*

The operator restarted their machine, got an empty frame, and recovered their conversation by
typing `/resume` by hand. Three verbs were missing and none of them existed anywhere:
`charter --help` had ten `frame-*` commands and neither a quit nor a close, and the palette
registered detach, two repo-selection rows, three densities and three surfaces.

**All three ship here.**

* **`F2 → charter: quit`** stops every harness on this plane — after writing the plane down,
  and after showing you, one row per chat, what will and will not come back.
* **`F2 → chat: close`** stops one chat and marks it so nothing brings it back.
* **`charter reopen`** puts the recorded plane back: every workspace, every chat, each one's
  persona and directory, and — for Claude Code — the conversation, by resuming it.
* **`F2 → chat: previous transcript`** opens what a chat had on screen before it was
  stopped, in a pager, in a window of its own.

## The record has to survive the thing that deletes chats, and one already did not

Charter has kept a chat's harness session id since #757 — `session.durable`, deliberately not
in `clear_shape`'s list, so the id a resume needs survives a frame ending. **Measured, it does
not survive a restart**, and the reason is one line up: `state.reap` deletes a chat's whole
directory when its launcher pid is dead, and after a restart *every* launcher pid is dead. So
the id was gone by the time anything could ask for it — deleted by the next launch, in a
directory nobody had noticed was temporary.

Inverting `reap` is the fix, and it is not a fix that fits in this change: it is stage 4 of
the design's own delivery order, six edits wide (`chat: close`, a `max_chats` cap and its
config field, a ghost-tab collector, `cmd_workspace_remove`'s leak, `state.clear_claim`) with
~60 tests asserting the current rule, four of them by name.

So the record is a **file**, not a directory:

```
.charter/frame/
  alpha.1/                  <- a chat: still a liveness marker, still reaped exactly as now
  alpha.1.transcript        <- what was on its screen
  reopen.json               <- the plane
```

`reap` skips anything that is not a directory, and so does the scan that reads the tab bar —
so a file there is invisible to both, outlives every reap, and is never mistaken for a chat.
**The chat directory goes on meaning exactly what it means today**, which is what makes this
shippable ahead of the redesign rather than after it.

*Recorded because it changes what a test can prove:* that durability turned out to be
**over-determined**. Three independent rules in `reap` keep a non-directory entry — the
`is_dir()` filter, the "a directory I cannot list is one I have no reading of" keep-rule (a
`NotADirectoryError` is an `OSError`), and `shutil.rmtree(ignore_errors=True)`, which cannot
remove a file even if it were reached. Deleting the filter leaves the suite green. The tests
assert the outcome the design rests on and say so rather than claiming a guard they cannot
turn red.

## What quit does, in order, and why the order is the feature

1. **Read the plane off disk** — every chat directory on *this* plane, with its workspace,
   persona, harness, directory and durable session id.
2. **Ask each tmux server which of them is live**, and which window each one is in.
3. **Capture each live chat's scrollback** while its pane still exists.
4. **Write the manifest.**
5. Drop the transcripts of chats the manifest no longer names.
6. **Kill** — one `kill-window` per chat.
7. **Prune the in-flight tracker.**

**4 before 6 is the whole point.** A record that depends on the thing it records succeeding
is not a record, which is the rule `charter trace` already keeps for a secret: it writes
before the value leaves. A quit that could not write the manifest **refuses**, and stops
nothing — the one thing that stops a quit, because killing a plane nothing will bring back is
the invasive quit this change exists to prevent.

**7 after 6, for the mirror of the same reason.** `inflight` records clear only on
`finish()`, so stopping every harness strands every record: `still_running` reports one for 30
minutes and `live` holds it for 24 hours, which would leave the frame's spinner animating for
a plane doing nothing and the dispatch-overlap nudge naming agents the quit itself killed.
Pruning first would discard the record of work that is still running if a kill then failed.

**The prune is plane-wide, and it can be, because the quit is.** Those records carry no chat,
no frame and no workspace, so nothing narrower is expressible — and it does not need to be:
the quit stops every chat this plane has a directory for, which is every process that could
have written one. A genuinely per-frame quit could not prune at all.

## The warning is per chat, and it is drawn before the keypress commits anything

`F2 → charter: quit` opens a confirmation, not an action. The row that goes through, and
under it one row per chat carrying that chat's own sentence — every chat row marked refused,
because it is the warning and pressing it does nothing.

```
> quit — stop 4 chats in 3 workspaces; 2 of 4 can resume the conversation
  alpha.1 · claude-code    conversation resumes
  alpha.2 · claude-code    reopens empty — no session id recorded for this chat yet
  api.1 · opencode         reopens empty — opencode records no session id to resume from
  gone.3 · claude-code     conversation resumes · workspace 'gone' is gone — reopens saying so
```

*An earlier draft put the confirming row at the bottom, under the list it is about, which
reads better and was measured to be wrong:* `palette.narrow` puts the cursor on the first row
when nothing has been typed, refused or not, so the surface opened with a chat row selected
and Enter bound to nothing at all. A palette row that visibly does nothing reads as broken,
and this is the one surface where the operator has just asked a question and is waiting for
the keypress that answers it.

**Four sentences and not one**, because "reopens empty" on its own would reasonably be filed
as a bug. Charter can resume Claude Code and nothing else: `record_harness_session` has
exactly one caller, Claude Code's own status-line hook, so no other harness has ever written
an id to ask with. The row says which of the four reasons applies to *that* chat — it has an
id, charter does not know what harness it was, it is Claude Code and has not taken a turn yet,
or its harness records no id at all — and it is asked of the harness registry rather than
written down, so the day a second harness starts writing an id there is one place to change.

It **warns and proceeds**. Refusing would leave you unable to quit while any agent was
working, which on a control plane is most of the time.

## A chat that cannot be resumed still comes back, empty, and says so

Its directory, its workspace and its persona return either way; only the conversation is gone.
Silently not reopening it would make a chat vanish across a restart, which is the opposite of
what was asked for. A chat whose **workspace has been deleted** comes back too, and says the
workspace is missing — it is never quietly re-homed, which is the rule §4j sets and #789
enforced.

**One chat cannot come back, and it says so rather than promising.** A chat with no
*workspace record at all* — the migration and truncation case, which is exactly why the quit
scans the frame root rather than asking any workspace's roster — has no session to be rebuilt
into. It is stopped like the rest and deliberately not recorded, because choosing a workspace
for it would be the re-homing §4j forbids arriving as a convenience, and a manifest line every
reader discards is a record that looks like a promise and is not. Its warning row says
`charter has no record of its workspace — it cannot be reopened`, and the quit's own summary
says how many of how many were kept.

**The workspace a chat comes back in is the one recorded, and #791 is what makes that a
fact.** A reopen mints a fresh chat id — but `new_chat_id` counts up from 1 and `reap` frees
the ordinals a quit's chats held, so it very often gets the same *name* back. While the
per-session `.charter/sessions/<id>.workspace` pointer was a rung of `state.own_workspace`, a
`charter workspace use` typed in the chat that previously held that ordinal would have
outranked the manifest for the chat that inherited it. That rung is gone, and the case is
pinned with the stale pointer deliberately planted.

**`cwd` had nowhere to live and now has a file.** `os.getcwd()` was read on both launch paths,
handed to tmux, and dropped. It could not join the identity record: every value in that record
goes onto a tmux `-e NAME=VALUE` argv, measured as world-readable in `/proc/<pid>/cmdline`,
and that list is a promise about what reaches an argv rather than a convenient bag.

**The resume flag rides the harness's existing pass-through.** `Harness.launch_argv` is
`[self.binary, *extra]` with no override anywhere in the registry, so the pass-through *is*
the seam; no member was added, which is what Phase 5's Task 9 asked for and the one thing in
that task that survived unchanged.

**No token-gauge gate, and the spec asked for one.** It specified gating the gauge on
`state.exit_code(fid) is None`, which is wrong in the direction that matters: for a chat
`kill-window` ended there IS no exit code, so the gate would show the gauge for exactly the
case it was written to suppress. And nothing needs suppressing — a reopened chat gets a fresh
chat id, so its directory has no gauge mapping at all and draws nothing until its own first
turn writes one. The ordering hazard the spec named ("this stage must ship before that one")
disappears with the gate.

## `chat: close` is the same teardown with one file more

Same plan, same warning, same kill, one target — and `closed` written into that chat's
directory. That file is the whole difference, and it exists because of a measurement: a
harness that ends on its own leaves an exit code, while `kill-pane`, `kill-window` and
`kill-session` leave nothing. This design reads "nothing" as **"we do not know it stopped —
bring it back"**, because that is what less invasive means. The stated cost is that a chat
stopped some other way may come back uninvited; `chat: close` is what pays it. Without the
mark, closing a chat and then quitting the plane brings the closed chat back, and there is
nothing you could do about it — which is asserted as a test, by removing the mark and watching
it happen.

Close also drops that chat's transcript, where quit keeps it: a capture exists to be offered
on the way back, and a closed chat is not coming back.

What close **cannot** do is refuse a busy chat. The design asks `workspace: close` to refuse
while its harness is working, and `inflight` cannot answer that question — its records name no
chat. Rather than refuse on a reading charter does not have, the confirmation row says the
chat will not come back, which is the sentence you need before pressing it.

## Scrollback: kept, offered, never replayed — and that needed an ADR amendment

tmux history dies with its session. `claude --resume` re-renders the *conversation*; it
restores no scrollback, and for the other harnesses it restores nothing at all. For a design
whose headline requirement is "less invasive", losing every visible transcript was a hole in
the premise.

Quit is the one moment charter knows a pane is about to be destroyed, so it reads it. **ADR
0018 says charter never touches the harness's pane — and the measurement showed charter had
been reading it since #384**: `_pane_last_words` runs `capture-pane -p -S -` on both launch
paths, and it is the only reason a harness that dies before the frame is drawn produces a
sentence instead of zero bytes. The ADR's own closing words are that conflating rendering with
observation *"is how a boundary like this erodes one convenient exception at a time"*, so the
ADR is amended in this change rather than quietly added to. The rule now reads: **charter
reads that pane at exactly two moments, both of them moments the pane is about to stop
existing, and writes nothing back into it.**

**The window it opens is targeted by SESSION ID, and every other spelling was measured
failing.** This is the #664/#695 shape and it cost a hand-run to find: `kill-window -t %N`
resolves a pane to its own window — charter's early-death path already relies on that — so
`new-window -t %N` looks like it should too. It does not. On tmux 3.7c *and* at the 3.2 floor,
on a session called `alpha.2` holding its own `$0`/`@0`/`%0`:

```
new-window -t %0        rc 1   can't specify pane here
new-window -t @0        rc 1   create window failed: index 0 in use
new-window -t alpha.2   rc 1   can't specify pane here      <- #695, again
new-window -t $0        rc 0
```

`-t` there is a target-*window*, a window id is read as the index to insert at (which is by
definition taken), and a dotted session name is parsed as `window.pane`. The first version of
this shipped `-t <pane>` and was correct against nothing.

Reopening **offers** the capture; it never replays it. Replaying bytes would present a session
that is not running as though it were, and put a previous run's output above a new run's
prompt with nothing marking the seam.

**Bounded, because "capture it" is not free.** One 200-column pane at charter's shipped
`history_limit = 50000` took the shared tmux server from 3.7 MB to **130 MB**, and
`capture-pane -S -` pipes all of it through charter. So the capture asks tmux for the last
2,000 lines and keeps at most 512 KB of what comes back, tail first — the bound belongs where
the memory is. `-e` keeps the colours; `-N` is what stops `-e` trimming trailing spaces, which
are the alignment of anything drawn in columns.

## Quit's blast radius is this plane, and only disk can say what that is

One tmux server serves every plane on the machine, session names carry no plane, and `default`
is a name every plane has — so `kill-server` would stop somebody else's frames and
`kill-session -t default` would stop whichever plane's `default` tmux resolved first. The set
quit stops is exactly *this plane's chat directories*, and the kill is aimed at a **window id
tmux itself just reported**, never at a session name and never at a pane id recorded before the
server may have restarted.

That is the one thing a single-plane test is blind to, so it is tested with two: two plane
roots, both with a workspace called `default`, both with a window in the one tmux session that
name can have — and quitting one leaves the other's harness running.

## Verification

* **Without tmux:** 69 new cases across four modules — what a quit records and in what order,
  the four warning sentences, what a reopen restores and what it reports instead, what close
  marks and drops, and every way the manifest refuses a value it cannot use.
* **Two guards mutated and confirmed RED:** dropping `state.record_cwd` from the launch path,
  and pruning `inflight` before the kill instead of after. A third property was found
  over-determined and is documented as such rather than claimed.
* **With a real tmux, on 3.7c and at the 3.2 floor:** the capture is real bytes off a real
  pane with its trailing spaces intact; the kill really ends the chats and, with the last
  window, the session and then the server; the pager window really opens beside the chat; and
  the two-plane case above.
* **And a correction to the design's own §2.12, measured on this branch: the real-tmux tests
  DO run in CI.** `.github/workflows/test.yml` installs no tmux, but `ubuntu-latest` ships
  one — so they ran there, and one of them failed on 3.11 and 3.13 while passing on 3.14 in
  the same run. That was a race — two of them, in the same assertion: a pane's process has not
  exec'd when `new-window` returns, and a pager that HAS exec'd has not necessarily painted.
  Both were found by CI and by neither hand-run, and the second needed a second CI run,
  because fixing the first made 3.11 and 3.13 green while 3.14 came back with an empty
  capture. The honest statement is narrower than "no tmux in CI":
  **CI answers on whatever tmux the runner image happens to ship, which is neither of the two
  versions charter promises**, so it is a third machine's opinion on top of the hand-runs
  rather than a substitute for them.

## What this does not do

* **Make the chat directory durable.** Stage 4, six edits, unchanged here — which is why a
  reopened chat is a new tab with a new id rather than the same directory woken up.
* **Reopen automatically.** Bare `charter` opens the frame (0.54.0); it does not consume the
  manifest. `charter reopen` is a thing you ask for.
* **Resume anything but Claude Code**, and the warning says so per chat.
* **Restore a selection, a pane map, or live scrollback.** The first two are destroyed on
  purpose; the third is offered as a file.
* **Refuse a close while that chat is working** — `inflight` cannot say which chat is.
* **Reopen from inside a tmux you already have.** Charter builds a frame there as a window on
  your own server, and that launcher stays awake for the life of each frame — it reads the
  harness's exit status itself instead of installing the `pane-died` hooks, which is what
  makes it correct there and what makes it block exactly as `attach` does. A reopen driving
  it would stop at the first chat, so `charter reopen` refuses with that sentence and leaves
  the record in place. Suppressing that wait is its own change, on a path whose exit-code
  contract depends on it.

## Adopting it

Nothing to configure. `charter reopen` appears in `charter --help`; the three palette rows
appear on any plane whose frame is on. `chat: previous transcript` is refused with its reason
until a quit has captured one.
