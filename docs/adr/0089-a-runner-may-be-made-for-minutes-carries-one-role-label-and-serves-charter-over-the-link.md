# A runner may be made for minutes, carries one role label, and serves charter over the link

**Accepted 2026-10-02** by the operator (rulings V46 to V55), drafted for program-map ticket
RR-28. It amends [ADR 0078](0078-a-runner-is-charterd-behind-a-connector-and-charters-own-keys-say-who-is-on-the-link.md),
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

**Its concepts.** A runner is still a **device**, so it, its **label** and its **provider**
belong to **Project**, as ADR 0078 put them. A **grant** is approved per workspace, so it is a
setting of **Workspace**. The **Runners** view tab is a view of **Workspace**: it shows the
runners that workspace's chats started, the ones pinned to it, and the devices its chats run on.
No sixth concept is added (ADR 0072 §2).

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
any number of tags of the operator's own words. Its ports, shells, logs and browser views reach the window only
over the link. It is private, it holds no vault value unless a human granted one, it belongs to
a chat or to a pinned workspace, it stops with the kill switch, and every start is in the event
log.**

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
| `app` | build and serve one version of a workspace repo (§5) | starting a chat; any vault value that is not an approved grant (§7) |
| `browser` | run browsers that charter's broker drives (§6) | starting a chat; every vault value, grant or not (§7) |

- **A runner carries exactly one role label.** A browser runner that could also serve an app
  would hold the app's grants beside a page an agent drives, which V52 forbids. A chat on a
  `harness` runner may still run a dev server, whose ports RR-24 forwards.
- **Both ends check the label.** The desktop's `charterd` refuses to send a chat start, or to push
  a vault value, down a link to a runner whose label forbids it. The runner's own host refuses
  them too, from the label it was created with. Either refusal is a `runner.refused` event (§13).
- **A provider declares the role labels it may make.** charter refuses a runner whose label its
  provider did not declare. A provider whose machines the user does not own does not declare
  `harness`, and its runners get no grants (V48, V52). Hosted providers are a later provider
  extension, and this record is complete without one.
- **Tags are any words the operator picks**, beside the label (`gpu`, `node-22`, `staging-data`). charter
  enforces nothing about a tag. A persona or workspace may ask for a runner by label and tags,
  and the Runners tab filters by them.

### 3. Every runner runs `charter serve` over the link

