# Search runs on derived SQLite indexes, one per project clone and one per machine

**Accepted 2026-10-01** by the operator (ruling V33), with dispatcher decisions D-0079a/b/c, drafted for program-map ticket KN-1 (#714). It follows these of the
operator's rulings:

- **Q12:** *"A derived SQLite FTS index under `.charter/`, rebuildable, never the truth."*
- **V2:** *"An ADR names four tiers: Plane (committed), Clone state (`.charter/`, per clone, not
  derived: vault registry, `fingerprint.key`, `reopen.json`, save/push journals, gate files),
  Machine (app data), Keyring."* Applied by
  [ADR 0069](0069-every-store-charter-writes-is-in-one-of-four-tiers.md), which amended OQ-10 to
  read: *"`.charter/` is clone state. It holds per-clone state that is not derived, which FR-10
  backs up, beside derived indexes marked rebuildable and transient files marked transient."*
- **V4:** *"Memory has an owner (workspace, persona, shared, me) and an audience (this machine, me
  on all my machines, team); approval follows the audience, closing the path where team-visible
  LIVE memory or session records reach a teammate's briefing unreviewed (amends X36/KN-21)."*
- **V8**, in part: *"An idle-CPU budget; polls driven by the file watcher and paused while windows
  are hidden."*
- **V11**, in part: *"a local redacted **transcript archive** in app data and cross-harness
  **transcript search** in the palette (★)"*.
- **V18:** *"The 73 findings of the map consistency audit are applied as written in its proposed
  fixes, since each follows from an existing ruling; where a finding and a ruling disagree, the
  ruling wins."* Finding C-15 is one of them. It split Q12's one index into two: *"a per-clone
  plane index under `.charter/` (Clone state, derived) and a machine index in app data for
  transcripts (Machine tier, FR-30)"*.
- **X47**, accepted with the consistency review as recommended: *"Does the local audit log live
  only in the app data dir, with `.charter/` reserved for derived, rebuildable indexes (Q12)?"*
- **V22a**, in part: *"one host per OS user per device serving every plane"*. **V22b**, in part:
  *"`$CHARTER_HOME` moves all clone state or none (#750)"*.
- **V25a**, which defines charter's data home: *"`$CHARTER_DATA_HOME`, else
  `$XDG_DATA_HOME/charter`, else the OS data dir's `charter/`"*.

It also takes the rules of KN-1's own row in the program map, which came from the phase-2
critique's fix for P3-21: *"one writer (`charterd`); rebuildable, never the truth; incremental by
pulled range and watcher paths; a background full rebuild ≤ 10 s at 50k; stale-while-refreshing;
WAL"*. No ruling quotes those numbers. They are the filed spec. The budgets of KN-2, KN-22 and
KN-32 are quoted from their rows in the same way.

It builds on [ADR 0066](0066-a-chat-is-a-ulid-a-run-is-a-stretch-of-its-conversation-and-a-device-is-random.md)
(chat and run ids), [ADR 0067](0067-a-chat-runs-in-a-sandbox-charter-compiles-for-its-harness.md)
(the chat sandbox), [ADR 0068](0068-a-chat-lives-in-charterd-and-the-app-is-its-client.md)
(`charterd`), ADR 0069 (storage tiers), [ADR 0072](0072-charter-has-five-concepts-and-every-other-word-belongs-to-one-of-them.md)
(the five concepts) and [ADR 0075](0075-an-audit-entry-is-metadata-in-a-store-of-its-own-and-telemetry-never-reads-it.md)
(`<data>`). It amends ADR 0067, ADR 0068 and ADR 0069, each in a section of its own below. KN-2
(project search and `recall` on the index), KN-9, KN-22 (scale budgets), KN-32 (transcript
search), KN-33 and HP-17 build on it.

Its concept is **Memory**: memory, session records and the transcript archive are Memory's parts,
and recall's results and the briefing are its views (ADR 0072 §2).

**On words.** Ruling V23b retires "plane" for "project" everywhere, and ADR 0072 asks every new
record to say project. This one does. File paths, code names and `docs/plane-format.md` keep
their current names until the rename lands, so `<plane>` in a path is the project's root.

## Where charter is today

