# A run moves only by a named cause, and a chat's state is read from its runs

**Proposed 2026-10-01**, drafted for program-map ticket FD-23 (#660). It follows these of the
operator's rulings:

- **V1:** *"a **run** is one harness conversation segment inside a chat; a child agent is a run
  with a parent run"*. The phase-2 critique's P1-05, which V1 answers, asked for this record: run
  states, derived chat states, one cause per transition, and needs-you as a view.
- **W8:** *"The agent run is the unit of governance, whoever spawned it."* Also: *"child agents
  show under their chat; budgets and the kill switch apply at the parent"*, and vendor-cloud
  sessions are listed *"read-only through each vendor's API or MCP as "remote chats" (state, PR,
  cost)"* and recorded *"in the audit as `observed`, not `governed`"*.
- **Q11:** *"Every agent is listed, observable and stoppable, and no agent's message is ever
  consent. Headless agents are allowed."*
- **N4:** *"per-task budgets (spend/time/tool calls) with auto-pause → needs-you"*.
- **N8:** *"anomaly detection (loops, denial spikes, runaway spend, stuck chats) that alerts or
  pauses"*.
- **N9:** *"kill switch (stop all agents, revoke tokens; policy-triggerable)"*.
- **X35**, accepted with the consistency review as recommended: *"does hibernation apply only to
  harnesses whose M10 row marks resume as native, with others kept hot or closed with a session
  record?"*
- **V8:** *"Lazy restore at relaunch."*

