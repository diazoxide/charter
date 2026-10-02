# A chat is a ULID, a run is a stretch of its conversation, and a device is random

**Accepted 2026-09-30** by the operator (ruling V21), drafted for program-map ticket FD-22 from the
operator's ruling V1 (phase-2 critique, accepted 2026-09-30). It
lands before FD-9's event log and AU-1's audit schema, and both build on it.

Today a chat's identity is a `u32`. `Sessions` deals it, and `reopen.json`'s `dealt` makes sure
it is never dealt twice (charter-app#90). That holds only **in one plane, on one machine**.
Four things lean on the number as if it held everywhere:

- **Session records** commit `chat: 7` to the plane's git (`sessionrecord::render`). They
  record no machine, so chat 7 on laptop A and chat 7 on laptop B are two different chats in the
  same `sessions/index.md`.
- **Handoff lineage** is kept as `from: {chat: <n>, name, workspace, report}` in `reopen.json`.
  The window draws it by name (`handedFrom.ts`: *"By the parent's name, never its number"*). A
  parent that is renamed, closed, or on another machine leaves a note that points at nothing.
- **Per-host logs** key on the hostname (`dispatch::host`). Two laptops called `MacBook-Pro`
  share a file, a renamed laptop starts a new one, and the audit chain AU-3 plans would give
  the same machine a second identity through its device key.
- **W8 made "the agent run" the unit of governance** and never said what a run is. The code has
  three candidates: a session (one PTY process), a chat (a tab with a number), and a
  conversation (the harness's id, which moves on `/clear`). Budgets set on a chat, audit keyed on
  a run and cost keyed on a conversation would not add up.

FD-9's events, AU-1's `actor` and `target`, OV-6's budgets, FD-18's child agents, the relay's
catch-up and the plane format as an open spec (KN-26) all need these ids. They are cheap to fix
now and a migration once any of those ships. This record fixes them, and no code changes with
it.

## The decision

### A chat is a ULID and its origin device, and the number is what the window calls it

**A chat's id is a [ULID](https://github.com/ulid/spec)**, minted once when the chat is created,
and never changed. (Once per clone and device since V43: see *ADR 0066, amended*, below.) The **origin device** is recorded beside it: the device id (below) of the
machine that created the chat. The origin device is a fact about the chat, not part of its key.
80 random bits per millisecond make a collision between two devices negligible, and a key that
contained a device would carry a stale device once RR-4 moves the chat.

**The `u32` stays, as the chat's display number** (`steward 3`). It keeps doing everything it
does today on this machine: `$CHARTER_CHAT` and `$CHARTER_SESSION_ID`, the hook token map, the
per-chat files under `.charter/sessions/<n>.*`, the `chat-<n>/` handback directory, the
`<chat>.saved` relay marker. `dealt` still guarantees that no number is dealt twice in a plane.

**The rule that decides which id goes where: the number never leaves the clone, and the id is
what leaves.** Anything a clone keeps for itself, on one machine, keys on the number, because the
number is unique there and it is what the operator reads. Anything committed, sent to another
process or device, or written to a store that outlives the clone, names the chat by its id: the
session record's frontmatter, lineage, events, audit entries and OTel. A number in one of those
is a label next to the id, never a key.

**Who mints it.** The host deals the number and mints the id in the same step. Today that is
the app's `Sessions`, and after FD-5 it is `charterd`. A chat charter only observes (FD-19's
vendor-cloud "remote chats", W8) gets an id when charter first lists it.

**No new environment variable.** A process inside a chat asks the app's record for its chat's id
by number, the way `reopen::conversation_of` answers its conversation today. `charter session
record` already reads the record that way. A second answer in the environment would be one more
place for the two to disagree.

### A run is a stretch of a chat's conversation with fixed attributes

**A run is one contiguous stretch of one harness conversation inside a chat, over which its
attributes do not change.** It has a ULID of its own. A chat has one or more runs, one after
another, and the latest one is the chat's **current run**.

A run's attributes are fixed for its whole life:

| Attribute | What it is |
|---|---|
| `chat` | the chat's id |
| `device` | the device the run's harness process runs on. It can differ from the chat's origin device once a chat moves (RR-4) or runs on a runner (V9) |
| `persona` | the persona the chat has adopted, or none |
| `harness` | the harness, by the word the plane uses for it (`claude`, `codex`, `opencode`) |
| `profile` | the harness profile, by name, never its command (ADR 0022) |
| `model_source` | where the model is reached: the profile's login, an API key, a gateway, a local model. It is read from the profile's declaration, never from output |
| `sandbox` | the sandbox backend the run is under: `none` today, and whatever SD-1 and W8's external backends add |
| `conversation` | the harness's own conversation id, where it has named one |
| `parent_run` | the run that spawned this one, for a child agent only (below) |
| `cause` | why the run began, from the list below |

**A new run begins exactly when one of these happens.** Each cause is something the host already
learns from a hook, the process exit or its own action. None is read from output (spec decision
3).

| `cause` | When |
|---|---|
| `start` | the chat is created. This is its first run |
| `clear` | the harness moves the chat to a new conversation in the same process: Claude Code's `SessionStart` with `source: clear` (ADR 0024, C6), or any report that the board *follows* onto a different conversation id |
| `reopen` | the app relaunches the chat and resumes its conversation (`reopen.json`'s `resume`) |
| `wake` | a hibernated chat is resumed (SC-4) |
| `fresh` | the chat is started again without its conversation: the harness could not find it (`lostOnResume`), or a workspace rename dropped it (`renamed_from`) |
| `switch` | an attribute changes while the chat carries on: another harness or profile (FR-28's "continue on"), a model a hook reports, a persona adopted mid-chat (the adopting command tells the host on the hook channel, as `charter session record` does; today it only writes `.charter/sessions/<n>.persona`), or a sandbox backend turned on or off |

Two things that look like causes are not:

- **Compaction** (`SessionStart` with `source: compact`) keeps the run. The conversation is the
  same one.
- **A harness naming its conversation for the first time** keeps the run. Codex and opencode name
  theirs only in the first turn's hook (ADR 0024, ADR 0058). The run gains its `conversation`;
  it does not start again. A *change* to a different id is `clear`.

A harness nested in the chat's shell (ADR 0024, C5) is not a run of this chat. The board refuses
its reports as it does today.

**This defines W8's "agent run".** The run is the principal an action is attributed to. Budgets
and the kill switch attach to the **chat** and sum over its runs (OV-6, FD-18). Audit's `actor`
is the run (AU-1). A harness switch is a new run in the **same** chat, so the chat's budget, audit
history and work-item link (FW-5) survive it. That is P1-07's point: the thing that survives
switching vendors is the chat.

### A child agent is a run with a parent run

**A sub-agent or teammate the harness spawns is a child run.** It has its own ULID, `parent_run`
set to the run it came from, and the parent's attributes, except its own `conversation`.

It is recognised from the hook payload's `agent_id`, **only on a harness where that field was
measured** to mean a sub-agent (`handoffguard::MEASURED_SUBAGENT_HARNESSES`: Claude Code and
Codex). The host maps `(chat, agent_id)` to the child run the first time it sees that pair, and
the run ends at the matching `SubagentStop`. opencode reports no such field, so an opencode chat
has no child runs, and its sub-agents' work is attributed to the parent run until a measurement
says otherwise.

A payload says which agent it came from, not which agent started that one. So every child run's
parent is the top-level run that was current when it appeared. A grandchild is recorded as a
child of the top-level run, until a harness reports nesting itself.

### Lineage is by id

**A link from one chat to another holds the other chat's id.** Four links are named:

| Link | Set when | Held on |
|---|---|---|
| `handed_from` | a handoff opens this chat (charter-app#258) | the new chat |
| `resumed_from` | **Resume** starts this chat from a session record (ADR 0064) | the new chat, taken from the record's `chat-id` |
| `forked_from` | a race (N1) or a fork starts this chat from another | the new chat |
| `moved_from` | RR-4 moves a chat between devices | the chat on its new device |

Each is set when the chat is created and never changed. A name drawn from a link, such as
`↳ from steward 3 · ops`, is a **copy** kept for display, because the other chat may be closed or
on another machine. It is never what the link is followed by. `forked_from` and `moved_from` are
reserved here and written by the features that create them.

The handoff pairing a report is checked against stays on the **number**. It is local: a report
goes back to a chat open on this machine, under `chat-<n>/`. The id is added beside the number,
so the link still resolves once the report is gone or is read somewhere else.

### A device is a random id, and the local principal never leaves the machine

**Each device gets a device id: a ULID minted at the first launch that finds none, and kept in
the machine store** (`machine.rs`, `$CHARTER_CONFIG_HOME/charter/machine.json`). A device is any
machine charter runs on: a desktop, a runner (V9) or, later, a viewer. Each has its own machine
store and so its own id.

**It is random, not derived from the device key.** AU-3's key rotates, and a rotation must not
change which device wrote what. A key rotation keeps the device id and writes a genesis entry
naming it (AU-3). Key backends (the keyring, then Secure Enclave or TPM, then an age-encrypted
file on a headless host) are AU-3's decision. FD-25 needs only the id, so FD-25 does not wait
for the audit chain. The hostname becomes a label that is shown, never a key.

**This amends [ADR 0034](0034-charter-keeps-a-little-state-outside-every-plane.md): the device
id is a new fact in the machine store.** It passes ADR 0034's test. It is about the machine, and
it is false inside any one plane, since one plane is opened on many devices. Deleting it costs
what that record allows, and no plane loses anything. The next launch mints a new id, and the
machine reads as a new device from then on. Records it wrote before still name the old id, which
is true: they came from the old one.

**The local principal is `local:<device>/<os-user>`**: the device id, and the login name the
operating system gives for the process's uid (never `$USER`). It is derived each time and never
stored, so it cannot disagree with its parts. A git identity may be shown next to it as a label,
never as part of it. **Agents act as `agent:<persona>/<run>`** (`none` for a chat with no
persona) on behalf of the local principal.

**What "never leaves the machine without an account" means.** Charter never sends the principal,
or the device id, to any service by itself. The principal is written only to machine-tier stores:
the event log and the audit chain, in the app data directory. It is never written into a plane.
**The device id does go into the plane**, in session records and, after FD-25, in the names of
per-host logs, because a plane the operator pushes is the operator's own act of publishing. The
id is a random pseudonym and says less about the machine than the hostname those logs already
publish. An export the operator runs (AU-12, the free SIEM export in W9) carries both, because
carrying them is what the export is for. When an account exists, an `account.link` audit event
binds the local principal to the account's principal, so no earlier entry is rewritten (P5-06).

**Human and agent principals never mix (V16).** A line on the hook channel produces an agent
actor, whatever it claims. The local principal acts only through the operator's own client scope.
The per-chat token (#535) is what makes the hook side of that true today: a hook line is admitted
only for the chat whose token it carries, and FD-6 generalises it.

### Every event carries one envelope, and a client subscribes from a cursor

**Every event the host records carries this envelope:**

```json
{"v": 1, "device_id": "<ULID>", "seq": 4182, "ulid": "<ULID>", "chat": "<ULID>",
 "run": "<ULID>|null", "parent_run": "<ULID>|null", "kind": "run.started", "body": {}}
```

| Field | Meaning |
|---|---|
| `device_id` | the device whose host wrote the event |
| `seq` | that device's sequence number: a `u64`, starting at 1, one higher for each event, and never reused. It is assigned by the single writer on the device (the app today, `charterd` after FD-5). A `charter` command reaches that writer through the hook channel and never numbers an event itself |
| `ulid` | the event's own id, unique everywhere. Its time part is the event's time, so there is no separate timestamp field |
| `chat` | the chat the event is about. It is set on every event about a chat, including those with no run, such as the chat being created or closed |
| `run` | the run the event is about, or `null` for an event about the chat as a whole |
| `parent_run` | the run's parent, for a child run's event, so a reader groups a child under its parent without a lookup |
| `kind` | what happened, as a dotted name (`chat.opened`, `run.started`, `run.ended`, `hook.pretooluse`). FD-9 lists them |
| `v` | the version of this `kind`'s `body`. The envelope's own shape changes only through a new record |

**`chat` is the one field added to V1's list.** FD-9's row already requires "the chat, run and
device ids" on every event. An event about a chat with no run would otherwise name its chat only
in its body, and every consumer that groups by chat would need a run-to-chat map before it could
read the stream.

**`subscribe(since)` is how a client reads events.** `since` is the last `seq` the client holds
for that device. The host sends every later event in order, then keeps the stream open. A cursor
older than what the host still keeps gets a fresh snapshot and a marker saying events were
missed, never a silent gap. With more than one device (a runner, later the relay), the cursor is a
map from device id to `seq`. The session protocol (V7, LV-2a) carries the call. FD-24 states the
retention and holds the acceptance test: kill `charterd` mid-turn, and a client that resubscribes
from its cursor misses nothing and duplicates nothing. The event log is machine state, in the app
data directory, never in a plane.

Audit entries (AU-4) and OTel records cite the event's `ulid`, so the three can be joined, and
a crash between an event and its audit entry can be found (P5-05).

### OpenTelemetry

| Attribute | Where | Value |
|---|---|---|
| `charter.chat.id` | every span and log record about a chat | the chat's id |
| `charter.run.id` | every span and log record about a run | the run's id |
| `charter.run.parent_id` | a child run's spans | its `parent_run` |
| `charter.event.id` | a span or log record derived from an event | the event's `ulid` |
| `host.id` | resource | the device id, in OTel's own attribute for a unique host id |
| `gen_ai.conversation.id` | GenAI spans | the harness's conversation id, **only where the harness supplied one**. The GenAI conventions say never to make one up, so a charter id never goes here |

**The local principal is never an OTel attribute.** `enduser.id` is not set. An OTel exporter
sends to a collector outside charter's control, and ruling V1 keeps the principal on the machine.
OTel carries the run instead, and the audit chain carries whose behalf the run acted on. The
display number is not exported either, following the rule above.

### The audit schema builds on this and defines no ids of its own

AU-1 lands after this record, and takes these from it:

- `actor` is the run, as `agent:<persona>/<run>`, which AU-18 may turn into a keyed pseudonym,
  or the local principal for an act the operator did in the window;
- `on_behalf_of` is the local principal until an account exists;
- `device` is the device id;
- `target` names chats (and work items, FW-5) by id.

AU-1 decides how these are stored, retained and exported. It does not mint an id, and it does
not name a chat by its number.

## What changes where

The code does not change with this record. Each row is what the implementation of FD-22, and the
tickets after it, change:

| Code as it is | What changes |
|---|---|
| `reopen.rs`: `Chat::number`, `Record::dealt` | Kept as they are: the display number, dealt as today. `Chat` gains `id` (the ULID), `device` (the origin device), `run` (the current run's id) and `resumed_from`. The record stays `version: 1`, for `number`'s and `pinned`'s reason: an absent id reads as "mint one at this launch", which is what every chat before this was. `reopen::identity_of(root, number)` answers a chat's id and current run the way `conversation_of` answers its conversation |
| `reopen.rs`: `HandedFrom { chat, name, workspace, report }` | Gains `id`, the parent's chat id. `chat` stays the key reports are left under on this machine, and `name` stays a display copy |
| `handedFrom.ts` | Draws the same sentence. The link it names is followed by id wherever a parent is looked up, and never by name |
| `sessionrecord.rs`: `ChatFacts` and `render` | The frontmatter gains `chat-id`, `device` (the chat's origin device, so the pair is V1's "ULID plus its origin device"), `run` (the run that wrote the record), `handed-from` and `resumed-from`, each always written, as `unknown` or `none` when not known. `ChatFacts` reads them from `reopen.json`, as it reads `conversation`. `chat:` stays as the display number. The record never gains a principal or a hostname |
| `machine.rs`: "Five things, and nothing else" | Gains the device id, with its creation time. ADR 0034 is amended (above). Where the store refuses (not unix, ADR 0031), there is no device id: chats still get ids, and `device` reads `unknown` |
| `hookwire.rs` and the per-chat token (#535) | **No change to the line.** A hook still carries the chat number and that chat's token, and nothing more. The host maps the number to the chat's id and current run in its own memory, and decides run boundaries from the facts it already reads (`SessionStart` source, the conversation it adopts or follows, `agent_id`). A hook never names a run. It cannot know one, because its environment was fixed at the `exec` and a run can change after it. So a chat also cannot claim another run as its own. FD-6 re-keys the token map on the chat id when `charterd` holds several planes' chats |
| `dispatch::host` and the `<host>` logs | Unchanged here. FD-25 re-keys per-host logs on the device id, with the hostname as a label, and migrates them through FR-9. The save branch's `<host>` is a name the operator reads and stays one |
| `state.rs` | Unchanged here. FD-23 gives runs their states, and derives the chat's from them |

## Migration

- **The device id is minted at the first launch that finds none.** No prompt, and nothing to
  migrate.
- **An open chat with no id gets one at the first launch that reads it**, as `number` was dealt
  to chats recorded before it existed. Its first run after the upgrade has `cause: reopen`. Its
  earlier history has no run ids, because no events were kept.
- **Session records already in a plane are never rewritten.** They are committed history, often
  in other people's clones, and a rewrite would conflict with every one of them. The critique's
  `legacy-<n>-<host>` cannot be computed anyway, since those records name no host. A reader that
  needs a key for a record without `chat-id` uses the record's plane-relative path, which is
  already unique and is how `sessionrecord::locate` names a record. Nothing links to such a
  record by chat id, and **Resume** from one leaves `resumed_from` as `unknown`.
- **A handoff recorded without `from.id`** keeps working by number, as it does today. Its lineage
  reads as `unknown`.
- **No recorded behaviour moves.** The Python charter never wrote session records or
  `reopen.json`, so `tests/fixtures/recorded/behaviour.jsonl` has no scenario that these keys
  change.

## What this costs

- **Two ids for one chat.** The number is what people say, and the id is what files and events
  say. The rule "the number never leaves the clone" is what keeps the two apart, and every new
  field has to be placed by it.
- **A downgrade loses identity.** An older app that rewrites `reopen.json` drops `id` and `run`,
  since it does not know the keys, and the next newer launch mints new ids for the same open
  chats. Records written before the downgrade keep the old ids. Bumping the version to prevent
  this would drop every operator's open chats at the upgrade, which costs more.
- **A deleted machine store makes the machine a new device.** Intended, and stated above.
- **opencode chats have no child runs** until a sub-agent field is measured on it.
- **A run is not a conversation.** Adopting a persona in the middle of a conversation starts a
  new run in the same conversation. That is the price of fixed attributes: an audit entry's
  persona is the one the run had, with no need to consult a timeline.

## What was rejected

- **Making the number unique across devices** (a device prefix, a shared counter in the plane).
  A counter in git conflicts on every concurrent chat, a prefix makes `steward 3` unreadable, and
  both keep a user-facing label doing an id's job.
- **Using the harness's conversation id as the chat's or the run's id.** It moves on `/clear`,
  Codex and opencode name it only in the first turn, a nested harness can report someone else's,
  and it belongs to a vendor. The OTel conventions forbid inventing one, and this is the mirror
  rule: charter does not borrow one either.
- **Deriving the device id from the device key.** The key rotates, and the id must not. It would
  also make FD-25 wait for AU-3.
- **The OS user, the git email or the hostname as the principal.** Each is shared, changes, or
  leaks more than it identifies. The OS user is in the principal only after the device id,
  which is what makes the principal unique.
- **A run id in the chat's environment.** A process's environment is fixed at the `exec`, and a
  run changes under it on `/clear`. Every child process would carry a stale id.
- **Chat and run lifecycle states in this record.** They are FD-23's, which lands after this one
  and can only be written once a run exists.

## Ruled (V21, 2026-09-30)

The three calls that went beyond the words of V1 were each ruled yes:

1. **`chat` is in the envelope**, beside V1's seven fields: chat-level events have no run.
2. **The device id is committed in session records** as a random pseudonym; the principal never
   is. "Never leaves the machine" means charter never sends it to a service.
3. **A mid-chat persona adoption or model switch starts a new run**, and `charter persona use`
   tells the host on the hook channel.

## ADR 0066, amended

**Settled by V43** (operator, 2026-10-02): *"a copied project's chats get new ids, and a moved
project keeps them. This amends ADR 0066 ("a chat's id is minted once" becomes "once per clone
and device"). `app/reopen.json` records the clone-key (ADR 0079) and the device id. On a
mismatch, charter looks at the recorded old root: if that root still exists and holds the same
ids, the project was copied and every chat's id and device is minted again; otherwise it was
moved, so the ids are kept and `clone` is rewritten. A same-path copy to another machine is
caught by the device id. Chat-link writers stay off until this lands."*

**Why.** `.charter/` is gitignored, so a `git clone` never carries the record. But `cp -R`, rsync
and a backup restore do, and with it every open chat's id. Two clones then hold one chat, and
everything keyed on the id merges them: the event log, session records, and ADR 0088's chat
links, whose log is `merge=union` (#732, from the #856 review).

**What it means.** "Minted once when the chat is created" now reads **minted once per clone and
device**. The rule *"the number never leaves the clone, and the id is what leaves"* is unchanged.
What changes is that a clone no longer trusts an id it did not mint:

- **The record says who wrote it.** `app/reopen.json` gains `clone`: the clone-key (ADR 0079 §8:
  the first 16 hex characters of the SHA-256 of the clone's canonical root), that root, and the
  device id. The app stamps it onto every write.
- **A launch compares it before any chat starts** (`reopen::arrive`):

  | The record says | And | It is | The ids |
  |---|---|---|---|
  | no `clone` | | a record from before V43 | kept; `clone` adopts this clone |
  | another device | | a copy, even at the same path | minted again, on this device |
  | this clone and device | | the same clone | kept |
  | another clone-key | the old root's record holds one of these chats' ids | a copy | minted again, on this device |
  | another clone-key | the old root is gone, or holds none of them | a move | kept; `clone` is rewritten |

  A device unknown on either side (ADR 0031) decides nothing, and the clone-key does. One shared
  id is enough to call it a copy: the original may have opened or closed other chats since.
- **A copy's chats keep everything else**: their numbers, conversations, names and
  `resumed_from`. Only `id`, `device` and `run` are the original's. `run` is cleared, and the
  start that puts the chat back begins the copy's first run.

**What it cannot tell.** A copy whose original was deleted, or sits on a volume that is not
mounted, reads as a move and keeps its ids. That is accepted. No live clone holds those ids then,
so nothing merges from that launch on. Records the original wrote before it went still name
them, and they name the same chat's history up to the copy, which is true.

**What it does not do.** It turns no chat-link writer on. ADR 0088 §7 keeps them off, and this
amendment only makes the ids they will write trustworthy. It does not bump the record's
`version`: an absent `clone` reads as a record from before V43, for `pinned`'s reason.
