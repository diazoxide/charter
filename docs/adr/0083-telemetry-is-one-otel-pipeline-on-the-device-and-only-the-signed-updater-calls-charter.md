# Telemetry is one OTel pipeline on the device, and only the signed updater calls Charter

**Accepted 2026-10-01** by the operator (ruling V34c), with dispatcher decisions D-0083a/b,
drafted for program-map ticket OB-1 (#686). It follows these of the operator's rulings:

- **O1:** *"Performance telemetry and the audit log are strictly separate systems, with separate
  stores, schemas and retention. The audit log is never sampled."*
- **O4:** *"No phone-home offline, except the signed updater. Product telemetry is opt-in, with a
  local viewer of what is sent. In orgs, the admin decides where members' telemetry goes (the
  org's own dashboard)."*
- **O5**, in part: *"One OTel pipeline. Local per-chat resources, cost and latency, plus export
  to the user's own backend (Langfuse, Grafana, Datadog) …"* and *"Charter product telemetry is
  opt-in only."* The org fleet dashboard it names is TM-2's, after GT-CLOUD.
- **O8**, in part, for the local store: *"the user decides"*, and *"Performance telemetry 14–30
  days."*
- **O9:** *"Crash reports: opt-in, out-of-process minidumps, Sentry SaaS in the EU, and the same
  local viewer."*
- **B2**, in part: what needs *"no account, no phone-home"* is *"recorded as an ADR and in the
  README, before any cloud code."* CF-1 is that record. This one holds the telemetry side of it.
- **X50**, accepted with the consistency review as recommended: *"Is the B2 ADR worded as "no
  network calls to Charter's servers without an account, except the signed updater; third-party
  calls that features make are listed in a local, viewable network log"?"*
- **V18:** *"The 73 findings of the map consistency audit are applied as written in its proposed
  fixes, since each follows from an existing ruling; where a finding and a ruling disagree, the
  ruling wins."* One of them is C-21, whose fix reads: *"… except the signed updater and the
  static signed files beside it (the first-party catalogue and the revocation feed, E4), each
  listed in the local network log"*.
- **V13**, in part: *"the first update check of each ISO week fetches an identical alternate
  manifest asset whose download count estimates weekly users with no identifier"*.
- **V25d:** *"Telemetry may derive its OTel logs from the event log, and never reads the audit
  store."*
- **W2**, in part: *"charter emits persona, workspace, piece and on_behalf_of as OTel resource
  attributes and ships a dashboard pack for Datadog/Grafana"*. D-0083a narrows it (below).
- **W13**, in short: the cloud waits for the GT-CLOUD gate, and *"OTel attribution with a
  Datadog/Grafana dashboard pack (W2)"* is built now, on the device. Nothing in this record
  waits on GT-CLOUD.
- **V34c**, the ruling on this record (*Ruled*, below): a spend budget may pause a chat on
  harness-reported spend.

It builds on [ADR 0042](0042-charter-updates-itself-and-nothing-it-cannot-verify-reaches-it.md)
(the signed updater), [ADR 0066](0066-a-chat-is-a-ulid-a-run-is-a-stretch-of-its-conversation-and-a-device-is-random.md)
(ids and the OTel attributes), [ADR 0067](0067-a-chat-runs-in-a-sandbox-charter-compiles-for-its-harness.md)
(the chat sandbox and its egress proxy), [ADR 0068](0068-a-chat-lives-in-charterd-and-the-app-is-its-client.md)
(`charterd` and the event log), [ADR 0069](0069-every-store-charter-writes-is-in-one-of-four-tiers.md)
(tiers), [ADR 0073](0073-a-harness-is-declared-as-data-and-a-chat-runs-it-at-one-of-three-levels.md)
(harness adapters), [ADR 0075](0075-an-audit-entry-is-metadata-in-a-store-of-its-own-and-telemetry-never-reads-it.md)
(audit and telemetry apart) and [ADR 0076](0076-a-run-moves-only-by-a-named-cause-and-a-chats-state-is-read-from-its-runs.md)
(the budget pause). It sits beside FD-8 (#647, PR #794), whose diagnostic log is neither audit
nor telemetry. It **amends** ADR 0066, ADR 0067, ADR 0068, ADR 0069, ADR 0075 and ADR 0076,
each in a section of its own below. OB-2, OB-3,
OB-4, OB-5, OB-9, OB-10, OB-12, OB-14, OB-15, OB-18 and OB-19 build on it.

Its concept is **Chat**: what telemetry measures is chats and their runs
([ADR 0072](0072-charter-has-five-concepts-and-every-other-word-belongs-to-one-of-them.md)). The
receiver is a part of the session host, so that part belongs to **Project**, as the audit does.

## Where charter is today

- **charter collects no telemetry.** There is no receiver, no exporter and no store. No harness
  is started with OTel settings, so Claude Code, Codex and opencode export nothing to charter.
- **Two things already reach the network on their own.** The updater fetches `latest.json` from
  GitHub Releases and verifies what it downloads (ADR 0042). `charter report` files an issue
  through the operator's own `gh`, after showing the text (#363). Neither sends a usage number.
- **Three records already name telemetry and leave it to OB.** ADR 0066 fixes four `charter.*`
  attributes, `host.id` and `gen_ai.conversation.id`, and says `enduser.id` is never set. ADR
  0075 says telemetry never reads the audit, may be sampled, never names a person, keeps *"OB's,
  14 to 30 days (O8)"*, and lives in *"the OTel pipeline's own store"*, which no record places.
  ADR 0070 sends every forge call to the network log (OB-15).
- **ADR 0076 pauses a chat on its budget** as *"The host's own act, from its own counts"*. No
  count of spend exists yet.
- **FD-8 (PR #794) adds a diagnostic log**, `<app log>/charter.<date>.log`, redacted, kept seven
  days. It rejected `<data>` for that log because `<data>` is *"the durable, backed-up home of
  the audit"*, and says *"a diagnostic log is neither audit nor telemetry"*.
- **The harnesses differ** (research 04 §2.1). Claude Code exports eight metrics, twenty-odd log
  events and beta traces, keeps prompt and tool content out unless `OTEL_LOG_*` gates are set, and
  attaches the signed-in user's account and email to its records. Codex exports when an `[otel]`
  section says so, and `codex exec` sends no metrics. opencode exports spans behind an
  experimental flag. The GenAI semantic conventions are still marked *Development*.

## The decision

### 1. One pipeline: one receiver, one store, one exporter, on each device

**Settled by O5:** *"One OTel pipeline."* On each device it is:

```
harness exporters (OB-3) ──► egress proxy ─┐
charter's own spans (OB-10) ───────────────┼─► receiver in charterd ─► check, strip, gate (§2–§4) ─► local store (§7) ─┬─► charter's views (OB-4..OB-7, OB-14)
host counters (OB-19) ─────────────────────┤                                                                          └─► the user's export destinations (§8)
the event log (V25d) ──────────────────────┘
```

- **The receiver is in `charterd`**, the one host per OS user per device (ADR 0068). The window
  reads telemetry from its host like everything else. There is no Collector binary and no second
  process.
- **Four sources, and only four.** The harnesses' own exporters, charter's own spans, the host's
  counters, and OTel logs derived from the event log (V25d). A new source is a change to this
  record. The last three are inside the host and need no credential.
- **Two things are never a source.** **The audit store**, settled by V25d and O1. **The
  diagnostic log** (FD-8): it is unstructured text, redacted by shape, and written by the app
  process. No `tracing` bridge feeds it to OTel. charter's own spans (OB-10) are made from events
  and the host's own timing, not from log lines.
- **Product telemetry and crash reports are not an export of this pipeline** (§9).

### 2. The receiver: OTLP/HTTP behind the egress proxy, and a run is known by its credential

- **OTLP over HTTP, protobuf and JSON.** No gRPC: every harness can send HTTP, and OB-9's export
  is HTTP too. Types come from `opentelemetry-proto` (OB-2).
- **Settled by D-0083b:** *"OTLP from a sandboxed chat goes through charter's existing loopback
  egress proxy (ADR 0067 §3). There is no new sandbox allowance. A whole-process sandbox (ADR
  0067 §6) states that it reaches the receiver only through the proxy."* This record applies it
  to every chat, sandboxed or not, so there is one path:
  - **The receiver listens on a unix socket** in the host's `0700` directory, beside the host's
    own socket. It opens no TCP port. ADR 0068's rejection of a loopback port stands.
  - **The adapter points every harness's exporter at the egress proxy** (OB-3), which forwards
    OTLP requests to the receiver's socket and nothing else on that route.
  - **Under a whole-process sandbox**, the profile already allows traffic only to the proxy
    (ADR 0067 §3), so the receiver is reachable and nothing new is opened.
  - **Under a harness's own sandbox**, which confines its tool commands, the exporter runs in
    the harness process and still sends to the proxy, because that is the endpoint it was given.
  - **The `localhost` preset neither opens nor closes the receiver.** It lets a chat's commands
    reach the operator's own services on loopback. The receiver is not on loopback, and the
    proxy's OTLP route is the same whether the preset is on or off.
- **Each run is given its own receiver credential at spawn**, as a path prefix in the endpoint:
  `http://<proxy>/otlp/<credential>`. OTLP's base-endpoint rule appends the standard
  `/v1/traces`, `/v1/metrics` and `/v1/logs` to it, so every exporter uses the standard paths
  under that prefix. A prefix reaches every signal's exporter, including Claude Code's beta trace
  exporter, which ignores extra headers (research 04 §2.1). The credential is minted fresh for
  each run, and dies with the run.
- **The credential names the run. The receiver checks what the record says against it:**
  - **Resource-level identity is overwritten** from the credential: `host.id`, and §6's
    `charter.project`, `charter.workspace`, `charter.persona`, `charter.harness` and
    `charter.branch`.
  - **Per-record ids are checked against the credential's run tree.** `charter.chat.id` must be
    the credential's chat. `charter.run.id` and `charter.run.parent_id` must be the credential's
    run or one of its child runs, so a child's records keep their own run (ADR 0066). A record
    that names a run outside that tree has its ids replaced by the credential's run, and the
    mismatch is counted. Where a record carries none, the receiver sets them from the credential.
  - **`charter.event.id` is kept.** On records from the host's own sources it is the event's
    `ulid`, as ADR 0066 says. On a record that arrives under a run's credential it is kept only
    if it names an event of that chat, and is removed otherwise.
  - A record with no credential, or a dead one, is dropped and counted.
- **What that holds, exactly.** A record is attributed to the chat whose credential it came
  under. A chat in the sandbox cannot read another run's credential, because the sandbox denies
  it other processes' environments and the host's directory (*ADR 0067, amended*), so **within
  the sandbox a chat can write records only about itself**. An **unsandboxed** chat runs as the
  same OS user as every other chat and can read another run's environment, so for it this is
  **best-effort**. That is the same residual every opted-out chat carries (ADR 0067 §7).
- **It answers at once and never pushes back.** It accepts a request, queues it, and replies
  success. When the queue is full it drops the oldest work and counts the drop. A harness never
  retries into its own memory because charter was slow, and no hook waits on telemetry (ADR 0075
  §1: *"dropped quietly, and counted"*). The counts are rows in `charter doctor --perf` (OB-19).
- **A chat whose harness cannot export still runs.** Its meters say *not reported*, and nothing
  else changes (OB-20's rule).

### 3. What the receiver strips before anything is stored

- **People are removed.** ADR 0075 settles that telemetry *"never names a person"*. The receiver
  deletes `enduser.*` and `user.*` attributes, and each harness adapter lists its own (Claude
  Code's account id, email and organisation id among them). An attribute the adapter does not
  know passes, so the list is kept by a test against each harness's recorded output (OB-3).
- **`charter.on_behalf_of` is never emitted on the device** (D-0083a, below).
- **Harness-specific attributes are an adapter's concern.** A `claude_code.*` metric is mapped to
  its GenAI name where one exists and kept under its own name where none does, as ADR 0073 asks of
  every per-harness feature.
- **Content is removed unless its gate is on** (§4).

### 4. Content gates are off by default, and only the human opens one, on this machine

Content means prompts, completions, tool arguments and tool results: the GenAI conventions'
`gen_ai.input.messages`, `gen_ai.output.messages`, tool call arguments and results, and each
harness's equivalent (`OTEL_LOG_USER_PROMPTS` and its kin for Claude Code).

- **Every gate is off by default.** A chat is started with its harness's content settings off,
  and the receiver drops content attributes whose gate is off **whatever the harness sent**. The
  gate is enforced twice, so a harness that ignores its setting still stores nothing.
- **Two gates:** `prompts` (what the user and the model said) and `tools` (tool arguments and
  results). They are separate because a team that wants tool timings with arguments may still not
  want prompts.
- **A gate is opened per project, in the Machine tier, and only from a human client scope.** The
  open gates live in `<config>/telemetry.json`, keyed by project, which chats are denied (*ADR
  0067, amended*). Only a `local-ui` caller can open one: the window, or `charter` on that scope.
  **No file in a project opens a gate**: not `charter.toml`, and not `charter.local.toml`, which
  is Clone state that a chat at the project root or without a sandbox can write.
- **A gate can be closed from more places than it can be opened.** `[telemetry] content` in
  `charter.toml` or `charter.local.toml` may only close gates, and so may an org's policy once an
  org project exists (C9, strictest wins). A file a chat can write can therefore only make
  telemetry hold less.
- **A gate is a project's.** A chat in project A with its gates open and a chat in project B with
  them closed are stored and exported differently.

### 5. The conventions are pinned

- **charter writes GenAI and MCP attributes by their semantic-convention names, pinned to one
  release** of `open-telemetry/semantic-conventions-genai`. OB-2 picks the newest tagged release
  when it lands and records it as a constant and as the resource's `schema_url`.
- **Moving the pin is a PR that maps old names to new**, for the store and for every export. A
  record stored under one pin is read through the map, never rewritten.
- **charter never invents a GenAI value.** ADR 0066's rule on `gen_ai.conversation.id` holds for
  every `gen_ai.*` attribute.

### 6. Attributes charter sets

ADR 0066's four `charter.*` attributes and `host.id` stand, with their places: `host.id` on the
resource, and `charter.chat.id`, `charter.run.id`, `charter.run.parent_id` and `charter.event.id`
on each record (§2). This record adds, on the resource, set by the receiver from the credential:

| Attribute | Value |
|---|---|
| `charter.project` | the project's name |
| `charter.workspace` | the workspace's name, absent at the project root |
| `charter.persona` | the persona's name, absent with none |
| `charter.branch` | the chat's branches, one per repo it works in (ADR 0072), as a string array; absent when it has none |
| `charter.harness` | the harness declaration's name (ADR 0073) |
| `charter.run.level` | the run's harness level, `1`, `2` or `3` |

- **These are W2's attribution, with two changes.** W2's *piece* is carried as `charter.branch`,
  the word ADR 0072 puts on screen, and as an array, because a chat works on one branch per repo.
  W2's `on_behalf_of` is not emitted (D-0083a).
- They are names a team gave its own things, not people. **They freeze when OB-18's dashboard
  pack ships**, since that is their first outside reader. Until then a rename is a change to this
  record.

### 7. The local store: rolling, capped, and the user's to change

- **Where:** `<data>/telemetry/`, charter's data home (ADR 0075), never in a project or a git work
  tree. The writer refuses such a path, as the audit's does. Chats are denied it (*ADR 0067,
  amended*).
- **Why `<data>`, when FD-8 kept its log out of it.** ADR 0069, as ADR 0075 amended it, gives
  `<data>` to data *"too large or too long-lived for the config home"*. A month of measurement for
  every chat on the device is both. FD-8's log is neither: a week of the app's own diagnostics,
  written by the app process, beside `panics.log`. O1 asks for separate stores, not separate
  directories: the telemetry store has its own directory, its own writer and its own retention,
  and no path between it and `<data>/audit/` (ADR 0075 §1). Its place in `<data>` does not put it
  in FR-10's backup either (*ADR 0069, amended*).
- **What:** records after §2 to §4, in rolling segments. OB-2 fixes the encoding, and an index
  for the views is rebuildable from the segments.
- **Retention.** **Settled by O8:** for the local store, *"the user decides"*, with *"Performance
  telemetry 14–30 days"*. **Defaults: 30 days and 1 GiB.** At either limit the oldest segment is
  deleted. Unlike the audit, nothing is kept past the cap, and nothing is recorded about a prune.
- **Collection can be turned off**, in `<config>/telemetry.json`, from a human client scope. Then
  no harness is given OTel settings, and the meters say *not reported*.

### 8. Export goes only where the user points it

**Settled by O5:** *"export to the user's own backend (Langfuse, Grafana, Datadog)"*.

- **Off by default.** No destination exists until the user adds one.
- **A destination is a machine setting**, in `<config>/telemetry.json`: a name, an OTLP/HTTP URL,
  the signals it takes, and an optional auth header. The header's value is a keyring item. It is
  added only from a human client scope. **A project can never name a destination**: a committed
  file must not be able to send a teammate's telemetry anywhere.
- **Each destination reads the store through a cursor of its own**, so an outage loses nothing
  that retention still holds, and one slow backend does not hold up another.
- **Every destination is listed in the network log** (OB-15), with its host and outcome, never a
  body or a header value.
- **In an org, the admin decides.** **Settled by O4:** *"In orgs, the admin decides where
  members' telemetry goes (the org's own dashboard)."* When an org project exists, its policy may
  add a destination for that project's chats, forbid the user's own for them, and close gates
  (C9). Before an account exists, there is no org, and only the user's settings apply.
- **The org fleet dashboard is TM-2's**, after GT-CLOUD. Nothing here waits on it.

### 9. What calls Charter, and nothing else does

**Settled by O4** (*"No phone-home offline, except the signed updater"*) **and X50** (*"no network
calls to Charter's servers without an account, except the signed updater; third-party calls that
features make are listed in a local, viewable network log"*). CF-1 writes the public wording.
For this pipeline that means:

- **Without an account, the only Charter-hosted reads are the signed updater's** (ADR 0042) **and
  the static signed files beside it**: the first-party catalogue and the revocation feed, as V18
  applied C-21, and the weekly manifest asset (V13, OB-17). They are reads of files. They carry
  no identifier, and each appears in the network log.
- **The OTel pipeline never sends to a Charter host by default**, and has no built-in destination.
  A Charter destination can exist only as an org's, after an account (§8).
- **Product telemetry (OB-12) is a separate, closed registry**, not a filter over this pipeline.
  Its events are written by charter's own code, each declared with its fields; nothing from a
  harness and no path or content can enter it. It is off until the user turns it on, shows its
  queue in a local viewer, and is listed in the network log when it sends. **Settled by O4 and
  O5:** *"Charter product telemetry is opt-in only."*
- **Crash reports (OB-11) stay as O9 settles them**, with their own consent and the same viewer.
- **The test is OB-15's:** *"a no-account run shows no Charter host except the updater and the
  static signed files beside it"*. It runs with collection on, so the pipeline is held to it.

### 10. What telemetry is trusted for

- **Telemetry is what a harness reports about itself**, through an exporter that runs in the
  harness and whose credential its own tool commands can read. charter treats it as a
  measurement: meters, timings and the views.
- **Time and tool-call budgets are counted from hooks**, as ADR 0076 has it: the host's own
  counts. Telemetry never moves them.
- **Spend is the one budget a harness reports.** For it, the adapter asks each harness for
  cumulative temporality where it offers one (OB-3), and the receiver keeps **the highest value
  seen per series**. A forged or replayed record can then only raise a chat's spend, never lower
  it. A new series adds to spend; it cannot subtract.
- **A chat whose spend stops reporting is shown as such.** When hooks show turns ending and no
  spend has arrived for that run, the meter says *spend not reported since <time>*, and the budget
  view says the same.
- **A spend budget may pause a chat on reported spend.** **Settled by V34c** (*Ruled*, below).
  The residual risk, an agent hiding its own spend, is stated wherever budgets are set.

## ADR 0066, amended

- **The OTel table** gains §6's six resource attributes. Its four `charter.*` rows keep their
  places, per record, and gain: **the receiver checks each against the credential's run tree
  (§2), sets them where a record carries none, and keeps `charter.event.id` only where it names an
  event of that chat.** `host.id` and the new resource attributes are **set from the credential,
  overwriting what a record carries.**
- **"`enduser.id` is not set"** now reads: **no person's identifier is set or kept. The receiver
  removes `enduser.*`, `user.*` and each adapter's own person attributes before anything is
  stored** (§3). `charter.on_behalf_of` is not emitted (D-0083a).

## ADR 0067, amended

- **§5 class 2, *"Charter's integrity state is denied to chats"***, gains: **the telemetry store
  (`<data>/telemetry/`) and its settings (`<config>/telemetry.json`)**. The store can hold another
  chat's content where a gate is open, and the settings say where telemetry goes and which gates
  are open. A chat that could read the first could read another chat's work, and a chat that
  could change the second could open its own gate or add a destination. The class also covers
  **the environments of processes outside the chat's own tree**, where another run's receiver
  credential is. SD-2 turns these into rules and tests.
- **§3, egress.** OTLP from every chat goes through the egress proxy to the receiver (D-0083b).
  The proxy's OTLP route forwards to the receiver's socket and nowhere else. No preset adds or
  removes it, `localhost` included, and nothing new is allowed in any profile.
- **§6, an external backend that wraps the whole harness**, reaches the receiver only through the
  proxy, as D-0083b says.
- **§4, a vendor's managed tier.** Where a harness's managed settings fix its OTel endpoint or
  its content settings, charter does not override them. That chat's meters say *locked by
  <vendor> admin*, and the receiver still enforces this record's gates on whatever reaches it.

## ADR 0068, amended

- **§5, the host's sockets.** The host gains **a telemetry ingest socket** in its `0700`
  directory, beside its own. It takes OTLP records and nothing else, answers no query, and accepts
  a record only under a live run's credential (§2). It is not a client scope.
- *What was rejected*, *"A loopback TCP port"*, stands: the receiver opens none. The only
  loopback listener on the path is ADR 0067's egress proxy.

## ADR 0069, amended

- **The inventory, *Every store, by tier*,** gains these rows, numbered from the first unused number
  after the highest on main.

  | # | Store | Tier | Sync | Backed up | Rebuildable |
  |---|---|---|---|---|---|
  | 77 | `<data>/telemetry/`, the telemetry store's segments and index (ADR 0083) | Machine | device-bound | no (the §2 exception below) | no |
  | 78 | `<data>/telemetry/cursors.json`, each export destination's place in the store (ADR 0083) | Machine, rebuildable | device-bound | no | yes |
  | 79 | `<config>/telemetry.json`, collection, retention and cap, each project's open content gates, and the export destinations (ADR 0083) | Machine | device-bound | yes | no |
  | 80 | an export destination's auth header (ADR 0083) | Keyring | — | no | no |

- **§2, what FR-10 backs up, gains a second exception**, beside the plain-file vault in Clone
  state: **the telemetry store is Machine and not rebuildable, and FR-10 does not copy it.**
  Losing it costs past charts and nothing charter or the user relies on, and gigabytes of charts
  restored to a new machine are worth less than the backup they would fill. The mark keeps its
  meaning; this is an exception to the rule, named as one.
- **Row 78 is rebuildable** because a lost cursor restarts its destination at the store's oldest
  record, and OTLP backends take a resend.
- **Row 79 is device-bound**, not syncable, because its destinations point into this machine's
  keyring and its gates are this machine's consent.
- **The receiver's per-run credential is held in memory only**, and is not a store.
- **`[telemetry] content` in `charter.toml` and `charter.local.toml`** is a key in files that
  already have their tiers. It may only close gates (§4).

## ADR 0075, amended

§1's table, column *Telemetry*:

- **Source** read *"harness exporters (OB-3), the host's counters (OB-19), and the event log
  (V25d)"*. It now reads: **harness exporters (OB-3), charter's own spans (OB-10), the host's
  counters (OB-19), and the event log (V25d)** (ADR 0083 §1).
- **Store** read *"the OTel pipeline's own store and the user's own backend (OB-1, OB-2)"*. It now
  reads: **`<data>/telemetry/` (ADR 0083 §7), and the destinations the user or an org's policy
  added (§8).**
- **Retention** read *"OB's, 14 to 30 days (O8)"*. It now reads: **30 days and 1 GiB by default,
  the user's to change (ADR 0083 §7).**
- **People in it** read *"never. `enduser.id` is not set (ADR 0066)"*. It now reads: **never. The
  receiver removes person attributes, and `charter.on_behalf_of` is not emitted (ADR 0083 §3,
  D-0083a).**

## ADR 0076, amended

- **§2's table, the `budget` row**, read *"the chat's budget ran out (N4, OV-7). The host's own
  act, from its own counts"*. It now reads: **the chat's budget ran out (N4, OV-7). The host's
  own act: from its own counts for time and tool calls, and for spend from the highest value the
  harness reported per series (ADR 0083 §10).**
- **The hooks-only rule, as ADR 0076 restated it** (*"Nothing parses harness output to decide
  anything"*), gains: **a budget pause may act on the spend a harness reports through its
  OTel exporter. That is a measurement the harness sends, not its output, and it is read only to
  pause.**

Ruled by V34c.

## What changes where

The code does not change with this record.

| Where | What changes |
|---|---|
| `docs/plane-format.md` | Rows 77 to 80 in *State charter keeps outside the plane*, marked **decided, not yet written** (in this PR) |
| `CONTEXT.md` | **Telemetry** names the receiver and the store; gains **Content gate** and **Export destination** (in this PR) |
| ADR 0066, ADR 0067, ADR 0068, ADR 0069, ADR 0075, ADR 0076 | Amended above. Their texts are left as accepted, and this record is the amendment |
| OB-2 | The receiver in `charterd`: OTLP/HTTP on a unix socket behind the egress proxy, per-run credentials and the checks of §2, the stripping of §3, the gates of §4, the pin of §5, the store of §7, the highest-value rule of §10. Its acceptance adds: a record naming another chat is attributed to its credential's chat; a child run's records keep its run; a content attribute with its gate off is not stored |
| OB-3 | Each adapter injects the proxy endpoint with the credential prefix, asks for cumulative temporality where offered, turns the harness's content settings off, defers to a vendor's managed tier, and lists its harness's person attributes, held by a test against recorded output |
| OB-9 | Destinations as §8 says: machine settings from a human scope, keyring headers, a cursor each |
| OB-10 | charter's spans are made from events and host timing, not from the diagnostic log |
| OB-12 | Product telemetry is a closed registry of its own (§9) |
| OB-15 | Its no-account test runs with collection on |
| OB-18 | The attributes of §6, without `charter.on_behalf_of` |
| OB-19 | Rows for receiver drops, unknown credentials, run-tree mismatches and queue depth |
| OV-6, OV-7 | Time and tool-call budgets from hooks; spend from §10, pausing on it (V34c), with the residual risk stated where budgets are set |
| SD-2 | The class 2 additions and the proxy's OTLP route, each with its test |
| CF-1 | Its wording holds the telemetry side of §9 |

## What this costs

- **Telemetry is only as true as the harness.** Codex's `exec` sends no metrics, opencode's flag
  is experimental, and a harness may change its names in any release. The adapters carry that
  cost, and the meters say *not reported* rather than guess.
- **Every chat's telemetry goes through the egress proxy.** The proxy is on the path of every
  harness's exporter, so it must be running whenever a chat is. A proxy that is down costs
  telemetry, never a chat.
- **Attribution is best-effort for an unsandboxed chat** (§2).
- **Stripping by list misses what the list does not know.** A harness that adds a new person
  attribute leaks it into the store until its adapter's list catches up. The recorded-output test
  is what notices.
- **Content capture is per machine and per human act.** A team that wants every member's prompts
  in its own backend must have each member open the gate, or have an org policy, and that is on
  purpose.
- **Charts are lost on a new machine.** The store is not backed up.
- **No per-human attribution on the device** (D-0083a). A team's backend sees projects,
  workspaces, personas and branches, not people, until org projects exist.

## What was rejected

- **Shipping an OTel Collector.** A second binary to sign and update, in another language, for
  what one receiver in `charterd` does (research 04 §2.1).
- **Pointing harnesses straight at the user's backend.** Then charter's own meters and views would
  have nothing, and every harness would need its own export settings and credentials.
- **A loopback port for the receiver**, or a new sandbox allowance for it. D-0083b routes through
  the proxy that already exists.
- **The credential in a header.** Claude Code's beta trace exporter ignores
  `OTEL_EXPORTER_OTLP_HEADERS` (research 04 §2.1).
- **Overwriting every `charter.*` attribute from the credential.** It would fold a child run's
  records into its parent and erase `charter.event.id`, the join key ADR 0066 gives the audit and
  telemetry.
- **Attributing a record by its own attributes alone.** A chat can set its own environment, so it
  could write records about another chat.
- **gRPC.** It excludes Langfuse, and OTLP/HTTP reaches every backend O5 names.
- **Content gates in a project file.** `charter.toml` would let a commit open a teammate's gate,
  and `charter.local.toml` is Clone state a chat can write.
- **A destination set by a project.** A commit could send a teammate's telemetry to any host.
- **Product telemetry as a filtered export of this pipeline.** A filter lets through whatever it
  forgot to exclude. A closed registry lets through only what it names.
- **Bridging the diagnostic log into OTel.** It is unstructured text with no schema, already
  redacted by shape, and written by another process. Its job is debugging charter, not measuring
  chats.
- **Backing up the store**, and **redefining *rebuildable*** to skip it. The first costs more than
  it saves. The second would blur a mark every other store relies on; an exception says what it
  is.
- **A single-valued `charter.piece`.** A chat works on one branch per repo, so one value would
  drop all but one.
- **Answering 429 under load.** A harness's exporter would buffer and retry inside the harness,
  growing its memory, which ADR 0082 counts as the scarce resource.

## Decided in drafting

1. **The receiver is in `charterd`, not the app or a sidecar.** The host already outlives the
   window and owns the event log. Rejected: the app (it is not always running while chats are),
   and a sidecar (a second process to supervise).
2. **A run is known by a credential in a path prefix of its endpoint**, under which the standard
   OTLP paths sit. Rejected: a header (above), one endpoint per chat on its own port (descriptors
   per chat, ADR 0082), and trusting `OTEL_RESOURCE_ATTRIBUTES`.
3. **Local collection is on by default; export, content and product telemetry are off.** Local
   collection never leaves the device, and the cost meters O5 promises need it. Rejected: off by
   default, which leaves every meter empty for a user who never finds the switch.
4. **Two content gates, `prompts` and `tools`, opened per project in the Machine tier from a
   human client scope only**, and closed by either project file or an org policy. Rejected: one
   gate (too coarse), and opening from a project file (above).
5. **Defaults of 30 days and 1 GiB**, inside O8's range at its top, because a month is the period
   a cost report covers. Rejected: 14 days, which cuts a month's report in half.
6. **The store is not backed up, as a named exception to ADR 0069 §2**, following the plain-file
   vault's. Rejected: backing it up (cost above), redefining *rebuildable* (above), and a fifth
   mark for one store.
7. **Export destinations are machine settings with keyring headers**, read through a cursor
   each. Rejected: a project setting (a commit would redirect telemetry), and an in-memory
   exporter queue (an outage would lose data the store still has).
8. **A user's existing OTel settings.** A vendor's managed tier wins, as ADR 0067 §4 says:
   charter never overrides an endpoint or a content setting the vendor's admin fixed, and the
   meters say *locked by <vendor> admin*. A user's own `OTEL_*` settings are replaced for chats
   charter starts, and the window offers the earlier endpoint once as an export destination.
   Rejected: leaving the user's settings (charter's meters would be empty), and adopting them
   silently (an export the user did not choose in charter).
9. **Attribute names under `charter.*`, freezing at OB-18.** `charter.branch` is a string array.
   Rejected: freezing now, before any outside reader exists, and `charter.piece` (above).
10. **Each device keeps its own pipeline.** A runner's chats report to the runner's host, and its
    store and destinations are its own. How the desktop's views read a runner's meters over the
    link is RR's to decide (ADR 0078).
11. **Spend keeps the highest value per series, and a silent spend meter says so** (§10).
    Rejected: summing deltas (a forged negative or a replay could lower or double spend), and
    hiding a meter that stopped (a suppressed exporter would look like a cheap chat).

## Decided (dispatcher, 2026-10-01)

- **D-0083a:** *"charter does not emit `charter.on_behalf_of` on the device, which keeps V1 and
  V25 intact. An org member id may be added under org policy once org projects exist, after
  GT-CLOUD. The keyed-pseudonym scheme is dropped."* Applied in §3, §6 and the amendments of ADR
  0066 and ADR 0075.
- **D-0083b:** *"OTLP from a sandboxed chat goes through charter's existing loopback egress proxy
  (ADR 0067 §3). There is no new sandbox allowance. A whole-process sandbox (ADR 0067 §6) states
  that it reaches the receiver only through the proxy."* Applied in §2 and the amendments of ADR
  0067 and ADR 0068, and extended to unsandboxed chats so there is one path.

## Ruled (V34c, 2026-10-01)

The one item put to the operator: *may a budget pause be driven by the spend a harness reports?*
Answered yes, as recommended.

- **V34c:** *"A spend budget may pause a chat on harness-reported spend. The receiver keeps the
  highest value it has seen per series, and "spend not reported since <time>" is shown, never
  acted on. This amends ADR 0076/V27's "own counts". The residual risk, an agent hiding its own
  spend, is stated wherever budgets are set."*
- Applied in §10 and *ADR 0076, amended*, which covers both the budget row and the hooks-only
  rule's restatement. Time and tool-call budgets stay counted from hooks.

## Later decisions

- **The encoding of the store and its index** (OB-2).
- **Sampling rules** beyond dropping under pressure, if a measured load needs them (OB-19, SC-17).
- **Windows**, when it is ported: the receiver's socket and the proxy's route are decided with
  Windows' host transport.
