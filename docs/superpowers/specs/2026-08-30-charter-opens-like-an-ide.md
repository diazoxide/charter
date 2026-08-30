# Charter opens like an IDE — bare launch, many workspaces at once, and nothing lost on exit

**Date:** 2026-08-30 · **Status:** specified, unimplemented
**Settles:** the operator's four asks of 2026-08-30, grilled to agreement over eight rounds.
**Builds on:** Phase 5 Stage 5a (#645) and 5b (#668). **Supersedes** Phase 5's Task 9 framing
of cold chats: Task 9 restores *a* chat; this restores the plane.

## 1. What the operator asked for, in their words

> now `charter claude` → `charter`
>
> when opening charter - its asking workspace before opening, but we need to remove workspace
> asking, its should by default open empty IDE and then user will open wanted workspace, also
> we need to keep state of charter - and next run, or another session should open exact same
>
> so user cant open 2 separate charters in same projects, even if opening - it should be same
> 1:1 sync sessions
>
> in our IDE user will able simultaneously open multiple workspaces/projects
>
> when user closing charter session we need to show warning - that all harness sessions will
> be closed, and when opening again - we need to resume harness sessions as well. so for user
> it should be "less invasive" - to not lost sessions, state etc

**"Make it like an IDE" is the organising principle, and it argued against two of the four
asks.** Both reversals are in §3 with the reasoning that produced them. Nothing here was
accepted because it was asked for.

## 2. What is already true, measured

Nine facts. Each one closed a branch of the design, and several closed it in the direction
opposite to the original ask.

**2.1 A workspace already IS a tmux session, and a second launch already joins it.**
`commands_frame.py:3657` is `session = state.workspace_prefix(ws)`, and `:3766` branches on
`if session in live_sessions:` — adding a chat window rather than starting a frame. The live
server shows `harness-wrapper` with **`attached=2`**: two clients, one session. **Ask 4 was
substantially already built by Stage 5a.**

**2.2 The `<workspace>-<pid>` sessions are legacy and cannot be minted.** `state.frame_id`
(`state.py:103`) says *"nothing mints one of these any more"*; production callers are zero.
It survives as the reader half so `_launcher_pid` can still parse pre-5a directories.

**2.3 A second launch drags the first client.** Both clients of a session share one current
window, so the second launch's `select-window` (`:3974`) moves the *already-attached* client
onto the new chat, and `_drop_panels(leaving)` (`:3986`) then tears down the panels of the
chat that client was looking at. **This is ask 4 arriving as a defect rather than as sync.**

**2.4 tmux does not clamp — it fights.** Charter sets neither `window-size` nor
`aggressive-resize`, so the default `latest` applies. Measured on 3.7c with real ptys:

```
client A 100x30 attached     window=100x29   A=100x30
client B 160x45 attached     window=160x44   A=100x30  B=160x45   <- A left mismatched
A resized to 240x60          window=240x59   A=240x60  B=160x45
```

The window snaps to whichever client last acted and the other is simply left wrong.
`smallest` would clamp; it is not the default. **An earlier draft of this design said
"clamped to the smaller" and was wrong.**

**2.5 "Reopen exactly as I left it" does not exist today, and four files are deliberately
destroyed to make sure of it.** A relaunch gets a new `fid`, and `clear_shape`
(`state.py:1077`) deletes `panes`, `selection`, `session` and the shape files, each with its
reason written at the line — *"a highlight is a claim about an action they did not take"*,
*"a gauge reading somebody else's 78% is worse than either"*. Three more files are rewritten
unconditionally by the launcher.

**2.6 "No workspace" exists as a sentinel with exactly one reader.** `workspace.chosen`
returns `str | None`; `None` means every rung came back empty. Its one production caller is
`_choose_workspace` (`commands_frame.py:3430`), which uses it to raise the picker. Everything
downstream is a string, and `state.workspace_for` is total — **no renderer can ever be handed
"no workspace."**

**2.7 The persona column is the only plane-scoped surface.** `repos`, `todos`, `changes`, the
identity name and the attention counts all resolve through `slots._frame_workspace`. An
"empty IDE" is four panes drawing nothing plus a working persona list.

**2.8 The harness choice is cosmetic at launch and decisive afterwards.** `cmd_launch` touches
it three times: argv, `$CHARTER_HARNESS`, one `which`. `launch_argv` is defined once with no
overrides. But **the context gauge only ever draws for Claude Code** —
`record_harness_session` has exactly one caller, Claude's `statusLine` hook, and Codex and
opencode both carry a `status-bar` deficit.

**2.9 `.charter/` is already per-machine.** `/.charter/` is gitignored wholesale and holds
zero tracked files. The per-machine state this design needs has a home already.

## 3. The two reversals

**3.1 "Empty IDE by default" is not IDE-like, and it was dropped.** No IDE opens blank when
you reopen it; the blank window is what you get on a first install or after deliberately
closing a folder. The requirement underneath was **"stop asking me a question I already
answered"** — which persistence delivers, and which needs no new state (§2.6 says a
no-workspace state would have to be invented through every renderer).

