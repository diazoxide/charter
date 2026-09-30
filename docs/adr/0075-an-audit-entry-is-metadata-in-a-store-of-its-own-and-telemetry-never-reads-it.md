# An audit entry is metadata in a store of its own, and telemetry never reads it

**Accepted 2026-10-01** by the operator (ruling V25), drafted for program-map ticket AU-1 (#689). It follows these of the
operator's rulings:

- **O1:** *"Performance telemetry and the audit log are strictly separate systems, with separate
  stores, schemas and retention. The audit log is never sampled."*
- **O6:** *"Audit schema: charter's own format (actor, on_behalf_of, action, target, org, device,
  outcome; metadata only). Exported as OCSF, enveloped in CloudEvents, streamed through Vector."*
- **C4:** *"Every audit entry carries both actor and on_behalf_of."*
- **X19**, accepted with the consistency review as recommended: *"Do audit entries store actors
  as per-user keyed pseudonyms (deleting the key on erasure, with the chain intact) …?"*
- **X47**, accepted the same way: *"Does the local audit log live only in the app data dir, with
  `.charter/` reserved for derived, rebuildable indexes (Q12)?"*
- **W8:** *"Audit: O3 reads "every tool call a level-2 or level-3 harness reports"; a coverage
  label per harness level; org key `minimumHarnessLevel`; the chat sandbox denies the audit dir
  and the device-key keyring item (charterd is the only writer); a sandbox opt-out is logged as
  `trust.sandbox.off`"*.
- **W9**, in short: the local audit export needs no account (an OCSF export command, plus a
  continuous file or OTLP-logs sink), and an entry's tool arguments are a digest, or encrypted
  to an org's key on an org project.
- **V1** and **V21**, through [ADR 0066](0066-a-chat-is-a-ulid-a-run-is-a-stretch-of-its-conversation-and-a-device-is-random.md):
  the chat, run and device ids, the local principal and the event envelope.
- **W13**, in short: the org audit log is cloud work and waits for the GT-CLOUD gate. Nothing in
  this record waits on it.

It also takes the storage rules of AU-1's own row in the program map, which came from the
phase-2 critique's fix for P3-24: *"rolling compressed segments, a default retention and disk
cap, group commit (≤ 100 ms and at turn end), 1,000 events/s without moving hook p95"*. No
ruling quotes those numbers. They are the filed spec.

It builds on ADR 0066, [ADR 0067](0067-a-chat-runs-in-a-sandbox-charter-compiles-for-its-harness.md)
(the chat sandbox), [ADR 0068](0068-a-chat-lives-in-charterd-and-the-app-is-its-client.md)
(`charterd` and the hook spool), [ADR 0069](0069-every-store-charter-writes-is-in-one-of-four-tiers.md)
(storage tiers) and [ADR 0073](0073-a-harness-is-declared-as-data-and-a-chat-runs-it-at-one-of-three-levels.md)
(harness levels). It amends ADR 0066, ADR 0067, ADR 0069,
[ADR 0070](0070-a-forge-is-one-seam-with-a-native-client-per-forge-and-gh-and-glab-are-its-fallback.md)
and [ADR 0071](0071-the-kill-switch-is-machine-state-the-command-line-stops-and-only-the-window-re-arms.md),
each in a section of its own below. AU-2 (the `charter-audit` crate), AU-3 (the device-signed
chain), AU-4, AU-5, AU-7, AU-18, AU-19, AU-20 and AU-22 build on it, and so does the telemetry
side (OB-1 to OB-3).

Its concept is **Project**. The audit is a part of the project, like the device and the machine
store ([ADR 0072](0072-charter-has-five-concepts-and-every-other-word-belongs-to-one-of-them.md) §2).

## Where charter is today

charter writes no audit and no telemetry. There is no audit store, no OTel exporter and no
receiver. But five accepted records already promise audit entries:

- **ADR 0067** records `trust.sandbox.off` and `trust.sandbox.on`, and each unlisted-egress
  exception, with who, when, which chat, harness, persona and machine, and the reason if one was
  typed.
- **ADR 0068** drains each chat's hook spool and *"seals it into the audit chain (AU-3, when that
  exists) with the result of the check"*.
- **ADR 0070** audits every forge write from its `Caller`: the account, the human, the chat and
  the surface.
- **ADR 0071** keeps `kill-switch.jsonl`, a journal of stops, re-arms and tampering. It is the
  switch's own record, not an audit.
- **ADR 0073** labels each run's audit coverage by its harness level, and gives the audit the
  `fallback` cause.

Two of them, 0067 and 0070, send their entries to *"charter's machine-state log (ADR 0034)"*
until the chain exists. ADR 0034 defines no such log, and no code writes one. ADR 0066 fixed the
ids and left *"how these are stored, retained and exported"* to this record.

Three more facts shape it:

- **Every tool call would be written three times** if the event log, the audit and telemetry each
  kept their own copy. The phase-2 critique estimated about 650 MB per 8-hour day at 50 busy
  chats (P3-24, an estimate). No record says when an entry is flushed, how long it is kept, or
  what happens when the disk fills.
- **The word "audit" is taken once already.** `charter secret audit` is a report on vault health.
  It is not this, and it keeps its name.
- **The local principal holds a login name** (`local:<device>/<os-user>`, ADR 0066). That is
  personal data, and a hash chain cannot be edited to remove it later.

## The decision

### 1. Audit and telemetry are two systems, and both start from the event log

**Settled by O1.** They differ in every column:

| | Audit | Telemetry |
|---|---|---|
| What it answers | who did what, for whom, to what, and was it allowed | how fast, how much, how often, at what cost |
| Source | one event in the host's event log per entry | harness exporters (OB-3), the host's counters (OB-19), and the event log (V25d) |
| Schema | this record's entry and its action registry (§4) | OpenTelemetry's conventions, with ADR 0066's attributes |
| Sampling | **never**. Every registered event is written | may sample, aggregate, and refuse under pressure (P3-25) |
| Store | `<data>/audit/` (§6) | the OTel pipeline's own store and the user's own backend (OB-1, OB-2) |
| Retention | §8 | OB's, 14 to 30 days (O8) |
| People in it | only as keyed pseudonyms (§3) | never. `enduser.id` is not set (ADR 0066) |
| When it fails | an alert, and the gap is recorded (§7) | dropped quietly, and counted |

**Rules that keep the two apart:**

- **Every audit entry is written from exactly one event, and cites it.** The host is the one
  writer of both (ADR 0066, ADR 0068). Because the two have one origin, a crash between them can
  be found and repaired (§7).
- **Telemetry never reads the audit store, and the audit never reads telemetry.** A cost or a
  latency that an audit entry needs comes from the event, never from a metric. Telemetry may
  derive its OTel logs from the event log instead of writing a third copy (V25d).
- **No code path writes both.** The `charter-audit` crate (AU-2) has no OpenTelemetry
  dependency, and the OTel exporter has no path into `<data>/audit/`.
- **They join only on the event id.** Both carry the event's `ulid` (`charter.event.id` on the
  OTel side, `event` in an entry). A join happens in the user's own backend, after the user
  exports both. charter never ships one to the other.
- **Product telemetry (O4) and crash reports (O9) are telemetry.** They stay opt-in, with their
  local viewer. Nothing here changes them.

Where one fact has an act and a number, the act is audit and the number is telemetry:

| Fact | Audit | Telemetry |
|---|---|---|
| A tool call and the guard's verdict | `tool.call`, allowed or denied | its count and latency |
| A turn's tokens and cost | nothing | tokens, cost, model |
| A budget pausing a chat (N4) | `budget.paused` | the spend that crossed it |
| A sandbox denial | `sandbox.denied` | the denial rate (SD-2's outcome bar) |
| An extension (E9) | capability use, and a crash | CPU and memory. This narrows E9 (V25d) |
| `charterd` restarting | `host.started`, with the reconcile result | uptime, restart count |

### 2. One entry: who, for whom, did what, to what, with what result

**Settled by O6 and C4:** the entry holds actor, on_behalf_of, action, target, org, device and
outcome, and metadata only. This record fixes their form. Each entry is one JSON line:

```json
{"v": 1, "device": "<ULID>", "seq": 812, "event": "<ULID>",
 "action": "trust.sandbox.off", "actor_kind": "human",
 "actor": "ps1:…", "on_behalf_of": "ps1:…", "org": null,
 "target": [{"type": "chat", "id": "<ULID>"}],
 "outcome": "succeeded", "coverage": null,
 "meta": {"classes": ["integrity", "vaults"], "note": "debugging the build cache"},
 "chain": {"prev": "sha256:…", "sig": "ed25519:…"}}
```

| Field | Meaning |
|---|---|
| `v` | the entry format's version. The meaning of `meta` is versioned per action in the registry (§4) |
| `device` | the device id of the host that wrote the entry (ADR 0066) |
| `seq` | the entry's place in this device's chain: a `u64` from 1, one higher each time, never reused. It is the chain's own number, not the event's `seq` |
| `event` | the `ulid` of the event the entry was written from. **Its time part is the entry's time**, as ADR 0066 does for events, so there is no timestamp field |
| `action` | what happened, from the registry (§4) |
| `actor_kind` | `agent`, `human`, `host` or `unattributed` (below) |
| `actor` | who acted, as a pseudonym (§3) |
| `on_behalf_of` | whose authority the act used, as a pseudonym (§3). Always set |
| `org` | the org the act was under. `null` until an account and an org exist (§9) |
| `target` | what was acted on: a list of `{type, id}`. Chats and runs by their ULIDs, work items by the tracker's key (FW-5), a repo by its inventory name, a vault and a secret by name. **Never a chat's number** (ADR 0066) |
| `outcome` | `allowed` or `denied` for a decision; `succeeded` or `failed` for an act; `recorded` for a fact with no result, such as a run starting |
| `coverage` | for an agent's entry, what charter could see of that run (§5). `null` otherwise |
| `meta` | the action's typed metadata (§4) |
| `chain` | the hash of the previous entry and the device's signature. AU-2 fixes the algorithms. The hash covers the entry in its RFC 8785 canonical form, without `chain.sig` |

**The actor kind comes from the channel, never from the payload.** ADR 0066 settled it: *"A line
on the hook channel produces an agent actor, whatever it claims. The local principal acts only
through the operator's own client scope."* This record adds two actor forms to ADR 0066's two
(see ADR 0066, amended):

| `actor_kind` | Who | The actor, before it is pseudonymised |
|---|---|---|
| `agent` | a run, reporting through the hook channel, its protocol (level 3) or the host acting for it | `agent:<persona>/<run>` (ADR 0066) |
| `human` | the operator, through a human client scope: the window, or `charter` on a human scope (ADR 0068) | the local principal, or the account's principal after `account.link` |
| `host` | `charterd` itself: a drain, a reconcile, a gap, a prune, a key rotation | `host:<device>` |
| `unattributed` | a fact the host read from a file any process of the user can write, such as the kill switch's `cli` lines (ADR 0071, amended below) | `unattributed:<device>` |

**`on_behalf_of` is always set.** For an agent it is the human the chat runs for. For `host` and
`unattributed` it is the device's owner: the local principal, or the account's principal once one
is linked. After `account.link` new entries name the account, and no earlier entry is rewritten
(ADR 0066).

### 3. A person is never written in plain

**Settled by X19:** actors are per-user keyed pseudonyms, erasure deletes the key, and the chain
stays intact.

- **`actor` and `on_behalf_of` are written as pseudonyms, `ps1:<…>`, keyed by the human the entry
  is on behalf of.** An agent's actor is pseudonymised under its human's key, as the AU-1 row
  says (*"actor = the run id (a keyed pseudonym, AU-18)"*). A `host` or `unattributed` actor is
  written the same way, under the device owner's key, so every entry has one shape.
- **The key is in the Keyring tier**, one per human principal on this device. AU-18 fixes the
  algorithm, the key's item name and how the viewer shows names while the key exists.
- **Erasure deletes the key.** The chain hashes the pseudonym, never the name, so
  `charter audit verify` still passes and the entries no longer say who.
- **What stays plain:** the device id, chat, run and work-item ids in `target`, and names in
  `meta`. They are random ids or project names. What links a device or a chat to a person lives in
  the project and the machine store, not here. Erasure makes an entry unreadable as to *who*, not
  as to *which chat*.
- **No entry is written to the audit store before AU-18 lands.** A chain cannot be pseudonymised
  after the fact, because rewriting an entry breaks every hash after it, and an entry written
  with a plain principal would sit under the genesis hashes for as long as the chain lives. That
  is the failure X19 asked to prevent. So AU-18 comes before AU-3 and before any other writer
  (§10, V25c).

### 4. Actions are registered, and metadata is typed

- **An action is a dotted name**, `<area>.<object>.<verb>` or `<area>.<verb>`
  (`trust.sandbox.off`, `forge.write`, `tool.call`).
- **Every action is in one registry**, in `charter-audit` (AU-2): its `meta` fields and their
  types, its `v`, and what AU-22 needs to export it. The writer refuses an unregistered action,
  and a test holds it there. WorkOS and Retraced register actions the same way (research 08
  §6.1).
- **`meta` is metadata only, and its types are a closed set:** an id, a name, an enum, a count, a
  duration, a boolean, a digest, a path relative to a repo or a project, and an **operator note**.
  A test fails a registry entry that declares any other type.
- **Never in an entry:** a prompt, a transcript, harness output, file contents, a diff, raw tool
  arguments, a secret value, an environment value, an absolute path or a URL's query. Tool
  arguments appear only as a digest: an HMAC keyed per device (AU-19). W9's org-encrypted option
  exists only on an org project, after GT-CLOUD.
- **An operator note is the one piece of text an entry may carry**: typed by the operator into a
  prompt that asks for it, on a `human` entry only, and at most 280 characters (an initial
  value, which AU-2 may change). ADR 0067's reason for a sandbox opt-out is one.

The registry starts with the actions that accepted records already promise. AU-4 and AU-5 add the
rest of O3's sources.

| Action | From | `meta` |
|---|---|---|
| `run.started`, `run.ended` | ADR 0066, ADR 0073 | `cause`, `level`, harness, profile name, `model_source`, sandbox backend, `parent_run` |
| `tool.call` | W8, AU-4 | tool name, args digest, the guard rule that decided |
| `sandbox.denied` | ADR 0067 | the denial class, a relative path or a digest |
| `trust.sandbox.off`, `trust.sandbox.on`, `trust.egress.exception` | ADR 0067 §7 | the classes lifted, the destination, an operator note |
| `forge.write` | ADR 0070 §6 | forge, account name, surface, operation, item |
| `secret.requested`, `secret.approved`, `secret.denied`, `secret.revealed` | V15, AU-5 | vault, secret name, channel, the approval's scope, how many times this chat has asked |
| `killswitch.stop`, `killswitch.rearm`, `killswitch.tamper` | ADR 0071 | `by` |
| `hook.spool.drained`, `hook.spool.gap`, `hook.spool.rejected` | ADR 0068 §6 | chat, the sequence range, the check's result |
| `host.started` | this record | the reconcile's result |
| `audit.chain.gap`, `audit.chain.genesis`, `audit.segment.pruned`, `audit.retention.changed`, `audit.exported` | this record, AU-3, AU-4, AU-22 | ranges, hashes, the settings, the export's target kind |
| `account.link` | ADR 0066 | nothing beyond the two pseudonyms |

### 5. Coverage says what charter could see

**Settled by W8** (a coverage label per harness level) and **ADR 0073 §7**, applied. Every
`agent` entry carries its run's coverage:

| `coverage` | The run |
|---|---|
| `process` | level 1: the process only |
| `hooks` | level 2: the tool calls its hooks report |
| `unarmed` | level 2 whose hooks have never reported (ADR 0073 §7) |
| `protocol` | level 3: every tool call its protocol reports |
| `observed` | a vendor-cloud "remote chat" charter lists but does not govern (W8, FD-19) |

A view of the audit shows the label beside the entry, so a reader never takes a level-1 chat's
silence for "it did nothing".

### 6. The store: rolling compressed segments in the OS data directory

**Settled by X47:** the audit lives only in the app data directory, and never in `.charter/`.

- **Where:** `<data>/audit/<device>/`. `<data>` is `$CHARTER_DATA_HOME` if set, else
  `$XDG_DATA_HOME/charter`, else the OS data directory's `charter/` (`~/Library/Application
  Support/charter` on macOS, `~/.local/share/charter` on Linux). This is a new Machine location
  beside the machine store in the config home (ADR 0069, amended below; V25a).
- **The writer refuses a path under a project or inside any git work tree**, whatever `<data>`
  resolves to. That is AU-3's acceptance and research 08 §6.3's rule that charter refuses any
  path under a project, and a test holds it.
- **One directory per device.** After a restore onto a machine that does not replace the old one,
  the old device's chain is kept as its records and never appended to (ADR 0069 §5). A directory
  per device id makes that a matter of not writing to it.
- **`charterd` is the only writer, and chats are denied the directory** (W8, ADR 0067 §5
  class 2).

| Store | Tier | Backed up (FR-10) |
|---|---|---|
| `<data>/audit/<device>/active.jsonl`: the segment being written, plain JSON lines | Machine, device-bound | yes |
| `<data>/audit/<device>/<first>-<last>.jsonl.zst`: sealed segments, zstd | Machine, device-bound | yes |
| `<data>/audit/<device>/checkpoints/`: signed checkpoints (AU-7) | Machine, device-bound | yes |
| `<data>/audit/retention.json`: retention and cap (§8) | Machine, syncable | yes |
| the pseudonym key per human (§3, AU-18) | Keyring | no |
| the device key (AU-3) | Keyring, or V1's age-encrypted file on a headless host | no |

`docs/plane-format.md` records each of these now, marked **decided, not yet written**, with its
tier, as ADR 0066 did for its fields. A store names its tier before it ships (ADR 0069, ruling
10), and these are decided before any code writes them.

**A segment is sealed at 64 MiB or at the end of the day, whichever comes first** (64 MiB is an
initial value, which AU-3 may tune against its throughput test). Sealing compresses it and names
it by its first and last `seq`. Only sealed segments are pruned (§8). A reader that wants to
search builds a rebuildable index over them (Q12's rule). The segments stay the truth.

### 7. Writing: grouped, off the hook's path, and reconciled

**Settled by the AU-1 row** (P3-24's fix): *"group commit (≤ 100 ms and at turn end), 1,000
events/s without moving hook p95"*.

- **A hook never waits on the audit.** The host answers a hook once the event is in the event
  log. The audit entry is written from the event log by a task of its own. The spec's 50 ms hook
  limit is unchanged.
- **Group commit.** Entries are batched and made durable together, at most 100 ms apart, at the
  end of each turn, and before a segment is sealed or a checkpoint is written.
- **Reconcile on start** (AU-4, P5-05). A crash loses at most the unflushed batch. At start the
  host compares the chain's last `event` with the event log, and writes the missing entries from
  the log. If the event log no longer has them, it writes `audit.chain.gap` with the range. The
  audit never loses an event silently (O1).
- **The hook spool.** **Settled by V22a:** *"the hook spool is one per chat, verified and sealed
  into the audit chain on drain"*. Each drained line becomes an event, and then an entry with the
  check's result in `meta`. A line that fails its MAC is `hook.spool.rejected`, and a gap in the
  sequence is `hook.spool.gap`.
- **The budget: 1,000 entries per second sustained, without moving the hook p95.** AU-3 holds the
  test, and `stress.yml` runs it on macOS and Linux (V8).
- **What it costs, as an estimate:** an entry is about 400 bytes as JSON. Ten busy chats at 0.3
  tool calls a second write about 1 KB/s, around 30 MB per 8-hour day, or about 4 MB compressed.

### 8. Retention and the disk cap

**O8** leaves local retention to the user. The AU-1 row asks for a default retention and a disk
cap. **Settled by V25b:**

- **Defaults: one year, and 2 GiB on disk.** An alert is raised at 80% of the cap.
- **Pruning removes whole sealed segments, oldest first,** and writes `audit.segment.pruned` with
  the segment's range and its last hash, so the chain verifies from the first segment kept.
- **A segment is pruned only when a signed checkpoint covers it, and, where the user has an
  export sink (AU-22), only once the sink has received it.** A segment that fails either test is
  kept, even past the retention or the cap.
- **At the cap, the oldest segment that passes both tests goes, even inside the retention
  period.** If none passes, the store grows past the cap and the alert says why. A new entry is
  never dropped and never sampled, and no agent is stopped.
- **Changing the retention or the cap is itself an entry**, `audit.retention.changed`. Org policy
  may clamp both later (C9), on an org project only.

### 9. Export, the org and the cloud

- **Export is a view of the store** (O6, W9). How an action maps to OCSF and how the CloudEvents
  envelope is filled is AU-22's to settle, from the registry of §4. Every export is an
  `audit.exported` entry.
- **`org` is `null` on every entry** until the machine has an account and the project an org.
  **Settled by W13**: the org log (AU-9), the merge of device chains (AU-10) and cloud retention
  (AU-11) wait for GT-CLOUD. This record makes them possible without a migration: `org` is in the
  entry, the device chain is what merges, and pre-join entries arrive flagged as device-attested
  (O7).

### 10. The first write waits for pseudonyms

The audit store gets its first entry only when AU-18 and AU-3 have both landed, and that entry is
AU-3's `audit.chain.genesis`. Until then:

- **ADR 0067's and ADR 0070's writers keep what their records do today**, with ADR 0034's
  nonexistent "machine-state log" read as the host's **event log** (FD-9), as the amendments
  below say. The event log is Machine and device-bound, is never committed and never sent, and
  ADR 0066 already allows the local principal in it.
- **When the audit starts, it is written from those events**, through the reconcile of §7, as far
  back as the event log still holds them. What is older than that has only its session record.
  Nothing is copied into the chain in plain, and nothing is rewritten.

## ADR 0066, amended

ADR 0066 names two actors: `agent:<persona>/<run>` for a run, and the local principal for the
operator. Its last section says the audit schema *"defines no ids of its own"*. This record keeps
that: it mints no id. It adds:

- **A field, `actor_kind`,** set from the channel an event arrived on (§2), so a reader never
  has to parse an actor to learn what it is. A pseudonymised actor cannot be parsed anyway.
- **Two actor forms:** `host:<device>`, for what `charterd` does on its own account (a drain, a
  reconcile, a gap, a prune, a key rotation), and `unattributed:<device>`, for a fact the host
  read from a file any process of the user can write. Neither is a principal that can be granted
  anything. They exist so that C4's rule, an actor on every entry, never forces a false one.
  Neither collides with `agent:none/<run>`, ADR 0066's form for a run with no persona.
- **Both forms and both of ADR 0066's are pseudonymised** before they are written (§3).

## ADR 0067, amended

§7 said: *"Until the audit chain (AU-1..AU-3) exists, these events go into the chat's session
record and charter's machine-state log (ADR 0034). AU-1 takes them over, and none is dropped when
it does."* There is no machine-state log. It now reads: **until the audit exists (§10), these
events go into the chat's session record and the host's event log. The audit is written from the
event log once AU-18 and AU-3 have landed.** The actions are `trust.sandbox.off`,
`trust.sandbox.on` and `trust.egress.exception`, and the reason the operator types is an operator
note (§4). The denials of §5 class 2 name the directory §6 places: `<data>/audit/`.

## ADR 0069, amended

- **§1, the Machine tier's places.** They were the machine store in the config home, the app's OS
  directories, and charter's lines in a harness's global config. They gain a fourth: **`<data>`,
  charter's data home**. That is `$CHARTER_DATA_HOME`, else `$XDG_DATA_HOME/charter`, else the OS
  data directory's `charter/`. It holds data that is too large or too long-lived for the config
  home and must never be kept with configuration. The audit is its first store. Every store in it
  is Machine, and names its own mark.
- **§6 and row 64, the audit chain.** *"Machine, device-bound. It is backed up"* stands, and the
  row now says where: `<data>/audit/<device>/`, with the stores of §6 above. Row 65, the device
  key, is unchanged.
- **FR-10** backs up `<data>/audit/` as well as the machine store.

Ruled by V25a.

## ADR 0070, amended

§6 said: *"Until the audit chain exists (AU-1 to AU-3), write entries go to the machine-state
log, as ADR 0067's audit events do. AU-1 takes them over without dropping any."* It now reads:
**until the audit exists (§10), each forge write is an event in the host's event log, and it
becomes a `forge.write` entry once AU-18 and AU-3 have landed.** The `Caller` maps onto the
entry: its human is `on_behalf_of`, and its chat, if any, is a `target`. The `actor` is that
chat's current run when the chat made the write, and the human when the operator made it,
whether or not a chat is named. The account and the surface go in `meta`. FI3 shows in the
entry: a write made with the human's token has a `human` actor, and an agent's own narrow
identity (SD-7a/b) has an `agent` one.

## ADR 0071, amended

The journal stays the switch's authority. The switch is still read by existence and without a
parse, and nothing here changes how it fails closed. What is added: **the host records each
journal line as an event when it reads it**, at start or through the watch, and the audit writes
it as `killswitch.stop`, `killswitch.rearm` or `killswitch.tamper`. A `window` line is a `human`
entry, an `app` line (a tamper the app wrote back) is a `host` entry, and a `cli` line is
`unattributed`, because any process of the user can run `charter stop --all` and the command is
not on a human scope. The residual that ADR 0071 states stands: a re-arm forged while no host
runs is recorded as what the host read.

## What changes where

The code does not change with this record.

| Where | Change |
|---|---|
| `CONTEXT.md` | Gains **Audit**, **Audit entry**, **Coverage** and **Telemetry** (in this PR) |
| `docs/plane-format.md` | `<data>` and its stores in the table of what charter-app keeps outside every project, each **decided, not yet written** with its tier; `<data>` in the Machine tier's definition; `CHARTER_DATA_HOME` and `XDG_DATA_HOME` among the variables (in this PR) |
| ADR 0066, ADR 0067, ADR 0069, ADR 0070, ADR 0071 | Amended above. Their texts are left as accepted, and this record is the amendment |
| AU-18 | The pseudonym and its key. It lands before AU-3 and before any write to the audit store (§10) |
| AU-2 (`charter-audit`) | The entry type, the action registry with its `meta` types, RFC 8785 canonical form, the hash chain and signature |
| AU-3 | The store of §6 and the writer of §7, with the path refusal, group commit, reconcile and the throughput test. Moves the `docs/plane-format.md` rows from decided to written |
| AU-4, AU-5 | Register the rest of O3's sources |
| AU-7 | Checkpoints in `checkpoints/`; `charter audit verify` across pruned segments |
| AU-8 | The viewer, the coverage labels and the retention settings of §8 |
| AU-19 | The args digest |
| AU-20 | The sandbox denial of `<data>/audit/` and the device key |
| AU-22 | Export, the OCSF and CloudEvents mapping, and the continuous sink |
| OB-1 to OB-3 | Telemetry, apart from the audit by §1's rules |
| FR-10 | Backs up `<data>/audit/` (ADR 0069, amended) |

## What this costs

- **Pseudonyms make the raw file unreadable.** A person reading a segment sees `ps1:…`, not a
  name. The viewer and export show names only while the keys exist. That is X19's point, and it
  also means a restore without the Keyring shows pseudonyms only.
- **The audit starts later.** Nothing is written until AU-18 and AU-3 have both landed. Until
  then the sandbox opt-outs and forge writes are in the event log and session records, and the
  audit picks them up only as far back as the event log reaches.
- **A second directory outside the project.** The machine store is in the config home and the
  audit is in the data home. A backup and an uninstall have to know both.
- **The cap can prune history the user wanted, or be exceeded.** A segment that no checkpoint or
  no sink has taken is kept past the cap, and the alert says so. Keeping everything forever would
  move the failure to a full disk.
- **The event log becomes load-bearing for the audit.** Reconcile needs the event log to keep
  what the audit has not yet written, so FD-24's event-log retention must be longer than any
  plausible outage of the audit writer. Days, not minutes, are enough.
- **An `unattributed` entry is a known hole.** A stop from the command line says only that it
  came from the command line.

## What was rejected

- **One store for audit and telemetry, with a kind column.** It is O1's opposite: one retention,
  one schema, and a sampler one setting away from the audit.
- **OTel logs as the audit store.** OTel may sample and batch-drop by design, and its exporter
  sends to a collector charter does not control.
- **Writing audit entries straight from hooks.** The hook would pay for the write, the chain
  would have many writers, and W8's "charterd is the only writer" would not hold.
- **SQLite as the store.** A hash chain is append-only and sequential, and plain segments are
  what tiled logs and exporters read. SQLite stays available as a rebuildable index over them.
- **Plain principals, erased by rewriting the chain.** A rewrite breaks every later hash, and
  verification after erasure is what X19 asks for.
- **Unchained, unpseudonymised entries before AU-3, sealed in at genesis.** The first draft of
  this record did that. Every such entry would keep a plain principal under the genesis hashes.
- **P3-24's default, "audit kept until exported, with a disk cap and a warning".** For the many
  users who never export, "until exported" means forever, so the cap alone would decide, with no
  rule for what goes first. §8 keeps its guard only where a sink exists.
- **Stopping every agent when the cap is reached.** A full disk would become the product's most
  common outage. An org may choose it later through policy.

## Ruled (V25, 2026-10-01)

The operator accepted all five questions as recommended:

1. **V25a: the audit lives in charter's data home, `<data>`.** ADR 0069 §1 gains it as a fourth
   Machine location.
2. **V25b: retention defaults are one year and 2 GiB.** A segment is pruned only once a checkpoint
   covers it and, where a sink exists, once the sink has it. When none qualifies, the store grows
   past the cap with an alert. P3-24's "kept until exported" is rejected.
3. **V25c: AU-18 moves ahead of AU-3 and of any write to the audit store**, to `Next ★`. Until then
   ADR 0067's and ADR 0070's writers keep their current behaviour.
4. **V25d: E9 is narrowed, and telemetry may read the event log.** An extension's CPU and memory
   are telemetry, and its capability use and crashes are audit. Telemetry may derive its OTel
   logs from the event log, and never reads the audit store.