- **There is no index and no database.** `Cargo.lock` has no SQLite, redb or sled. `charter
  recall` reads every entry of every memory base and keyword-scans it per query
  (`crates/charter-core/src/recall.rs`, through `memstore::read_files`). A chat's briefing reads
  its store's entries the same way (`briefing.rs`), and so do the memory duplicate checks. Track 02
  of the research counts about eight such reads per project change, and calls them a felt hitch at
  thousands of memories. `memory/index.md` and `sessions/index.md` are lists for people, not
  search indexes.
- **Reading a memory is gated.** `memstore::read_files` refuses an entry that leaves the project,
  such as a committed `leak.md` symlinked to a file outside it. `recall` never opens a memory any
  other way. An index must not become a way around that gate.
- **The watcher does not see memory.** `planewatch` watches the project root, `workspaces/`, each
  workspace, its `todos/`, `personas/` and `.claude/`, each without recursion. `memory/` and
  `sessions/` are not among them, so a memory an agent writes reaches no panel until some other
  event arrives.
- **There is no transcript archive.** A chat's conversation lives only in its harness's own
  store. KN-31 adds a redacted archive in the Machine tier. KN-32's search reads it.
- **Charter already has a secret scanner.** `secretshape` finds credential shapes in text and
  answers with the kind, never the value. `charter save` refuses a memory or ref it flags, and the
  commit scan (ADR 0074) runs it over every line a chat's commit adds.

Three questions have no answer yet. Where does each index live, given ADR 0069's tiers? Who
writes it, when several processes read the same files? And how does an index over transcripts,
which can hold secrets, avoid becoming a second copy of them?

## The decision

**Search runs on two kinds of derived SQLite FTS5 index. Each project clone has one, in its
`.charter/`, over the project's own files. Each machine has one, in `<data>`, over the transcript
archive only. `charterd` is the only writer of both. Neither is ever the truth: deleting one costs
a rebuild and nothing else, and every reader has a correct answer without it. An index holds the
words of what it indexes, after redaction and the scanner, and never their text as written. It
never answers a reader with anything that reader could not have read from the files.**

### 1. Two indexes, and each one's sources

**Settled by Q12, V2 and V18** (finding C-15): a per-clone index under `.charter/` that is Clone
state and derived, and a machine index for transcripts in the Machine tier. **Decided by D-0079a**
(see "Decided (dispatcher)"): the machine index covers the transcript archive only.

| | The project index | The machine index |
|---|---|---|
| One per | project clone | device |
| Where | `<plane>/.charter/index/<clone-key>/` (§8) | `<data>/index/` (§8) |
| Tier | Clone state, rebuildable | Machine, device-bound, rebuildable |
| Sources | the project's memory bases (workspace, persona, shared, refs), session records and todos, including a LOCAL workspace's | the transcript archive (KN-31), and nothing else |
| Serves | `recall`, the briefing, the memory tab, KN-2's project search | KN-32's transcript search in the palette |

**An index lives in the tier of its sources, and never mixes sources from two tiers.** The rule
decides both columns:

- **The project index holds only what is already in this clone**, committed or not. It takes no
  source from the Machine tier, so nothing private to the operator is copied into a directory one
  `git add` from the project. That is the reason ADR 0069 §4 gives for keeping private memory out
  of `.charter/`, and it holds for a copy of that memory too.
- **The machine index holds transcripts only** (D-0079a). Transcripts come from a harness on this
  device, and the archive that holds them is Machine.
- **Owner and audience are two different things, and only the owner "me" has a home today.** V4
  gives memory an owner (workspace, persona, shared, me) and, separately, an audience (this machine,
  me on all my machines, team). Memory **owned** by "me" lives in a personal overlay project (V4),
  and a clone of that project has its own project index like any other. Memory whose **audience**
  is "this machine", KN-7's private scope, which KN-28 turns into an audience setting, is kept in
  the Machine tier (ADR 0069 §4). This record puts it in neither index: never in the project
  index, by the rule above, and not in the machine index, by D-0079a. Until KN-28 says otherwise,
  `recall` reads it by scanning, as it reads every memory today.
- **The session's ephemeral scratch** (`.charter/persona-state/ephemeral/`) is not indexed. It is
  transient, it is not in `recall`'s default scopes, and `recall` reads it directly when asked.

