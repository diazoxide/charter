# charter serves one person's agents first, and its scale is a hot-chat count per RAM class

**Proposed 2026-10-01**, drafted for program-map ticket FD-1 (#640). It follows these of the
operator's rulings:

- **Q1:** *"Target: teams, about 200 open chats per machine (about 50 hot, the rest hibernated),
  unbounded through remote hosts, budgets measured in CI."*
- **G1:** *"The first customer is the solo power user running several harnesses and many agents,
  then small teams (2–10)."*
- **X35**, accepted with the consistency review as recommended: *"Is Q1 restated as "solo power
  user first, same budgets", and does hibernation apply only to harnesses whose M10 row marks
  resume as native, with others kept hot or closed with a session record?"*
- **X27**, accepted as recommended: *"Is a gate's user signal defined as "at least 3 design
  partners name the problem unprompted, or at least 30% of opted-in users use the shipped feature
  weekly""*.
- **W8:** *"The agent run is the unit of governance, whoever spawned it."* Also: *"Q1 is
  re-grilled as "agents visible and governable per human"; SC effort past what dogfood shows waits
  for a user signal."*
- **V8:** *"Q1's targets are stated per RAM class."* Also: *"Lazy restore at relaunch"* and
  *"`stress.yml` becomes a required check on macOS and Linux, asserting descriptor counts."*
- **V18:** *"The 73 findings of the map consistency audit are applied as written in its proposed
  fixes"*, which is how W8's and X27's words reached every SC ticket.

It builds on [ADR 0026](0026-the-apps-stack-is-locked-by-what-m0-measured.md) (what M0
measured at fifty sessions), [ADR 0068](0068-a-chat-lives-in-charterd-and-the-app-is-its-client.md)
(`charterd`, one per OS user per device, and its descriptors), [ADR 0069](0069-every-store-charter-writes-is-in-one-of-four-tiers.md)
(tiers), [ADR 0073](0073-a-harness-is-declared-as-data-and-a-chat-runs-it-at-one-of-three-levels.md)
(`resumes_by_id`) and [ADR 0076](0076-a-run-moves-only-by-a-named-cause-and-a-chats-state-is-read-from-its-runs.md)
(run states, hibernation, holds). It **amends** ADR 0026 and **supersedes** `docs/spec.md`'s goal
paragraph and its *Live sessions* limit, each in a section of its own below. SC-4 (hibernation),
SC-8 (CI stress at the target), SC-17 (performance budgets of record), SC-19 (the memory-pressure
guard) and RR-1 (the SSH runner) build on it.

Its concept is **Chat** ([ADR 0072](0072-charter-has-five-concepts-and-every-other-word-belongs-to-one-of-them.md)).

## Where charter is today

Three records say three different things about whom charter is for and how far it goes.

- **`docs/spec.md`'s goal** is one developer: *"A developer runs dozens of harness sessions
  (Claude Code, Codex) at once, across workspaces and repos, and always knows which one needs
  them."* Its limits table says *"Live sessions: **50**. This is the product's scale, not a speed
  target"*, and ADR 0026 closes its scrollback section with *"the product's scale is fifty"*.
- **Q1** says teams and 200 open chats per machine, 50 of them hot.
- **G1** says the solo power user first. X35 reconciled the two in words, and W8 re-grilled Q1 as
  a per-human target. Neither said what a number is per: per machine, per human or per project,
  and on which machine.

What charter measured, and where:

| Source | What | Number |
|---|---|---|
| `stress.yml` on `main`, 2026-09-28, Ubuntu (`stress.jsonl`) | fifty fake-harness tabs, three rounds | threads 49 → 245 at fifty (**3.9 a chat**); descriptors 54 → 204 (**3 a chat**); native memory 319 → 325 MB (**0.1 MB a chat**); fifty opened in 23 s; after closing, 45 threads and 50 descriptors, every round |
| the same run, macOS | | threads 24 → 223 (**4.0 a chat**); native memory 187 → 345 MB in round 1 (**3.2 MB a chat**) and flat after, 343 MB with all closed; fifty opened in 40 s; descriptors not counted on macOS yet (SC-15) |
| ADR 0026 (M0 bench, the operator's machine) | fifty sessions, 49 streaming | keystroke worst 26 ms; tab switch worst 48 ms; the app's native process 144 MB |
| ADR 0026 | an idle hidden session at the 5000-line cap, 150 columns | **20.2 MB** each |
| the research audit of 2026-09, the operator's machine, twelve chats | each Claude Code process | **385 MB average**, 235 MB to 1.1 GB |
| the same | the window's web content process | **813 MB, peak 1.5 GB**. No CI job measures it (SC-1) |

The stress job's harness is a fake, so its numbers are charter's own cost. **At fifty chats,
charter's native side costs a few hundred megabytes and the harnesses cost about 19 GB.** The
ceiling on one machine is the harnesses' memory, not charter's code.

## The decision

### 1. Whom: one person's agents first, then teams, with the same budgets

**Settled by X35:** *"Q1 restated as "solo power user first, same budgets""*. **Settled by G1:**
*"the solo power user running several harnesses and many agents, then small teams (2–10)."*

A team does not change any budget in this record. A team is several people, each on their own
devices, each running their own agents. Every number below is **per human** (W8) and, for memory,
**per device**. A team of five is five humans at these targets, not one machine at five times
them. Team features belong to the Project concept and to later tickets. None of them may assume a
larger per-device count than this record's.

### 2. What is counted: chats, hot and open

**Settled by W8:** *"agents visible and governable per human"*. **Visible** means listed with its
state. **Governable** means that it can be paused and stopped, the kill switch reaches it, and its
asks reach needs you (Q11, ADR 0071, ADR 0076). Both hold for **every** run whatever the count.
They are properties, not targets, and no budget below trades them away. A count past a target may
make the window slower. It never makes a run invisible or unstoppable.

Two counts, both over a human's **chats**, each read from its current run's state (ADR 0076 §1):

| Count | A chat is counted when its current run is | What it costs |
|---|---|---|
| **Hot** | `starting`, `working`, `input-required` or `paused`: it has a process | a harness process, about four of the host's threads, three descriptors, and its scrollback in memory |
| **Open** | any live state: `queued` to `hibernated` | hot chats, plus hibernated and queued ones, which have no process |

**Child runs are not counted separately.** A sub-agent runs inside its parent's harness process,
or is spawned by it. Either way its memory is part of what the parent's harness costs, and the
harness footprint below is measured with it. Child runs are still listed under their chat, as W8
says.

**A chat on a harness that cannot resume natively is hot for as long as it is open**, because
ADR 0076 §5 keeps it hot (settled there by X35). The open target is only reachable with chats on
harnesses that resume natively.

### 3. The targets, per RAM class

**Settled by V8:** *"Q1's targets are stated per RAM class."*

A device's **RAM class** is its physical memory, rounded down to the nearest class below. The
hot target is what fits when charter and its harnesses use **at most half** of the device's
memory, leaving the other half to the operating system, the editor, the browser and the builds
the agents start:

```
hot target = floor( (RAM / 2 − charter's own base) / (harness footprint + scrollback) )
```

With today's measurements, in binary units: charter's own base is **2 GB** (the web content
process's measured peak of 1.5 GB plus the native side at about 350 MB), the harness footprint is **385 MB** (Claude
Code, the only one measured), and the scrollback is **20 MB** a hot chat. That gives:

| RAM class | Physical memory | Hot chats | Open chats |
|---|---|---|---|
| 8 GB | under 12 GB | **5** | 200 |
| 16 GB | 12 to 24 GB | **15** | 200 |
| 32 GB | 24 to 48 GB | **35** | 200 |
| 64 GB | 48 GB and over | **50** | 200 |

- **50 is a ceiling, not the formula's answer.** On 64 GB the formula gives 75. Q1's *"about 50
  hot"* is the top class's number, because 50 is what ADR 0026 measured the spec's felt-speed
  limits at, and no one has measured the window past it. A larger class raises it only through §6.
- **Open is 200 in every class.** A hibernated chat has no process (ADR 0076 §1), so what it
  costs is charter's own: a row, a journal entry and a scrollback snapshot on disk (§5). SC-17
  sets that cost's budget; this record only requires that it does not grow with RAM class.
- **The counts are the formula applied to today's measurements.** When SC-1 measures Codex's and
  opencode's footprints, or SC-17 records a new base, the table is recomputed from the formula,
  using the largest measured harness footprint. The formula is the decision; the table is its
  current answer.
- **Beyond one device: Q1's *"unbounded through remote hosts"*.** A runner is a device (ADR 0068
  §8), with its own class and its own targets. A human's total is the sum over their devices.
  Visibility and governability (§2) hold across all of them.

### 4. A target is a budget, never a cap on the operator

**The hot target never refuses, queues or hibernates a chat the operator starts.** Going past it
is the operator's choice on their own machine, and a number derived from a judgement (the half
share) is no reason to stop them.

