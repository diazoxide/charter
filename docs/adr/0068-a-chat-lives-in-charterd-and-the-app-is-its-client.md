# A chat lives in `purlisd`, and the app is its client

**Accepted 2026-09-30** by the operator (ruling V22a), drafted for program-map ticket FD-2. It
follows five of the operator's rulings:

- **Q2:** a `purlisd` session host, always on and supervised by the app, with per-connection
  authentication from day one.
- **V7:** hardening that host. A public session protocol split from the UI's private one,
  client scopes, any ordered byte stream as transport, descriptor handoff on upgrade, a supervisor
  with a crash-loop breaker, and a hook spool.
- **V9:** a remote runner is `purlisd` on another machine, reached through a connector command
  such as `ssh`.
- **V16:** agents never hold human powers.
- **X24:** the host's lifecycle. It is a login item only when the user has a trigger or a paired
  device, and otherwise it exits with the app.

It **amends [ADR 0025](0025-charter-is-rebuilt-as-a-desktop-app-on-a-rust-core.md)'s "No
daemons"**. It builds on [ADR 0066](0066-a-chat-is-a-ulid-a-run-is-a-stretch-of-its-conversation-and-a-device-is-random.md)
(chat, run and device identity) and keeps to [ADR 0067](0067-a-chat-runs-in-a-sandbox-charter-compiles-for-its-harness.md)
(the chat sandbox). The tickets that build the host (FD-5 and FD-6, FD-17, FD-26 to FD-30), the
runner records (RR-13, RR-14) and two later ADRs (LW-1 for live sessions, ED-1 for the editor
protocol) all build on it.

## Where purlis is today

ADR 0025 decided: *"No daemons. Every session is a child of the app. Quitting ends them all."* The
code does exactly that, in four places:

- **A session ends when it is dropped.** `session.rs` says so in its first paragraph: *"there is no
  daemon, and nothing outlives the app"*. `impl Drop for Session` hangs up the program's process
  group, waits 500 ms, and kills what is left.
- **The app holds every session.** `Sessions` (`app/src-tauri/src/sessions.rs`) keeps them in one
  map in the app's own process. `Sessions::end_all` is what quitting calls.
- **An update ends every agent.** `install_update` (`updates.rs`) calls
  `Planes::let_go_of_all_to_update`, which writes each plane's `reopen.json` and ends the chats. The
  new version then offers **Reopen all**, which brings each chat back through its harness's own
  resume. A turn in flight is lost, and the window asks first for that reason.
- **The app owns the hook channel.** `hooks::socket_for` binds `.charter/app/hooks.sock` in each
  plane (or a per-user fallback when that path is too long). The per-chat tokens that #535 added
  live in the app's memory only (`ChatTokens`).

Track 02 of the research ranks this as the worst reliability property at scale: *"App lifetime
equals agent lifetime. A crash, a quit or an update ends every agent."* At 50 chats, one crash of
the app ends 50 conversations mid-turn. The fix it names is the tmux-server, VS Code Server and
Zed-remote pattern: a session-host process that owns the terminals, which the window attaches to.
Q2 ruled for it, and V7 hardened it. This record is the design those rulings asked for.

## The decision

**The sessions move out of the app into `purlisd`, a session host that is the same `purlis`
binary run as `purlis serve`. The app starts it and is its first client. An update of the app
ends no agent, and a crash of the app ends none that the app comes back for within the grace
period (section 2). An upgrade of `purlisd` hands its live terminals to the new version over
`SCM_RIGHTS`, with drain-and-resume as the fallback. Every connection is authenticated from the
first release, and a connection from a chat's process tree can never hold a human power.**

### 1. One binary, one host per OS user per device

- **`purlisd` is `purlis serve`.** It is the `purlis` binary the app already ships beside itself
  (`crate::charter_binary`, the one *Install `purlis` command in PATH* links). There is no second
  executable to sign, notarize, version or update. `purlisd` is the name for the process, never a
  separate file.
- **One host per OS user per device, serving every plane that user opens.** ADR 0066 anticipated
  this: FD-6 (per-connection authentication) re-keys the token map on the chat id *"when `charterd`
  holds several planes' chats"*. One host means one event-log writer per device, which is what ADR
  0066's `seq` needs (*"assigned by the single writer on the device"*). A second `purlis serve`
  for the same user finds the first one's socket answering, and exits with a sentence saying so.
  The one exception is a runner changing version (section 8).
- **Its socket is per user, in a private directory:** `$XDG_RUNTIME_DIR/charter/charterd.sock` on
  Linux, and the per-user `TMPDIR` on macOS. This is the same choice `hooks::private_dir` makes for
  the hook fallback, for the same reasons. The directory is created at `0700` as it is made, the way
  `hookwire::private_directory` already does it.

**What `purlisd` owns is decided by one rule: it holds what must outlive the window, and what a
chat must never reach. Nothing else moves.**

| Moves into `purlisd` | Why |
|---|---|
| Every session: the PTY, its engine and its views (`Sessions`) | They must outlive the window (Q2) |
| The hook channels and the per-chat tokens (`hooks.rs`, `ChatTokens`) | A hook fires whether or not a window is open |
| The board: each chat's state as its hooks report it | It is derived from the hooks, so it lives where they arrive |
| `planewatch` and auto-save (`autosave.rs`) | Auto-save (ADR 0051) must keep saving a chat's work while the window is gone |
| The event log, as its only writer (ADR 0066; FD-9, the host's event log; FD-24, subscribing from a cursor) | One writer per device |
| **Every read of a vault's values**, and the V15 approval gate (V15: manual approval of secrets; V16b: vault enforcement in the host) | There is one enforced path to a secret's value. See below |
| The session records `purlisd` writes for a chat that ends | The chat can end with no window open |

**Everything else stays in the app.** The window, the plane reads that fill its panels
(`plane_sidebar`, the memory and todo commands), the palette, and **writing settings** all keep
calling `charter-core` from the app's process, as they do now. They are reads and writes of files
in the plane and the machine store, which the operator's own process may already touch, and
moving them would put every panel behind a socket for no gain. The host exposes as commands only
the few settings it acts on itself, such as a chat's sandbox opt-out (ADR 0067 §7), and only on
`local-ui` (section 5).

**One path to a secret's value.** The host is the only process that opens a vault's storage for its
values. The app's vault view (`vaults.rs`) keeps listing names and metadata from the plane, but
**reveal, copy and every other read of a value go through `local-ui`** to the host, which applies
the vault's reveal setting (V15's OS authentication) and records the read. A chat reaches a value
only through the host's gate on the `chat` scope (section 5), and its sandbox denies the storage
itself (ADR 0067 §5.1). A value never reaches any other scope.