**The palette asks both kinds of index and shows the hits grouped by kind:** chats, memory,
records, todos. It does not merge them into one ranked list, because a BM25 score depends on the
corpus it was computed in, and scores from two indexes do not compare. A transcript hit opens the
chat, or Resume at that point, by its chat id, run id and turn (ADR 0066). A memory, record or
todo hit opens the item. Cross-project search (KN-11) asks each open project's index in the same
way.

### 2. Never the truth

**Settled by Q12:** rebuildable, never the truth.

- **Deleting an index is always safe.** The next writer rebuilds it from its sources. FR-10 does
  not back it up, since ADR 0069 §2 skips anything rebuildable.
- **Every reader has a correct answer without the index.** With no project index, `recall` and
  the briefing scan the files as they do today. That is slow at scale but correct. With no
  machine index, transcript search says it is building, and answers from what is indexed so far
  with a count ("412 of 1,000 chats indexed").
- **A schema change is a rebuild, never a migration.** Each index records its schema version, the
  version of the redaction rules it was built with (§6), and the source position it reflects. A
  writer that finds a version it did not write discards the index and builds a new one.
- **One test holds it**, as the phase-2 critique's P5-09 asked. Deleting every SQLite file charter
  keeps and restarting gives the same search, recall and briefing answers on a fixture project.
  The same test covers FI7's forge cache when that lands. KN-2 holds it.

### 3. One writer: `charterd`

**Settled by the KN-1 row:** one writer, `charterd`. V22a makes it one host per OS user per
device, serving every project, so there is one writer per index without further coordination.

- **`charterd` builds and updates both kinds of index, on its own device.** A runner's host
  (ADR 0078) builds the indexes of the clones and the archive on the runner. A desktop never opens a
  runner's index files. How a desktop searches a runner's chats is the runner tickets' to decide.
- **`charterd` is the writer.** It already owns `planewatch` and the
  event log (ADR 0068 §1), so it sees the changes the index needs.
- **Every other process only reads.** The app's panels, the hooks (the SessionStart briefing), the
  CLI and the palette open the index read-only (`SQLITE_OPEN_READONLY`, `query_only`). ADR 0068
  keeps project reads in the app, and this does not change that: the app reads the index
  directly, as it reads the files.
- **With no host running, the CLI reads and never writes.** If the index is there it uses it,
  even if it is behind (§5), and if it is missing it scans the files. It never builds one. This is
  P3-21's fix: *"the CLI reads only when the host is absent"*.
- **The writer holds a lock.** `charterd` takes an advisory `flock` on the index's `writer.lock`
  before writing and keeps it while it runs. A second writer, such as a transient host during an
  upgrade handoff (ADR 0068 §7) or two runner versions (ADR 0068 §8), waits for it and does not
  write alongside.

### 4. Keeping it fresh

**Settled by the KN-1 row:** incremental by pulled range and watcher paths, and a background full
rebuild of at most 10 s at 50k. **Settled by V8:** *"polls driven by the file watcher and paused
while windows are hidden"*.

Each indexed item keeps its path, a content hash, and the file's size and modification time. The
project index also keeps the commit it last reconciled against.

- **After a pull, a save or a branch change,** the writer asks git for the paths that changed
  between the commit it last saw and the new one (`git diff --name-only`), limited to the indexed
  directories, and re-indexes only those. If the old commit is no longer an ancestor, after a
  rebase or a reset, it reconciles by stat sweep (below).
- **Between commits, the watcher's paths.** `planewatch` gains `memory/` and `sessions/` in each
  workspace, `personas/<name>/memory/` and `personas/_shared/memory/`, still without recursion.
  Each batch it reports re-indexes the files it names whose size, time or hash changed.
- **At start, a stat sweep.** The writer compares every indexed directory's files with what it
  recorded, and re-indexes what differs. At 50,000 files that is 50,000 `stat` calls, well under
  a second (an estimate). Only a changed stat leads to a read.
- **The machine index follows the archive.** `charterd` writes the archive at a turn's end and at
  close (KN-31), and indexes the new turns in the same pass. Archive retention and erasure delete
  the rows in the same act that deletes the chat (§6).
- **A full rebuild runs in the background.** It is used when there is no index, when a version
  does not match (§2), and when the operator asks for one (KN-2 names the command). It runs at low priority,
  pauses while no window is showing (V8: paused while windows are hidden), and never blocks a
  hook.

