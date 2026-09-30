# A runner is `charterd` behind a connector, and charter's own keys say who is on the link

**Proposed 2026-10-01**, drafted for program-map ticket RR-13 (#722), for the operator's ruling.
It follows these of the operator's rulings:

- **V9:** *"A runner is any host running `charterd`, reached through a **connector command**
  (`ssh`, `gh cs ssh`, `coder ssh`, `docker exec`/devcontainer, `kubectl exec`; the relay
  later). Charter bootstraps a verified `charterd` of the matching version; it survives an SSH
  drop as a systemd user unit (linger) or equivalent."* The same ruling sends code by git, forwards
  secrets from the desktop per command with an allowlist and step-up, keeps harness logins in the
  harness's own flow, runs the same sandbox on the runner, and makes the runner a device with its
  own audit chain.
- **Q2:** *"`charterd` session host, always on and supervised by the app. Per-connection
  authentication (peer uid plus a per-session token) from day one."*
- **V7**, in part: *"Transport is any ordered byte stream (socket, pipe, a child's stdio such as
  ssh), multiplexing many chats with per-stream flow control"*, and *"client scopes local-ui,
  terminal, fleet-mcp, remote-link, with vault values never reachable from remote-link"*.
- **V1**, in part: *"Device-key backends include an age-encrypted file for headless hosts, with
  rotation genesis entries."*
- **V15**, in part: *"On runners the approval always happens where the vault lives (the
  desktop), including for a resident store's secrets; with no reachable approver the request is
  denied."*
- **V16 (d):** *"On runners the chat sandbox denies `charterd`'s install files and the git
  internals `charterd` operates on."*
- **V19 (b):** *"RR-5 (runner sandboxing, including V16's denials of `charterd`'s install files
  and git internals) blocks RR-1's launch requirement, so the first runner slice never ships
  without it."*
- **V22a**, in part: *"a runner's host versions live under the machine store
  (`<config>/server/<ver>/`)"*.
- **V22d**, in part: *"the CLI stops and only the window re-arms"*.
- **E1**, in part: *"Every adapter seam is an extension point"*.
- **FI3:** *"The human's UI token never reaches an agent; agents keep their own narrow identity
  (SD-7a/b, gap G3), and gain no new power from this feature before those land."*
- **B1** and **W13**, in short: a feature that needs no server charter runs stays in the
  open-source build and is complete there. The relay and anything charter would host wait for
  the GT-CLOUD gate. Nothing in this record waits on it.

