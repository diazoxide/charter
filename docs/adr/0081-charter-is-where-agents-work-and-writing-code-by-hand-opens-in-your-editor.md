# charter is where agents work, and writing code by hand opens in your editor

**Proposed 2026-10-01**, drafted for program-map ticket ED-1 (#719). It follows these of the
operator's rulings:

- **F2:** *"charter is the control plane, not a full editor. (a) Editor integrations: VS Code,
  Zed and JetBrains extensions ("open in", "send selection to a chat", "show this chat's
  changes"). (b) An embedded lightweight CodeMirror 6 viewer/editor for reading, small edits and
  review. No full editor."*
- **X26:** *"F2 carries a non-goals list (no LSP, no debugger, no project-wide refactors, no
  editor extension API, no terminal multiplexing beyond chats and shells). Public positioning is
  **"the agent IDE"**: the environment where your agents work, not a code editor."*
- **W2**, in part: *"\"The agent IDE\" stays the category"*.
- **R4:** *"Diff computed by git in the Rust core; rendered with CodeMirror 6 + its merge view,
  the same component as the light editor (one editor stack, TS only)."*
- **R7:** *"The right side of the diff is editable while the chat is idle (warning otherwise);
  save uses the stale-check with Reload/Overwrite; the edit is recorded as a human edit in the
  chat's history."*
- **R12:** *"Non-goals kept (X26): no LSP, debugger, refactors or editor plugins; deep work opens
  in the user's editor at the same file:line via the editor integrations."*
- **W7**, in part: the Next gate names *"a URL-scheme \"open in editor at line\" (vscode://,
  zed://, idea://, $EDITOR +line)"*.
- **E1**, in part: *"Every adapter seam is an extension point: […] review decorations, policy
  packs, palette commands, MCP tools exposed to chats."*
- **E5:** *"API stability: protocol versions per capability (ADR 0053), N−1 supported for 6
  months, SDKs generated from one schema (TS, Rust, Python, Go), a conformance kit (N20), a public
  API changelog."*
- **E6:** *"UI is blocks by default. A capability-gated **webview** (sandboxed iframe, strict CSP,
  message API) is approved separately for rich UIs."*
- **V7**, in part: *"client scopes local-ui, terminal, fleet-mcp, remote-link, with vault values
  never reachable from remote-link."* **V22a**, in part: *"a sixth client scope `chat`, and chats
  are denied `charterd.sock`"*.
- **V16a:** *"The terminal, fleet-MCP and approval client scopes (FD-27) need a credential the
  chat sandbox cannot read, and `charterd` refuses them to connections from a chat's process tree;
  an agent can never answer its own asks or approve its own secret requests."*
- **V11**, in part: *"per-piece setup, run and archive scripts and a port range per piece; a
  preview pane"*. **N59**, **N60**: the test-results, log, ports and preview views.
- **X22:** *"Every new ruling must name which concept it belongs to."*