**The budgets.** KN-22 puts each of them in `stress.yml` on macOS and Linux (V8).

| What | Budget | From |
|---|---|---|
| A full rebuild of the project index at 50,000 memories and records | ≤ 10 s, in the background | KN-1 |
| A project search | under 100 ms on a large project | KN-2 |
| `recall` at 50,000 | p95 ≤ 100 ms | KN-22 |
| The SessionStart hook at 50,000 | p95 ≤ 50 ms | KN-22 |
| A transcript search over 1,000 archived chats | under 200 ms | KN-32 |
| A full rebuild of the machine index | resumable; ≤ 60 s per 1,000 archived chats, an initial value KN-22 measures | D-0079c |

### 5. Stale while refreshing

**Settled by the KN-1 row:** stale-while-refreshing, and WAL.

- **WAL mode**, so readers never wait for the writer and the writer never waits for readers. An
  incremental update is one transaction. A reader sees the index as it was before it or after it,
  never in between.
- **A full rebuild writes a new generation beside the live one.** The writer builds it in a new
  directory, closes it (which checkpoints its WAL and removes it), and then replaces the `current`
  file, which names the live generation, in one atomic rename. Readers open the generation that
  `current` names. A reader that opened the old one keeps its open file and finishes its query.
  The writer deletes the old generation at its next start, or once a minute has passed.
  Renaming a new database over the live file would be wrong: a WAL-mode reader opening the new
  file could pick up the old file's `-wal`.
- **An answer says when it may be behind.** While the writer has work queued, or a rebuild is
  running, each answer carries `refreshing` and the time its index reflects. The memory tab and
  the palette show a small "refreshing" marker, and the answer is served anyway.

### 6. The index is never a second copy of a secret

Transcripts can hold secrets: a value a command printed, a token pasted into a prompt, customer
data in a tool's output. An index over them must not become a second place those live, and one
that outlives the source.

- **The machine index reads only the archive, and never a harness's own transcript files.** The
  archive is redacted before it is written (KN-31, with LW-8a's redaction: vault values, the
  shape rules and path excludes). So the index is built from text that has already been redacted.
- **Every text is scanned again before it is indexed.** Before tokenising, the writer runs each
  text, from either kind of index, through charter's existing shape scanner (`secretshape`), with
  the rules a project save uses. A span it flags is replaced by its kind, such as `[JWT]`. Neither
  the value nor a masked head of it reaches the index. For the project index this catches a
  secret that reached the files without `charter save`, such as a hand edit or a teammate's
  commit. For the machine index it is a second check after LW-8a.
- **The index keeps terms and their positions, not the text as written.** Both kinds use FTS5
  contentless tables (`content=''`, `contentless_delete=1`) at `detail=full`. They hold each term
  with the row, column and position it occurs at, and the row metadata. That is enough to
  reconstruct much of a text's wording, so the index is treated as holding the words of what it
  indexes, after redaction and the scanner, and is guarded like its source. It holds no
  formatting and is not a readable copy. `detail=full` is kept because phrase and `NEAR` queries
  need positions, and finding something said in a chat is what transcript search is for.
  `detail=column` would lose phrase and `NEAR` queries, and `detail=none` would also lose
  searching within one column, such as titles. A result's snippet is read from the source at
  query time, for the top hits only, and masked with the same scan.
- **What deleting a source guarantees.** These are all the guarantees:
  - **At once:** its rows go in the same pass that sees the deletion, as when a memory is deleted
    or the archive drops a chat by retention or erasure. From then on no query returns them.
    In FTS5 that delete is a tombstone. The terms stay in the index's segments until a merge
    drops them.
  - **Off the database file:** the writer runs FTS5's `optimize`, which merges the segments and
    drops tombstoned entries, and then `wal_checkpoint(TRUNCATE)`. `secure_delete` is on, so
    SQLite zeroes the pages it frees, in the database file and not only in its WAL. The writer runs
    this immediately after an erasure: an archive erasure or retention drop, or a re-redaction
    (below). After any other deletion it runs within a day, in an idle period.
  - **Not promised:** blocks the filesystem kept elsewhere. An old generation that a rebuild
    unlinks (§5) is not overwritten. Copy-on-write filesystems, SSD wear levelling and filesystem
    snapshots can also keep earlier blocks of the live file. So **erasure never relies on a
    rebuild**. It is done in place, in the live generation, as above. Copies outside charter's
    reach are a matter for full-disk encryption, which is the operator's to turn on.