**Settled by V48 (c).** A runner's shell, logs, ports and browser frames are streams of its link
(ADR 0068 §4's session protocol, multiplexed per stream), whatever made the runner. charter never
uses a backend's own exec, log, port-forward or preview-URL API to reach what runs inside it.

- **Why.** One authenticated path per runner, encrypted end to end by the link, with its N−1
  version promise, instead of a different API per backend and a different trust story for each.
  A Kubernetes API server, a container engine's socket or a vendor's gateway carries the
  connector's bytes and sees nothing inside them.
- **New stream kinds, on the same protocol.** The session protocol gains a port stream (RR-24),
  a log stream (an app runner's build and serve output) and a browser view stream (§6). Each is a
  protocol version under FD-4's negotiation, which fails closed.
- **Logs are views, not telemetry.** An app's output and a browser's console are content. They
  are shown live in the Runners tab with a bounded scrollback, as a terminal is, and are not a
  fifth source for ADR 0083's pipeline. Making them one would be a change to that record.
- **`charter serve --foreground` is the container's first process**, with the container runtime
  as its supervisor (ADR 0078 §5). The app or the browser runs under it.

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
- **The core provider and an extension are called the same way.** The core provider implements
  the seam's interface inside charter, so a second backend changes no caller.
- **A devcontainer spec describes a runner's image** (the Dev Containers specification's
  `devcontainer.json`). charter reads its image or Dockerfile, its features, and the commands that
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
  operator or the owning chat asks for, which the event log records. Many runners side by side
  are many versions side by side.
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
  WebSocket URL or a port.
- **The broker is the boundary, not the browser.** Each call is a typed tool call charter logs
  as an event (§13), with its target page and outcome, never the page's content. What a page
  says back is untrusted tool output; the harness's own handling of that is unchanged.
- **Every browser is shown live in the window.** The runner's host streams the browser's frames
  as a browser view stream (§3), and the Runners tab and the owning chat's pane show them. The
  operator may type into a page through the view; the event log records it as a human act.
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
  made in the window on `local-ui` or by `charter` on that scope. It is kept in
  `<config>/runner-grants.json` (§14), never in the plane, so a commit cannot grant itself
  anything. A grant names its workspace, repo, spec path, vault and entry. Revoking it takes
  effect at the next start.
- **charter injects a granted value at start**, through ADR 0078 §7's push: the desktop's host
  sends it down the link for that runner's serve command, bound to the runner and the start.
  Nothing writes it into an image, a build layer, the record, a log stream or the event log; the
  log stream's view masks a value it recognises, as a convenience and not as the boundary.
- **Agents may start runners whose grants are approved, and cannot add or widen one** (V16). A
  chat's start of an app runner whose spec declares an entry with no grant starts it without that
  entry, and the chat gets a needs-you item naming the missing grant.
- **A `browser` runner never gets a value**, granted or not, and neither does any runner whose
  provider does not declare `harness` and whose machines the user does not own (§2). The desktop
  refuses the push, and the runner's host refuses to receive it.

### 8. Private by default; a public URL is a human's act

**Settled by V53.**

- **A runner's ports are reachable only over its link.** RR-24 forwards a listening port to
  `127.0.0.1` on the desktop, on a port the desktop picks, and brokers it to the workspace's
  browser runners. Nothing is bound on the runner's public address, the container engine
  publishes no port, and a provider's own port or preview feature is never called for a private
  port.
- **A public URL is made only by a human** from the Runners tab or `charter runner expose` on a
  human scope: one port, a time limit the human picks (default one hour), revocable at any time,
  and listed on the runner's row with its expiry while it lasts. The provider's `expose` makes it
  where the provider can; the core provider exposes the desktop's forward on an address the human
  chooses. An agent cannot expose, extend or re-expose a port (V16). Expiry, revocation, the
  runner's stop and the kill switch all end it.
- **Egress** follows ADR 0067's presets, applied to the runner by its provider: the toolchains
  and forge presets for an app's build (SD-4), SD-31's `browser` lane for a browser runner, and
  nothing a preset does not list.

### 9. Lifecycle: an owner, a pin, an idle stop, a budget, the kill switch

**Settled by V54.**

- **A short-lived runner belongs to the chat that started it**, or to the workspace when a human
  pins it to the workspace. A runner the operator starts from the Runners tab belongs to the
  workspace and is pinned. Pinning is a human act; a chat cannot pin.
- **A chat-owned runner is stopped and destroyed when its chat ends** (the chat's last run
  reaches `completed`, `failed` or `stopped` and the chat is closed), after its idle timeout, or
  when the workspace's runner budget runs out (§11). The idle timeout counts time with no
  stream open on its link and no request served on a forwarded port; the default is thirty
  minutes, and the workspace may change it.
- **A pinned runner is stopped, never destroyed, by idle, by budget and by the kill switch.** It
  keeps its row in the Runners tab, and a human starts it again or destroys it.
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

- **A local runner is a rootless container**: Podman, or Docker in rootless mode, and the core
  provider says plainly when it found neither and falls back to a rootful engine, which the
  operator must accept once.
- **It drops every Linux capability, sets no-new-privileges, runs as a non-root user, and mounts
  nothing from the host but its own work directory.** The engine's socket is never mounted into
  a runner. A seccomp profile is the engine's default or stricter.
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
- **Each workspace has a runner budget on this machine**: how many agent-started runners may run
  at once, how many runner-hours a day, and, for a provider that reports what a runner costs, how much spend
  a month. A start past it is refused, and the chat gets a needs-you item saying which limit it
  reached; a running runner past the spend or hours limit is stopped (§9). **A provider's spend
  budget starts at zero**, so no agent spends money until a human sets one.
- **The budget caps agents, never the operator.** A human may start a runner past it from the
  Runners tab, after one line saying so, as ADR 0082 §4 lets the operator pass a target.
- **The budget is a human's setting**, kept in `<config>/runner-budgets.json` (§14) and changed
  only on `local-ui` or by `charter` on that scope. A chat cannot raise it.
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
- **Stop, destroy, pin, expose and grant are the tab's controls**, each a human act on
  `local-ui`. The kill switch's state is shown on every row it reached or did not reach.

### 13. The event log and the audit

**Settled by V46 and V50.** Every act in this record is an event in the host's event log
(ADR 0066), and the audit is written from it (ADR 0075).

| Event kind | Written when | Body (never a value, a page's content or a log line) |
|---|---|---|
| `runner.started`, `runner.stopped`, `runner.destroyed` | a short-lived runner changes state | runner id, label, tags, provider, owner (chat and run, or `workspace`), source ref, the cause (`human`, `chat`, `idle`, `budget`, `chat-ended`, `killswitch`, `orphan`) |
| `runner.pinned`, `runner.unpinned` | a human pins or unpins one | runner id, workspace |
| `runner.refused` | a start, a chat start or a value push is refused by label, budget or the kill switch | runner id, what was refused, the rule |
| `runner.port.forwarded`, `runner.port.closed` | RR-24 opens or closes a forward | runner id, runner port, desktop port |
| `runner.exposed`, `runner.unexposed` | a human makes or ends a public URL | runner id, port, expiry, the cause of its end |
| `browser.opened`, `browser.closed`, `browser.call` | the broker opens a browser, closes one, or runs a tool call | runner id, chat and run, tool name, the page's origin, outcome |
| `grant.approved`, `grant.revoked`, `grant.injected` | a human approves or revokes a grant, or charter injects one at a start | workspace, repo, vault and entry name, runner id for an injection |

ADR 0075's registry gains them (ADR 0075, amended, below).

### 14. The stores, and their tiers

| Store | Where | Tier |
|---|---|---|
| a short-lived runner's record | `<data>/runners/<runner-id>.json` on the desktop: its id, role label, tags, provider and the provider's handle for the machine, its owner, workspace and source ref, its pinned device id and link key, its forwards and public URLs with their expiries, its state, its runner-hours and spend so far | Machine, device-bound. Backed up, so a restore can find a machine that still runs or bills |
| the grants | `<config>/runner-grants.json`: per project and workspace, each grant's repo, spec path, vault, entry, and when and by whom it was approved | Machine, device-bound, because each grant points into this machine's vaults |
| the runner budgets | `<config>/runner-budgets.json`: per project and workspace, the concurrency, runner-hours and spend limits, the idle timeout, and the period's usage | Machine, device-bound |
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
  `unexpose` (ADR 0089 §2, §8). Its other calls stand.
- **The decision's first sentence**, *"A runner is a device running `charter serve`"*, stands,
  and gains: **every runner, however it was made, carries exactly one role label (ADR 0089 §2)**.
- **§7's table** gains a port stream, a log stream and a browser view stream, each a stream of
  the link (ADR 0089 §3), and a grant's injection, which is §7's secret push applied to a start
  (ADR 0089 §7).
- **§4's table, the `chat` row,** *"nothing"*, now reads: **a start or stop of a runner the chat
  owns or one pinned to its workspace, within the workspace's budget, and a browser tool call to
  such a browser runner. The desktop's host performs each at its own gate, as its own act down the
  link; the chat holds nothing on the link and sends it nothing directly** (ADR 0089 §6, §11).
  The rest of §4 stands, and so do the three reasons an agent on the desktop cannot act on a
  runner as a human.
- **§8's `runners.json` row** gains each long-lived runner's role label and tags.

## ADR 0068, amended

§5, the `chat` scope's row, gains **the runner requests of ADR 0089 §11 (start, stop and list
the chat's own runners and its workspace's pinned ones) and the browser MCP's tool calls (ADR
0089 §6)**, each checked by `charterd` against the chat's workspace, the runner's label and the
workspace's budget. No human power comes with them: pinning, exposing, granting, changing a
budget and starting past it stay on `local-ui` and `terminal`.