It builds on [ADR 0061](0061-a-curation-action-is-a-chat-with-its-prompt-typed-and-never-sent.md)
(a prompt typed and never sent), [ADR 0062](0062-a-harness-started-by-hand-in-a-shell-tab-is-warned-about-by-the-command-that-started-it.md)
(shell tabs), [ADR 0068](0068-a-chat-lives-in-charterd-and-the-app-is-its-client.md) (`charterd`,
its protocols and scopes, which names *"ED-1 for the editor protocol"* as a record that builds on
it), [ADR 0069](0069-every-store-charter-writes-is-in-one-of-four-tiers.md) (tiers),
[ADR 0072](0072-charter-has-five-concepts-and-every-other-word-belongs-to-one-of-them.md) (the five
concepts), [ADR 0075](0075-an-audit-entry-is-metadata-in-a-store-of-its-own-and-telemetry-never-reads-it.md)
(the audit), [ADR 0076](0076-a-run-moves-only-by-a-named-cause-and-a-chats-state-is-read-from-its-runs.md)
(a run's states), [ADR 0078](0078-a-runner-is-charterd-behind-a-connector-and-charters-own-keys-say-who-is-on-the-link.md)
(runners and the link) and [ADR 0080](0080-a-level-3-chat-is-an-acp-client-session-and-its-acp-adapter-program-is-the-users.md)
(level-3 chats). It **amends ADR 0061, ADR 0068 and ADR 0078**, each in a section of its own
below. RC-1 (the Review & code ADR), ED-2 (the editor protocol) and ED-3 to ED-5 (the editor
integrations) build on it. Its concept is **Workspace**: the light editor and the editor
integrations are views of a workspace's repos and its chats' branches.

## Where charter is today

charter has no editor of any kind. A persona's `persona.md` opens in whatever app the operating
system opens a Markdown file with (`app/src-tauri/src/personas.rs`, through the opener plugin),
and `docs/spec.md` says so: *"because charter has no editor for it"*. There is no CodeMirror in
the app, no diff view, and no way to open a file at a line in another editor. No editor has a
charter extension, and `charterd`'s scopes (ADR 0068 §5) have none meant for one.

charter hosts two kinds of terminal: a chat's harness, and a shell tab's shell (ADR 0062). The
window's panes split to hold tabs side by side (the centre region in `docs/spec.md`);
nothing splits inside a terminal.

## What the rulings already settle

| ED-1 asks | Settled by |
|---|---|
| The positioning, "the agent IDE" | X26 and W2, quoted above. "The agent IDE" stays the public category name, and this record does not re-word it |
| The five non-goals | X26, and R12 repeats them for Review & code. §2 below draws each one's line; it adds none and drops none |
| An in-app editor at all | F2(b): one CodeMirror 6 component *"for reading, small edits and review"*. R4: the diff view is the same component |
| Editing beside a running agent | R7: editable while the chat is idle, a warning otherwise, the stale check, and the edit recorded as a human's |
| Where deep work goes | R12: *"opens in the user's editor at the same file:line"*. W7: the URL-scheme hand-off is in RC-lite (RC-20) |
| What the editor integrations do | F2(a): *"open in"*, *"send selection to a chat"*, *"show this chat's changes"* |
| The protocol's stability promise | E5 and ADR 0068 §4: versioned, N−1 for six months, a conformance kit |

What is left for this record is four things: **a test that decides whether a feature is
charter's**, **where each non-goal's line runs**, **the editor protocol** (what it is part of,
who may speak it and what it may do), and **the words**.

## The decision

**charter is where agents work: it starts them, shows what they are doing, and is where their
work is reviewed, steered and handed on. Writing code by hand is your editor's job. charter's one
editor, the light editor, is for reading a file, making a small edit in it, and reviewing a diff;
deep work leaves for your editor at the same file and line. Your editor reaches charter through
an editor integration that speaks the editor protocol, a narrow part of `charterd`'s public
session protocol under a scope of its own. It can bring a chat forward in charter's window and
type a selection into an idle chat's prompt; it can never send it, start a chat or answer one.**

### 1. The test: is it about agents' work?

**A feature is charter's when it serves agents' work: starting a chat, watching it, reviewing
what it did, steering it, or handing it on. A feature that serves writing code by hand is your
editor's.** X26's list names the five places where the line has already been drawn. A proposed
feature that is on none of them is decided by this test, in its ticket, which says which of the
five concepts owns it (X22, ST9).

What charter builds for code, all of it on the agents' side of the test:

| What | Why it is charter's | Ticket |
|---|---|---|
| The light editor: open any file of a chat's branch, read it, make a small edit | reviewing and steering an agent's work in place | RC-5, RC-10 |
| The review view, one diff engine | reviewing what an agent did | RC-2, RC-4 |
| Git basics: history, blame with the chat and persona that wrote a line, hunk staging | reviewing, and saying which agent did what | RC-14 to RC-16 |
| Test results, logs, ports and a preview, per branch | watching what an agent's change does | ED-8 to ED-10 |
| Open in your editor at file:line | handing deep work on | RC-20, RC-19 |
| Editor integrations | reaching the agents from where the code is written | ED-2 to ED-5 |

**A small edit** is one a person makes in one file, by typing, while reading or reviewing: a
typo, a line an agent got wrong, a comment. The light editor does not bound its size. What makes
an edit not small is being about more than one file at once, and §2 rules those out.

### 2. The non-goals, and where each line runs

Each non-goal is X26's. The table says what it rules out and what it leaves in, so that a ticket
near the line can be read against it.

| Non-goal (X26) | charter never builds | Still charter's |
|---|---|---|
| **No LSP** | a language server run for a person: completion, hover, go to definition, find references, diagnostics, code actions, format on save | syntax highlighting from the light editor's own grammars; search within a file; a harness's own language servers inside its chat, which are the harness's; a code index for agents over MCP (N51) |
| **No debugger** | breakpoints, stepping, variable inspection, a debug adapter | test results and logs read from a branch's runs (ED-8, ED-9); a preview of what a branch serves (ED-10) |
| **No repo-wide refactors** (X26's *"project-wide"*) | an operation that edits more than one file at once from charter's window: rename a symbol, replace across files, move a file and fix its imports, apply a fix everywhere | asking an agent to do it, and reviewing the diff; R7's edit of one file; hunk staging (RC-16) |
| **No editor extension API** | code from an extension, a pack or a persona running inside the light editor: its own language modes, commands, keymaps, panels or decorations drawn by its code | E1's extension points: review decorations and palette commands are **data** an extension returns, drawn by charter (E6's blocks); an approved webview (E6) is a view tab of its own, never inside the light editor. charter's own editor integrations are extensions **of other editors**, and are not this |
| **No terminal multiplexing beyond chats and shells** | a third kind of terminal: panes inside one terminal, named terminal sessions to detach and reattach, a send-keys or scripting API, driving tmux or screen | a chat's terminal; a shell tab; split panes that hold tabs; `charter attach` to a chat (FD-21) |

**"Repo-wide", not "project-wide".** X26 says *"project-wide"* in an editor's sense of the word:
the folder an editor has open. In charter a project is charter's own (V23a, ADR 0072 §1), and a
refactor never spans one. This record and every ticket after it say **repo-wide**, which is what
X26 meant.

**A `run` script stays inside the two kinds.** Where V11's per-branch `run` entries (ED-14) show
their output, a shell tab or a read-only log view (ED-9), is ED-14's to decide. This record
only rules out a third kind of terminal for them.

### 3. Your editor

**Your editor** is the editor the operator already uses: VS Code, Zed, a JetBrains IDE, or
whatever `$VISUAL` or `$EDITOR` names. charter never replaces it, and two things connect the two:

- **Open in your editor at file:line** (RC-20, W7). From any file and line charter shows (the
  light editor, a diff, a review comment, a session record), charter hands the file and line to
  your editor through its URL scheme (`vscode://`, `zed://`, `idea://`) or `$VISUAL`/`$EDITOR`
  with `+line`. It needs nothing installed in the editor. Which editor is a setting of the
  operator's on this machine, and RC-20 declares it (ST1).
- **An editor integration** (ED-3 to ED-5): charter's own extension for VS Code (and Open VSX),
  charter's own extension for Zed, and charter's own plugin for JetBrains IDEs. Each is a thin
  client of the editor protocol (§4). It carries F2(a)'s three commands and nothing else:
  - **Open in charter**: the chat, or else the workspace, that the current file belongs to, shown
    in charter's window.
  - **Send selection to a chat**: the selected text and its `path:line` range, typed into an idle
    chat's prompt and never sent (subject to ruling 1). With no idle chat, the integration reveals
    the workspace, so the person starts a chat in charter, and offers to copy the selection to the
    clipboard.
  - **Show this chat's changes**: the files a chat's branches changed, listed in the editor and
    diffed there with the editor's own diff view.

Where the integrations live and in which languages is ruling 2.

### 4. The editor protocol

**The editor protocol is not a third protocol.** ADR 0068 §4 has two: the public session
protocol and the private UI RPC. The editor protocol is four commands of the session protocol
(L0 in ADR 0078 §1), versioned with it, published with it (LV-2a), and held to E5's N−1 promise by
the same conformance kit. An editor integration is published separately from the app and lags
it, which is exactly the case the session protocol's promise exists for; the private UI RPC makes
no such promise.

**The four commands** (ED-2 fixes their shape):

| Command | What it answers or does |
|---|---|
| `resolve` a path | the project, the workspace, and the chats whose branch folder holds the path, each with its number, title, harness product name and whether `place` would take it now; or *not in any project charter has open* |
| `reveal` | asks the window to bring forward a chat's tab, a workspace, or a Review tab of a chat's changes (RC-4) |
| `place` a selection | types the text and its `path:line` range into an existing chat's prompt, never sent, under the rules below |
| `changes` of a chat | per branch: the repo's path, the base and head refs, and each changed path with its A/M/D/R mark, from RC-2's diff engine. The editor reads the contents from git itself |

**The editor scope.** An editor integration connects to `charterd.sock` with a credential for a
new scope, `editor`, minted like the other human scopes' (ADR 0068 §5). The four commands are
this scope's alone. See *ADR 0068, amended*. The scope is narrow by construction:

- **It can bring something forward and type text a person then sends. It can make no agent
  act.** No `start`, `stop`, `write`, `attach` or `answer`; no new chat; no terminal bytes,
  transcripts, memory, vault values or settings. Every prompt that reaches an agent from an editor
  was sent by a person, in charter's window, and every chat was started by one.
- **`reveal` never moves keyboard focus into a terminal.** It brings the window and the tab
  forward with focus on the tab in its strip, so a key pressed while switching windows reaches no
  harness. The person clicks into the chat, or presses a key to enter it, before typing.
- **What `resolve` and `changes` show.** Chat numbers and titles, workspace and branch names,
  refs, and changed paths: metadata, never contents. Any other extension in the same editor,
  running as the operator, can read them through the credential. That is acceptable because the
  same process can already read all of it, and more, from the disk: the project is a git repo the
  operator's processes can read, and so are the repos and branches the paths name. The scope adds
  no content and no reach a process running as the operator lacks.

**`place`: the race with an ask, closed in `charterd`.** A paste into a terminal that is showing
an ask could be read as its answer. So:

- **`charterd` does the check and the write as one step**, under the run's state lock, so no move
  of the run (ADR 0076 §2) comes between them. The window does not type it.
- **It types only into a run in `input-required (ready)` or `input-required (turn-ended)`.**
  `starting`, `working`, `input-required (asked)`, `paused`, `queued`, `hibernated` and every end
  are refused, with a sentence naming the state. A `hibernated` chat is refused, not woken. The
  text is never held to be typed later.
- **It types by ADR 0061's paste, in the harnesses ADR 0061 can type into**
  (`Harness::ready_to_type`): one bracketed paste, control characters taken out, no line end. It
  never types a line end, and never a paste of one character: a selection that is empty or one
  character long once cleaned is refused.
- **The residual risk.** The lock orders charter's view of the run, not the harness's screen. A
  harness that opens a dialog of its own between turns, before it reports it, could receive the
  paste while the dialog is open. Because the paste is bracketed, holds no line end and is never a
  lone character, a dialog that reads the paste as key presses would still need to take a
  bracketed multi-character paste as a choice. TS1 measures, per harness, that a bracketed paste
  into its dialogs chooses nothing; a harness that fails it is refused `place`, as ADR 0061 refuses
  a harness it cannot type into safely.
- **A level-3 chat is refused, for now.** ADR 0080 gives a level-3 chat a transcript view tab and
  defines no prompt input in it. `place` takes a level-3 chat once that input exists, by the same
  rules, through the host's own delivery and not a paste.
- **`place` needs a connected window.** With no `local-ui` client connected, `charterd` refuses
  `reveal` and `place` with *charter's window is not open*. The integration offers to open charter
  the way the operating system opens an app; `charterd` never starts the window.
- **A selection past 64 KiB is sent as its `path:line` range alone**, and the typed text says the
  selection was left out.

**On a runner.** An editor whose extension host runs elsewhere (a remote host, a container) talks
to the `charterd` on that machine, if there is one. The four commands never cross the link (see
*ADR 0078, amended*).

**No inbound URL scheme.** charter registers no `charter://` handler that an editor, or anything
else, could call to reach a chat. A URL scheme can be invoked by a web page in a browser, and a
page must never be able to type into a chat's prompt.

### 5. The audit

Per ADR 0075 §4, every action is registered in AU-2's registry. **One action is this record's:
`editor.selection.placed`**, with the chat as its target and the line count as its `meta`, and
never the text or the path. A refused `place` is the same action with a result of refused and the
state that refused it. `reveal`, `resolve` and `changes` read or show and change nothing, so they
are not audit actions. A prompt the operator then sends is audited as any prompt is.

### 6. Tiers

**This record adds no store.** The `editor` credential is one more file in a store ADR 0069
already tiers: its §6 table, row 66, *"`<config>/charterd/` credentials"*, Machine, transient,
device-bound (ADR 0068). `docs/plane-format.md` had no row for it, so this PR adds one, citing
all three records:

| Path | Tier | What it holds | Written by |
|---|---|---|---|
| `<config>/charterd/<scope>` | Machine, device-bound, transient | **decided, not yet written** (ADR 0068, ADR 0069, ADR 0081). One credential per human client scope (`local-ui`, `terminal`, `fleet-mcp`, `approval` and `editor`), minted fresh at each start of `charterd`. Chats are denied the directory | `charterd` (FD-6, FD-27) |

Everything else this record touches is in other records' stores or in none:

- the audit action goes to the audit store (ADR 0075): **Machine**, device-bound;
- the editor RC-20 opens is a setting ST1's registry declares: **Machine**, syncable, and RC-20's
  row;
- an editor integration's own settings are the editor's, written by the editor: no charter store,
  **None**;
- the light editor's edits are the operator's files in a branch folder: **None**, as every piece
  is (ADR 0027).

## ADR 0061, amended

ADR 0061 types a prompt into a **new** chat once, at its first `SessionStart`, and never late. It
gains a second case: **an editor's `place` types into a running chat**, under §4's rules: only in
`input-required (ready)` or `input-required (turn-ended)`, as one step with the check under the
run's state lock in `charterd`, never a line end, never one character, never held to be typed
later. ADR 0061's rule stands for both cases: the text is typed and **never sent**. Its paste, its
cleaning and `Harness::ready_to_type` are the same for both.

The rest of ADR 0061 stands.

## ADR 0068, amended

**§4, the session protocol.** It gains the editor protocol's four commands (`resolve`, `reveal`,
`place`, `changes`). The count of *"about fifteen commands"* grows by four. The UI RPC is not
changed.