- **When redaction changes, the index follows.** If LW-8a re-redacts archived chats, for example
  after a vault value is added that an older chat contained, the archive names the chats it
  rewrote, and the writer re-indexes those chats and deletes their old rows. If the rules
  themselves change version, the recorded rules version no longer matches, and the whole index is
  rebuilt (§2).
- **What remains.** A secret that neither LW-8a nor the scanner recognises is in the index as a
  term, as it is in the source. The index adds no reach to it: it is in the same tier as its
  source, `0600` in a `0700` directory, and chats are denied the machine index (ADR 0067, amended
  below). It does not claim to catch what the scanner cannot.

### 7. An index never widens what a reader may see

An index is a faster way to answer a question, not a way to answer a different one.

- **The writer reads project files only through `memstore::read_files`,** the same gate `recall`
  uses. A committed symlink out of the project is refused there, and so it is never indexed.
- **Every row keeps the fields that decide who may see it:** its owner (workspace, persona,
  shared, me), its audience and approval state (V4), and its workspace and persona. Every query
  names the caller's reach, and the filter runs in the query. For each caller the index answers
  the same set of items the file scan does. KN-2's tests compare the two on a fixture project.
- **What reaches a chat is only what is approved for it.** **Settled by V4:** approval follows the
  audience. `recall` and the briefing, which put memory into a chat, serve only memory approved
  for that audience. The operator's own palette may show a pending item, labelled as pending.
- **A chat searches through the host, and only its own clone.** A chat reaches search through
  `charterd` with the `chat` scope's search capability (ADR 0068, amended below), and is answered
  from its own clone only:
  - **The project index:** the host opens only the index under the chat's own `<clone-key>`, which
    it computes from the clone the chat runs in and never takes from the request. Under a shared
    `$CHARTER_HOME` the state directory holds several clones' indexes, and a chat is never
    answered from another one.
  - **The machine index:** every row carries the chat's id, run and turn (ADR 0066) and the
    `<clone-key>` of the clone the chat ran in. The host filters on that key. A chat therefore sees
    the archived transcripts of chats from its own clone, and never those of another project or
    clone. The clone is the unit because a project has no machine-wide id yet. That is stricter
    than "the same project", and never looser.

### 8. The stores

`docs/plane-format.md` records each of these now with its tier, marked **decided, not yet
written**. A store names its tier before it ships (ADR 0069, ruling 10).

| Store | Tier | Backed up (FR-10) |
|---|---|---|
| `.charter/index/<clone-key>/`: the `current` file and each generation's `search.sqlite` with its `-wal` and `-shm` | Clone state, rebuildable | no |
| `.charter/index/<clone-key>/writer.lock` | Clone state, transient | no |
| `<data>/index/`: the `current` file and each generation's `search.sqlite` with its `-wal` and `-shm` | Machine, device-bound, rebuildable | no |
| `<data>/index/writer.lock` | Machine, device-bound, transient | no |

- **The project index is under the clone's state directory, keyed by clone.** **Settled by
  V22b:** `$CHARTER_HOME` moves all clone state or none, and the index moves with the rest.
  Because a shared `$CHARTER_HOME` can serve several clones, and each clone can be at a different
  commit, the index sits under `<clone-key>`: the first 16 hex characters of the SHA-256 of the
  clone's canonical root path. Two clones never share an index, and a chat is answered only from
  its own (§7). A moved clone builds a new one, and the writer deletes a `<clone-key>` directory
  whose clone no longer exists when it starts.
- **The machine index is in `<data>`**, charter's data home (V25a). **Decided by D-0079b:** in
  `<data>`, beside the archive, not in the OS cache. The archive's home is KN-31's to decide, and
  this record does not decide it. KN-31's row and V11 say *"app data"*. ADR 0069 names the Tauri
  application-data directory `<app data>`, and that directory is not `<data>`. If KN-31 puts the
  archive in `<app data>`, the index still sits in `<data>` under D-0079b, and "beside" means
  "beside the archive's KN-31 location" in the same Machine tier, not in the same directory. That
  mismatch is KN-31's to resolve, not this record's. Either way the index is in the tier its
  source is in. It is device-bound because it describes this device's archive, and
  it is never synced: what could follow the operator to another machine is the archive, not an
  index built from it. The audit (ADR 0075) is not one of its sources.
