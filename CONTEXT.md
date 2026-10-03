# charter

A desktop app for running many agent chats across your projects, and always knowing which of
them needs you. This file is the glossary: the words, not how they are built.

## Language

### The five concepts

charter has five concepts: **Project**, **Workspace**, **Chat**, **Persona** and **Memory**.
Every other word here is a part, a view or a setting of one of them, and the first hour's
screens say only those five plus **Save** and **branch** (ADR 0072).

**Project**:
One of the five concepts: the git repo that holds a team's workspaces, personas, memory and
settings, and the tab the app shows it in. The app can hold several. Vaults, save modes,
extensions and everything that holds for the whole machine are its settings. A code repo is
never a project, on any forge: it is a **repo** (ADR 0072).
_Avoid_: instance, plane (in UI text), project (for a GitLab repo)

**Project template**:
A part of **Project**: a stack's starting layout, chosen in the first run — two personas, a
starter `workspace.md` context, a review checklist (`REVIEW.md` in the reviewer persona's refs)
and the commands every harness asks about first. charter ships one each for Rust, TypeScript,
Python, Go, monorepos and docs-only repos, as data with a version, and lays one out only where
nothing is there yet (FR-17).
_Avoid_: plane template, scaffold, starter kit

**Workspace**:
One of the five concepts: a named piece of work inside a plane, with its own charter
(`workspace.md`), memory, todos and repos.
_Avoid_: task, context

**Chat**:
One of the five concepts: one conversation with an agent, in a tab, in one workspace or at the
plane root. It is where work happens and what needs you. It has a number the window shows
(`steward 3`) and an id that never changes, has one or more **runs**, and shows its
**branches**, one per repo it works in (ADR 0066, ADR 0072).
_Avoid_: session (that is the process), thread, agent, conversation (that is the harness's)

**Persona**:
One of the five concepts: a role a chat can take, with its own charter (`persona.md`), memory
and vault, handed to the harness as a sub-agent. Its curation actions and its logs are parts of
it.
_Avoid_: agent, sub-agent (that is the harness's form of it), bot, role (as the name)

**Memory**:
One of the five concepts: what charter keeps so the next chat starts knowing what earlier ones
learned. Each memory has an **owner**: a workspace, a persona, everyone (shared memory, in
`personas/_shared/memory/`) or **me**, one person's own, kept in a personal overlay plane on
their own private remote. It also has an **audience**: this machine, me on all my machines, or
the team. Approval follows the audience: memory the team will read waits for approval, so
nothing reaches a teammate's briefing unreviewed. Session records are memory too. `charter recall`
searches it as one, and a chat's briefing is drawn from it (ADR 0072).
_Avoid_: knowledge, rules, notes (for the whole of it), context

### The plane and what lives in it

**Plane**:
The git repo a project's charter lives in: its settings, personas, memory, todos and
workspaces. It is the project's database, and a change counts once it reaches the plane's
remote. **The word is being retired**, everywhere: the new term is **Project**, in the window,
`charter --help`, the docs, the code and the format. Until the rename lands, the code and the
format still say plane, and charter reads the old names for a compat window (ADR 0072).
_Avoid_: control plane, config repo, charter repo

**charter-plane**:
The charter project's own plane: the one charter is developed from, public as an example of a
plane. It is not the product. The product, the app and its core, is **charter**.
_Avoid_: charter (for the plane), the charter repo

**Work item**:
One piece of work a tracker holds: a forge issue, epic or sub-issue, or a todo (the plane's own
tracker). A chat links to at most one; a work item may have many chats. It is part of a
Workspace, shown in its Work section, and never a sixth concept (ADR 0072).
_Avoid_: task, ticket (in UI text), card (that is how a board draws one)

**Tracker key**:
A work item's identity: the tracker's name and the item's own reference where it lives, such as
`github:github.com/owner/repo#12` or `todo:<workspace>/<todo>`. Two devices name the same item
the same way. When an item moves, its old key is kept and points to the new one. It belongs to
**Workspace**, with the work item it names (ADR 0088).
_Avoid_: issue id, forge id (that is the forge's own identifier, which travels beside the key)

**Work link**:
A workspace's or a chat's record that it works on a work item. A chat has at most one; a
workspace may have many. The project keeps them, so they reach every device. It belongs to
**Workspace** (ADR 0088).
_Avoid_: link (unqualified: that is the runner link), relation

**Relation** (between work items):
How one work item stands to another: a pull or merge request that closes it, blocked-by, or
parent and child. It belongs to **Workspace**, with the work items it joins (ADR 0088).
_Avoid_: link (unqualified), dependency (that is one kind of relation)

**Plane root**:
The plane's own directory, as a place a chat works — and anywhere else in the plane that is no
workspace's, such as `docs/`: the workspace strip's first tab, drawn as an icon, always there. A chat started there is in no workspace on purpose — it looks after the
plane and names a workspace with `-w` when it acts on one. It is not a workspace: it has no
charter, memory or todos. In code it is still `OUTSIDE`, the strip every chat working in no
workspace is filed on.
_Avoid_: master, home, outside every workspace (in UI text), the default workspace

**Repo** (of a workspace):
A clone of a code repository that a workspace holds. It has its own remote and its own rules,
and it is never part of the plane's commits.
_Avoid_: clone (as a noun in UI text), guest checkout, project

**Piece**:
A git worktree of a workspace's repo, at `workspaces/<ws>/.worktrees/<repo>/<piece>`, where
one chat works on its own branch. Git says which pieces exist. The piece log says what git
cannot: that charter cut it (`claimed`), and whether its worker declared it `done` or
`abandoned`. A piece that declared nothing is **silent**, reported as an age and never as a
failure.
On screen a piece is shown as its **branch**; "piece" stays in the plane format, the code and
`charter worktree` (ADR 0072).
_Avoid_: task, slot, branch (for the directory)

**Branch** (of a chat):
What the window shows for a piece: one row per repo a chat works in, reading *`<branch>` in
`<repo>`*. A chat working in a repo's shared clone shows the branch the clone has checked out,
marked *shared*. **New branch** cuts a piece; **Remove folder** removes the worktree and keeps
the branch (ADR 0072).
_Avoid_: the chat's branch (singular: a chat may work in several repos), piece, worktree (in UI text)

**Writing chat**:
A chat that starts in a repo's shared clone (`workspaces/<ws>/<repo>`), which is what *New tab
in <repo>* and *Start new chats in <repo>* point at. By default it starts on a new branch of its
own instead: a piece named after the chat, or `chat-<n>`, cut before it starts and taken back if
the start is refused. The picker's *start on a new branch* box, ticked by default, is the
opt-out. A chat started in the workspace's own directory, at the project root or in an existing
piece is not a writing chat and is cut nothing (GL-1).
_Avoid_: guessing from what a chat does (charter never reads a harness's output to decide)

**Change** (cross-repo):
One piece of work across several of a workspace's repos, recorded as intent only in
`workspaces/<ws>/changes/<slug>.json`: why, which repos, which branch in each, and which must
land first. Whether each part is pushed, checked or landed is read from git and the forge, and
never stored (ADR 0060).
_Avoid_: change (for one pull request), changeset, epic

**Member**:
One repo's part of a change: the repo, its branch for this change, and the members it `needs`
landed first.
_Avoid_: part, sub-change

**Request**:
A member's pull request, or merge request on GitLab.
_Avoid_: change (for a PR), MR/PR in UI text

**Landed**:
A member whose request the forge reports merged, that charter landed, and whose merge commit, as
charter's landing log recorded it, is still on the default branch. A merge charter queued, or
whose read-back failed, is landed once a later `charter change land` finds it merged at its
**pending landing**'s head and logs it.
_Avoid_: merged (a browser merge is merged but not logged), done

**Pending landing**:
Charter's evidence that it started landing a member: the request, the head its checks passed on
and how (direct or queue), written before the forge is asked. Only a merge at that head is ever
logged as charter's; a refused one is no evidence.
_Avoid_: queued (a pending landing is also written for a direct merge), in flight

**Inventory**:
The plane's list of repos it can clone (`inventory/repos.json`), committed and shared. It only
grows: `discover` and the repo picker add to it, and a repo leaves it only through an exclude.
_Avoid_: repo list (for what the picker shows), catalogue

**Reachable repos**:
The repos your own forge login can reach under the plane's owners, as the repo picker shows
them. Asked each time the picker opens and never saved, because each engineer reaches different
ones.
_Avoid_: discovered repos, the inventory

**Forge account**:
One sign-in to one forge host: a kind, a host and a login, held in the keyring or reached
through `gh`'s or `glab`'s own login. Each repo is bound to one (ADR 0070). It is the human's,
signed in from the window through a forge registration, a PAT or an imported CLI login, and it
never reaches a chat. It holds for the whole machine, so it is a setting of the **Project**
(ADR 0072 §2, ADR 0077).
_Avoid_: forge login (for charter's own sign-in), connection, integration

**Forge registration**:
What a forge host knows charter by when a person signs in: a GitHub App on GitHub, an OAuth
application on GitLab, identified by a public client id. charter's own exist on github.com and
gitlab.com; a GHES or a self-managed GitLab needs one made on that host. It mints nothing for an
agent. A setting of the **Project**, like the forge account that uses it (ADR 0072 §2, ADR 0077).
_Avoid_: OAuth app (unqualified), integration, client

**Forge capability**:
One thing a forge may or may not do for one repo, such as a merge queue, judged per forge, host
and tier, with the fallback charter uses where it is unavailable (ADR 0070).
_Avoid_: capability (unqualified, which is an extension's), feature flag

**LIVE / LOCAL**:
Whether a workspace's charter, memory and todos are published with the plane (LIVE) or stay on
this machine (LOCAL, the default).
_Avoid_: shared/private, public

**Tier** (of a store):
Where a file charter keeps lives, and so what a backup, a second machine and a deletion do to
it. **Plane** is committed. **Clone state** is per clone and never committed: `.charter/`,
`charter.local.toml` and a LOCAL workspace's files. **Machine** is outside every plane, and each
store there is syncable or device-bound. **Keyring** is the operating system's credential store.
A derived store is also marked rebuildable (ADR 0069).
_Avoid_: app data (for the Machine tier as a whole), cache (for clone state), local state

**Clone-key**:
A project clone's identity on a machine: the first 16 hex characters of the SHA-256 of the
clone's canonical root path. It keys the reopen record's `clone` (V43), the search index will
be keyed on it (ADR 0079), and a save branch's default name takes its first six. A clone's own
word for itself, never committed, and different in every copy at another path. A copy at the
same path on another machine has the same key, and the device id is what tells it apart.
_Avoid_: clone id, plane hash, install id

**Copy** (of a project):
A second clone made from a project's directory, `.charter/` and all: `cp -R`, rsync, a backup
restored beside the original, or the same path on another machine. Its first launch finds that
another clone still holds its chats, or that another device wrote its record, and gives every
chat a new id (V43). A `git clone` is not a copy in this sense: it carries no `.charter/`.
_Avoid_: fork (that is a workspace's), duplicate

**Move** (of a project):
The same clone at a new path on the same machine. Its first launch finds no other clone holding
its chats, so the chats keep their ids and the record names the new clone-key (V43).
_Avoid_: rename (that is a workspace's), relocation

### Runs and devices

**Run**:
One stretch of a chat's conversation with a harness, over which its persona, harness, profile,
model source, device, sandbox and harness level stay the same. A chat has one or more runs, one
after another. A new one begins when the chat starts, on `/clear`, when the app reopens it, when
it wakes, when it starts again without its conversation, or when any of those attributes
changes. Compaction keeps the run. A **child run** is a sub-agent or teammate the harness
spawns, with the run it came from as its parent. The run is who an action is attributed to (W8's
"agent run"), and budgets add up over a chat's runs (ADRs 0066, 0073).
_Avoid_: session (that is the process), conversation (that is the harness's), turn

**Run state**:
Where a run is. While it lives: `queued`, `starting`, `working`, `input-required`, `paused` or
`hibernated`. Then, once and for good: `completed`, `failed` or `stopped`. **Stopped** is an end
someone chose (the operator, a policy, the kill switch, the host); **failed** is one nobody chose.
A chat's state is its current run's (ADR 0076).
_Avoid_: status, session state (the old five), done (for completed), idle (for a state)

**Hot chat**:
A chat whose current run has a process: `starting`, `working`, `input-required` or `paused`.
An **open** chat is any chat whose current run is live, hot or not; a hibernated one is open and
not hot. charter's scale is counted in these per device: a target of 200 open, and a hot target
per **RAM class** (ADR 0082).
_Avoid_: active chat, live chat (live is a run state's), running chat (in UI text)

**RAM class**:
A device's physical memory, as the largest of 8, 16, 32 and 64 GB it reaches, which sets how
many hot chats the device targets; a device under 8 GB targets one. A property of the device, so
it belongs to Project. A budget, never a cap: charter warns past it and refuses nothing (ADR 0082).
_Avoid_: tier (that is a store's), machine size, profile

**Performance budget**:
One measured limit on what charter's chats cost the device: a count, a size, a time or a share of
a core, stated at the device's hot target, and named with the job that measures it. It is
checked in CI either exactly or against what `main` last recorded, or on the operator's machine
at each release. It is a target, never a promise, and never a cap on the operator. The spec's
speed and memory limits are performance budgets too; its open and hot chat counts are targets. It belongs to **Chat** (ADR 0086).
_Avoid_: budget (unqualified: that is a chat's spend budget), limit (for a row CI checks only
against earlier runs), SLA

**Remote chat**:
A vendor-cloud session charter lists read-only, with its state, pull request and cost: a chat of
kind **observed**, which charter never pauses, stops or counts toward a budget. Every chat
charter starts is **governed** (W8, ADR 0076).
_Avoid_: cloud chat, external agent, remote runner (that is a **Runner**)

**Device**:
A machine charter runs on: a desktop, a runner, or later a viewer. Each has a random id kept in
its machine store, which is how records, events and the audit say where something happened. Its
hostname is a label, never a key. The operator on a device is its **local principal**
(`local:<device>/<os-user>`), and charter never sends it anywhere without an account (ADR 0066).
_Avoid_: host (that is `charterd`, the process), machine (in UI text), node

**Harness declaration**:
Data that says how to start one harness, how to name and resume its sessions, which levels it
offers and what it can do. The ones for Claude Code, Codex and opencode ship with charter; a
project may declare more, which each machine approves before they run, and never replaces a
built-in's (ADR 0073).
_Avoid_: harness definition, harness config (that is the harness's own), profile (that is which
program runs on this machine)

**Harness level**:
How much charter learns from a chat's harness, set when a run starts and fixed for it: **1**,
the terminal alone; **2**, the terminal with the harness's own hooks reporting to charter; **3**,
a structured protocol, ACP or the harness's own. A fall back to a lower level starts a new run.
Never shown on a first-hour surface (ADR 0073).
_Avoid_: tier (that is a store's), mode, integration level

**Harness capability**:
One thing a harness does or does not do for a chat, such as report that it is waiting: yes, no
with the fallback charter uses, or unknown, which reads as no. The capability card shows the
*no*s in plain words (ADR 0073).
_Avoid_: capability (unqualified, which is an extension's), feature, support

**Model choice**:
The model a chat runs on: the model, its provider, and the **model source** it is reached
through. Set on the chat, or given as a default by its persona, its workspace or the project, and
fixed for a run. It is always shown with the account it is billed to (ADR 0087).
_Avoid_: model config, model settings

**Model source**:
How a chat reaches its model: the harness's own **login**, a **key** the user keeps in a vault,
or a **local** model. None of them passes through Charter's servers. charter never reads, stores
or relays a harness's login (ADR 0087).
_Avoid_: provider (that is who serves the model), backend, BYO (for the whole of it)

**Session host** (`charterd`):
The process that owns every chat's terminal on a device, one per OS user per device: the
`charter` binary run as `charter serve`. The app is its client (ADR 0068).
_Avoid_: daemon, server (in UI text), backend

**Runner**:
A device, other than the one the window is on, whose own session host runs a workspace's chats,
serves a version of one of its repos, or runs browsers, as its **label** says. A long-lived runner
is a machine the operator adds; a short-lived one is made by a **runner provider** for a human or
a chat, for minutes or hours. Either way the desktop's session host reaches it through a
**connector** and talks to it over a **link**, and its shell, logs, ports and browser views come
only that way. It keeps its own device id, event log, audit chain and kill switch, and it needs
no server charter runs. A short-lived runner is owned by the chat that started it, or by its
workspace when a human started it or **pinned** it there. A runner is a device, so it belongs to
**Project**; which runner a workspace's chats run on is a **Workspace** setting, and the
**Runners** view tab shows a workspace's runners (ADR 0072 §2, ADR 0078, ADR 0089).
_Avoid_: remote (unqualified), agent host, worker, server (in UI text), sandbox, environment, box,
belongs to (for the lifecycle sense: say "is owned by")

**Connector**:
The command whose stdin and stdout reach a runner's session host: `ssh <alias>`,
`gh codespace ssh`, `coder ssh`, `docker exec -i`, `kubectl exec -i`, and later the relay. Held
as an argument vector on the machine that uses it, never committed in a project. It gives
reachability, never identity. It belongs to **Project**, with the device it reaches (ADR 0072
§2, ADR 0078).
_Avoid_: transport (that is FD-4's framing), tunnel, provider (that is the **runner provider**,
which creates the machine)

**Link**:
The encrypted, mutually authenticated stream between the desktop's session host and a runner's,
carried by a connector. Each end proves itself with its device's link key, pinned when the runner
was added, in a Noise `XX` handshake. The desktop always opens it. It belongs to **Project**,
with the two devices it joins (ADR 0072 §2, ADR 0078).
_Avoid_: connection (unqualified), session (that is the process), pairing (that is how the keys
were pinned)

**Label** (of a runner):
What a runner is for: exactly one of `harness` (runs chats), `app` (serves one version of a
workspace repo) or `browser` (runs browsers only charter drives). charter enforces it at both
ends of the link: only a `harness` runner starts chats, and a `browser` runner never receives a
vault value. Tags, any words the operator picks, sit beside it and are enforced by nothing. A
part of **Runner**, so it belongs to **Project** (ADR 0089).
_Avoid_: role (unqualified), type, capability (that is an extension's), tag (for the role label)

**Runner provider**:
What makes and destroys short-lived runners and hands back a connector to each: charter's own
core provider, which drives Docker or Podman on this machine, or an extension on the provider
seam for anything else. It declares which labels it may make; whether its machines are the
user's own is the operator's setting for it, never its own claim, and starts as not owned. A
part of **Project**, beside the extensions it is one of (ADR 0078 §6, ADR 0089).
_Avoid_: provider (unqualified: a vault's provider is where a vault keeps its values, ADR 0047,
and a model's is who serves the model, ADR 0087), backend (in UI text), executor, cloud,
connector (that only reaches a runner)

**Grant** (to a runner):
A human's approval, once per workspace on this machine, of one vault entry an app runner's spec
declares it needs. charter injects the value when that runner starts and only when it builds a
ref a human chose, never an agent's own unreviewed branch, and never into its image or logs. An
agent may start a runner with the grants it has and never adds one; a `browser` runner, and a
runner of a provider not marked as owned, never gets one. A setting of **Workspace** (ADR 0089).
_Avoid_: permission, secret (for the approval), capability (that is an extension's)

**Editor protocol**:
The part of the session host's public protocol an editor integration speaks: find which chat or
workspace a file belongs to, ask the window to show one, type a selection into a chat's prompt
without sending it, and list a chat's changed files. Nothing in it starts, sends to or answers a
chat. It is a part of the session host, so it belongs to **Project** (ADR 0072 §2, ADR 0081).
_Avoid_: editor API, editor extension API (that is a non-goal: code running inside charter's editor)

**Audit**:
The record of who did what, for whom, to what, and whether it was allowed, kept per device by
the session host as **audit entries** in charter's data home, never in a project. Once AU-3
lands, the entries are a device-signed hash chain. It is never sampled, and it is a separate
system from telemetry (ADR 0075).
_Avoid_: log (unqualified), history, `charter secret audit` (that is a vault health report)

**Audit entry**:
One line of the audit, written from one event: an action, its actor and whose behalf it acted
on (both as keyed pseudonyms), what it acted on, its outcome, and typed metadata. Never a prompt,
output, file contents, raw arguments or a secret value. An agent's entry says what charter could
see of its run: its **coverage** (ADR 0075).
_Avoid_: audit event (the event is what the entry is written from), log line

**Coverage** (of an audit entry):
What charter could see of the run an agent's audit entry is about, set by its harness level:
the process only (level 1), the tool calls its hooks report (level 2), or every tool call its
protocol reports (level 3). A level-2 run whose hooks never reported is **unarmed**, and a
vendor-cloud chat charter only lists is **observed**. Shown beside the entry, so silence is never
read as "did nothing" (ADR 0073, ADR 0075).
_Avoid_: level (that is the run's), completeness

**Telemetry**:
What charter measures about how chats and charter itself perform: time, resources, tokens and
cost. It is one OpenTelemetry pipeline per device: the session host receives what the harnesses
and charter report, keeps it on the device for a month, and sends it only to the user's own
**export destinations**. The opt-in product telemetry and crash reports are telemetry too, each
with its own consent. It may be sampled, never names a person, and never reads the audit (ADR
0075, ADR 0083).
_Avoid_: audit, analytics, metrics (for the whole of it), log (that is the diagnostic log)

**Content gate**:
A switch that lets telemetry keep what was said in a chat: `prompts`, or tool arguments and
results (`tools`). Both are off by default. Only the human opens one, for a project, on this
machine; a project's settings or an org's policy can only close it (ADR 0083).
_Avoid_: redaction (that is removing secrets from text), privacy mode

**Export destination**:
A backend the user added, such as Langfuse, Grafana or Datadog, that the device's telemetry is
sent to over OTLP. A machine setting, never a project's, and later also an org's (ADR 0083).
_Avoid_: exporter (that is the harness's own sender), sink (that is the audit's)

**Search index**:
A derived SQLite full-text index that search, `recall` and the briefing read, kept so they need
not scan every file. Each project clone has one over its memory, session records and todos, in
`.charter/`; each machine has one over the transcript archive, in charter's data home. The
session host is its only writer, and a chat searches only its own clone's. It is never the
truth: deleting it costs a rebuild. It holds the words of what it indexes, with their positions
and after redaction, but not the text as written. A deleted item stops answering at once and
leaves the file at the next merge and checkpoint; an unlinked old copy is not overwritten. It
never answers a reader with anything the files would not (ADR 0079).
_Avoid_: index (unqualified: `memory/index.md` and `sessions/index.md` are lists for people),
cache, database

### The window

**Split window**:
An OS window a project tab was moved into, beside the main window. It holds its own projects,
and closing it moves them back to the main window with every chat still running.
_Avoid_: detached tab, pop-out, secondary window

**Strip**:
One row of tabs: projects (in the title bar), a project's workspaces, or a workspace's chats.
A strip's order never changes on its own, and it never scrolls. The operator can drag a tab
along it; dropped among the pinned tabs it is pinned, and among the others it is unpinned.
_Avoid_: tab bar, scroller

**Pin**:
One operator's mark that a project, workspace or chat matters to them. It is kept on this
machine and never in the plane. A pinned item is drawn first, and the workspace strip draws
only pinned workspaces plus the one you are in.
_Avoid_: favourite, star, bookmark, pin (for a runner: that is **Pin** (of a runner))

**Pin** (of a runner):
A human's act that gives an agent-started runner to its workspace: the runner is then owned by
the workspace instead of the chat, outlives the chat, and stops only by a human or the kill
switch. Kept in the runner's record on this machine. A part of **Runner**, so it belongs to
**Project** (ADR 0089).
_Avoid_: keep, hold, pin (unqualified, which is the strip's mark)

**Show-more**:
The button at the end of a strip that lists what the strip is not drawing, sorted by activity,
with the needs-you count of everything it hides.
_Avoid_: overflow (in UI text), more tabs

**Needs you**:
A chat that is waiting on the operator: a view, computed from its current run (an ask, a turn
that ended, a budget or policy pause) and its own items (a report back, a refused commit, a
secret waiting for approval), and never a state of its own. Every project's are listed in the
title bar's ✋ menu, and each is counted in red on its tab and on any show-more hiding it.
**Ignore** clears a chat's items until the next one arrives (ADR 0076).
_Avoid_: notification, alert (alerts are a separate drawer), waiting (for the state)

**Ask**:
A chat's harness handing control to a human: a permission, a question, or a nudge. Every
harness's own form of it is read into one shape: what it would do, the options as the harness
offered them, who may answer, its deadline, its risk, a masked one-line summary, and whether it
elicits a secret. One answer per ask, and the first wins; every later one hears "answered
elsewhere". An agent never answers one (ADR 0080 §5). A part of **Chat**.
_Avoid_: prompt (that is what the operator types), approval (that is one kind of answer)

**Kill switch**:
Stop all on the title bar, or `charter stop --all`: every chat's and shell's program that charter
started, in every project and window, is interrupted and ended, and no chat starts until the
operator **re-arms** it from the title bar. It is a stop, not a close: the tabs stay, each
reading as a chat whose program ended. A new shell still opens, so the operator can look around.
Nothing on the command line re-arms (ADR 0071).
_Avoid_: panic button, pause (nothing is resumed on re-arm)

**Shell tab**:
A tab running the operator's own shell, with no harness and no profile, opened by `New shell`.
A harness typed into one runs outside charter's session tracking, so charter's **shell-tab
shims** stand first on its `PATH`: the harness still starts, after one line saying so, and the
tab shows a banner offering to open it as a chat instead (ADR 0062).
_Avoid_: terminal (for the tab), console, plain chat

**Light editor**:
charter's one editor, for reading a file of a chat's branch, making a small edit in it and
reviewing a diff. It has no language server, debugger, repo-wide refactor or extension code of
anyone else's; deep work opens in **your editor** at the same file and line. It is a view of a
**Workspace**'s repos (ADR 0072 §2, ADR 0081).
_Avoid_: editor (unqualified), code editor, IDE (except in the category phrase "the agent IDE",
which is charter as a whole)

**Your editor**:
The editor the operator already uses (VS Code, Zed, a JetBrains IDE, or `$EDITOR`), where writing
code by hand happens. charter opens a file there at a line, and never replaces it. Which editor
is a setting of this machine, so it belongs to **Project** (ADR 0072 §2, ADR 0081).
_Avoid_: external editor; IDE (except in the category phrase "the agent IDE")

**Comparison**:
A base and a head in one of a workspace's repos, which git turns into the diff a Review tab shows: a
chat's branch against its base, two refs, uncommitted work, one agent turn, a request. A
cross-repo change is one comparison per member. It is a part of a **Workspace** (ADR 0084).
_Avoid_: diff (for the pair; the diff is what git computes from it), changeset

**Review tab**:
A view tab showing one comparison (or one per member of a cross-repo change), where the operator
reads the diff, ticks files as viewed, comments on lines, and ends by sending the comments to the
chat, publishing them to the request, or approving. There is one per branch, and it remembers
where the operator was. The forge's own review of a request stays the review of record. "Review"
on its own is the plain verb, as on the chat tab's button, and is not a charter noun. It is a view
of a **Workspace**'s repos (ADR 0084).
_Avoid_: Review (as a noun for the tab), code review (for charter's), PR review (that is the
forge's), diff view

**Review draft**:
A Review tab's unsent comments: each one's line and the operator's words, never code. Only the
window writes it, it is kept on this machine and travels with the operator's other machines,
and it leaves only by **Send to agent**, **Publish** or being kept. It belongs to **Workspace**
(ADR 0084).
_Avoid_: pending review (that is GitHub's), draft (unqualified)

**Human edit**:
An edit the operator saved to a file in a chat's branch from the light editor. charter records
which file and lines, and announces it to the chat's harness at its next turn: one line of
context naming the files edited since its last turn, paths only, never contents. Edits from
several sittings add up until that turn, and the announcement is never a prompt sent for the
operator. It is a part of a **Chat**'s history (ADR 0084).
_Avoid_: manual edit, override

**Session record**:
A summary a chat writes of its own session when it closes through **Smart close** — its goal,
what it did, what it decided, what is still open and how to pick it up — filed as one file in
its workspace's `sessions/` (the plane's own, at the plane root). The chat gives the title and
the five sections; charter gives everything else (which chat, persona, harness, profile,
conversation, workspace, directory and pieces), keeps the index and `workspace.md`'s one `## Sessions` line, and names
the newest in the next chat's briefing (ADR 0064).
_Avoid_: handoff, log, transcript

**Smart close**:
Closing a chat after it has written its session record: the app sends it one line naming
charter's `smart-close` skill (the operator's click is the consent, the exception to a curation
action's never-sent prompt), and the tab closes when `charter session record` tells the app the
record is saved — never on anything the chat printed. Until then the chat is **wrapping up**:
its tab says so, and the operator's Cancel smart close or their own typing stops it.
_Avoid_: save and close, archive

**Resume** (of a session record):
Starting a NEW chat from a session record, in the record's place — the directory it ran in,
where that is still in the place — on its harness and the profile it ran on where this machine
still has it, given its conversation where the harness can still find it, and with the record
quoted in its briefing. What it had to guess instead, it says.
Where the conversation cannot be given it is a fresh chat with the record, and it says why. It
never reopens the chat that wrote the record.
_Avoid_: reopen, restore (a relaunch reopens the chats that were open)

**Plane updated** (of a chat):
A chat started before the plane's start-time instructions (`CLAUDE.md`, the harness settings
and sub-agents, a persona's charter) changed on disk. It runs on what it read until it is
started fresh, and its tab carries a quiet mark saying so. It is not a needs-you item.
_Avoid_: stale, behind, outdated, Incoming (that is the remote's commits)

### Saving

**Save**:
Taking what changed in the plane or a repo as far as its mode allows: commit, push, PR, merge.
_Avoid_: sync (that word is `charter sync`'s), commit (a save may be more than one), publish

**Sync**:
Fetching every clone in a workspace and fast-forwarding the ones that hold no work:
`charter sync`, and *Sync repos* in the window. Nothing else is called Sync: not a save, not
moving a project to its pinned version, not writing persona sub-agents, and not any state kept
between devices (ADR 0072).
_Avoid_: pull, refresh, update (for this)

**Mode**:
How far a save goes: `off`, `commit`, `push`, `pr` or `pr-merge`. Each value includes the
steps of the one before it. The plane has one mode, and each repo has its own.
_Avoid_: policy, posture, share

**Auto-save**:
A save charter starts by itself: after a quiet period, when a session ends, or when the app
quits.
_Avoid_: sync, background push

**Target branch**:
The branch a save is meant to end up on.
_Avoid_: base branch, main (it need not be)

**Save branch**:
The one branch per clone of the plane (named for the machine and the clone) that the plane's
PR modes push to, carrying one open PR into the target branch.
_Avoid_: PR branch, `charter/<sha>` branch

**Stage**:
Where unsaved work sits. It is *changed* (not committed), *committed* (not pushed), *pushed*
(PR open), or *saved* (on the target branch).
_Avoid_: status, sync state

**Blocked**:
A save that can't go further without a person: a conflict or a merge or rebase git stopped
part-way, a refused push, a secret the scan caught, a mode the remote can't take. Auto-save pauses until it's cleared.
_Avoid_: failed, error, stuck

**Incoming**:
Commits on the remote that this machine doesn't have yet.
_Avoid_: behind (in UI text)

**Commit scan**:
The check a chat's own commit passes before git makes it: the lines it adds, scanned for keys
and personal data, and refused with each finding masked. `charter scan` runs it on what is
staged. Part of Workspace, as a check on a repo's save (ADR 0074).
_Avoid_: secret scan (that is the plane save's), leak check

**Allowlist** (of the commit scan):
A repo's `.charter-scan-allow.toml`: the findings the commit scan lets through, each entry with
its reason, committed by the operator and read as it is at `HEAD`. A setting of a repo.
_Avoid_: ignore list, exceptions, whitelist

**Shared / Local** (settings):
Where a setting's value comes from: `charter.toml` (committed, the team's) or
`charter.local.toml` (this machine's). A Local value overrides the Shared one key by key.
_Avoid_: global/user, project/personal

### Core and extensions

**Core**:
What charter does itself, on every platform, with no extension on. A plane's instructions and
the session-start briefing may depend only on the core.
_Avoid_: built-ins (for core features), platform

**Extension**:
A directory the operator installs and approves on this machine, whose manifest declares what it
contributes and which capabilities it asks for. Its program runs as the operator, one question
at a time.
_Avoid_: plugin, add-on, module

**Built-in extension**:
An extension that ships inside the app and is trusted through the app's signature rather than
an approval prompt. A copy of one anywhere else is an ordinary extension.
_Avoid_: bundled plugin, first-party plugin, core extension

**Capability**:
One thing charter does for an extension that asked for it in its manifest and was approved,
such as showing a badge or adding a CLI command. It describes charter's conduct, never a limit
on the extension.
_Avoid_: permission, grant (that is a runner's **Grant**), power

**Facts file**:
A file an extension keeps in its own state directory, holding the values charter shows for it
(badges, repo cells) without starting its program.
_Avoid_: cache, status file

**Event**:
One question charter asks an extension after a core action it hears about has finished, such as
a workspace being created or the plane being saved. What it answers never changes the action.
_Avoid_: hook (for this), notification, subscription

**Briefing section**:
Text an extension adds to a chat's session-start briefing, quoted as data under the
extension's name. It is never an instruction, and never a permission, a hook or a setting.
_Avoid_: prompt, context injection

**Action** (of an extension):
A verb an extension declares and offers on the rows of its views: pressing one asks its program
to *run action `<id>` on `<subject>`*. Never one of charter's own verbs. charter asks first when
the manifest says so, and always before one that deletes.
_Avoid_: command (for this), verb (unqualified), button

**Curation action**:
A chat charter opens on a workspace, a persona or the plane with a prompt already typed into it
and never sent: the operator reads it and presses Enter. charter ships three of its own
(`charter/safe-remove`, `charter/compact`, `charter/add-curation-action`), and a persona
declares more as `personas/<name>/curation/<id>.md`, which that persona runs. Unlike an
extension's **Action**, nothing runs a program: the chat is the whole of it (ADR 0061).
_Avoid_: action (unqualified), quick action, macro

**Palette command** (of an extension):
A row an extension adds to the palette, named with the extension's name, that opens one of its
views or runs one of its actions.
_Avoid_: shortcut, menu item

**Extension command**:
A command an extension adds to the `charter` command line, run as `charter <extension id>
<command> …`. It says whether it writes, and what its program prints and its exit status reach
the caller unchanged. An extension's id is never one of charter's own command words.
_Avoid_: subcommand (unqualified), plugin command, palette command (for this)

**Core-owned alias**:
A core command whose words forward to an extension command and give its output, so a plane's
instructions keep working when a feature moves into an extension (`charter ws todo` once todos
does).
_Avoid_: shim, redirect

**Write paths**:
The plane-relative paths an extension declares it writes. charter hands them resolved with
each request and reports a change outside them; it does not stop one.
_Avoid_: sandbox, allowed paths, scope (as if enforced)

**Harness adapter**:
charter code that arms one harness through its own mechanism for one chat, with nothing written
into the harness's config: what level 2 and a harness's own protocol need. It never stands in for
the harness's program (ADRs 0050, 0073).
_Avoid_: wrapper, driver, plugin (that is the harness's)

**ACP adapter program**:
A program the user installs that speaks ACP for a harness that does not, such as
`claude-agent-acp` or `codex-acp`. charter spawns it as a level-3 chat's program, found by name
on `PATH`, and never ships, downloads or updates one. It is not a harness adapter, which is
charter's code (ADRs 0073, 0080).
_Avoid_: ACP adapter (on its own), harness adapter (for this), bridge

**Wrap**:
To run a chat's unmodified harness inside a sandbox profile or backend charter generates (ADR
0067). Never to **stand in** for the harness: putting charter's own program where the harness's
is expected and changing what it or its model sees, which charter never does. X34's *"wraps a
harness binary"* means standing in (ADR 0073).
_Avoid_: wrap (for a stand-in, a shim or an adapter)

**Harness plugin**:
A Claude Code, Codex or opencode plugin, chosen per project. "Plugin" on its own always means
this, never a charter extension. charter's own is one too: the Claude Code plugin the app
bundles, named `charter` (`charter@inline`, skills `charter:<skill>`), always on in the chats
the app starts. It is not the Python charter's `charter@charter`, which is always off there.
`charter plugin install` puts a copy of it, `charter@charter-app`, in front of the chats the
operator starts outside the app (ADR 0057). For opencode, charter's own is the **opencode
shim**, a script the app loads into each opencode chat it starts, and whose guard-only variant
`charter plugin install` writes into opencode's plugin directory (ADR 0058).
_Avoid_: extension (for this); "charter plugin" for anything but charter's own

**charter's skills**:
The skills in charter's plugin (`skills/` in the bundle), one source for every harness. Each
harness is handed them by its own route, for the chat alone: Claude Code loads the plugin,
opencode is told the directory through the shim, and a Codex chat is **briefed** on them, a list
of names, descriptions and `SKILL.md` paths at `SessionStart` (ADR 0063).
_Avoid_: "Claude Code skills" for these; a copy of them anywhere

**Vault**:
A named set of secrets charter keeps in the system keyring and hands to a command, never to
the model and never to an extension. Vaults are core.
_Avoid_: secret store, keychain (as the name of the concept)

**Forge extension**:
An extension about a code host's pull requests, merge requests or issues, which reaches the
forge through `gh` or `glab`'s own login and never through a secret charter hands it. Once
PE-29 opens the forge seam to extensions, it asks charter to make the call instead (ADR 0070).
_Avoid_: forge plugin, GitHub integration

**Editor integration**:
charter's own extension for VS Code or Zed, or its own plugin for JetBrains IDEs: installed in
**your editor**, it opens a file's chat in charter, sends a selection to a chat's prompt for the
operator to send, and shows a chat's changes. It speaks the **editor protocol**, and is neither a
charter **extension** nor a **harness plugin**. It is a view of a **Workspace** and its chats,
drawn in your editor (ADR 0072 §2, ADR 0081).
_Avoid_: extension or plugin (on their own), IDE plugin
