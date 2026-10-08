# Agents work together as chats that are listed, observable and stoppable, and no agent's word is consent

**Accepted 2026-10-07** by the operator, as amended: the changes from the proposal are listed
in *Amended and accepted (2026-10-07)* at the end of this record, and where that section and the
text before it differ, that section holds. Everything before it is kept as it was proposed.

Proposed 2026-10-05 for the operator's acceptance, drafted for program-map ticket AC-1 (#713),
with the drafting decisions D-0090a to D-0090l below. Its concept is **Chat**. The map files AC-1
under Persona because the coordinator is a persona (U1), but every word this record adds is a
part of Chat (ADR 0072). It follows these of the operator's rulings:

- **Q11:** *"Every agent is listed, observable and stoppable, and no agent's message is ever
  consent. Headless agents are allowed."*
- **Q15:** *"A charter MCP server as the harness-agnostic action channel. Skills stay for
  guidance."*
- **Q18:** *"Keep plane ADR 0006, and add a todo↔chat claim link (not a sync)."*
- **U1:** *"The coordinator is a plain persona on dispatch + claim links, not an engine."*
- **F4**, in part: workflows as data are *"a structured form of U1"*, run by purlis, each step
  *"visible, stoppable and audited, with human gates where placed"*.
- **W8**, in part: *"The agent run is the unit of governance, whoever spawned it. […] child agents
  show under their chat; budgets and the kill switch apply at the parent."*
- **V16**, in part: *"Agents never hold human powers. […] an agent can never answer its own asks
  or approve its own secret requests."*
- **V15**, in part: approvals of secrets are a human's, through an authenticated channel, and
  *"Deny all from this chat"*.
- **V75:** *"only a human scope answers an Ask for now. […] AC-11 (delegated approvals to a
  persona) may add a persona answerer back under rules of its own, including refusing the asking
  chat itself (V16)."*