- **Modes.** Directories are `0700` and files `0600`, as for every other file under `.charter/`
  and `<data>`.

### 9. SQLite itself

- **`rusqlite` with its bundled SQLite.** FTS5, `contentless_delete` (SQLite 3.43 or later) and a
  known version on every platform matter more than using the system library, whose version
  differs between macOS releases and Linux distributions. FI7's forge cache uses the same
  dependency.
- **FTS5 with `unicode61 remove_diacritics 2`** as the tokeniser, and BM25 ranking with a title
  weighted above the body. A trigram table for substring search is left to KN-2, if its labelled
  queries need it.
- **One file per index generation**, holding the item table, the FTS table and a `meta` table
  (the schema version, the rules version, the source position, the time built).

## ADR 0067, amended

§5 lists what a chat's sandbox always denies, as classes. Two additions:

- **Class 2, charter's integrity state, gains both kinds of index, for writing.** A chat that
  could write an index could change what `recall`, the briefing and the palette tell the next
  chat. `charterd` is their only writer (§3).
- **Chats may not read the machine index or the transcript archive either. Ruled by V33.** The machine index describes every project's chats on this machine,
  so reading it would let a chat in one project search another project's conversations. The
  host's `chat`-scoped search (§7, and ADR 0068, amended) would be the chat's way in. The project
  index may still be read, since it holds nothing the chat could not read from its project's
  files, but it is never written. This would be the first read denial about one project reaching
  another, rather than about secrets or integrity. **The archive has no ADR yet.** Denying it here
  pre-empts KN-31, as V33 accepts.

## ADR 0068, amended

- **§5, the `chat` scope gains one capability: search, limited to its own clone.** A connection on
  the `chat` scope may ask the host to search. The host answers from the project index under the
  chat's own `<clone-key>` and from machine-index rows carrying that key (§7). It derives the key
  from the chat's identity and never from the request. The capability reads and never writes.
  It reaches no other clone, project or chat's archive, and it gives a chat no human power.
- **§1's table gains a row.** *Moves into `charterd`:* **the derived search indexes, as their only
  writer (ADR 0079)**, because one writer per device is what makes a WAL index safe without
  cross-process coordination, and `charterd` already owns the watcher that drives it.
- **"Everything else stays in the app" is unchanged.** The app reads the index read-only, as it
  reads the project's files.
- **`planewatch` gains the memory and session directories** that §4 names, still without
  recursion.

## ADR 0069, amended

- **§6, the stores that records in flight add, gains two rows:** the project index,
  `.charter/index/<clone-key>/`, **Clone state, rebuildable**, and the machine index,
  `<data>/index/`, **Machine, device-bound, rebuildable**, each with its `writer.lock` marked
  transient (§8). With ADR 0075's audit and ADR 0078's runner bare repos, `<data>` now has three
  stores.
- **§4's reading of OQ-10 stands, and this record applies it.** *"Beside derived indexes marked
  rebuildable"* now has its first index. Memory whose audience is this machine stays out of
  `.charter/`, and so does any index of it (§1).

## What changes where

The code does not change with this record.

| Where | Change |
|---|---|
| `docs/plane-format.md` | An entry for `.charter/index/<clone-key>/` and one for its `writer.lock`, each **decided, not yet written**, with its tier; rows for `<data>/index/` and its `writer.lock` in the table of what charter-app keeps outside every project; the index added to the `CHARTER_HOME` row (in this PR) |
| `CONTEXT.md` | Gains **Search index** (in this PR) |
| ADR 0067, ADR 0068, ADR 0069 | Amended above. Their texts are left as they are, and this record is the amendment |
| KN-2 | The project index, the writer in `charterd`, `recall` and the briefing on it, the no-index scan kept as the fallback, the equal-answers test (§7) and the delete-and-restart test (§2) |
| KN-32 | The machine index over the archive, keyed by clone; the palette's grouped hits; the `chat` scope's search capability; its row amended by D-0079a |
| KN-31 | Names the chats it re-redacts, so the index can follow (§6); decides the archive's home, which this record does not (§8) |
| KN-22 | The budgets of §4 in `stress.yml` |
| FD-10 | `planewatch` gains the memory and session directories (§4) |
| SD-2 | The sandbox denials of ADR 0067, amended |
| FI7 | Shares the `rusqlite` dependency, and the delete-and-restart test |

