# A chat lives in `charterd`, and the app is its client

**Proposed 2026-09-30**, drafted for program-map ticket FD-2 from the operator's rulings Q2
(`charterd` session host, always on and supervised by the app, per-connection authentication from
day one), V7 (charterd hardening), V9 (a runner is `charterd` behind a connector), V16 (agents
never hold human powers) and X24 (the lifecycle). An agent drafted it and the operator rules it
(W7). It **amends [ADR 0025](0025-charter-is-rebuilt-as-a-desktop-app-on-a-rust-core.md)'s "No
daemons"**. It builds on [ADR 0066](0066-a-chat-is-a-ulid-a-run-is-a-stretch-of-its-conversation-and-a-device-is-random.md)
(identity) and keeps to [ADR 0067](0067-a-chat-runs-in-a-sandbox-charter-compiles-for-its-harness.md)
(sandbox). FD-5, FD-6, FD-17, FD-26..FD-30, RR-13, RR-14, LW-1 and ED-1 build on it.

## Where charter is today

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
the window ends 50 conversations mid-turn. The fix it names is the tmux-server, VS Code Server and
Zed-remote pattern: a session-host process that owns the terminals, which the window attaches to.
Q2 ruled for it. V7 hardened it. This record is the design those rulings asked for.

## The decision

**The sessions move out of the app into `charterd`, a session host that is the same `charter`
binary run as `charter serve`. The app starts it, supervises it and is its first client. A crash or
an update of the app no longer ends an agent. An upgrade of `charterd` hands its live terminals to
the new version over `SCM_RIGHTS`, with drain-and-resume as the fallback. Every connection is
authenticated from the first release, and a connection from a chat's process tree can never hold a
human power.**

### 1. One binary, one host per user per device

- **`charterd` is `charter serve`.** It is the `charter` binary the app already ships beside itself
  (`crate::charter_binary`, the one *Install `charter` command in PATH* links). There is no second
  executable to sign, notarize, version or update. `charterd` is the name for the process, never a
  separate file.
- **One host per OS user per device, serving every plane that user opens.** ADR 0066 anticipated
  this: FD-6 re-keys the token map on the chat id *"when `charterd` holds several planes' chats"*.
  One host means one event-log writer per device, which is what ADR 0066's `seq` needs (*"assigned
  by the single writer on the device"*). A second `charter serve` for the same user finds the first
  one's socket answering, and exits with a sentence saying so.
- **Its socket is per user, in a private directory:** `$XDG_RUNTIME_DIR/charter/charterd.sock` on
  Linux, and the per-user `TMPDIR` on macOS. This is the same choice `hooks::private_dir` makes for
  the hook fallback, for the same reasons. The directory is created at `0700` as it is made, the way
  `hookwire::private_directory` already does it.

**What `charterd` owns is decided by one rule: it holds what must outlive the window, and what a
chat must never reach. Nothing else moves.**

| Moves into `charterd` | Why |
|---|---|
| Every session: the PTY, its engine and its views (`Sessions`) | They must outlive the window (Q2) |
| The hook channels and the per-chat tokens (`hooks.rs`, `ChatTokens`) | A hook fires whether or not a window is open |
| The board: each chat's state as its hooks report it | It is derived from the hooks, so it lives where they arrive |
| `planewatch` and auto-save (`autosave.rs`) | Auto-save (ADR 0051) must keep saving a chat's work while the window is gone |
| The event log, as its only writer (ADR 0066, FD-9, FD-24) | One writer per device |
| Resolving secrets and the V15 approval gate (V16b, ADR 0067 §5.1) | The approval is enforced only if the chat cannot reach round it |
| The session records `charterd` writes for a chat that ends | The chat can end with no window open |

**Everything else stays in the app.** The window, the plane reads that fill its panels
(`plane_sidebar`, the memory and todo commands), settings, the vault view and the palette all keep
calling `charter-core` directly from the app's process, as they do now. They are reads of files the
plane already holds. Moving them would put every panel behind a socket for no reliability gain, and
make the host bigger than the job. The app asks `charterd` only for what the host owns.

