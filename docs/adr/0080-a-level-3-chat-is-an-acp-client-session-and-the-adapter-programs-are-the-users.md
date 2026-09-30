# A level-3 chat is an ACP client session, and the adapter programs are the user's

**Proposed 2026-10-01**, drafted for program-map ticket HP-1 (#668). It follows these of the
operator's rulings:

- **Q6′:** *"Three harness levels (terminal-only, hooks/native, ACP). Harnesses are declared as
  data in the plane, and charter never ships harness code."*
- **X34**, accepted with the consistency review: *"\"Never ships harness code\" means binaries;
  adapters are charter code"*.
- **V24a:** *"A chat in a tab starts in its terminal: level 2, or level 1 without an adapter, even
  when level 3 is offered."*
- **V16:** *"(a) The terminal, fleet-MCP and approval client scopes (FD-27) need a credential the
  chat sandbox cannot read, and `charterd` refuses them to connections from a chat's process tree;
  an agent can never answer its own asks or approve its own secret requests."*
- **V9**, on harness logins: *"charter never reads, copies, stores or relays a harness
  credential"*.
- **W8:** *"Audit: O3 reads \"every tool call a level-2 or level-3 harness reports\""*.
- **W1:** *"HP-3, HP-8 and HP-16 become ★ blockers of LW-5; LW-5's acceptance answers a Claude
  Code, a Codex and an opencode ask."*
- **W10:** *"New FR \"no harness found\" (official installers in a shell tab, the harness's own
  login, local model fallback)."*

It builds on [ADR 0073](0073-a-harness-is-declared-as-data-and-a-chat-runs-it-at-one-of-three-levels.md),
which already decides most of what HP-1's title names, and does not re-decide it. It also
builds on [ADR 0066](0066-a-chat-is-a-ulid-a-run-is-a-stretch-of-its-conversation-and-a-device-is-random.md)
(runs), [ADR 0067](0067-a-chat-runs-in-a-sandbox-charter-compiles-for-its-harness.md) (the
sandbox), [ADR 0068](0068-a-chat-lives-in-charterd-and-the-app-is-its-client.md) (`charterd` and
its scopes), [ADR 0071](0071-the-kill-switch-is-machine-state-the-command-line-stops-and-only-the-window-re-arms.md)
(the kill switch) and
[ADR 0075](0075-an-audit-entry-is-metadata-in-a-store-of-its-own-and-telemetry-never-reads-it.md)
(the audit). It **amends** ADR 0068 and ADR 0073, each in a section of its own below. HP-2 (the
ACP client), HP-3 (Codex), HP-4 (Claude), HP-5 and HP-16 (the normalised Ask, answered over ACP)
and HP-14 (registry agents) build on it. Its concept is **Chat**.

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

What is left for this record is four things: **how charter speaks ACP**, **which adapter programs
it relies on and what happens without them**, **what the two sides negotiate**, and **how a
permission request reaches the guard and needs you**.

## The decision

**A level-3 chat over ACP is one ACP session on one agent process that `charterd` spawns in the
chat's sandbox and speaks to over stdio, as the client. charter offers the agent no file system
and no terminal of its own, never supplies a credential, and never stands between the agent and
another client. A permission request is an ask: the guard answers it first, and otherwise it
waits in needs you until a human scope answers it through ACP.**

### 1. How charter speaks ACP

- **charter is an ACP client and nothing else.** It is never an ACP agent, and never a proxy
  between another client and an agent. Settled by ADR 0073 §4: *"a proxy that edits its traffic"*
  is standing in, which charter never does. Research track 04 suggested sitting as an ACP proxy to
  inject charter's tools. HP-7's MCP server does that job without it (below).
- **`charterd` is the client.** The agent process is a chat's program, so it lives where every
  chat lives (ADR 0068). The app, `charter attach` and every other client reach a level-3 chat
  through `charterd`'s session protocol, never over ACP. The client is Rust, on the
  `agent-client-protocol` crate (HP-2), because ADR 0067 keeps *"no language mixing in the shipped
  app"*. The adapter programs may be Node: they are the user's, not charter's.