The picker stays, and fires only when **nothing is open and nothing is restorable**. That is
one rule, and it keeps #518's property: `default` is never answered by silence.

A blank frame remains reachable as an explicit `charter --no-workspace`, which is what an IDE
means by "close folder" — a thing you ask for, not a thing you land in.

**3.2 "Cannot open 2 charters" was already true, and the lock was dropped.** §2.1 shows a
second launch joins. What was missing was not a lock but a defect fix (§2.3) — and given §2.4,
forbidding a second client would have bought a geometry fight rather than sync. **The
requirement is "never fork the state", and attaching already satisfies it.**

## 4. The design

### 4a. Bare `charter`

`charter` with no subcommand launches the frame. The harness comes from a `[harness] default`
key in `charter.toml`; a plane that sets none keeps today's usage output rather than guessing.

**The default when a plane does set one should be `claude`**, and the reason is §2.8: it is the
only harness that writes the session id, so it is the only one with a context gauge and — see
§4e — the only one charter can resume. A bare command that silently loses both would be
charter choosing for the operator without saying so.

### 4b. Many workspaces open, one visible

A workspace is a tmux session (§2.1); several may be live at once; one client shows one at a
time and the `workspaces` bar switches between them. **Not** panes from two workspaces in one
window — that would require the frame to stop being one tmux session, which is the foundation
everything since #488 rests on.

Switching is `switch-client`, plus **#686's treatment one scope out**: a background window
keeps stale geometry (§7.4 of the Phase 5 spec, measured on both versions), so a workspace
switched back into must be re-dressed unconditionally, exactly as #686 now does for chats.

**The `workspaces` bar joins the shipped default when this lands, and not before.** Its cost —
a permanent row, a process, ~7 of every switch's 41 tmux calls — is only worth paying once it
is a project switcher rather than a label naming the one workspace you have.

### 4c. Open, live, and the difference between them

Two states, and the distinction is load-bearing:

* **open** — has a chat directory that has not been closed. Appears in the bar.
* **live** — has a running session and harness.

**The bars draw two marks, not three.** Cold (never started this run) and dead (harness
exited) are the same thing to an operator — nothing is running, touching it starts something —
and charter cannot reliably distinguish them after the fact. A third mark would be a
distinction the tool cannot sustain.

**tmux is the truth; the file is a hint.** On every paint, open is `(what is on disk) ∪ (live
sessions)`. A session charter did not record is still open, because it exists. That way the
persisted state can only ever be *stale*, never *authoritative* — which is the failure mode an
operator can recover from.

### 4d. What persists, and where

Derived wherever possible. `.charter/frame/` already holds one directory per chat, and that IS
the open set. **One small file holds only what cannot be derived**: the last-active chat id and
the tab order. Both degrade harmlessly — a lost update costs an opening tab, not state.

**No lock.** Two charter processes may write it; last writer wins. A lock file is a thing that
gets stranded, and #685 spent a whole PR on that class of problem.

**Reap must stop treating "no live window" as "delete".** A chat directory is durable state
now, not a liveness marker. The rule is restated rather than patched: **a chat directory lives
until the operator closes the chat.** Closing is the only reaper — `chat: close`,
`workspace: close` — plus a bound on cold chats per workspace, which is what `max_chats`
(`state.py:132`, a comment no code reads) was always for.

### 4e. Resume, and the file that must stop being deleted

Charter already records the harness's own session id (`.charter/frame/<fid>/session`). It is
exactly what a resume needs, and `clear_shape` deliberately deletes it (§2.5).

**Both requirements are right; the file is serving two purposes.** The *gauge* must not show a
stale reading. The *identifier* is fine to keep. So: **keep the id as durable per-chat state;
make the gauge refuse to draw unless the id belongs to a live harness.** The deletion was
protecting a reading, not the identifier.

What a reopen restores is Task 9's own list — **workspace, persona, harness, cwd** — into the
chat's *existing* directory rather than a new `fid`, which is what those `clear_*` calls say
they are waiting for. It does **not** restore `selection` or `panes`; §2.5's reasons for
destroying those are still right.

