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

**2.9 A harness's exit is already recorded, per chat, and survives.** `record_exit`
(`state.py:382`) writes the exit *code* into the chat's own directory, and `clear_shape` wipes
`density`, `hidden`, `panes`, `session`, `selection` — **not `exit`**. Measured on this plane:
`.charter/frame/harness-wrapper.2/exit` holds `0`, from the frame the operator killed when they
restarted. **An earlier draft of this spec claimed charter could not tell a cold chat from a
dead one and drew both the same. That was wrong, and the correction is richer than the claim:
never-started, exited-cleanly, and died-with-code-N are all distinguishable and already
recorded.**

**2.10 tmux has no per-client current window inside one session, on either version.**
Measured with real ptys on 3.7c and 3.2: a client on `chat1`, then `new-window -d` +
`select-window`, lands that client on `chat2` — identically at the floor. **The only mechanism
that does not drag is a session group** (`new-session -t ws -s ws2`), also measured on both:
the attached client stays put while the grouped session moves. But adopting groups redefines
"a workspace is a tmux session", which §4b calls the foundation everything since #488 rests on.
**So §2.3's defect has no small fix, and §4k is the cheap correct answer to it instead.**

**2.11 `F2 → charter: detach` already exists and is already correct.** `builtin_actions.py:112`
defines `_detach`/`_detachable`, and `build()` registers it **first** (`:356`). It survived
Stage 5a's move of the session identity: measured on both versions, `detach-client -s api.1`
detaches the client of session `api` and leaves a client on `zeta` alone; a target naming
nothing is a silent rc-0 no-op. **An earlier draft of this spec specified a feature that
ships.** One residual worth stating: `-s` detaches *every* client of the session, which under a
deliberately-multi-client design is a choice rather than an accident.

**2.12 CI never runs a real tmux.** `.github/workflows/test.yml` installs none, so the 95
real-tmux tests skip there. **Every tmux claim in this spec has to be hand-verified on this
machine, on both versions**, and a green gate says nothing about any of them. The 3.2 floor
binary is preserved at `~/.local/share/charter-testing/tmux-3.2`.

**2.13 `.charter/` is already per-machine.** `/.charter/` is gitignored wholesale and holds
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

**One hazard, and it is the #687/#690 shape.** `cmd_launch` returns `bypass(argv)` when stdout
is not a tty. Today bare `charter` is argparse's own usage error — exit 2, no side effect.
Afterwards, on a plane that sets a default, **`charter 2>&1 | head` would exec the harness**.
Correct against every config anyone tested, wrong the first time a script probes for charter.
The non-tty path must keep printing usage.

Note also that "three touch points in `cmd_launch`" (§2.8) measures how a *chosen* harness
threads through, not the feature: there is no `[harness]` section in `instance.py` at all, so
this is a new config section plus a new CLI dispatch mode.

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

**The two bars are not one proposition, and only one of them is useful today.**

* **`chats` is correct now.** `slots.chats_bar` reads `.charter/frame/` and marks the active
  chat; this plane already has two. It stays correct after the redesign in §6 with only a dead
  mark added.
* **`workspaces` is not.** `slots.workspaces_bar` is `switch.workspaces()` — **every directory
  under `workspaces/`**, which on this plane is fifteen. Until open and live are real states
  (§4c) it draws a directory listing, not a project switcher, and a project switcher is the
  only thing that justifies its row.

So `chats` may be placed at any time; **`workspaces` joins the shipped default only once §4c
lands**, together with §4g and §4h as one change.

**Neither bar handles a click.** Both are registered `needs=()` with no `events=`/`on_event=`,
and `Component.__post_init__` refuses an `on_event` without `events`. Click-to-switch is two
fields plus a handler, and fires only on `mouse = true` planes.

**Cost of placing one:** a row and a border off the harness, one ~24 MB panel process, and ~7
tmux calls on each half of a switch. The 41-call / ~360 ms figure in Phase 5's §7.10 was
measured with *four* panels, so a fifth makes it roughly 48.

### 4c. Open, live, and the difference between them

Two states, and the distinction is load-bearing:

* **open** — has a chat directory that has not been closed. Appears in the bar.
* **live** — has a running session and harness.

