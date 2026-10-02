# A work item's identity is its tracker key, and the project records its links

**Accepted 2026-10-02** by the operator (ruling V40), for program-map ticket FW-5 (#732), whose
implementer stopped because no record fixed the commitments below. Its concept is **Workspace**: work items are views of a
workspace, not a sixth concept. It follows these of the operator's rulings:

- **V3:** *"One domain type, **Work item**, with trackers as backends (forges per FI4, Linear/Jira
  extensions, and the plane's todos as the local tracker). A chat links to zero or one work item;
  budgets, the board (OV-8/FI8), FI9 and AC-7 hang off that link. The five on-screen nouns stay
  (X22, FI5)."*
- **FI4:** *"One neutral work model mapped per forge: WorkItem (issue, epic, sub-issue), Milestone,
  Iteration, Board/Project (a view), Label, Assignee, Link (closing PR/MR, blocked-by,
  parent/child). GitHub: Issues, sub-issues, issue types, Milestones, Projects v2, dependencies.
  GitLab: Issues, Epics/Iterations (Premium), Milestones, Boards, work items. Forgejo: Issues,
  Milestones, Projects. Missing features are capability flags shown on the capability card (W10).
  Linear and Jira plug into the same model as extensions (FG-9, PE-18)."*
- **FI5:** *"No new top-level concept: work items, milestones and boards are views in a
  Workspace's "Work" section (and a Project's, for items spanning workspaces). X22 holds."*
- **FI6:** *"Workspace membership in three layers: (1) every issue in a workspace's repos shows
  automatically; (2) items created from charter carry a label (`charter::ws::<name>` scoped on
  GitLab, `ws:<name>` on GitHub; prefix configurable), on by default for private repos and off for
  public ones; (3) the plane records the link (workspace ↔ item, chat ↔ item), surviving label
  renames and repos where labels can't be created. Charter never bulk-relabels existing issues."*
- **FI7:** *"A derived, rebuildable SQLite cache of forge items in app data (never the plane:
  private issues must not land in git; never a second source of truth, OV-8's rule). Offline
  reads; writes go straight to the forge; freshness by conditional-request polling under a
  per-account budget; webhooks later through the relay after GT-CLOUD."*
- **FI9**, in part: *"Issues drive agents: "Start a chat on this" (persona, harness, a branch named
  after the issue); the chat, branch and PR carry the link and the PR closes the issue"*.
- **X22**, in part: *"The five core concepts are **Project, Workspace, Chat, Persona, Memory**."*
- **V21 (b)**, in part: *"the random device id is committed to session records in the plane, while
  the local principal never leaves machine-level stores"*.

It builds on [ADR 0066](0066-a-chat-is-a-ulid-a-run-is-a-stretch-of-its-conversation-and-a-device-is-random.md)
(chat ULIDs and the device id), [ADR 0069](0069-every-store-charter-writes-is-in-one-of-four-tiers.md)
(storage tiers), [ADR 0070](0070-a-forge-is-one-seam-with-a-native-client-per-forge-and-gh-and-glab-are-its-fallback.md)
(the forge seam and `ForgeRef`) and [ADR 0075](0075-an-audit-entry-is-metadata-in-a-store-of-its-own-and-telemetry-never-reads-it.md)
(the audit's `target`). It amends ADR 0066, ADR 0069 and ADR 0070, each in a section of its own
below. FW-6a/b, FW-7, FW-9, FW-11a/b, OV-6 and AC-7 build on it.

## Where charter is today

Two accepted records already name a work item, and they do not say the same thing:

- **ADR 0075's** audit `target` names *"work items by the tracker's key (FW-5)"*.
- **ADR 0066's** audit section says *"`target` names chats (and work items, FW-5) by id"*.

Neither says what the key or the id is. ADR 0070 §1 adds that a forge's own identifier (a GitHub
node id, a GitLab global id or `iid`) *"travel[s] inside a neutral type as an opaque `ForgeRef`"*.