### 2. The lifecycle (X24, FD-17)

X24's ruling: `charterd` is a login item only when the user has at least one trigger or paired
remote device, with a tray presence and a one-click stop, and otherwise it exits with the app. This
record reads that ruling against three ways the app can go away, because they are not the same:

| The app… | What `charterd` does |
|---|---|
| **quits** (the operator chose Quit) | Ends every session, as today, and exits. `reopen.json` is written first, exactly as `let_go_of_every_plane` writes it now. This is ADR 0025's promise, kept: quitting still ends the agents |
| **restarts to update** | Keeps every session. The app tells the host it is restarting before it goes, and the new app reattaches. **Reopen all** stops being the answer for an app update: nothing ended, so nothing needs reopening (FD-7) |
| **crashes**, or is killed | Keeps every session for a **grace window** (proposed: 10 minutes). An app that starts within it reattaches, and nothing was lost. When the window passes with no app, `charterd` ends the sessions as a quit would, writing `reopen.json` first, and exits. Agents never keep running unseen for longer than that |

When the user has a trigger or a paired device (FD-17), none of the three ends the sessions: the
host keeps running as a login item and shows in the tray, where one click stops it. **Stop** from
the tray is a quit.

The app tells `charterd` which case it is by saying so before it goes (`quit` or `restarting`). A
connection that closes without either is a crash. That is the only signal the host needs, and it
cannot be mistaken, because a quit that fails to say so is treated the safer way: as a crash whose
sessions are ended when the grace window passes.

On battery, `charterd` follows V8's idle budget: its timers and polls are driven by the file
watcher and pause while no window is showing them. It never throttles a running chat's terminal.

### 3. The supervisor and its crash-loop breaker (FD-29)

- **The app is the supervisor while it runs.** It starts `charterd` if none is answering, and starts
  it again if it exits unexpectedly. When the host is a login item (FD-17), the operating system's
  own service manager supervises it: a launchd agent on macOS, a systemd user unit on Linux, each
  with its own restart throttle. Charter writes no supervisor of its own for that case, because both
  are the standard tool for it.
