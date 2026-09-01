# Phase 5 — workspace tabs, chat tabs, and many harnesses in parallel

> **For agentic workers:** REQUIRED SUB-SKILL: `superpowers:subagent-driven-development`.

**Goal:** a workspace holds several chats, a chat picks its harness when it starts, and switching
between either loses nothing. A frame stops being a tmux session and becomes a tmux window; a
tmux session becomes a workspace. Two new components draw the readout; the palette stays the
mechanism.

**Spec:** `docs/superpowers/specs/2026-08-28-phase5-workspace-and-chat-tabs.md`.
**It overrides `2026-08-25-agentic-ide-foundation.md` §4j in three places** and says which.
**Measurements:** that spec's §7 — every number, with the command that produced it.

## What the measurements already decided

- **`window-resized` does not exist on tmux 3.2** (`invalid option`, rc=1) and a background
  window keeps stale geometry on both 3.2 and 3.7c. **So the switch reasserts the layout itself
  and never relies on a hook.**
- **Panels follow the active chat; they do not duplicate.** 32 panel processes at 23.7 MB
  resident each, rendering the same `gather.json` at widths that are wrong, is what the
  alternative buys. Panels created *after* the switch are born at the true width.
- **The name is allocated, not hashed.** A hash collides silently into a shared
  `.charter/frame/<fid>/`, and a `-{ordinal}` tail makes `state._launcher_pid` read `2` as a
  live pid, so every dead chat looks live forever. `{workspace}.{n}` makes `_launcher_pid`
  return `None`, and that `None` is the version discriminator.
- **Per-window `-e` overrides a session-wide `set-environment`**, on 3.7c and on 3.2. That is
  how `$CHARTER_SESSION_ID` becomes per-chat, and it is the whole identity mechanism.
- **`tmuxctl.chain` is worth 3.3× on the switch path** — four `split-window` go from 22.2 ms to
  6.7 ms, four `kill-pane` from 19.3 ms to 4.9 ms.
- **A tmux window name is not an identity.** `automatic-rename` is on by default and follows the
  foreground process; `allow-rename on` lets the pane's own output set it; and `rename-window`
  refuses a newline on 3.7c and accepts it on 3.2. The bars read charter's record.
- **The harness is the cost.** 19 live `claude` processes on the measuring machine: 4.29 GB,
  mean 226 MB. A panel is noise beside it.

## Global constraints