## ADR 0067, amended

§5 class 2, *"Charter's integrity state is denied to chats"*, gains **the short-lived runner
records (`<data>/runners/`), the grants (`<config>/runner-grants.json`) and the runner budgets
(`<config>/runner-budgets.json`)**. The first would let a chat change a provider handle or a
pinned key, the second would let it grant itself a value, and the third would let it raise its
own budget. §3's egress presets apply to runners through their provider (ADR 0089 §8), and the
`browser` lane is SD-31's.

## ADR 0069, amended

The inventory, *Every store, by tier*, gains these rows. Rows 82 to 86 are ADR 0083's, ADR
0085's and ADR 0088's, so these are 87 to 90:

| # | Store | Tier | Sync | Backed up | Rebuildable |
|---|---|---|---|---|---|
| 87 | `<data>/runners/<runner-id>.json`, a short-lived runner's record (ADR 0089) | Machine | device-bound | yes | no |
| 88 | `<config>/runner-grants.json`, the grants (ADR 0089) | Machine | device-bound | yes | no |
| 89 | `<config>/runner-budgets.json`, the runner budgets (ADR 0089) | Machine | device-bound | yes | no |
| 90 | runner images and stopped containers in the container engine's store (ADR 0089) | None | — | no | — |

Row 74, `<config>/runners.json`, keeps its tier and gains a long-lived runner's label and tags.
A record is backed up although its machine may be gone, because a restore onto a machine that
replaces this one must be able to stop what still bills. A grant restored onto such a machine
names vault entries that the restore marks missing, as ADR 0069 §5 does for the keyring.