- **SC-19** warns before a chat starts under memory pressure and offers to hibernate idle ones. It
  reads the class's hot target to say how far past it the device is. It does not block.
- **ADR 0076 §3's `capacity` hold** stays what it is: a trigger queue's or an explicit cap's. The
  hot target is not one. A trigger (AC-7) may use the target as its cap, and says so.
- **Hibernation stays ADR 0076 §5's**: idle past the threshold, at a turn boundary, on a harness
  that resumes natively. The hot target does not shorten the threshold. SC-13 (a scheduler that
  does) is gated by §6.

### 5. What is measured where

| Where | What | Kind |
|---|---|---|
| CI, `stress.yml`, macOS and Linux (SC-8) | fake harness: the top class's 50 hot plus 150 hibernated, three rounds. Native memory, web content memory, threads and descriptors, each against its SC-17 budget, and the leak check between rounds | **charter's own cost.** A relative-regression gate, required once SC-8 lands (V8) |
| CI, `bench.mjs` (SC-16) | the spec's latency limits at 50 hot | relative regression |
| the operator's machine, each release (DF-2) | real harnesses at that machine's class: the harness footprint, the web content peak, the spec's felt-speed limits | **absolute.** The numbers feed the formula |

**CI cannot measure a harness's footprint**, because CI runs a fake one. The formula is why that
is enough: CI holds charter's own base and per-chat cost, and the operator's machine measures the
harness term. Each class's count is arithmetic from both, not a run on a machine of that class.

