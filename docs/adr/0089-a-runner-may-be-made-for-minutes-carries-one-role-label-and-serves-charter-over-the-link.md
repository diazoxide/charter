# A runner may be made for minutes, carries one role label, and serves charter over the link

**Accepted 2026-10-02** by the operator (rulings V46 to V55, V58 and V59), drafted for
program-map ticket RR-28. It amends [ADR 0078](0078-a-runner-is-charterd-behind-a-connector-and-charters-own-keys-say-who-is-on-the-link.md),
whose §6 said a short-lived machine is not a runner, so CONTRIBUTING's rule for a changed
decision makes it a record of its own. It follows these of the operator's rulings:

- **V46:** *"agents may start, stop and use remote compute in their workspace within the
  workspace's budget (ADR 0082: budgets, not caps). Each start is visible in the window and the
  event log, and the kill switch stops them all. Exposing one publicly or giving it credentials
  stays a human action (V16)."*
- **V47:** *"harnesses reach remote browsers through a browser MCP server that charter runs and
  brokers over the Link. The harness never gets the raw CDP or Playwright endpoint. Every browser
  is logged, stops with the kill switch, and is shown live in the window."*
- **V48:** *"grow **Runner** with labels, under three conditions:"*
  - *"ADR 0078 §6 is amended, so a runner may be short-lived and made by a provider."*
  - *"The role labels `harness`, `app` and `browser` are enforced capabilities: no harness on a
    hosted runner, and no vault credentials on a browser runner. Free-form tags sit beside
    them."*
  - *"Every runner runs `charter serve` over the Link, so shells, logs, ports and browser views
    are the same on every backend."*
- **V49:** *"local Docker and Podman ship in core. Kubernetes (agent-sandbox), cloud VMs and
  sandbox services are provider extensions on ADR 0078's provider seam. A devcontainer spec
  describes a runner's image."*
- **V50:** *"a **Runners view tab**, a view like the persona and changes views. It lists runners
  by label, state and cost, with live browser frames, shells, logs and forwarded ports for each.
  A chat's pane links to the runners it started, and their events go to the event log (ADR
  0075)."*
- **V51:** *"an `app` runner is started from a branch or commit of a workspace repo, then builds
  and serves that version. Its ports are forwarded to the desktop's loopback (RR-24) or brokered
  to browser runners. Many runners means many versions side by side."*
- **V52:** *"an app runner declares the vault entries it needs. A human approves each grant once
  per workspace, and charter injects it at start, never into images or logs. Agents may start
  runners that use approved grants but cannot add a grant (V16). Browser runners, and hosted
  runners with no harness, never get grants."*
- **V53:** *"a runner is private by default. Its ports are reachable only over the Link: on the
  desktop's loopback, and brokered to the workspace's browser runners. A public URL is a human
  action, logged, time-limited and revocable. Runner egress follows ADR 0067's presets, plus a
  `browser` lane (SD-31)."*
- **V54:** *"a runner belongs to the chat that started it, or to the workspace when a human pins
  it. It stops when its chat ends, after an idle timeout, or when the workspace budget runs out.
  The kill switch stops every runner, and a pinned runner survives in the Runners tab."*
- **V55:** *"a local runner defaults to a rootless container (Podman, or rootless Docker):
  capabilities dropped, no host mounts beyond its own work directory, and egress presets. On
  macOS that container sits inside the Docker or Podman VM. Providers may offer microVM isolation
  (Kata, Firecracker) as an option."*
- **V58**, the operator's ruling on this record's first draft, in part: *"(a) a provider's spend
  budget starts at zero; (b) exactly one role label per runner, with free-form tags unlimited;
  (c) hosted rules stay generic: a provider declares the labels it may make, and one whose
  machines the user doesn't own gets no `harness` label and no grants"*.
- **V59**, from this record's review:
  - *"(a) A grant reaches an app runner only when it builds a ref a human chose: a branch or
    commit the operator approved, such as main or a reviewed PR head. It never reaches an
    agent's own unreviewed branch, which runs with no grants or with test credentials."*
  - *"(b) Whether a provider's machines are the user's own is a per-provider setting the
    operator approves in the window. It defaults to not-owned, so the no-harness and no-grants
    rules apply until the operator says otherwise, and a provider's own claim never counts."*
  - *"(c) On app and browser runners, the workload runs as its own unprivileged user that cannot
    read `charter serve`'s key or state."*
- **D-ADR0089a**, the program's decision from the same review: *"Budgets count only runners an
  agent started, and stop only those. Pinned and human-started runners stop only by a human or
  the kill switch."* *"Idle means no running chat, no open stream, no request served and no
  build in progress. A harness runner lives while its chats run."* *"Pin, expose, grant and
  budget changes need the `local-ui` scope only."*
- **V16**, in part: agents never hold a human's powers.