**The bars distinguish three, because charter can (§2.9).** *Live*; *idle* — never started
this run, or exited cleanly, which are the same thing to an operator and both mean "touch it to
start"; and *died* — a non-zero exit, which is not the same thing at all and is the one state
that should reach for the operator's attention.

An earlier draft drew two marks on the grounds that cold and dead were indistinguishable. They
are not: the exit code is per chat and survives. Drawing a crash as though it were an ordinary
closed tab is the convincing-empty this project refuses everywhere else.

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

### 4f. Scrollback: preserve a record, do not fake a session

tmux history dies with the session. `claude --resume` re-renders the *conversation*; it does
not restore the pane's raw scrollback, and for Codex and opencode it restores nothing at all
(§2.8). For a design whose headline requirement is *"less invasive"*, losing every visible
transcript on quit is a hole in the premise — and eight rounds of grilling did not find it.

Quit is the one moment charter knows a pane is about to be destroyed, and
`tmux capture-pane -p -S -` hands back the whole history.

**On quit, each chat's scrollback is written to its own directory. On reopen it is offered,
never replayed.** `F2 → chat: previous transcript` opens the captured text; the live pane
starts clean. Replaying bytes into a pane would present a session that is not running as
though it were, which is the convincing-empty this project refuses everywhere else — and it
would put a *previous* run's output above a *new* run's prompt with nothing marking the seam.

This is also the half that works where resume does not: a harness charter cannot resume still
had a transcript, and that transcript is the whole of what the operator loses.

**Bounded like the chats are** (§4d): the last capture per chat, not a history of them.

### 4g. Activity: a tab that wants you says so

With several workspaces open, an agent finishing in one you are not looking at is invisible.
That makes multiple-open *worse* than single-open — four more places where something may have
happened and no signal from any of them. Every surface a modern engineer uses solves this with
an unread mark, and the reason is the same one.

Charter already has the data: `inflight` is what the panel spinner reads, and the frame already
knows when a dispatch starts and ends.

**Three marks per tab in the `workspaces` bar** — *working*, *wants you* (something finished
since you last looked at it), *idle* — plus *died* from §4c, which is the one that should be
loudest.

**"Since you last looked" is per client and needs no new persistence**: switching into a
workspace clears its mark, and tmux already knows which session a client is on.

**This belongs in the same change that places the bar, not after it.** Adding it later means
revisiting the bar's render, its width arithmetic and the state plumbing separately; adding it
now is a field in something already being written. It is the clearest instance of the
operator's own rule — *a cheap feature added now is a dear one added later*.

### 4h. Stale is marked, because §4d stopped refreshing it

§4d stops a background workspace's `gather` from scanning. That is right — you are not looking
at it — but it means the numbers a tab carries can be old, and charter owes the operator a mark
saying so rather than presenting a stale count as current.

The refresh happens on switch-in. Between backgrounding and that refresh, the workspace's own
counts are marked stale wherever they are drawn.

This is not a new feature; it is the other half of a decision already taken. Deferring it means
touching the same renderers twice.

### 4i. Leaving: detach is the default, quit is a choice

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

### 4j. A harness that exits on its own

Neither detach nor quit, and the most common ending of all. **The chat becomes a dead tab and
is kept** — the same state as a cold one (§4c), so it costs nothing new. Closing the workspace
because one chat ended would be charter deciding the operator is finished when they closed one
thing.

### 4k. `charter -w foo` opens or focuses

With several workspaces open, an explicit `--workspace` is ambiguous for the first time: "as
well" or "instead". **Open-or-focus**: if `foo` is live, attach to it; if not, open it and
leave the others. That is `code <path>`'s behaviour, it never destroys state, and the flag
means one thing whether or not the workspace happens to be running.

### 4l. What draws first

Panels first, harness second. **The bar is what tells the operator the restore worked**, and
drawing it first makes a slow harness launch legible instead of looking like a hang.

**This is machinery, not ordering, and an earlier draft said otherwise.** Charter separates
panel launch from harness launch only on the operator-tmux path, which uses a placeholder plus
`respawn_argv`. On the private path `layout.session_argv` and `chat_window_argv` start the
harness *inside* the `new-session`/`new-window`, so panels cannot precede it without adopting
the placeholder machinery there too. Cost it as such.