**Stores.** This record adds one store, and relies on a CI artifact that is not a store:

| Store | Tier | What it holds |
|---|---|---|
| `<data>/scrollback/<chat id>` | Machine, device-bound, rebuildable | **decided, not yet written** (SC-4). A hibernated chat's scrollback, so its pane shows history on waking. Rebuildable, because the harness's own resume brings the conversation back and the snapshot only saves the pane's history. Device-bound, because the harness session it shows lives on this device. Chats are denied it. Deleted when its chat wakes or ends. SC-5 bounds its size. SC-4 may rename the path, not the tier |
| `app/logs/stress.jsonl` | None | a CI artifact the e2e harness writes, not a store charter writes. Kept by the workflow's artifact retention |

The RAM class is read from the operating system when the host starts and is not stored.

### 6. Past the target waits for a user signal

**Settled by W8:** *"SC effort past what dogfood shows waits for a user signal."* **Settled by
X27:** the signal is *"at least 3 design partners name the problem unprompted, or at least 30% of
opted-in users use the shipped feature weekly"*.

**A miss inside a target is a bug. Going past a target is a feature.** Measuring and guarding
never wait. **What dogfood shows** is the counts on the maintainers' own devices, running the
current release daily (DF-2), read at each planning pass. Building for a count that neither
dogfood nor this record's table has reached waits for the signal. So does raising any number in
§3's table beyond what the formula gives.

| SC ticket | Waits for a signal? | Why |
|---|---|---|
| SC-1, SC-15, SC-17, SC-8, SC-16 | no | measuring, and budgets of record |
| SC-2, SC-3, SC-7, SC-10 | no | a felt hitch or an unbounded growth inside today's counts |
| SC-19 | no | a guard, and it refuses nothing (§4) |
| SC-4, SC-20, SC-5 | no for their acceptance; yes past it | the open target of 200 cannot be met without hibernation, and no class holds 200 hot |
| SC-12 | no if SC-8 finds a miss; yes otherwise | its own text already says so |
| SC-6, SC-13, SC-14 | yes | each makes more than the table's counts fit, or trades a cost the table does not need |

## ADR 0026, amended

ADR 0026's scrollback section ends *"the limit is per session, and the product's scale is fifty."*
**The product's scale is now this record's §3**: 200 open chats, and a hot count per RAM class
whose top is fifty. ADR 0026's measurements stand and are still what the top class's felt-speed
limits were measured at. Its 20.2 MB per idle hidden session is the scrollback term in §3's formula.

## `docs/spec.md`, superseded in two places

- **The goal** (*"A developer runs dozens of harness sessions…"*). Its actor stays one developer,
  and gains §1's "then teams, with the same budgets". Its "dozens" becomes §3's counts. Its "no
  daemon" was already replaced by ADR 0068.
