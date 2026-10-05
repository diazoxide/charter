# purlis serves one person's agents first, and its scale is a hot-chat count per RAM class

**Accepted 2026-10-01** by the dispatcher under the operator's delegation of 2026-10-01 (only
major decisions go to the operator); decisions D-0082a/b. Drafted for program-map ticket FD-1
(#640). It follows these of the operator's rulings:

- **Q1:** *"Target: teams, about 200 open chats per machine (about 50 hot, the rest hibernated),
  unbounded through remote hosts, budgets measured in CI."*
- **G1**, in part: *"the solo power user running several harnesses and many agents, then small
  teams (2–10)."*
- **X35**, accepted with the consistency review as recommended: *"Is Q1 restated as "solo power
  user first, same budgets", and does hibernation apply only to harnesses whose M10 row marks
  resume as native, with others kept hot or closed with a session record?"*
- **X27**, accepted as recommended, which defines a gate's user signal. LN-6 carries the
  definition.
- **W8:** *"The agent run is the unit of governance, whoever spawned it."* Also: *"Q1 is
  re-grilled as "agents visible and governable per human"; SC effort past what dogfood shows waits
  for a user signal."*
- **V8:** *"Q1's targets are stated per RAM class."* Also: *"`stress.yml` becomes a required check
  on macOS and Linux, asserting descriptor counts."*
- **V18:** *"The 73 findings of the map consistency audit are applied as written in its proposed
  fixes"*, which is how W8's and X27's words reached the SC tickets that carry them.

It builds on [ADR 0026](0026-the-apps-stack-is-locked-by-what-m0-measured.md) (what M0
measured at fifty sessions), [ADR 0068](0068-a-chat-lives-in-charterd-and-the-app-is-its-client.md)
(`purlisd`, one per OS user per device, and its descriptors), [ADR 0069](0069-every-store-charter-writes-is-in-one-of-four-tiers.md)
(tiers), [ADR 0073](0073-a-harness-is-declared-as-data-and-a-chat-runs-it-at-one-of-three-levels.md)
(`resumes_by_id`), [ADR 0076](0076-a-run-moves-only-by-a-named-cause-and-a-chats-state-is-read-from-its-runs.md)
(run states, hibernation, holds) and [ADR 0078](0078-a-runner-is-charterd-behind-a-connector-and-charters-own-keys-say-who-is-on-the-link.md)
(a runner is a device). It **amends** ADR 0026 and `docs/spec.md`'s goal paragraph and its
*Live sessions* limit, each in a section of its own below. SC-4 (hibernation), SC-8 (CI stress at
the target), SC-17 (performance budgets of record), SC-19 (the memory-pressure guard) and RR-1
(the SSH runner connector) build on it.

Its concept is **Chat** ([ADR 0072](0072-charter-has-five-concepts-and-every-other-word-belongs-to-one-of-them.md)).
The **RAM class** it defines is a property of a device, which is a machine-wide setting, so that
part belongs to **Project**.

**Units.** Every memory size in this record is binary: 1 GB is 1024 MB, and 1 MB is 1024 KB.
`stress.jsonl` records KB, and its figures below are converted.

## Where purlis is today

Three records say three different things about whom purlis is for and how far it goes.

- **`docs/spec.md`'s goal** is one developer: *"A developer runs dozens of harness sessions
  (Claude Code, Codex) at once, across workspaces and repos, and always knows which one needs
  them."* Its limits table says *"Live sessions: **50**. This is the product's scale, not a speed
  target"*, and ADR 0026 closes its scrollback section with *"the product's scale is fifty"*.
- **Q1** says teams and 200 open chats per machine, 50 of them hot.
- **G1** says the solo power user first. X35 reconciled the two in words, and W8 re-grilled Q1 as
  a per-human target. Neither said what a number is per (a machine, a human or a project), or on
  which machine.

### What was measured, where and when

These are the numbers the formula in §3 uses. They are written here because CI artifacts expire.

| Source | What | Number |
|---|---|---|
| `stress.yml` run **36474042885** on `main`, 2026-09-28, Ubuntu (its `stress.jsonl`) | fifty fake-harness tabs, three rounds | threads 49 → 245 at fifty (**3.9 a chat**); descriptors 54 → 204 (**3 a chat**); native memory 311.4 → 317.4 MB (**0.12 MB a chat**); fifty opened in 23 s; after each round's close, 45 threads and 50 descriptors; 321.7 MB after round 3 |
| the same run, macOS | | threads 24 → 223 (**4.0 a chat**); native memory 182.6 → 337.2 MB in round 1 (**3.1 MB a chat**), flat in rounds 2 and 3, 334.6 MB with all closed; fifty opened in 40 s; descriptors not counted on macOS yet (SC-15) |
| ADR 0026 (M0 bench, the operator's machine) | fifty sessions, 49 streaming | keystroke worst 26 ms; tab switch worst 48 ms; the app's native process 144 MB |
| ADR 0026 | an idle hidden session at the 5000-line cap, 150 columns | **20.2 MB** each |
| the operator's macOS machine, 2026-09-29, charter 0.2.0-dev.138 up 3 days with 12 chats, by `ps` | each of 14 Claude Code processes | **385 MB average**, 235 MB to 1.1 GB, 5.39 GB in all |
| the same machine and moment, by `footprint` | the window's web content process | **813 MB, peak 1,529 MB**. No CI job measures it yet (SC-1) |

The stress job's harness is a fake, so its numbers are purlis's own cost. **At fifty chats,
purlis's native side costs a few hundred megabytes and the harnesses cost about 19 GB.** The
ceiling on one machine is the harnesses' memory, not purlis's code.

## The decision

### 1. Whom: one person's agents first, then teams, with the same budgets

**Settled by X35:** *"Q1 restated as "solo power user first, same budgets""*. **Settled by G1:**
*"the solo power user running several harnesses and many agents, then small teams (2–10)."*

A team does not change any budget in this record. A team is several people, each on their own
devices, each running their own agents. A team of five is five people at these targets, not one
machine at five times them. Team features belong to the Project concept and to later tickets.
None of them may assume a larger per-device count than this record's.

### 2. What is counted: chats on a device, hot and open

**Settled by W8:** *"agents visible and governable per human"*. **Visible** means listed with its
state. **Governable** means that it can be paused and stopped, the kill switch reaches it, and its
asks reach needs you (Q11, ADR 0071, ADR 0076). Both hold for **every** run whatever the count.
They are properties, not targets, and no budget below trades them away. A count past a target may
make the window slower. It never makes a run invisible or unstoppable.

**The targets are per device.** A device's `purlisd` serves one OS user (ADR 0068), so a
device's count is one human's chats on it. A human's total is the sum over their devices (§3).

Two counts, each over a device's **chats** and read from the current run's state (ADR 0076 §1):

| Count | A chat is counted when its current run is | What it costs |
|---|---|---|
| **Hot** | `starting`, `working`, `input-required` or `paused`: it has a process | a harness process, about four of the host's threads, three descriptors, and its scrollback in memory |
| **Open** | any live state, `queued` to `hibernated` | hot chats, plus hibernated and queued ones, which have no process |

**Child runs are not counted separately.** A sub-agent runs inside its parent's harness process,
or is spawned by it. Either way its memory is part of what the parent's harness costs, and the
harness footprint below is measured with it. Child runs are still listed under their chat, as W8
says.

**A chat on a harness that cannot resume natively is hot for as long as it is open**, because
ADR 0076 §5 keeps it hot (settled there by X35). The open target can only be reached with chats
on harnesses that resume natively.

### 3. The targets, per RAM class

**Settled by V8:** *"Q1's targets are stated per RAM class."*

**A device's RAM class is the largest of 8, 16, 32 and 64 GB that is not more than its physical
memory.** A 12 GB machine is in the 8 GB class, and a 128 GB machine is in the 64 GB class. A
machine with less than 8 GB has no class, and its hot target is **1**.

The hot target is what fits when purlis and its harnesses use **at most half** of the class's
memory. That leaves the other half to the operating system, the editor, the browser and the
builds the agents start:

```
hot target = max( 1, floor( (class / 2 − charter's own base) / (harness footprint + scrollback) ) )
```

The inputs, from the measurements above:

- **purlis's own base is 2 GB.** That is the web content peak of 1,529 MB plus the native side at
  about 337 MB, rounded up.
- **The harness footprint is 385 MB.** That is Claude Code, the only harness measured.
- **The scrollback is 20.2 MB** for each hot chat.

The formula gives:

| RAM class | Hot chats (target) | Open chats (target) |
|---|---|---|
| under 8 GB (no class) | **1** | 200 |
| 8 GB | **5** | 200 |
| 16 GB | **15** | 200 |
| 32 GB | **35** | 200 |
| 64 GB | **50** | 200 |

- **50 is a ceiling, not the formula's answer.** On 64 GB the formula gives 75. Q1's *"about 50
  hot"* is the top class's number (D-0082a), because 50 is what ADR 0026 measured the spec's
  felt-speed limits at, and nobody has measured the window past it. Only §6 raises it.
- **Open is 200 in every class.** A hibernated chat has no process (ADR 0076 §1), so it costs only
  purlis's own resources: a row, a journal entry and what SC-4 keeps of its scrollback. SC-17 sets
  that cost's budget. This record only requires that it does not grow with RAM class.
- **The counts are targets, not measurements.** They are the formula applied to today's
  measurements. When SC-1 measures Codex's and opencode's footprints, or a release measurement
  records a new base (§5), the table is recomputed from the formula using the largest measured
  harness footprint. The formula is the decision; the table is its current answer.
- **Beyond one device: Q1's *"unbounded through remote hosts"*.** A runner is a device running
  `purlis serve` (ADR 0078), with its own class and its own targets. A human's total is the sum
  over their devices. Visibility and governability (§2) hold across all of them.

### 4. A target is a budget, never a cap on the operator

**The hot target never refuses, queues or hibernates a chat the operator starts.** Going past it
is the operator's choice on their own machine. A number derived from a judgement (the half share)
is no reason to stop them.

- **SC-19** warns before a chat starts under memory pressure and offers to hibernate idle ones. It
  reads the class's hot target to say how far past it the device is. It does not block.
- **ADR 0076 §3's `capacity` hold** stays what it is: a trigger queue's or an explicit cap's. The
  hot target is not one. A trigger (AC-7) may use the target as its cap, and says so when it does.
- **Hibernation stays as ADR 0076 §5 defines it**: idle past the threshold, at a turn boundary, on a
  harness that resumes natively. The hot target does not shorten the threshold. SC-13 (a
  scheduler that would) is gated by §6.

### 5. What is measured where, and who owns it

| Where | What | Kind | Owner |
|---|---|---|---|
| CI, `stress.yml`, macOS and Linux | fake harness: the top class's 50 hot plus 150 hibernated, three rounds. Native memory, web content memory, threads and descriptors, each against its SC-17 budget, and the leak check between rounds | **purlis's own cost.** A relative-regression gate, required once SC-8 lands (V8) | SC-1 adds the fields, SC-8 the load and the budgets |
| CI, `bench.mjs` | the spec's latency limits at 50 hot | relative regression | SC-16 |
| the operator's machine, each release | real harnesses at that machine's class: the harness footprint per harness and the web content peak | **absolute.** These feed the formula | **a follow-up ticket to file** (per-release scale measurement) |
| the maintainers' devices, daily | the open and hot counts they actually run | what dogfood shows (§6) | the same follow-up ticket |

**CI cannot measure a harness's footprint**, because CI runs a fake one. The formula is why that
is enough: CI holds purlis's own base and per-chat cost, and the release measurement supplies the
harness term. Each class's count is arithmetic from both, not a run on a machine of that class.

**Stores.** This record adds no store. `app/logs/stress.jsonl` is tier **None**: a CI artifact the
e2e harness writes, not a store purlis writes, and kept only by the workflow's artifact
retention. **The hibernated chat's scrollback snapshot is SC-4's to decide**, with its store and
tier. The RAM class is read from the operating system when the host starts and is not stored.

### 6. Past the target waits for a user signal

**Settled by W8:** *"SC effort past what dogfood shows waits for a user signal."* The signal is
X27's, as LN-6 records it.

**A miss inside a target is a bug. Going past a target is a feature.** Measuring and guarding
never wait. **What dogfood shows** is the counts the maintainers run on their own devices (§5),
read at each planning pass. Building for a count that neither dogfood nor §3's table has reached
waits for the signal. So does raising any number in §3's table beyond what the formula gives.

Each SC ticket, by what its own row says:

| SC ticket | Waits for a signal? | Why |
|---|---|---|
| SC-1, SC-8, SC-15, SC-16, SC-17, SC-18 | no | measuring, and budgets of record |
| SC-2, SC-3, SC-7, SC-10, SC-11 | no | a felt hitch, an unbounded growth, or a lost layout inside today's counts |
| SC-19 | no | a guard, and it refuses nothing (§4) |
| SC-4 | no for its acceptance; yes past it | its row says so, and the open target cannot be met without hibernation |
| SC-5, SC-20 | no | their rows carry no signal clause |
| SC-12 | no if SC-8 finds a miss; yes otherwise | its row says so |
| SC-13 | yes | its row: built only after a user signal |
| SC-6, SC-14 | no clause; they follow their horizon | their rows carry no signal clause. §6's rule applies if either is used to raise a target |

## ADR 0026, amended

ADR 0026's scrollback section ends *"the limit is per session, and the product's scale is fifty."*
**The product's scale is now this record's §3**: a target of 200 open chats per device, and a hot
target per RAM class whose top is fifty. ADR 0026's measurements stand, and they are still what
the top class's felt-speed limits were measured at. Its 20.2 MB per idle hidden session is the
scrollback term in §3's formula.

## `docs/spec.md`, amended

- **The goal** (*"A developer runs dozens of harness sessions…"*). Its actor stays one developer,
  and gains §1's "then teams, with the same budgets". Its "dozens" becomes §3's targets. Its "no
  daemon" was already replaced by ADR 0068.
- **The limits table's *Live sessions* row** (*"**50**. This is the product's scale"*) becomes two
  rows, both marked as **targets, not measured**: **open chats, 200 per device**, and **hot chats,
  per RAM class (ADR 0082 §3), 50 at the top class**.
- **"Keystroke to screen ≤ 50 ms while 49 other sessions stream"** now reads *while the device's
  hot target minus one other chats stream*: 49 at the top class.
- The other limits are unchanged and are held at the device's hot target.

These edits are in this PR, each marked with this record's number.

## What changes where

The code does not change with this record.

| Where | What changes |
|---|---|
| `docs/spec.md` | The goal, the *Live sessions* row and the keystroke row, as above (in this PR) |
| `CONTEXT.md` | Gains **Hot chat** and **RAM class** (in this PR) |
| ADR 0026 | A line pointing its *"the product's scale is fifty"* at this record (in this PR) |
| A follow-up ticket to file | The per-release scale measurement: each harness's footprint and the web content peak on the operator's machine, and the maintainers' daily open and hot counts (§5) |
| SC-4 | The hibernated chat's scrollback: where it is kept, its tier, and SC-5's bound on it |
| SC-8 | 50 hot plus 150 hibernated fake chats, asserting §5's measures |
| SC-17 | The base and per-chat budgets, a hibernated chat's cost, and a row naming the formula's inputs |
| SC-19 | Reads the class's hot target. Warns and offers; never refuses |
| RR-1 | The SSH runner connector's device reports its RAM class, and its targets are its own |

## What this costs

- **A 16 GB laptop targets 15 hot chats, not 50.** That is what fits. A target of 50 would be a
  number the harnesses break, not purlis.
- **The table moves when harnesses change.** A harness that doubles its memory halves the hot
  count, and the table follows the formula rather than keeping the old number.
- **The half-of-memory share is a judgement.** It is a working machine's headroom, not a measured
  limit. A device used only for agents could run more, and §4 lets it.
- **CI never runs a real harness at scale.** The per-class counts are arithmetic, checked on one
  class per release.

## What was rejected

- **One number for every machine** (the spec's fifty, or Q1's fifty). V8 rules it out, and 50
  Claude Code processes need about 19 GB, which a 16 GB machine does not have.
- **Counting per project instead of per device.** A person with three projects open is still one
  person on one device, and every project's chats share that device's memory.
- **Counting child runs as chats.** Their memory is inside the parent's harness footprint, so
  counting them twice would shrink the targets for a cost already counted.
- **The hot target as a cap that queues new chats.** It stops the operator on their own machine
  for a number derived from a judgement (the half share). The guard warns instead (SC-19).
- **Measuring each class on a machine of that class in CI.** Hosted runners come in fixed sizes,
  and a fake harness would not show the harness term anyway.
- **A higher top than 50 on large machines now.** Nothing measures the window's felt speed past
  50, and no user has asked. §6 is the way to raise it.

## Decided in drafting

1. **Counts are over a device's chats, read from the current run's state, and child runs are not
   counted separately.** Rejected: counting runs (a chat's ended runs are history, not load), and
   counting child runs (§2).
2. **A RAM class is the largest of 8, 16, 32 and 64 GB not more than physical memory, and a
   machine under 8 GB targets 1 hot chat.** Rejected: classes by available memory (it changes by
   the minute), classes by machine model (that misses Linux), and a target of 0, which would say a
   small machine cannot run purlis at all.
3. **The hot target comes from a formula with a half-of-memory share**, and the table is its
   current answer. Rejected: fixed numbers per class, which age with every harness release, and a
   larger share, which leaves a working machine no room for builds.
4. **Open is 200 in every class**, since a hibernated chat has no process. Rejected: scaling open
   with RAM, which ties a cost in disk and rows to memory.
5. **A target never refuses or queues an operator's chat** (§4). Rejected: a cap (above).
6. **The hibernated chat's scrollback store is left to SC-4.** Rejected: deciding it here, which
   would pre-empt SC-4's own design.
7. **"What dogfood shows" is the maintainers' daily counts**, owned by the per-release scale
   measurement ticket still to be filed, read at each planning pass. §6's table sorts the SC
   tickets by what their own rows say.

## Decided (dispatcher, 2026-10-01)

- **D-0082a:** Q1's "about 50" is the count for the top RAM class. This applies V8; it does not
  amend Q1.
- **D-0082b:** no public scale number is published until SC-8 and a release measurement confirm
  it. **This record does not own publishing.** The ticket that writes the product's public copy
  does, and it holds to this rule.