It builds on [ADR 0066](0066-a-chat-is-a-ulid-a-run-is-a-stretch-of-its-conversation-and-a-device-is-random.md)
(chat, run and device identity), [ADR 0068](0068-a-chat-lives-in-charterd-and-the-app-is-its-client.md)
(`charterd`, its grace period and crash recovery), [ADR 0071](0071-the-kill-switch-is-machine-state-the-command-line-stops-and-only-the-window-re-arms.md)
(the kill switch), [ADR 0073](0073-a-harness-is-declared-as-data-and-a-chat-runs-it-at-one-of-three-levels.md)
(harness levels and `fallback`), ADR 0074 (a refused commit, charter#770) and
[ADR 0075](0075-an-audit-entry-is-metadata-in-a-store-of-its-own-and-telemetry-never-reads-it.md)
(audit entries). It **amends** ADR 0066, ADR 0068 and ADR 0075, each in a section of its own
below. FD-29 (crash recovery), SC-4 (hibernation), OV-7 (budget pause), AC-7 (triggers), FD-19
(remote chats), IB-10 (the inbox) and FD-12's board all build on it.

Its concept is **Chat** ([ADR 0072](0072-charter-has-five-concepts-and-every-other-word-belongs-to-one-of-them.md)).

## Where charter is today

`crates/charter-core/src/state.rs` holds a chat's state as one of five values: `Unknown`,
`Running`, `Waiting`, `Done` and `Failed`. Hooks move it, and so does the program's exit. Beside
the state, a `needs_you` flag says whether the chat is in the queue, with three kinds of item on
top of it: an ask (`asking`), a report back (charter-app#259) and, once charter#770 lands, a
refused commit (ADR 0074). `docs/spec.md` names the same five as the *session state*.

That machine was built for one chat, one process and one conversation. The program map adds
states without adding them to it, and each ticket would add its own flag:

- a chat **paused** by its budget (OV-7, N4) or by anomaly detection (N8);
- a chat **hibernated** with no process, to be woken on focus (SC-4);
- a chat **stopped** by the kill switch (OV-1). Today it reads `Failed`, because a signal leaves
  no exit code, and so does a chat the operator closed mid-turn;
- a chat **queued** by a trigger (AC-7), a lazy restore (V8) or the launch question
  (charter-app#250), which today has no state at all;
- a chat whose process died with `charterd` (FD-29), which ADR 0068 calls `failed(host-crash)`;
- a **remote chat** charter only observes (FD-19);
- a child agent (FD-18), which ADR 0066 made a run, and which has no state yet.

ADR 0066 left the states to this record: *"FD-23 gives runs their states, and derives the chat's
from them."*

## The decision

**A run is in one of nine states. It moves between them only by a cause from a closed list, and
each cause is a fact that arrives from a hook, the protocol, the process exit, the host's own act,
the operator or a policy. Nothing is read from output. A run ends exactly once, in one of three
end states, and only on a fact from outside the chat's own process tree. A chat's state is read
from its current run and never stored. Needs you is a view over the runs and a chat's own items,
and never a state.**

### 1. Nine states

| State | The run | Has a process |
|---|---|---|
| `queued` | exists, and waits to be admitted. Its **hold** says why (§3) | no |
| `starting` | its program has been started, and its harness has not reported yet | yes |
| `working` | a turn is in flight | yes |
| `input-required` | the harness has handed control back. Its **reason** says why: `asked` (a question or a permission, in the middle of a turn), `turn-ended` (the turn is over) or `ready` (a session began and nothing has been asked) | yes |
| `paused` | charter is holding it: no tool call runs and no input is delivered until it is unpaused. Its hold says who paused it | yes, stopped (§4) |
| `hibernated` | its program was ended on purpose at a turn boundary, and its conversation is kept to be resumed | no |
| `completed` | **end.** Its program exited cleanly, or the next run of the chat took over from it | no |
| `failed` | **end.** Its program ended in a way nobody chose: a non-zero exit, a signal charter did not send, a start that could not happen, a lost protocol channel or a host crash | no |
| `stopped` | **end.** A person, a policy or the host ended it on purpose | no |

`queued` to `hibernated` are **live**. The three ends are final: a run never leaves one, and a
report that arrives for an ended run is refused, as a report for an exited chat is today.

**A run never goes back to `starting` or `queued`.** Carrying on after an end is always a new
run, with an ADR 0066 cause (`reopen`, `wake`, `fresh`, `switch`, `fallback`).

**A shell tab has no run** (ADR 0066: a run is a harness conversation). Its program is shown
alive or ended, and the kill switch ends it, as ADR 0071 says.

### 2. Every move names one cause

**The rule: every transition names exactly one cause, from the closed list below, and each cause
is one kind of fact.** The same cause may appear in more than one row. A move with no cause, or
a cause outside the list, is refused by `state.rs`, and a test holds it there. This keeps spec
decision 3, *nothing parses output*, true for every state.

A run also has ADR 0066's `cause`, which says why the run *began*. That list is unchanged. This
record's causes say why a run *moved*. The two lists share no word.

| From | To | Cause | The fact it comes from |
|---|---|---|---|
| (none) | `queued` | `requested` | the host was asked to start a run: the operator, a relaunch, a handoff, **Resume**, a curation action, a trigger, a wake, or a `switch` or `fallback` that needs a new process |
| (none) | the state its predecessor was in | `succeeded` | a new run begins in the same process: `clear`, or a `switch` that keeps the process (a persona adopted, a model a hook reports). The predecessor ends `completed \| superseded` in the same step |
| `queued` | `starting` | `admitted` | the host's own act, once no hold is left (§3) |
| `queued` | `stopped` | `refused` | a gate refused the start: the kill switch (ADR 0071), `minimumHarnessLevel` (ADR 0073 §7) or an unapproved harness declaration (V24b). The event names the gate |
| `starting` | `input-required (ready)` | `session-reported` | level 2: the harness's `SessionStart` for a session beginning. Level 3: the protocol's new or loaded session |
| `starting` | `input-required (ready)` | `spawned` | level 2, for a harness whose declaration answers *no* to `reports_its_start_before_the_first_prompt` (ADR 0073 §6): the host started the program, and nothing more will come before a prompt |
| `starting` | `working` | `spawned` | level 1: the host started the program. A level-1 run is `working` until it ends (below) |
| `starting`, `input-required` | `working` | `prompted` | level 2: `UserPromptSubmit`. Level 3: the host delivered a prompt |
| `input-required (asked)` | `working` | `answered` | the turn went on after an ask: at level 2, a tool hook from the same run; at level 3, the host delivered the answer |
| `working` | `input-required (asked)` | `asked` | level 2: a `Notification` while a turn runs. Level 3: an ask in the protocol |
| `working`, `input-required (asked)` | `input-required (turn-ended)` | `turn-ended` | level 2: `Stop`. Level 3: the protocol's end of turn |
| `working`, `input-required` | `paused` | `budget` | the chat's budget ran out (N4, OV-7). The host's own act, from its own counts |
| `working`, `input-required` | `paused` | `policy` | anomaly detection or a policy paused it (N8). The host's own act |
| `working`, `input-required` | `paused` | `operator` | the operator paused it, from a human client scope (ADR 0068 §5) |
| `paused` | the state it was paused from | `unpaused` | the operator unpaused it, or raised the budget, from a human client scope. Only the operator unpauses, whoever paused |
| `input-required (ready or turn-ended)` | `hibernated` | `idle` | the host's own act: idle past the threshold, on a harness that resumes natively (§5) |
| any live state | `completed` | `superseded` | the chat's next run began: the `succeeded` row above, or a `reopen`, `fresh`, `switch` or `wake` (a hibernated run ends this way when its chat wakes) |
| `starting`, `working`, `input-required` | `completed` | `exited` | the program exited with code 0, and the host had not stopped it |
| `starting`, `working`, `input-required`, `paused` | `failed` | `exited` | the program exited non-zero or on a signal the host did not send |
| `queued`, `starting` | `failed` | `spawn-failed` | the program could not be started: not found, not executable, or its sandbox could not be compiled |
| `working`, `input-required` | `failed` | `channel-lost` | level 3: the protocol channel ended or reported an error charter cannot continue past, while the chat carries on at a lower level. The next run begins with ADR 0073's `fallback` |
| `working`, `paused` | `failed` | `host-crash` | the next host found the run in its journal, and its program gone (FD-29, ADR 0068 §3) |
| `input-required (ready or turn-ended)` | `hibernated` | `host-crash` | the same, for a run idle at a turn boundary on a harness that resumes natively (ADR 0068, amended) |
| `input-required` | `failed` | `host-crash` | the same, for any other `input-required` run |
| any live state | `stopped` | `closed` | the operator closed the chat: its tab (Smart close included, ADR 0064), or **Start fresh** at the launch question, which drops it from the reopen record |
| any live state | `stopped` | `operator` | the operator stopped this run and kept its tab (Q11) |
| any live state | `stopped` | `killed` | the kill switch was thrown (ADR 0071), from the window, the command line or a policy (N9) |
| any live state | `stopped` | `quit` | the app let go of the project, a quit or the project closed, and the reopen record keeps the chat |
| any live state | `stopped` | `grace` | the app crashed and did not come back within ADR 0068's grace period |
| `input-required` | `stopped` | `drained` | an upgrade fell back to drain and resume (ADR 0068 §7). The chat comes back with a `reopen` run |

A hibernated or queued run has no process to lose, so a host crash leaves it as it is.

**A stop the host caused is a stop, whatever the exit code says.** The host writes its intent
before it sends the signal. The exit that follows ends the run `stopped`, with the host's cause,
even though a signal leaves no code. This refines the spec's *"`failed` is a non-zero exit"*
(operator's ruling, 2026-09-18): `failed` is a non-zero exit that charter did not cause.

**An end is only ever entered on a fact the chat cannot forge**: the exit status, the host's own
act, a human client scope, a policy, or the protocol channel closing. So **`SessionEnd` ends
nothing.** It is recorded, and the run ends at the exit that follows. `state.rs` already refuses
to let `SessionEnd` mark a chat ended, for this reason (*"let anything in the chat's own process
tree freeze a WORKING chat at `done` with one forged report"*). This record extends the rule from
the `ended` flag to the state. Hooks still move the four states in the middle, because a forged
`Stop` costs a wrong mark and no lost work.

**A `Notification` after a turn has ended moves nothing.** The run is already `input-required
(turn-ended)`. That is today's rule, which tells Claude Code's idle nudge from an ask.

### 3. A queued run says what it waits for

`queued` has one state and several **holds**. A run is admitted when it has none left:

| Hold | Waits for | From |
|---|---|---|
| `launch` | the operator's answer to the launch question. Nothing a record names starts before it | charter-app#250, ADR 0035 |
| `lazy` | the chat to be shown or prompted, for a chat put back at a relaunch | V8 |
| `budget` | the operator to raise the chat's budget, which is spent | N4 |
| `capacity` | a slot, where a trigger queue or a cap on running chats is full | AC-7 |
| `disk` | free disk above V8's admission guard | V8 |

Which chats a relaunch restores lazily is V8's ticket to decide. This record only gives it a
state to use.

### 4. Paused means stopped in place

**A paused run's process group is stopped with `SIGSTOP`, and unpausing sends `SIGCONT`.** That
is the terminal's own job control, what Ctrl-Z does. It stops spending at once, at every level,
without the harness's help. The guard also refuses any tool call that reaches it while the run is
paused, for a hook already in flight.

A paused run keeps the state it was paused from, and goes back to it on `unpaused`. Hooks that
were queued in the kernel arrive after `SIGCONT` and move the run as usual. A model request in
flight when the run was paused may fail when it resumes, and the harness retries it or reports
it. That is the price of a pause that works at level 1 (ADR 0073 §1: *"Level 1 is a complete
product"*).

**Budgets pause at the chat, and the whole chat pauses.** Settled by W8: *"budgets and the kill
switch apply at the parent"*. A child run shares its parent's process group, so stopping the
group stops the children too. A chat whose budget is spent when its next run is asked for keeps
that run `queued` with the `budget` hold, rather than starting it only to pause it.

### 5. Hibernation

**Only a harness that resumes natively hibernates. Settled by X35** (*"does hibernation apply only
to harnesses whose M10 row marks resume as native, with others kept hot or closed with a session
record?"*). In this record's terms, that is a harness whose declaration answers *yes* to
`resumes_by_id` (ADR 0073 §6).

- **Only at a turn boundary.** A run hibernates from `input-required` with reason `ready` or
  `turn-ended`, never `asked`, because the ask would be lost with the process. Never from
  `working` or `paused`.
- **The idle threshold** is SC-4's: at least the harness's cache TTL, 10 minutes by default (V8,
  through SC-4's row).
- **Waking is a new run.** The hibernated run ends `completed | superseded`, and the next begins
  with ADR 0066's `wake`, `queued` and then admitted. A wake is a start, so the kill switch
  refuses it.

### 6. A child run's states

A child run (ADR 0066) has three states only:

| From | To | Cause | The fact |
|---|---|---|---|
| (none) | `working` | `child-seen` | the first hook carrying a new `agent_id` for this chat, on a harness where that field is measured |
| `working` | `completed` | `child-ended` | the matching `SubagentStop` |
| `working` | the parent's end state | the parent's cause | the parent run ended first |

A child's asks come on its parent's hooks, so a child is never `input-required`. It is paused
with its parent (§4) and is never queued or hibernated. Its moves are events, so the board shows
it under its chat (W8). No child move changes the chat's own state.

### 7. Remote chats: the `observed` kind

**Every chat has a kind: `governed`, for a chat charter starts, or `observed`, for a
vendor-cloud session charter lists (FD-19). Settled by W8** (*"read-only"*, and *"records them in
the audit as `observed`, not `governed`"*).

- An observed chat's runs use the same nine states. The vendor's status is mapped onto them, and
  **every move has the one cause `observed`**: the vendor reported it. The host causes no move.
- There is no pause, hibernation, budget enforcement or queue for them. Their cost is shown, and
  never counted toward a budget charter enforces.
- **The kill switch does not reach them**, and the window says so beside **Stop all**. A read-only
  listing cannot stop anything. Q11's *"stoppable"* is met by a link to the vendor's own page for
  the session. See the questions below.
- Their audit coverage is `observed` (ADR 0075 §5), and their outcome goes into a session record,
  as W8 says.

### 8. A chat's state is derived

**A chat stores no state of its own.** What the window draws is read from its runs:

| The chat | When |
|---|---|
| **open** | it has a tab. The sidebar draws its **current run's** state, with the reason or hold. A chat put away by a quit is still open: it is in the reopen record, and its run ended `stopped \| quit` |
| **wrapping up** | open, with a Smart close sent and its record not yet saved (ADR 0064). A flag on an open chat, not a run state |
| **closed with a record** | its tab closed after `charter session record` saved its session record |
| **closed without a record** | its tab closed any other way |

**Closed is final.** No run begins in a closed chat. **Resume** starts a new chat from the
record, with `resumed_from` (ADR 0066, ADR 0064).

The event log carries `chat.opened` and `chat.closed` (with whether a record was saved), and for
each run `run.started` (ADR 0066's cause), `run.moved` (from, to, this record's cause, and the
reason or hold) and `run.ended` (the end state, the cause, and the exit code where there is one).
FD-9 lists them with the other kinds.

### 9. Needs you is a view

**Needs you is computed, never stored.** A chat is in it while it has at least one of these
items:

| Item | From |
|---|---|
| its current run is `input-required` with reason `asked` or `turn-ended` | hooks, the protocol |
| its current run is `paused` with hold `budget` or `policy` (not the operator's own pause) | N4, N8 |
| its next run is `queued` with hold `budget` | N4 |
| another chat reported back to it (charter-app#259) | the hook channel |
| a commit it made was refused (ADR 0074). Settled by V26a: *"A refused-commit needs-you item clears on the chat's next prompt."* | the hook channel |
| a command in it waits for a secret's approval. Settled by V15: *"the ask tops needs-you"* | the host (V15, V16) |

A run that is `ready` is not an item: a chat that has just started or been put back has asked for
nothing. **Ignore** (charter-app#248) clears a chat's items until the next one arrives, and
changes no run. The order, priorities and channels of the view are TS4's and IB-10's.

### 10. What ends each concept

The five concepts end in different ways. None of them ends a run except through a cause above.

| Concept | Its end | What it does to runs |
|---|---|---|
| **Chat** | closed (§8). Final | its live run ends `stopped \| closed` |
| **Workspace** | removed (`charter workspace remove`, the safe-remove skill). Final in charter, recoverable from git while committed | **refused while any chat in it has a live run**, as a rename is refused today. A removal must not leave a program running in a deleted directory |
| **Persona** | removed. Final in charter, recoverable from git while committed | **refused while a live run has adopted it**. A run's persona is fixed (ADR 0066), and its files must outlast the run |
| **Project** | closed on this machine, which lets go of it and keeps it on disk. Opened again later | every live run ends `stopped \| quit`, and the reopen record is kept |
| **Memory** | archived, with Undo (ADR 0065) | none |

The run's own end states are §1's. A device does not end in charter. A deleted machine store
makes it a new device (ADR 0066).

## ADR 0066, amended

ADR 0066 says when a run begins. It does not say when a run ends. It now also says:

- **A run ends exactly once, with an end state and a cause** from this record. A run whose chat
  carries on in a new run ends `completed | superseded` at the moment the next one begins, so a
  chat has at most one live top-level run.
- **A child run ends with its parent** when the parent ends first (§6).
- **Every chat has a kind**, `governed` or `observed` (§7). ADR 0066 already gave an observed chat
  an id when charter first lists it.

## ADR 0068, amended

- **§3, what a crash of the host costs.** ADR 0068 marks every run that was `working` as
  `failed(host-crash)`. That stands, and `paused` runs join it. **A run that was `input-required`
  at a turn boundary (`ready` or `turn-ended`) becomes `hibernated` instead**, where its harness
  resumes natively, because nothing was lost: its conversation is whole and waiting for a prompt.
  Where the harness does not resume natively, it is `failed | host-crash`. A run that was
  `asked` is `failed`, because the ask was lost. See the questions below.
- **The run journal holds each live run's state**, not only its conversation id: the chat and run
  ids, the state with its reason or hold, the state a paused run was paused from, the
  conversation, the harness and level. The host replaces it whole at every move, as `reopen.json`
  is replaced. It is at `<config>/charterd/runs.json`, so the chat sandbox's denial of
  `<config>/charterd/` (ADR 0067, ADR 0069 §6) covers it. The host reads it at start and never
  clears it as it clears the credentials beside it.
- **§2, the grace period.** Runs that end when it passes end `stopped | grace`.
- **§7, the drain fallback.** A run ended at its turn end ends `stopped | drained`, and the chat
  comes back with a `reopen` run.
- **§7, a handed-over program's lost exit code.** Such a program's end is learned from the end of
  its output, with no code. The run ends `completed | exited` when a `SessionEnd` for good came
  first, or at level 1, and `failed | exited` otherwise. See the questions below.

## ADR 0075, amended

- **`run.ended`'s `meta` gains `state`** (`completed`, `failed` or `stopped`) and the end
  cause, beside the ADR 0066 `cause` it already carries for `run.started`.
- **Two actions join the registry: `run.paused` and `run.unpaused`.** Their `meta` is the cause
  (`operator` or `policy`) and the run's state at the time. The actor is `human` for the operator
  and `host` for a policy. A budget pause stays ADR 0075's `budget.paused`.
- Every other move (`working` ⇄ `input-required`, `hibernated`) is an event and not an audit
  entry. It says what a harness was doing, not who did what, so it is telemetry's (O1).

## What changes where

The code does not change with this record.

| Where | What changes |
|---|---|
| `crates/charter-core/src/state.rs` | `State` becomes the nine run states, with `input-required`'s reason and the holds. A `Move { from, to, cause }` is the one way a run changes, and a table of allowed `(from, to, cause)` refuses the rest. **One test per row of §2's table and §6's** (FD-23's acceptance). `needs_you` becomes a function over the run and the chat's items. `exited` reads the host's stop intent first. `SessionEnd` no longer sets `Done` |
| `Board`, the app's `hooks.rs` | Track each chat's current run and its children. `chat-moved` carries the run state and its reason or hold |
| `app/src/chatState.ts`, `NeedsYou.tsx` | Draw the nine states, and read needs you from the items. The words shown are the design system's (ADR 0072 §3) |
| `docs/spec.md` | Its *Session state* line is replaced by this record's states, when `state.rs` changes |
| `charterd` (FD-5, FD-29) | Writes the run journal, and does the crash accounting of *ADR 0068, amended* |
| `halt.rs`, `Planes::stop_every_agent` | Writes the stop intent before signalling, so each run ends `stopped \| killed` |
| OV-7, SC-4, AC-7, V8's lazy restore | Use `paused`, `hibernated` and `queued` with their holds, and add no state of their own |
| FD-19 | The `observed` kind |
| The workspace and persona removal paths | Refuse while a live run is in the workspace or has adopted the persona (§10) |
| `docs/plane-format.md` | `<config>/charterd/runs.json`, **decided, not yet written** (in this PR) |
| `CONTEXT.md` | Gains **Run state** and **Remote chat**. **Needs you** is restated as a view (in this PR) |

## What this costs

- **A pause may cost a model request.** `SIGSTOP` is immediate and works at every level, and a
  request in flight may fail on resume.
- **A wrapper profile that outlives its harness keeps the run open.** `SessionEnd` no longer ends
  a run, so a wrapper that stays alive after its harness exits keeps the run `input-required`
  until the wrapper exits. Its pane shows it.
- **A tool hook now moves one state.** An answered ask at level 2 is learned from the next tool
  hook of the same run. Until one arrives, the chat still reads `asked`. That is today's behaviour,
  and it gets better, not worse.
- **Two cause lists.** ADR 0066's `cause` says why a run began, and this record's says why it
  moved. The event says which it is.
- **Refusing a removal while a chat runs** is one more thing the operator must close first.

## What was rejected

- **A2A's `TaskState` as the state list.** A2A's `submitted`, `working`, `input-required`,
  `completed`, `failed` and `canceled` map onto `queued`, `working`, `input-required`,
  `completed`, `failed` and `stopped`, and FD-12's board uses that map. But A2A has no hold, no
  process-less state and no `starting`. Its `auth-required` is an ask, which is `input-required
  (asked)` here. Its `rejected` is `stopped | refused`.
- **Needs you as a state.** A chat that is `working` can also have a report back waiting. A state
  can hold one of the two, and the queue has to show both.
- **Storing the chat's state.** It would be a second copy of the current run's, and the two would
  disagree after a crash.
- **`detached`, for a chat whose window went away (FD-7).** A window is a client (ADR 0068). A
  run does not move when a client leaves.
- **`wrapping up` as a run state.** The run is `working` while it writes its record. Smart close
  is something the chat is doing, not something the run is.
- **`SessionEnd` as the end of a run.** The chat could forge it. The exit is the fact.
- **Pausing only by refusing tool calls.** The model keeps generating, and spending, until the
  turn ends. There is no tool call at level 1 to refuse.

## For the operator's ruling

The calls that V1, W8, Q11, N4, N8, N9, X35 and V26a already make are marked *Settled by* in the
body. These are left open:

1. **After a host crash, a run that was idle at a turn boundary becomes `hibernated`, not
   `failed`**, where its harness resumes natively. *Recommend yes.* Nothing was lost, and calling
   it a failure teaches the operator to ignore failures.
2. **A handed-over program's end with no exit code is `completed` when a `SessionEnd` came first,
   or at level 1, and `failed` otherwise.** *Recommend yes.* It uses the one fact there is, and a
   forged `SessionEnd` can only hide a failure the operator sees in the pane anyway.
3. **A pause is `SIGSTOP` of the run's process group at every level**, with the guard refusing
   tool calls as a backstop. *Recommend yes.* It is the only lever that stops spending at once at
   level 1, and it is the terminal's own job control.
4. **A run that ends `failed` puts its chat in needs you until the chat is shown**, and
   `completed` and `stopped` do not. *Recommend yes.* A host crash's one-click resume needs a place
   to be offered, and W10 already asks the queue to show failed chats.
5. **Q11's "stoppable" does not reach a remote chat.** Charter links to the vendor's own page to
   stop it, and **Stop all** says it did not stop them. *Recommend yes.* W8 made remote chats
   read-only, and a stop charter cannot carry out would be a promise it cannot keep.
6. **Removing a workspace or a persona is refused while a live run is in it or has adopted it.**
   *Recommend yes.* A rename already refuses, and a removal must not leave a program running in a
   deleted directory.