**§5, the scopes.** The table gains a row:

| Scope | Who connects | Where | Its credential | What it may do |
|---|---|---|---|---|
| `editor` | an editor integration in the operator's editor (ED-3 to ED-5) | `charterd.sock` | the `editor` credential | the editor protocol only (ADR 0081 §4): resolve a path, ask the window to reveal a chat, workspace or review without focusing a terminal, type a selection into an idle chat's prompt without sending it, and list a chat's changed files |

- **The four commands are the `editor` scope's alone.** `local-ui`, `terminal`, `fleet-mcp`,
  `approval`, `remote-link` and `chat` are refused them. The window has its own UI RPC and needs
  none of them; no other scope has a use for them.
- **It is a human scope for V16a's check.** A connection from a chat's process tree is refused it,
  and the sandbox denies chats its credential, as for `terminal`, `fleet-mcp` and `approval`.
- **It is refused vault values, settings, answers, and every command that starts, stops, writes to
  or attaches to a chat.** `place` is not `write`: `charterd` does the write itself, under §4's
  rules, and the scope never holds a terminal stream.
- **Its credential guards against chats, not against the operator's own programs.** Every scope's
  credential is a `0600` file the operator's processes can read. What keeps a chat out is the
  sandbox's denial of the directory. Another extension in the same editor, running as the
  operator, can read it, which is why the scope can make no agent act.

