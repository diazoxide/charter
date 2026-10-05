# Agents work together as chats that are listed, observable and stoppable, and no agent's word is consent

**Proposed 2026-10-05** for the operator's acceptance, drafted for program-map ticket AC-1 (#713),
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
