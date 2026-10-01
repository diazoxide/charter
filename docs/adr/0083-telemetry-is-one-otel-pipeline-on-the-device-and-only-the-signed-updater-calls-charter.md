# Telemetry is one OTel pipeline on the device, and only the signed updater calls Charter

**Proposed 2026-10-01**, drafted for program-map ticket OB-1 (#686). It waits for the operator's
ruling on the three items under *For the operator's ruling*. It follows these of the operator's
rulings:

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
- **V18**, which applied the map audit's fix C-21 to CF-1's row: *"The static signed files beside
  the updater (the first-party catalogue PE-44 and the revocation feed PE-8) are the only other
  Charter-hosted reads"*.
- **V13**, in part: *"the first update check of each ISO week fetches an identical alternate
  manifest asset whose download count estimates weekly users with no identifier"*.
- **V25d:** *"Telemetry may derive its OTel logs from the event log, and never reads the audit
  store."*
- **W2**, in part: *"charter emits persona, workspace, piece and on_behalf_of as OTel resource
  attributes and ships a dashboard pack for Datadog/Grafana"*.
- **W13**, in short: the cloud waits for the GT-CLOUD gate, and *"OTel attribution with a
  Datadog/Grafana dashboard pack (W2)"* is built now, on the device. Nothing in this record
  waits on GT-CLOUD.

It builds on [ADR 0042](0042-charter-updates-itself-and-nothing-it-cannot-verify-reaches-it.md)
(the signed updater), [ADR 0066](0066-a-chat-is-a-ulid-a-run-is-a-stretch-of-its-conversation-and-a-device-is-random.md)
(ids and the OTel attributes), [ADR 0067](0067-a-chat-runs-in-a-sandbox-charter-compiles-for-its-harness.md)
(the chat sandbox), [ADR 0068](0068-a-chat-lives-in-charterd-and-the-app-is-its-client.md)
(`charterd` and the event log), [ADR 0069](0069-every-store-charter-writes-is-in-one-of-four-tiers.md)
(tiers), [ADR 0073](0073-a-harness-is-declared-as-data-and-a-chat-runs-it-at-one-of-three-levels.md)
(harness adapters) and [ADR 0075](0075-an-audit-entry-is-metadata-in-a-store-of-its-own-and-telemetry-never-reads-it.md)
(audit and telemetry apart). It sits beside FD-8 (#647, PR #794), whose diagnostic log is neither
audit nor telemetry. It **amends** ADR 0066, ADR 0067, ADR 0068, ADR 0069 and ADR 0075, each in a
section of its own below. OB-2, OB-3, OB-4, OB-5, OB-9, OB-10, OB-12, OB-14, OB-15, OB-18 and
OB-19 build on it.

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
- **FD-8 (PR #794) adds a diagnostic log**, `<app log>/charter.<date>.log`, redacted, kept seven
  days. Its PR says *"a diagnostic log is neither audit nor telemetry"*.
- **The harnesses differ** (research 04 §2.1). Claude Code exports eight metrics, twenty-odd log
  events and beta traces, keeps prompt and tool content out unless `OTEL_LOG_*` gates are set, and
  attaches the signed-in user's account and email to its records. Codex exports when an `[otel]`
  section says so, and `codex exec` sends no metrics. opencode exports spans behind an
  experimental flag. The GenAI semantic conventions are still marked *Development*.

## The decision

### 1. One pipeline: one receiver, one store, one exporter, on each device

**Settled by O5:** *"One OTel pipeline."* On each device it is:

```
harness exporters (OB-3) ─┐
charter's own spans (OB-10)┼─► receiver in charterd ─► strip and gate (§3, §4) ─► local store (§7) ─┬─► charter's views (OB-4..OB-7, OB-14)
host counters (OB-19) ─────┤                                                                        └─► export to the user's own backends (§8)
the event log (V25d) ──────┘
```

- **The receiver is in `charterd`**, the one host per OS user per device (ADR 0068). The window
  reads telemetry from its host like everything else. There is no Collector binary and no second
  process.
- **Four sources, and only four.** The harnesses' own exporters, charter's own spans, the host's
  counters, and OTel logs derived from the event log (V25d). A new source is a change to this
  record.
- **Two things are never a source.** **The audit store**, settled by V25d and O1. **The
  diagnostic log** (FD-8): it is unstructured text, redacted by shape, and written by the app process. No
  `tracing` bridge feeds it to OTel. charter's own spans (OB-10) are made from events and the
  host's own timing, not from log lines.
- **Product telemetry and crash reports are not an export of this pipeline** (§9).

### 2. The receiver: OTLP/HTTP on loopback, and a chat is known by its credential

- **OTLP over HTTP, protobuf and JSON**, at the standard `/v1/traces`, `/v1/metrics` and
  `/v1/logs` paths. No gRPC: every harness can send HTTP, and OB-9's export is HTTP too. Types come
  from `opentelemetry-proto` (OB-2).
- **It listens on loopback, on a port chosen when the host starts.** OTLP exporters cannot reach
  a unix socket, which is why this listener differs from the host's own socket (*ADR 0068,
  amended*).
- **Each run is given its own receiver credential at spawn**, inside the endpoint URL the
  adapter injects (OB-3), so that it reaches every signal's exporter, including those that ignore
  extra headers. It is minted fresh for each run, and dies with the run.
- **The credential says which chat and run a record is about, never the record.** The receiver
  overwrites every `charter.*` resource attribute and `host.id` from the credential. A record
  with no credential, or a dead one, is dropped and counted. One chat cannot write records about
  another. This follows ADR 0066's rule that an actor comes from the channel, never the payload.
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
  `charter.on_behalf_of` waits for the operator's ruling (item 1 below).
- **Harness-specific attributes are an adapter's concern.** A `claude_code.*` metric is mapped to
  its GenAI name where one exists and kept under its own name where none does, as ADR 0073 asks of
  every per-harness feature.
- **Content is removed unless its gate is on** (§4).

### 4. Content gates are off by default, and a project can only close them

Content means prompts, completions, tool arguments and tool results: the GenAI conventions'
`gen_ai.input.messages`, `gen_ai.output.messages`, tool call arguments and results, and each
harness's equivalent (`OTEL_LOG_USER_PROMPTS` and its kin for Claude Code).

- **Every gate is off by default.** A chat is started with its harness's content settings off,
  and the receiver drops content attributes whose gate is off **whatever the harness sent**. The
  gate is enforced twice, so a harness that ignores its setting still stores nothing.
- **Two gates:** `prompts` (what the user and the model said) and `tools` (tool arguments and
  results). They are separate because a team that wants tool timings with arguments may still not
  want prompts.
- **Only this machine opens a gate.** `[telemetry] content` is read from `charter.local.toml`
  (Local), never from `charter.toml`. A Shared value may only **close** gates: a committed file
  cannot turn on content capture on a teammate's machine.
- **An org's policy may close gates and never open them**, once an org project exists (C9,
  strictest wins).
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

ADR 0066's four `charter.*` attributes and `host.id` stand. This record adds, on the resource,
set by the receiver from the credential:

| Attribute | Value |
|---|---|
| `charter.project` | the project's name |
| `charter.workspace` | the workspace's name, absent at the project root |
| `charter.persona` | the persona's name, absent with none |
| `charter.piece` | the piece's name, where the chat is in one |
| `charter.harness` | the harness declaration's name (ADR 0073) |
| `charter.run.level` | the run's harness level, `1`, `2` or `3` |

These are W2's attribution, apart from `on_behalf_of`. They are names a team gave its own things,
not people. **They freeze when OB-18's dashboard pack ships**, since that is their first outside
reader. Until then a rename is a change to this record.

### 7. The local store: rolling, capped, and the user's to change

- **Where:** `<data>/telemetry/` (ADR 0075's data home), never in a project or a git work tree.
  The writer refuses such a path, as the audit's does.
- **What:** records after §3 and §4, in rolling segments. OB-2 fixes the encoding, and an index
  for the views is rebuildable from the segments.
- **Retention.** **Settled by O8:** for the local store, *"the user decides"*, with *"Performance
  telemetry 14–30 days"*. **Defaults: 30 days and 1 GiB.** At either limit the oldest segment is
  deleted. Unlike the audit, nothing is kept past the cap, and nothing is recorded about a prune.
- **Not backed up.** Losing the store costs past charts and nothing else, so it is
  **rebuildable** in ADR 0069's sense: FR-10 skips it (*ADR 0069, amended*).
- **Collection can be turned off** (`[telemetry] collect = false`, Local). Then no harness is
  given OTel settings, and the meters say *not reported*.

### 8. Export goes only where the user points it

**Settled by O5:** *"export to the user's own backend (Langfuse, Grafana, Datadog)"*.

- **Off by default.** No destination exists until the user adds one.
- **A destination is a machine setting**: a name, an OTLP/HTTP URL, the signals it takes, and an
  optional auth header. The header's value is a keyring item. **A project can never name a
  destination**, Shared or Local: a committed file must not be able to send a teammate's
  telemetry anywhere.
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
  the static signed files beside it**: the weekly manifest asset (V13, OB-17), the first-party
  catalogue (PE-44) and the revocation feed (PE-8), as V18 settled. They are reads of files. They
  carry no identifier, and each appears in the network log.
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

Telemetry is what a harness reports about itself, through an exporter that runs in the harness
and whose credential its tool commands can read. charter treats it as a measurement: meters,
timings and the views. Whether a budget may act on it is item 2 below.

## ADR 0066, amended

- **The OTel table** gains §6's six resource attributes, and a line: **the receiver sets every
  `charter.*` attribute and `host.id` from the run's credential, and overwrites what a record
  carries.**
- **"`enduser.id` is not set"** now reads: **no person's identifier is set or kept. The receiver
  removes `enduser.*`, `user.*` and each adapter's own person attributes before anything is
  stored** (§3). Whether `charter.on_behalf_of` is one of them is item 1 below.

## ADR 0067, amended

§3's egress presets gain a fixed allowance, outside every preset: **a chat may reach its host's
telemetry receiver on loopback, at the one port the host chose, and nothing else on loopback
through it.** It applies only where the sandbox confines the harness process itself. Where the
sandbox confines the harness's tool commands only, the exporter already runs outside it. This
opening is item 3 below. Until it is ruled, a whole-process sandbox keeps loopback closed, and
that chat's meters say *not reported*.

## ADR 0068, amended

*What was rejected* says *"A loopback TCP port. Any local user can connect to one, and a peer uid
is not available on it."* That stands for the session protocol and every client scope. It gains:
**the telemetry receiver is a loopback port of its own, because OTLP exporters cannot reach a
unix socket. It takes OTLP records and nothing else, answers no query, and accepts a record only
under a live run's credential** (§2). A process that reaches the port without one can make the
host count a drop, and nothing more.

## ADR 0069, amended

The inventory, *Every store, by tier*, gains these rows. Their numbers follow ADR 0078's 73 to 76
and are taken in merge order.

| # | Store | Tier | Sync | Backed up | Rebuildable |
|---|---|---|---|---|---|
| 77 | `<data>/telemetry/`, the telemetry store's segments and index (ADR 0083) | Machine, rebuildable | device-bound | no | yes |
| 78 | `<data>/telemetry/cursors.json`, each export destination's place in the store (ADR 0083) | Machine, rebuildable | device-bound | no | yes |
| 79 | `<config>/telemetry.json`, collection on or off, retention, cap and the export destinations (ADR 0083) | Machine | device-bound | yes | no |
| 80 | an export destination's auth header (ADR 0083) | Keyring | — | no | no |

- **§2, the *rebuildable* mark,** read *"derived from something else. Deleting it costs a rebuild
  and nothing more."* It gains: **or a measurement charter keeps only to draw past charts, whose
  loss costs those charts and nothing charter or the user relies on.** Row 77 is the first such
  store. It is not derived from another store, and this record says so rather than adding a fifth
  mark for one store.
- **Row 78 is rebuildable** because a lost cursor restarts its destination at the store's oldest
  record, and OTLP backends take a resend.
- **Row 79 is device-bound**, not syncable, because its destinations point into this machine's
  keyring.
- **The receiver's per-run credential is held in memory only**, and is not a store.
- **The content gates are keys in `charter.toml` and `charter.local.toml`**, which already have
  their tiers.

## ADR 0075, amended

§1's table, column *Telemetry*:

- **Store** read *"the OTel pipeline's own store and the user's own backend (OB-1, OB-2)"*. It now
  reads: **`<data>/telemetry/` (ADR 0083 §7), and the destinations the user or an org's policy
  added (§8).**
- **Retention** read *"OB's, 14 to 30 days (O8)"*. It now reads: **30 days and 1 GiB by default,
  the user's to change (ADR 0083 §7).**
- **People in it** read *"never. `enduser.id` is not set (ADR 0066)"*. It now reads: **never. The
  receiver removes person attributes (ADR 0083 §3)**, pending item 1.

## What changes where

The code does not change with this record.

| Where | What changes |
|---|---|
| `docs/plane-format.md` | Rows 77 to 80 in *State charter keeps outside the plane*, marked **decided, not yet written** (in this PR) |
| `CONTEXT.md` | **Telemetry** names the receiver and the store; gains **Content gate** and **Export destination** (in this PR) |
| OB-2 | The receiver in `charterd`: OTLP/HTTP on loopback, per-run credentials, the stripping of §3, the gates of §4, the pin of §5, the store of §7. Its acceptance adds: a record under another chat's credential is attributed to its own; a content attribute with its gate off is not stored |
| OB-3 | Each adapter injects the endpoint with the credential, turns the harness's content settings off unless the gate is open, and lists its harness's person attributes, held by a test against recorded output |
| OB-9 | Destinations as §8 says: machine settings, keyring headers, a cursor each |
| OB-10 | charter's spans are made from events and host timing, not from the diagnostic log |
| OB-12 | Product telemetry is a closed registry of its own (§9) |
| OB-15 | Its no-account test runs with collection on |
| OB-18 | The attributes of §6; `charter.on_behalf_of` as item 1 is ruled |
| OB-19 | Rows for receiver drops, unknown credentials and queue depth |
| SD-2 | The loopback allowance of *ADR 0067, amended*, once item 3 is ruled |
| CF-1 | Its wording holds the telemetry side of §9 |

## What this costs

- **Telemetry is only as true as the harness.** Codex's `exec` sends no metrics, opencode's flag
  is experimental, and a harness may change its names in any release. The adapters carry that
  cost, and the meters say *not reported* rather than guess.
- **A loopback port.** Any local process can reach it. The credential limits what it can do to
  making the host count a drop, but the port is there.
- **Stripping by list misses what the list does not know.** A harness that adds a new person
  attribute leaks it into the store until its adapter's list catches up. The recorded-output test
  is what notices.
- **Content capture is per machine.** A team that wants every member's prompts in its own
  backend must have each member open the gate, or have an org policy, and that is on purpose.
- **Charts are lost on a new machine.** The store is not backed up.

## What was rejected

- **Shipping an OTel Collector.** A second binary to sign and update, in another language, for
  what one receiver in `charterd` does (research 04 §2.1).
- **Pointing harnesses straight at the user's backend.** Then charter's own meters and views would
  have nothing, and every harness would need its own export settings and credentials.
- **Attributing a record by its own `charter.*` attributes.** A chat can set its own environment,
  so it could write records about another chat.
- **gRPC.** It excludes Langfuse, and OTLP/HTTP reaches every backend O5 names.
- **Content gates in `charter.toml`.** A commit could turn on prompt capture on every teammate's
  machine.
- **A destination set by a project.** A commit could send a teammate's telemetry to any host.
- **Product telemetry as a filtered export of this pipeline.** A filter lets through whatever it
  forgot to exclude. A closed registry lets through only what it names.
- **Bridging the diagnostic log into OTel.** It is unstructured text with no schema, already redacted by
  shape, and written by another process. Its job is debugging charter, not measuring chats.
- **Backing up the store.** Gigabytes of charts restored to a new machine are worth less than the
  backup they would fill.
- **Answering 429 under load.** A harness's exporter would buffer and retry inside the harness,
  growing its memory, which ADR 0082 counts as the scarce resource.

## Decided in drafting

1. **The receiver is in `charterd`, not the app or a sidecar.** The host already outlives the
   window and owns the event log. Rejected: the app (it is not always running while chats are),
   and a sidecar (a second process to supervise).
2. **A run is known by a credential in its endpoint URL.** In the URL rather than a header,
   because Claude Code's beta traces exporter does not apply `OTEL_EXPORTER_OTLP_HEADERS`
   (research 04 §2.1). Rejected: headers (above), one port per chat (descriptors per chat, ADR
   0082), and trusting `OTEL_RESOURCE_ATTRIBUTES`.
3. **Local collection is on by default; export, content and product telemetry are off.** Local
   collection never leaves the device, and the cost meters O5 promises need it. Rejected: off by
   default, which leaves every meter empty for a user who never finds the switch.
4. **Two content gates, `prompts` and `tools`, opened only by a Local setting**, and closed by a
   Shared one or an org policy. Rejected: one gate (too coarse), and Shared opening (a commit
   would open a teammate's).
5. **Defaults of 30 days and 1 GiB**, inside O8's range at its top, because a month is the period
   a cost report covers. Rejected: 14 days, which cuts a month's report in half.
6. **The store is marked rebuildable and is not backed up.** Rejected: backing it up (cost above),
   and a fifth mark (an amendment of ADR 0069's vocabulary for one store).
7. **Export destinations are machine settings with keyring headers**, read through a cursor
   each. Rejected: a project setting (a commit would redirect telemetry), and an in-memory
   exporter queue (an outage would lose data the store still has).
8. **A user's existing `OTEL_*` settings.** For chats charter starts, charter's endpoint replaces
   them, and the window offers the earlier endpoint once as an export destination. Rejected:
   leaving them (charter's meters would be empty), and adopting them silently (an export the user
   did not choose in charter).
9. **Attribute names under `charter.*`, freezing at OB-18.** Rejected: freezing now, before any
   outside reader exists.
10. **Each device keeps its own pipeline.** A runner's chats report to the runner's host, and its
    store and destinations are its own. How the desktop's views read a runner's meters over the
    link is RR's to decide (ADR 0078).

## For the operator's ruling

1. **`charter.on_behalf_of` (W2) against ADR 0066 and ADR 0075.** W2 says charter *"emits
   persona, workspace, piece and on_behalf_of as OTel resource attributes"*. ADR 0066 says *"The
   local principal is never an OTel attribute"*, and ADR 0075, accepted under V25, says telemetry
   *"never names a person"*. Both cannot hold. **Recommended:** emit `charter.on_behalf_of` only
   when the project turns attribution on (off by default), and then as a keyed pseudonym of the
   human, under a key the project's members share, so a team can map it to names in its own
   backend and charter never exports a login name. On an org project, the org's member id, under
   the org's policy. This amends either W2 or ADRs 0066 and 0075, so it is the operator's.
2. **Budgets that read telemetry.** OB-4's meters feed OV-6's budgets, which pause a chat. The
   numbers come from the harness, and an agent's tool commands can reach its exporter's
   credential, so an agent could report less than it spent. **Recommended:** a budget is a guard
   against runaway cost, not against an agent that lies, and charter says so where budgets are
   set. A budget that must hold against a hostile agent waits for spend charter measures itself
   (a model gateway, MS). This sets a security boundary, so it is the operator's.
3. **The loopback opening in a whole-process sandbox** (*ADR 0067, amended*). It lets a confined
   chat reach one port on loopback that it cannot reach today. **Recommended:** allow it, with
   the receiver accepting only OTLP under a live credential and answering nothing. Rejected
   alternative: a file spool the chat writes, which would need a second exporter in every
   harness. It changes a sandbox boundary, so it is the operator's.

## Later decisions

- **The encoding of the store and its index** (OB-2).
- **Sampling rules** beyond dropping under pressure, if a measured load needs them (OB-19, SC-17).
- **Windows**, when it is ported: the receiver's port is the same, and its sandbox allowance is
  decided with Windows' sandbox.
