# A run moves only by a named cause, and a chat's state is read from its runs

**Accepted 2026-10-01** by the operator (ruling V27), drafted for program-map ticket FD-23 (#660). It follows these of the
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
- **V8:** *"Lazy restore at relaunch."* Its ticket SC-20 reads: *"Visible, needs-you and mid-turn
  chats restart first, staggered; the rest appear hibernated and resume on focus"*.
- **W10:** *"LW-5's queue and IB-10 show finished and failed chats"*.

It builds on [ADR 0066](0066-a-chat-is-a-ulid-a-run-is-a-stretch-of-its-conversation-and-a-device-is-random.md)
(chat, run and device identity), [ADR 0068](0068-a-chat-lives-in-charterd-and-the-app-is-its-client.md)
(`purlisd`, its grace period and crash recovery), [ADR 0071](0071-the-kill-switch-is-machine-state-the-command-line-stops-and-only-the-window-re-arms.md)
(the kill switch), [ADR 0073](0073-a-harness-is-declared-as-data-and-a-chat-runs-it-at-one-of-three-levels.md)
(harness levels and `fallback`), [ADR 0074](0074-a-chats-git-runs-charters-hooks-through-its-environment.md) (a refused commit) and
[ADR 0075](0075-an-audit-entry-is-metadata-in-a-store-of-its-own-and-telemetry-never-reads-it.md)
(audit entries). It **amends** ADR 0066,
[ADR 0067](0067-a-chat-runs-in-a-sandbox-charter-compiles-for-its-harness.md), ADR 0068,
ADR 0071, ADR 0075 and the hooks-only rule in `AGENTS.md`, each in a section of its own below.
FD-29 (crash recovery), SC-4 (hibernation), SC-20 (lazy restore), OV-7 (budget pause), AC-7
(triggers), FD-19 (remote chats), IB-10 (the inbox) and FD-12's board all build on it.

**Two things here pre-empt later tickets**, so each can start from them and may add to them:
the run journal's contents (*ADR 0068, amended*), ahead of FD-29, and the actions `run.paused`
and `run.unpaused` (*ADR 0075, amended*), ahead of AU-2's registry and AU-4's sources.

Its concept is **Chat** ([ADR 0072](0072-charter-has-five-concepts-and-every-other-word-belongs-to-one-of-them.md)).

## Where purlis is today

`crates/purlis-core/src/state.rs` holds a chat's state as one of five values: `Unknown`,
`Running`, `Waiting`, `Done` and `Failed`. Hooks move it, and so does the program's exit. Beside
the state, a `needs_you` flag says whether the chat is in the queue, with three kinds of item on
top of it: an ask (`asking`), a report back (purlis#259) and a refused
commit (ADR 0074). `docs/spec.md` names the same five as the *session state*.

That machine was built for one chat, one process and one conversation. The program map adds
states without adding them to it, and each ticket would add its own flag:

- a chat **paused** by its budget (OV-7, N4) or by anomaly detection (N8);
- a chat **hibernated** with no process, to be woken on focus (SC-4);
- a chat **stopped** by the kill switch (OV-1). Today it reads `Failed`, because a signal leaves
  no exit code, and so does a chat the operator closed mid-turn;
- a chat **queued** by a trigger (AC-7), a lazy restore (SC-20) or the launch question
  (purlis#250), which today has no state at all;
- a chat whose process died with `purlisd` (FD-29), which ADR 0068 calls `failed(host-crash)`;
- a **remote chat** purlis only observes (FD-19);
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
| `paused` | purlis is holding it: no tool call runs and no input is delivered until it is unpaused. Its hold says who paused it | yes, stopped (§4) |
| `hibernated` | its program was ended on purpose at a turn boundary, and its conversation is kept to be resumed | no |
| `completed` | **end.** Its program exited cleanly, or the next run of the chat took over from it | no |
| `failed` | **end.** Its program ended in a way nobody chose: a non-zero exit, a signal purlis did not send, a start that could not happen, a lost protocol channel or a host crash | no |
| `stopped` | **end.** A person, a policy or the host ended it on purpose | no |

`queued` to `hibernated` are **live**. The three ends are final: a run never leaves one, and a
report that arrives for an ended run is refused, as a report for an exited chat is today.

**A run never goes back to `starting` or `queued`.** Carrying on after an end is always a new
run, with an ADR 0066 cause (`reopen`, `wake`, `fresh`, `switch`, `fallback`).

**A shell tab has no run** (ADR 0066: a run is a harness conversation). Its program is shown
alive or ended, and the kill switch ends it, as ADR 0071 says.

### 2. Every move names one cause

**The rule: every move names exactly one cause, from the closed list below, and each cause is
one kind of fact.** This is how this record reads the ticket's *"each transition with its one
allowed cause"*: many pairs of states have several causes (`working` to `paused` has three), so
the rule is about each move, not each pair. The same cause may appear in more than one row. A move with no cause, or
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
| `input-required (ready or turn-ended)` | `hibernated` | `quit` | the app let go of the project, for a run idle at a turn boundary on a harness that resumes natively (§3) |
| any live state | `completed` | `superseded` | the chat's next run began: the `succeeded` row above, or a `reopen`, `fresh`, `switch` or `wake` (a hibernated run ends this way when its chat wakes) |
| `starting`, `working`, `input-required`, `paused` | `completed` | `exited` | the program exited with code 0, and the host had not stopped it |
| `starting`, `working`, `input-required`, `paused` | `failed` | `exited` | the program exited non-zero or on a signal the host did not send |
| `queued`, `starting` | `failed` | `spawn-failed` | the program could not be started: not found, not executable, or its sandbox could not be compiled |
| `working`, `input-required` | `failed` | `channel-lost` | level 3: the protocol channel ended or reported an error purlis cannot continue past, while the chat carries on at a lower level. The next run begins with ADR 0073's `fallback` |
| `starting`, `working`, `paused` | `failed` | `host-crash` | the next host found the run in its journal, and its program gone (FD-29, ADR 0068 §3) |
| `input-required (ready or turn-ended)` | `hibernated` | `host-crash` | the same, for a run idle at a turn boundary on a harness that resumes natively (ADR 0068, amended) |
| `input-required` | `failed` | `host-crash` | the same, for any other `input-required` run |
| any live state | `stopped` | `closed` | the operator closed the chat: its tab (Smart close included, ADR 0064), or **Start fresh** at the launch question, which drops it from the reopen record |
| any live state | `stopped` | `operator` | the operator stopped this run and kept its tab (Q11) |
| any live state | `stopped` | `killed` | the kill switch was thrown (ADR 0071), from the window, the command line or a policy (N9) |
| any live state but `hibernated` | `stopped` | `quit` | the app let go of the project, a quit or the project closed, and the reopen record keeps the chat. A run idle at a turn boundary on a harness that resumes natively is hibernated instead (row above), and a run already `hibernated` stays so |
| `input-required (ready or turn-ended)` | `hibernated` | `grace` | the same as `hibernated \| quit`, when ADR 0068's grace period passes |
| any live state but `hibernated` | `stopped` | `grace` | the app crashed and did not come back within ADR 0068's grace period. As with `quit`, an idle run on a harness that resumes natively is hibernated instead, and a hibernated run stays so |
| `input-required` | `stopped` | `drained` | an upgrade fell back to drain and resume (ADR 0068 §7). The chat comes back with a `reopen` run |

A hibernated or queued run has no process to lose, so a host crash leaves it as it is.

**A stop the host caused is a stop, whatever the exit code says.** The host writes its intent
before it sends the signal. The exit that follows ends the run `stopped`, with the host's cause,
even though a signal leaves no code. This refines the spec's *"`failed` is a non-zero exit"*
(operator's ruling, 2026-09-18): `failed` is a non-zero exit that purlis did not cause.

**A hook never leaves a chat without a live run, and a hook alone never makes a run
`completed`.** Precisely:

- **A chat's last run ends only on a fact the chat cannot forge**: the exit status, the end of
  the program's output, the host's own act, a human client scope, a policy, or the protocol
  channel closing.
- **A hook may end a run in two ways only.** It may begin a successor in the same process (the
  `succeeded` row, from `SessionStart` with `source: clear`), which ends the run `completed |
  superseded` and leaves the chat a live run. And a `SubagentStop` ends a **child** run
  `completed | child-ended` (§6), which never ends the chat's own run. A forged one costs a wrong
  run boundary or a child shown as done, and no lost work.
- **`SessionEnd` ends no run.** It is recorded, and the run ends at the exit that follows. Its one
  other use is as evidence where an exit code was lost in an upgrade (*ADR 0068, amended*). There
  the run ends on the end of the program's output, which the chat cannot forge, and `SessionEnd`
  only decides between `completed` and `failed`, with the code recorded as lost either way. `state.rs` already refuses
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
| `launch` | the operator's answer to the launch question. Nothing a record names starts before it | purlis#250, ADR 0035 |
| `stagger` | its turn in a relaunch's staggered restart | SC-20 |
| `budget` | the operator to raise the chat's budget, which is spent | N4 |
| `capacity` | a slot, where a trigger queue or a cap on running chats is full | AC-7 |
| `disk` | free disk above V8's admission guard | V8 |

**A relaunch follows SC-20 in these states.** When the app lets go of a project (a quit, the
project closed, the grace period passing), each live run that is idle at a turn boundary, on a
harness that resumes natively, becomes `hibernated | quit` (or `hibernated | grace`, when the
grace period passed) rather than `stopped`. Nothing is lost: the program ends, as ADR 0068 §2
says, and the conversation is kept. A run already `hibernated` stays so. Every other live run
ends `stopped | quit` (or `stopped | grace`). At the next launch, once the launch question is
answered:

- visible, needs-you and mid-turn chats get a new run (ADR 0066's `reopen`), `queued` with the
  `stagger` hold, and restart first;
- the rest **appear hibernated**, as SC-20 says, and each gets its `reopen` run when it is shown
  or prompted.

The hibernated state survives the relaunch in the run journal (*ADR 0068, amended*).

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

**A process that left the group is not stopped.** A tool subprocess that calls `setsid` (a
daemon, some MCP servers, a dev server started in the background) is in a process group of its
own, so it keeps running while the run is paused. What stops is the harness and whatever stayed
in its group, which is where model requests come from, so spending stops. A daemon that spends on
its own, or does work of its own, goes on. The kill switch has the same gap today (ADR 0071
signals process groups). Holding every process a chat started is the sandbox's work: a cgroup
freezer on Linux, and on macOS nothing purlis can use yet (SD-2).

**Budgets pause at the chat, and the whole chat pauses.** Settled by W8: *"budgets and the kill
switch apply at the parent"*. A child run shares its parent's process group, so stopping the
group stops the children too. A chat whose budget is spent when its next run is asked for keeps
that run `queued` with the `budget` hold, rather than starting it only to pause it.

### 5. Hibernation

**Only a harness that resumes natively hibernates. Settled by X35** (*"does hibernation apply only
to harnesses whose M10 row marks resume as native, with others kept hot or closed with a session
record?"*). In this record's terms, that is a harness whose declaration answers *yes* to
`resumes_by_id` (ADR 0073 §6).

**Of X35's two answers for the others, this record takes "kept hot".** A chat on a harness that
cannot resume natively is never hibernated, and it is never closed for being idle. Closing it
with a session record is left to the operator's own Smart close (ADR 0064), because a close is
final (§8) and purlis does not end a chat that nobody asked it to end.

- **Only at a turn boundary.** A run hibernates from `input-required` with reason `ready` or
  `turn-ended`, never `asked`, because the ask would be lost with the process. Never from
  `working` or `paused`.
- **The idle threshold** is SC-4's: *"at least the harness's cache TTL (default 10 minutes)"*.
- **Waking is a new run.** The hibernated run ends `completed | superseded`, and the next begins
  with ADR 0066's `wake` (or `reopen`, for a run hibernated by a quit, §3), `queued` and then
  admitted. A wake is a start, so the kill switch
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

**Every chat has a kind: `governed`, for a chat purlis starts, or `observed`, for a
vendor-cloud session purlis lists (FD-19). Settled by W8** (*"read-only"*, and *"records them in
the audit as `observed`, not `governed`"*).

- An observed chat's runs use the same nine states. The vendor's status is mapped onto them, and
  **every move has the one cause `observed`**: the vendor reported it. The host causes no move.
- There is no pause, hibernation, budget enforcement or queue for them. Their cost is shown, and
  never counted toward a budget purlis enforces.
- **The kill switch does not reach them**, and the window says so beside **Stop all**. A read-only
  listing cannot stop anything. Q11's *"stoppable"* is met by a link to the vendor's own page for
  the session. Ruled by V27.
- Their audit coverage is `observed` (ADR 0075 §5), and their outcome goes into a session record,
  as W8 says.

### 8. A chat's state is derived

**A chat stores no state of its own.** What the window draws is read from its runs:

| The chat | When |
|---|---|
| **open** | it has a tab. The sidebar draws its **current run's** state, with the reason or hold. A chat put away by a quit is still open: it is in the reopen record, and its run ended `stopped \| quit` |
| **wrapping up** | open, with a Smart close sent and its record not yet saved (ADR 0064). A flag on an open chat, not a run state |
| **closed with a record** | its tab closed after `purlis session record` saved its session record |
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
| another chat reported back to it (purlis#259) | the hook channel |
| a commit it made was refused (ADR 0074). Settled by V26a: *"A refused-commit needs-you item clears on the chat's next prompt."* | the hook channel |
| a command in it waits for a secret's approval. Settled by V15: *"the ask tops needs-you"* | the host (V15, V16) |
| its current run ended `completed \| exited` or `failed`, until the chat is shown. Settled by W10: *"LW-5's queue and IB-10 show finished and failed chats"* | the exit, the host |

A run that ended `stopped` is not an item: the operator, a policy or the host ended it on purpose,
and the kill switch's own control says so. W1's list, *"done / failed / blocked / asking /
over-budget, not only permission asks"*, matches these items. A run that ended `completed |
superseded` is not one either, because its chat carries on. A run that is `ready` is not an item: a chat that has just started or been put back has asked for
nothing. **Ignore** (purlis#248) clears a chat's items until the next one arrives, and
changes no run. The order, priorities and channels of the view are TS4's and IB-10's.

### 10. What ends each concept

The five concepts end in different ways. None of them ends a run except through a cause above.

| Concept | Its end | What it does to runs |
|---|---|---|
| **Chat** | closed (§8). Final | its live run ends `stopped \| closed` |
| **Workspace** | removed (`purlis workspace remove`, the safe-remove skill). Final in purlis, recoverable from git while committed | **refused while any chat in it has a live run**, as a rename is refused today. A removal must not leave a program running in a deleted directory |
| **Persona** | removed. Final in purlis, recoverable from git while committed | **refused while a live run has adopted it**. A run's persona is fixed (ADR 0066), and its files must outlast the run |
| **Project** | closed on this machine, which lets go of it and keeps it on disk. Opened again later | every live run ends `stopped \| quit`, and the reopen record is kept |
| **Memory** | archived, with Undo (ADR 0065) | none |

The run's own end states are §1's. A device does not end in purlis. A deleted machine store
makes it a new device (ADR 0066).

## ADR 0066, amended

ADR 0066 says when a run begins. It does not say when a run ends. It now also says:

- **A run ends exactly once, with an end state and a cause** from this record. A run whose chat
  carries on in a new run ends `completed | superseded` at the moment the next one begins, so a
  chat has at most one live top-level run.
- **A child run ends with its parent** when the parent ends first (§6).
- **Every chat has a kind**, `governed` or `observed` (§7). ADR 0066 already gave an observed chat
  an id when purlis first lists it.

## ADR 0067, amended

ADR 0067's denial classes are the stores a chat may not touch (ADR 0069 §6). **They gain the run
journal, `<config>/runs/`.** A chat that could write it could choose how its own runs are
accounted for after a crash. The host enforces it through the same deny list it compiles for
the other classes.

## ADR 0068, amended

- **§3, what a crash of the host costs.** ADR 0068 marks every run that was `working` as
  `failed(host-crash)`. That stands, and `paused` runs join it. **A run that was `input-required`
  at a turn boundary (`ready` or `turn-ended`) becomes `hibernated` instead**, where its harness
  resumes natively, because nothing was lost: its conversation is whole and waiting for a prompt.
  Where the harness does not resume natively, it is `failed | host-crash`. A run that was
  `asked` is `failed`, because the ask was lost. Ruled by V27.
- **The run journal holds each live run's state**, not only its conversation id: the chat and run
  ids, the state with its reason or hold, the state a paused run was paused from, the
  conversation, the harness and level. A hibernated run stays in it across a quit (§3). The host
  replaces it whole at every move, as `reopen.json` is replaced. **These contents pre-empt
  FD-29**, which builds the journal and may add to them.
- **It lives at `<config>/runs/journal.json`, not under `<config>/charterd/`.** That directory
  is tiered transient and is not backed up (ADR 0069 §6, row 66), while the journal must survive
  a crash and is backed up (row 68). A path of its own keeps both rows true, so ADR 0069 does not
  change. The chat sandbox denies it (*ADR 0067, amended*).
- **§2, the grace period.** When it passes, runs end `stopped | grace`, or become `hibernated |
  grace` when they are idle on a harness that resumes natively, as at a quit (§3).
- **§7, the drain fallback.** A run ended at its turn end ends `stopped | drained`, and the chat
  comes back with a `reopen` run.
- **§7, a handed-over program's lost exit code.** Such a program's end is learned from the end of
  its output, with no code. **`run.ended` always records `exit_code: lost`**, so neither the event
  log nor the audit states a guess as a fact. The run ends `completed | exited` only when a
  `SessionEnd` for good came first, and `failed | exited` otherwise. That includes every level-1
  run, which has no `SessionEnd`, so a lost code never reads as success. Ruled by V27.

## ADR 0071, amended

The kill switch stops every program by signalling its process group, and today a chat stopped
that way reads `Failed`, because a signal leaves no exit code. **A run the switch ends now reads
`stopped | killed`.** For that, the host records its stop intent for each run before it signals
(`Planes::stop_every_agent`, after `halt.rs` has thrown the switch), and the exit that follows is
read against that intent (§2). A start refused while the switch is thrown ends `stopped |
refused`, naming the switch as the gate. The rest of ADR 0071 stands: the command line stops,
only the window re-arms, and re-arming restarts nothing.

## ADR 0075, amended

- **`run.ended`'s `meta` gains `state`** (`completed`, `failed` or `stopped`), the end cause,
  and `exit_code`: a number, `none` for a signal, or `lost` after an upgrade handoff (*ADR 0068,
  amended*). The ADR 0066 `cause` stays on `run.started`.
- **Two actions join the registry: `run.paused` and `run.unpaused`.** They pre-empt AU-2's
  registry and AU-4's list of sources, which take them over. Their `meta` is the cause
  (`operator` or `policy`) and the run's state at the time. The actor is `human` for the operator
  and `host` for a policy. A budget pause stays ADR 0075's `budget.paused`.
- Every other move (`working` ⇄ `input-required`, `hibernated`) is an event and not an audit
  entry. It says what a harness was doing, not who did what, so it is telemetry's (O1).

## The hooks-only rule, amended

`AGENTS.md` (and `CLAUDE.md`, a link to it) says: *"Nothing parses harness output to decide
anything. A session's state comes from hooks only, or, for a chat at level 3, from its structured
protocol (ADR 0073)."* This record moves a run by facts that are not hooks: the program's exit,
the end of its output, the host's own acts (admission, hibernation, a budget or policy pause, a
stop it caused, crash accounting), the operator's acts on a human client scope, and, for a remote
chat, what the vendor reports. None of them is harness output. The rule now reads:

> **Nothing parses harness output to decide anything.** A run's state moves only by a named cause
> (ADR 0076): its hooks, or, for a chat at level 3, its structured protocol (ADR 0073); its
> program's exit; an act of the host, the operator or a policy; or, for a remote chat, what its
> vendor reports.

This PR changes the rule in `AGENTS.md`.

## What changes where

The code does not change with this record.

| Where | What changes |
|---|---|
| `crates/purlis-core/src/state.rs` | `State` becomes the nine run states, with `input-required`'s reason and the holds. A `Move { from, to, cause }` is the one way a run changes, and a table of allowed `(from, to, cause)` refuses the rest. **One test per row of §2's table and §6's** (FD-23's acceptance). `needs_you` becomes a function over the run and the chat's items. `exited` reads the host's stop intent first. `SessionEnd` no longer sets `Done` |
| `Board`, the app's `hooks.rs` | Track each chat's current run and its children. `chat-moved` carries the run state and its reason or hold |
| `app/src/chatState.ts`, `NeedsYou.tsx` | Draw the nine states, and read needs you from the items. The words shown are the design system's (ADR 0072 §3) |
| `docs/spec.md` | Its *Session state* line is replaced by this record's states, when `state.rs` changes |
| `purlisd` (FD-5, FD-29) | Writes the run journal, and does the crash accounting of *ADR 0068, amended* |
| `halt.rs`, `Planes::stop_every_agent` | The host records the stop intent before signalling, so each run ends `stopped \| killed` (*ADR 0071, amended*) |
| OV-7, SC-4, SC-20, AC-7 | Use `paused`, `hibernated` and `queued` with their holds, and add no state of their own |
| FD-19 | The `observed` kind |
| The workspace and persona removal paths | Refuse while a live run is in the workspace or has adopted the persona (§10) |
| `docs/plane-format.md` | `<config>/runs/journal.json`, **decided, not yet written** (in this PR) |
| `AGENTS.md` | The hooks-only rule, amended above (in this PR) |
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

## Ruled (V27, 2026-10-01)

The operator accepted all five questions as recommended:

1. **V27a: after a host crash, a run that was idle at a turn boundary on a harness that resumes
   natively becomes `hibernated`**, not `failed`.
2. **V27b: when an upgrade hands a program over and its exit code is lost, the run is `completed`
   only if a `SessionEnd` came first, and `failed` otherwise.** `exit_code: lost` is recorded
   either way.
3. **V27c: a pause is `SIGSTOP` of the run's process group at every level**, with the guard as a
   backstop. Processes that `setsid` out of the group keep running until the sandbox can hold
   them.
4. **V27d: remote chats cannot be stopped from purlis.** It links to the vendor's page, and
   **Stop all** says so. **Removing a workspace or a persona is refused** while a live run is in
   it or has adopted it.

Clarified 2026-10-01 in #777's final commit: four §2 rows tightened to match §3 and V27; no ruling
changed. Three follow from §3 and V27a (a hibernated run stays so at a quit or a lapsed grace
period, an explicit `hibernated | grace` row, and a `starting` run failing on a host crash). The
fourth, **a paused run's clean exit completing it**, was decided in implementation: a paused
program that exits 0 ended cleanly, and nothing else in the table says otherwise.