The rest of ADR 0068 stands.

## ADR 0078, amended

**§1, L0.** The session protocol is no longer *"unchanged"* from ADR 0068 §4: it carries the four
editor commands. **§4's table gains a row: the editor protocol's four commands are sent down the
link from no scope.** They act on one host's own window and chats; a `remote-link` connection is
refused them on the runner, and an editor on a runner speaks to that runner's own host. The rest
of ADR 0078 stands.

## What changes where

The code does not change with this record.

| Where | What changes |
|---|---|
| `charterd` | ED-2: the `editor` scope and the four commands, with FD-6's authentication and FD-27's scope checks; `place`'s check and write under the run's state lock |
| The session protocol crate | LV-2a: the four commands, in the schema and the conformance kit |
| Editor integrations | ED-3, ED-4, ED-5: thin clients of the protocol, wherever ruling 2 puts them |
| The window | ED-2: `reveal` with focus on the tab, never a terminal |
| Harness checks | TS1: per harness, a bracketed paste into its dialogs chooses nothing |
| The light editor and review | RC-1 and RC-5: one CodeMirror 6 component with charter's own fixed grammars and no extension code inside it |
| Open in your editor | RC-20: the URL-scheme hand-off and its setting |
| Audit | AU-2 registers `editor.selection.placed` |
| `docs/plane-format.md` | The `<config>/charterd/<scope>` row (in this PR) |
| `CONTEXT.md` | Gains **Light editor**, **Your editor**, **Editor integration** and **Editor protocol**, each with its concept (in this PR) |
| Tickets | A ticket near a non-goal cites §2's line, and uses **repo-wide** for X26's *"project-wide"* |