### 2. The lifecycle (X24; FD-17 builds it)

X24's ruling: `purlisd` is a login item only when the user has at least one trigger or paired
remote device, with a tray presence and a one-click stop, and otherwise it exits with the app. This
record reads that ruling against the three ways the app can go away, because they are not the same:

| The app… | What `purlisd` does |
|---|---|
| **quits** (the operator chose Quit) | Ends every session, as today, and exits. `reopen.json` is written first, exactly as `let_go_of_every_plane` writes it now. ADR 0025's promise is kept: quitting ends the agents |
| **restarts to update** | Keeps every session. The app tells the host it is restarting before it goes, and the new app reattaches. **Reopen all** stops being the answer for an app update, since nothing ended (FD-7) |
| **crashes**, or is killed | Keeps every session for a **grace period** (ruled, V22a: 10 minutes). An app that starts within it reattaches, and nothing is lost. When the grace period passes with no app, `purlisd` ends the sessions as a quit would, writing `reopen.json` first, and exits. Agents never keep running unseen for longer than that |

When the user has a trigger or a paired device (FD-17), none of the three ends the sessions: the
host keeps running as a login item and shows in the tray, where one click stops it. **Stop** from
the tray is a quit.

The app tells `purlisd` which case it is by saying so before it goes (`quit` or `restarting`). A
connection that closes without either is a crash. A quit that fails to say so is treated the safer
way, as a crash whose sessions are ended when the grace period passes.

**During the grace period the host is supervised as it is at any other time** (section 3): by the
operating system's service manager where there is one, and by nothing where the app was the only
supervisor. That second case needs nothing more. A crash of the host ends its chats whether or not
anything restarts it (section 3), so a restart in the grace period would only do the crash
accounting, and the app's next launch starts a host that does the same accounting from the journal.

On battery, `purlisd` follows V8's idle budget (V8: performance budgets per class of machine): its
timers and polls are driven by the file watcher and pause while no window is showing them. It never
throttles a running chat's terminal.

### 3. Supervision, and what a crash costs (FD-29: supervisor, crash recovery and a chaos test)

**The operating system's service manager supervises the host wherever one is available**, with its
own restart policy and its own restart limit. purlis writes no supervisor for those cases:

- **Linux with a systemd user session:** a systemd user unit. When the host is a login item (FD-17)
  it is an installed, enabled unit. Otherwise the app starts it as a transient unit
  (`systemd-run --user`), which needs no file installed and goes away when the host exits. Both have
  `Restart=on-failure`, and the crash-loop breaker is systemd's own `StartLimitBurst` and
  `StartLimitIntervalSec`.
- **macOS, when the host is a login item:** a launchd agent with `KeepAlive` on unsuccessful exit,
  and launchd's own throttle.
- **macOS when it is not a login item, Windows, and Linux without a systemd user session:** there
  is no OS supervisor purlis may use without registering a background item, which X24 rules out
  when there is no trigger or device. **Only here does the app supervise**: it starts the host if
  none is answering and starts it again after an unexpected exit, and it has the one piece of
  supervision code purlis writes, a breaker that stops after **three unexpected exits within five
  minutes**, the same limits the units above are given.

When a breaker trips, whichever one it is, the app says the host is failing, shows the last lines of
its log (FD-8: `tracing` to a rotating file), and offers **Try again**. Until the host starts, the
app draws the plane but runs no chat, and says why. There is no in-process fallback (V22a).