## What this costs

- **A C dependency.** Bundled SQLite compiles C into the `charter` binary, which adds build time
  and a library to keep patched. It is the one charter takes on for search and the forge cache.
- **Snippets cost a read.** A contentless index keeps no text as written, so the top hits'
  snippets are read from their sources at query time. At 20 hits that is 20 small reads. The index
  gives up `snippet()` and `highlight()` in return for not holding a readable copy of the prose.
- **Erasure costs a merge.** Removing deleted terms from the file takes an FTS5 `optimize` and a
  checkpoint (§6). That is seconds at 50,000 items, and it runs after every erasure.
- **Positions are kept.** `detail=full` holds enough to reconstruct much of a text's wording.
  The index is guarded as its source is, rather than treated as harmless.
- **Two indexes answer one search box.** The palette groups by kind rather than ranking across
  them, so the best memory and the best chat sit in separate groups.
- **Disk.** 50,000 memories of 1 to 2 KB each is 50 to 100 MB of text, and an FTS5 index of it is
  a fraction of that. During a rebuild two generations exist at once. These are estimates, and
  KN-22 measures them.
- **Masking costs recall.** A false positive of the shape scanner, such as `password: String` in a
  code sample, is masked in the index, so a search for that exact text does not find it.
- **The CLI can be behind.** With no host running, the CLI reads the index as the host last left
  it, marked as possibly behind. It never writes one.

## What was rejected

- **One index for everything, in `.charter/`.** Q12's original shape. Transcripts and
  machine-only memory would sit in a project directory, in a tier that travels further than their own. C-15
  split it, and the phase-2 critique's P2-17 said why: transcripts can hold secrets and customer
  data.
- **One index for everything, in `<data>`.** It would put each project's index outside the clone
  it describes. A project's index then has to know every clone on the machine, and a backup of
  the clone could not rely on the index being next to the files.
- **Tantivy instead of SQLite.** Stronger ranking, but one more storage engine next to FI7's
  SQLite, and no transactional reader and writer model like WAL's. Q12 names SQLite.
- **`detail=column` or `detail=none`.** Smaller and less reconstructable, but it gives up phrase
  and `NEAR` queries, and with `none` searching within a column too. §6 states what `detail=full`
  costs instead.
- **Erasure by rebuild.** A rebuild unlinks the old file and does not overwrite it (§6), so it
  is not erasure.
- **Storing the text in the index (FTS5 with content).** Faster snippets, but a full copy of every
  transcript turn outside the archive, with its own retention and its own deletion to get right.
- **Every process writes the index.** The app, the CLI and the hooks all touch the files, and
  letting each one update the index would bring back the `index.lock` contention Track 02 found in
  git polling. A hook would also pay for a write inside its 50 ms.
- **Migrating an index across schema versions.** A derived store has nothing to preserve that a
  rebuild does not restore, and a migration is code that runs once per user and is rarely tested.
- **Indexing the harness's own transcript files directly.** They are not redacted, they are in
  the harness's format and location, and a harness can move or delete them. The archive is the
  one source, and it is already redacted.

## Decided (dispatcher)

The dispatcher decided the three other questions of this record's first draft, recorded in
DECISIONS.md as D-0079:

- **D-0079a:** *"KN-32's map row is amended to C-15: the machine index covers the transcript
  archive only, and the palette searches both indexes."* Applied in §1.
- **D-0079b:** *"The machine index lives in `<data>`, beside the archive, not in the OS cache."*
  Applied in §8, which says what "beside" means while KN-31 has not placed the archive.
- **D-0079c:** *"Rebuild budget: resumable, starting at ≤60 s per 1,000 archived chats; KN-22
  measures it."* Applied in §4's budgets.

## Ruled (V33, 2026-10-01)

The operator accepted the one question as recommended:

1. **V33: a chat reaches transcripts only through a `chat`-scope search, limited to its own
   `<clone-key>`.** The host derives the key from the chat and never from the request (ADR 0068,
   amended). **ADR 0067 §5 gains a read denial of the machine index and the transcript archive**,
   the first denial about one project reaching another. That denial pre-empts KN-31 for the
   archive.
