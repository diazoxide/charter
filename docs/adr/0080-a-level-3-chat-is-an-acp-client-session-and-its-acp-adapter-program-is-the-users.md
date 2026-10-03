# A level-3 chat is an ACP client session, and its ACP adapter program is the user's

**Accepted 2026-10-01** by the operator (ruling V28), drafted for program-map ticket HP-1 (#668). It follows these of the
operator's rulings:

- **Q6′:** *"Three harness levels (terminal-only, hooks/native, ACP). Harnesses are declared as
  data in the plane, and charter never ships harness code."*
- **X34**, as [ADR 0073](0073-a-harness-is-declared-as-data-and-a-chat-runs-it-at-one-of-three-levels.md)
  renders it: *"never ships harness code" means charter never ships, patches or wraps a harness
  binary. Per-harness adapters are charter code, and they are data where a template suffices.*
- **V24a:** *"A chat in a tab starts in its terminal: level 2, or level 1 without an adapter, even
  when level 3 is offered."*
- **V16:** *"(a) The terminal, fleet-MCP and approval client scopes (FD-27) need a credential the
  chat sandbox cannot read, and `charterd` refuses them to connections from a chat's process tree;
  an agent can never answer its own asks or approve its own secret requests."*
- **V9**, the runner ruling, whose rule on harness logins this record applies to local chats as
  well: *"charter never reads, copies, stores or relays a harness credential"*.
- **W8:** *"Audit: O3 reads \"every tool call a level-2 or level-3 harness reports\""*.
- **W1:** *"HP-3, HP-8 and HP-16 become ★ blockers of LW-5; LW-5's acceptance answers a Claude
  Code, a Codex and an opencode ask."*
- **W10:** *"New FR \"no harness found\" (official installers in a shell tab, the harness's own
  login, local model fallback)."*
- **V27** (ADR 0076): *"V27a After a host crash, a run that was idle at a turn
  boundary on a harness that resumes natively becomes `hibernated`, not `failed`."* *"V27b When an
  upgrade hands a program over and its exit code is lost, the run is `completed` only if a
  `SessionEnd` came first, and `failed` otherwise."* *"V27c A pause is `SIGSTOP` of the run's
  process group at every level, with the guard as a backstop."*
- **The operator's standing rule on languages**, given 2026-09-18: *"final app should not use
  python, fully clean implementation in rust — no need to mix languages"*.