**There is no work-item type and no link.** A todo is a memory file under
`workspaces/<ws>/todos/`, named `<YYYYMMDD-HHMMSS>-<slug>.md`, with a title and a body and **no
fields at all**: closing one with `ws todo done` journals `Closed todo: <title>` in the workspace's
memory and deletes the file (`docs/plane-format.md`, *`workspaces/<ws>/todos/`*). A chat records
nothing about what it works on. A chat's ULID lives in the host's memory for one launch: FD-9
mints it, and #834 makes it survive a relaunch.

**The two per-device logs a workspace has today are never committed.** The piece claim log
(`pieces/<host>.jsonl`) and the landing log (`changes/log/<host>.jsonl`) are Clone state, keyed by
hostname until FD-25 (#662) re-keys them on the device id. Both are append-only JSON Lines with a
closed key set, and both have a `merge=union` line in the project's `.gitattributes` block.

## The decision

**A work item is named by its tracker key: the tracker's name, a colon, and a locator that the
tracker's own references already use. The key is derived from the item, so two devices name the
same item the same way with no coordination. A forge's own id travels beside the key in the
neutral type and in the item cache, never inside the key and never in the project. A workspace's
links to work items, and its chats' links, are an append-only log per device under
`workspaces/<ws>/work/`, holding keys and chat ULIDs only. Promoting a todo to an issue closes the
todo and writes an alias from the todo's key to the issue's, so every link on the todo now reaches
the issue.**

### 1. The tracker key

```
key      = tracker ":" locator
tracker  = [a-z][a-z0-9-]*
```

| Tracker | Locator | Example |
|---|---|---|
| `github` | `<host>/<owner>/<repo>#<number>` | `github:github.com/owner/repo#12` |
| `gitlab`, an issue or task | `<host>/<namespace path>/<repo>#<iid>` | `gitlab:gitlab.com/group/sub/repo#12` |
| `gitlab`, an epic | `<host>/<group path>&<iid>` | `gitlab:gitlab.com/group&3` |
| `forgejo` (reserved until FG-13) | `<host>/<owner>/<repo>#<number>` | `forgejo:codeberg.org/owner/repo#12` |
| `todo` | `<workspace>/<file stem>` | `todo:smart-ide/20261002-081200-port-the-picker` |
| an extension's | the extension's own grammar | `linear:<org>/<TEAM-123>` (illustrative) |

- **The tracker names the backend kind, and the locator names the instance.** `github` and
  `github.com` are not redundant: `github:ghe.example.com/…` is a GitHub Enterprise Server item,
  and the backend that answers it is the GitHub one. A GitLab repo's namespace path may hold any
  number of groups; the separator is the forge's own sigil, `#` for an issue and `&` for an epic,
  neither of which a path can contain.
- **The locator is the reference the forge itself prints**, fully qualified with its host. A
  person can read a key off an audit export or a log line and find the item.
- **Normalised once, compared exactly.** The host is lowercased, with no scheme, and keeps a port
  only when it is not the default. The owner, namespace and repo are spelled as the forge's API
  returns them, not as they were typed; a number has no leading zeros. Two keys are the same item
  when their bytes are equal.
- **A todo's locator is its workspace and its file stem**, not its slug or its title. The stem
  is unique in its directory, it is what `ws todo done` already resolves a typed slug to, and two
  todos may share a title.
- **An extension tracker declares its prefix** in its manifest. `github`, `gitlab`, `forgejo` and
  `todo` are reserved, and a second extension declaring a prefix one already holds is refused at
  install. The extension owns its locator's grammar, under three rules: one line, no whitespace,
  and the item's own human reference (Linear's `TEAM-123`, Jira's `KEY-123`) qualified by the
  site it lives on.
- **`ForgeRef` travels beside the key, never inside it.** The neutral `WorkItem` carries both.
  The item cache (FI7) keeps the `ForgeRef`, so a round trip never re-derives it (ADR 0070 §1).
  The project never holds a `ForgeRef`.

ADR 0075's `target` entry for a work item is `{type: "work_item", id: <key>}`.

### 2. A key that moves leaves an alias

A key names where an item is, so it can go stale: a repo is renamed or transferred, an issue is
moved to another repo, a workspace is renamed. **The old key is never rewritten. An alias line
(§3) maps it to the new one, and every reader resolves a key through its aliases to the end.**

- **`moved`:** written when the item cache (FW-7) finds an item it holds by its `ForgeRef` under a
  new locator. Both forges say where a moved issue went: GitHub redirects the old path, and GitLab's
  API names the issue it was moved to.
- **`renamed`:** written by `workspace rename`, one line for each `todo:` key of the renamed
  workspace that any line of its log names.
- **`promoted`:** §5.

An audit entry written before a move keeps the old key, which was true when it was written; an
export resolves it through the project's aliases if it wants the current one. An alias that would
close a cycle is refused when it is written. A cycle that a merge creates stops resolution at the
first repeated key, and `doctor` reports it.

### 3. The work link log

**`workspaces/<ws>/work/<device>.jsonl`**, where `<device>` is this device's id (ADR 0066), is
one append-only JSON Lines file per device per workspace. Each line has a closed key set:

| Line | Keys | Meaning |
|---|---|---|
| workspace link | `v`, `ts`, `op: "link"`, `item` | the workspace holds this item (FI6 layer 3) |
| chat link | `v`, `ts`, `op: "link"`, `item`, `chat` | this chat works on this item |
| unlink | `v`, `ts`, `op: "unlink"`, `item`, and `chat` for a chat's | either link ends |
| alias | `v`, `ts`, `op: "alias"`, `from`, `to`, `cause` | `from` now resolves to `to`; `cause` is `promoted`, `moved` or `renamed` |

`v` is `1`, `ts` is UTC ISO-8601 seconds, `item`, `from` and `to` are tracker keys, and `chat` is a
chat's ULID. A line with any other key set, or any other `op` or `cause`, is skipped and counted in
`doctor`, as the piece claim log skips one.

**What a reader folds.** Every file in `work/`, sorted by `ts`, then by file name, then by line.

- **A chat's link is the last chat link or unlink for that chat across every workspace's log**,
  because a chat can change workspace and keeps its link (V3: zero or one). Linking a chat that is
  already linked replaces its link.
- **A workspace's items are its workspace links**, plus every item a chat link in its log names.
  A workspace link for an item FI6 layer 1 already shows is allowed and changes nothing on screen.
- **A key is read through its aliases** before anything is compared, so a chat linked to a todo
  that was promoted is linked to the issue.

**Keys only, never content.** A line holds no title, body, label, assignee, state or `ForgeRef`,
and never the local principal or the OS user (ADR 0066). Who wrote a line is the audit's to record
(ADR 0075), not this log's. The item's content stays in the forge and in the machine-tier cache,
as FI7 requires.

**Tier: Plane when LIVE, Clone state when LOCAL**, the tier of the todos it sits beside. A LIVE
workspace's log is committed with it, and a LOCAL one's stays in this clone and is backed up as
clone state. **This departs from the piece claim log and the landing log**, which are Clone state
even for a LIVE workspace. Those record what one clone did. This one records what FI6 layer 3
says *"the plane records"*: links that must reach the operator's other devices and survive a new
clone. So:

- the LIVE block gains `!/workspaces/<n>/work` and `!/workspaces/<n>/work/**`, and a LIVE
  workspace's staged paths gain `work`;
- the `.gitattributes` block gains `workspaces/*/work/*.jsonl merge=union`. One writer per device
  means two devices never write the same file. Two clones of one project on one machine share a
  device id and do, and every line they append is whole, so the union driver keeps both sides.
- `workspace fork` does not copy `work/`. A fork's chats and todos are new, and the source's links
  stay the source's.

**What a committed log publishes, to whoever can read the project's remote:** which items a
workspace and its chats work on, as tracker, host, repo path and number; chat ULIDs and device ids,
which session records already carry (V21); and the times links were made. For an item in a private
repo, that says the repo and an item number exist. It does not say anything the item contains.

### 4. A chat in no workspace has no link yet

A chat at the project root is *"in no workspace on purpose"* (`CONTEXT.md`, **Plane root**), and
FI5 puts work items in a workspace's Work section. So in FW-5 a project-root chat cannot be linked,
and the window does not offer it. FI5's project-level Work section, for items spanning workspaces,
reads every workspace's log and needs no store of its own. A project-level log is a later record's
to add if a ruling asks for project-root chats to link.

### 5. Promoting a todo to an issue

**`charter ws todo promote <slug> --repo <repo>`**, and **Promote to issue** on a todo's row in the
Work list, which runs the same core function.

1. **The slug resolves as `ws todo done` resolves one**: an exact stem, else the one file whose
   name ends `-<slug>.md`, refusing a slug more than one file ends in.
2. **`--repo` names a repo of the workspace by its inventory name**, and is optional when the
   workspace has exactly one repo on a forge. A repo whose forge cannot create an issue for this
   account is refused, by its capability flag and the account's rights (FI14), before anything is
   sent.
3. **The issue is created** through the forge seam's `WorkItems` area, as the signed-in human
   (FI3), with the todo's title as its title and its text, without the stamp line, as its body. It
   carries FI6's layer-2 label where that is on for the repo. The write is audited with its
   `Caller` (ADR 0070).
4. **The alias line is appended** to this device's log: `{op: "alias", from: <todo key>, to:
   <issue key>, cause: "promoted"}`.
5. **The todo is closed as `ws todo done` closes one**, except that the journal line reads
   `Promoted todo: <title> → <issue key>`, so the workspace's memory says where it went.

**The Work list shows one item.** A todo whose key has an alias is not listed, and a chat linked
to it shows under the issue. That is FW-5's acceptance: a todo promoted to a GitHub issue keeps its
chat link, and the board shows one card.

**What it costs to stop half-way.** A crash after step 4 leaves the todo file on disk with an
alias: readers already treat it as the issue, and the next `ws todo` or `doctor` finishes the
close. A crash between steps 3 and 4 leaves an issue nobody linked and the todo still open, and a
second promote makes a second issue. The command prints the issue's key the moment it exists, so
the operator can close the duplicate.

**What it publishes.** Promoting sends the todo's title and text to the forge, where the repo's
readers can see them, and a public repo's readers are everyone. The command and the window name
the repo, and whether it is public, before they send. A todo was project content until then.

**The CLI word.** `promote` joins `done` and `forget` as a verb `ws todo` reads by the shape of the
call: two positionals act on a todo, one records one. `ws todo promote` with no slug is refused, as
`ws todo done` is, rather than recording a todo called "promote". `--repo` is accepted only with
`promote`. Promoting to an extension's tracker is a later flag (`--tracker`), when FG-9 or PE-18
gives one a create.

### 6. What FW-5 builds, and what it leaves

- **FW-5 builds** the neutral `WorkItem` and `TrackerKey` types, the key's parser and normaliser,
  the work link log with its fold and its aliases, `todo` as a tracker backend, and promote.
- **FI4's item-to-item Link is a relation.** A closing PR or MR, blocked-by, and parent/child are
  **relations** between work items in the neutral model, and FW-6a/b map them. "Work link" is
  only what §3's log records, and "Link" alone stays the runner link's word (ADR 0078).
- **The `WorkItems` area trait arrives with FW-5**, with one method, `create`, for both forges,
  because ADR 0070 §1 adds an area trait *"in the same PR as the first ticket that needs it, with
  both forges' implementations"*, and promote is that ticket. FW-6a and FW-6b map the rest of FI4
  onto it.
- **The typed `RepoRecord` is split out of FW-5** into a ticket of its own,
  [#857](https://github.com/diazoxide/charter/issues/857) (§ *ADR 0070, amended*). A key needs only a repo's host and path, which FW-5 reads from the inventory row it
  already has. `RepoRecord` is the `Repos` area's return type, which every inventory caller moves
  to. That is FG-3's area, not the work model's, and FW-5 already blocks seven tickets.
- **`charter report` (#806) moves with FW-6a (#733)**, as ADR 0070's amendment 2 says. It files on
  charter's own tracker and writes no link, so FW-5 does not touch it.

### 7. What waits on #834

**A chat link names a chat by its ULID, and until #834 a chat's ULID lasts one launch.** A link
written today would name an id the chat no longer has after a relaunch. So **#834 blocks the chat
half of FW-5**: the window and the hooks write no chat link line until a reopened chat keeps its
id. The key, the log, workspace links, aliases and promote do not wait. The chat half of FW-5's
acceptance test runs once #834 has landed.

FD-25 (#662) is not a blocker. This log is keyed on the device id from its first line, so it has
no hostname era to migrate.

## ADR 0066, amended

Its audit section's *"`target` names chats (and work items, FW-5) by id"* now reads:
**`target` names chats by id, and work items by their tracker key (ADR 0088).** That is
what ADR 0075 already says, and the two records then agree.

## ADR 0069, amended

The inventory, *Every store, by tier*, gains one row. Row 85 is ADR 0085's, so this one is 86:

| # | Store | Tier | Sync | Backed up | Rebuildable |
|---|---|---|---|---|---|
| 86 | `workspaces/<ws>/work/<device>.jsonl`, the work link log (ADR 0088) | Plane when LIVE, Clone state when LOCAL | — | remote, or yes | no |

It is not rebuildable: the links are the operator's and the agents' decisions, and nothing else
records them. The FI7 cache it points into stays row 70, Machine and rebuildable.

## ADR 0070, amended

Its FG-3 amendment 5 ends *"FW-5 defines the typed record."* It now reads: **#857, split out of
FW-5 for it, defines the typed record (ADR 0088 §6).** Section 1's
`WorkItems` area gains its first method, `create`, in FW-5.

## What changes where

The code does not change with this record.

| Where | Change |
|---|---|
| `docs/plane-format.md` | An entry for `workspaces/<ws>/work/<device>.jsonl`, **accepted, not yet written** (in this PR). The LIVE block, the staged paths and the `.gitattributes` block change in FW-5's PR, which writes the log |
| `CONTEXT.md` | Gains **Tracker key**, **Work link** and **Relation** (in this PR). "Link" alone stays the runner link's word; FI4's item-to-item Link (closing PR, blocked-by, parent/child) is called a **relation** in the neutral model, so the three never share a word |
| ADR 0066, ADR 0069, ADR 0070 | Amended above. Their texts are left as they are, and this record is the amendment |
| FW-5 (#732) | §6: the types, the key, the log, `todo` as a tracker, promote, and `WorkItems::create` for both forges. Blocked by #834 for its chat half only (§7), recorded on the issue |
| #857 (new) | The typed `RepoRecord` for the `Repos` area (§6) |
| FW-6a (#733), FW-6b (#734) | The rest of FI4 on `WorkItems`; #806 moves with FW-6a |
| FW-7 (#735) | The cache keeps each item's `ForgeRef` beside its key, and writes `moved` aliases |
| `charter workspace rename`, `fork` | `rename` writes `renamed` aliases; `fork` does not copy `work/` |

## What this costs

- **A key changes when its item moves**, and every reader resolves aliases before it compares.
  A move the cache never sees, because nobody opened the item after it moved, leaves a link on the
  old key until somebody does.
- **A committed log grows forever.** Links and aliases are rare (a few lines per chat), so a year of
  a busy workspace is a few thousand lines. Compaction is not designed here.
- **Clocks decide order across devices.** Two devices that link the same chat within their clock
  skew may fold in the wrong order. A chat runs on one device at a time, so this needs two
  operators, or one operator on two machines, acting on one chat in the same second.
- **A promote can leave a duplicate issue** (§5).
- **A project-root chat cannot be linked** (§4).

## What was rejected

- **The `ForgeRef` as the identity.** It is opaque, differs between a forge's REST and GraphQL
  answers, cannot name a todo or an extension's item, and means nothing to a person reading an audit
  export.
- **A ULID charter mints per item.** Two devices would mint two ids for one issue, and the mapping
  from forge item to ULID would become primary state that FI7's rebuildable cache could not hold.
- **The link in the todo's file, or in `workspace.json`.** A todo has no fields, a forge item has no
  file, and `workspace.json` is rewritten whole, so two devices' links would conflict on every save.
- **The link in the item cache.** The cache is device-bound and rebuildable, and FI7 forbids it to
  be a source of truth; FI6 says the project records the link.
- **The link as a forge label or comment.** That is FI6's layer 2, and layer 3 exists because
  labels are renamed and some repos cannot have them.
- **One shared log per workspace.** Every device would append to one file, and two devices' saves
  would conflict on it, where a file per device conflicts only in the one-machine case above.
- **The implementer's line shapes**, `{chat, item}` and `{promoted, to}`, as they were. They become
  `op` lines with a closed key set, as the piece claim log's are, so an alias can also carry
  `moved` and `renamed`, and an unlink is a line, not an absence.

## Erratum (implementation, FW-5 #861)

Recorded by FW-5's implementer and not a change to the decision. §3's fold order, *"sorted by
`ts`, then by file name, then by line"*, leaves one tie: two workspaces' logs share a file name,
because each is named by the same device id, so two lines with the same `ts` and the same line
number in the same-named file of two workspaces are not ordered by it. The fold keeps §3's order
and breaks that tie, and only that tie, by the workspace's name.

## Ruled (V40, 2026-10-02)

Settled by V40: *"ADR 0088 is accepted with its four recommendations."* Each question this
record put to the operator, with its decision:

1. **The work-item key.** Decided as recommended. V40 (a): *"A work item's key is
   `<tracker>:<locator>`. Examples: `github:github.com/owner/repo#12`,
   `gitlab:gitlab.com/group/repo#12`, `gitlab:gitlab.com/group&3` for an epic, and
   `todo:<ws>/<file stem>` for a todo. The host is lowercased and keys are compared exactly.
   Extensions declare their own prefix. `ForgeRef` travels beside the key and never goes into the
   project. A moved key gets an alias line."* Applied in §1 and §2. Rejected: the `ForgeRef` as
   the identity, and a ULID charter mints.
2. **Where links are stored.** Decided as recommended. V40 (b): *"Links go in an append-only log
   per device at `workspaces/<ws>/work/<device>.jsonl`. It holds keys and chat ULIDs only, uses
   `merge=union`, is committed when the workspace is LIVE and is clone state when it is LOCAL."*
   Applied in §3. Rejected: Clone state always.
3. **Promote, and its word.** Decided as recommended. V40 (c): *"`charter ws todo promote <slug>
   --repo <repo>` creates the issue (after saying which repo and whether it is public), writes the
   alias, and closes the todo as `done` does."* Applied in §5.
4. **The typed `RepoRecord`.** Decided as recommended. V40 (d): *"The typed `RepoRecord` gets its
   own ticket."* That ticket is [#857](https://github.com/diazoxide/charter/issues/857), and ADR
   0070's amendment 5 names it (§6).
5. **A chat at the project root.** V40 does not rule it. FW-5 follows the recommendation, that a
   project-root chat cannot be linked (§4), until a ruling asks otherwise.

Two things were stated, not asked, and stand: `charter report` (#806) moves with FW-6a (#733) as
ADR 0070's amendment 2 rules, and #834 blocks FW-5's chat half only (§7), as #732 now records.