**What a crash of the host costs.** When `purlisd` dies, its PTY masters close, and the kernel
hangs up every chat's terminal. The harnesses end. That is the honest cost of one host process, and
V7 accepts it. What purlis owes after it is FD-29's: the host journals each run's conversation id
(ADR 0066's `conversation` attribute) as it learns it, so the next host marks every run that was
`working` as `failed(host-crash)` and offers a one-click resume through the harness's own resume. A
chat on a harness-native host (FD-20: attaching to a harness's own server) is re-attached, not
resumed.

**A `kill -9` chaos test runs in CI** on macOS and Linux: kill `purlisd` with 20 chats open, and
every chat is back within 30 seconds or offered with its reason (FD-29's acceptance).

### 4. Two protocols over any ordered byte stream (FD-4: the transport; FD-26: the split)

- **The session protocol is small, public and versioned.** It carries events and about fifteen
  commands: list, attach with a snapshot, write, resize, answer, stop, start, and
  `subscribe(since)` from ADR 0066's cursor. It carries the compatibility promise (E5: versioned
  public interfaces), so a client and a host one version apart (N−1) always talk. LV-2a publishes it
  as its own MIT crate, ungated and free. `purlis attach` (FD-21), the fleet MCP (HP-20: a fleet
  view for outside agents) and a runner's link (V9) are its clients.
- **The session protocol is proved from outside the host.** LV-2a's conformance tests run a stub
  remote client **in a separate process** against a real `purlis serve`: it lists, snapshots,
  answers and stops, over a stream that is not the app's (V7). A protocol only the app has ever
  spoken is not yet public.
- **The UI RPC is private.** It is what the window needs beyond the session protocol, generated
  from `ipc_commands.rs` with a typed TypeScript client (FD-4). It ships with the app, changes with
  the app, and is excluded from the compatibility tests. The app and the host it starts are the same
  version except across an upgrade (section 7), which the session protocol alone carries.
- **The transport is any ordered byte stream:** a local socket, or a child's stdio (`ssh`,
  `docker exec`, `kubectl exec`). Nothing in either protocol assumes a local socket. That is what
  lets a runner be the same host behind a connector (section 8).
- **Many chats share one stream, each with its own flow control.** Every chat's terminal is a
  stream with per-stream credit, and a priority control lane carries commands and events past a busy
  terminal. Terminal frames are raw bytes, never JSON strings: today's `Channel<String>` escapes
  every ESC byte into six. FD-4 picks the framing, and it uses an existing multiplexer with those
  properties rather than a new one.
- **A view's queue is bounded in bytes.** Today `Session::attach` gives each view an unbounded
  `mpsc::channel`, so a pane that stops reading holds memory without limit. From FD-4, a view that
  falls behind its bound is dropped and sent a fresh snapshot. `Engine::snapshot` already plays the
  scrollback, the screen, the cursor and the modes into a blank terminal, so a view that has fallen
  behind and one that is new are handled the same way.

### 5. Every connection is authenticated, and a chat never holds a human power (Q2, V16; FD-6, and FD-27 for scopes)

**Each connection is authenticated twice: by the peer's uid, and by a credential for one client
scope.** A connection from another uid is refused before it says anything. The `0700` directory
already keeps other users out of the socket, and the uid check still holds if that directory is
ever wrong. A connection that presents no credential, or the wrong one, is refused. There is no
anonymous scope, not even for listing.

| Scope | Who connects | Where | Its credential | What it may do |
|---|---|---|---|---|
| `chat` | a hook, or a `purlis` command, inside one chat | that plane's hook socket | the chat's own token (#535), minted at the `exec` | report, ask for a ticket and open or report back (as today), ask for a secret to be resolved into a command (the gate decides), tell the host of a run change (ADR 0066's `purlis persona use`), and tell it a commit was refused (`CommitRefused`, ADR 0074) |
| `local-ui` | the app's window | `charterd.sock` | the `local-ui` credential | the whole session protocol and UI RPC, **vault values** (section 1), and the settings the host acts on |
| `terminal` | `purlis attach` and `purlis ls` from the operator's shell (FD-21) | `charterd.sock` | the `terminal` credential | the session protocol on the operator's chats |
| `fleet-mcp` | the fleet MCP server (HP-20) | `charterd.sock` | the `fleet-mcp` credential | the capability set FD-27 gives it |
| `approval` | `purlis inbox` and approvals from a shell | `charterd.sock` | the `approval` credential | answer needs-you and V15 approvals |
| `remote-link` | a peer device through a connector (V9) | the connector's stdio | the device key, inside the stream (RR-13) | the session protocol |

- **Vault values and the settings the host acts on are reachable from `local-ui` only.** Every
  other scope is refused them, `terminal`, `fleet-mcp`, `approval` and `remote-link` included
  (FD-27). The `chat` scope never receives a value either: the host resolves a secret into the
  environment of the command it was asked for, after the gate, and the value is never written back
  on the channel.
- **The human scopes' credentials are files in the machine store** (ADR 0034's directory, under
  `charterd/`), one per scope, each `0600`. **Q2's "per-session token" is read as one credential per
  scope per start of `purlisd`**: the host mints them fresh every time it starts, so a credential
  lives only as long as the host that minted it, and a crash or an upgrade rotates them. The app
  reads its own like any other client, so there is one mechanism, whoever started the host. (A
  chat's token is the other kind of session token, one per chat, and #535 already mints it.)
- **A chat's sandbox denies that directory**, which is ADR 0067's denial class 3. **It also denies
  connecting to `charterd.sock` at all.** A chat reaches the host only through its plane's hook
  socket, where the only scope is `chat`. So the human scopes are not only refused to a chat: they
  are on a socket the chat cannot open. ADR 0067 §2's *"asked of `charterd` over its socket"* is the
  hook socket.
- **`purlisd` also refuses a human scope to a connection from a chat's process tree** (V16a). It
  reads the peer's pid from the socket and refuses when the peer's session id is one of its chats'
  (every chat's program leads its own session, since `portable-pty` starts it with `setsid`), or
  when a chat's program is among the peer's ancestors. **This check is a second layer, not the
  boundary.** A process can leave its session and its parent on purpose, as `session.rs` already
  notes of daemons. The boundary is the credential the sandbox keeps from the chat. The check
  catches the ordinary case, such as a harness that runs `purlis inbox` itself, and says so in the
  refusal.
- **An agent can never answer its own asks or approve its own secret requests.** Answering and
  approving are `approval` and `local-ui` capabilities only. The `chat` scope can ask and wait. It
  cannot answer.

### 6. The hook channel keeps its line, and its spool is per chat (FD-30: the hook spool)

- **The line does not change.** A hook still writes one JSON line with its chat number and its
  token to the path in `$PURLIS_HOOK_SOCKET`, inside the spec's 50 ms. ADR 0066 already fixed this.
  What changes is who listens: `purlisd` binds each open plane's hook socket, at the path
  `hooks::socket_for` gives today, and holds `ChatTokens`.
- **The hook wire supports N−1** (FD-28): a hook of the previous version is served by the new host.
  A hook carries no version today, so the first release with `purlisd` adds an optional `v` to the
  line, and an absent `v` reads as the version before it.
- **When the socket refuses, the hook spools, to its own chat's spool file.** There is one spool
  per chat, never one per plane, beside the chat's other per-chat files under `.charter/`. The hook
  appends its line within 50 ms, with the next number in that chat's sequence and a MAC keyed by
  that chat's token.
- **The host checks each spool as it drains it**, when it starts. It verifies every line's MAC
  against the chat's token and its sequence against the last number it holds for that chat. It
  records each verified line as an event, then seals it into the audit chain (AU-3, when that
  exists) with the result of the check. A line that fails the MAC, and a gap in the sequence, are
  recorded as such. Audit never silently loses an event (O1: the audit log is never sampled).

**What that holds, stated plainly.** A chat's sandbox allows its hooks to append to **its own
spool only**, and denies every other chat's. Claude Code runs its hooks outside the sandbox it
gives its own tools, so for a Claude Code chat the agent's commands cannot touch the spool at all.
Codex and opencode run under purlis's own wrap, hooks included (ADR 0067 §2), so there a process
in the chat can write or truncate **that chat's own spool**, and could write lines as that chat.
That is no more than it can already do on the live channel, where anything in the chat's process
tree holds the chat's token and can report as the chat. It can never write or remove another
chat's lines. Lines it removes before a later one are a gap the drain detects; lines removed from
the end, before the drain, are not detected. Once drained and sealed, a line is out of the chat's
reach. FD-30's acceptance line *"a chat cannot write the spool"* becomes *"a chat cannot write
another chat's spool, and a gap in its own is recorded"* (V22a).

**Note, 2026-09-30 (ADR 0074):** `CommitRefused` is a `chat`-scope line like a report, and
spools like one when the socket refuses it. Until FD-30 lands (charter#667), no hook spools:
`purlis git-hook` drops the line on a refused socket, as every hook does today, and the
commit is refused all the same. FD-30 covers this line with the rest.

*Amended 2026-10-02 by ruling V63, for FD-30 (charter#667, PR #953).* Four things above read
differently as built:

- **Codex's hooks run outside purlis's wrap**, as Claude Code's do. Today no harness purlis
  sandboxes runs its hooks inside the sandbox, so a chat's sandbox denies reading and writing
  **every** chat's spool, its own included.
- **The verifier is at rest.** Each line's MAC is keyed by a key derived from the chat's token
  (HMAC-SHA256 of the token under a fixed label). The host writes that key, never the token, to
  `keys.json` in the spool directory before the token reaches the chat, so a later host can
  check the line. The file is owner-only, and the sandbox denies a chat reading and writing it.
- **A spool exists only where the sandbox's integrity denial reaches**, which is a project's
  `.charter/app/spool/`. Beside a hook socket anywhere else, the hook spools nothing, writes no
  key, and reports the line as lost.
- **A gap is found from the file's own highest number.** Lines removed from the end are not
  detected, and neither are lines removed and then followed by new ones.

*Note, 2026-10-07, for issue 983: the shape as built, with nothing above changed.* A chat's
spool is a folder, `spool/<n>/`, with one file per line, where it was one file of lines. A hook
takes its number by making a file exclusively and waits on no lock, so hooks of one chat that
spool at once no longer cost each other a line: under the lock the file needed, a hook that
waited 250 ms behind the others' syncs lost its line and said so. The line, its number, its MAC,
the keys, the drain's checks and the sandbox's denial are as above, and a gap is found from the
highest number a key's files hold. The file of a build before is still drained (N−1).
`docs/plane-format.md` has the shape.

### 7. An upgrade hands the terminals over, and drains only when it must (V7; FD-28: upgrade without losing agents)

**The normal path hands every live PTY master to the new host over `SCM_RIGHTS`.** A PTY master is
a file descriptor, and a unix socket can pass one to another process. The program on the other side
of the terminal never notices. This is the pattern nginx and HAProxy use for binary upgrades, and
systemd's descriptor store uses for restarts. `rustix` has a safe API for it, so it needs no
`unsafe`.

**The handoff between the two hosts:**

1. **The old host starts the new one** as its own child, `purlis serve --take-over`, with one end
   of a `socketpair` as the child's stdin. The old host keeps the other end. Starting a child with a
   chosen stdin needs only `std::process`.
2. **The old host quiesces.** It stops accepting input on every stream and stops reading the PTYs.
   Bytes the harnesses write from then on wait in the kernel's PTY buffer, where the new host reads
   them. The engine has consumed everything already read, so nothing is read twice or lost.
3. **The old host sends its state, then its descriptors.** For each session: the PTY master, the
   program's pid and process group, the engine's snapshot (`Engine::snapshot`, which includes the
   scrollback and the modes), and any input still queued for a program that was not reading. For
   the host: the listening sockets (`charterd.sock` and every plane's hook socket), so no hook and
   no client is refused during the swap; the chat tokens; the board; each chat's ids and current run
   (ADR 0066); and the event log's last `seq`. The old host writes no event after it sends that
   `seq`, and the new host writes none before it has it, so the log keeps one writer.
4. **The new host builds each session from what it received:** a fresh engine that the snapshot is
   played into, and the master it was handed. It answers on the inherited listening sockets. The
   old host exits without ending any program. That needs a way out that is not `Drop`, since
   dropping a `Session` hangs up its program: `Session` gains a consuming hand-over that gives up
   the master and the child without ending either.
5. **Clients reconnect.** The window, `purlis attach` and a runner's link each see their
   connection close, reconnect, resubscribe from their cursor (`subscribe(since)`) and re-attach
   their views with a snapshot. Their connections are not handed over: a reconnect is the path
   every client needs anyway, for a crash and for a dropped view (section 4).

**How the supervisor keeps track of the new host.** The new host is the old host's child, so each
supervisor from section 3 needs one more step:

| Supervisor | After the handoff |
|---|---|
| systemd | The new host tells systemd `MAINPID=<its pid>` and `READY=1` on `$NOTIFY_SOCKET` (the unit is `Type=notify` with `NotifyAccess=all`), and systemd adopts it as the unit's main process. This is systemd's documented way to re-execute a service in place. Writing a datagram to that socket needs only the standard library |
| launchd | launchd tracks only the process it started, and it cannot run a second instance of the same job. So the handoff runs twice. The new host serves as a **transient host**. The old host exits with a failure status, and launchd starts the job again. That job, finding a host answering with a handoff pending, takes everything over from the transient host by the same five steps, and the transient host exits. Chats keep running through both hops, and the job's own process is again the one launchd started |
| the app | The app supervises by its connection, not by a child's pid, so a new host answering on the socket is all it needs. Nothing more is done |

FD-28's acceptance holds it: an update with 20 live chats changes the host's pid, and every chat
survives, mid-turn included. The chaos test in CI runs it under each supervisor purlis uses.

**What a handed-over program loses: its exit status.** It is still the old host's child, so when
the old host exits, the operating system reparents it, and the new host cannot `wait` for it. The
new host learns that it ended the way `session.rs` already learns it first, from the end of its
output, and reports the end without a code. The group it may still signal is checked first (its
session id is still the chat's, and the chat's terminal is still open), because the leader is no
longer held unreaped by a parent that keeps its id from being reused.

**The fallback is drain and resume.** It is used when the handoff cannot run: on Windows (see *Later decisions*),
when the new host cannot read the old host's state (the state is private between adjacent versions,
and a version that changes it past N−1 must say so), or when the handoff fails or does not finish
by its **handoff deadline** (proposed: 10 seconds). Then:

- the old host starts no new turn, and each chat is ended at its next turn end, which its hooks
  report;
- the new host starts, and each ended chat comes back through its harness's own resume, as
  **Reopen all** does today (`reopen.rs`, `sessionresume.rs`);
- a shell tab is not a harness and has no resume. It is named in the window before the fallback
  starts, as a mid-turn chat is today.

### 8. A runner is the same host behind a connector (V9; RR-13: the runner ADR)

A runner runs the same `purlis serve`. The app reaches it through a connector command's stdio
(`ssh`, `gh cs ssh`, `coder ssh`, `docker exec`, `kubectl exec`), and the session protocol runs
over that stream unchanged. The connection's scope is `remote-link`, authenticated by the device key
inside the stream, not by anything the transport provides.

- **Supervision on a runner** is RR-15's (a headless host that survives an SSH drop): a `systemd
  --user` unit with linger on Linux, a LaunchAgent on macOS, and `purlis serve --foreground` in a
  container, where the container's own runtime is the supervisor.
- **A runner never hands off. It drains** (RR-14: runner bootstrap and version skew). Its versions
  sit side by side under the machine store, at `<config>/server/<ver>/` (V22a, from ADR 0069), as
  an AppImage's copy does. A new version takes the new chats, and the old
  one keeps its chats until each ends and is never killed. That is the one time a device has two
  hosts. The new host is then the device's only event-log writer, and the old one sends its events
  to the new one as a client, so ADR 0066's single writer still holds.
- **The chat sandbox on a runner denies the host's install files** and the git internals it works
  on (V16d, ADR 0067 §5.4).

RR-13 decides the rest. This record only guarantees that nothing in the host assumes the window is
on the same machine.

### 9. Descriptors (V7; SC-15: the open-file limit)

A host with 200 chats holds several descriptors per chat: the master, the reader's and writer's
copies, each view's stream, and the hook connections. macOS starts a launchd-started process with a
soft `RLIMIT_NOFILE` of 256. **The app and `purlisd` each raise the soft limit to
`min(hard, OPEN_MAX)` when they start**, through `rustix::process::setrlimit`, which is safe. The
cap matters on macOS: the hard limit there is often `RLIM_INFINITY`, and asking for it fails with
`EINVAL`. Descriptor counts are recorded in `stress.jsonl` on macOS and Linux (SC-15), and
`stress.yml` asserts them (V8).

## ADR 0025, amended

ADR 0025's bullet *"No daemons. Every session is a child of the app. Quitting ends them all… When
the app starts again, it reopens chats through each harness's own resume, not through reattach"* is
**replaced by this record's section 2**. The rest of ADR 0025 stands. In particular, the core still
owns every terminal's state headless, the window still draws only the panes on screen, purlis
still never parses a harness's output, and there is still no terminal frontend: `purlis attach` is
a client of the host, drawing through the operator's own terminal (FD-21), not a second UI purlis
maintains. `docs/spec.md`'s *"with no daemon"* goal (line 20) and its 50-session limit (line 408)
change with FD-1's scope record (the product's scale target), which supersedes those lines.

## What changes where

The code does not change with this record. Each row is what the tickets after it change:

| Code as it is | What changes |
|---|---|
| `app/src-tauri/src/sessions.rs`, `hooks.rs`, `planewatch.rs`, `autosave.rs` | They leave the app crate, because `purlis serve` is in the `purlis` binary, which must not link the Tauri app crate. They go into a new workspace crate, `charter-host`, which depends on `charter-core` and never on Tauri. The CLI links it for `purlis serve`, and the tests drive it in process. FD-3 (the `SessionHost` trait) extracts the interface first, in place, with no behaviour change |
| `app/src-tauri/src/vaults.rs` | Keeps listing a vault's names and metadata. Reveal, copy and every read of a value become `local-ui` calls to the host (section 1) |
| `session.rs`: `impl Drop for Session` ends the program | Kept. `Session` gains a consuming hand-over that gives up the master and the child without ending them (section 7). The module's first paragraph (*"there is no daemon"*) is rewritten to say the host is the owner |
| `session.rs`: `Session::attach` gives each view an unbounded `mpsc::channel` | A byte-bounded queue. A view that falls behind is dropped and re-sent a snapshot (FD-4) |
| `hookwire.rs`: the line, `ChatTokens`, `Listener::bind` | The line is unchanged, and gains an optional `v`. `ChatTokens` is held by the host and handed over on an upgrade. `Listener` accepts an inherited listening socket as well as binding one. The per-chat spool, its MAC and the drain are FD-30's |
| `updates.rs`: `install_update`, `Planes::let_go_of_all_to_update` | An app update tells the host it is restarting, and ends no chat. A host update runs section 7. `mark_restart_to_update` and **Reopen all** stay for the fallback and for a crash of the host |
| `lib.rs`: `Sessions` and the hook listeners built at start | The app connects to `purlisd`, starting it under the supervisor section 3 names if none answers. It raises `RLIMIT_NOFILE` first |
| `ipc_commands.rs` | The source the UI RPC and its TypeScript client are generated from (FD-4) |
| Where the `purlis` binary runs from | In place from the app bundle on macOS and from `/usr/bin` with the `.deb`. An AppImage's mount goes away when the AppImage exits, taking the running binary's pages with it, so there `purlisd` runs from a copy in a versioned directory under the machine store, as RR-14 installs it on a runner |

## What this costs

- **A second process, and a socket between the window and the terminals.** Every keystroke and
  every frame crosses it. A local unix socket adds microseconds, far inside ADR 0026's limits, and
  FD-5 re-measures every one of them through the host in CI before it ships (SC-16: the latency
  gate).
- **A crash of `purlisd` still ends every agent.** One host process is one point of failure. The
  supervisor, the journal and one-click resume make the failure short and visible, but they do not
  make it free. The alternatives that avoid it are under *What was rejected*.
- **A handed-over program's exit code is lost** (section 7).
- **The launchd handoff takes two hops**, so it runs the handoff code twice per upgrade on macOS
  when the host is a login item.
- **A chat whose sandbox the operator turned off can read the human scopes' credentials.** ADR
  0067 §7 lifts denial class 3 with the rest, and the process-tree check alone does not hold
  against a process working to escape it. The chat's badge already says it is unsandboxed. This
  record adds nothing to hide it.
- **Agents can outlive the window by up to the grace period** after a crash, with nothing on screen
  saying so. That is the price of not losing them to a crash.

## What was rejected

- **No host: keep sessions in the app.** It keeps ADR 0025's simplicity, and it keeps the worst
  reliability property at scale. Q2 ruled against it.
- **A host process per chat** (the containerd-shim or conmon pattern): each chat's PTY held by a
  small process of its own, so a crash of the main host ends nothing. It is standard practice, and it
  costs 200 more processes at Q1's target (about 200 open chats per machine), each needing its own
  authentication and upgrade path. V7 chose one host with descriptor handoff. The per-chat shim
  stays the answer to reach for if the crash rate FD-29 measures ever says a host crash is common.
- **The supervisor holding a copy of every master** (systemd's descriptor store) so a host crash
  ends nothing. On macOS without launchd the supervisor is the app, and the app holding the masters
  is the property this record removes. On Linux it is worth measuring later, as an addition that
  needs no change to this design.
- **A separate `purlisd` executable.** Q2 said "same binary". One binary is one thing to sign,
  version and verify (ADR 0042), and the CLI and the host can never be of different versions.
- **One host per plane.** It would give one device several event-log writers, several supervisors
  and several upgrade handoffs, and ADR 0066 already assumes one host holds several planes.
- **Handing client connections over on an upgrade.** Every client already needs to reconnect after
  a host crash. A second path for the upgrade would be the one that is rarely exercised.
- **One spool per plane.** A chat that can append to a shared file can also truncate it, and would
  take every other chat's lines with its own.
- **A loopback TCP port.** Any local user can connect to one, and a peer uid is not available on
  it. A unix socket in a `0700` directory is the standard answer.

## Later decisions

- **Windows.** Windows is not ported yet (`updating.md`), and `hookwire` refuses there today
  (purlis#95). Its transport, and whether its upgrade can do more than drain, are decided when
  it is ported.
- **How soon a security release forces the upgrade.** FD-28 asks for a deadline. The handoff loses
  nothing, so the length only matters where the fallback is the only path. FD-28 proposes it.

## Ruled (V22, 2026-09-30)

1. **One host per OS user per device**, serving every plane.
2. **After an app crash, agents keep a 10-minute grace period**, then end as a quit would.
3. **If the host will not start, the app runs no chats and says why.** There is no in-process
   fallback.
4. **A sixth client scope, `chat`**, and chats are denied `charterd.sock`.
5. **The hook spool is one per chat**, verified and sealed into the audit chain on drain. FD-30's
   acceptance becomes *"a chat cannot write another chat's spool, and a gap in its own is
   recorded"*.
6. **Clients reconnect across an upgrade**, and a handed-over program's exit code is lost.
7. **Under launchd an upgrade hands off twice**, through a transient host.
8. **The host code goes in a new `charter-host` crate.**
9. **The app keeps plane reads and settings writes; every vault-value read goes through the
   host.** The operator's own CLI reads vaults directly for now.
10. **A runner's host versions live under the machine store** (`<config>/server/<ver>/`), from
    ADR 0069's item 9.

## Amended by FD-4 (#643, PR #817), 2026-10-01

FD-4 built §4's transport as the `charter-session-protocol` crate: a version negotiation that
fails closed, Yamux over any ordered byte stream, and views with a watermark and a byte-bounded
queue. One thing departed from the text above, and the text above is left as accepted.

1. **There is no priority control lane; the control lane is never behind more than a bounded
   amount instead.** §4 says *"a priority control lane carries commands and events past a busy
   terminal"*. The multiplexer §4 asks for, an existing one, is Yamux, and Yamux has no stream
   priorities: its frames go out in the order the streams hand them over. What carries a
   command or an event past a busy terminal is that nothing ahead of it is unbounded:
   - each stream's bytes in flight are held to its Yamux window, and each link to
     `MOST_STREAMS` (256) streams and a 64 MiB receive window across them;
   - each view's bytes written and not drawn are held to its high watermark (64 KiB), and its
     queue to `most_queued_bytes` (1 MiB), past which the view is dropped and re-attached;
   - Yamux sends data in frames of at most 16 KiB, so a control frame interleaves with them
     rather than waiting for a terminal's whole burst.

   **What was measured**, in the crate's tests on the operator's machine, which is evidence and
   not a gate (ADR 0086): a keystroke through ten panes flooding 100 to 270 MB/s over a unix
   socket at a p95 of 3 to 21 ms; and a needs-you through fifty busy chats on an in-process
   shaped link (150 ms round trip, 2% per-packet loss as fast retransmit, 10 MB/s, 2 MiB in
   flight, no congestion control) at a p50 of about 250 to 300 ms and a worst of 330 to 720 ms.
   With the watermark's gate removed, a slow client had 266 KiB in flight instead of 64 KiB.

   **What is not measured yet:** the same over TCP shaped by kernel netem, which a CI job
   files for, and the keystroke budget through the host, which is ADR 0086's L1 row in
   SC-16's `bench` job. If either shows the control lane waiting too long, the fix is a
   scheduler in `link` that drains the control lane first, behind the same interface; it
   changes nothing else in this record.

## Amended by FD-6 (#645, PR #927), 2026-10-02

FD-6 built §5's authentication. The text of §5 above is left as accepted, and this note is
the amendment. **Ruled by V68** (operator, 2026-10-02): admission is mutual.

1. **A credential is proved, never presented.** §5 says a connection *"presents"* a credential.
   On the wire it never crosses at all:
   - the host sends a fresh challenge;
   - the client answers with its scope, a fresh nonce of its own, and an HMAC under the
     scope's credential over a client label, the scope and the challenge;
   - the host checks that proof in constant time.

   Whatever answers at the socket's path therefore learns nothing it can replay
   (`charter_session_protocol::auth`).
2. **The host proves itself too.** The host's admission carries an HMAC under the same
   credential, over a distinct host label, the scope, its challenge and the client's nonce.
   The client checks it in constant time and refuses a host that cannot make it, before
   anything else is said. A process of the same user that bound the socket's path while the
   host was down can then neither collect a credential nor pass for the host. The two labels
   differ, so neither proof can be reflected as the other.
3. **The uid check is a type.** Only the listener's check makes the connection the host's
   `serve` takes. An unmapped uid (the overflow uid, or `(uid_t)-1`) is refused on either end,
   since two of them compare equal without being one user.
4. **One deadline covers the version, the admission and the control lane together.**
5. **The checks have one copy.** A peer's uid, a private directory and a private file are
   checked by `crates/same-user` for `charterd.sock`, the hook sockets and the credentials alike.

What is left is stated in the protocol's docs: a same-user process that can already sit between
a client and a running host could relay one live admission. Such a process can already open the
credential files, which is why the chat sandbox denies a chat both the files and the socket.

## Amended by FD-26 (#663), 2026-10-03

FD-26 built §4's split as two modules of `charter-session-protocol`: `session`, the public
session protocol, and `ui`, the private UI RPC, on one control lane. The text of §4 above is left
as accepted, and this note is the amendment. **Ruled by V76** (operator, 2026-10-03), on the
review of FD-26.

1. **The link's version is the session protocol's version.** The version §4's link negotiates
   before admission is the session protocol's (`session::VERSION`, 1.0), not a second number
   inside the lane. A minor only adds: a command, a field, an event kind, a word. A reader
   ignores fields it does not know. A host answers a command it does not know with
   `unknown_command` and keeps going, which is how a newer client learns that an older host
   lacks it. A frame kind it does not know is passed over. A new major is a break, and a host
   speaks its own major and the one before (N−1).

   **What this costs against E5** (*"protocol versions per capability"*): there is one version
   for the whole session protocol, not one per capability. A capability that changes a command
   or a reply moves the whole protocol's minor, and a client tells what a host lacks by its
   refusals, not by a per-capability number it was told up front. Per-capability versions can
   be added later as a field of the hello, which a minor may do. They are not built until a
   second capability needs one.
2. **The wire format.** Each control frame is one JSON object with one key saying what it is:
   - `call`: a command from the client, `{"id": <n>, "command": "<word>", …its fields}`;
   - `reply`: its answer from the host, `{"re": <n>, "ok": <answer>}` or
     `{"re": <n>, "refused": {"code": "<code>", "why": "<sentence>"}}`, in any order relative to
     other replies;
   - `pushed`: from the host to a client that subscribed, `{"event": <ADR 0066's envelope>}` or
     `{"missed": {"device", "after", "resumes_at"}}`;
   - `ui`: a frame of the UI RPC.

   The bytes `write` types into a chat cross as standard base64 in a `bytes` field, so a mouse
   report that is not UTF-8 crosses whole. A terminal's output never crosses the lane: an
   attach opens a view (§4) on a stream of its own.
3. **The refusal codes** a program reads are `unknown_command`, `malformed` (a known command
   in a shape the host cannot read), `no_such_chat`, `no_such_project` and `not_allowed` (the
   scope may not do this, FD-27). A later minor may add codes; a client treats one it does not
   know as a refusal and shows its `why`.
4. **Every word on the wire is snake_case** (V76c): keys, command words, answer kinds, refusal
   codes, and the words a value carries, such as a chat's state (`needs_you`, never
   `needs-you`). Kebab-case is for UI text only. **The one exception is an event's `kind`**,
   which keeps ADR 0066's dotted names (`run.started`, `hook.pretooluse`): it is the event
   log's own word, carried unchanged inside a `pushed` frame, and the same word the log, the
   audit and OTel use.
5. **A project is named by a stable id from the start** (V76b). `start` and each listed chat name
   their project by a ULID minted once into its `charter.toml` as `[project] id`
   (`docs/plane-format.md`). A path appears only as `project_path`, a hint to show a person,
   never a key, since one project is at another path on every clone and device.
6. **A `missed` marker carries no snapshot. The client re-attaches** (V76a). A client that missed
   events and shows a chat's terminal sends `attach` again: the view's stream opens with the
   terminal's snapshot (§4). So there is one way to get a screen back, whether the view fell
   behind, the client reconnected, or events were missed. ADR 0066's *"a fresh snapshot and a
   marker"* is read that way; see *ADR 0066, amended by FD-26*.
7. **The compatibility promise is held in CI.** `crates/session-protocol/tests/fixtures/session-protocol.jsonl`
   records one frame of every kind, and every build must read each one back exactly as
   recorded. Each frame, outcome, command, answer and pushed variant gets its word from an
   exhaustive `match`,
   so a new variant does not compile without a word, and then fails the test until its frame
   is recorded. No UI RPC frame is in the record.
8. **The UI RPC is the window's alone, for one build.** The host serves it only on a `local-ui`
   link, and only after a hello naming the host's own build. Any other scope, another build,
   or a call before the hello is refused, and the session protocol on the same link keeps
   working. Its methods are the app's commands from `ipc_commands.rs`, and its typed TypeScript
   client (`app/src/uiRpc.ts`) is generated from the same list as the window's own, leaving out
   the commands that take a channel: on the link, a terminal's bytes are a view.

## Amended by HP-6 (#672), 2026-10-04

HP-6 lets a reply on a plane's hook socket carry authority: the window's answer to a chat's
permission prompt goes back on the hook that asked, and the hook prints the harness's
decision. The text of §5 above is left as accepted, and this note is the amendment. **Decided by
the dispatcher (D-88n)**, tightening V68 and FD-6's mutual admission.

1. **The hook authenticates the host by its peer's credentials, before it writes anything.** On
   connecting, `charter hook permissionrequest` reads the listening process's uid and pid from
   the kernel (`SO_PEERCRED` on Linux; `LOCAL_PEERCRED` and `LOCAL_PEERPID` on macOS). It goes
   on only if that uid is its own and that pid is one of the hook's own ancestors
   (`charter_same_user::admit_host`). The hook itself never counts. Otherwise it writes nothing
   and prints nothing, and the harness's own prompt decides. A process a chat runs is a
   descendant of the harness, never its ancestor.
   **The peer pid is not an identity on its own.** On both kernels it is the number of the
   process that listened, and the kernel keeps that number after the process is gone. This is
   measured for Linux's `SO_PEERCRED` once a child keeps the socket; macOS's `LOCAL_PEERPID` is
   a bare sample. A later process can be given the same number. So the process with that pid
   must also **hold a socket bound at the hook's path now**, read from the kernel's own tables:
   - **Linux:** a listening entry for the path in `/proc/net/unix`, whose inode is one of
     `/proc/<pid>/fd`'s `socket:[inode]` links.
   - **macOS:** `/usr/sbin/lsof -a -p <pid> -U -F n`, by its absolute path and with no
     environment, naming the path. The `libproc` crate was the alternative, but it adds a
     dependency whose `unsafe` is only moved out of this workspace.

   Paths are compared once resolved, so `/tmp` and `/private/tmp`, or a link to the socket, name
   the same one. Any error fails closed. A process given a dead listener's pid holds no such
   socket, and nothing a chat runs can hand one to an ancestor. No timestamp is trusted: a
   socket file's times are its owner's to set.
2. **No secret is used, because none would hold.** The hook's environment and arguments are the
   chat's to read, whatever is put there.
3. **The sandbox's integrity denial on the socket's directory stays**, as the second layer.
4. **When `purlisd` hosts chats, the hook socket's owner must remain the chats' ancestor.**
   Either `purlisd` itself owns each plane's hook socket, or the hook is taught to admit the
   process that does by the same test: it holds the socket at the path. A host arrangement in which the
   process listening on a hook socket is not an ancestor of the chats it serves breaks
   permission answers, and they fail closed: the pane asks.

## Amended by FD-27 (#664), 2026-10-04

FD-27 built §5's client scopes: what each may call, and the refusal of a person's scope to a
chat's processes. The text of §5 above is left as accepted, and this note is the amendment. It
applies V7, V16a and V75.

1. **One table says what each scope may call, and `session::serve` checks it** before any host
   hears of a command (`charter_session_protocol::grants`). A command the scope is not granted
   is refused `not_allowed`, and the link carries on. The UI RPC's hello asks the same table.

   | Command | `local-ui` | `terminal` | `fleet-mcp` | `approval` | `editor` | `remote-link` |
   |---|---|---|---|---|---|---|
   | `list` | yes | yes | yes | yes | | yes |
   | `attach`, `detach` | yes | yes | | | | yes |
   | `write`, `resize` | yes | yes | | | | yes |
   | `answer` an ask | yes | | | yes | | |
   | `answer` an ask that elicits a secret | yes | | | | | |
   | `stop` | yes | yes | yes | yes | | yes |
   | `start` | yes | yes | | | | yes |
   | `subscribe`, `unsubscribe` | yes | yes | yes | yes | | yes |
   | the UI RPC: vault values, settings writes, the app's commands | yes | | | | | |

   - **`editor`** has a row now. It has none of these commands: its four are ADR 0081's, built
     by ED-2, and are its alone.
   - **`fleet-mcp`** lists, watches events and stops. Its clients are outside agents (HP-20),
     and an agent holds no human power (V16): it never types into, starts, resizes, views or
     answers a chat. A view is left out until HP-20 asks for one.
   - **Answering is `local-ui`'s and `approval`'s** (V75), and an ask that elicits a secret is
     `local-ui`'s alone. Which asks elicit a secret is the host's to say; a host that does not
     say is taken to hold one, so the check fails closed.
   - **Neither `terminal` nor `remote-link` answers, pending an operator ruling.** ADR 0078's
     amendment of this section lets `terminal` answer on the desktop, and lets a runner's
     `remote-link` carry an answer that a desktop human scope sent. V75, ruled later, names only
     `local-ui` and `approval`. These are two conflicts, and both wait on the operator. Until
     then both scopes are refused `answer`, which fails closed.
2. **No scope on `charterd.sock` is admitted from inside a chat**, whatever it proves. Every
   such scope is a person's. The listener reads the peer's pid from the socket as it accepts,
   and the host's handshake asks its chats: a peer that is a chat's program, is in a chat's
   session, or has a chat's program among its ancestors is refused, with a sentence that says
   so, before its proof is checked. A peer whose ancestry or session cannot be read is refused
   the same way. The ancestry is read by the same code HP-6's hook uses
   (`charter_same_user`). §5 already calls this the second layer: the first is item 3.
3. **A chat's sandbox denies reading and writing `<config>/charterd/`**, where the credentials
   and the socket live, under ADR 0067 §5's class 3, on every harness purlis compiles a sandbox
   for. Each compiler writes the path as the kernel names it too, so a config root reached
   through a link is still denied. **That denial does not stop a connect to the socket:** a
   sandbox treats connecting to a unix socket as network, not as reading a file. What keeps a
   chat off `charterd.sock` is that no compiler allows a unix socket beyond the hook socket, and
   each compiler has a test for that. For Claude Code's own unix-socket default, and for Linux,
   this is still to be checked (#664).
4. **`remote-link` is admitted by a proved device, never by a credential file.** The admission
   exchange refuses the scope's word whatever proof comes with it. A host serves a device's link
   only through a call that takes the result of ADR 0078 §3's Noise handshake. RR-13 builds that
   handshake, so until then no build of the host admits `remote-link` at all. The crate's own
   tests stand in for it, and drive a stub `remote-link` client in a separate process.

## Amended again by FD-4 (#643), 2026-10-04

The first FD-4 amendment's item 1 said there is no priority control lane, and named the fix if
the lane was found waiting too long: a scheduler in `link` that drains the control lane first,
behind the same interface. It was found waiting too long, and the scheduler is built. §4's text,
*"a priority control lane carries commands and events past a busy terminal"*, holds again, and
that item is withdrawn.

1. **Why.** Yamux takes one frame from each stream with something to send, in turn. So a control
   frame waits behind a 16 KiB frame of every busy stream. On a 1 MB/s link, a connector's stdio
   to a remote host, fifty busy streams kept a control frame waiting 908 ms. The scheduler
   brings that to 73 ms, which is the 64 KiB the link itself holds.
2. **How.** The multiplexer writes into the scheduler (`priority` in the crate), not the
   transport. The scheduler cuts the bytes into Yamux's own frames and sends the control lane's
   first: Yamux stream 1, the client's first, which the host checks is stream 1 before it takes
   it as the lane. The session's pings and pongs go with the lane, as Yamux puts a pong ahead of
   the streams, so the round trip Yamux measures is not the scheduler's queue. The other streams
   take turns, one frame each, as Yamux's own streams do, and a go-away goes last.
   - **Two bounds.** The streams' frames are bounded at 16 MiB (256 streams × a view's 64 KiB
     watermark), and the lane's, pings and pongs included, at 2 MiB (two of its largest
     frames). Each frame counts at least 64 bytes against its bound, what a queued frame holds
     of the heap, so a flood of 12-byte pongs is bounded in memory, not only in bytes on the
     wire. Past either bound, the scheduler takes no next frame of any kind until it has made
     room, and that includes a frame with no body. That is the transport's back-pressure,
     passed on: Yamux stops writing, and stops reading once a pong or a window update waits.
     So a peer that grants credit and never reads, or floods pings, cannot grow either queue.
   - **What that costs.** Past a bound, **everything waits, the lane too.** Yamux has one
     frame on its way at a time, so it cannot hand over the lane's frame while another is
     stuck. In use the bounds are not reached, since the windows and watermarks hold what is
     in flight well under them.
   - **A dropped link** writes what it was handed and shuts the transport, for at most
     10 seconds.
3. **The wire is unchanged, and so is the version.** Every frame is Yamux's, whole, and each
   stream's frames keep their order. Only frames of different streams are reordered, and Yamux
   allows that, since each stream has its own window and sequence. A peer that does not schedule
   reads the same bytes. So the protocol stays 1.0, and no capability is negotiated: there is
   nothing for the other end to agree to.
4. **What it cannot jump** is what the transport already holds: a socket's send buffer, a pipe,
   `ssh`'s channel window. That wait is bounded by the windows and watermarks the first FD-4
   amendment lists. Sizing a remote link's watermarks to its round trip is #930.
5. **Measured** on the crate's deterministic link simulator (`tests/common/netsim.rs`): tokio's
   virtual time, seeded loss, a rate, a delay and a buffer. The ends' own work takes no time
   there, so the delays are the protocol's and the link's, and the tests assert them; ADR 0086
   keeps only wall-clock budgets out of `cargo test`. Fifty busy chats on a 150 ms, 2% loss,
   10 MB/s link with 2 MiB in it: a needs-you at a p50 of 227 ms and a worst of 386 ms, where
   Yamux alone gave 262 and 455, within #643's 1 s plus the round trip. The heap peaked at
   57 MB of the 90 MB the limits allow. `charter-session-bench`'s keystroke under ten flooding
   panes costs about 20 µs: a median of about 0.176 ms against main's 0.156 ms (+13%), inside
   the `bench` job's 1.20 gate. The same over TCP under
   kernel netem is still SC-21 (#828).