- **The transport is stdio, and only stdio.** ACP's HTTP and WebSocket transports are marked
  work in progress, and charter does not need them: a chat on a runner is a chat of the
  `charterd` on that runner (ADR 0068 §8), which speaks stdio to its agent there.
- **One agent process per chat, one session per process.** charter never shares an agent process
  between chats. A shared process would be one sandbox for two chats, one process group for the
  kill switch, and one process tree for V16's check.
- **The agent process is started like a terminal chat's program**, with three differences only:
  its argv is the declaration's `[levels] acp` argv, its stdio is two pipes `charterd` holds
  instead of a PTY, and it has no pane. The rest is the same:
  - it leads its own session, as a PTY program does, so ADR 0068 §5's process-tree check and
    ADR 0071's stop of the whole process group hold for it;
  - it gets the chat's environment, including the chat's number, token and hook socket, so a
    `charter` command it runs and ADR 0074's git hooks work as in any chat;
  - `session/new` gets the chat's worktree as `cwd`, and **charter's MCP server** in
    `mcpServers` once HP-7 builds it. Nothing else is injected.
- **Its sandbox is the chat's.** Settled by ADR 0067 §2: *"The compile is total, or the chat is
  wrapped."* The adapter program is what runs, so a harness's native sandbox counts only where
  SD-2 shows its compiled policy reaches through that program to the harness. Until it does for a
  given adapter, the chat is wrapped in charter's generated OS profile, as an opencode chat is
  today.

### 2. What charter offers the agent: no file system, no terminal, no elicitation yet

At `initialize` the client says which client methods it serves. **charter serves
`session/request_permission` and nothing optional: `fs.readTextFile`, `fs.writeTextFile` and
`terminal` are all false, and `elicitation/create` is not offered.**

- **`fs/*` and `terminal/*` would make `charterd` do the chat's work.** Their calls are
  answered by the client, which is `charterd`, outside the chat's sandbox. ADR 0067 §2 runs the
  other way: what reaches past the sandbox is asked of `charterd` and decided there, and the
  chat's own reads, writes and commands stay inside. With both off, an ACP agent uses its own
  tools in its own process, under the chat's sandbox, as it does at levels 1 and 2. The ACP
  adapter programs and opencode run without them; HP-2 tests each built-in with both off.
- **Elicitation waits for HP-5's `elicits_secret`.** A request for a value from the human is the
  class of ask where a secret could reach the transcript. charter offers it once HP-5 can mark
  and route such an ask, and not before.

### 3. Which adapter programs, and what happens without one

**charter relies on no adapter program to run a chat.** Level 1 needs none, and level 2 needs
only charter's own harness adapter. An ACP adapter program raises a harness to level 3; its
absence lowers the offer, never the product.

| Harness | Level 3 through | Ticket |
|---|---|---|
| opencode | its own ACP mode, the same program | HP-2, first |
| Codex | the `codex-acp` adapter program, or the Codex app-server (HP-3 picks, per ADR 0073 §1) | HP-3 |
| Claude Code | the `claude-agent-acp` adapter program, falling back to the terminal | HP-4 (Later) |
| Any other | whatever its project declaration's `[levels] acp` names | HP-14 |

- **An ACP adapter program is not a harness adapter.** The harness adapter is charter code (X34).
  The ACP adapter program is the user's, installed by the user, updated by the user, and found
  as a bare name on `PATH`, as ADR 0073 §5 requires of every program a declaration names.
  `CONTEXT.md` gains the term.
- **A built-in declaration may name one.** Its `[levels] acp` argv names the adapter program, and
  the built-in's `tested` range covers it (ADR 0073 §8). A project declaration's `acp` argv is
  part of what the machine approves under V24b.