**Resume is Claude-only and must say so.** §2.8: no other harness writes the id. The warning
in §4f names, per chat, what will and will not come back — at the moment the operator is
deciding, not after.

**A resumed chat says what was in flight.** `--resume` restores a conversation, not work: a
half-finished edit is half-finished, a running test is dead. Charter already knows what was
running — `inflight` is what the spinner reads. A resumed chat opening with *"2 tools were
running when this closed"* is honest; one opening pristine is the convincing-empty this project
refuses everywhere else.

### 4f. Leaving: detach is the default, quit is a choice

**Closing the window detaches.** tmux sessions survive a client leaving, so the common exit
costs nothing, loses nothing, and needs no resume. This is the whole of "less invasive", and it
is free.

**`F2 → charter: quit` kills**, with a warning naming every workspace and chat that will stop
and — per §4e — which of them can be resumed. It **warns and proceeds**; it does not refuse.
`workspace: close` refuses while its harness is busy, because closing one thing while it works
is probably a mistake. Quit is unambiguous: you asked to stop everything, and refusing would
leave an operator unable to quit while any agent was working, which on this plane is most of
the time.

**Quit is reachable only from the palette** — never from a signal, never from a closing
terminal. A terminal that dies detaches, as it always did.

**`F2 → charter: detach`** exists so that leaving deliberately does not require knowing tmux's
prefix, which the frame otherwise hides.

### 4g. A harness that exits on its own

Neither detach nor quit, and the most common ending of all. **The chat becomes a dead tab and
is kept** — the same state as a cold one (§4c), so it costs nothing new. Closing the workspace
because one chat ended would be charter deciding the operator is finished when they closed one
thing.

### 4h. `charter -w foo` opens or focuses

With several workspaces open, an explicit `--workspace` is ambiguous for the first time: "as
well" or "instead". **Open-or-focus**: if `foo` is live, attach to it; if not, open it and
leave the others. That is `code <path>`'s behaviour, it never destroys state, and the flag
means one thing whether or not the workspace happens to be running.

### 4i. What draws first

Panels first, harness second. **The bar is what tells the operator the restore worked**, and
drawing it first makes a slow harness launch legible instead of looking like a hang. Charter
already separates panel launch from harness launch, so this is ordering rather than machinery.

### 4j. `charter status` learns about open and live

It is the non-frame view of the plane. A `status` reporting "15 workspaces" while five are open
and one is live is answering a question nobody asked, and two surfaces disagreeing about the
plane is the kind of thing found six months later.

## 5. What this does not do, stated rather than implied

* **Resume for Codex and opencode.** Neither writes a session id (§2.8). They get the warning
  and the kill; they do not get the conversation back, and the warning says so.
* **Restore a selection or a pane map.** Deliberately (§2.5).
* **Distinguish cold from dead** (§4c).
* **Two workspaces visible at once.** §4b — that is a different product.
* **Solve the geometry fight** (§2.4). Two clients on one session still race; this design
  removes the reason to have two rather than making two work.

## 6. Delivery, in an order that can be stopped between stages

1. **The client-drag defect (§2.3).** Smallest, and it is a live bug rather than a feature. A
   new chat must not move a client that did not ask to move.
2. **Split the session id from the gauge (§4e).** One file, two purposes, no behaviour change
   yet — but it is the prerequisite for everything in §4e and it stands alone.
3. **Open vs live, and the persisted pointer (§4c, §4d).** Includes the reap rule change, which
   is the riskiest single edit here: it inverts a rule three PRs this week depended on.
4. **Reopen into an existing directory (§4e).** Phase 5's Task 9, with the plane-level framing.
5. **Detach, quit, and the warning (§4f, §4g).**
6. **Bare `charter` and `[harness] default` (§4a).**
7. **Place the `workspaces` bar by default (§4b), and `charter status` (§4j).** Last, because
   both are only correct once the states beneath them are real.

## 7. How this gets verified

The rule this project has paid for repeatedly, most recently in #664 and #701: **a feature
charter ships must be usable on charter's own plane**, and it must be exercised against a real
plane rather than a synthetic config. Two of tonight's defects — a `size` key that dropped a
whole arrangement, and a documented snippet that turned the frame off — reached `main` because
nothing tested them anywhere else.

So the acceptance test is not a unit test. It is: **this plane, with several workspaces open,
quit and reopened, with the suite green and `charter.toml` carrying the bars.**