## ADR 0071, amended

**The switch reaches short-lived runners.** A stop stops every short-lived runner through its
provider, as well as the agents on every long-lived runner over its link (ADR 0078, amended). A
pinned runner is stopped and kept; a chat-owned runner whose chat has ended is destroyed after
the re-arm, not before, so the operator can look at what it was doing. While the machine is
stopped, no runner starts.

## ADR 0075, amended

The registry of §4 gains:

| Action | From | `meta` |
|---|---|---|
| `runner.started`, `runner.stopped`, `runner.destroyed` | ADR 0089 §9 | runner id, label, provider, owner, source ref, cause |
| `runner.pinned`, `runner.unpinned` | ADR 0089 §9 | runner id, workspace |
| `runner.refused` | ADR 0089 §2, §11 | runner id, what was refused, the rule |
| `runner.exposed`, `runner.unexposed` | ADR 0089 §8 | runner id, port, expiry, cause |
| `browser.opened`, `browser.closed`, `browser.call` | ADR 0089 §6 | runner id, tool name, the page's origin, outcome |
| `grant.approved`, `grant.revoked`, `grant.injected` | ADR 0089 §7 | workspace, repo, vault, entry name, runner id |

§2's actor rules gain: **a start, stop or browser call a chat asked for is an `agent` entry by
that chat's run; a stop for idle, budget, a chat's end or an orphan is a `host` entry; a pin, an
exposure, a grant and a start past the budget are `human` entries.** `runner.port.forwarded` and
`runner.port.closed` are events and not audit entries: a forward follows from a start and gives
no new power.

## What changes where

The code does not change with this record.

| Where | Change |
|---|---|
| `CONTEXT.md` | **Runner**'s definition grows; gains **Label**, **Provider** and **Grant**; **Capability**'s and **Pin**'s entries name the new senses (in this PR) |
| `docs/plane-format.md` | The stores of §14, each **decided, not yet written** with its tier, and `runners.json`'s label and tags (in this PR) |
| ADR 0067, ADR 0068, ADR 0069, ADR 0071, ADR 0075, ADR 0078 | Amended above. Their texts are left as accepted, and this record is the amendment |
| RR-29 | The runner record, the role labels and their checks at both ends of the link (§1, §2, §14) |
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