- **When it is missing, charter says so where the operator meets it.** The capability card reads
  the level-3 offer as *no, the adapter program is not installed*. `charter doctor` names it.
  A chat that would have started at level 3 starts at its next level and says why. Applying W10's
  *"official installers in a shell tab"*, charter may offer the adapter's own documented install
  command in a shell tab, which the operator runs. charter never downloads, bundles or updates one.
- **charter never writes an adapter program of its own.** One for Claude Code would be charter
  code standing where the harness's program is expected, which ADR 0073 §4 forbids.

### 4. What the two sides negotiate

- **Protocol version.** charter supports the ACP protocol version its client crate implements.
  An agent that answers `initialize` with a version charter does not support does not start at
  level 3: the chat starts at its next level and says why. That start never became a level-3
  run, so it is not a `fallback`; it is a first run at the lower level.
- **Authentication. Settled by V9:** *"charter never reads, copies, stores or relays a harness
  credential"*. When an agent requires authentication, charter does not supply one. It tells the
  operator to log in through the harness's own flow in a shell tab (the declaration's `login`),
  and the chat starts at its next level. An auth method that asks the client for a value is
  never used.
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
   options are shown as the agent gave them, in its words.
3. **Only a human scope answers. Settled by V16:** *"an agent can never answer its own asks or
   approve its own secret requests"*. An answer comes from a `local-ui` or `approval` client
   (ADR 0068 §5), and later a paired device under LW-9. The agent's own stdio can ask and never
   answer; see *ADR 0068, amended*. The first answer wins and every other gets "answered
   elsewhere" (HP-5), which ACP's one-response-per-request rule already requires.
4. **It waits.** An ACP agent waits for its answer, and charter adds no deadline of its own: an
   unanswered ask stays in needs you until it is answered, the turn is cancelled, or the chat is
   stopped. A secret's approval is a different ask with its own timeout (V15), decided in
   `charterd`, never over ACP.
5. **Stop cancels; the kill switch does not wait.** Stop on a level-3 chat sends `session/cancel`
   and answers every pending request `cancelled`, as ACP requires. The kill switch sends nothing
   and waits for nothing: it ends the agent's process group as it ends any chat's program
   (ADR 0071).

**What the guard can and cannot do at level 3.** An ACP agent asks only when it chooses to. A
tool call it runs without asking reaches charter as a `tool_call` update, often once it has run.
charter audits every one (W8, coverage `protocol`), and the guard can refuse none of them. So
**an ACP run's `denies_before_run` is *no***, and the sandbox is the boundary, as it is for every
chat (ADR 0067). ADR 0073 §7 forwarded the requirement to name that capability to the W8 policy
work; this record supplies the answer for ACP. A harness adapter may still arm the harness's own
hooks alongside ACP where the harness runs them (HP-3 and HP-4 decide per harness), and a run so
armed may read *yes* once TS1 measures it.

### 6. Tiers

**This record adds no store.** What it records goes to stores other records set:

- the negotiated protocol version and harness capabilities are recorded on the run, in the event
  log (FD-9): **Machine**, device-bound (ADR 0069);
- each permission request, its answer, who answered through which scope, and each guard refusal
  go to the audit store (ADR 0075): **Machine**, device-bound, in charter's data home;
- the agent's own session files are the harness's, written by it and not by charter, like a
  terminal chat's today. They are no charter store.

## ADR 0068, amended

ADR 0068 §5's table gives the `chat` scope to *"a hook, or a `charter` command, inside one chat"*
on *"that plane's hook socket"*. A level-3 chat has a second channel into `charterd`: its agent's
stdio. That channel is **`chat` scope**, and adds one row's worth to it:

- **What it may do:** report (`session/update`, the response to `session/prompt`) and ask
  (`session/request_permission`). It may call no other client method, since §2 of this record
  offers none, and a call to one is refused and recorded.
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
  handshake wins for that run, the run records it (§6 above), and a control the difference
  disables names it at its point of action. The declaration is not rewritten, and TS1's nightly
  run is where a built-in's expectation is corrected.