It builds on ADR 0078 (the runner, the connector, the link, the provider seam),
[ADR 0066](0066-a-chat-is-a-ulid-a-run-is-a-stretch-of-its-conversation-and-a-device-is-random.md)
(devices and events), [ADR 0067](0067-a-chat-runs-in-a-sandbox-charter-compiles-for-its-harness.md)
(the sandbox and its egress presets), [ADR 0068](0068-a-chat-lives-in-charterd-and-the-app-is-its-client.md)
(`charterd` and its client scopes), [ADR 0069](0069-every-store-charter-writes-is-in-one-of-four-tiers.md)
(tiers), [ADR 0071](0071-the-kill-switch-is-machine-state-the-command-line-stops-and-only-the-window-re-arms.md)
(the kill switch), [ADR 0075](0075-an-audit-entry-is-metadata-in-a-store-of-its-own-and-telemetry-never-reads-it.md)
(the audit and its registry) and [ADR 0082](0082-charter-serves-one-persons-agents-first-and-its-scale-is-a-hot-chat-count-per-ram-class.md)
(budgets, never caps). It amends ADR 0067, ADR 0068, ADR 0069, ADR 0071, ADR 0075 and ADR 0078,
each in a section of its own below.

**Its concepts.** A runner is still a **device**, so it, its **label** and its **runner
provider** belong to **Project**, as ADR 0078 put them. In this record, *provider* always means a
runner provider, never a vault's provider (ADR 0047) or a model's (ADR 0087). A **grant** is
approved per workspace, so it is a setting of **Workspace**. The **Runners** view tab is a view
of **Workspace**: it shows the runners that workspace's chats started, the ones its workspace
owns, and the devices its chats run on. No sixth concept is added (ADR 0072 §2).

## Where charter is today