### 4m. `charter status` learns about open and live

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
* **Restore live scrollback.** §4f preserves a *record* and offers it; the reopened pane starts
  clean. Nothing here makes a killed session's history scroll again.
* **Notify outside the frame.** §4g marks a tab that wants you; it does not ring a bell, raise
  a desktop notification, or reach an operator who is not looking at charter at all.

## 6. Delivery

**A cost analysis re-ordered this section and corrected three of its claims.** What follows is
the corrected order; the corrections are named where they land, because a delivery plan that
quietly changed is one nobody can audit.

### Ready now — independent of each other and of everything below

1. **Place the `chats` bar on this plane** (§4b). *Config only, zero code.* #702 removed the
   last blocker. **Not free of verification**: this plane pins `repos size = 15`, and the row
   budget cannot be computed from `layout.slot_sizes` before the table is committed — `_size_of`
   falls back to `_placed_here()`, which reads `config.FRAME["components"]`. That is #687's
   exact shape, so the check is a real frame at a real height, not a Python call.

2. **The durable session id (§4e), in its reduced form.** `record_harness_session` also writes a
   sibling `clear_shape` does not list; `clear_shape` and the gauge are untouched. ~20 lines,
   two files, **zero behaviour change**, and it is the prerequisite for stage 6.

   **The full §4e is not a refactor and an earlier draft said it was.** "Make the gauge refuse
   unless the id is live" needs a liveness signal a panel process can afford on its repaint
   path — an open design question, not a settled one. The reduced form sidesteps it entirely.

3. **Open-or-focus (§4k).** *This replaces "fix the client-drag defect" as the first stage.*
   §2.10 shows §2.3 has no small fix — tmux has no per-client current window, and the only
   non-dragging mechanism redefines the foundation. **Open-or-focus removes the reason to drag
   instead of fixing the drag**: a live workspace whose session already has an attached client
   gets attached to, not added to. One `list-clients -t` and a branch in `cmd_launch` — and it
   is ask 4 as the operator wrote it.

   **Ten lines and a tmux semantics claim is not cheap.** #664, #687 and #690 all came from
   exactly that combination. Two-version verification by hand (§2.12).

### Not a stage — a redesign

**Stage 4 changes what a chat directory *is***: a liveness marker today, reaped when no window
holds it; durable state afterwards, reaped only when the operator closes the chat. #685, #691
and #696 all rest on the current rule — #685 exists *because* a directory was deleted by the
next launch.

It is also **three features, not one**. `reap` carries four keep-rules and only `if d.name in
live` inverts; the other three (server scope, exit-code protection, the claim window) are about
other races and survive untouched. What makes it expensive is that **~60 tests across 10 files
assert the current rule**, four by name in their titles, and that **`chat: close` does not
exist anywhere in `charter/`** while `max_chats` is a comment nothing reads. Ship the inversion
without close and the cap and `.charter/frame/` grows forever — which `reap`'s own docstring
names as the failure it exists to prevent.

4. **A chat directory becomes durable (§4c, §4d), with `chat: close` and the `max_chats` cap.**
   Verify against #685's own reproduction before anything lands on top.
5. **Reopen into an existing directory, and resume (§4e).** Phase 5's Task 9, plane-scoped.
6. **Quit, the warning, and the scrollback capture (§4i, §4f, §4j).** Quit is what makes the
   capture necessary; they ship together or the capture has no trigger.
7. **Bare `charter` and `[harness] default` (§4a)**, including the non-tty hazard.
8. **Place the `workspaces` bar with activity and stale marks (§4b, §4g, §4h), and
   `charter status` (§4m).** One change, not three — splitting them means touching the same
   render, width arithmetic and plumbing three times.

### Already shipped, specified in error

**`F2 → charter: detach` exists** (§2.11) and is already correct after Stage 5a. An earlier
draft listed it as work. It needs none.

## 7. How this gets verified

The rule this project has paid for repeatedly, most recently in #664 and #701: **a feature
charter ships must be usable on charter's own plane**, and it must be exercised against a real
plane rather than a synthetic config. Two of tonight's defects — a `size` key that dropped a
whole arrangement, and a documented snippet that turned the frame off — reached `main` because
nothing tested them anywhere else.

So the acceptance test is not a unit test. It is: **this plane, with several workspaces open,
quit and reopened, with the suite green and `charter.toml` carrying the bars.**