- **A `yes` needs both.** A capability the declaration says *no* or *unknown* to is not used on
  the agent's word alone when it is one a policy relies on, `denies_before_run` first among them.
  An agent's own report cannot raise what a policy trusts.

ADR 0073 §4's last bullet, *"HP-1's row already says charter never ships one, and HP-1 records
it"*, is recorded by §3 above.

## What changes where

The code does not change with this record.

| Where | What changes |
|---|---|
| `charter-core` / `charterd` | HP-2: the ACP client in `charterd`, stdio, with §2's client capabilities, §4's negotiation and §5's order |
| Declarations | FD-14: the built-ins' `[levels] acp` argv names the adapter program; `tested` covers it |
| The capability card and `charter doctor` | HP-19: *no, the adapter program is not installed*, and the negotiated differences |
| The normalised Ask | HP-5 and HP-16: §5's order, scopes and cancellation |
| Sandbox | SD-2: per adapter program, whether the native compile reaches through it; until then, the wrap |
| Policy | SD-14's policy layer: `denies_before_run` is *no* for an ACP run |
| `CONTEXT.md` | Gains **ACP adapter program** beside **Harness adapter** (in this PR) |

## What this costs

- **No file system from the client means no edit review before it lands.** An editor that
  serves `fs/write_text_file` sees each write first. charter sees an ACP agent's edits as tool
  calls and diffs, once they are made, and holds them to the worktree through the sandbox.
- **The guard sees only what the agent asks about.** At level 2 a Claude Code chat's guard
  refuses before anything runs. At level 3 it answers asks, and audits the rest.
- **A level-3 offer depends on a program the user maintains.** When the adapter program lags its
  harness, the capability card says so, and a chat starts in its terminal. charter cannot fix
  an adapter's gaps on the user's behalf.

## What was rejected

- **charter as an ACP proxy or conductor**, injecting its tools into another client's session.
  It is standing in (ADR 0073 §4), and HP-7's MCP server, passed at `session/new`, reaches every
  ACP agent without it.
- **Serving `fs/*` and `terminal/*`.** See §2 and the first open question.
- **A shared agent process for several chats.** See §1.
- **ACP over HTTP or WebSocket.** Work in progress in ACP, and a runner's own `charterd` makes it
  unnecessary.
- **Supplying credentials to `authenticate`.** V9.
- **A charter-written ACP adapter for Claude Code.** It would stand in for the harness's program,
  and it would be Node in the shipped app.

## For the operator's ruling

ADR 0073 and V24 settled HP-1's title and outcome line; these are the questions still open.

1. **charter offers ACP agents no `fs` and no `terminal` client methods.** *Recommend yes.* Served
   by `charterd`, they would run the chat's reads, writes and commands outside its sandbox, and
   charter would own a second enforcement path to keep equal to ADR 0067's. The cost is that
   charter never sees an edit before it lands; revisit only if SD work finds a way to serve writes
   under the chat's own compiled policy.
2. **Opening a level-3 chat in a tab shows its structured transcript as a view tab**, with its
   asks and a prompt box, and does not switch it to its terminal. *Continue in terminal* is an
   explicit action that starts a new run with cause `switch`, where the harness resumes by id.
   *Recommend yes.* V24a says where a tab chat *starts*; a chat dispatched headless and opened
   later should not lose its turn in flight to a silent switch.
3. **"Allow always" options are shown as the agent gives them**, and a policy that requires every
   tool call to be asked hides them (strictest wins, C9). *Recommend yes.* The option is the
   agent's own remembered rule, which charter cannot see into, so hiding it by default would
   break agents' normal flow for no guarantee charter could hold.
4. **An unanswered ACP ask has no charter deadline.** *Recommend yes.* It stays in needs you until
   it is answered, cancelled or stopped. A budget (N4) or Stop ends the wait; a silent timeout
   that rejects would read as the operator's refusal.