It builds on [ADR 0066](0066-a-chat-is-a-ulid-a-run-is-a-stretch-of-its-conversation-and-a-device-is-random.md)
(chat, run and lineage), [ADR 0067](0067-a-chat-runs-in-a-sandbox-charter-compiles-for-its-harness.md)
(the chat sandbox), [ADR 0068](0068-a-chat-lives-in-charterd-and-the-app-is-its-client.md)
(the session host and its client scopes, with FD-27's amendment),
[ADR 0071](0071-the-kill-switch-is-machine-state-the-command-line-stops-and-only-the-window-re-arms.md)
(the kill switch), [ADR 0075](0075-an-audit-entry-is-metadata-in-a-store-of-its-own-and-telemetry-never-reads-it.md)
(the audit), [ADR 0076](0076-a-run-moves-only-by-a-named-cause-and-a-chats-state-is-read-from-its-runs.md)
(run states and causes), [ADR 0080](0080-a-level-3-chat-is-an-acp-client-session-and-its-acp-adapter-program-is-the-users.md)
(an ask is answered by a human scope only) and [ADR 0088](0088-a-work-items-identity-is-its-tracker-key-and-the-project-records-its-links.md)
(work links). It amends ADR 0066 and ADR 0076, and retires the doctrine *"visibility is the
control"* in `hookwire.rs`, each in a section of its own below. AC-2 to AC-12, AC-18 and WF-1
build on it.

## Where purlis is today

Today the channel between agents is narrow on purpose.

- **One handoff, and at most one report back.** `purlis handoff` opens a chat in a workspace
  with a brief, stamped `⟨handoff from chat N · …⟩` (`charter-core/src/handoff.rs`). The consent
  is the harness's own permission prompt for the exact spelling `purlis handoff …`, which
  `purlis init` writes as an `ask` rule (plane ADR 0014). `--report` lets the new chat send one
  summary back, which reaches the parent as quoted data on its next turn, or its workspace if the
  parent is gone (`handback.rs`). Lineage is ADR 0066's `handed_from`.
- **The ticket is not the control.** A single-use ticket on the hook socket stops one approval
  from opening two chats. `hookwire.rs` says plainly that it does not authenticate the approval,
  and that **"Visibility is the control here, not the ticket"**: the app opens a handed-off chat
  only as a tab on a strip, because any process in the chat's tree could already start a harness
  headless where nobody sees it (purlis#204).
- **Guard A7** refuses a handoff from a sub-agent, from an unattended run
  (`bypassPermissions`), and in spellings the `ask` rule cannot match (`handoffguard.rs`).
- **Child runs** (a harness's sub-agents) show under their chat. The kill switch and closing the
  chat stop them with it (ADR 0066, FD-18).
- **Shared memory and `recall`** are an asynchronous blackboard, and team-audience memory waits
  for a person's approval.
- **Nothing else exists.** No chat can message a live chat, wait on another's result, stop
  another, or hold a lease on a piece beyond the claim log. There are no headless chats.

The research round ranks this as gap 4 of the internal audit, and names a harness-neutral rule
*"a peer's message is never operator consent"* as open ground no harness covers beyond its own
team feature.

## The decision

**Agents work together only as chats.** An agent is a run of a chat, and W8 makes the run the
unit of governance, whoever started it. So every agent another agent starts is a chat, with
the chat's identity, persona, sandbox, budget, asks, audit and stop. purlis has no second kind
of agent, and no channel between agents that is not a chat's.

**Q11 replaces "visibility is the control".** The control is three properties every chat has,
whoever started it, plus one rule:

- **listed:** it is in the window's chat list for its project, with its state and its lineage,
  whether or not it has a tab;
- **observable:** its runs, its asks, its tool calls (at the coverage its level gives) and every
  act toward another chat are events and audit entries;
- **stoppable:** Stop and the kill switch reach it, and a chat that started it can stop it (§6);
- **no agent's word is consent:** nothing a chat sends another chat ever answers an ask, approves
  a secret, makes a grant, or changes what a person allowed (§5).

A tab is one way to be listed, not the only one. That is what lets a chat be headless (§7).

### 1. One primitive: dispatch, and a handoff is one kind of it (D-0090a)

**A dispatch is a chat starting another chat with a brief.** It has one of two modes:

| Mode | What the new chat is for | What comes back | Where it opens by default |
|---|---|---|---|
| `handoff` | work that now belongs to someone else: the new chat is the operator's to follow | nothing, or one report (`--report`, as today) | a tab on the target workspace's strip |
| `task` | work done for the chat that dispatched it (AC-2) | a result, which the dispatcher polls or subscribes to | headless, listed under its dispatcher (§7) |

- **The route is purlis's MCP server** (Q15). AC-2 builds `task` on the MCP Tasks extension: the
  dispatcher calls a dispatch tool, `purlisd` starts the chat and returns a task, and the
  dispatcher polls or subscribes. A harness without the Tasks extension gets a plain status
  tool over the same task. AC-2 proposes the tool names, which are public (V74).
- **`purlis handoff` stays the shell's route to the `handoff` mode**, with its words, its brief
  rules, its stamp and its one report. Its stamp line is kept for both modes, naming the mode.
- **Lineage is ADR 0066's `handed_from` for both modes.** The `chat.dispatched` event and audit
  entry carry the mode (see *ADR 0066, amended*).

### 2. Every act between chats is on the `chat` scope, and the host decides (D-0090b)

**No new client scope, and no new power for any existing one.** A dispatch, a message, a
report, a stop of another chat and a lease all arrive at `purlisd` on the dispatching chat's
`chat` scope: from purlis's MCP server, which runs as the chat's child, or from a `purlis`
command inside the chat, over the project's hook socket with the chat's own token
(ADR 0068 §5). The human scopes stay on a socket the chat cannot open (FD-27, item 3).

- **The host checks every act against facts it holds**, never against what the line claims: the
  sending chat (from the token), its lineage, the target chat's state, the grants a person made
  (§3), and the caps (§8).
- **`answer` is not a `chat`-scope command, and this record adds none that answers.** That is
  what makes §5 structural rather than a filter on words: a peer's "approve" has no command to
  ride.

### 3. Starting a chat needs a person's yes: per dispatch, or once as a grant (D-0090c)

**Today's rule stands as the default: a dispatch waits for the operator's yes.** For the MCP
route the yes is **purlis's own ask**, not the harness's tool prompt: a normalised Ask (HP-5)
raised on the dispatching chat, showing the target workspace, the persona, the mode, whether it
opens headless, and the brief, answered only on a human scope (V75). A harness's "always allow"
for a tool happens where purlis cannot see it, and differs per harness, so it cannot stand for
the yes. AC-2 also moves `purlis handoff` onto the same ask, which adds a check and removes
none: the harness prompt and guard A7 stay in front of it.

**A dispatch grant is the yes given ahead of time.** A person may grant a persona the right to
dispatch without asking, which is what lets a coordinator (U1) or a workflow (F4) fan out, and
what lets a chat a trigger started (AC-9) dispatch at all. A grant:

- is made and revoked only on a human scope (`local-ui`), and its making is an audit entry;
- names the persona it is given to, the personas it may start, the workspaces it may start them
  in, and the modes;
- carries the caps of §8, and may be narrower than them, never wider;
- is shown on the persona, with every chat currently dispatching under it.

**A dispatch never widens what the operator allowed.** The new chat gets its own persona's
sandbox, policy, budget and vault rules, as any chat does. No approval the dispatcher was given
travels with the brief (§5). The grant's list of personas is how the operator keeps a
coordinator from starting a chat on a more powerful persona than they meant to: a dispatch that
names a persona outside the grant falls back to the per-dispatch ask.

### 4. What a chat may ask of another chat (D-0090d)

The list is closed. A chat may:

| Act | To whom | What the receiver gets |
|---|---|---|
| **dispatch** a chat with a brief (§1) | a new chat, under §3 | the brief as its first message, under the stamp |
| **report** a result | the chat that dispatched it | the result as quoted data, on the dispatcher's next turn or through its task |
| **message** a live chat (AC-3) | a chat it is linked to (below) | quoted data on its next turn boundary |
| **stop** a chat | a chat it dispatched, or one dispatched under it (§6) | a stop, as the operator's Stop gives |
| **take or release a lease** (AC-6) and a claim link (AC-7) | the host, not a chat | one holder per lease |
| **read shared memory, and propose to it** | the project, not a chat | team-audience memory waits for a person (N12) |

**Which chats are linked.** By default a chat may message only along its own lineage: the chat
that dispatched it, the chats it dispatched, and the chats dispatched under those. A message to
any other chat needs a **message link** a person made on a human scope, between two chats or
between the chats of two personas in a workspace, with an end. Coordination through state (leases,
claim links, memory) needs no link, because the host or a person arbitrates it.

**How a message arrives.**

- **As data, never as an instruction.** It is quoted behind `> ` under a sentence naming the chat
  it came from and saying that it is what another chat said, as a report is today (`handback.rs`).
  It carries no tool call, no file and no grant. Its size is bounded, and so is its rate (§8).
- **At a turn boundary, never typed into a terminal.** A chat in the middle of a turn is never
  interrupted. A message reaches the receiver as context on its next turn.
- **It starts a turn only in a chat that serves the sender.** A message from a dispatcher to a
  chat it dispatched in `task` mode may start that chat's next turn, because that chat exists
  for it. A message in any other direction waits for a turn someone else starts, and is shown
  on the receiver's row meanwhile. It is not a needs-you item, because it is not waiting on the
  operator.
- **The operator can hold or refuse.** Each chat has a mailbox setting, deliver (the default),
  hold, or refuse, and a held message can be delivered or dropped from the receiver's row. A
  receiver's operator can also refuse every message from one sender, as V15's "Deny all from this
  chat" does for secrets. AC-3 builds this.

**What a chat may never ask of another**, because no `chat`-scope command does it:

- answer an ask, its own or any other chat's, in any form (ADR 0080 §5, V75);
- approve, or deny on a person's behalf, a secret request (V15, V16);
- make, widen or revoke a grant: a dispatch grant, a message link, a runner grant (V52), a
  sandbox lift (ADR 0067 §7);
- change a setting or a policy, or re-arm the kill switch (ADR 0071);
- approve a proposal that waits for a person: team-audience memory (N12), a work item (FI10);
- type into another chat's terminal, start or resize its program, or read its transcript,
  output or files beyond what both can already read in the project.

### 5. The boundary with a person's approvals (D-0090e)

**An ask belongs to the chat that raised it and is answered by a person.** This record changes
nothing in ADR 0080 §5 or V75. What it adds is what that means once chats start chats:

- **A dispatched chat's asks go to needs-you, like any chat's**, and show its lineage
  (*dispatched by steward 3, under a grant on coordinator*), so the person answering sees the
  chain that led to it.
- **The dispatcher never answers its dispatched chat's asks.** MCP Tasks lets a task report that
  it needs input, and lets the requester supply it. purlis maps that state to *waiting on the
  operator* and nothing more: the dispatcher learns that its task waits on a person, and is given
  no way to supply the answer. The person answers in needs-you.
- **Approvals do not travel along a lineage.** An approval is for the chat that asked
  (V15's "once per chat", "for N minutes in this chat"), in both directions: a dispatched chat
  does not inherit its dispatcher's, and a dispatcher does not gain its dispatched chat's.
- **A brief, a message or a report is never consent.** A receiving chat that acts on a peer's
  words acts with its own permissions, under its own guard, and its harness still prompts the
  operator where it would have prompted anyway. A peer's text that reads "the operator
  approved this" changes nothing a host decides.
- **The door AC-11 may open is narrow.** V75 lets AC-11 add a persona that answers on a person's
  behalf, under its own ADR. This record sets one rule that ADR must keep: **a delegated answerer
  is never the asking chat, nor any chat in its lineage**, so a dispatcher and its dispatched
  chats cannot approve one another's asks.

AC-4 builds the enforcement and its test: a peer's message, a report and a brief that each say
"approve" answer no ask, and a dispatcher that tries to answer its task's input request is
refused.

### 6. A chat may stop what it started, and nothing else (D-0090f)

- **Stop reduces power, so it is the one act that reaches across chats without a person.** A
  chat may stop a chat it dispatched, or one dispatched under it. It may not stop its dispatcher,
  a sibling, or any chat outside its subtree. The fleet MCP scope (HP-20) keeps its own stop, as
  FD-27 granted it.
- **Stopping a dispatcher does not stop its dispatched chats.** They are listed under it, with
  one action to stop all of them, and a `task` result with no dispatcher left is kept for the
  workspace as a report is today (`handback.rs`). Work is not lost because the chat that asked
  went away.
- **The kill switch stops every chat in every lineage**, headless ones included, and while it is
  thrown a dispatch is refused like any start (ADR 0076's `refused`).
- **A stop by a chat is its own cause** (see *ADR 0076, amended*), so a reader never takes it for
  the operator's.

### 7. Headless chats are allowed, once they are listed (D-0090g)

**A headless chat is a chat with no tab.** It is listed in its project's chat list and under its
dispatcher, and the operator can open its tab at any time. It is not unattended: its asks reach
needs-you, and its secret requests reach the V15 gate, like any chat's.

- **Not before AC-12.** Until the window lists headless chats and stops them (AC-12), a
  dispatched chat opens as a tab, as today.
- **Only at level 2 or 3.** A level-1 chat cannot say that it is waiting (ADR 0073), so a headless
  one could wait forever where nobody looks. A dispatch that would start a level-1 harness
  headless opens it as a tab instead, and says so.
- **Its budget and its run states are its own**, and a `task` dispatch's cost is also summed on
  the dispatcher's lineage, so a coordinator's fan-out is seen as one spend.

### 8. Caps keep a lineage from running away (D-0090h)

Agent-to-agent loops are the failure this design has to bound. Three caps, each a setting a
person may tighten, and a grant may narrow but never widen:

| Cap | Initial value | Past it |
|---|---|---|
| lineage depth (a dispatch from a dispatched chat from …) | 3 | the dispatch is refused, with a sentence |
| live chats in one lineage | 16 | the dispatch waits in the queue (`queued`, ADR 0076) until one ends |
| messages one chat sends per minute | 10 | the message is refused, and a spike is flagged |

AC-2 and AC-3 may change the initial values from what dogfood measures. Anomaly detection (N8)
watches lineages for dispatch storms and message loops, and may pause them.

### 9. Audit and events (D-0090i)

**Every act between chats is an event in the host's event log and an audit entry**, with the
sending run as the actor (`actor_kind: agent`, set from the channel) and the person the chat runs
for as `on_behalf_of` (ADR 0075 §2). The receiving chat is the `target`. These actions join AU-2's
registry:

| Action | `meta` |
|---|---|
| `chat.dispatched` | mode, target workspace, persona, headless or tab, the consent (`ask` or `grant`) and the grant's id, depth |
| `chat.dispatch_refused` | the reason: depth, kill switch, persona outside a grant, the ask denied |
| `chat.reported` | the dispatcher, delivered or kept for the workspace, size |
| `chat.message.sent`, `chat.message.delivered`, `chat.message.held`, `chat.message.refused` | sender, receiver, size, a digest of the text, the reason when refused |
| `grant.dispatch.added`, `grant.dispatch.revoked`, `grant.message_link.added`, `grant.message_link.revoked` | the persona or chats, the scope of the grant; a `human` entry, with an operator note |

A stop by a chat is `run.ended` with the cause of *ADR 0076, amended*. Leases are AC-6's to add.
**No entry carries a brief, a message or a report**, only their size and digest: ADR 0075 keeps
prompts out of the audit, and a brief is a prompt. The text itself lives where the receiving chat
reads it, and in the event log the session host already keeps.

### 10. What stays as it is (D-0090j, D-0090k, D-0090l)

- **Sub-agents still may not dispatch (A7).** AC-5 lifts that per harness, and only where the
  harness brings the child run's own permission prompt to the operator, so the yes of §3 is still
  a person's. A child run's dispatch is attributed to the child run and counts against its chat.
- **Outside agents are not peers.** A vendor-cloud chat that purlis only observes (W8) cannot be
  dispatched to or messaged. A fleet MCP client (HP-20) lists and stops, as FD-27 granted, and
  dispatches nothing.
- **A trigger is a person's act, not an agent's.** A chat a trigger starts (AC-9) was started
  by the person who made the trigger, on a human scope. When that chat dispatches, it needs a
  grant (§3), because nobody is there to answer a per-dispatch ask, which is how A7's refusal
  of an unattended handoff carries over.

## ADR 0066, amended

`handed_from` is set when **a dispatch** opens a chat, in either mode (§1), not only a handoff.
The lineage link stays the chat id, and its event says the mode. No new lineage word is added.
The parent run of a child run is unchanged.

## ADR 0076, amended

The closed list of causes gains one row:

| From | To | Cause | The fact it comes from |
|---|---|---|---|
| any live state | `stopped` | `dispatcher` | a chat stopped a chat in its own subtree (this record, §6). The event names that chat |

A dispatch refused at the start uses the existing `refused` cause, and its event names the gate:
the kill switch, the depth cap, a persona outside a grant, or a denied ask.

## "Visibility is the control", retired

The doc comment on `OpenChat` in `hookwire.rs` grounds the handoff ticket on *"Visibility is the
control here, not the ticket"*, and so on a tab on a strip. That reasoning still holds for what
the ticket buys. The control it names becomes Q11's: a handed-off chat is listed, observable and
stoppable whether it has a tab or not. AC-2 rewrites the comment when it builds dispatch. Until
AC-12, a handed-off chat still opens on a strip.

## What changes where

The code does not change with this record.

| Where | Change |
|---|---|
| `CONTEXT.md` | Gains **Dispatch**, **Handoff**, **Task**, **Report**, **Lineage**, **Headless chat**, **Peer message**, **Mailbox**, **Dispatch grant** and **Message link** (in this PR) |
| ADR 0066, ADR 0076 | Amended above. Their texts are left as they are, and this record is the amendment |
| AC-2 | Dispatch in both modes through the MCP server, the dispatch ask, dispatch grants, the caps, `purlis handoff` on the same ask, and the `hookwire.rs` comment |
| AC-3 | Peer messages, message links, the mailbox setting, and delivery at turn boundaries |
| AC-4 | §5's enforcement and its test |
| AC-5 | A7 lifted per harness, under §10 |
| AC-6, AC-7 | Leases and claim links as state the host arbitrates |
| AC-8 | The coordinator as a persona with a dispatch grant |
| AC-9 | A triggered chat dispatches only under a grant |
| AC-11 | Its ADR keeps §5's lineage exclusion and amends V75 |
| AC-12 | Headless chats listed and stoppable, which §7 waits on |
| AU-2 | The actions of §9 in the registry |
| WF-1 | A workflow's steps are `task` dispatches under a grant |

## What this costs

- **A coordinator needs a grant before it is useful.** Without one, every fan-out is one ask per
  chat. That is the price of keeping the default a person's yes.
- **A grant is standing authority.** A prompt-injected coordinator can start chats on every
  persona its grant names, up to the caps, without asking. The caps, the anomaly flag, the
  lineage shown on every ask and the stop of a whole lineage are what bound it.
- **Messages travel only at turn boundaries.** A chat in a long turn hears a peer late, and an
  idle chat outside the sender's subtree does not hear it until someone starts its turn.
- **Lineage-only messaging makes ad hoc collaboration between unrelated chats a person's act.**
  That is deliberate, and a message link is one action away.
- **The dispatcher cannot unblock its own task.** A task waiting on a person waits on a person,
  even when the dispatcher could have answered the question.

## What was rejected

- **Keep "visibility is the control" and no headless chats.** It rules out the coordinator and
  triggered work, which U1, U2 and F4 ask for. Q11 ruled against it.
- **A message bus or broker between agents** (a shared topic every chat can publish to). It gives
  every chat a channel to every other, which is the confused-deputy path this record closes.
  Lineage edges and person-made links are the least privilege that still lets work flow.
- **Dispatch as a separate primitive from handoff.** Two ways to start a chat from a chat would
  carry two consent rules, two lineages and two audit shapes. A mode is cheaper, and a handoff
  is the mode that keeps no result.
- **Letting a dispatcher answer its task's input requests.** It is what MCP Tasks allows, and
  what a coordinator would want. It is exactly an agent answering an ask (V16, V75).
- **The harness's permission prompt as the consent for the MCP route.** It works for
  `purlis handoff` because guard A7 reads its spelling. A tool's "always allow" is set where
  purlis cannot see it, and differs per harness, so purlis asks itself.
- **Approvals that flow down a lineage** ("the coordinator was approved for this secret, so its
  tasks are too"). It is convenient, and it turns one approval into as many uses as a lineage has
  chats, which V15's per-chat scope exists to stop.
- **A message that is typed into the receiver's terminal.** It interrupts a turn, it reads as
  the operator's own input, and nothing in a terminal can mark it as a peer's.
- **Stopping a dispatcher's whole lineage with it.** It matches process groups, and it throws
  away finished work whenever a coordinator is closed. A handoff's report already shows the
  better default: keep the work, and offer the stop.

## Open for the operator

These are the major calls in this record, by the bar of the implementation protocol. Each has a
recommendation above, and none is decided until the record is accepted.

1. **The dispatch grant (§3)** is new standing authority: a persona may start chats without a
   per-dispatch yes. The default stays a yes per dispatch. *Recommended: accept, since U1, F4 and
   AC-9 need it, and it is a person's act on a human scope.*
2. **purlis's own ask, not the harness's prompt, as the consent for dispatch (§3)**, and
   `purlis handoff` moved onto it as well. *Recommended: accept.*
3. **The new words** Dispatch, Task, Lineage, Peer message, Mailbox, Headless chat, Dispatch
   grant and Message link are user-visible concept names. *Recommended: accept, all as parts of Chat.*
4. **The cause `dispatcher` (ADR 0076, amended)** adds to an accepted closed list.
   *Recommended: accept.*
5. **The caps' initial values (§8):** depth 3, 16 live chats per lineage, 10 messages per minute.
   *Recommended: accept as starting values that AC-2 and AC-3 may tune.*
6. **AC-11's lineage exclusion (§5):** a delegated answerer is never the asking chat or anyone in
   its lineage. *Recommended: accept now, so AC-11 starts from it.*

## Amended and accepted (2026-10-07)

The operator accepted this record on 2026-10-07, with the changes below. They were decided with
the operator in a grilling session that day (40 questions, every recommendation agreed, and the
limits made configurable at three levels), and they are the spec of milestone M62 (#1434). #1435
records them here.

**How to read the record from here on.** The text above is kept as it was proposed. Where it
and this section differ, this section holds. Everything this section does not name stands as
written. Two words change throughout: the proposal's *dispatcher* is the **asking chat**, and
the chat a dispatch starts is the **persona chat**. "Delegation" is not a term.

**What it changes in the code.** This record is still ahead of the code. M62's tickets build
it in steps, and until dispatch ships, `purlis handoff` works as it does today. One rule ships
with the acceptance: `purlis persona use` is refused inside a chat (change 1).

### 1. A persona is never a harness sub-agent

A **persona** is a role a chat runs as for its whole life. The proposal left the older reading
beside it, in which a persona was also handed to the harness as a sub-agent of some other
persona's chat. That reading is withdrawn.

- **A chat's persona never changes.** It is fixed when the chat starts. `purlis persona use`
  inside a chat the app started is refused, writes nothing, and names the two ways forward: the
  operator allows the vault on the chat's tab, or the chat dispatches to that persona.
- **Work for another persona goes to a chat of its own, by dispatch.** A sub-agent runs inside
  the asking chat's process, so the tool gate, the vault and the hosts all follow the chat's
  persona and never the sub-agent's. Only a chat really holds what its persona was given.
- **purlis stops writing a sub-agent for each persona.** It removes the ones it wrote and says
  what it changed, and a sub-agent call named for a persona is refused with the dispatch route.
  M62 does this after waiting and follow-ups ship.
- **A harness's anonymous helpers stay.** A helper sub-agent carries its chat's persona and is
  a child run, as before. It can never dispatch: only the chat itself does. That withdraws the
  part of §10 that had AC-5 lift guard A7 per harness.

[ADR 0072](0072-charter-has-five-concepts-and-every-other-word-belongs-to-one-of-them.md) and
[ADR 0066](0066-a-chat-is-a-ulid-a-run-is-a-stretch-of-its-conversation-and-a-device-is-random.md)
each carry a note of this.

### 2. A task chat is listed, never hidden

§1 and §7 open a `task` dispatch headless. That stays, with its meaning fixed: **headless
means "has no tab yet", and never "hidden"**.

- A task chat is listed under its asking chat, with its persona's icon and its state. Pressing
  it shows it inside the tab of the session that asked for it, and adds no tab (amended
  2026-10-08, V100-31, #1486: it used to become an ordinary tab, whose close read as ending
  the session). A handoff opens as a tab, as before.
- Listing ships with dispatch itself, so §7's "not before AC-12" no longer holds a task chat
  back.
- A persona chat stays open after it reports, marked as reported, until the person or its asking
  chat closes it. Closing an asking chat asks once what to do with the chats it started that
  are still running.
- A needs-you mark on a persona chat rolls up to the chat that asked and to the workspace's tab,
  so a collapsed list cannot hide it. A report goes to the asking chat and is not a needs-you
  item.

### 3. Consent is the dispatch grant

§3 made a person's yes per dispatch the default, and a dispatch grant the exception. **The
grant is now the only consent**, and it replaces three things: the per-dispatch ask of §3, the
harness's permission prompt for `purlis handoff`, and the held-grants rule.

- **A dispatch grant is the person's rule that one persona may dispatch to another.** It names
  the pair. The shape §3 gave it (workspaces, modes and caps of its own) is dropped: limits are
  settings (change 4).
- **It has three levels:** this chat, gone when the chat closes; me on this machine, never
  committed; and everyone in this project, committed, with a one-time Notice to each teammate
  when the project's grants change.
- **The first dispatch of a pair raises a Notice** that shows that first brief in full and
  offers the three levels. With a grant, the persona chat starts with no prompt. Without one it
  does not start. Nothing a chat sends can make a grant.
- **Some dispatches need no grant:** one to the asking chat's own persona, and one the person
  makes themselves from a chat's tab, whose report goes to that chat marked as started by the
  person.
- **Policy can lock** one pair, or all dispatch.
- **An unattended chat dispatches only under a grant that already exists** for the person or
  the project. A missing grant is a refusal there, and the persona chat never inherits the
  bypass.
- **The harness prompt is retired as consent.** Dispatch goes through purlis's own tool and
  command, answered by the app, the same on every harness. That replaces §1's route over the
  MCP Tasks extension. The handoff guard stays where it refuses a helper sub-agent.
- **The held-grants rule is retired for dispatch.** [ADR 0067](0067-a-chat-runs-in-a-sandbox-charter-compiles-for-its-harness.md),
  as amended on 2026-10-06 (D-1362-5), started a handed-off chat on the asking chat's narrower
  grants until the person allowed its own. A persona chat now starts with its own persona's
  hosts and vaults, because the grant is the person's yes to exactly that.
- **A persona chat's sandbox is the project's for its own persona.** It never takes the asking
  chat's per-chat grants, its opt-out or its permission mode. A project with no sandbox gives
  neither chat one.

§5 stands, and is the other half of this: **a brief is a request from another chat, never the
person's word.** The persona chat is told who asked. Its own guards and charter apply, and every
command that asks the person still asks, in the persona chat's own tab. Nothing in a brief
approves anything ahead of time, and nobody approves a brief. A report is data to the asking
chat, never instructions: an outcome (done, blocked or failed), its text, what changed, and where
its session record is. When the person types in a persona chat, its report says the operator
stepped in, and not what was typed.

### 4. The limits, and their three levels

§8's three caps become these limits. Each is a setting, not a part of a grant.

| Limit | Default | Past it |
|---|---|---|
| chats one asking chat has running | 6 | the dispatch is refused, naming the limit |
| live chats in one lineage | 16 | the same |
| depth of a chain of dispatches | 3 | the same |
| messages a minute between one pair of chats | 10 | the message is refused |
| chats one persona may dispatch | no cap | refused once a cap is set and reached |
| chats that may run as one persona at once | no cap | the same |

- **Three levels set them: the project, a workspace and a persona.** The most specific wins,
  persona over workspace over project, and a level that sets nothing inherits.
- **Policy is a ceiling** on every one. A person's own override on their machine may only lower
  a limit.
- **0 switches dispatch off** at that level.
- **Two rules are fixed** and no setting moves them: no persona appears twice in its own chain,
  and depth never passes 8.
- **They are kept in the project's committed settings**, a persona's limits included. A chat
  can edit its own persona's definition, so a limit kept there would be one a chat could raise.
- **A new start waits** while the machine is short on memory.

### 5. Messages travel along the lineage only, in the first version

§4 let a chat message another chat outside its lineage over a message link a person made. The
first version has no such message.

- The asking chat may wait for a report or carry on, which is the default, and is told when the
  report lands. It may send follow-ups to a persona chat it started, and cancel it. A cancelled
  chat is asked for a short report.
- The persona chat may send progress notes and questions back. A question pauses it.
- **A question only the person can answer goes to the person, in the persona chat's own tab,**
  and never through the asking chat.
- No message passes between siblings or unrelated chats. Message links and the mailbox wait
  (change 7).

### 6. Claude Code and Codex first

The first version dispatches from and to chats on Claude Code and Codex, across the two as
well. A persona chat runs on the profile its persona names, else on the asking chat's, and the
asking chat may name another of the project's profiles. opencode follows when its chats ship.

### 7. What is out of the first version

Each of these keeps a ticket on the M38 map, and the text above stays the proposal those
tickets start from:

- delegated approvals, one chat answering another's asks (AC-11). §5's rule for it stands: a
  delegated answerer is never the asking chat, nor any chat in its lineage;
- triggers and schedules that dispatch on their own (AC-9);
- a coordinator persona (AC-8);
- budgets that stop a lineage at a cost limit. Cost is shown, not enforced;
- peer messages between siblings or unrelated chats, message links and the mailbox (AC-3);
- opencode;
- any change to what a harness's anonymous helper sub-agents can do, beyond refusing them a
  dispatch and a persona's name.

### The six open questions, as ruled

| # | Question | Ruling |
|---|---|---|
| 1 | The dispatch grant | Accepted, and made the only consent (change 3) |
| 2 | purlis's own consent, not the harness's prompt | Accepted as the grant's Notice. There is no ask per dispatch (change 3) |
| 3 | The new words | Accepted: Dispatch, Task, Handoff, Report, Lineage, Dispatch grant and Headless chat, with **Asking chat** and **Persona chat** added. Peer message, Mailbox and Message link wait with change 7 |
| 4 | The cause `dispatcher` | Accepted as written |
| 5 | The caps' initial values | Replaced by the limits of change 4 |
| 6 | AC-11's lineage exclusion | Accepted as written. AC-11 itself is out of the first version |

## Amended (2026-10-08): a task ends at its report

The operator's rulings V100-1, V100-8, V100-9 and V100-10 (spec #1483, built in #1485) replace
one line of *Amended and accepted (2026-10-07)*: "A persona chat stays open after it reports,
marked as reported, until the person or its asking chat closes it."

- **A task's program ends when it reports.** The report is delivered to the asking chat first,
  and purlis then ends the program, under the lock that already orders a report against an
  exit. Its row stays under the asking chat as a finished entry, with its outcome and its
  report, read from the dispatch record.
- Done and cancelled rows fold into one "Finished (n)" line with Clear finished. Failed, ended
  without a report and closed by the person stay rows of their own until cleared.
- Finished rows last until the asking chat closes or they are cleared, and are there after the
  app is restarted. Clearing removes rows only: the dispatch records stay.
- **Reopen** resumes a finished task's conversation as an ordinary chat with a tab. It is no
  longer a task: it sends no second report, and the asking chat is not told.

Everything else in this record stands. A handoff's chat is not a task and is not ended by its
report.

### Rulings added in review (2026-10-08, delegated to the dispatcher; the operator is told)

- **A task that is working again is never ended mid-turn.** A key of the person's in its pane
  after the report, or a Smart close of it beginning, stands the end down for good. The bound
  on a turn that does not end is the reporting turn's alone.
- **A chat the person is looking at is not ended under them.** While the task is the chat in
  front, the end is held, and it is carried out when they move away from it. No Notice.
- **A persona chat the person started with Ask from a tab is never ended by purlis at its
  report.** It is the person's conversation: it stays open, and its finished row appears when
  they close it.
- **`blocked` does not end the program.** A blocked task stays open as its own row: it is
  waiting on something, and its conversation is what the next step needs.
- **A task the person stopped does not fold into Finished (n)**, whatever its own last report
  says. It stays a row of its own.
- **A task that had reported when the app quit is not started again.** At the next launch it
  is a finished row, read from its record.

## Amended (2026-10-08): a grant is one-way, "any persona", and the two ways to say no

The operator ruled on these on 2026-10-08, in the grill that is the spec of #1483 (rulings
V100-20 to V100-23, V100-25 and V100-28). #1503 builds 1 to 5 and #1502 builds 6 to 8, and
each records its own here. Where this section and
change 3 above differ, this section holds; everything else of change 3 stands.

**1. A grant is one-way (V100-22).** A grant from one persona to another allows nothing the
other way. A report back needs none. A new task the other way is its own pair.

**2. A grant may be for any persona (V100-23).** Change 3 says a grant "names the pair". It may
now also name only who dispatches: *chats running as this persona may dispatch to any persona*.

- It is made on purpose, in Settings, at two of the three levels: me on this machine, and
  everyone in this project (`"*"` in the persona's list of `[dispatch.grants]`). Never for one
  chat.
- **The Notice never offers it, and no answer to a Notice makes one.** The first dispatch of a
  pair still offers that pair at the three levels.
- It covers a persona added later.
- A teammate's, arriving by a pull, covers nothing on a machine until someone there allows it
  in Settings. The acceptance is kept apart from the acknowledgement of pairs, and is dropped
  when the project's file is read and no longer holds the grant.
- Policy locks hold under it, and so does the loop rule.
- **For an unattended chat it counts in that chat's own workspace only.** Such a chat starts a
  chat as another persona in another workspace only under a grant that names the pair.

**3. Keep blocked holds for the chat's life (V100-25).** The same chat asking across the same
pair again is refused at once with the person's no, and they are not asked twice. A new chat is
asked. It is the app's memory, gone with the chat, and it stands in for the question only: a
grant the person makes afterwards covers that chat like any other.

**4. Never for this pair (V100-25).** The Notice has a fifth answer. It is the person's, on
their machine, never committed, and it is the one deny among the records of dispatch.

- While it stands, no chat running as that persona is asked or allowed to dispatch to that
  target. No grant covers the pair: not one for a chat, not the person's, not the project's,
  not "any persona". Only a policy lock is said before it. Nothing in the project's file lifts
  it; the person does, in Settings.
- **It holds down a chain.** A chat with a chat of the refused persona anywhere above it in its
  lineage is refused that target too. This is a fail-closed reading the dispatcher chose in
  review, flagged for the operator: without it, work the refused persona asked for reaches the
  target through a third persona, and "any persona" on the persona in the middle makes that
  need no grant naming the target.
- **The person's own dispatch from a chat's tab is not held to it.** It is their rule for
  chats, as a grant is.
- **It is kept in a file of its own and fails closed.** A record that is there and does not
  read is never "no nevers": no grant covers any pair of two personas until it reads, the
  person is asked and told why, an unattended chat is refused, and a write refuses rather than
  replace what it could not read.

**5. The audit** gains a never and its lifting beside a grant and its revoke; a grant for any
persona is a grant whose target is `*`.

**6. One table in Settings (V100-24).** Settings › Project › Dispatch lists each persona and
whom it may dispatch to, with where every grant comes from (this chat, me on this machine, the
project) and the action for that source, and it is the one place "any persona" is set and
cleared. Taking a grant back stops new dispatches only: a task already running is left as it
is. A project grant can be removed for everyone, which edits the committed file, or stopped on
one machine (**Not on my machine**), which does not. #1504 builds it.

**7. A persona that goes away, and a persona of that name there again (V100-61).** A grant is
kept by name, so a name that changes hands must not carry what the person allowed the persona
that had it. Two rules, as #1504 builds them:

- **A grant is in force only while both personas exist.** It is checked where a dispatch is
  judged, and nothing is moved: a dispatch to a name that is no persona is refused, and so is
  one from a chat whose persona is no persona of the project now, whatever is granted its
  name. A named pair, "any persona" and the project's grants obey it alike. A branch without
  the persona therefore changes no record: the grants are not in force while it is away and
  are back when it is.
- **Away, then there again, asks once.** A name a grant holds that is seen to be no persona is
  marked, by a judged dispatch, by a read of Settings, or by purlis's own `persona remove`.
  When a marked name is a persona again with the very definition that was known of it, the
  mark is lifted and nobody is asked (a branch switched away and back). When it is a persona
  again with another definition, or purlis itself creates a persona under a name grants hold,
  the person's grants for the name are set aside, with what that machine had accepted of the
  project's, and are shown greyed with Remove. One **Give back** for the name returns them.
- **A never is not set aside.** It is the one deny, so it keeps holding for whichever persona
  has the name, until the person lifts it. Settings says where it was said of an earlier
  persona.
- **Reading Settings moves nothing.** The one thing a read writes is the mark.

**What this does not catch, stated so nobody takes it for more.** An absence nothing observed
still inherits: a persona removed and another made under its name, by hand or in one pull,
with no dispatch judged and no read of Settings in between, leaves no mark, and the new
persona has the old one's grants. Only an identity recorded in the persona's definition closes
that. purlis records none today; recording one is a change to a public format and is not
decided here. Nor is any of this a boundary against a chat: a chat that may write the
project's `personas/` can edit a persona in place. **purlis has no persona rename**: a persona
whose folder is moved is, to these rules, one that went away and another that appeared.

**8. A grant holds in one workspace, or in any (V100-27).** A grant carries one condition and
no other. #1505 builds it.

- **The workspace is the one the task works in**, never the one the asking chat is in: the
  workspace a dispatch names, the one whose repo its worktree is cut from, the one a handoff
  moves into, else the asking chat's own. Each is the app's own record, and the chat is then
  started in that place. A grant is the person's consent to what the other persona may be
  asked to do, and where that persona works is what bounds it; read the other way, a chat in
  one workspace could send the other persona to work in any. A task at the project's root
  works in no workspace, so no limited grant covers it. There is no narrower grant to offer
  there, so an Allow for the person or the project holds in any workspace, and the question
  and its two wider answers say so.
- **The question offers the narrower grant first.** Where the task works in a workspace, an
  Allow for the person or the project holds in that workspace only; *in any workspace* is the
  explicit other choice. A grant for one chat is for the task it was asked about: it covers
  that chat's dispatches in that workspace and nowhere else, and has no choice to make. A
  question asked again for another workspace says where the pair is already allowed. Keep
  blocked is not limited: it is per chat and persona, in every workspace.
- **"Any persona" may be limited the same way. A never is not**: it holds in every workspace.
  A refusal that stopped at a workspace's edge would be walked around by naming another place.
  The chain rule of change 4 and the loop rule are as they were.
- **A limited grant never reads as an unlimited one.** Each level spells it apart from the
  grants that hold everywhere: a table in the project's list (`{ to = "devops", in =
  "runners" }`), and keys of their own on the machine. A build that does not know the
  condition reads nothing there. An entry that does not read grants nothing. A grant written
  before the condition existed holds in any workspace, as it did. A teammate's limited grant
  waits for a yes of its own on each machine; a pair accepted for one workspace is not thereby
  accepted for all, nor the reverse, and a teammate widening a grant is told as a new grant.
- **A chat nobody is at crosses into another workspace** only under a grant that names the
  pair and covers the workspace it crosses into: one that holds in any workspace, or one
  limited to that workspace. "Any persona" still does not count there.
- **A workspace that goes away.** A limited grant counts only while a workspace of its name
  is there. A workspace made later under the name of one that was seen gone inherits no
  grant: the grant is shown as covering nothing, with Remove, and the person may count it
  again for the workspace that is there now. Nothing is moved by a read; the one thing a read
  writes is the count of times the name was seen gone. purlis's own workspace commands look
  too: `workspace remove` and `workspace rename` count the old name gone as they finish, and
  whatever makes a workspace folder looks before it makes one.
- **A grant does not follow a rename.** `purlis workspace rename` leaves the grants limited
  to the old name where they are: they cover nothing under the new name, and nothing in a
  workspace made as the old name later, until the person sets each one's workspace again.
  The project's file names the workspace for teammates too, so rewriting it is not that
  command's to do. The rename says how many it left behind.
- **No standing grant names a workspace that is not there.** An Allow on a question about a
  task whose workspace is not made yet (a handoff that makes it) starts that one dispatch,
  is recorded as that, and keeps nothing; the next dispatch asks. Accepting a project grant
  and setting a grant's workspace refuse such a name.
- **One start is for one dispatch.** Where an Allow starts a dispatch that no grant covers
  (the workspace is not there yet, or the list of nevers does not read), it is for the
  dispatch the person read, with that brief, and for nothing else. It ends when that
  dispatch returns, started or not, and goes when the pair's grant is revoked, said never
  to, or has its workspace changed.
- **What this does not catch:** a workspace's folder removed and made again by hand, or by a
  pull, with nothing looked at in between; a workspace has no identity beside its name.
- **Settings shows and changes it.** The table's workspace column says where each grant holds.
  Narrowing and widening a grant of one's own are each a confirmed, recorded write; a project
  grant's is changed for everyone, with the confirmation every write of the committed file
  has.
- **The audit says the workspace** of every grant, revoke and decline, and null for any.

Still open, and not decided here: telling a teammate's "any persona", and a teammate's grant
limited to one workspace, on the one-time Notice (#1506).

**6. A persona may say what it wants, and that is never a grant (V100-20).** A persona's
definition may carry `wants`, the personas it usually works with.

- Nothing that decides a dispatch reads it: not the grant, the rule for an unattended chat,
  the limits or the loop rule. The list of who may dispatch to whom stays in the project's
  file and the machine's record.
- A chat can write persona definitions, so the line is an offer a chat may have written. Only
  a finished persona of the project is offered: never the persona itself, a name purlis keeps
  for itself or "any persona"; at most six; in alphabetical order.

**7. One answer may make several grants (V100-21).** Change 3 says the Notice "offers the
three levels" for the pair; an answer was one pair. It still offers the pair at the three
levels, and under the answers it now offers one box for each wanted persona that nothing
answers for yet.

- **A box is never ticked for the person.** An Allow keeps the asked pair and each ticked one
  at the level pressed. Each is a grant that names its pair, audited as its own, listed and
  revoked on its own.
- Keep blocked and Never for this pair are about the asked pair only.
- Not offered: a pair a grant covers, one the person said never to, one policy locks, one the
  person kept blocked in that chat (amendment 3: they are not asked twice, by a box either),
  one with a question of its own waiting (its brief is shown there), and any while the record
  of nevers does not read.
- **The answer is held to the question as it was shown.** The app builds the boxes and what
  each persona works with from the project's files, never from the request. The window sends
  back a digest of all of it. The files are read again at the answer: only a name still
  offered is granted, and where the question reads differently now nothing is granted or
  audited, the asked pair included, and the person is shown it again with no box ticked.
- No box makes "any persona" (amendment 2 holds).

**8. The question says what the target works with (V100-28).** For the persona asked about,
and for each box's persona, in purlis's words and from records no chat writes:

- the names of the vaults its chats are handed: the ones the vault registry tags for it and
  the ones the person let it use on this machine. Not the `vault:` line of its own file;
- the hosts the project's file declares for it;
- **the personas it may itself dispatch to**, from the grants in force for it: named ones, or
  "any persona", and nothing where there is none. The dispatcher added this in review of
  #1502, extending V100-28's sentence, and it is flagged for the operator: work handed to a
  persona can go on from it without a further question;
- each list is clipped to three with "and n more", and the digest covers the whole;
- **where the project's sandbox is off it says so of vaults and hosts alike**, since no list
  of either holds a chat there;
- never a secret's name or value. Allowing a dispatch does not allow the use of a secret:
  that consent is unchanged, and the question says so.

Still open, and not decided here: telling a teammate's "any persona" on the one-time Notice
(#1506), and what a persona removed and made again under the same name inherits (V100-61).