- **The limits table's *Live sessions* row** (*"**50**. This is the product's scale"*) becomes two
  rows: **open chats, 200**, and **hot chats, per RAM class (ADR 0082 §3), 50 at the top class**.
  The other limits are unchanged and are held at the device's hot count.

Both edits are in this PR, each marked with this record's number.

## What changes where

The code does not change with this record.

| Where | What changes |
|---|---|
| `docs/spec.md` | The goal and the *Live sessions* row, as above (in this PR) |
| `docs/plane-format.md` | `<data>/scrollback/<chat id>`, **decided, not yet written** (in this PR) |
| `CONTEXT.md` | Gains **Hot chat** and **RAM class** (in this PR) |
| ADR 0026 | A line pointing its *"the product's scale is fifty"* at this record, once it is accepted |
| SC-4, SC-5 | The scrollback snapshot's store and its bound |
| SC-8 | 50 hot plus 150 hibernated fake chats, asserting §5's measures |
| SC-17 | The base and per-chat budgets, a hibernated chat's cost, and a row naming the formula's inputs |
| SC-19 | Reads the class's hot target. Warns and offers; never refuses |
| RR-1 | A runner reports its RAM class, and its targets are its own |

## What this costs

- **A 16 GB laptop is promised 15 hot chats, not 50.** That is what fits. Saying 50 would be a
  number the harnesses break, not charter.
- **The table moves when harnesses change.** A harness that doubles its memory halves the hot
  count, and the table follows the formula rather than holding the old number.
- **The half-of-memory share is a judgement.** It is a working machine's headroom, not a measured
  limit. A device used only for agents could run more, and §4 lets it.
- **CI never runs a real harness at scale.** The per-class counts are arithmetic, checked on one
  class per release.

## What was rejected

- **One number for every machine** (the spec's fifty, or Q1's fifty). V8 rules it out, and 50
  Claude Code processes need about 19 GB, which a 16 GB machine does not have.
- **Counting per project or per machine instead of per human.** W8 names the human. A person with
  three projects open is still one person watching their agents, and a shared runner serves
  several humans, each at their own targets.
- **Counting child runs as chats.** Their memory is inside the parent's harness footprint, so
  counting them twice would shrink the targets for a cost already counted.
- **The hot target as a cap that queues new chats.** It stops the operator on their own machine
  for a number derived from a judgement (the half share). The guard warns instead (SC-19).
- **Measuring each class on a machine of that class in CI.** Hosted runners come in fixed sizes,
  and a fake harness would not show the harness term anyway.
- **A higher top than 50 on large machines now.** Nothing measures the window's felt speed past 50,
  and no user has asked. §6 is the way to raise it.

## Decided in drafting

1. **Counts are over chats, read from the current run's state; child runs are not counted
   separately.** Rejected: counting runs (a chat's ended runs are history, not load), and counting
   child runs (§2).
2. **A RAM class is physical memory rounded down to 8, 16, 32 or 64 GB**, with boundaries at 12,
   24 and 48 GB. Rejected: classes by available memory (it changes by the minute) and by machine model
   (it misses Linux).
3. **The hot target comes from a formula with a half-of-memory share**, and the table is its
   current answer. Rejected: fixed numbers per class, which age with every harness release, and a
   larger share, which leaves a working machine no room for builds.
4. **Open is 200 in every class**, since a hibernated chat has no process. Rejected: scaling open
   with RAM, which ties a cost that is disk and rows to memory.
5. **A target never refuses or queues an operator's chat** (§4). Rejected: a cap (above).
6. **The hibernated chat's scrollback snapshot is Machine, device-bound, rebuildable**, and FR-10
   does not back it up. Rejected: not rebuildable, which would make FR-10 copy gigabytes of pane
   history that the harness's resume replaces.
7. **"What dogfood shows" is the maintainers' daily counts under DF-2**, read at each planning
   pass, and §6's table sorts the SC tickets by it.

## For the operator's ruling

1. **Q1's "about 50 hot" is the 64 GB class's number, and smaller classes get fewer** (5, 15 and
   35 today). *Recommend yes.* V8 says Q1's targets are stated per RAM class, so this reads V8
   rather than changing Q1. It is listed because it narrows what Q1 said for most laptops.
2. **No public number until SC-8 and a release's measurement on the operator's machine confirm
   it.** Until then the website, the README and launch copy say what §2 guarantees (every agent
   listed, pausable and stoppable) and no count. Once measured, a count is published as
   "measured on <class>", never as a promise. *Recommend yes.* A number on the website is hard to
   take back, and the one number that would be quoted, 50, is only true on the top class.