## What this costs

- **An operator who wants to stay in one window cannot.** Completion, go to definition, a
  debugger and a rename across files are one hand-off away, not in charter. The hand-off at
  file:line (RC-20) and the integrations are what pay for it.
- **Sending a selection takes more steps.** The text lands in the chat's prompt unsent, the tab
  comes forward without terminal focus, and the person clicks in and presses Enter. A busy chat
  refuses it, and with no idle chat the person starts one in charter.
- **No `place` at level 3** until a level-3 chat's view tab has a prompt input.
- **Up to three integrations in three languages and three marketplaces.** Each is thin, but each
  has its editor's release cadence, and N−1 means the protocol cannot drop a command for six months
  after the integrations stop using it.
- **A language the light editor has no grammar for shows as plain text.** Only a charter release
  adds one.
- **The `editor` credential is readable by every extension in the same editor.** The scope's
  limits are the defence, not the credential.

## What was rejected

- **A full editor in charter** (Monaco, or a CodeMirror with language servers). F2 and X26 rule it
  out; it would compete with editors the operator already has, and lose.
- **A separate editor protocol beside the session protocol.** A third protocol would need its own
  versioning, promise and conformance kit, for four commands.
- **Giving editor integrations the `terminal` scope.** It can write to a chat's terminal, which is
  sending a prompt and answering an ask; that is too much for code that shares a process with the
  editor's other extensions.
