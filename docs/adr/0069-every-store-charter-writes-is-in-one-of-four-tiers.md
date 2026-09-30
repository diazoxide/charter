# Every store charter writes is in one of four tiers

**Accepted 2026-09-30** by the operator (ruling V22b), drafted for program-map ticket FR-30
(#623). It follows the operator's ruling **V2** (phase-2 critique): *"An ADR names four tiers: Plane (committed), Clone
state (`.charter/`, per clone, not derived: vault registry, `fingerprint.key`, `reopen.json`,
save/push journals, gate files), Machine (app data), Keyring. Every file in plane-format.md gets a
tier. OQ-10 is amended; FR-10's backup includes clone state."*

It amends **OQ-10** and [ADR 0034](0034-charter-keeps-a-little-state-outside-every-plane.md)
(the machine store: see its amendment of 2026-09-30). It gives a tier to the stores that three
records add: [ADR 0066](0066-a-chat-is-a-ulid-a-run-is-a-stretch-of-its-conversation-and-a-device-is-random.md)
(chat, run and device identity),
[ADR 0067](0067-a-chat-runs-in-a-sandbox-charter-compiles-for-its-harness.md) (the chat sandbox)
and [ADR 0068](0068-a-chat-lives-in-charterd-and-the-app-is-its-client.md) (`charterd`), all
accepted. FR-10 (#608, backups) is built on it. So are LW-27, KN-26, KN-28, KN-29, KN-31
(#717) and FW-7 (#735), which each add a store.

## Where charter is today

charter keeps its files in three places, and none of them has a name that says what it is for.

- **The plane's git.** `charter.toml`, personas, a LIVE workspace, `vaults.json`. The rule
  charter was built on is that the plane is the state.
- **`.charter/` and the other files git ignores.** `docs/plane-format.md` calls `.charter/`
  "runtime state". OQ-10 ruled that it *"holds only derived, rebuildable indexes"*. The code
  says otherwise. The vault registry, `fingerprint.key`, `reopen.json`, the save and push
  journals, the profile and MCP consent records and the rename journal all live there. None of
  them can be rebuilt. Deleting `fingerprint.key` changes every `fp:` value charter has shown,
  and deleting `harness-profiles-launched.json` makes every profile ask for consent again.
- **Outside every plane.** ADR 0034 allowed a machine store for facts about the operator or the
  machine. It holds `machine.json`, and it now also holds `layout.json`, `theme.json`,
  `extensions.json`, the plugin copy and the restart marker. Beside it are the app's OS
  directories (the shell-tab shims, the git hooks, the panic log), the lines charter writes into Claude Code's,
  Codex's and opencode's global config, and two kinds of keyring item.

Three questions have no answer until each store has a tier. What does a backup carry (FR-10)?
What may follow the operator to a second machine? What does deleting a file cost? OQ-10's
answer to the third question was wrong, and a backup built on it would have skipped the vault
registry.

## The decision

### 1. Four tiers

| Tier | Where it is | What it means |
|---|---|---|
| **Plane** | committed to the plane's git | It travels to every clone, and the remote is its backup. |
| **Clone state** | in the plane directory, never committed: `<plane>/.charter/`, `workspaces/<ws>/.charter/`, `charter.local.toml`, a LOCAL workspace's files, the generated harness layer, charter's block in a clone's `.git/info/exclude` | Per clone. Not derived, unless it is marked rebuildable. FR-10 backs it up. |
| **Machine** | outside every plane: the machine store (below), the app's OS directories, and charter's lines in a harness's global config | About the operator or this machine, true in no single plane. Each store is **syncable** or **device-bound**. |
| **Keyring** | the operating system's credential store: the macOS Keychain, the Secret Service | Secret values and key material. Nothing else holds a value, except a plain-file vault. FR-10 never copies it. |

**The Machine tier's home is ADR 0034's machine store, which is not the OS application-data
directory.** V2 says "Machine (app data)", and ADR 0034 put the store under the application-data
directory. `machine.rs` then moved it to the config home (`$CHARTER_CONFIG_HOME`, else
`$XDG_CONFIG_HOME`, else `~/.config`, then `charter/`), so that a machine has one `charter/`
directory. ADR 0034 is amended to say so plainly. This record calls the tier **Machine** and
says where each store in it is. It does
not move any store. The Tauri application-data and log directories are Machine too. They hold
only the shims and the git hooks (ADR 0074), which are rebuilt at every launch, and the panic
log.

A path `docs/plane-format.md` records that is **not charter's store** gets the tier **None**:
the operator's checkout and its worktrees, a harness's own files, a vendor CLI's output, an
extension's own folder. The tier says that FR-10 does not carry it and that charter is not the
one to rebuild it.

### 2. Marks

A tier is followed by the marks that apply to it:

- **syncable** or **device-bound**, on every Machine store and only there. **Syncable** means an
  operator preference that means the same thing on another machine, such as the window layout or
  the theme. **Device-bound** means true of this machine only: an absolute path, a consent given
  here, an identity, or a pointer into this machine's keyring.
- **rebuildable**: derived from something else. Deleting it costs a rebuild and nothing more.
  V2's "derived indexes" are these.
- **transient**: it lives for one session, one turn or one operation. Examples are a gate file,
  a per-session pointer, a report waiting for its hook, a lock or a socket.
- **legacy**: only the retired Python charter creates it. charter-app neither reads it nor
  writes it, or at most keeps it consistent across a rename.

**What FR-10 backs up follows from the tier and the marks.** It backs up every Clone state and
Machine store that is not rebuildable, transient or legacy, and nothing else. The Plane tier is
backed up by its remote. The Keyring tier is backed up by the operating system, or not at all.
There is one exception inside Clone state. A **plain-file vault holds secret values**, so FR-10
does not copy it, because FR-10 carries *"the vault registry (not secret values)"*.

**Gate files are Clone state and transient.** V2 lists them among the clone-state files that
are not derived, and that is right. They also expire with their session, so FR-10's list, which
does not name them, is right as well.

### 3. Every file in the plane format has a tier, and a test holds it there

Every store heading in `docs/plane-format.md` now carries a `**Tier:**` line, and every table
of paths has a Tier column. The document gains a section defining the tiers and marks, and a
table of what charter-app keeps outside the plane. That table did not exist, because the
document was written about the Python charter. Two entries are new: `.charter/app/hooks.sock`,
which the app binds and no document mentioned, and the keys that `charter plugin install`
writes into `~/.claude/settings.json`. The document said charter never writes that file, and
that is still true of the Python charter.

`crates/charter-core/tests/every_store_the_plane_format_names_has_a_tier.rs` reads the document's
file sections, from "Finding the plane" to the appendix, and fails when:

- a `###` or `####` heading has no tier and is not listed in the test as something other than
  a store (a key, a group, a rule), with the reason;
- a table whose first column is `Path` has no Tier column;
- a tier or mark is not one this record defines, including a tier line with stray punctuation;
- either boundary heading is missing, so that a renamed heading cannot make it pass while
  checking nothing.

**Naming the tier of a new store joins the definition of done** (ST9). It is written into
`AGENTS.md`'s rules, and the test enforces it for the document. The test cannot see a store that
code writes and no document mentions, and review is what catches that (see the rulings).

### 4. OQ-10, amended

OQ-10 said: *"`.charter/` holds only derived, rebuildable indexes. Review drafts (R5) and private
memory (KN-7) move to the app data dir."*

It now reads: **`.charter/` is clone state.** It holds per-clone state that is not derived,
which FR-10 backs up, beside derived indexes marked rebuildable and transient files marked
transient. Each file says which it is. **Review drafts (R5) and private memory (KN-7) still move
to the Machine tier**, as syncable stores that FR-10 backs up. They belong to the operator rather
than to a clone, and a private memory must never be one `git add` away from the plane. R5's
words *"in .charter/"* give way to this.

### 5. FR-10's backup, and restoring on another machine

FR-10 backs up clone state and the Machine stores that §2 selects, to a folder the operator
chooses. Two rules follow from the tiers:

- **A device-bound store is restored as the same device, or not at all.** When FR-10 restores
  onto a machine, it asks one question: **does this machine replace the one the backup came
  from?**
  - **Yes:** the device-bound stores come back as they were. That is the device id
    (ADR 0066), the approvals and recent planes in `machine.json`, `extensions.json`, the
    event log and the audit chain. So are the clone-state consent records:
    `harness-profiles-launched.json` and `mcp-approved.json`.
  - **No, or a machine that already has a device id:** none of that comes back as live
    state. The device id is minted fresh, as ADR 0066 does for a deleted store, so two
    machines never share one. Every approval and consent is dropped, because it was given on
    another machine, for absolute paths that may name something else here, and the operator
    is asked again the first time each one matters. The event log and audit chain are kept
    as the old device's records, under its id, and never appended to.

  Syncable stores (`layout.json`, `theme.json`, review drafts, private memory) come back
  either way.
- **A plain-file vault's registry entry comes back, and its file does not.** FR-10 carries no
  secret values (§2). A restore restores the entry in `.charter/vaults.json`, marks the vault
  **file missing**, and names it in the restore's summary, so the operator knows which values
  to put back. It never drops the entry silently. Its file may also be outside the plane
  (`charter vault add --file`), and it is Clone state only while it is inside `.charter/`.
- **A restore brings back what a clone held, not what a harness held.** `reopen.json` names each
  chat's harness conversation. That conversation lives in the harness's own store, which is not
  charter's. A restored chat whose conversation is not on the new machine starts fresh and says
  why, as a renamed workspace's chat already does.

### 6. The stores that records in flight add

These are decided and not yet written. Each gets its tier now, so the tickets that build them
start from it.

| Store | Record | Tier |
|---|---|---|
| the device id, in `machine.json` | ADR 0066 | Machine, device-bound |
| the event log (FD-9) | ADR 0066, 0068 | Machine, device-bound. It is backed up |
| the local audit chain (AU-1..AU-3) | ADR 0066, 0067 | Machine, device-bound. It is backed up, and `charter audit verify` passes after a restore (FR-10) |
| the device key (AU-3) | ADR 0066 | Keyring. On a headless host, V1's age-encrypted file stands in for it |
| the human scopes' credentials, `<config>/charterd/` | ADR 0068 | Machine, device-bound, transient. Minted at each start of `charterd` |
| `charterd.sock`, in `$XDG_RUNTIME_DIR/charter/` on Linux or the per-user `TMPDIR` on macOS, not in the machine store | ADR 0068 | Machine, device-bound, transient |
| the per-chat hook spool, beside the chat's files in `.charter/` | ADR 0068 | Clone state, transient. See the rulings |
| the run journal `charterd` resumes from (FD-29) | ADR 0068 | Machine, device-bound |
| `charterd`'s copy of itself for an AppImage, under the machine store | ADR 0068 | Machine, device-bound, rebuildable |
| a runner's host versions, under the machine store at `<config>/server/<ver>/` | ADR 0068 | Machine, device-bound, rebuildable |
| a runner's resident secret store (age-encrypted, V9) | V9 | Keyring, in its headless form |
| the forge-item cache (FI7) | FI7 | Machine, device-bound, rebuildable |
| review drafts (R5) and private memory (KN-7) | OQ-10 | Machine, syncable |

ADR 0067 adds no store of its own. Its denial classes are the set of stores a chat may not
touch, and they follow the tiers: the Keyring, the plain-file vaults, the audit chain, the device
key and `<config>/charterd/`. **The hook spool is the exception, and ADR 0068 is the rule for
it.** A chat's hooks may append to **that chat's own spool**, and never to another chat's. The
host verifies each line's MAC and sequence as it drains the spool, and records a gap in its own
spool as a gap. ADR 0067's wording, *"denies the hook spool"*, was corrected to the same rule in
ADR 0067 itself (V22).

## Every store, by tier

**This table is a snapshot, taken on 2026-09-30.** `docs/plane-format.md` is the record of each
store's tier, and it is the document the test checks. When the two disagree, the format document
is right, and this table is not updated.

Each row gives the store, its tier, whether it is syncable or device-bound, whether FR-10 backs
it up and whether it is rebuildable. The writer of each is in `docs/plane-format.md`. Paths are
relative to the plane root unless they start with `<config>` (the machine store), `~` or
`keyring`. **Backed up** means by FR-10: the Plane tier's backup is its remote.

| # | Store | Tier | Sync | Backed up | Rebuildable |
|---|---|---|---|---|---|
| 1 | `charter.toml` | Plane | — | remote | no |
| 2 | `.gitignore`, `.gitattributes` (including the LIVE block) | Plane | — | remote | no |
| 3 | `inventory/repos.json` | Plane | — | remote | no |
| 4 | `docs/topology.md`, the README roster block | Plane | — | remote | no |
| 5 | `vaults.json` (the shared registry, no values) | Plane | — | remote | no |
| 6 | `personas/<name>/**`, `personas/_shared/**` | Plane | — | remote | no |
| 7 | `personas/_dispatch/*.jsonl`, `personas/_skills/*.jsonl` | Plane | — | remote | no |
| 8 | `.claude/agents/<name>.md` | Plane | — | remote | yes |
| 9 | `<plane>/.claude/settings.json`, `<plane>/opencode.json` | Plane | — | remote | no |
| 10 | `workspaces/.gitkeep`, `workspaces/.default` | Plane | — | remote | no |
| 11 | `sessions/` (the plane root's session records) | Plane | — | remote | no |
| 12 | a workspace's `workspace.md`, `workspace.json`, `memory/`, `todos/`, `sessions/`, `changes/<slug>.json` | Plane when LIVE, Clone state when LOCAL | — | remote, or yes | no |
| 13 | `charter.local.toml` | Clone state | — | yes | no |
| 14 | `.charter/vaults.json` (the local registry) | Clone state | — | yes | no |
| 15 | `.charter/vaults/<name>.json`, a plain-file vault (Clone state only inside `.charter/`) | Clone state | — | **no: it holds values**. Its registry entry is restored and marked file missing (§5) | no |
| 16 | `.charter/vaults/<name>.json` (a reference vault), `<name>.meta.json` | Clone state | — | yes | no |
| 17 | `.charter/vaults/<name>.keys.json` | Clone state | — | yes | no |
| 18 | `.charter/fingerprint.key` | Clone state | — | yes | no |
| 19 | `.charter/app/reopen.json` | Clone state | — | yes | no |
| 20 | `.charter/plane-push.json` (the push journal) | Clone state | — | yes | no |
| 21 | `.charter/save-journal.jsonl`, `.charter/save-branch.json` | Clone state | — | yes | no |
| 22 | `.charter/harness-profiles-launched.json` | Clone state | — | yes | no |
| 23 | `.charter/mcp-approved.json` | Clone state | — | yes | no |
| 24 | `.charter/active-persona` | Clone state | — | yes | no |
| 25 | `workspaces/<ws>/refs/` | Clone state | — | yes | no |
| 26 | `workspaces/<ws>/changes/log/<host>.jsonl` | Clone state | — | yes | no |
| 27 | `workspaces/<ws>/pieces/<host>.jsonl` | Clone state | — | yes | no |
| 28 | `.charter-generated`, in a workspace or a clone | Clone state | — | yes | no |
| 29 | `<plane>/.claude/settings.local.json` | Clone state | — | yes | no |
| 30 | `.charter/workspace-rename.json` | Clone state, transient | — | no | no |
| 31 | `.charter/sessions/<sid>.{workspace,lock,persona,tools,gate,usage,memnudge}`, `.charter/terminals/*` | Clone state, transient | — | no | no |
| 32 | `.charter/sessions/<chat>.saved`, in the plane or a workspace | Clone state, transient | — | no | no |
| 33 | `.charter/handbacks/**` | Clone state, transient | — | no | no |
| 34 | `.charter/persona-state/ephemeral/**`, `persona-state/trace/*.jsonl` | Clone state, transient | — | no | no |
| 35 | `.charter/commit-gate/`, `dispatch-inflight/`, `ws-edit-nudge/`, `agent-personas.json` | Clone state, transient | — | no | no |
| 36 | `.charter/app/hooks.sock` | Clone state, transient | — | no | no |
| 37 | `workspaces/<ws>/pieces/seen/**` | Clone state, transient | — | no | no |
| 38 | `.charter/keyring-stub.json` (fenced test builds only) | Clone state, transient | — | no | no |
| 39 | `.charter/cache/glstate.json`, `glstate.refreshing` | Clone state, rebuildable | — | no | yes |
| 40 | `.charter/unrecorded/*.json` | Clone state, rebuildable | — | no | yes |
| 41 | `workspaces/<ws>/.charter-structure` | Clone state, rebuildable | — | no | yes |
| 42 | the generated harness layer: `workspaces/<ws>/.claude/settings.json`, a clone's `.claude/settings*.json`, mirrored plane paths | Clone state, rebuildable | — | no | yes |
| 43 | charter's block in `<clone>/.git/info/exclude`, and the git config charter sets | Clone state, rebuildable | — | no | yes |
| 44 | `.charter/frame/**` | Clone state, legacy | — | no | — |
| 45 | `.charter/reports/`, `chat-turns/`, `guard-seen.json`, `locks/`, `dispatch-commit.lock` | Clone state, legacy | — | no | — |
| 46 | `.charter/cache/{repostate,update,vaulthealth,harness-wiring}.json`, `cache/update-baseline` | Clone state, legacy | — | no | — |
| 47 | `.charter/sessions/*.{configver,ask-pending,route-pending}`, `active-workspace` | Clone state, legacy | — | no | — |
| 48 | `.charter/workspace-tab-order`, `workspace-arrivals/`, `ws-autosave/` | Clone state, legacy | — | no | — |
| 49 | `<config>/machine.json` | Machine | device-bound | yes (§5) | no |
| 50 | `<config>/machine.json.lock` | Machine, transient | device-bound | no | no |
| 51 | `<config>/layout.json` | Machine | syncable | yes | no |
| 52 | `<config>/theme.json` | Machine | syncable | yes | no |
| 53 | `<config>/restarted-to-update` | Machine, transient | device-bound | no | no |
| 54 | `<config>/extensions.json` | Machine | device-bound | yes | no |
| 55 | `<config>/plugin/` | Machine, rebuildable | device-bound | no | yes |
| 56 | `<app data>/shims/` | Machine, rebuildable | device-bound | no | yes |
| 57 | `<app log>/panics.log` | Machine, transient | device-bound | no | no |
| 58 | charter's keys in `~/.claude/settings.json`, its hook in `~/.codex/config.toml`, and `~/.config/opencode/{plugin/charter.ts,command/charter.md,charter-context.md}` with its keys in `opencode.json` | Machine, rebuildable | device-bound | no | yes |
| 59 | `<config>/reporting-consent` | Machine, legacy | syncable | no | — |
| 60 | `keyring charter/<vault>/<8 hex>`: a keyring vault's values | Keyring | — | no | no |
| 61 | `keyring charter/@identity/<16 hex>`: a provider's identity token | Keyring | — | no | no |
| 62 | the device id, in `machine.json` (ADR 0066) | Machine | device-bound | yes (§5) | no |
| 63 | the event log (FD-9) | Machine | device-bound | yes | no |
| 64 | the audit chain (AU-1..AU-3) | Machine | device-bound | yes | no |
| 65 | the device key (AU-3) | Keyring | — | no | no |
| 66 | `<config>/charterd/` credentials, and `charterd.sock` in `$XDG_RUNTIME_DIR/charter/` or the per-user `TMPDIR` (ADR 0068) | Machine, transient | device-bound | no | no |
| 67 | the per-chat hook spool (ADR 0068) | Clone state, transient | — | no | no |
| 68 | the run journal (FD-29) | Machine | device-bound | yes | no |
| 69 | `charterd`'s copies of itself: an AppImage's, and a runner's `<config>/server/<ver>/` | Machine, rebuildable | device-bound | no | yes |
| 70 | the forge-item cache (FI7) | Machine, rebuildable | device-bound | no | yes |
| 71 | review drafts (R5), private memory (KN-7) | Machine | syncable | yes | no |
| 72 | a guest checkout, its worktrees, an extension's workspace folder or state directory, a vendor's `.playwright*` files, Claude Code's own files | None | — | no | — |
| 73 | `<app data>/git-hooks/` (ADR 0074, added 2026-09-30) | Machine, rebuildable | device-bound | no | yes |

## Where V2's tiers do not fit cleanly

Each of these is recorded in `docs/plane-format.md` where the store is, and the choices are in
the rulings.

1. **A workspace's files are in two tiers.** The same path is Plane when the workspace is LIVE
   and Clone state when it is LOCAL. The tier line says both
   (`Plane when LIVE, Clone state when LOCAL`), so a backup has to read the LIVE block to know which.
2. **A plain-file vault is secret values in Clone state.** V2 keeps values in the Keyring.
   ADR 0047 made the keyring the default, but a plain-file vault is still a supported provider,
   and its file sits beside the registry that FR-10 backs up, or anywhere else `--file` put it.
   This record keeps it in Clone state while it is inside `.charter/`, has FR-10 skip it, and
   has a restore mark its entry file missing (§5).
3. **`machine.json` holds syncable preferences and device-bound facts in one file.** Pins and
   the update channel are preferences. The approvals, the window set and the recent planes are
   keyed by absolute path, and the device id is about to join them. So the file is
   device-bound as a whole. Syncing pins would mean moving them to a file of their own, and no
   ticket asks for that yet.
4. **`$CHARTER_HOME` splits clone state in two, and shares part of it across clones.**
   `plane::state_dir` honours the variable, and `plane.rs` says why: *"an operator points it at a
   shared directory to keep one vault and one state across several clones"*. The vault
   registry, `fingerprint.key`, the push and save journals, the gate files and the MCP consent
   follow it. The session pointers, `reopen.json`, usage, the trace, handbacks, the forge cache,
   profile trust and the rename journal are always under `<plane>/.charter`. With the variable
   set, "per clone" is false for the first group, and FR-10 has to back up both directories.
   This is a defect, filed as [#750](https://github.com/diazoxide/charter/issues/750), and
   `docs/plane-format.md`'s row for `CHARTER_HOME` now says what moves and what does not.
5. **Harness config that charter co-writes is Machine, but it is not in the machine store.**
   charter owns a few keys in `~/.claude/settings.json`, one hook in `~/.codex/config.toml` and
   three opencode files. The tier applies to charter's lines, which are rebuilt by
   `charter plugin install`, and not to the files, which belong to the harness.
6. **The hook spool is transient, but it carries audit events.** ADR 0068 has `charterd` drain
   each chat's spool and seal it into the audit chain. A spool that was never drained, because
   the machine died first, holds events nothing else has. Marking it transient means a backup
   does not save them. A chat may write its own spool and never another chat's (ADR 0068), so
   the spool is not denied to the chat that owns it.
7. **A runner's host versions lived in a third place.** ADR 0068's draft put them under
   `~/.charter/server/<ver>/`, which is neither the machine store nor an OS application
   directory. Ruled (V22a, V22b): they live under the machine store, at `<config>/server/<ver>/`.
8. **The Keyring has a headless form.** V1's device key and V9's resident secret store use an
   age-encrypted file where there is no keyring. It sits in the Keyring tier because of what it
   holds, even though it is a file on disk.
9. **`.charter/app/reopen.json` is Clone state, but what it holds is device-bound.** Each chat's
   working directory is an absolute path, and its conversation id points into this machine's
   harness store. §5 says what a restore does with it.

## What changes where

| Where | Change |
|---|---|
| `docs/adr/0034-…` | An amendment: the machine store is in the config home, not the application-data directory |
| `docs/plane-format.md` | A section defining the tiers and marks. A `**Tier:**` line on every store entry (136 of them), a Tier column in the pointers table, a table of what charter-app keeps outside the plane, an entry for `.charter/app/hooks.sock`, a correction to "Claude Code's own files", which `charter plugin install` does write into, and a correction to the `CHARTER_HOME` row (#750) |
| `crates/charter-core/tests/every_store_the_plane_format_names_has_a_tier.rs` | New. In the file sections, it fails on a heading with no tier unless the test lists it as not a store, on a table of paths with no Tier column, on a tier it cannot read, and when the section it checks cannot be found. It does not see a store named only in the prose under another heading |
| `AGENTS.md` (`CLAUDE.md`) | The rule that a new store names its tier, as part of the definition of done |
| `CONTEXT.md` | **Tier** (of a store) |
| FR-10 (#608) | Built from §2 and §5 |
| No code | No store moves. Where a store is in the wrong tier today, this record names it and does not move it |

## What this costs

- **A tier line on every entry.** The document gets longer, and any new entry must choose a
  tier. That choice is the point of the record.
- **The test reads structure, not prose.** Every heading in the file sections must carry a tier
  or be listed in the test as not a store, and every table of paths must have a Tier column. A
  store named only in a paragraph under some other heading is not checked. The document's
  convention of one heading or one table row per store is what closes that gap, and review
  holds it. The test walks lines rather than using a Markdown parser, because the document is
  line-shaped and a parser would be a new dependency for one doc test.
- **Legacy entries stay.** Python-only files keep their entries, marked legacy, because a plane
  that Python charter ran on still has them on disk. They go when the plane format drops them.

## What was rejected

- **Moving every non-derived file out of `.charter/`** to make OQ-10's original sentence true.
  Where a store is was never the problem. The problem was that the sentence was false and
  nothing checked it. Moving the vault registry would also break every plane on disk and the
  `$CHARTER_HOME` sharing that operators use today.
- **A tier per directory rather than per file.** `.charter/` holds consent records, journals,
  gate files, sockets and caches side by side, and a directory-level tier would be wrong for
  most of them.
- **"App data" as the name of the Machine tier.** The code's machine store is in the config
  home, not the application-data directory, and the tier also covers charter's lines in harness
  config. A name that points at one directory would be wrong about the rest.
- **Deciding syncable per field.** It would be more exact for `machine.json`, but the unit that
  a backup or a sync moves is a file. The tier describes files.
- **A test that scans the code for writes.** File names in this codebase are built from
  constants, joins and helpers spread across many modules, so such a test would miss stores or flag
  stores that are not there. The document test is exact about the document, and review covers
  the code.

## Ruled (V22, 2026-09-30)

1. **A fifth answer, `None`**, for paths charter records but does not own.
2. **The marks `transient` and `legacy`**, beside V2's `rebuildable`.
3. **`machine.json` is device-bound as a whole.**
4. **A plain-file vault is Clone state and is not backed up**; a restore marks it "file missing".
5. **A restore keeps device-bound state only when the operator says this machine replaces the old
   one.**
6. **`$CHARTER_HOME` moves all clone state or none** ([#750](https://github.com/diazoxide/charter/issues/750)).
7. **Review drafts and private memory are Machine and syncable.**
8. **The hook spool is transient**, and is drained before any backup.
9. **Runner binaries live under the machine store**, at `<config>/server/<ver>/`.
10. **The tier test checks the document**, and "names its tier" is a review question on every PR
    that adds a store.