It builds on [ADR 0066](0066-a-chat-is-a-ulid-a-run-is-a-stretch-of-its-conversation-and-a-device-is-random.md)
(devices and the event cursor), [ADR 0067](0067-a-chat-runs-in-a-sandbox-charter-compiles-for-its-harness.md)
(the chat sandbox), [ADR 0068](0068-a-chat-lives-in-charterd-and-the-app-is-its-client.md)
(`charterd`, its scopes and its §8, which left the runner to this record),
[ADR 0069](0069-every-store-charter-writes-is-in-one-of-four-tiers.md) (tiers),
[ADR 0073](0073-a-harness-is-declared-as-data-and-a-chat-runs-it-at-one-of-three-levels.md)
(levels) and [ADR 0075](0075-an-audit-entry-is-metadata-in-a-store-of-its-own-and-telemetry-never-reads-it.md)
(the audit). It amends
[ADR 0042](0042-charter-updates-itself-and-nothing-it-cannot-verify-reaches-it.md), ADR 0066,
ADR 0067, ADR 0068, ADR 0069,
[ADR 0071](0071-the-kill-switch-is-machine-state-the-command-line-stops-and-only-the-window-re-arms.md)
and ADR 0075, each in a section of its own below. RR-1, RR-2, RR-3, RR-5, RR-14 to RR-23, RR-26,
QA-15, IB-12 and IB-13 build on it, and so will LW-1 (live sessions through the relay) and LW-2
(end-to-end encryption between one's own devices).

**Its concepts.** A runner is a **device**, and the connector and the link are how one device
reaches another, so all three belong to **Project**, where ADR 0072 §2 puts the device and the
machine store. Which runner a workspace's chats run on is a **Workspace** setting (RR-23). That
choice is the ticket's concept, and it is the only part of a runner a workspace holds.

## Where charter is today

charter has no runner, and no code that reaches another machine. `docs/spec.md` says *"Remote
sessions are not in v1"* and keeps sessions behind one interface so that SSH or devcontainers
can come later. Four accepted records already lean on this one:

- **ADR 0068 §4** makes the transport any ordered byte stream, a child's stdio included, and
  keeps the session protocol public and versioned (N−1), with `subscribe(since)`.
- **ADR 0068 §5** names a sixth client scope, `remote-link`, *"authenticated by the device key
  inside the stream (RR-13)"*, and gives it the session protocol.
- **ADR 0068 §8** has a runner run the same `charter serve`, drain instead of hand off on an
  upgrade, keep its versions under `<config>/server/<ver>/`, and deny its install files to chats.
  It ends: *"RR-13 decides the rest."*
- **ADR 0066** makes a runner a device with its own id, and turns the event cursor into a map
  from device id to `seq` once there is more than one device.

The program map lists the mechanisms one by one: SSH (RR-1), devcontainers (RR-3), the relay
(RR-2), hosted runners (RR-6), and, in the critique, Codespaces, Coder and Kubernetes. Each would
be an integration of its own without one model. And one filed row disagrees with a ruling: RR-14
installs to `~/.charter/server/<ver>/`, where V22a put the versions under the machine store.

## The decision

**A runner is a device running `charter serve`. The desktop's `charterd` reaches it through a
connector: a command whose stdin and stdout are a byte stream to the runner's host. Inside that
stream, the two hosts authenticate each other with keys charter pinned when the runner was added,
and encrypt everything after, whatever the connector is. The connector gives reachability and
never identity. Nothing on this path needs a server charter runs.**

### 1. Four layers, and what is not a layer

| Layer | What it is | Who decides its details |
|---|---|---|
| **L0 · the session protocol** | ADR 0068 §4's public protocol, unchanged, over the link of §3 | FD-4, FD-26, LV-2a |
| **L1 · the connector** | a command whose stdio reaches `charter bridge` on the runner (§2) | this record; presets in RR-1 and RR-3 |
| **L2 · bootstrap** | install a verified `charter` of the right version on the runner, and keep its host running (§5) | RR-14, RR-15 |
| **L3 · a provider** (optional) | an extension that creates, starts, stops and destroys a machine in the user's own account, and hands back a connector (§6) | RR-21 |

**Networks are not layers.** Tailscale, WireGuard, a VPN, a jump host, an IAP tunnel or AWS SSM
are how the user's own `ssh` already reaches a machine. charter runs `ssh` and inherits all of
them from `~/.ssh/config`, as Zed and VS Code Remote-SSH do.

### 2. A connector is a command, held as data

- **A connector is an argument vector,** to which charter appends the runner-side command,
  `<config>/server/<ver>/charter bridge`. charter never builds a shell string of its own on the
  desktop, and never runs a connector through a shell there.
- **Two places at the far end are shell by design, and are stated here:**
  - `ssh`, `gh codespace ssh` and `coder ssh` join their remote arguments into one string, which
    the remote login shell parses. So every argument charter appends is quoted for a POSIX shell,
    paths included, and those arguments are only charter's own: the bridge's path and fixed
    words. Nothing a project or a chat wrote is ever among them.
  - The bootstrap's probe (§5) is a POSIX `sh` script, sent the same way and quoted the same way.
- **Presets, each with every forwarding its tool offers turned off:**

  | Preset | Connector | Forwarding |
  |---|---|---|
  | SSH | `ssh -T <alias> --` | `-o ForwardAgent=no -o ForwardX11=no -o ClearAllForwardings=yes -o Tunnel=no` |
  | Codespaces | `gh codespace ssh -c <name> --` | the same `-o` options, passed through to the `ssh` it runs. `gh`'s own port forwarding (`gh codespace ports`) is a separate command that charter never runs |
  | Coder | `coder ssh <workspace> --` | forwards nothing unless asked: charter passes none of `--forward-agent`, `--forward-gpg` or `--remote-forward`, and clears their `CODER_SSH_*` environment variables. RR-1 checks this against the Coder version it tests |
  | a container, or a devcontainer (RR-3) | `docker exec -i -u <user> <container>` | nothing to turn off. `-u` names a non-root user, because one OS user is one person (§3) |
  | Kubernetes | `kubectl exec -i <pod> [-c <container>] --` | nothing to turn off. The user is the container's, set by the pod's security context; `charter runner doctor` warns when it is root |
  | anything else | an argument vector the operator types | the operator's to set. Doctor says it cannot check it |

  `charter runner doctor` warns when the user's own configuration turns a forwarding back on for
  that host. A runner never receives a way to act as the operator's SSH or GPG identity.
- **`charter bridge` is a pipe, not a host.** It connects its stdio to the runner's
  `charterd.sock` as the same OS user, starting the host under its supervisor (§5) when none
  answers. The link's handshake (§3) ends in the runner's host, not in the bridge, so the bridge
  holds no key and makes no decision.
- **A runner is defined only on the machine that uses it.** A connector runs a program on a
  click, which is ADR 0022's reason for asking before anything a project declares runs. So the
  definition, with its connector and pinned key, is Machine state on the desktop (§8), written
  only from the window or from `charter runner add` on a human scope. **A project may name a
  runner, and never define one.** RR-23's workspace default names a runner by name. A name this
  machine does not know starts no chat, and the window says which runner is missing and offers
  to add it.
- **The relay is a second kind of connector, built in rather than a command**, once GT-CLOUD
  opens (W13, LW-1). A hosted runner is then a provider that charter operates, reached through
  the relay. Both use the link of §3 unchanged, so nothing on the connector path changes when
  they come.
- **A connector that fails shows its own words.** Its stderr is shown in full when the link does
  not come up, since that is where `ssh` explains a host key or an authentication failure.
  charter never parses it to decide anything.
- **Reconnect is the desktop's.** A dropped connector is retried with backoff, the runner's tabs
  say *reconnecting…*, and the SSH preset sets `ServerAliveInterval`. On reconnect the desktop
  resubscribes from its cursor and re-attaches its views with a snapshot (ADR 0068 §4). The
  runner's chats never notice. There is no UDP transport: persistence comes from the host owning
  the sessions, not from the connector.

### 3. The link: charter's keys inside the stream

ADR 0068 §5 already decided that `remote-link` is authenticated *"by the device key inside the
stream"*, not by the transport. This section says how.

- **Every device has a link key**, a static X25519 key pair, minted with the device's first
  runner or first pairing. It is apart from AU-3's audit signing key, and it is the per-device
  X25519 key that LW-1 and LW-2 build on, so the relay adds no third key (question 1 of *For the
  operator's ruling*). Its private half is in the Keyring tier. On a headless runner it is held
  in the same age-encrypted form V1 gives the device key. Rotating it keeps the device id and
  needs a new pairing (ADR 0066, amended).
- **The handshake is Noise `XX`, and both static keys are checked against the pinned records.**
  Noise is the standard for authenticating two static keys over an untrusted stream: WireGuard,
  libp2p and the Lightning Network use it, and the Rust implementation is the `snow` crate. The
  cipher suite is `25519_ChaChaPoly_BLAKE2s`, and a change of suite is a protocol version.
  - **Why `XX` and not `KK`.** A runner has several peers, one per device of its owner. `KK`
    makes the responder know the initiator's key before the first message and never sends it,
    so the runner would need a hint in the clear to choose a peer, and a refusal could not say
    which key was offered. `XX` sends each side's static key encrypted inside the handshake. The
    runner looks the desktop's key up in its peers after decrypting it, and the desktop compares
    the runner's key with the one it pinned. Either side aborts on a mismatch before any
    transport message. It costs one more half round trip once per link, which is nothing next to
    a connector's own login. LW-2 already names `XX` per device pair, so the relay and a
    connector run one handshake.
  - **The prologue binds the link's context:** the protocol name and version, and the runner's
    device id as the desktop expects it. The runner puts its own device id in the same place, so
    a connector that reaches a different runner than the definition names fails the handshake,
    even when that runner would accept the desktop's key.
- **The stream is encrypted even when the connector already is.** Over SSH that is encryption
  twice, and it costs little CPU. It keeps one path for every connector, including those whose
  stream passes through a party the operator does not run (a Kubernetes API server, a vendor's
  gateway, later the relay). A forwarded secret (§7) then crosses every connector the same way.
- **Pairing happens once, over the connector, when the runner is added.** `charter runner add`
  bootstraps the runner (§5), then asks the runner's host, through the bridge, to add the
  desktop's public link key to its peers and to return its own device id and public link key.
  Both are pinned: the desktop in its runner definition, the runner in its peers file (§8). The
  connector's own authentication, the user's SSH key or `gh` login, is what vouches for this one
  exchange. After it, the transport vouches for nothing: a link whose keys do not match is
  refused, whatever the connector.
- **The same two pinned records, whatever route pairs them.** Pairing through the relay later
  (LW-1, RR-2) must produce exactly these two records, so a runner paired over SSH and later
  reached through the relay is one runner with one pairing.
- **The runner's host refuses a handshake whose key is not in its peers**, and records the
  refusal with the offered key's fingerprint, which `XX` has delivered by then (§10). Removing a
  runner removes its row on the desktop and, when the link is up, the desktop's key from the
  runner's peers. `charter runner peers` on the runner lists and removes peers from a shell on
  that machine.
- **One OS user on a runner is one person, and its peers are that person's devices (RR-26).**
  Pairing needs the connector's own login as that user, so charter adds no second way in. A
  second person on a shared machine uses a user of their own, and the docs say so (V9: a shared
  runner never lets two humans use one subscription). `charter runner doctor` names each peer it
  holds, so a peer nobody expected is visible. A device outside the owner's set is refused, and
  team grants wait for GT-CLOUD.

### 4. Who may do what over the link

**The link is asymmetric. The desktop is always the client.** The desktop's `charterd` opens it
and holds `remote-link` on the runner's host. The runner never holds a scope on the desktop's
host. What the runner sends back is events on a connection the desktop opened: output, state,
asks and requests. The desktop's host decides what to do with each.

- **The link lives in the desktop's `charterd`, not in the app.** A runner's asks and secret
  requests must reach the desktop's host while no window is open, which is ADR 0068 §1's rule
  for what the host holds. The window reaches a runner through its own `local-ui` connection to
  its own host, which relays. Holding a link does not keep the desktop's host running: see
  ADR 0068, amended.
- **What the desktop may send down the link depends on the scope that asked for it:**

  | Sent down the link | From which scope on the desktop |
  |---|---|
  | a stop | any scope, and the kill switch file (below) |
  | the session protocol: list, attach, write, resize, start | `local-ui` and `terminal` |
  | an answer to an ask | `local-ui`, `terminal` and `approval` |
  | what FD-27 gives `fleet-mcp` locally, and no more | `fleet-mcp` |
  | a re-arm, a chat's sandbox opt-out, removing a peer | `local-ui` only |
  | nothing | `chat` |

- **A stop needs no human.** Stopping only removes power, so a stop may go down the link whatever
  asked for it, as `charter stop --all` may stop a machine from any shell (ADR 0071). A stop that
  arrives over the link is recorded on the runner as `unattributed`, never `human`, because the
  runner cannot know who asked.
- **A re-arm and a sandbox opt-out come from the window only.** They restore or widen power, so
  they travel only when the operator acts in the window, through `local-ui`. The re-arm is an
  amendment to V22d's *"only the window re-arms"*, since a runner has no window of its own
  (question 2 of *For the operator's ruling*).
- **Why an agent on the desktop cannot act on a runner.** Three things hold it, not one:
  - the `chat` scope has no command that reaches a link;
  - the human scopes' credentials are denied to a chat's sandbox, and `charterd` refuses those
    scopes to a connection from a chat's process tree (ADR 0068 §5, V16a);
  - the link key is denied to a chat's sandbox (ADR 0067, amended), so a chat cannot open a link
    of its own.
  On the runner, a chat cannot reach its host's human scopes either (ADR 0068 §5, unchanged), and
  its peers file is denied to it, so it cannot add a device that could.
- **Vault values are never requestable over `remote-link`.** The runner cannot ask the desktop's
  host for a value as a command in any scope, because it holds none. A value crosses the link
  only when the desktop's host pushes it, for one command it has approved at its own gate (§7).
  That is how this record reads V7's *"never reachable from remote-link"*: reachable means
  requestable (ADR 0068, amended).

### 5. Bootstrap: a verified host of the matching version, kept running

**Settled by V9 and V22a**, applied. RR-14 and RR-15 build it.

- **Where:** `<config>/server/<ver>/charter` on the runner, `<config>` being the machine store as
  the runner resolves it (`$CHARTER_CONFIG_HOME`, else `$XDG_CONFIG_HOME`, else `~/.config`, then
  `charter/`). RR-14's row, which said `~/.charter/server/<ver>/`, follows V22a.
- **What:** the same `charter` binary the desktop ships, built for the runner's platform, as a
  release asset of its own: Linux x86_64 and aarch64, and macOS arm64. Each is signed with
  minisign by ADR 0042's key (ADR 0042, amended). A Windows runner waits for the Windows port
  (ADR 0068's *Later decisions*).
- **Verified on the desktop, checked on the runner before its first run.** The desktop verifies
  the asset's minisign signature with the key ADR 0042 commits, and only then trusts its SHA-256.
  The asset reaches the runner one of two ways: the runner downloads it from GitHub Releases, or
  the desktop uploads it through the connector for a machine with no outbound access. Either way
  it lands in a temporary name in `<config>/server/`, not yet executable. **No `charter` exists
  on the runner yet to check it, so the check is the probe's:** the desktop runs `sha256sum`,
  or `shasum -a 256` where that is what the platform has, over the file through the connector,
  compares the result with the digest it verified, and only then has the probe mark the file
  executable and move it to its version's directory. A mismatch deletes the file, and the runner
  is not added.
- **The trust this rests on.** The runner's own coreutils, and the operator's login on the
  runner, are trusted to report a digest honestly. A runner whose operating system lies about a
  file's hash can run anything it likes as that user already, so the check guards the file's path
  from the release to the runner, not the runner from itself. The trust root, ADR 0042's key,
  stays on the desktop, and the runner needs none of its own to start.
- **Versions side by side, and the old one drains.** The desktop uses a runner host of its own
  version. A new version takes new chats, and the old host keeps its chats until each ends and is
  never killed (ADR 0068 §8). The session protocol's N−1 promise (E5) covers the overlap.
- **The host stays up with no client (RR-15).** A `systemd --user` unit with linger on Linux (the
  bootstrap asks once, and says why), a LaunchAgent on macOS, and `charter serve --foreground`
  in a container, whose runtime is the supervisor. See ADR 0068, amended, for what that changes.
- **The probe is small, and changes nothing but the one file.** Before installing, the desktop
  runs the probe through the connector to report the platform, the resolved `<config>`, the
  versions present and whether linger is on. Its only write is the install step above.

### 6. Providers are optional extensions that hand back a connector

**Settled by V9 and E1** (every adapter seam is an extension point). RR-21 builds it.

- **A provider is an extension on one seam:** `create`, `start`, `stop`, `snapshot`, `destroy`
  and `status`, and it returns a connector. The generic provider is any host already in
  `~/.ssh/config`, with nothing to create. The first cloud provider is Hetzner, then AWS, GCE,
  OCI and Fly. *Not binding:* RR-21 may add a call for prices when it builds its cost guard.
- **The provider's token stays in a vault on the desktop**, and the calls go from the desktop to
  the provider's own API. The machine it creates gets the user's SSH key and nothing else from
  cloud-init. charter's bootstrap (§5) then runs as on any other host.
- **Idle shutdown and the cost guard are RR-21's**, from V9's row: stop after a period with no
  running chat and no client, snapshot-and-delete where a stopped machine still bills, a monthly
  cap with a hard stop, and destroy on workspace remove.
- **Short-lived sandbox services are not runners.** A runner is a host that holds sessions for
  days. Services whose machines live for minutes are revisited as providers for hosted runners,
  after GT-CLOUD.

### 7. What crosses the link

| What | How | Record |
|---|---|---|
| Terminals, state, asks, answers | the session protocol (L0): per-chat streams with credit, a priority control lane, a bounded view that is dropped and re-snapshotted (ADR 0068 §4) | FD-4 |
| Events | the desktop subscribes with a cursor per device (ADR 0066). The runner's host is the only writer of the runner's event log. The desktop never copies the runner's events into its own | ADR 0066 |
| Code | git's own pack protocol, carried as one more stream of the link, to a bare repo per workspace repo that the runner's host keeps. The desktop pushes, with snapshot refs for uncommitted work, and fetches into `refs/remotes/<runner>/*`. Saves and pull requests happen on the desktop, so the runner needs no forge credential (FI3) | RR-16 |
| The project | the same way as code: records fast-forward, and persona, `workspace.md` and shared-memory changes made on the runner arrive as proposals | RR-17 |
| Secrets | a runner chat asks its own host; the host has no vault, so it sends the request up the link; the desktop's host applies V15's gate there, and pushes the value for that one command, bound to the request and the chat. Nothing is kept on the runner. With no link up the request waits for V15's timeout, then is denied. A resident store is opt-in, and its approvals still happen on the desktop | RR-18, RR-19, SD-40 |
| The kill switch | a stop and a re-arm, as commands on the link (below) | ADR 0071, amended |
| Checkpoints of the runner's audit chain | pulled by the desktop on each connect | RR-22 |

**Which secrets a runner may ask for is RR-18's.** V9 gives forwarding a per-runner allowlist and
step-up on first use. This record fixes only the path above: the request goes up the link, and
the decision and the value stay with the desktop's host. RR-18 decides the allowlist's shape,
where it is kept, its tier, and how step-up meets V15's approval.

**Code goes over the link, never over a second connection.** Git's own SSH transport would work
for the SSH preset and for no other connector, and would put a second path into the runner's
bare repos beside the one the host guards. Carrying the pack protocol on the link keeps every
connector equal, and keeps the bare repos behind the host that V16d already denies to chats.
RR-16 picks how git on the desktop reaches the link: a remote helper, `git-remote-charter`, is
git's standard way.

**The kill switch reaches every runner the link can reach.** When this machine is stopped, by
**Stop all** in the window or by `charter stop --all` in any shell, the desktop's host sees the
switch (ADR 0071's watch) and sends a stop down every open link at once. Each runner's host
throws its own switch: its own `halted` and journal, with `by: link`. While the desktop is
stopped, a stop is the first thing any link carries when it opens, so a runner that was asleep
is stopped as soon as it is reached. A runner that cannot be reached keeps its agents running
until it is, and the window names it as *not reached* (question 3 of *For the operator's
ruling*). On the runner itself, `charter stop --all` from a shell stops it as on any machine.

**The window's re-arm re-arms this machine and every runner the same stop reached**, and lists
them before it does. A runner stopped from its own command line, or one the re-arm cannot reach,
stays stopped, shows so on its row, and has a re-arm of its own there.

### 8. The stores, and their tiers

| Store | Where | Tier |
|---|---|---|
| the runner definitions | `<config>/runners.json` on the desktop: each runner's name, connector, preset, provider if any, the runner's device id and its pinned public link key | Machine, device-bound |
| the runner's peers | `<config>/peers.json` on the runner: each paired device's id and pinned public link key, and when it was paired | Machine, device-bound |
| the link key | the Keyring; on a headless runner, the same age-encrypted form V1 gives the device key | Keyring |
| the runner's host versions | `<config>/server/<ver>/` on the runner (V22a) | Machine, device-bound, rebuildable |
| the bare repos | `<data>/repos/<workspace>/<repo>.git` on the runner, in charter's data home (ADR 0075), one per workspace repo | Machine, device-bound, rebuildable. The desktop's clone is the truth, and a push rebuilds them |

**Both pinned records are device-bound.** A pinned key says which two devices trust each other,
and a second machine of the same person pairs on its own. A restore onto a machine that does not
replace the old one keeps neither (ADR 0069 §5). `docs/plane-format.md` records each store now,
marked **decided, not yet written**, with its tier, and ADR 0069's inventory gains them (ADR
0069, amended). RR-1, RR-14 and RR-16 move them to written.

**Chats may touch none of them.** The definitions hold a connector, which the host runs, and the
peers decide who gets a human scope on the runner. ADR 0067, amended, adds them to the classes a
chat's sandbox always denies.

### 9. Where the runner fits the rest

- **The sandbox is the same.** A runner chat compiles SD-2's policy on the runner. The runner's
  install files, bare repos and the git internals its host works on are denied to chats (V16d),
  and so are its audit directory, its keys and its peers. RR-5 blocks RR-1 (V19b).
- **Harness levels are the runner's.** A chat on a runner starts at the level its harness offers
  *on that runner* (ADR 0073 §2). A project's harness declaration is approved per machine
  (V24b), so a runner approves it once too, from the desktop's window over the link.
- **Harness logins stay in the harness's flow** (RR-20). A login happens in a shell tab on the
  runner, through the harness's own device or paste-code flow. charter never reads, copies,
  stores, vaults or relays a harness credential.
- **Needs-you with the desktop closed** goes through notifiers the user owns (IB-12) and
  `charter inbox` over any SSH client on the runner (IB-13), which answers asks through the
  runner's own `approval` scope. A secret approval is never answered there: it happens where the
  vault lives (V15).
- **The runner is an audited device** (RR-22). Its host writes its own chain, the desktop keeps
  copies of its signed checkpoints, and a destroy writes a final checkpoint first.

### 10. The audit, and who acted on a runner

- **The runner's host writes what it did, and the desktop's host writes what it did.** A secret
  request appears on both chains: its request and its use on the runner's, and its approval and
  forwarding on the desktop's (V9: audited on both chains).
- **A human act over the link is a `human` entry on the runner, by the runner's own local
  principal**, with the peer's device id in `meta`. These are the acts only `local-ui`,
  `terminal` and `approval` can send (§4). ADR 0066 keeps each device's local principal on that
  device, so the desktop's login name is never sent to the runner. The one-person-per-user rule
  (§3) is what makes the runner's own principal the right one.
- **A stop over the link is `unattributed`** (§4), with the peer's device id in `meta`.
- **New actions**, in ADR 0075's registry (amended below): `runner.added`, `runner.removed`,
  `runner.installed`, `link.peer.added`, `link.peer.removed`, `link.refused` and
  `secret.forwarded`.

## ADR 0042, amended

§1 and §3 sign what the updater installs: the app's bundles. They gain **one `charter` binary
per runner platform** (Linux x86_64 and aarch64, macOS arm64), published as release assets and
signed with minisign by the same key, with its public half the one `tauri.conf.json` commits.
The manifest names each asset's version as the updater's manifest does, so a runner's binary is
bound to the version it claims, as §3's `requireSignedVersion` binds the app's. The desktop
verifies every runner asset before it trusts its digest (ADR 0078 §5). A release that is missing
a runner asset, or its signature, fails as a bundle without one fails today. Nothing charter
cannot verify reaches a runner either.

## ADR 0066, amended

§ *A device is a random id* says a key rotation keeps the device id, and names one key, AU-3's.
**A device now has two keys, both apart from its id:** AU-3's signing key for the audit chain,
and the **link key** of ADR 0078 §3, a static X25519 key for links between devices. Both are in
the Keyring tier, both take V1's age-encrypted form on a headless host, and rotating either
keeps the device id. Rotating the signing key writes AU-3's genesis entry. Rotating the link key
unpairs the device from every runner, which the operator pairs again. Neither key is derived
from the other, and the device id is derived from neither.

## ADR 0067, amended

§5 class 2, *"Charter's integrity state is denied to chats"*, gains: **the device's link key, the
runner definitions (`<config>/runners.json`) and a runner's peers (`<config>/peers.json`)**. The
first would let a chat act as its device on a link, the second would let it change a command the
host runs, and the third would let it add a device to the runner's human scope. Class 4, on a
runner, gains **the bare repos the runner's host keeps** beside the git internals V16d names.
The classes stay classes, and SD-2 and RR-5 turn them into rules and tests.

## ADR 0068, amended

- **§2, the lifecycle, on the desktop. Adding a runner changes nothing about Quit.** X24's
  *paired device* is a device that connects **to** this host to watch or answer (the relay's
  phone and PWA, after GT-CLOUD). A runner is a device this host connects to, so a desktop with a
  runner is not a login item for that reason, and **Quit still ends every local chat and exits
  the host**, as ADR 0025's promise and §2's table say. Quitting closes the host's links. The
  runner's chats keep running on the runner, and its asks and secret requests wait there: an ask
  until the next link or `charter inbox` on the runner, a secret request until V15's timeout
  denies it. A desktop that is a login item for another reason (a trigger) keeps its links while
  it runs.
- **§2, the lifecycle, on a runner.** The table there reads the app going away three ways. On a
  runner there is no app, and a link closing is none of the three. **On a runner, no client's
  departure ends a session, and the ten-minute grace period does not apply.** The runner's host
  is always a login item under RR-15's supervisor, and runs until the operator stops it, a
  provider's idle policy stops the machine, or the kill switch stops its agents. For the runner,
  X24's condition is met: the desktop is a device that connects to it.
- **§5, the `remote-link` row.** *"The device key, inside the stream (RR-13)"* now reads: **the
  device's link key, in a Noise `XX` handshake checked against the keys pinned at pairing (ADR
  0078 §3)**. What it may do now reads: **what ADR 0078 §4's table sends: the session protocol,
  answers, the kill switch, and, from the desktop's window only, a re-arm, a chat's sandbox
  opt-out and the removal of a peer. The desktop always opens the link, and the runner never
  holds a scope on the desktop's host.**
- **§5, answering.** *"Answering and approving are `approval` and `local-ui` capabilities
  only"* now reads: **answering is an `approval`, `local-ui` or `terminal` capability on the
  desktop, and on a runner also `remote-link`, which carries only an answer one of those scopes
  sent from the desktop.** Approving a secret stays on the desktop's `approval` and `local-ui`,
  where the vault is (V15). The `chat` scope still asks and waits, and never answers.
- **§5, vault values.** *"Every other scope is refused them, … `remote-link` included"* stands,
  read as **reachable means requestable**: no scope, `remote-link` included, can ask a host for a
  value. The desktop's host may push one value down a link, for one command a runner chat asked
  it to run and its own gate approved (ADR 0078 §7). The push is the desktop host's act, not a
  capability of any scope.
- **§8, *"The app reaches it through a connector command's stdio"*,** now reads: **the desktop's
  `charterd` reaches it through a connector command's stdio, and the window reaches it through
  its own host** (ADR 0078 §4). *"RR-13 decides the rest"* is this record.

## ADR 0069, amended

- **§6**, the stores that records in flight add, gains the runner definitions, the runner's
  peers, the link key and the runner's bare repos, with the tiers of §8 above. The row for *"a
  runner's host versions"* stands.
- **The inventory, *Every store, by tier*,** gains these rows. Row 69 already holds a runner's
  `<config>/server/<ver>/`.

  | # | Store | Tier | Sync | Backed up | Rebuildable |
  |---|---|---|---|---|---|
  | 73 | `<config>/runners.json`, the runner definitions (ADR 0078) | Machine | device-bound | yes | no |
  | 74 | `<config>/peers.json`, a runner's peers (ADR 0078) | Machine | device-bound | yes | no |
  | 75 | the link key (ADR 0078) | Keyring | — | no | no |
  | 76 | `<data>/repos/<workspace>/<repo>.git`, a runner's bare repos (ADR 0078) | Machine, rebuildable | device-bound | no | yes |

## ADR 0071, amended

- **The switch reaches runners.** A stop on the desktop, from the window or from `charter stop
  --all`, is sent down every open link, and is the first command on any link that opens while
  the desktop is stopped (ADR 0078 §7). A runner that is not reached keeps its agents running
  until it is, and the window says so.
- **On a runner, the journal's `by` gains `link`.** A `link` stop comes from any scope on the
  desktop, and a `link` re-arm only from the desktop's window (ADR 0078 §4). A runner has no
  window, so a `link` re-arm is how it re-arms. This amends V22d's *"only the window re-arms"*
  to *"only a window re-arms: this machine's, or, for a runner, its desktop's"* (question 2 of
  *For the operator's ruling*). A `cli` line on the runner still only stops.
- **The window's re-arm lists the runners it will re-arm.** It re-arms those the same stop
  reached. A runner stopped by its own command line is re-armed from its own row.
- *"Not in scope: chats a `charterd` hosts are OV-2"* stands for triggered and headless chats. A
  runner's chats are reached by this amendment.

## ADR 0075, amended

The registry of §4 gains:

| Action | From | `meta` |
|---|---|---|
| `runner.added`, `runner.removed` | ADR 0078 §3 | runner name, preset, the runner's device id, the pinned key's fingerprint |
| `runner.installed` | ADR 0078 §5 | version, digest, `download` or `upload` |
| `link.peer.added`, `link.peer.removed` | ADR 0078 §3, on the runner | the peer's device id and key fingerprint |
| `link.refused` | ADR 0078 §3, on the runner | the offered key's fingerprint, the reason (unknown key, runner id mismatch) |
| `secret.forwarded` | ADR 0078 §7, on the desktop | vault, secret name, runner device id, the request it answers |

§2's actor rules gain:

- **A human act that arrives over `remote-link` is a `human` entry by the receiving device's own
  local principal, with the sending device's id in `meta`.** No device's principal is sent to
  another.
- **The kill switch's `by: link` maps by event:** a `link` stop is `unattributed`, because any
  scope on the desktop may send it; a `link` re-arm is `human`, because only the desktop's window
  sends it. Both carry the sending device's id in `meta`.

## What changes where

The code does not change with this record.

| Where | Change |
|---|---|
| `CONTEXT.md` | Gains **Runner**, **Connector** and **Link**; **Remote chat**'s avoid line points to **Runner** (in this PR) |
| `docs/plane-format.md` | The stores of §8 in the table of what charter-app keeps outside every project, each **decided, not yet written** with its tier; the kill switch's `by` gains `link` on a runner (in this PR) |
| ADR 0042, ADR 0066, ADR 0067, ADR 0068, ADR 0069, ADR 0071, ADR 0075 | Amended above. Their texts are left as accepted, and this record is the amendment |
| RR-14 | The install path is `<config>/server/<ver>/` (V22a); the probe's digest check before the first run (§5). The release gains a signed `charter` asset per runner platform (ADR 0042, amended); `release.yml` publishes Linux x86_64 only today |
| RR-1 | The SSH preset with its options, `charter runner add`, pairing (§3), the link on the desktop's host, and the scope table of §4 |
| RR-2 | Pairing through the relay produces the same two pinned records as pairing over a connector (§3), and uses the link unchanged |
| RR-3 | The container preset, with its named non-root user |
| RR-5 | ADR 0067's classes on the runner, including the amendment |
| RR-15 | The runner's supervisor and ADR 0068 §2 as amended |
| RR-16, RR-17 | Code and the project as streams of the link (§7) |
| RR-18, RR-19, SD-40 | Secret forwarding as §7 describes it. RR-18 owns the allowlist and step-up |
| RR-21 | The provider seam (§6) |
| RR-22 | The runner's chain, and the actor rules of §10 |
| RR-26 | Mostly decided by §3: one OS user is one person, the peers are that person's devices, a foreign key is refused and recorded, and doctor names every peer |
| OV-1 and OV-2 | The kill switch on the link (ADR 0071, amended) |
| LW-1, LW-2 | Build on the link key and Noise `XX` of §3, and produce the same two pinned records (questions 1 and 4 of *For the operator's ruling*) |
| `docs/spec.md` | *"Remote sessions are not in v1"* changes when RR-1 ships, not here |

## What this costs

- **Encryption twice over SSH.** ChaCha20-Poly1305 on a terminal's bytes is a small cost next to
  the network, and one path for every connector is worth it.
- **Pairing trusts the connector once.** Whoever can log in to the runner as that user when it is
  added can pair it. That is the trust the operator already gives SSH, and after it the keys decide.
- **The first install trusts the runner's own hash tool.** A runner that lies about a digest is
  already lost to that user, so this guards the path from the release, not the runner itself.
- **A runner cannot be stopped while it cannot be reached.** Its agents keep running until the
  link returns, and the window says so. `charter stop --all` in a shell on the runner is the
  fallback.
- **Quitting the desktop leaves runner chats running, and waiting.** Their secret requests are
  denied at V15's timeout until a desktop is back.
- **Secrets need the desktop awake**, unless the operator opts into a resident store. An
  unattended runner whose command needs a forwarded secret waits, then is denied.
- **Two hosts on a runner during an upgrade.** The old one drains, which can take as long as its
  longest chat.
- **A binary per runner platform.** The release gains assets, each signed and each tested on
  QA-15's runner.
- **The first install over a slow link is slow** when the desktop uploads, since the binary is
  tens of megabytes. A runner with outbound access downloads it instead.

## What was rejected

- **Forwarding the host's socket** (`ssh -L local.sock:remote.sock`). It needs streamlocal
  forwarding on both ends and stale-socket cleanup, it is unreliable on Windows' OpenSSH, and it
  cannot reach `kubectl exec`, `gh codespace ssh`, `docker exec` or the relay. Stdio into a bridge
  works through anything that runs a command.
- **The transport's authentication as the runner's identity.** It exists for SSH only, differs
  per connector, and would vanish with the relay. A pinned pair of charter keys survives a change
  of connector.
- **Noise `KK`.** It fits one peer. With several it needs a peer hint in the clear, and it never
  tells a refusing runner which key was offered (§3).
- **TLS with self-signed certificates instead of Noise.** It would work, and it brings
  certificate formats, expiry and name checks to a case with pinned keys and no names.
- **Authentication without encryption after the handshake.** It would leave a forwarded secret
  to the connector's protection, which differs per connector.
- **An integration per product** (Codespaces, Coder, Kubernetes, devcontainers). Each already
  gives a command with stdio, and a preset is all it needs.
- **rsync, Mutagen or a forge clone on the runner for code.** The first two copy files and not
  history, and the third needs a forge credential on the runner (FI3). A forge-clone mode comes
  with per-agent identities (SD-7a/b).
- **Git's own SSH transport for code.** SSH only, and a second way into the bare repos (§7).
- **A vault on the runner by default.** The OS keyring is usually missing on a headless machine,
  and a copy at rest on a machine the user may throw away is the wrong default. Forwarding is the
  default, and a resident store is opt-in (RR-19).
- **Connectors committed in the project.** A project could then ship a command that runs on a
  click (ADR 0022). A project names a runner and never defines one.
- **A runner making the desktop's host a login item.** Local chats would then outlive a Quit,
  which ADR 0025 and ADR 0068 §2 promise they do not.
- **Mosh, Eternal Terminal or a UDP transport.** The host owns the sessions, so a reconnect with
  a snapshot gives the same result.

## For the operator's ruling

1. **A link key of its own, or the device key?** RR-13's row says *"device-key auth inside the
   stream"*. This record gives each device a separate X25519 link key beside AU-3's signing key.
   The two keys have different algorithms and different jobs, and they rotate for different
   reasons. A key that only authenticates links can be re-paired without touching the audit
   chain. The link key is also the per-device X25519 key LW-2 names, so the relay adds no third
   key. **Recommended: a separate link key**, in the same tier and backend as the device key,
   and the one LW-1 and LW-2 build on.
2. **May a runner be re-armed over the link?** V22d rules that only the window re-arms. A runner
   has no window. **Recommended: yes, and only from the desktop's window through `local-ui`**,
   which amends V22d to *"only a window re-arms: this machine's, or, for a runner, its
   desktop's"*. The act still starts in a window, and neither `terminal`, `fleet-mcp` nor a
   command line can send it.
3. **A stop that cannot reach a runner.** A runner's agents keep running until its link returns.
   The alternative is a runner that stops its own agents when it has not heard from any peer for
   a set time, which would stop every unattended runner V9 exists for. **Recommended: no stop
   by silence.** The window names every runner a stop has not reached, keeps trying, and points
   at `charter stop --all` on the runner. An org policy may choose a timeout later.
4. **Merge this before LW-1 exists?** RR-13's row says it is *"reviewed with LW-1 and LW-19"*.
   Neither is written, and LW-19 is *PWA code integrity*, which pins a bundle hash at pairing and
   does not pair devices. The pairing through the relay is LW-1's and RR-2's. **Recommended:
   merge this on its own now, and put "produce the same two pinned records as ADR 0078 §3, over
   the same Noise `XX` link" in the acceptance of LW-1 and RR-2**, and "pins its bundle hash
   beside, never instead of, those records" in LW-19's, so each is reviewed against this record
   when it is written.