- `dependencies = []`, stdlib only. stdlib `unittest`, never pytest.
- Run the suite in **two environments** and say which; they must agree.
- `PersonaIso`; new `patch.dict(os.environ, …)` MUST pass `clear=True`.
- `tui.width()` never `len()`. `contain.one_line` **before** width arithmetic (#472).
- Every state write goes through `config.write_for` / `config.private_mkdir`, or
  `tests/_statedirscan.py` fails the build (#505/#603/#581/#582).
- **A chat's id and a chat's label are two strings with two alphabets.** The id is
  `_UNSAFE`-sanitised and may reach tmux. The label is open and may reach only the palette and
  the bars. A test that lets a label reach a tmux argv is a failing test.
- **Nothing is added to `layout.CARRIABLE` without an argument for it.** A tmux `-e` becomes
  world-readable argv.
- Mutation-test every guard: apply, RED, restore, GREEN, `__pycache__` cleared. **Report the
  mutation actually run and its actual result.**
- No version bump, no stamping, no tag. **Implement, PR, merge — never release alone.**

## The deletion sweep — required before any PR in this phase

`tools/sweep.py` is in the tree and `.github/workflows/sweep.yml` runs it on every pull request:

```
python3 tools/sweep.py --gate --jobs 4 --json sweep-results.json --summary "$GITHUB_STEP_SUMMARY" [--base "$BASE_SHA"]
```

The job reports and blocks nothing — `--enforce` is deliberately absent — so **the branch author
still runs it and still reads it.** Run it locally before submitting:

```
python3 tools/sweep.py                    # this branch, against its merge-base
```

`classify()` buckets survivors as `unpinned`, `masked` (≥2 survivors in one `(path, symbol)` —
*more* urgent, read together), `platform`, `unresolved` and `pinned`. **A timeout is
`unresolved`, neither green nor red.** There is no suppression list: "equivalent mutant" and
"dead code" are the same finding.

**So: for every `if` you add that refuses, clamps, contains or falls back, write the test that
goes RED when that line is deleted** — and assert **which** refusal fired, not just that
something did. Two guards in sequence mask each other, and an exit-code assertion cannot tell
them apart.

Phase 5 has four guards where a masked survivor would be expensive, and each gets a named test:

1. the ordinal allocator's `FileExistsError` claim,
2. the `pane-died` teardown running `kill-window` and not `kill-session`,
3. the label containment before width arithmetic,
4. the live-chat cap's refusal.

---

## Stage 5a — a frame is a window

Nothing visible changes. Everything underneath does.

### Task 1: A chat id is allocated, not computed

- [ ] **Step 1:** write the failing test — `state.new_chat_id("api")` returns `api.1`, then
      `api.2`; two allocators racing the same workspace never return the same id; and a
      workspace named `api.2` does not collide with workspace `api` chat 2.
- [ ] **Step 2:** run it, confirm it fails.
- [ ] **Step 3:** implement. Allocation is a bare `os.mkdir(d, 0o700)` plus an explicit
      `os.chmod(d, 0o700)` — **not** `config.private_mkdir`, which swallows `FileExistsError` on
      a directory (#331) and is therefore idempotent, which is exactly wrong for an allocator.
      `FileExistsError` means "taken, try `n+1`".
- [ ] **Step 4:** `state._launcher_pid("api.3")` returns `None`, and that is the discriminator.
      Test that an old-shape `api-12345` still parses, still reports liveness by pid, and is
      still reaped by the old rule.
- [ ] **Step 5:** **the id is never parsed for meaning.** Test: rename a workspace under two
      live chats; both still resolve, both keep their old-prefixed ids, and the bar shows the
      workspace `frame_workspace(fid)` names.
- [ ] **Step 6:** mutations — make the allocator scan-then-create instead of claiming by
      `mkdir`; use `private_mkdir` so `FileExistsError` is swallowed; let `_launcher_pid` parse
      a dot id. Each RED.
- [ ] **Step 7:** commit.

### Task 2: The private server grows windows, and liveness moves

- [ ] **Step 1:** write the failing test — two chats in one workspace are two windows on one
      tmux session named after the workspace, and `state.reap` keeps both while their windows
      exist and neither launcher process does.
- [ ] **Step 2:** run it, confirm it fails.
- [ ] **Step 3:** implement. `list-windows -a -F '#{window_name}'` is the live set for a chat, as
      `_live_windows` already does on the guest path. **This makes the private path work the way
      the guest path already works** — do not invent a third shape.
- [ ] **Step 4:** **`set -w -t <window> @charter_chat <chat id>`** at creation, so a lookup asks
      the window what chat it is rather than parsing a name. The same window-option mechanism
      `@charter_hatch` already uses, and the reason `F12` works per chat with no new code.
- [ ] **Step 5:** **the `pane-died` teardown runs `kill-window`, not `kill-session`.** This is
      the single most dangerous line in the port: unchanged, one chat's harness dying takes
      every other chat in that workspace with it, mid-turn. Test it with two chats and kill one.
- [ ] **Step 6:** `state.clear_shape` still unlinks `session` at launch (#413).
- [ ] **Step 7:** mutations — restore `kill-session` in the teardown hook; take liveness from the
      launcher pid for a dot id; drop `@charter_chat`. Each RED.
- [ ] **Step 8:** commit.

### Task 3: Identity is per chat

- [ ] **Step 1:** write the failing test — two windows on one session, created with different
      `-e CHARTER_SESSION_ID` and `-e CHARTER_HARNESS`, each report their own values from inside
      the pane, over a session-wide `set-environment` that says something else.
- [ ] **Step 2:** run it, confirm it fails.
- [ ] **Step 3:** implement through `layout._env_argv`, which already raises on any name outside
      `CARRIABLE`. **Add nothing to `CARRIABLE`.**
- [ ] **Step 4:** assert the consequence: `.charter/sessions/<chat id>.persona`, `.workspace`,
      `.tools` and `.gate` become per-chat with no new code, because `session.current()` reads
      `$CHARTER_SESSION_ID`. Two chats, two personas, two tool ceilings, each enforced in its own
      process.
- [ ] **Step 5:** mutation — carry the identity session-wide instead of per window; confirm two
      chats then share a persona pointer, and confirm RED.
- [ ] **Step 6:** commit.

**Stage 5a exit criteria**

- Two chats run in one workspace on one tmux session, each with its own frame directory, its own
  `$CHARTER_SESSION_ID`, its own persona pointer and its own tool ceiling.
- Killing one chat's harness leaves the other running.
- `state.reap` removes a chat whose window is gone and keeps one whose window is not, with no
  launcher process alive for either.
- Old `{workspace}-{pid}` frames still launch, still report liveness, still reap. No migration
  ran.

---

## Stage 5b — switching, and the surface

### Task 4: The switch action

- [ ] **Step 1:** write the failing test — switching to a chat selects its window, tears down the
      previous chat's panels, splits panels into the new one, and bumps so they paint.
- [ ] **Step 2:** run it, confirm it fails.
- [ ] **Step 3:** implement over `_apply_arrangement(fid, want=[])` then
      `_apply_arrangement(fid, want=<arrangement>)` — the one live pane-mutation funnel, not a
      second path.
- [ ] **Step 4:** **one `tmuxctl.chain` per group.** Measured: 6.7 ms chained against 22.2 ms
      unchained for four splits. `chain` returns `None` when the servers disagree; a `None` is a
      refusal, not a fallback to N invocations.
- [ ] **Step 5:** **the switch reasserts the layout; it does not rely on `window-resized`.** Test
      the whole path with the hook removed — it must produce the same layout, because on tmux 3.2
      the hook does not exist.
- [ ] **Step 6:** **bump, do not just write a pointer** (#411/#412). A pointer some panels may
      read is the bug those issues were filed for.
- [ ] **Step 7:** actions are fire-and-report, never blocking (§4g). A switch returns having
      started, and a failure surfaces rather than hanging the palette.
- [ ] **Step 8:** mutations — drop the teardown so panels accumulate; write the pointer without
      bumping; replace the chained invocation with N invocations and assert the *behaviour* that
      changes, not the timing; make the switch depend on `window-resized`. Each RED.
- [ ] **Step 9:** commit.

### Task 5: Harness selection at chat creation

- [ ] **Step 1:** write the failing test — `F2` → `pick:harness` lists the registered harnesses,
      choosing one creates a chat running it, and a harness whose `binary` is not on `PATH` is
      listed **with its reason**, not hidden.
- [ ] **Step 2:** run it, confirm it fails.
- [ ] **Step 3:** implement over `choose.py` as a third noun. **This is the first place charter
      asks rather than detects** (§4j) — so the row source is `harness.all()` in registration
      order, and `harness.current()` marks the default.
- [ ] **Step 4:** a harness's `deficits` appear as the row's note. An operator choosing `codex`
      should see `session-lock` before they choose it, not after.
- [ ] **Step 5:** mutations — hide an unavailable harness; drop the deficit note. Each RED.
- [ ] **Step 6:** commit.

### Task 6: The chat picker, and the containment that matters

- [ ] **Step 1:** write the failing test — a chat picker lists the workspace's chats, choosing
      one switches to it, and a chat whose label contains a newline, U+2028, an escape sequence,
      a quote or a `#` renders as **one row** and runs nothing.
- [ ] **Step 2:** run it, confirm it fails.
- [ ] **Step 3:** implement as `choose.CHAT`, a third entry in `NOUNS`, over the same
      `overlay.Surface` with `OPEN_ID`/`NAME_ID` unchanged. A chat's row id is `chat:<chat id>`;
      the `:` is what keeps it out of the action namespace `component._ID_RE` governs, and out of
      `palette.matches`' id filter.
- [ ] **Step 4:** **containment.** `contain.one_line` **before** width arithmetic (#472). **Test
      with hostile labels; do not reason about them.** And assert the stronger property: a label
      never reaches a tmux argv or a tmux format string at all — measured, a name routed through
      `rename-window` is stored already expanded, and `#{E:@opt}` expands a `#{...}` in an
      option's value.
- [ ] **Step 5:** a refused switch — the live-chat cap, or a chat whose window is gone — **says
      so on screen**, with which refusal fired.
- [ ] **Step 6:** mutations — skip containment; let a label reach `new-window -n`; swallow the
      refusal. Each RED.
- [ ] **Step 7:** commit.

### Task 7: The two bars

- [ ] **Step 1:** write the failing test — `workspaces` and `chats` are components in the Phase 1
      registry, `chats` **hides itself when there is one chat** showing only "add chat", and both
      drop before `top` when rows are short.
- [ ] **Step 2:** run it, confirm it fails.
- [ ] **Step 3:** implement as `edge="top"`, `size=Fixed(1)`, registered in `builtins.build()` in
      split order, and add both to `layout._DROP_ORDER` above `top`:
      `("right", "repos", "chats", "workspaces", "top")`.
- [ ] **Step 4:** **degradation inside the row.** The active chat's label in full, the others as
      marks and a count; never wraps, never scrolls, never truncates a name below
      `slots._NAME_MIN_W` (12 cells) — below that, marks only. Test at 200, 80 and 40 columns.
- [ ] **Step 5:** the marks come from `charter/inflight.py` where the harness reports, and from
      `#{window_activity_flag}` / `#{window_bell_flag}` where it does not — one
      `display-message -p -F` over the session's windows, the shape `_measure_window` already
      uses. **`monitor-activity on` per chat window; `bell-action` left alone**, because it is
      the operator's terminal preference and charter's conf is session-scoped.
      **The activity flag is a hint** — "bytes moved" includes a spinner — and the docstring says
      so.
- [ ] **Step 6:** mutations — draw the chat bar with one chat; drop it from `_DROP_ORDER`; let a
      name truncate below the minimum. Each RED.
- [ ] **Step 7:** commit.

**Stage 5b exit criteria**

- Every chat is reachable in **≤2 keystrokes** at 200, 80 and 40 columns, and at 40 columns
  where the bars cannot be drawn.
- A hostile chat label renders as one row, runs nothing, and never appears in a tmux argv.
- The switch produces the correct layout **with the `window-resized` hook removed**.
- `F12` returns to the harness from any chat, including one whose palette is deliberately hung.
- The chat bar is absent with one chat and present with two.

---

## Stage 5c — the cost, and the honesty

### Task 8: The cap, the history limit, and the cost readout

- [ ] **Step 1:** write the failing test — creating a chat past `[frame] max_chats` is refused
      **with its reason and the name of the coldest live chat**, and the value is refused rather
      than clamped at the config boundary, as `FRAME_PANE_PAD_MAX` already is.
- [ ] **Step 2:** run it, confirm it fails.
- [ ] **Step 3:** implement. Default **6** — see the spec §6.1 for the arithmetic. Per plane,
      not per workspace, because the memory is the machine's.
- [ ] **Step 4:** **`history-limit` for chat windows.** Charter ships 50,000, and one 200-column
      pane filling it took the shared tmux server from 3.8 MB to 130.6 MB. Operator decision
      §6.2 recommends 20,000. Whatever the answer, it is one committed value, validated at the
      boundary, and `docs/frame.md` states the measured cost beside it.
- [ ] **Step 5:** **the cost readout.** Every chat's token gauge on the bar, not just the one in
      front of you — extend `slots.py`'s existing gauge, which already reads
      `state.harness_session(fid)`, to every chat in the workspace. A chat burning tokens behind
      your back says so.
- [ ] **Step 6:** mutations — clamp the cap instead of refusing; drop the coldest-chat name from
      the refusal; show the gauge only for the active chat. Each RED.
- [ ] **Step 7:** commit.

### Task 9: Cold chats, and the sentence that admits what is lost

> **AMENDED 2026-09-01 — steps 1, 3 and 5 are superseded; step 4 stands.** This task said a
> reopened chat *"starts a fresh harness session"* and *"must not inherit the closed one's
> token gauge"*. The operator reversed the first, and the second turned out to be wrong on
> its own terms. Both reversals are recorded here rather than left to be inferred from a
> later document, because §4j of the IDE spec was silently violated for five days by exactly
> that — a later document citing an earlier one without re-reading it, which shipped a bug
> the operator had to report.
>
> **1. Resume, not fresh.** `docs/superpowers/specs/2026-08-30-charter-opens-like-an-ide.md`
> §4e settles it and the operator settled it again in their own words: *"when opening again
> we need to resume harness sessions as well… to not lose sessions, state etc."* #757 keeps
> the id (`session.durable`) for exactly this, and a reopen appends `--resume <id>` to the
> harness's own argv. This task's *"starts a fresh harness session"* is no longer true and
> was never argued for — it was written when nothing recorded an id to resume from.
>
> **2. Resume is Claude-only, which is what this task was really right about.** §2.8:
> `record_harness_session` has exactly one caller, Claude Code's `statusLine` hook. So the
> honest form of *"a resume that silently starts a new conversation is worse than a sentence
> saying it will"* is not "do not resume" — it is **resume where charter has an id, and say
> per chat which of the four reasons it has none**. That sentence is `frame/leave.py`'s four
> notes, drawn in the quit's confirmation before any keypress commits anything.
>
> **3. The gauge gate is DROPPED, and there are three reasons, the third of which is the
> one that actually settles it.** §4e specified gating the gauge on
> `state.exit_code(fid) is None`.
>
> * **It reads a killed chat as live.** For a chat `kill-window` ended, `exit_code` is
>   `None` (§2.17 — `kill-pane`, `kill-window` and `kill-session` write nothing, measured),
>   so the gate answers "show the gauge" for precisely the case it was written to suppress.
>   That is the inverse of the intent, and it is a fact about charter's own files.
> * **It would blank a correct gauge.** `claude --resume` preserves the harness's session id
>   — one `sessionId` carried across the gap a restart made, in the operator's own
>   transcript — so after a resume the usage history the gauge reads is that same
>   conversation's own. *(Recorded as the operator's measurement; not re-run here, and see
>   the next point for why nothing shipped depends on it.)*
> * **Nothing needs suppressing, because there is nothing to inherit.** A reopen mints a
>   FRESH chat id (point 4), so the reopened chat's directory has no `session` file at all:
>   `state.harness_session` answers `None`, `frame/slots.py`'s own rule draws no gauge, and
>   Claude Code's `statusLine` hook writes the mapping on the chat's first turn from the
>   live payload. Pinned by
>   `tests/test_a_reopen_says_what_it_cannot_bring_back.WhatAReopenPutsBack.
>   test_a_reopened_chat_draws_no_gauge_until_its_own_first_turn`, which is also the
>   assertion that will go red at the stage that relaunches into a chat's own directory —
>   which is where the second point above stops being decoration and starts being the
>   argument.
>
> **The ordering hazard §4e names disappears with the gate**: there is no stage that has to
> ship before another.
>
> **4. Reopening does not relaunch into the chat's OWN directory, and cannot yet.** That
> needs stage 4 of the IDE spec's delivery order — the chat directory becoming durable —
> which is six edits wide and not done. `reap` still deletes a chat directory whose launcher
> pid is dead, and after a restart every launcher pid is dead, so `session.durable` does not
> survive to be read. A reopen therefore mints a FRESH chat id and seeds it from a
> plane-scoped manifest (`.charter/frame/reopen.json`, a file, which `reap` does not collect).
> One consequence worth stating: because the id is fresh, the stale
> `.charter/sessions/<fid>.workspace` rung #791 removes cannot decide a reopened chat's
> membership — **except** where the ordinal is recycled onto the same name, which is common.
>
> **Step 4 stands unchanged and is now load-bearing.** `Harness.launch_argv` is
> `[self.binary, *extra]` with no override anywhere in the registry, so the pass-through IS
> the seam; `frame/leave.resumable_harness` asks the registry which harness records an id,
> and no member was added.

- [x] **Step 1:** ~~closing a chat kills its window and keeps its record; reopening restores
      the workspace, the persona, the harness and the cwd, **starts a fresh harness session**,
      and says so **before** the relaunch.~~ Amended: `chat: close` kills the window and marks
      the chat closed so nothing brings it back; a **quit** is what keeps the record; a reopen
      restores the workspace, the persona, the harness and the cwd, and **resumes** the
      conversation where charter has an id. `tests/test_a_reopen_says_what_it_cannot_bring_back.py`.
- [x] **Step 2:** the tests were written first and failed first; the two guards that could be
      mutated away were each verified RED without them (see step 5).
- [x] **Step 3:** implemented. **`state.clear_shape` is untouched** — it still unlinks
      `session` for #413's reason, which is an argument about a *reading*; the ID survives in
      `session.durable` (#757) and, across a reap, in the manifest. No gauge gate: see the
      amendment above for the two measurements that killed it.
- [x] **Step 4:** **no resume member was added to `Harness`.** The flag rides the existing
      `extra` pass-through, and which harness may be handed it is asked of the registry
      (`frame/leave.resumable_harness`).
- [x] **Step 5:** mutations run and RED: dropping `state.record_cwd` from the private-server
      launch path (`BothLaunchPathsRecordIt`), and pruning `inflight` before the kill instead
      of after (`TheTrackerIsPrunedAfterTheKill`). One property was found to be
      **over-determined** and is recorded as such rather than claimed as guarded: three
      independent rules in `reap` keep a non-directory entry in the frame root, so no single
      mutation can take the manifest's durability away.
- [ ] **Step 6:** commit.

### Task 10: Measure at 5 and 10 chats — §4j's instruction, discharged

- [ ] **Step 1:** the spec's §7.9 says this is **not** discharged by the spec. Every scale figure
      there used `sleep` processes standing in for harnesses. Run six real harnesses in six real
      tabs.
- [ ] **Step 2:** measure, with the command quoted: tmux server RSS and fd count; each panel
      process's RSS and `phys_footprint`; each harness's RSS; wall-clock switch latency from
      keypress to first painted panel.
- [ ] **Step 3:** compare against the spec's extrapolation and **write down where it was wrong**.
      An extrapolation nobody checked is a guess with a table around it.
- [ ] **Step 4:** if the paint lag is visible, the fallback is keeping the previous chat's panels
      alive for one switch — **and only then**, because building that cache now, unmeasured, is
      §4d's layout engine arriving through the back door for the third time.
- [ ] **Step 5:** append the results to the spec's §7. Do not open a new document.
- [ ] **Step 6:** commit.

### Task 11: The docs, and the sentence about boundaries

- [ ] **Step 1:** `docs/frame.md` — what a chat is, how the bars degrade, what `max_chats` and
      the chat `history-limit` cost, and that `bell-action` is deliberately untouched.
- [ ] **Step 2:** **`SECURITY.md` and `docs/frame.md` both carry the sentence**: *a chat tab is a
      container, not a boundary. Two chats in one plane run as the same user, under the same
      plane, with the same vaults, and can read each other's files. A second tab does not add
      that risk — it multiplies the number of processes that hold it.* A tab looks like a
      boundary, which is exactly why the text has to say it is not.
- [ ] **Step 3:** `docs/harnesses.md` — two chats on the same harness share that harness's
      credentials, and charter does not separate them.
- [ ] **Step 4:** the per-persona grant, spelled out for two tabs: each chat's `toolgate.decide`
      grants `effective_tools(its persona) ∩ frozen_tools(its persona, its chat id)`, the
      intersection means narrowing lands at once and widening never, and **a persona can only
      widen a chat, never restrict it below the plane's own guards**.
- [ ] **Step 5:** a news entry. **No version bump, no stamping, no tag.**
- [ ] **Step 6:** commit.

**Stage 5c exit criteria**

- The cap refuses with a reason and a name, and the value is refused rather than clamped.
- Every chat's cost is visible from the chat you are looking at.
- Reopening a cold chat says what it will not restore, before it does not restore it.
- Six real harnesses have been run in six real tabs and the numbers are written down beside the
  extrapolation they test.
- The word "boundary" appears in `SECURITY.md` next to the word "not".

---

## Exit criteria for the phase

- **A workspace holds several chats and switching between them loses nothing** — measured, with
  two real harnesses, across a terminal resize, on tmux 3.7c **and on tmux 3.2**.
- **Every chat is reachable in ≤2 keystrokes at every width**, including widths where neither bar
  can be drawn.
- **A hostile chat label renders as one row and never reaches tmux.**
- **One chat's harness dying does not touch another chat.**
- **`state.reap` bounds `.charter/frame/` under the new id shape**, and old-shape frames still
  launch, report liveness and reap with no migration.
- **The switch works with `window-resized` removed**, because on `tmuxctl.FLOOR` it does not
  exist.
- **Nothing was added to `layout.CARRIABLE`.**
- **`SECURITY.md` says a tab is not a boundary.**
- The suite gives the same answer in two environments **and CI is green at the head sha under
  review**. Two local environments cannot see a CI-only failure: #554's overlay module passed
  12/12 locally while CI was red at that exact head, and `gh pr checks` reports "no checks
  reported" and `mergeStateStatus: CLEAN` identically when no run was ever created (#561). Read
  `gh api repos/diazoxide/charter/commits/<HEAD_SHA>/check-runs`, which cannot confuse the two.
- **The deletion sweep is run by the repository, not promised by whoever wrote the branch** —
  `.github/workflows/sweep.yml` on every PR, and `python3 tools/sweep.py` locally before
  submitting. Four named guards in this phase, four named tests.
- **Implement, PR, merge — never release alone.** No version bump, no stamping, no tag.