- **Sending the selection** instead of typing it. It would let any program that can read the
  credential drive an agent.
- **`place` into a new chat.** Opening a chat starts an agent's program; the person starts it in
  charter instead.
- **The window doing the typing.** The check and the write would be two steps across a process
  boundary, with room for an ask between them.
- **Cancelling a paste an ask raced, by clearing the prompt line.** Clearing is more keys sent to a
  terminal that may be showing a dialog. A bracketed paste with no line end, never one character,
  is the safer rule.
- **Queueing a selection for a busy chat** and typing it at the next turn boundary. It could land
  after the operator has typed something else, the reason ADR 0061 never types late.
- **An inbound `charter://` URL scheme** for "open in charter". Reachable from a web page (§4).
- **Language grammars as extension data.** CodeMirror's grammars are code, so an extension
  grammar is extension code inside the light editor, which X26 rules out.

## Decided in drafting

Each of these is inside the rulings above and easy to change later, so it is decided here, with
its reason:

1. **The test in §1 (agents' work, or writing code by hand).** X26 names five lines; tickets will
   meet features on none of them, and a test is cheaper than a ruling each time.
2. **"Repo-wide" replaces X26's "project-wide" in every document after this one.** Since V23a,
   "project" is charter's word for its own repo, and a refactor never spans one.
3. **The editor protocol is four commands of the public session protocol**, not a protocol of its
   own and not the private UI RPC. The integrations ship apart from the app, which needs the public
   protocol's N−1 promise; a third protocol would duplicate it.
4. **`charterd` does `place`'s check and write as one step under the run's state lock, only in
   `input-required (ready|turn-ended)`, refusing `hibernated` and level 3, never a line end or one
   character.** It closes the race with an ask as far as charter can see it, and TS1 measures the
   rest.
5. **`reveal` brings the tab forward with focus off the terminal.** A key pressed while switching
   windows must reach no harness.
6. **`reveal` and `place` need a connected window, and `charterd` never starts one.** The window is
   where a person sees what the request did.
7. **No inbound URL scheme.** A web page can call one.
8. **Review decorations and palette commands from extensions are data, drawn by charter**; a
   webview is a view tab of its own. It reads E1 and E6 together under X26's "no editor extension
   API".
9. **The light editor's grammars are a fixed set charter ships.** An extension grammar is code
   (see *What was rejected*).
10. **A selection past 64 KiB is sent as its range alone.** A prompt that size is an attachment,
    and the agent can read the file.
11. **One audit action, `editor.selection.placed`, without text or path.** Text that reaches a
    prompt from outside the window should be traceable, and ADR 0075 never records content.
12. **The plane-format row covers every scope's credential**, filling the row ADR 0068 and ADR
    0069 left out of that document, rather than naming the `editor` one alone.

## Later decisions

- **Where the editor integrations are published.** A marketplace publisher id cannot be renamed,
  so it is a public commitment. ED-3 to ED-5 are Later; the publisher is decided when the first of
  them is scheduled, after the repo move (GH1).
- **An editor on a runner reaching the desktop's window.** An editor attached to a runner
  (Remote-SSH, a devcontainer) has its integration on the runner. Reaching the desktop's window
  from there crosses the link the other way from ADR 0078 §4, and RR's tickets decide it.
- **Where a `run` script's output goes**, a shell tab or the log view: ED-14 (§2).
- **Live updates in the editor.** The four commands answer when asked. A subscription (a chat's
  changes as they happen, a needs-you count in the editor's status bar) is ED-2's to propose, as a
  fifth command under the same scope limits.
- **The JetBrains plugin's client.** E5's generated SDKs name TypeScript, Rust, Python and Go, not
  Kotlin or Java. ED-5 picks between a generated Java client and a hand-written one under the
  conformance kit.

## For the operator's ruling

1. **A seventh client scope, `editor`, and "send selection" typed, never sent** (a security
   boundary, extending V7's and V22a's scopes, and narrowing F2(a)'s *"send selection to a
   chat"*). The scope may resolve a path, ask the window to reveal a chat, workspace or review
   without focusing a terminal, type a selection into an idle existing chat's prompt under §4's
   rules without sending it, and list a chat's changed files. It may not start a chat, send a
   prompt, answer an ask, or read a value or a setting, and chats are denied it as they are the
   other human scopes. *Recommend yes.* The alternative, sending the selection, would let any
   extension in the operator's editor drive an agent.
2. **Where the editor integrations live, and in which languages** (the operator's rule on
   languages, *"final app should not use python, fully clean implementation in rust — no need to
   mix languages"*, and a lasting CI cost). Each integration must be written in its editor's
   language: TypeScript for VS Code, Rust for Zed, Kotlin or Java for JetBrains. The options:
   - **(a) A separate repo per integration.** This repo stays as it is; each integration pins a
     protocol version and runs the conformance kit in its own CI. Protocol changes span two repos.
   - **(b) In this repo, under `editors/`, with the rule read as being about the shipped app.**
     The protocol and its clients change in one pull request; CI gains a toolchain per editor.
   - **(c) VS Code only for now, JetBrains later.** Build ED-3 alone in this repo (TypeScript and
     its toolchain are already here for the window), publish to the VS Code Marketplace and Open
     VSX, and decide Zed and JetBrains, and where they live, when each is scheduled.

   *Recommend (c)*: it adds no language and no toolchain this repo lacks, one extension reaches
   VS Code and the editors that install from Open VSX, and Kotlin's build and CI cost waits until
   a JetBrains integration is scheduled.