- **The breaker: three unexpected exits within five minutes stops the restarts.** The app then says
  the host is failing, shows the last lines of its log (FD-8's `tracing` file), and offers two
  things: **Try again**, and **Run without the host for now**. The second runs the same host code
  in-process for the rest of this launch, which is charter as it is today: the window's badge says
  that agents end when charter quits. That in-process host is FD-3's `SessionHost` trait with its
  local implementation, so it costs no second code path. The launchd and systemd units use the same
  thresholds.
- **What a crash of the host costs.** When `charterd` dies, its PTY masters close, and the kernel
  hangs up every chat's terminal. The harnesses end. That is the honest cost of one host process,
  and V7 accepts it. What charter owes after it is FD-29's: the host journals each run's
  conversation id (ADR 0066's `conversation` attribute) as it learns it, so the restarted host marks
  every run that was `working` as `failed(host-crash)` and offers a one-click resume through the
  harness's own resume. A chat on a harness-native host (FD-20) is re-attached, not resumed.
- **A `kill -9` chaos test runs in CI** on macOS and Linux: kill `charterd` with 20 chats open, and
  every chat is back within 30 seconds or offered with its reason (FD-29's acceptance).

### 4. Two protocols over any ordered byte stream (FD-4, FD-26)

- **The session protocol is small, public and versioned.** It carries events and about fifteen
  commands: list, attach with a snapshot, write, resize, answer, stop, start, and
  `subscribe(since)` from ADR 0066's cursor. It carries the compatibility promise (E5): a client and
  a host one version apart (N−1) always talk. LV-2a publishes it as its own MIT crate, and
  `charter attach` (FD-21), the fleet MCP (HP-20) and a runner's link (V9) are its clients.
- **The UI RPC is private.** It is what the window needs beyond the session protocol, generated
  from `ipc_commands.rs` with a typed TypeScript client (FD-4). It ships with the app, changes with
  the app, and is excluded from the compatibility tests. The app and the host it starts are always
  the same version, except across an upgrade (section 7), which the session protocol alone carries.
- **The transport is any ordered byte stream:** a unix socket, a named pipe on Windows, or a
  child's stdio (`ssh`, `docker exec`, `kubectl exec`). Nothing in either protocol assumes a local
  socket. That is what lets a runner be the same host behind a connector (section 8).
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

### 5. Every connection is authenticated, and a chat never holds a human power (Q2, V16, FD-6, FD-27)

**Each connection is authenticated twice: by the peer's uid, and by a credential for one client
scope.** A connection from another uid is refused before it says anything. The `0700` directory
already keeps other users out of the socket, and the uid check still holds if that directory is
ever wrong. A connection that presents no credential, or the wrong one, is refused. There is no
anonymous scope, not even for listing.

| Scope | Who connects | Where | Its credential | What it may do |
|---|---|---|---|---|
| `chat` | a hook, or a `charter` command, inside one chat | that plane's hook socket | the chat's own token (#535), minted at the `exec` | report, ask for a ticket and open or report back (as today), ask for a secret (the gate decides), and tell the host of a run change (ADR 0066's `charter persona use`). Nothing else |
| `local-ui` | the app's window | `charterd.sock` | the `local-ui` credential | everything the host offers, including settings writes and vault values |
| `terminal` | `charter attach` and `charter ls` from the operator's shell (FD-21) | `charterd.sock` | the `terminal` credential | the session protocol on the operator's chats |
| `fleet-mcp` | the fleet MCP server (HP-20) | `charterd.sock` | the `fleet-mcp` credential | the capability set FD-27 gives it |
| `approval` | `charter inbox` and approvals from a shell | `charterd.sock` | the `approval` credential | answer needs-you and V15 approvals |
| `remote-link` | a peer device through a connector (V9) | the connector's stdio | the device key, inside the stream (RR-13) | the session protocol. **Never vault values, and never settings writes** (V7) |

- **The human scopes' credentials are files in the machine store** (ADR 0034's directory, under
  `charterd/`), one per scope, each `0600`. `charterd` mints them fresh every time it starts, so a
  credential lives only as long as the host that minted it. That is Q2's "per-session token". The
  app reads its own like any other client, so there is one mechanism, whoever started the host.
- **A chat's sandbox denies that directory**, which is ADR 0067's denial class 3. **A chat's
  sandbox also denies connecting to `charterd.sock` at all.** A chat reaches the host only through
  its plane's hook socket, where the only scope is `chat`. So the human scopes are not merely
  refused to a chat; they are on a socket the chat cannot open. ADR 0067 §2's *"asked of `charterd`
  over its socket"* is the hook socket.
- **`charterd` also refuses a human scope to a connection from a chat's process tree** (V16a). It
  reads the peer's pid from the socket and refuses when the peer's session id is one of its chats'
  (every chat's program leads its own session, since `portable-pty` starts it with `setsid`), or
  when a chat's program is among the peer's ancestors. **This check is a second layer, not the
  boundary.** A process can leave its session and its parent on purpose, as `session.rs` already
  notes of daemons. The boundary is the credential the sandbox keeps from the chat. The check
  catches the ordinary case, such as a harness that runs `charter inbox` itself, and says so in the
  refusal.
- **An agent can never answer its own asks or approve its own secret requests.** Answering and
  approving are `approval` and `local-ui` capabilities only. The `chat` scope can ask and wait. It
  cannot answer.

### 6. The hook channel keeps its line (FD-6, FD-30)

- **The line does not change.** A hook still writes one JSON line with its chat number and its
  token to the path in `$CHARTER_HOOK_SOCKET`, inside the spec's 50 ms. ADR 0066 already fixed this.
  What changes is who listens: `charterd` binds each open plane's hook socket, at the path
  `hooks::socket_for` gives today, and holds `ChatTokens`.
- **The hook wire supports N−1** (FD-28): a hook of the previous version is served by the new host.
  A hook carries no version today, so the first release with `charterd` adds an optional `v` to the
  line, and an absent `v` reads as the version before it.
- **When the socket refuses, the hook spools** (FD-30). It appends its line to a per-plane spool
  file within 50 ms, with a per-chat sequence number, and `charterd` drains the spool when it
  starts, recording any gap in the sequence. Audit never silently loses an event (O1).
- **A hook that runs inside the sandbox can write the spool, so the spool is tamper-evident rather
  than unwritable.** Claude Code runs its hooks outside the sandbox it gives its own tools, so there
  the spool is simply denied to the chat, as ADR 0067 §5.2 says. Codex and opencode run under
  charter's own wrap, hooks included (ADR 0067 §2), and a hook that must append to a file can append
  to it. So each spooled line carries a MAC keyed by the chat's own token, and its sequence number.
  A chat cannot write a line another chat's token would verify, and a line it removes or rewrites
  shows as a gap or a failed MAC, which the host records. FD-30's test *"a chat cannot write the
  spool"* becomes *"a chat cannot forge another chat's line, and cannot remove its own unseen"*.
  This is for the operator's ruling (item 5).

### 7. An upgrade hands the terminals over, and drains only when it must (V7, FD-28)

**The normal path hands every live PTY master to the new host over `SCM_RIGHTS`.** A PTY master is
a file descriptor, and a unix socket can pass one to another process. The program on the other side
of the terminal never notices. This is the pattern nginx and HAProxy use for binary upgrades, and
systemd's descriptor store uses for restarts. `rustix` has a safe API for it, so it needs no
`unsafe`.

The sequence:

1. The supervisor starts the new host with `--take-over`, and connects it to the old host over a
   `socketpair` whose other end it passes to the old host on its `local-ui` connection.
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
   old host exits without ending any program. It needs a way out that is not `Drop`, since dropping
   a `Session` hangs up its program: `Session` gains a consuming hand-over that gives up the master
   and the child without ending either.
5. **Clients reconnect.** The window, `charter attach` and a runner's link each see their
   connection close, reconnect, resubscribe from their cursor (`subscribe(since)`) and re-attach
   their views with a snapshot. Their connections are not handed over: a reconnect is the path
   every client needs anyway, for a crash and for a dropped view (section 4).

FD-28's acceptance holds it: an update with 20 live chats changes the host's pid, and every chat
survives, mid-turn included.

**What a handed-over program loses: its exit status.** It is still the old host's child, so when
the old host exits, the operating system reparents it, and the new host cannot `wait` for it. The
new host learns that it ended the way `session.rs` already learns it first, from the end of its
output, and reports the end without a code. The group it may still signal is checked first (its
session id is still the chat's, and the chat's terminal is still open), because the leader is no
longer held unreaped by a parent that keeps its id from being reused.

**The fallback is drain and resume.** It is used when the handoff cannot run: on Windows (below),
when the new host cannot read the old host's state (the state is private between adjacent versions,
and a version that changes it past N−1 must say so), or when the handoff fails or does not finish
within its deadline. Then:

- the old host starts no new turn, and each chat is ended at its next turn end, which its hooks
  report;
- the new host starts, and each ended chat comes back through its harness's own resume, as
  **Reopen all** does today (`reopen.rs`, `sessionresume.rs`);
- a shell tab is not a harness and has no resume. It is named in the window before the fallback
  starts, as a mid-turn chat is today.

**A security release forces the upgrade within a window** (proposed: 24 hours from install). The
handoff loses nothing, so it runs at once. If only the fallback is possible, chats still mid-turn
when the window closes are ended and resumed, and each loses its turn in flight, named in the window
first.

### 8. A runner is the same host behind a connector (V9, RR-13)

A runner runs the same `charter serve`. The app reaches it through a connector command's stdio
(`ssh`, `gh cs ssh`, `coder ssh`, `docker exec`, `kubectl exec`), and the session protocol runs
over that stream unchanged. The connection's scope is `remote-link`, authenticated by the device key
inside the stream, not by anything the transport provides. On the runner, the host is supervised by
a systemd user unit with linger (RR-15), and the chat sandbox denies its install files and the git
internals it works on (V16d, ADR 0067 §5.4). Its versions sit side by side under
`~/.charter/server/<ver>/`, and an old one drains its sessions and is never killed (RR-14). RR-13
decides the rest. This record only guarantees that nothing in the host assumes the window is on
the same machine.

### 9. Descriptors (V7)

A host with 200 chats holds several descriptors per chat: the master, the reader's and writer's
copies, each view's stream, and the hook connections. macOS starts a GUI process with a soft
`RLIMIT_NOFILE` of 256. **The app and `charterd` each raise the soft limit to the hard limit when
they start**, through `rustix::process::setrlimit`, which is safe. `stress.yml` asserts the
descriptor counts (V8).

### 10. Windows

Windows is not ported yet (`updating.md`), and `hookwire` refuses there today (charter-app#95).
When it is, the host's transport is a named pipe, and its upgrade is drain-and-resume only. A
ConPTY is a pseudoconsole owned by a `conhost` process that is tied to the process that created it,
so there is no descriptor to hand over. The rest of this record holds unchanged there.

## ADR 0025, amended

ADR 0025's bullet *"No daemons. Every session is a child of the app. Quitting ends them all… When
the app starts again, it reopens chats through each harness's own resume, not through reattach"*
becomes:

> **One session host.** Every session is a child of `charterd`, which the app starts and
> supervises. Quitting still ends them all, unless the operator has a trigger or a paired device
> (ADR 0068). An update or a crash of the app ends none of them: the app reattaches. Resume is for
> what a crash of the host itself ended.

The rest of ADR 0025 stands. In particular, the core still owns every terminal's state headless,
the window still draws only the panes on screen, charter still never parses a harness's output, and
there is still no terminal frontend: `charter attach` is a client of the host, drawing through the
operator's own terminal (FD-21), not a second UI charter maintains. `docs/spec.md`'s *"with no
daemon"* goal (line 20) and its 50-session limit (line 408) change with FD-1's scope record, which
supersedes those lines.

## What changes where

The code does not change with this record. Each row is what the tickets after it change:

| Code as it is | What changes |
|---|---|
| `app/src-tauri/src/sessions.rs`, `hooks.rs`, `planewatch.rs`, `autosave.rs` | They leave the app crate, because `charter serve` is in the `charter` binary, which must not link the Tauri app crate. They go into a new workspace crate, `charter-host`, which depends on `charter-core` and never on Tauri. The CLI links it for `charter serve`, and the app links it for the in-process fallback (section 3) and the tests. FD-3 extracts `trait SessionHost` first, in place, with no behaviour change |
| `session.rs`: `impl Drop for Session` ends the program | Kept. `Session` gains a consuming hand-over that gives up the master and the child without ending them (section 7). The module's first paragraph (*"there is no daemon"*) is rewritten to say the host is the owner |
| `session.rs`: `Session::attach` gives each view an unbounded `mpsc::channel` | A byte-bounded queue. A view that falls behind is dropped and re-sent a snapshot (FD-4) |
| `hookwire.rs`: the line, `ChatTokens`, `Listener::bind` | The line is unchanged, and gains an optional `v`. `ChatTokens` is held by the host and handed over on an upgrade. `Listener` accepts an inherited listening socket as well as binding one. The spool and its MAC are FD-30's |
| `updates.rs`: `install_update`, `Planes::let_go_of_all_to_update` | An app update tells the host it is restarting, and ends no chat. A host update runs section 7. `mark_restart_to_update` and **Reopen all** stay for the fallback and for a crash of the host |
| `lib.rs`: `Sessions` and the hook listeners built at start | The app connects to `charterd`, starting it if none answers. It raises `RLIMIT_NOFILE` first |
| `ipc_commands.rs` | The source the UI RPC and its TypeScript client are generated from (FD-4) |
| Where the `charter` binary runs from | In place from the app bundle on macOS and from `/usr/bin` with the `.deb`. An AppImage's mount goes away when the AppImage exits, taking the running binary's pages with it, so there `charterd` runs from a copy in a versioned directory under the machine store, as RR-14 installs it on a runner |

## What this costs

- **A second process, and a socket between the window and the terminals.** Every keystroke and
  every frame crosses it. A local unix socket adds microseconds, far inside ADR 0026's limits, and
  FD-5 re-measures every one of them through the host in CI before it ships.
- **A crash of `charterd` still ends every agent.** One host process is one point of failure. The
  supervisor, the journal and one-click resume make the failure short and visible; they do not make
  it free. The alternatives that avoid it are under *What was rejected*.
- **A handed-over program's exit code is lost** (section 7).
- **A chat whose sandbox the operator turned off can read the human scopes' credentials.** ADR
  0067 §7 lifts denial class 3 with the rest, and the process-tree check alone does not hold
  against a process working to escape it. The chat's badge already says it is unsandboxed. This
  record adds nothing to hide it.
- **Agents can outlive the window by up to the grace window** after a crash, with nothing on screen
  saying so. That is the price of not losing them to a crash.

## What was rejected

- **No host: keep sessions in the app.** It keeps ADR 0025's simplicity and keeps the worst
  reliability property at scale. Q2 ruled against it.
- **A host process per chat** (the containerd-shim or conmon pattern): each chat's PTY held by a
  small process of its own, so a crash of the main host ends nothing. It is standard practice, and it
  costs 200 more processes at Q1's target, each needing its own authentication and upgrade path. V7
  chose one host with descriptor handoff. The per-chat shim stays the answer to reach for if the
  crash rate FD-29 measures ever says a host crash is common.
- **The supervisor holding a copy of every master** (systemd's descriptor store) so a host crash
  ends nothing. On macOS the supervisor is the app, and the app holding the masters is the property
  this record removes. On Linux it is worth measuring later, as an addition that needs no change to
  this design.
- **A separate `charterd` executable.** Q2 said "same binary". One binary is one thing to sign,
  version and verify (ADR 0042), and the CLI and the host can never be of different versions.
- **One host per plane.** It would give one device several event-log writers, several supervisors
  and several upgrade handoffs, and ADR 0066 already assumes one host holds several planes.
- **Handing client connections over on an upgrade.** Every client already needs to reconnect after
  a host crash. A second path for the upgrade would be the one that is rarely exercised.
- **A loopback TCP port.** Any local user can connect to one, and a peer uid is not available on
  it. A unix socket in a `0700` directory is the standard answer.

## For the operator's ruling

These calls go beyond the words of Q2, V7, V9, V16 and X24, and each is worth a yes or a no:

1. **One host per OS user per device, serving every plane** (section 1). The alternative is one per
   plane, rejected above.
2. **A crash of the app keeps the agents for a 10-minute grace window**, then ends them as a quit
   would (section 2). X24 says the host "exits with the app" when there is no trigger or device, and
   does not say what happens when the app did not choose to exit. The alternatives: end them at once,
   as a quit would (a crash of the window then still ends every agent), or keep them until the next
   launch with no limit (agents then run unseen for as long as nobody opens charter).
3. **When the crash-loop breaker trips, the operator can run the host in-process for that launch**,
   which is charter as it is today, with a badge (section 3). The alternative is no charter until the
   host starts, which is simpler and leaves the operator with nothing.
4. **A sixth scope, `chat`**, is named for the hook channel, beside FD-27's five, and a chat's
   sandbox denies it `charterd.sock` altogether (section 5). This makes the hook socket ADR 0067's
   "its socket", and puts every human scope on a socket a chat cannot open.
5. **The hook spool is tamper-evident, not unwritable, where the hook runs inside charter's own
   wrap** (section 6). It amends FD-30's acceptance line. The alternative, a spool a sandboxed hook
   cannot write, would leave Codex and opencode hooks with nowhere to go while the host restarts.
6. **Clients reconnect across an upgrade**, and a handed-over program's exit code is lost (section
   7).
7. **A security release forces the upgrade within 24 hours**, and only the drain-and-resume
   fallback can cost a turn (section 7). FD-28 asks for "a window" and gives no length.
8. **The host code moves into a new crate, `charter-host`** (*What changes where*), rather than into
   `charter-core`, which would then own threads, sockets and a server loop.
9. **What stays in the app** (section 1): plane reads, settings and the vault view stay in the app's
   process, and only what must outlive the window or stay out of a chat's reach moves into the
   host.