ADR 0078 made a runner a device running `charter serve`, reached through a connector and spoken
to over a Noise `XX` link. Its §6 ended: *"Short-lived sandbox services are not runners. A
runner is a host that holds sessions for days."* No runner code exists yet; RR-1 (#720), RR-5
(#721) and RR-14 to RR-16 (#723 to #725) are open.

What the operator asked for does not fit that sentence. Inside a workspace, many app servers,
many versions of one app and many browsers run away from the operator's own machine, started by
the operator or by a chat, and every one is visible in the window: its browser frames, its
shell, its logs. The program map names pieces of this in five places (RR-24's ports, RR-25's
presets, AC-15's browser lane, ED-10's preview pane, SD-31's `browser` lane) and joins them
nowhere. Without one model, each executor (a local container, a Kubernetes pod, a cloud VM)
would get a shell, a log stream and a port path of its own.

## The decision

**A runner is any device running `charter serve` over the link, for days or for minutes. A
long-lived runner is a machine the operator adds; a short-lived one is made by a provider, which
for a local container is charter's own core and otherwise an extension on ADR 0078's seam. Every
runner carries exactly one role label, `harness`, `app` or `browser`, which charter enforces, and
any number of tags in the operator's own words. Its ports, shells, logs and browser views reach
the window only over the link. It is private, it holds no vault value unless a human granted one
for a ref a human chose, it is owned by a chat or by its workspace, it stops with the kill
switch, and every start is in the event log.**

### 1. A runner lives for days or for minutes

**Settled by V48 (a).** ADR 0078 §6's last paragraph is replaced (ADR 0078, amended, below).

- **A long-lived runner** is ADR 0078's: a machine the operator adds with `charter runner add`,
  defined in `<config>/runners.json`, holding chats for days.
- **A short-lived runner** is made by a **provider** when a human or a chat asks for one, and
  destroyed when its owner is done with it (§9). Its record is a file of its own (§14), not a row
  in `runners.json`, because there may be many and they come and go by the minute.
- **Both are the same thing once made.** Both are devices with a random id (ADR 0066), both run
  `charter serve` (§3), both are paired by pinned link keys, and both are reached through a
  connector the provider hands back. Nothing above the connector knows which kind a runner is.
- **Pairing a short-lived runner happens at creation, over the connector its provider returned**,
  as ADR 0078 §3 pairs any runner. Its pinned key is kept in its record. A runner whose key does
  not match is refused, whoever made it.

### 2. Role labels and tags

**Settled by V48 (b).** A label says what a runner is for, and charter enforces it in
`charterd`, not in the window.

| Role label | What it may do | What charter refuses it |
|---|---|---|
| `harness` | run chats, as ADR 0078's runners do | nothing ADR 0078 does not already refuse |
| `app` | build and serve one version of a workspace repo (§5) | starting a chat; any vault value that is not an approved grant for a ref a human chose (§7) |
| `browser` | run browsers that charter's broker drives (§6) | starting a chat; every vault value, grant or not (§7) |

- **A runner carries exactly one role label.** A browser runner that could also serve an app
  would hold the app's grants beside a page an agent drives, which V52 forbids. A chat on a
  `harness` runner may still run a dev server, whose ports RR-24 forwards.
- **Both ends check the label.** The desktop's `charterd` refuses to send a chat start, or to push
  a vault value, down a link to a runner whose label forbids it. The runner's own host refuses
  them too, from the label it was created with. Either refusal is a `runner.refused` event (§13).
- **A provider declares the role labels it may make**, and charter refuses a runner whose label
  its provider did not declare. **Whether a provider's machines are the user's own is the
  operator's setting, never the provider's claim** (§4, V59 (b)). Until the operator marks a
  provider as owned, its runners may not carry `harness` and get no grants, whatever it
  declares (V48, V52). Hosted providers are a later provider extension, and this record is
  complete without one.
- **Tags are any words the operator picks**, beside the label (`gpu`, `node-22`,
  `staging-data`). charter enforces nothing about a tag. A persona or workspace may ask for a
  runner by label and tags, and the Runners tab filters by them.

### 3. Every runner runs `charter serve` over the link

**Settled by V48 (c).** A runner's shell, logs, ports and browser frames are streams of its link
(ADR 0068 §4's session protocol, multiplexed per stream), whatever made the runner. charter never
uses a backend's own exec, log, port-forward or preview-URL API to reach what runs inside it.

- **Why.** One authenticated path per runner, encrypted end to end by the link, with its N−1
  version promise, instead of a different API per backend and a different trust story for each.
  A Kubernetes API server, a container engine's socket or a vendor's gateway carries the
  connector's bytes, and cannot read or change what the link carries without failing its
  handshake. **That does not protect the runner from whoever runs its backend:** the engine's
  owner, the cluster's administrator or the machine's host can read and change what runs inside
  the runner, its memory and its files, as with any machine. The link protects the path, not the
  machine, which is why ownership is the operator's setting (§4) and grants follow it (§7).
- **New stream kinds, on the same protocol.** The session protocol gains a port stream (RR-24),
  a log stream (an app runner's build and serve output) and a browser view stream (§6). Each is a
  protocol version under FD-4's negotiation, which fails closed.
- **Logs are views, not telemetry.** An app's output and a browser's console are content. They
  are shown live in the Runners tab with a bounded scrollback, as a terminal is, and are not a
  fifth source for ADR 0083's pipeline. Making them one would be a change to that record.
- **`charter serve --foreground` is the container's first process**, with the container runtime
  as its supervisor (ADR 0078 §5). The app or the browser runs under it.
- **On an `app` or `browser` runner the workload runs as a user of its own** (V59 (c)): an
  unprivileged user apart from the one `charter serve` runs as, which cannot read the host's
  link key, peers file, configuration or data directories, all of which are the serve user's
  and closed to every other user. The connector's `-u` names the serve user; the app, its build
  and the browser run as the workload user. A chat on a `harness` runner is confined by ADR
  0067's sandbox instead, which already denies those files (§10).

### 4. Backends: Docker and Podman in core, everything else a provider extension

**Settled by V49.**

- **Docker and Podman are a core provider**, charter's own, with nothing to install. It speaks
  each engine's API over its local socket, prefers Podman or rootless Docker (§10), and turns
  ADR 0078 §6's calls into the engine's create, start, stop and remove. Its connector is
  `docker exec -i` or `podman exec -i` as the container's non-root user (ADR 0078 §2).
- **Every other backend is a provider extension on ADR 0078 §6's seam**: `create`, `start`,
  `stop`, `snapshot`, `destroy` and `status`, returning a connector. This record adds to the seam
  only what §2 and §8 need: the role labels a provider may make, and an optional `expose` and
  `unexpose` for a provider that can make a public URL. A Kubernetes provider on
  kubernetes-sigs/agent-sandbox is the example extension that proves the seam (RR-36). Cloud VMs,
  microVM services and hosted providers come the same way, later.
- **Ownership is a per-provider setting the operator approves in the window** (V59 (b)). Each
  provider extension is marked *owned* (its machines are the user's own: their own cluster,
  their own cloud account) or *not owned*, and **the default is not owned**. Only the operator
  changes it, on `local-ui`, and the change is an event (§13). Nothing a provider says about
  itself, in its manifest or its answers, changes it. The core provider's containers are on this
  machine, so it is owned. A not-owned provider's runners never carry `harness` and never get a
  grant (§2, §7).
- **The core provider and an extension are called the same way.** The core provider implements
  the seam's interface inside charter, so a second backend changes no caller.
- **A devcontainer spec describes a runner's image** (the Dev Containers specification's
  `devcontainer.json`). charter reads its image or Dockerfile, its features, the commands that
  run *inside* the container (`onCreateCommand`, `updateContentCommand`, `postCreateCommand`,
  `postStartCommand`), and its `forwardPorts` as the ports to offer.
- **A spec never widens what this record allows.** `initializeCommand` runs on the host, so
  charter never runs it. The fields that would mount the host, add privileges, share the host's
  network or pass arbitrary engine arguments are refused, and the refusal names the field. A
  repo chooses what is in the image, never how the image is confined (§10).
- **The verified `charter` enters the image at build, never by a host mount.** charter builds a
  layer over the spec's image holding the `charter` binary it verified as ADR 0078 §5 does, and
  reuses that image for every runner of the same spec and version.

### 5. App runners start from a branch or a commit

**Settled by V51.**

- **An `app` runner names a repo of its workspace and a ref**: a branch, a commit, or a chat's
  piece. The desktop pushes that ref over the link into the runner's bare repo (ADR 0078 §7,
  RR-16), so the runner needs no forge credential. The runner checks it out, builds it with the
  spec's commands, and serves it.
- **The version is fixed for the runner's life.** A new commit is a new runner, or a rebuild the
  operator or the owning chat asks for, which is a `runner.rebuilt` event (§13). Many runners
  side by side are many versions side by side.
- **Whether the ref is one a human chose decides its grants** (§7, V59 (a)). A ref is
  human-chosen when the operator picked it on `local-ui`, when it is the head of the workspace's
  target branch as last fetched from its remote (never a local branch an agent can move), or
  when it is a commit the operator approved, such as a pull request head reviewed in a Review
  tab. Every other ref, an agent's own branch or piece above all, is unreviewed, and its runner
  starts with no grants or with test credentials only. A rebuild onto a new commit is decided
  again.
- **Its ports are forwarded, never published** (§8): to the desktop's loopback, and brokered to
  the workspace's browser runners.

### 6. Browser runners, and the browser MCP charter brokers

**Settled by V47.**

- **A `browser` runner runs browsers, and only charter's broker drives them.** The browser's
  DevTools endpoint listens inside the runner, on its loopback, and is never forwarded, published
  or handed to anyone. Its driver is charter's own code in the runner's host, in Rust like the
  rest of the shipped app.
- **A harness reaches it through a browser MCP server that charter runs for the chat**, on the
  chat's own MCP route (HP-7), and delivers to each harness by that harness's own route (ADR 0050,
  ADR 0063). A tool call goes from the harness to charter's server, over the link to the browser
  runner's host, and to the browser. The harness never receives a CDP or Playwright endpoint, a
  WebSocket URL or a port. **No tool passes protocol messages through:** each tool is a typed
  action (open, navigate, click, type, read, screenshot, close), and none takes a raw CDP or
  Playwright command, so the MCP is not a remote endpoint by another name.
- **The broker is the boundary, not the browser.** Each call is a typed tool call charter logs
  as an event (§13), with its target page and outcome, never the page's content. What a page
  says back is untrusted tool output; the harness's own handling of that is unchanged.
- **Every browser is shown live in the window.** The runner's host streams the browser's frames
  as a browser view stream (§3), and the Runners tab and the owning chat's pane show them. The
  operator may type into a page through the view, which is a `browser.typed` event, a human act
  (§13). **What the operator types into a page an agent drives is visible to that agent**, which
  can read the page; the view says so, and a secret belongs in AC-15's lane, not in a typed
  field.
- **A browser runner reaches app runners only through the broker.** Its requests to a forwarded
  app port go up its link and down the app runner's (§8). Its egress is ADR 0067's presets plus
  SD-31's `browser` lane, which reaches the public web and refuses private, link-local and
  loopback addresses other than the brokered ports.
- **A browser runner never receives a vault value** (§7). Logging in with a vault credential is
  AC-15's local lane or a `harness` runner the operator owns.

### 7. Grants: a human approves each vault entry once per workspace

**Settled by V52.**

- **An app runner declares the vault entries it needs** in its spec's charter section
  (`customizations.charter.grants`, each a vault and an entry name). A declaration is a request,
  never an approval.
- **A grant is a human's approval of one declared entry for one workspace on this machine**,
  made on `local-ui` only: in the window, or by `charter` on that scope. It is kept in
  `<config>/runner-grants.json` (§14), never in the plane, so a commit cannot grant itself
  anything. A grant names its workspace, repo, spec path, vault and entry. Revoking it takes
  effect at the next start.
- **A grant reaches a runner only when it builds a ref a human chose** (§5, V59 (a)). A runner on
  an unreviewed ref, which is every agent's own branch, gets no granted value. The operator may
  approve a second entry as the grant's **test credential**, which such a runner gets instead.
  The `grant.injected` event names the ref and which rule made it human-chosen.
- **charter injects a granted value at start**, through ADR 0078 §7's push: the desktop's host
  sends it down the link for that runner's serve command, bound to the runner and the start.
  Nothing writes it into an image, a build layer, the record, a log stream or the event log; the
  log stream's view masks a value it recognises, as a convenience and not as the boundary.
- **Agents may start runners whose grants are approved, and cannot add or widen one** (V16). A
  chat's start of an app runner whose spec declares an entry with no grant starts it without that
  entry, and the operator gets a needs-you item on that chat naming the missing grant.
- **A `browser` runner never gets a value**, granted or not, and neither does any runner of a
  provider the operator has not marked as owned (§4, V59 (b)). The desktop
  refuses the push, and the runner's host refuses to receive it.

### 8. Private by default; a public URL is a human's act

**Settled by V53.**

- **A runner's ports are reachable only over its link.** RR-24 forwards a listening port to
  `127.0.0.1` on the desktop, on a port the desktop picks, and brokers it to the workspace's
  browser runners. Nothing is bound on the runner's public address, the container engine
  publishes no port, and a provider's own port or preview feature is never called for a private
  port.
- **A public URL is made only by a human** from the Runners tab or `charter runner expose` on
  `local-ui`: one port, a time limit the human picks (default one hour), revocable at any time,
  and listed on the runner's row with its expiry while it lasts. The provider's `expose` makes it
  where the provider can; the core provider exposes the desktop's forward on an address the human
  chooses. An agent cannot expose, extend or re-expose a port (V16). Expiry, revocation, the
  runner's stop and the kill switch all end it.
- **Egress** follows ADR 0067's presets, applied to the runner by its provider: the toolchains
  and forge presets for an app's build (SD-4), SD-31's `browser` lane for a browser runner, and
  nothing a preset does not list.

### 9. Lifecycle: an owner, a pin, an idle stop, a budget, the kill switch

**Settled by V54 and D-ADR0089a.**

- **A short-lived runner is owned by the chat that started it, or by its workspace.** A runner a
  chat starts is owned by that chat and is **agent-started**. A runner a human starts, in the
  Runners tab or by `charter runner start` on `local-ui`, is owned by its workspace and is
  **human-started**; the command names the workspace it starts in, and a human start never
  belongs to a chat. A human may **pin** an agent-started runner to its workspace, which then
  owns it as if a human had started it. A chat cannot pin.
- **An agent-started runner stops when its chat ends, when it is idle, or when the workspace's
  runner budget runs out** (§11), and is destroyed when it stops. Its chat ends when the chat's
  last run reaches `completed`, `failed` or `stopped` and the chat is closed.
- **Idle means no running chat on the runner, no open stream on its link, no request served on a
  forwarded port and no build in progress**, for the workspace's idle timeout (thirty minutes by
  default).
- **A `harness` runner lives while its chats run.** When the chat that owns it ends, the chats
  running on it keep running: the runner outlives its owner until the last of them ends, then it
  is idle and stops. It stays agent-started and counted in the budget until then, and the
  Runners tab names its owner as ended.
- **A human-started or pinned runner stops only by a human or by the kill switch.** Idle and the
  budget never stop it. It is stopped, never destroyed, by the kill switch, keeps its row in the
  Runners tab, and a human starts it again or destroys it.
- **The kill switch stops every runner.** A stop on the desktop (ADR 0071) stops every
  short-lived runner through its provider, and stops the agents on every long-lived runner over
  its link, as ADR 0078 §7 already does. While the desktop is stopped no runner starts. A
  provider that cannot be reached leaves its runner named as *not reached* (V29c), and charter
  keeps trying.
- **An orphan is found on the next start of `charterd`.** The host reads every runner record,
  asks each provider for `status`, and stops or destroys what its owner's state says should no
  longer run.

### 10. Isolation defaults

**Settled by V55.**

- **A local runner is a rootless container**: Podman, or Docker in rootless mode. When the core
  provider finds neither, it says so plainly and offers a rootful engine, which the operator must
  accept once on `local-ui`, an event of its own (§13). No runner starts on a rootful engine
  before that.
- **It drops every Linux capability, sets no-new-privileges, and mounts nothing from the host but
  its own work directory.** The engine's socket is never mounted into a runner. A seccomp profile
  is the engine's default or stricter.
- **Two users inside, neither root** (§3, V59 (c)). `charter serve` runs as its own non-root
  user, and keeps only the capabilities it needs to start a process as another user; the
  workload of an `app` or `browser` runner runs as a second unprivileged user with no
  capabilities at all, under no-new-privileges, and cannot read the serve user's files.
- **On macOS the container is inside the Docker or Podman VM**, so the VM is a second wall, and
  the work directory is the only path shared into it.
- **A provider may offer microVM isolation** (Kata, Firecracker) as an option it declares. It is
  never the default, because neither runs natively on macOS.
- **ADR 0067's sandbox still confines a chat on a `harness` runner** (RR-5). The container is the
  runner's wall; the sandbox is the chat's.

### 11. Agents act within a budget

**Settled by V46**, with ADR 0082 §4's rule that a budget never caps the operator.

- **A chat may start, stop and use runners in its own workspace** through charter's per-chat MCP
  server (HP-7) and `charter runner start` on its hook channel. Both reach `charterd` on the
  `chat` scope, which gains these requests and nothing else on a link: the human scopes stay
  refused to a chat (ADR 0068 §5).
- **Each workspace has a runner budget on this machine, and it counts only agent-started
  runners** (D-ADR0089a): how many may run at once, how many runner-hours a day, and, for a
  provider that reports what a runner costs, how much they may spend a month. A chat's start past
  it is refused, and the operator gets a needs-you item on that chat saying which limit it
  reached; an agent-started runner past the spend or hours limit is stopped (§9). **A provider's
  spend budget starts at zero**, so no agent spends money until a human sets one (V58 (a)).
- **The budget caps agents, never the operator**, which is ADR 0082 §4's rule. Human-started and
  pinned runners are not counted against it and are never stopped by it, so the operator's own
  runners are never refused or stopped by a budget.
- **The budget is a human's setting**, kept in `<config>/runner-budgets.json` (§14) and changed
  only on `local-ui`: in the window, or by `charter` on that scope. A chat cannot raise it. Each
  change is a `runner.budget.changed` event (§13).
- **Exposing a port or adding a grant stays a human's act** (§7, §8), whatever the budget.

### 12. The Runners view tab

**Settled by V50.**

- **A view tab like the persona and changes views**, opened from the workspace. It lists the
  workspace's runners by label, state and cost, with their tags, owner, source ref, uptime and
  spend so far.
- **Each runner opens to its streams**: a shell on the runner (a shell tab on that device, ADR
  0062's kind), its log stream, its forwarded ports with a button to open each on the desktop's
  loopback, its live browser frames, and its public URL with its expiry, if a human made one.
- **A chat's pane links to the runners it started**, and each runner's row names its owning chat.
- **Start, stop, destroy, pin, expose, grant and the budget are the tab's controls**, each a
  human act on `local-ui`. The kill switch's state is shown on every row it reached or did not
  reach.

### 13. The event log and the audit

**Settled by V46 and V50.** Every act in this record is an event in the host's event log
(ADR 0066), and the audit is written from it (ADR 0075).

| Event kind | Written when | Body (never a value, a page's content or a log line) |
|---|---|---|
| `runner.started`, `runner.stopped`, `runner.destroyed` | a short-lived runner changes state | runner id, label, tags, provider, owner (chat and run, or `workspace`), source ref, the cause (`human`, `chat`, `idle`, `budget`, `chat-ended`, `killswitch`, `orphan`) |
| `runner.rebuilt` | the operator or the owning chat rebuilds an app runner onto a new commit | runner id, the old and new commit, who asked, whether the new ref is human-chosen |
| `runner.pinned`, `runner.unpinned` | a human pins or unpins one | runner id, workspace |
| `runner.budget.changed` | a human changes a workspace's runner budget or idle timeout | workspace, the limits before and after |
| `runner.provider.ownership` | a human marks a provider as owned or not owned | provider, the new setting |
| `runner.engine.rootful.accepted` | a human accepts a rootful engine for the core provider | the engine and its socket |
| `runner.refused` | a start, a chat start or a value push is refused by label, budget or the kill switch | runner id, what was refused, the rule |
| `runner.port.forwarded`, `runner.port.closed` | RR-24 opens or closes a forward | runner id, runner port, desktop port |
| `runner.exposed`, `runner.unexposed` | a human makes or ends a public URL | runner id, port, expiry, the cause of its end |
| `browser.opened`, `browser.closed`, `browser.call` | the broker opens a browser, closes one, or runs a tool call | runner id, chat and run, tool name, the page's origin, outcome |
| `browser.typed` | the operator types into a page through the browser view | runner id, the page's origin, how many keystrokes (never the keys) |
| `grant.approved`, `grant.revoked`, `grant.injected` | a human approves or revokes a grant, or charter injects one at a start | workspace, repo, vault and entry name; for an injection the runner id, the ref, the rule that made it human-chosen, and whether the test credential was used |

ADR 0075's registry gains them (ADR 0075, amended, below).

### 14. The stores, and their tiers

| Store | Where | Tier |
|---|---|---|
| a short-lived runner's record | `<data>/runners/<runner-id>.json` on the desktop: its id, role label, tags, provider and the provider's handle for the machine, its owner, workspace and source ref, its pinned device id and link key, its forwards and public URLs with their expiries, its state, its runner-hours and spend so far | Machine, device-bound. Backed up, so a restore can find a machine that still runs or bills |
| the grants | `<config>/runner-grants.json`: per project and workspace, each grant's repo, spec path, vault, entry, its test credential if any, the commits the operator approved for it, and when and by whom it was approved | Machine, device-bound, because each grant points into this machine's vaults |
| the runner budgets | `<config>/runner-budgets.json`: per project and workspace, the concurrency, runner-hours and spend limits, the idle timeout, and the period's usage | Machine, device-bound |
| whether a provider is owned | inside the provider extension's row in `<config>/extensions.json`, beside its approved fingerprint. **No store of its own** | Machine, device-bound, as that file is |
| a runner's labels and tags | inside its record, or inside its row in `<config>/runners.json` for a long-lived runner. **No store of its own**: the three role labels are fixed in charter's code | the tier of the file they are in |
| runner images and stopped containers | the container engine's own store | None: the engine's, which charter names and prunes but does not own |

**Chats may touch none of them.** A record holds a pinned key and a provider handle, the grants
decide which values a runner receives, and the budgets decide what agents may spend. ADR 0067,
amended, adds them to the classes a chat's sandbox always denies.

## ADR 0078, amended

- **§6's last paragraph**, *"Short-lived sandbox services are not runners. A runner is a host
  that holds sessions for days. Services whose machines live for minutes are revisited as
  providers for hosted runners, after GT-CLOUD."*, now reads: **a runner may live for days or for
  minutes. A provider makes a short-lived runner on request, and destroys it when its owner is
  done with it (ADR 0089 §1, §9). Docker and Podman are a core provider; every other backend is a
  provider extension on this seam (ADR 0089 §4).**
- **§6's seam gains** the role labels a provider may make, and an optional `expose` and
  `unexpose` (ADR 0089 §2, §8), and each provider gains the operator's *owned* setting, default
  not owned (ADR 0089 §4). Its other calls stand.
- **§6's third bullet**, *"Idle shutdown and the cost guard are RR-21's"*, with *"a monthly cap
  with a hard stop"*, is replaced: **the idle stop and the spend limit are ADR 0089 §9's idle rule
  and §11's workspace runner budget, which count and stop only agent-started runners. RR-21 keeps
  snapshot-and-delete where a stopped machine still bills, a still-billing badge, and destroy on
  workspace remove.** A monthly hard stop on a runner the operator started is gone, as ADR 0082
  §4 asks.
- **The decision's first sentence**, *"A runner is a device running `charter serve`"*, stands,
  and gains: **every runner, however it was made, carries exactly one role label (ADR 0089 §2)**.
- **§7's table** gains a port stream, a log stream and a browser view stream, each a stream of
  the link (ADR 0089 §3), and a grant's injection, which is §7's secret push applied to a start
  (ADR 0089 §7).
- **§4's table, the `chat` row,** *"nothing"*, now reads: **a start or stop of a runner the chat
  owns or one pinned to its workspace, within the workspace's budget, and a browser tool call to
  such a browser runner. The desktop's host performs each at its own gate, as its own act down the
  link; the chat holds nothing on the link and sends it nothing directly** (ADR 0089 §6, §11).
  The rest of §4 stands.
- **§4's first reason an agent on the desktop cannot act on a runner**, *"the `chat` scope has
  no command that reaches a link"*, now reads: **the `chat` scope has no command that reaches a
  link except ADR 0089's runner requests and browser tool calls, which the desktop's host
  performs at its own gate, which carry no human power, and none of which pins, exposes, grants,
  changes a budget or answers an ask.** The second and third reasons stand unchanged.
- **§8's `runners.json` row** gains each long-lived runner's role label and tags.

## ADR 0068, amended

§5, the `chat` scope's row, gains **the runner requests of ADR 0089 §11 (start, stop and list
the chat's own runners and its workspace's pinned ones) and the browser MCP's tool calls (ADR
0089 §6)**, each checked by `charterd` against the chat's workspace, the runner's label and the
workspace's budget. No human power comes with them. **Pinning, exposing, granting, changing a
budget, marking a provider as owned and accepting a rootful engine are `local-ui` acts only**
(D-ADR0089a): `terminal`, `approval` and `fleet-mcp` cannot do them either.

## ADR 0067, amended

§5 class 2, *"Charter's integrity state is denied to chats"*, gains **the short-lived runner
records (`<data>/runners/`), the grants (`<config>/runner-grants.json`) and the runner budgets
(`<config>/runner-budgets.json`)**. The first would let a chat change a provider handle or a
pinned key, the second would let it grant itself a value, and the third would let it raise its
own budget. §3's egress presets apply to runners through their provider (ADR 0089 §8), and the
`browser` lane is SD-31's.

## ADR 0069, amended

The inventory, *Every store, by tier*, gains these rows. Rows 81 to 84 are ADR 0083's, row 85
is ADR 0085's and row 86 is ADR 0088's, so these are 87 to 90. Row 81 is also the number ADR
0069's own table gives pending landings (#472), a clash that predates this record and that it
leaves to its own fix:

| # | Store | Tier | Sync | Backed up | Rebuildable |
|---|---|---|---|---|---|
| 87 | `<data>/runners/<runner-id>.json`, a short-lived runner's record (ADR 0089) | Machine | device-bound | yes | no |
| 88 | `<config>/runner-grants.json`, the grants (ADR 0089) | Machine | device-bound | yes | no |
| 89 | `<config>/runner-budgets.json`, the runner budgets (ADR 0089) | Machine | device-bound | yes | no |
| 90 | runner images and stopped containers in the container engine's store (ADR 0089) | None | — | no | — |

Row 74, `<config>/runners.json`, keeps its tier and gains a long-lived runner's label and tags.
`<config>/extensions.json` keeps its tier and gains a provider's *owned* setting.
A record is backed up although its machine may be gone, because a restore onto a machine that
replaces this one must be able to stop what still bills. A grant restored onto such a machine
names vault entries that the restore marks missing, as ADR 0069 §5 does for the keyring.

## ADR 0071, amended

**The switch reaches short-lived runners.** A stop stops every short-lived runner through its
provider, as well as the agents on every long-lived runner over its link (ADR 0078, amended). A
pinned or human-started runner is stopped and kept; an agent-started runner whose chat has
ended is destroyed after the re-arm, not before, so the operator can look at what it was doing.
While the machine is stopped, no runner starts.

## ADR 0075, amended

The registry of §4 gains:

| Action | From | `meta` |
|---|---|---|
| `runner.started`, `runner.stopped`, `runner.destroyed` | ADR 0089 §9 | runner id, label, provider, owner, source ref, cause |
| `runner.rebuilt` | ADR 0089 §5 | runner id, old and new commit, whether the new ref is human-chosen |
| `runner.pinned`, `runner.unpinned` | ADR 0089 §9 | runner id, workspace |
| `runner.budget.changed` | ADR 0089 §11 | workspace, the limits before and after |
| `runner.provider.ownership` | ADR 0089 §4 | provider, the new setting |
| `runner.engine.rootful.accepted` | ADR 0089 §10 | the engine and its socket |
| `runner.refused` | ADR 0089 §2, §11 | runner id, what was refused, the rule |
| `runner.exposed`, `runner.unexposed` | ADR 0089 §8 | runner id, port, expiry, cause |
| `browser.opened`, `browser.closed`, `browser.call` | ADR 0089 §6 | runner id, tool name, the page's origin, outcome |
| `browser.typed` | ADR 0089 §6 | runner id, the page's origin, the keystroke count |
| `grant.approved`, `grant.revoked`, `grant.injected` | ADR 0089 §7 | workspace, repo, vault, entry name; for an injection the runner id, the ref, the rule that made it human-chosen, and whether the test credential was used |

§2's actor rules gain: **a start, stop or browser call a chat asked for is an `agent` entry by
that chat's run; a stop for idle, budget, a chat's end or an orphan is a `host` entry; a pin, an
exposure, a grant, a human start, a rebuild the operator asked for, a budget change, an
ownership change, accepting a rootful engine and typing into a page are `human` entries; a
rebuild the owning chat asked for is an `agent` entry.** RR-29 registers these actions and
rules. `runner.port.forwarded` and
`runner.port.closed` are events and not audit entries: a forward follows from a start and gives
no new power.

## What changes where

The code does not change with this record.

| Where | Change |
|---|---|
| `CONTEXT.md` | **Runner**'s definition grows; gains **Label**, **Provider** and **Grant**; **Capability**'s and **Pin**'s entries name the new senses (in this PR) |
| `docs/plane-format.md` | The stores of §14, each **decided, not yet written** with its tier, and `runners.json`'s label and tags (in this PR) |
| ADR 0067, ADR 0068, ADR 0069, ADR 0071, ADR 0075, ADR 0078 | Amended above. Their texts are left as accepted, and this record is the amendment |
| RR-29 | The runner record, the role labels and their checks at both ends of the link (§1, §2, §14), and the registry rows and actor rules of ADR 0075, amended |
| RR-3 | The core Docker and Podman provider, the devcontainer spec, the verified binary in the image, the isolation defaults (§4, §10) |
| RR-30 | The Runners view tab (§12) |
| RR-24 | Ports over the link to the desktop's loopback and to browser runners (§8) |
| RR-31 | Owners, pins, idle stop, the budget, the kill switch, and a chat's start and stop (§9, §11) |
| RR-32 | App runners from a ref (§5) |
| RR-33 | Grants (§7) |
| RR-34 | Browser runners and the brokered browser MCP (§6) |
| RR-35 | A public URL as a human's act (§8) |
| RR-21 | The provider seam open to extensions, with the labels and `expose` (§4) |
| RR-36 | The Kubernetes example provider extension (§4) |
| SD-31 | The `browser` lane, applied to browser runners (§6, §8) |
| AC-15, ED-10, RR-25 | AC-15 stays the local lane for authenticated browsing; ED-10's pane lists RR-24's forwards; RR-25's presets stand beside providers |

## What this costs

- **Every runner carries a `charter` and a link.** A container that only serves an app pays for
  a host process and a handshake. It buys one path for shells, logs, ports and browsers on every
  backend, and one trust story.
- **Brokered traffic goes through the desktop.** A browser runner reaching an app runner goes up
  one link and down another, so the desktop must be awake and the round trip is longer than a
  direct container network. It keeps every port private.
- **A browser is driven by charter's own driver, not by the harness's favourite browser tool.**
  Tools that want a raw endpoint do not work against a browser runner.
- **A repo's devcontainer spec can fail where an editor would run it.** charter refuses
  `initializeCommand` and the fields that widen isolation, so a spec that needs them does not
  start a runner.
- **Short-lived records are backed up**, though most describe machines that are gone, so a
  restore can stop the ones that are not.
- **Two users in every app and browser runner.** `charter serve` keeps the capabilities to start a
  process as another user, so "every capability dropped" holds for the workload, not for the
  host process.
- **An agent's own branch runs without the real credentials.** Testing what an agent wrote
  against a real service needs a human to approve the commit, or a test credential.
- **A zero spend budget by default** means an agent cannot use a provider that bills until a human
  sets a budget for that workspace.

## What was rejected

- **A new noun beside Runner** (an environment, a sandbox, a box). It would be a sixth concept
  or a second device, with its own shell, log and port paths. The operator grew Runner instead
  (V48).
- **Reaching each backend through its own API** (an engine's exec and logs, a pod's port-forward,
  a vendor's preview URL). It would give each backend its own path and its own authentication,
  and none of them is the link's.
- **Handing the harness a CDP or Playwright endpoint.** Whoever holds one controls the browser
  and, for some, the OS user it runs as. The broker logs every call and holds the endpoint
  inside the runner (V47).
- **Several role labels on one runner.** A browser beside an app's grants is the case V52
  forbids, and one label keeps both checks simple.
- **Grants committed in the repo.** A commit could then grant itself a value. A repo declares,
  and a human on this machine approves.
- **Publishing ports by default**, as many hosted products do. A forwarded loopback port and a
  brokered port cover testing, and a public URL is a human's act with an expiry (V53).
- **A provider's microVM as the default.** It does not run on macOS, where most desktops are.
- **App logs as telemetry.** They are content, and ADR 0083's four sources would gain a fifth.

## Ruled (V46 to V55, 2026-10-02)

The operator ruled the remote-compute idea in one grill, after the research on remote compute
and browsers. V48 settled the open question of whether to grow Runner or add a noun, under the
three conditions quoted above. Each section of this record says which ruling settles it.