It builds on ADR 0073, which already decides most of what HP-1's title names, and does not
re-decide it. It also builds on
[ADR 0066](0066-a-chat-is-a-ulid-a-run-is-a-stretch-of-its-conversation-and-a-device-is-random.md)
(runs), [ADR 0067](0067-a-chat-runs-in-a-sandbox-charter-compiles-for-its-harness.md) (the
sandbox), [ADR 0068](0068-a-chat-lives-in-charterd-and-the-app-is-its-client.md) (`charterd` and
its scopes), [ADR 0071](0071-the-kill-switch-is-machine-state-the-command-line-stops-and-only-the-window-re-arms.md)
(the kill switch), [ADR 0074](0074-a-chats-git-runs-charters-hooks-through-its-environment.md)
(a chat's git hooks), [ADR 0075](0075-an-audit-entry-is-metadata-in-a-store-of-its-own-and-telemetry-never-reads-it.md)
(the audit) and [ADR 0076](0076-a-run-moves-only-by-a-named-cause-and-a-chats-state-is-read-from-its-runs.md) (the run lifecycle). It **amends**
ADR 0067, ADR 0068 and ADR 0073, each in a section of its own below. HP-2 (the ACP client), HP-3
(Codex), HP-4 (Claude Code), HP-5 and HP-16 (the normalised Ask, answered over ACP) and HP-14
(registry agents) build on it. Its concept is **Chat**.

## Where charter is today

No chat runs over ACP, and charter has no ACP code. Every chat runs its harness in a PTY that
`charterd` owns (ADR 0068), at level 2 where charter has a harness adapter and armed it, or level
1 otherwise. What a chat is doing comes from its harness's hooks on the hook channel. An approval
the harness wants is a hook line (Claude Code's `PermissionRequest` and `Notification`), and the
operator answers it in the pane. charter's guard is a `PreToolUse` hook: it sees a tool call before
it runs and can refuse it with a sentence saying why. The normalised Ask (HP-5) is not built.

## What ADR 0073 already settled

HP-1 is mostly settled. ADR 0073 was drafted after HP-1 was filed, and it answers the ticket's
title and its outcome line:

| HP-1 asks | Settled by |
|---|---|
| ACP is level 3 | ADR 0073 §1: level 3 is *"a structured protocol"*, with *"ACP as the one charter writes once"*. |
| "Beside the PTY" | ADR 0073 §2 and V24a: it holds **per harness, not per chat**. A tab chat starts in its terminal; a chat with no terminal shown starts at the highest level its harness offers. The one way a chat has both is FD-20. |
| *"ACP adapters are user-installed harness-side programs that charter spawns and never ships"* | ADR 0073 §4: an ACP adapter program *"The user installs it, and charter spawns it like any declared program."* ADR 0073's rejected list: shipping them. |
| How a harness says it offers ACP | ADR 0073 §3: the declaration's `[levels] acp` argv. |
| A level-3 chat's state | ADR 0073, *The hooks-only rule, amended*: from its structured protocol. |
| When ACP stops working mid-chat | ADR 0073, *ADR 0066, amended*: the `fallback` cause. |
| What the audit says a level-3 run covered | ADR 0073 §7 and ADR 0075 §5: coverage `protocol`. |

What is left for this record is four things: **how charter speaks ACP**, **which ACP adapter
programs it relies on and what happens without them**, **what the two sides negotiate**, and **how
a permission request reaches the guard and needs you**.

## The decision

**A level-3 chat over ACP is one ACP session on one agent process that `charterd` spawns in the
chat's sandbox and speaks to over stdio, as the client. charter offers the agent no file system
and no terminal of its own, never supplies a credential, and never stands between the agent and
another client. A permission request is an ask: the guard answers it first, and otherwise it
waits in needs you until a human scope answers it through ACP.**

### 1. How charter speaks ACP

- **charter is an ACP client and nothing else.** It is never an ACP agent, and never a proxy
  between another client and an agent. ADR 0073 §4 names *"a proxy that edits its traffic"* as
  standing in, which charter never does. Research track 04 suggested sitting as an ACP proxy to
  inject charter's tools. HP-7's MCP server does that job without it (below).
- **`charterd` is the client.** The agent process is a chat's program, so it lives where every
  chat lives (ADR 0068). The app, `charter attach` and every other client reach a level-3 chat
  through `charterd`'s session protocol, never over ACP. The client is Rust, on the
  `agent-client-protocol` crate (HP-2), under the operator's standing rule on languages. The ACP
  adapter programs may be Node: they are the user's, not charter's.
- **The transport is stdio, and only stdio.** ACP's HTTP and WebSocket transports are marked
  work in progress, and charter does not need them: a chat on a runner is a chat of the
  `charterd` on that runner (ADR 0068 §8), which speaks stdio to its agent there.
- **One agent process per chat, one session per process.** charter never shares an agent process
  between chats. A shared process would be one sandbox for two chats, one process group for the
  kill switch, and one process tree for V16's check.
- **The agent process is started like a terminal chat's program**, with three differences only:
  its argv is the declaration's `[levels] acp` argv, its stdio is two pipes `charterd` holds
  instead of a PTY, and it has no pane. The rest is the same:
  - it leads its own process group, so ADR 0068 §5's process-tree check, ADR 0071's stop and
    V27c's pause of the whole process group hold for it. **Amended by V77:** it does not lead
    its own session, as a PTY program does, because `setsid` in the child needs a `pre_exec`,
    which is `unsafe`, and the one audited block is the executor's. It stays in the host's
    session, so **the host never has a controlling terminal**: the app (and `charterd`, once it
    runs on its own) leaves any terminal it was started from before it starts a chat
    (`charter_core::noterminal`), and a level-3 chat refuses to start while the host still has
    one;
  - it gets the chat's environment, including the chat's number, token and hook socket, so a
    `charter` command it runs and ADR 0074's git hooks work as in any chat;
  - `session/new` gets the chat's worktree as `cwd`, and **charter's MCP server** in
    `mcpServers` once HP-7 builds it. Nothing else is injected.
- **Its sandbox is the chat's**, compiled for the program that actually runs at level 3. See
  *ADR 0067, amended*.

### 2. What charter offers the agent: no file system, no terminal, no elicitation yet

At `initialize` the client says which client methods it serves. **charter serves
`session/request_permission` and nothing optional: `fs.readTextFile`, `fs.writeTextFile` and
`terminal` are all false, and `elicitation/create` is not offered.**

- **`fs/*` and `terminal/*` would make `charterd` do the chat's work.** Their calls are
  answered by the client, which is `charterd`, outside the chat's sandbox. ADR 0067 §2 runs the
  other way: what reaches past the sandbox is asked of `charterd` and decided there, and the
  chat's own reads, writes and commands stay inside. With both off, an ACP agent uses its own
  tools in its own process, under the chat's sandbox, as it does at levels 1 and 2. **That the
  built-in ACP adapter programs and opencode work fully with both off is expected and not yet
  verified.** HP-2's acceptance tests each one with both off.
- **Elicitation waits for HP-5's `elicits_secret`.** A request for a value from the human is the
  class of ask where a secret could reach the transcript. charter offers it once HP-5 can mark
  and route such an ask, and not before.

### 3. Which ACP adapter programs, and what happens without one

**charter relies on no ACP adapter program to run a chat.** Level 1 needs none, and level 2 needs
only charter's own harness adapter. An ACP adapter program raises a harness to level 3; its
absence lowers the offer, never the product.

| Harness | Level 3 through | Ticket |
|---|---|---|
| opencode | its own ACP mode, the same program | HP-2, first |
| Codex | the `codex-acp` ACP adapter program, or the Codex app-server (HP-3 picks, per ADR 0073 §1) | HP-3 |
| Claude Code | the `claude-agent-acp` ACP adapter program, falling back to the terminal | HP-4 (Later) |
| Any other | whatever its project declaration's `[levels] acp` names | HP-14 |

- **An ACP adapter program is not a harness adapter.** The harness adapter is charter code (X34).
  The ACP adapter program is the user's, installed by the user, updated by the user, and found
  as a bare name on `PATH`, as ADR 0073 §5 requires of every program a declaration names.
  `CONTEXT.md` gains the term.
- **A built-in declaration may name one.** Its `[levels] acp` argv names the ACP adapter program,
  and the built-in's `tested` range covers it (ADR 0073 §8). A project declaration's `acp` argv
  is part of what the machine approves under V24b.
- **When it is missing, charter says so where the operator meets it.** The capability card reads
  the level-3 offer as *no, the ACP adapter program is not installed*. `charter doctor` names it.
  A chat that would have started at level 3 starts at its next level and says why. Applying W10's
  "no harness found" FR (*"official installers in a shell tab"*) to ACP adapter programs, charter
  may offer the program's own documented install command in a shell tab, which the operator runs.
  charter never downloads, bundles or updates one.
- **charter never writes an ACP adapter program of its own.** One for Claude Code would be charter
  code standing where the harness's program is expected, which ADR 0073 §4 forbids.
- **What W1 gets from this.** HP-16 answers a Codex ask and an opencode ask over ACP. HP-4 is
  Later, so the Claude Code ask in LW-5's acceptance is answered at level 2, through Claude Code's
  own permission hooks (HP-6), not over ACP.

### 4. What the two sides negotiate

- **Protocol version.** charter supports the ACP protocol version its client crate implements.
  An agent that answers `initialize` with a version charter does not support does not start at
  level 3: the chat starts at its next level and says why. That start never became a level-3
  run, so it is not a `fallback`; it is a first run at the lower level.
- **Authentication, applying V9 to local chats.** charter never supplies a credential or any
  other value to `authenticate`. It may call `authenticate` with a `methodId` alone for a method
  in which the agent runs its own login flow (a browser sign-in the agent opens, for example),
  because that passes nothing through charter. A method that asks the client for a value is never
  used. When no usable method remains, charter tells the operator to log in through the harness's
  own flow in a shell tab (the declaration's `login`), and the chat starts at its next level.
- **Agent capabilities become harness capabilities for the run.** What the agent reports at
  `initialize` (loading a session, the prompt content it takes, and each optional method it
  advertises) is mapped onto ADR 0073 §6's harness capabilities for that run: `loadSession` is
  `resumes_by_id`, and so on, one row per flag, in HP-2. **An optional method charter has not
  seen advertised is never called.** Silence is *no*, as ADR 0073 §6 says.
- **The declaration is the expectation; the handshake is the fact.** See *ADR 0073, amended*.

### 5. A permission request is an ask: the guard first, then needs you

A `session/request_permission` carries the tool call and the options the agent offers (allow
once, allow always, reject once, reject always). HP-5 normalises it into the Ask; HP-16 answers it.
This record fixes the order and who may answer:

1. **The guard answers first.** charter's guard reads the request's tool call as it reads a
   `PreToolUse` hook's. If it would refuse, charter answers with the agent's reject option at
   once, the operator never sees an ask, and the refusal and its reason are shown in the chat and
   written to the audit. ACP's answer carries only an option, so the agent learns *rejected*, not
   why; charter adds the reason in the response's metadata, which an agent may ignore.
2. **Otherwise it is needs you.** The chat is waiting from the request until its answer. The
   options are shown as the agent gave them, in its words, except for "allow always", which V28c
   limits.
3. **Only a human scope answers. Settled by V16:** *"an agent can never answer its own asks or
   approve its own secret requests"*. An answer comes from a `local-ui` or `approval` client
   (ADR 0068 §5), and later a paired device under LW-9. The agent's own stdio can ask and never
   answer; see *ADR 0068, amended*. The first answer wins and every other gets "answered
   elsewhere" (HP-5), which ACP's one-response-per-request rule already requires.
4. **Its deadline is null.** HP-5's Ask carries a deadline that *"sits below the harness's hook
   timeout"*, so that charter's answer lands before the harness gives up on its hook. ACP has no
   such timeout: the agent waits for its answer. There is no bound to sit below, so an ACP ask's
   deadline is **null**, which HP-5's shape must allow, and charter adds none of its own. An
   unanswered ask stays in needs you until it is answered, the turn is cancelled, or the chat is
   stopped. A secret's approval is a different ask with its own timeout (V15), decided in
   `charterd`, never over ACP.
5. **Stop cancels; the kill switch does not wait.** Stop on a level-3 chat sends `session/cancel`
   and answers every pending request `cancelled`, as ACP requires. The kill switch sends nothing
   and waits for nothing: it ends the agent's process group as it ends any chat's program
   (ADR 0071).
6. **A pause holds the ask (V27c).** A paused run's process group is stopped, so it can read
   nothing. A pending request stays in needs you and may still be answered; `charterd` holds the
   answer and writes it when the run is resumed, and the ask reads *answered, delivered on
   resume*. A paused agent sends no new request, so the guard has nothing to answer until then.

**Audit actions.** Per ADR 0075 §4, every action is registered in AU-2's registry. **HP-5 owns
the ask actions**, since an ask has the same shape at every level, and this record gives it the
ACP inputs: an ask raised (source `acp`, the option kinds offered), answered (the option kind
chosen, the answering scope, the device), and withdrawn (cancelled by Stop or `session/cancel`).
**A guard refusal is a `tool.call` entry**, which ADR 0075's registry already has, with *"the
guard rule that decided"* and a result of refused; it needs no new action. **One new action is
this record's**: `acp.call.refused`, when the agent calls a client method charter did not offer
(*ADR 0068, amended*), with the method's name as its `meta`.

**What the guard can and cannot do at level 3.** An ACP agent asks only when it chooses to. A
tool call it runs without asking reaches charter as a `tool_call` update, often once it has run.
charter audits every one (W8, coverage `protocol`), and the guard can refuse none of them. This
record gives the SD-14 policy work an input, not a ruling: **an ACP run's `denies_before_run` is
*no***, and the sandbox is the boundary, as it is for every chat (ADR 0067). ADR 0073 §7 forwarded
the requirement to name that capability to that work; this is the answer for ACP. A harness
adapter may still arm the harness's own hooks alongside ACP where the harness runs them (HP-3 and
HP-4 decide per harness), and a run so armed may read *yes* once TS1 measures it.

### 6. An upgrade and a host crash

`charterd` holds the agent's stdio, as it holds a terminal chat's PTY, so a level-3 run meets the
host's lifecycle the same way:

- **An upgrade** (ADR 0068 §7, FD-28) hands the two pipes over as it hands a PTY over, with the
  client's state: the session id, the negotiated capabilities, and the pending request ids with
  their asks. A run whose handover fails is drained as a terminal chat's is. Applying V27b, the
  protocol stands in for `SessionEnd` (ADR 0073, *ADR 0066, amended*): a run whose exit code is
  lost is `completed` only if the agent had answered its last `session/prompt` and no turn was
  open, and `failed` otherwise.
- **A host crash** closes `charterd`'s ends of the pipes, as it closes PTY masters (ADR 0068 §3).
  The agent reads end of input and exits. Applying V27a, a run that was idle at a turn boundary
  becomes `hibernated` where the agent advertised `loadSession`, and is resumed on a new agent
  process with `session/load`; otherwise it is `failed`, as a mid-turn run always is. A pending
  ask dies with its run, and needs you drops it with the run's state.

### 7. Tiers

**This record adds no store.** What it records goes to stores other records set:

- the negotiated protocol version and harness capabilities are recorded on the run, in the event
  log (FD-9): **Machine**, device-bound (ADR 0069);
- each ask, its answer, who answered through which scope, each guard refusal and each refused
  client call go to the audit store (ADR 0075): **Machine**, device-bound, in charter's data home;
- the handover state in §6 travels inside FD-28's handover, which ADR 0068 §7 already covers, and
  is not kept once the new host holds it;
- the agent's own session files and settings are the harness's, written by it and not by charter,
  like a terminal chat's today. They are no charter store.

## ADR 0067, amended

ADR 0067 §2's table compiles the sandbox **per harness**: Claude Code's `--settings`, Codex's
permissions profile, and a generated OS profile for opencode. At level 3 the program that runs is
not always the harness's own: it is an ACP adapter program, or the harness in another mode. So
the table gains a row, and the compile's key changes:

| Harness | What charter compiles the policy into |
|---|---|
| **Any harness at level 3** | **Per program and level, not per harness.** The harness's native compile counts only where SD-2 shows that this program, at this level, carries it to the harness (for example, the ACP adapter program passes the permissions profile on to Codex). Until SD-2 shows that for a program, the chat is wrapped in charter's generated OS profile. |

- **The generated OS profile adds to the native sandbox; it replaces it only under V21's ruling
  4.** ADR 0067 §2 already says *"Where both apply, the stricter answer wins"*. At level 3 the
  wrap is added around the program and the harness's own sandbox stays compiled wherever the
  program carries it.
- **Codex under `codex-acp`: its own sandbox is still compiled.** Ruled item 4, *"charter may turn
  Codex's own sandbox off only when its wrap is measured strictly stricter; otherwise Codex keeps
  its own"*, holds at level 3 unchanged. Where `codex-acp` cannot carry the permissions profile,
  the chat is wrapped as well, and Codex keeps whatever sandbox of its own it applies; charter
  never turns it off unmeasured. HP-3 and SD-2 measure it, and the app-server route gets the same
  measurement.
- **Windows: a level-3 chat is refused.** Ruled item 3 starts Windows chats *"at the visible
  opt-out until a backend exists"*. A level-3 chat has no terminal tab to show that opt-out on for
  as long as it lives, which ADR 0067 §7 requires, and no generated profile exists there to wrap
  it. So it fails closed: charter refuses to start a chat at level 3 on Windows, says why, and
  offers it at its next level in a tab, where the opt-out is visible. This lifts when a Windows
  backend exists or SD-2 measures a native route through the program.

The rest of ADR 0067 stands.

## ADR 0068, amended

ADR 0068 §5's table gives the `chat` scope to *"a hook, or a `charter` command, inside one chat"*
on *"that plane's hook socket"*. A level-3 chat has a second channel into `charterd`: its agent's
stdio. That channel is **`chat` scope**, and adds one row's worth to it:

- **What it may do:** report (`session/update`, the response to `session/prompt`) and ask
  (`session/request_permission`). It may call no other client method, since §2 of this record
  offers none, and a call to one is refused and recorded (`acp.call.refused`).
- **What it may never do:** answer an ask, its own or any chat's, or reach any human scope.
  Answering stays with `local-ui` and `approval` (§5 of ADR 0068).
- **Its credential is the pipe.** `charterd` created both ends when it spawned the process, so
  nothing else can speak on it. The chat's token still applies to its hook socket, where a
  level-3 chat's `charter` commands and git hooks report.

The rest of ADR 0068 §5 stands.

## ADR 0073, amended

ADR 0073 §6 says *"A flag's answer can differ by level, since a harness can report more over its
own protocol than through its hooks."* For a level-3 run over ACP, this record adds where the
answer comes from:

- **The declaration's level-3 answers are the expectation**, and the capability card shows them
  before any run, as ADR 0073 §6 says.
- **The agent's `initialize` answer is the fact for the run.** Where the two differ, the
  handshake wins for that run, the run records it (§7 above), and a control the difference
  disables names it at its point of action. The declaration is not rewritten, and TS1's nightly
  run is where a built-in's expectation is corrected.
- **Input to SD-14, not a ruling: a `yes` a policy relies on needs both.** A capability the
  declaration says *no* or *unknown* to should not be trusted by a policy on the agent's word
  alone, `denies_before_run` first among them. The policy work decides.

ADR 0073 §4's last bullet, *"HP-1's row already says charter never ships one, and HP-1 records
it"*, is recorded by §3 above.

## What changes where

When this record was accepted, no code changed with it. V77 later changed one thing in the
host: the app leaves its controlling terminal as it starts (`charter_core::noterminal`, §1).

| Where | What changes |
|---|---|
| The app, `charterd` | V77: the host leaves its controlling terminal at start, and a level-3 chat refuses to start while it has one |
| `charter-core` / `charterd` | HP-2: the ACP client in `charterd`, stdio, with §2's client capabilities, §4's negotiation, §5's order and §6's handover state |
| Declarations | FD-14: the built-ins' `[levels] acp` argv names the ACP adapter program; `tested` covers it |
| The capability card and `charter doctor` | HP-19: *no, the ACP adapter program is not installed*, and the negotiated differences |
| The normalised Ask | HP-5: a null deadline for ACP asks, and the ask actions; HP-16: §5's order, scopes, cancellation and pause |
| Sandbox | SD-2: the level-3 row, per program and level; Windows refuses level 3 until a backend exists |
| Audit | AU-2 registers `acp.call.refused`, and HP-5's ask actions |
| Policy | SD-14: the inputs in §5 and *ADR 0073, amended* |
| `CONTEXT.md` | Gains **ACP adapter program** beside **Harness adapter** (in this PR) |

## What this costs

- **No file system from the client means no edit review before it lands.** An editor that
  serves `fs/write_text_file` sees each write first. charter sees an ACP agent's edits as tool
  calls and diffs, once they are made, and holds them to the worktree through the sandbox.
- **The guard sees only what the agent asks about.** At level 2 a Claude Code chat's guard
  refuses before anything runs. At level 3 it answers asks, and audits the rest. Every "allow
  always" an operator gives widens the set of calls the guard never sees again.
- **A level-3 offer depends on a program the user maintains.** When the ACP adapter program lags
  its harness, the capability card says so, and a chat starts in its terminal. charter cannot fix
  an ACP adapter program's gaps on the user's behalf.
- **No level 3 on Windows** until a sandbox backend exists there.

## What was rejected

- **charter as an ACP proxy or conductor**, injecting its tools into another client's session.
  It is standing in (ADR 0073 §4), and HP-7's MCP server, passed at `session/new`, reaches every
  ACP agent without it.
- **Serving `fs/*` and `terminal/*`.** See §2 and V28a.
- **A shared agent process for several chats.** See §1.
- **ACP over HTTP or WebSocket.** Work in progress in ACP, and a runner's own `charterd` makes it
  unnecessary.
- **Supplying a credential or any value to `authenticate`.** V9, applied in §4.
- **A charter-written ACP adapter program for Claude Code.** It would stand in for the harness's
  program, and it would break the operator's standing rule on languages.

## Ruled (V28, 2026-10-01)

1. **charter offers ACP agents no `fs` and no `terminal` client methods.** A harness whose ACP
   adapter program can't work without them waits for level 3 (V28a).
2. **A level-3 chat opened in a tab shows its transcript as a view tab.** *Continue in terminal*
   is an explicit `switch`, disabled with the reason shown when the harness lacks
   `resumes_by_id`. New tab chats still start in their terminal (V24a) (V28b).
3. **"Allow always" is shown only where the ACP adapter program keeps it for the session.** Where
   it persists into the worktree's harness settings, as `claude-agent-acp` does into
   `.claude/settings.local.json`, charter hides it and offers allow-once only, saying why. A policy
   can hide it everywhere (C9) (V28c).
4. **An ACP ask has no deadline.** It stays in needs you until answered, cancelled or stopped
   (V28d).

## Ruled (V77, 2026-10-03, the HP-2 review)

5. **A level-3 agent leads its own process group, not its own session, and the host never has
   a controlling terminal.** `process_group(0)` stays; no new `unsafe`. The app and `charterd`
   leave their terminal as they start, with a test that a host started in a terminal has none
   afterwards, and none after it opens a terminal pair. §1 is amended to say so.
