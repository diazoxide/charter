# The plane format

> Moved here from purlis/purlis-plane's `docs/plane-format.md` (at commit `0ae0961d`) by
> [ADR 0044](adr/0044-charter-apps-design-record-lives-in-charter-app.md). The text is unchanged.
> A path it names that is not in this repository — `docs/research/…`, `docs/superpowers/…`,
> `charter/*.py`, an ADR numbered below 0025 — is in
> [purlis/purlis-plane](https://github.com/purlis/purlis-plane/tree/0ae0961d8a6a8e59b48ba43b10d28de8fd87afb7).

A **control plane** is a directory marked by `charter.toml`. This document records what is
inside one: every file and every field the Python `purlis` reads or writes, as it does today
at commit `50d31dc` (0.62.1).

It exists because purlis is being rebuilt as a desktop app on a Rust core (ADR 0025, spec
`docs/superpowers/specs/2026-09-17-charter-app.md`, decision 13). **The plane format does not
change.** The app reads *and writes* the plane in Rust from M1, and answers its own hooks: no
shipped path crosses languages, and Python charter is the differential oracle in CI rather than a
dependency of the app (ADR 0025 as amended, spec decisions 14 and 15). Until Python charter is
retired at M4 both implementations still work on the *same plane at the same time* — the CLI, the
hooks a plane is already wired with and a running tmux frame are Python — so a file one of them
writes is a file the other one may be reading a moment later.

This document is the contract between them. It is descriptive, not aspirational: where the code
and the prose docs disagree, the code is recorded here and the disagreement is named.

## How to read it

Every file is marked **stable** or **internal**.

- **stable** — the rebuild must read it, and write it, exactly as recorded here. A file is
  stable when any of these is true: the operator edits it by hand; it is committed to the
  plane's git and so travels to other machines and people; a process other than the one that
  wrote it reads it; or a harness (Claude Code, opencode, Codex) reads it.
- **internal** — implementation state: written and read by one module as a cache, lock or
  marker, safe to delete, rebuilt on demand. The rebuild may change these, and each entry says
  what deleting it costs.

Marking is deliberately generous. Anything purlis writes that another process reads is stable,
because during the migration those two processes are Python charter and the app.

Every claim cites the code that makes it true, as `charter/<module>.py:<line>` at `50d31dc`.
Citations are checked mechanically; if you move code, re-run the check (below).

Four calls were close enough to be worth stating outright:

- **`.charter/frame/**` is the tmux frame's.** The app replaces that frame rather than
  reading its state, so it neither reads nor writes there. The ruling is spelled out at
  [`frame/`](#frame--the-tmux-frames-own-state-chats-panels-reopen), and ADR 0032 records
  what it costs the one ladder that reads a file there — the workspace resolution order.
- **A cache a second process reads is still internal** when deleting it costs only
  recomputation — `cache/harness-wiring.json`, `cache/repostate.json`, `cache/glstate.json`.
  Each entry says what deleting it costs, which is the thing a rebuild actually needs.
- **`.charter-generated` is stable.** Deleting it does not degrade gracefully: every file
  it vouches for is then read as the operator's own and never refreshed again.
- **`.charter/harness-profiles-launched.json` is stable**, though it lives in the state
  directory: it records an operator's consent to run a command, and a second process reads
  it to decide whether to ask again (ADR 0022).

Field tables use: **Required** (must be present), **Optional** (a default applies, given in the
row), and the status of that field where it differs from its file's.

### Every store has a tier

Every file below also carries a **Tier:** line, which says where it lives and so what a backup,
a second machine and a deletion do to it
([ADR 0069](adr/0069-every-store-charter-writes-is-in-one-of-four-tiers.md)):

- **Plane** — committed to the plane's git. The remote is its backup, and it travels to every
  clone.
- **Clone state** — per clone, in the plane directory but never committed: `.charter/`,
  `charter.local.toml`, a LOCAL workspace's files. Not derived from anything, so FR-10's backup
  carries it.
- **Machine** — outside every plane: the machine store (ADR 0034's directory,
  `$PURLIS_CONFIG_HOME`, else `$XDG_CONFIG_HOME`, else `~/.config`, then `purlis/` once
  rename-local has moved it, else `charter/`), purlis's data home (`<data>`:
  `$PURLIS_DATA_HOME`, else `$XDG_DATA_HOME/` then `purlis/` once rename-local has moved it,
  else `charter/`, else the same under the OS data directory; ADR 0075,
  `crates/purlis-core/src/datahome.rs`), the app's OS
  directories, and the lines purlis writes into a harness's global config. Each one is
  **syncable** (a preference of the operator's that could follow them to another machine) or
  **device-bound** (true of this machine only: an absolute path, a consent, an identity).
- **Keyring** — the operating system's credential store. It holds secret values, and nothing
  else holds them except a plain-file vault. FR-10 never copies it.
- **None** — a path this document records that is not purlis's store: the operator's
  checkout, a harness's own file, a vendor's output.

A workspace's files are in two tiers at once, and say so: **Plane when LIVE, Clone state when
LOCAL**. The same path is committed while the workspace is LIVE and stays in this clone while
it is LOCAL, so a backup reads the LIVE block to know which.

After the tier come the marks that apply: **rebuildable** (derived: deleting it costs a rebuild
and nothing else, and a backup skips it), **transient** (it lives for a session, a turn or one
operation, and a backup skips it) and **legacy** (only the retired Python charter creates it,
and purlis at most keeps it consistent). Clone state or Machine with none of the three
marks is what FR-10 backs up.

**Three transient stores are collected** (SC-7). When the app opens a plane that is not already
open in it, `purlis_core::retention::on_open` removes, from that plane's own `.charter/` only
(`<plane>/.charter/…`, never a `$CHARTER_HOME` that several planes may share):

- a `.charter/sessions/<sid>.<ending>` marker last written 30 days or more before
  (`retention::KEEP_FOR`) — every ending but `.tools` and `.gate`, which go sooner (below);
- a `.charter/persona-state/trace/<session>.jsonl` trace last appended to 30 days or more
  before, **unless it records a secret handed out** (`secret-exec`, `secret-reveal`,
  `secret-cp`, `identity-move`: `secrets::cmd::HANDED_OUT`). Such a trace is kept until AU-5
  writes those events into the audit chain, and after that it follows the 30-day rule (V71);
- a `.charter/reports/<id>.json` draft the Python charter left, last written 30 days or more
  before.

It keeps the files of every chat the plane's reopen record (`.charter/app/reopen.json`) will
bring back, however old: those keyed on the chat's number or on the conversation it resumes.
With no record at all, it sweeps by age alone. If the record is there but does not parse, is
of another version, cannot be read, or was written before chats kept their numbers, it keeps
every session marker and trace and collects only the report drafts. It removes only plain files
whose name is one purlis writes. It opens each directory from the plane without following a
link, and ages, reads and removes every file through that handle. It never touches anything
else in `.charter/`: the hook spool under `app/`, the event log, `terminals/`,
`persona-state/ephemeral/` and the rest are not its business. The age is the file's
modification time, so a file a chat still writes stays.

What it cannot see is outside that guarantee:
- A chat started outside the app (a harness in a terminal) is in no reopen record. If it
  resumes after 30 days without writing, its pointers are already gone.
- A chat brought back later with Resume from a record, rather than reopened at this launch, is
  in the same position.
- A `.charter/` copied or synced from another device carries that device's ages and chats.

A tier line is `**Tier:** <tier>[, <mark>…]`, optionally followed by ` — ` and a reason. The tier
and marks carry no punctuation of their own: a line with no reason has no full stop. A new
store lands in this document with its tier, under its own heading or as a row of a table whose
first column is `Path`.
`crates/purlis-core/tests/every_store_the_plane_format_names_has_a_tier.rs` fails until it
has one, and it fails on any new heading in the file sections that has no tier and is not
listed in the test as something other than a store. It does not see a store named only in the
prose under another heading, which is why every store gets a heading or a row.

## Contents

- [Fixture planes](#fixture-planes)
  - [Checking the citations](#checking-the-citations)
- [Finding the plane, and the plane root](#finding-the-plane-and-the-plane-root)
  - [Plane-root discovery (no file of its own, but the rule every file below hangs off)](#plane-root-discovery-no-file-of-its-own-but-the-rule-every-file-below-hangs-off)
  - [`charter.toml`](#chartertoml)
  - [`charter.local.toml`](#charterlocaltoml)
  - [`.charter/harness-profiles-launched.json`](#charterharness-profiles-launchedjson)
  - [`harnesses/<name>.toml` — a harness declaration](#harnessesnametoml--a-harness-declaration)
  - [`.charter/harness-declarations-approved.json`](#charterharness-declarations-approvedjson)
  - [`.charter/unattended-logins.json`](#charterunattended-loginsjson)
  - [`.gitignore` (plane root)](#gitignore-plane-root)
  - [`.gitattributes` (plane root)](#gitattributes-plane-root)
  - [Baseline directories: `personas/`, `inventory/`, `workspaces/`](#baseline-directories-personas-inventory-workspaces)
  - [`personas/<front-door>/` (what `purlis init` scaffolds at the plane root)](#personasfront-door-what-purlis-init-scaffolds-at-the-plane-root)
  - [`inventory/repos.json`](#inventoryreposjson)
  - [`docs/topology.md`](#docstopologymd)
  - [`README.md` — the generated persona roster block](#readmemd--the-generated-persona-roster-block)
  - [Plane-root files that exist but are **not** this area](#plane-root-files-that-exist-but-are-not-this-area)
- [Workspaces](#workspaces)
  - [`workspaces/`](#workspaces-1)
  - [`workspaces/.default`](#workspacesdefault)
  - [`workspaces/<ws>/`](#workspacesws)
  - [`workspaces/<ws>/workspace.md` — the living charter](#workspaceswsworkspacemd--the-living-charter)
  - [`workspaces/<ws>/workspace.json` — the committed manifest](#workspaceswsworkspacejson--the-committed-manifest)
  - [`workspaces/<ws>/memory/` — the task journal](#workspaceswsmemory--the-task-journal)
  - [`workspaces/<ws>/todos/`](#workspaceswstodos)
  - [`workspaces/<ws>/work/<device>.jsonl` — the work link log](#workspaceswsworkdevicejsonl--the-work-link-log)
  - [`workspaces/<ws>/refs/README.md` (and whatever else the operator drops in `refs/`)](#workspaceswsrefsreadmemd-and-whatever-else-the-operator-drops-in-refs)
  - [`workspaces/<ws>/sessions/` and `sessions/` — session records](#workspaceswssessions-and-sessions--session-records)
  - [`.charter/sessions/<chat>.saved` and `workspaces/<ws>/.charter/sessions/<chat>.saved` — a saved record to pass on](#chartersessionschatsaved-and-workspaceswschartersessionschatsaved--a-saved-record-to-pass-on)
  - [`workspaces/<ws>/changes/<slug>.json` — a cross-repo change](#workspaceswschangesslugjson--a-cross-repo-change)
  - [`workspaces/<ws>/changes/log/<device>.jsonl` — the landing log](#workspaceswschangeslogdevicejsonl--the-landing-log)
  - [`workspaces/<ws>/changes/log/pending/<device>.jsonl` — pending landings](#workspaceswschangeslogpendingdevicejsonl--pending-landings)
  - [`workspaces/<ws>/pieces/<device>.jsonl` — the piece claim log](#workspaceswspiecesdevicejsonl--the-piece-claim-log)
  - [`workspaces/<ws>/pieces/seen/<repo>.json` and `pieces/seen/<repo>/<piece>.json`](#workspaceswspiecesseenrepojson-and-piecesseenrepopiecejson)
  - [`workspaces/<ws>/.charter-structure` — the layout stamp](#workspaceswscharter-structure--the-layout-stamp)
  - [`workspaces/<ws>/.charter-generated` — the harness-layer ownership marker](#workspaceswscharter-generated--the-harness-layer-ownership-marker)
  - [`workspaces/<ws>/.claude/settings.json` — the generated harness layer](#workspaceswsclaudesettingsjson--the-generated-harness-layer)
  - [`workspaces/<ws>/<repo>/` — a cloned repo (guest checkout)](#workspaceswsrepo--a-cloned-repo-guest-checkout)
  - [`<clone>/.git/info/exclude` — purlis's managed block](#clonegitinfoexclude--purliss-managed-block)
  - [`workspaces/<ws>/.worktrees/<repo>/<piece>/` — pieces](#workspaceswsworktreesrepopiece--pieces)
  - [`.gitignore` (plane root) — the managed live-workspace block](#gitignore-plane-root--the-managed-live-workspace-block)
  - [`.charter/…` — active-workspace pointers and per-plane workspace state](#charter--active-workspace-pointers-and-per-plane-workspace-state)
- [Personas, memory and the roster](#personas-memory-and-the-roster)
  - [`personas/`](#personas)
  - [`personas/<name>/persona.md`](#personasnamepersonamd)
  - [`personas/<name>.md` (legacy flat layout)](#personasnamemd-legacy-flat-layout)
  - [`personas/<name>/memory/MEMORY.md`](#personasnamememorymemorymd)
  - [`personas/<name>/memory/<slug>.md` (and `personas/_shared/memory/<slug>.md`)](#personasnamememoryslugmd-and-personas_sharedmemoryslugmd)
  - [`personas/<name>/memory/archive/<slug>.md`](#personasnamememoryarchiveslugmd)
  - [`personas/<name>/memory/.gitkeep`, `personas/<name>/refs/.gitkeep`](#personasnamememorygitkeep-personasnamerefsgitkeep)
  - [`personas/<name>/refs/README.md` and `personas/<name>/refs/**`](#personasnamerefsreadmemd-and-personasnamerefs)
  - [`personas/<name>/mcp.json`](#personasnamemcpjson)
  - [`personas/<name>/bin/<script>`](#personasnamebinscript)
  - [`personas/<name>/curation/<id>.md` — a curation action](#personasnamecurationidmd--a-curation-action)
  - [`personas/_shared/` (`memory/`, `refs/`)](#personas_shared-memory-refs)
  - [`personas/.default` (legacy)](#personasdefault-legacy)
  - [`personas/_dispatch/<YYYY-MM>.<device>.jsonl`](#personas_dispatchyyyy-mmdevicejsonl)
  - [`personas/_dispatch/<YYYY-MM>.<host>.backfill.jsonl`](#personas_dispatchyyyy-mmhostbackfilljsonl)
  - [`personas/_skills/<YYYY-MM>.<device>.jsonl`](#personas_skillsyyyy-mmdevicejsonl)
  - [`.claude/agents/<name>.md` (generated sub-agent)](#claudeagentsnamemd-generated-sub-agent)
  - [`.charter/persona-state/ephemeral/<session>/<name|_shared>/<slug>.md`](#charterpersona-stateephemeralsessionname_sharedslugmd)
  - [`.charter/persona-state/trace/<session>.jsonl`](#charterpersona-statetracesessionjsonl)
  - [`.charter/reports/<id>.json`](#charterreportsidjson)
  - [`.charter/handbacks/` — reports back from handed-off chats](#charterhandbacks--reports-back-from-handed-off-chats)
  - [`~/.config/charter/reporting-consent` (outside the plane)](#configcharterreporting-consent-outside-the-plane)
  - [`.charter/sessions/<sid>.persona`, `.charter/terminals/<tid>.persona`, `.charter/active-persona`](#chartersessionssidpersona-charterterminalstidpersona-charteractive-persona)
  - [`.charter/mcp-approved.json`](#chartermcp-approvedjson)
  - [`[memory] share` — how it affects persona files](#memory-share--how-it-affects-persona-files)
- [Vaults: the registry, and nothing inside it](#vaults-the-registry-and-nothing-inside-it)
  - [`vaults.json` (plane root — the SHARED half)](#vaultsjson-plane-root--the-shared-half)
  - [`.charter/vaults.json` (the LOCAL half)](#chartervaultsjson-the-local-half)
  - [`.charter/vaults/` (directory)](#chartervaults-directory)
  - [`.charter/vaults/<name>.json` — plain-file vault](#chartervaultsnamejson--plain-file-vault)
  - [`.charter/vaults/<name>.meta.json` — rotation sidecar](#chartervaultsnamemetajson--rotation-sidecar)
  - [`.charter/vaults/<name>.keys.json` — keyring vault's keys index](#chartervaultsnamekeysjson--keyring-vaults-keys-index)
  - [`.charter/keyring-stub.json` — a test build's keyring](#charterkeyring-stubjson--a-test-builds-keyring)
  - [`.charter/vaults/<name>.json` — reference vault (same path, different content)](#chartervaultsnamejson--reference-vault-same-path-different-content)
  - [Secret reference syntax](#secret-reference-syntax)
  - [`.charter/fingerprint.key`](#charterfingerprintkey)
  - [1Password provider — not a file, but a shape another implementation must match](#1password-provider--not-a-file-but-a-shape-another-implementation-must-match)
  - [Guarded paths (why a harness cannot read any of the above)](#guarded-paths)
- [Files purlis writes that a harness reads](#files-purlis-writes-that-a-harness-reads)
  - [2a. Inside the plane](#2a-inside-the-plane)
  - [`<plane>/.claude/settings.json`](#planeclaudesettingsjson)
  - [`<plane>/.claude/settings.local.json`](#planeclaudesettingslocaljson)
  - [`<plane>/opencode.json`](#planeopencodejson)
  - [`<plane>/.gitignore` (the lines purlis owns)](#planegitignore-the-lines-purlis-owns)
  - [Generated harness layer in `workspaces/<ws>/` and in clones](#generated-harness-layer-in-workspacesws-and-in-clones)
  - [2b. Outside the plane (machine-global)](#2b-outside-the-plane-machine-global)
  - [`~/.config/opencode/plugin/charter.ts` (`$XDG_CONFIG_HOME` honoured)](#configopencodeplugincharterts-xdg_config_home-honoured)
  - [`~/.config/opencode/command/charter.md`](#configopencodecommandchartermd)
  - [`~/.config/opencode/charter-context.md`](#configopencodecharter-contextmd)
  - [`~/.config/opencode/opencode.json`](#configopencodeopencodejson)
  - [`~/.codex/config.toml` (`$CODEX_HOME` honoured)](#codexconfigtoml-codex_home-honoured)
  - [Claude Code's own files — purlis does NOT write them](#claude-codes-own-files--purlis-does-not-write-them)
  - [The shipped plugin's `hooks/hooks.json`](#the-shipped-plugins-hookshooksjson)
  - [2c. Git config purlis sets](#2c-git-config-purlis-sets)
  - [2d. Charter-private caches in this area](#2d-charter-private-caches-in-this-area)
  - [`.charter/cache/harness-wiring.json`](#chartercacheharness-wiringjson)
  - [`.charter/unrecorded/<sha256[:32]>.json`](#charterunrecordedsha25632json)
- [`.charter/` — runtime state](#charter--runtime-state)
  - [Conventions that apply to every file in this area](#conventions-that-apply-to-every-file-in-this-area)
  - [`sessions/` — per-session markers](#sessions--per-session-markers)
  - [`sessions/<sid>.workspace`](#sessionssidworkspace)
  - [`sessions/<sid>.lock`](#sessionssidlock)
  - [`sessions/<sid>.tools` — the persona tool **ceiling**](#sessionssidtools--the-persona-tool-ceiling)
  - [`sessions/<sid>.gate` — "a ceiling was taken for this session"](#sessionssidgate--a-ceiling-was-taken-for-this-session)
  - [`sessions/<sid>.usage` — token/cache trend ring buffer](#sessionssidusage--tokencache-trend-ring-buffer)
  - [`sessions/<sid>.memnudge`](#sessionssidmemnudge)
  - [`sessions/<sid>.configver`](#sessionssidconfigver)
  - [`sessions/<sid>.<tool_use_id>.<kind>.ask-pending`](#sessionssidtool_use_idkindask-pending)
  - [`sessions/<sid>.route-pending`](#sessionssidroute-pending)
  - [`sessions/<sid>.persona`](#sessionssidpersona)
  - [`terminals/` — per-pane pointers](#terminals--per-pane-pointers)
  - [`terminals/<tid>.workspace`, `terminals/<tid>.persona`](#terminalstidworkspace-terminalstidpersona)
  - [`frame/` — the tmux frame's own state (chats, panels, reopen)](#frame--the-tmux-frames-own-state-chats-panels-reopen)
  - [`frame/chat-ids.json`](#framechat-idsjson)
  - [`frame/chat-ids.lock`](#framechat-idslock)
  - [`frame/<chat>/` — one directory per chat](#framechat--one-directory-per-chat)
  - [`frame/reopen.json`](#framereopenjson)
  - [`frame/<chat>.transcript`](#framechattranscript)
  - [`frame/<frame-id>/` for a non-chat frame (e.g. the live plane's `probe-1`)](#frameframe-id-for-a-non-chat-frame-eg-the-live-planes-probe-1)
  - [`app/` — the desktop app's own state](#app--the-desktop-apps-own-state)
  - [`app/sandbox.json`](#appsandboxjson)
  - [`app/sandbox-blocks.json`](#appsandbox-blocksjson)
  - [`app/reopen.json`](#appreopenjson)
  - [`app/hooks.sock`](#apphookssock)
  - [Top-level markers, gates and ledgers](#top-level-markers-gates-and-ledgers)
  - [`chat-turns/<chat>`](#chat-turnschat)
  - [`dispatch-inflight/<agent>.<random>.json`](#dispatch-inflightagentrandomjson)
  - [`commit-gate/<sid>`](#commit-gatesid)
  - [`dispatch-commit.lock`](#dispatch-commitlock)
  - [`guard-seen.json`](#guard-seenjson)
  - [`mcp-approved.json`](#mcp-approvedjson)
  - [`agent-personas.json`](#agent-personasjson)
  - [`plane-push.json`](#plane-pushjson)
  - [`save-branch.json`](#save-branchjson)
  - [`save-journal.jsonl`](#save-journaljsonl)
  - [`ws-edit-nudge/<sid>-<workspace>`](#ws-edit-nudgesid-workspace)
  - [`ws-autosave/<workspace>`](#ws-autosaveworkspace)
  - [`workspace-tab-order`](#workspace-tab-order)
  - [`workspace-arrivals/<workspace>`](#workspace-arrivalsworkspace)
  - [`unrecorded/<sha256(realpath(tree))[:32]>.json`](#unrecordedsha256realpathtree32json)
  - [`locks/harness-wiring-<digest16>.lock`](#locksharness-wiring-digest16lock)
  - [`active-workspace` (legacy)](#active-workspace-legacy)
  - [`active-persona`](#active-persona)
  - [`cache/` — derived data with a TTL](#cache--derived-data-with-a-ttl)
  - [`cache/repostate.json`](#cacherepostatejson)
  - [`cache/glstate.json`](#cacheglstatejson)
  - [`cache/glstate.refreshing`](#cacheglstaterefreshing)
  - [`cache/update.json` and `cache/update.checking`](#cacheupdatejson-and-cacheupdatechecking)
  - [`cache/update-baseline`](#cacheupdate-baseline)
  - [`cache/vaulthealth.json`](#cachevaulthealthjson)
  - [`cache/harness-wiring.json`](#cacheharness-wiringjson)
  - [`index/<clone-key>/` — the project's search index](#indexclone-key--the-projects-search-index)
  - [`index/<clone-key>/writer.lock`](#indexclone-keywriterlock)
  - [State purlis keeps **outside** the plane](#state-charter-keeps-outside-the-plane)
  - [Environment variables that move or key this state](#environment-variables-that-move-or-key-this-state)
  - [What the tmux frame and the status line read that a **hook** wrote](#what-the-tmux-frame-and-the-status-line-read-that-a-hook-wrote)
- [Appendix: what this survey found in the code](#appendix-what-this-survey-found-in-the-code)
  - [In the plane root](#in-the-plane-root)
  - [In workspaces](#in-workspaces)
  - [In personas and memory](#in-personas-and-memory)
  - [In the vaults and the harness wiring](#in-the-vaults-and-the-harness-wiring)
  - [In `.charter/`](#in-charter)

## Fixture planes

The spec asks for fixture planes both implementations test against. They live in the
`purlis` repo, at `tests/fixtures/planes/`, and they were **generated by running this
purlis** — never hand-written — so they are true by construction. The generator that ran it
was retired with the Python oracle (purlis ADR 0046) and the planes are data now. Their
README records what the generator pinned (clock, hostname, user, session id, `PATH`) to keep a
regeneration byte-identical, and what a committed fixture cannot carry (every `.git` directory, the caches keyed by absolute
path, `fingerprint.key`, and the empty directories a fresh plane has — `inventory/` and
`workspaces/` — which are recorded beside each plane instead).

There are two: `minimal`, what `purlis init` leaves behind, and `daily`, a plane in use — a
LIVE workspace with a clone, memory, todos and a snapshot; a second workspace left local; a
second persona with its own and shared memory; a vault registry; and the session state a
harness run leaves in `.charter/`. A file documented here that no fixture holds is one no
offline command writes; the entry for it says which writer to call instead.

One practical note for an agent working with them: purlis's guard denies tool calls that
read a `.charter/vaults/` path, and it cannot know that a fixture vault holds only the
literal `fixture-not-a-secret`. Expect the denial, and do not work around it.

### Checking the citations

`tests/test_the_plane_format_spec_cites_lines_that_exist.py` walks every citation in this
document and fails if the file is gone, the line is past the end of it, or the line is blank —
the three ways a line number rots when code moves under it. It is a pointer check, not a truth
check: it cannot tell whether the cited line still says what this document claims. If it goes
red, re-derive the numbers rather than editing them by hand.

### Compatibility across purlis versions (FR-24)

A team shares one project through git, and each person upgrades purlis on their own schedule.
So two purlis versions read the same files, and this section is the rule they follow (X31, as
amended by V5, and ruled by V37 and D-FR24). It is written before the code that holds it (V5).

**Settled by X31 and V5:**

- **An additive change keeps `schema`.** A new optional key, file or section that an older
  purlis can ignore safely does not bump anything.
- **A reader keeps what it does not know.** A purlis that rewrites a file keeps every key,
  section and line it does not understand, in place. `charter.toml` is edited as text, one key
  at a time ([`charter.toml`](#chartertoml), *Encoding details*), and `workspace.json` is
  rewritten from the whole document purlis read, unknown keys included.
- **A change an older purlis must not ignore is a required feature**, named in `charter.toml`'s
  top-level `requires` list. A purlis that lacks any feature the project requires **opens it
  read-only**: it reads, writes nothing to it, and names the purlis version that has the
  feature.
- **Each sub-format carries its own version**, as `workspaces/<ws>/.charter-structure` and the
  hook registry's `schema` already do, and none is compared with another (see the note on
  `instance.SCHEMA` and `workspace.STRUCTURE_VERSION` at the end of this document).
- **A bump of `schema` or of a sub-format's version needs an ADR, a migration (FR-9) and a
  `doctor` row**, and is staged and announced, so that one person's migration never locks the
  rest of the team out (V5).

**Settled by V37 and D-FR24:**

- **`requires` takes effect together with `schema = 2`** (V37a), as git's `extensions.*` do
  under `repositoryformatversion = 1`. A project that lists `requires` declares `schema = 2`, so
  that a purlis older than FR-24, which does not know `requires`, refuses the project for its
  schema instead of writing it. A purlis that knows `requires` reads it at any `schema`.
- **A `schema` this purlis does not understand makes the project read-only everywhere** (V37a),
  not only for forge commands. This purlis understands 2. A new project is still written with
  `schema = 1`, because it requires nothing.
- **An entry is `{ feature, since }`** (V37b), a public format commitment:

  ```toml
  schema = 2
  requires = [
    { feature = "memory-proposals", since = "0.9.0" },
  ]
  ```

  `feature` names the feature. `since` is the first purlis version that has it, and it is
  what a purlis that lacks it names in its refusal. Without `since`, the refusal says a newer
  purlis has it.
- **Old forms are read for six months from the first release that writes the new form** (V37c).
  After a bump of `schema` or of a sub-format's version, purlis keeps reading the previous
  version for six months. The plane → project rename's old names (#762, ADR 0072) are read for
  the same six months.
- **A file with a closed key set keeps it closed, and gets its own sub-format version**
  (D-FR24). A change record (`workspaces/<ws>/changes/<slug>.json`, ADR 0060) refuses a key it
  does not know rather than drop it. The first change that adds a key to it also gives change
  records a version of their own (absent means 1), so that a purlis that does not know the new
  version locks out only change records, not the whole project.

**Failing closed.** A `charter.toml` that cannot be read or is not TOML, a `schema` that is not
an integer or is higher than 2, a `requires` that is not a list, and an entry with no readable
`feature` each make the project read-only, with a reason that says which. A purlis that guessed
would write a project it does not understand. A directory with no `charter.toml` is not a
project and says nothing.

**Read-only means the committed project files.** Clone state and machine stores belong to this
clone and this machine, and are still written.

**What purlis does with a read-only project today:**

- **The commands that only read still run**, after one line on stderr saying the project is
  read-only and why: `status`, `recall`, `statusline`, `workspace list`, `workspace current`,
  `workspace recall`, `persona list`, `change list`, `session list`, `harness list` and
  `guard list` (or bare `guard`). Each is on the list because a test runs it on a read-only
  project and finds every file unchanged. `doctor` (without `--fix`), `update`, `version`,
  `news`, `root` and the commands that need no project run as they always do. The list is
  explicit: a command that is not on it is refused, so a new command is refused on a read-only
  project until it is shown to only read.
- **Every other command is refused**, each refusal naming the reason: extension commands and the
  core-owned aliases onto them (the same test `extension::cli::extension_command` gives the tool
  guard), `doctor --fix`, `init` and `reinit`.
- **`init` and `reinit` facing a `charter.toml` that is a link out of the project, or into its
  `.git`, write nothing**, and say where the link leads. A link that stays inside the project is
  read through like the file it points at, so its `requires` and `schema` count.
- `purlis doctor`'s `schema` row fails with the same reason and its remedy.
- **The window, and the hooks a chat runs, follow in #826, and no release carries this rule
  without them.** Until #826 lands they do not check `requires` or `schema`.

**The features this purlis knows** (`compat::KNOWN`); any other `requires` entry makes a project
read-only to it:

- **`purlis-names`** (RN-7, V93g). The project's committed files use purlis's names: the manifest
  is `purlis.toml`, and the managed blocks, the generated agents' marker, a committed
  `workspace.json`'s digest key and `.claude/settings.json`'s harness variable are spelt
  `purlis`. Hook commands keep `charter` for the rename's window. `purlis doctor --fix rename-plane` adds it, in the one commit that renames
  those files; nothing else does. A purlis without the feature opens the project read-only and
  says which version has it, so it never writes charter's names beside purlis's.

Read by `crates/purlis-core/src/compat.rs` (`read`, `SCHEMA`), and checked by the `purlis`
command before it runs a command that could write (`crates/purlis-cli/src/main.rs`) and by
`purlis doctor`'s `schema` row (`crates/purlis-core/src/doctor/config.rs`).

## Finding the plane, and the plane root

Covered here: `charter.toml`, `charter.local.toml`, `.charter/harness-profiles-launched.json`,
`harnesses/<name>.toml` and `.charter/harness-declarations-approved.json`,
plane-root discovery, `.gitignore`, the baseline directories, the `purlis init` front-door
scaffold, `inventory/repos.json`, `docs/topology.md`, and the generated README roster block.

---

### Plane-root discovery (no file of its own, but the rule every file below hangs off)

- **Marker:** `charter.toml` at the directory — `is_file`, never a directory
  (`charter/root.py:17`, `charter/root.py:62`). **Status: stable** — it is the one thing a
  second implementation must agree on to find the same plane.
  **Rename window (#1253, RN-1):** `purlis.toml` marks a plane too, and when a directory
  holds both it is the manifest (V93e). `charter.local.toml` and `.charter-scan-allow.toml`
  follow the same rule, and a write goes to the file the plane already has, so a plane with
  only old names is written as before. The state folder is `.purlis/` only when it is the one
  there, or when both are there and this machine's rename-local record says the project moved;
  otherwise it is `.charter/` (RN-2a). A name held under both spellings is named by the doctor's
  `renamed leftovers` row: `rename-plane` reconciles the committed files; `rename-local` moves
  `.charter/` and `charter.local.toml` only where the purlis name is not there yet, and never
  merges two. Every name the rename moves, with its old spellings, is in `crates/purlis-core/src/names.rs`.
- **Resolution order** (`charter/root.py:32`, `find_root`):
  1. `$CHARTER_ROOT` wins outright; it is `expanduser`'d and `resolve`'d, and a value with no
     `charter.toml` under it **raises** rather than falling back to the walk
     (`charter/root.py:49`, `charter/root.py:56`).
  2. Otherwise walk up from `start` (default cwd, resolved) through `(cur, *cur.parents)`
     (`charter/root.py:60`).
  3. A found marker is redirected to the **main working tree** when the directory is a linked
     git worktree whose main tree also carries the marker (`charter/root.py:310` `_plane_of`,
     `charter/root.py:157` `main_worktree_of` — pure path arithmetic on the `.git` file's
     `gitdir:` line, no subprocess).
  4. Then hop **outward** through any enclosing plane's `workspaces/` until the answer stops
     moving (`charter/root.py:91` `_outermost`, `charter/root.py:333` `enclosing_plane`: the
     enclosing marker only counts when `here.relative_to(parent/"workspaces")` succeeds,
     `charter/root.py:363`).
  5. If no marker at all: if the cwd (or an ancestor) is a linked worktree, retry from its
     main tree and that tree's parents (`charter/root.py:79`).
  6. Still nothing → `ControlPlaneNotFound` (`charter/root.py:88`); `find_root_or_cwd`
     swallows that and returns the start directory instead (`charter/root.py:370`), which is
     what makes `purlis --version` and `purlis init` work outside a plane.
- `config` bootstraps from this at **import** of any command
  (`charter/config.py:918`), and `config.use(root)` re-points the whole module
  (`charter/config.py:844`). `purlis init` calls `use()` the moment it writes the marker
  (`charter/commands.py:2718` then `config.use(root)`).
- **Env vars purlis honours here:** `CHARTER_ROOT` (`charter/root.py:20`), `CHARTER_HOME`
  (the state dir, `charter/config.py:42`), `CHARTER_WORKTREES` (`charter/config.py:82`).
- `NESTED_ORIGIN` records the nested plane the caller stands in when the hop fired
  (`charter/config.py:697`, `charter/root.py:131`).
- `.charter-generated` / `.charter-structure` are **workspace-interior** markers written by
  `charter/workspace.py:1935` and `charter/workspace.py:4477`, not plane-root ones — they
  belong to the workspaces area.

Paths derived from the root (all in `derive`, `charter/config.py:661`) that land in this area:
`ROOT` (`:680`), `HAS_CONTROL_PLANE` (`:684`), `SHARED_VAULTS = root/"vaults.json"` (`:774`),
`WORKSPACES_DIR` (`:780`), `INVENTORY = root/"inventory"/"repos.json"` (`:783`),
`DOCS_DIR = root/"docs"` (`:786`), `PERSONAS_DIR` (`:819`), `STATE_DIR` (`:791`).

---

### `charter.toml`

- **Format:** TOML (parsed with stdlib `tomllib`, `charter/instance.py:122`). Hand-maintained;
  purlis only ever edits two keys, as raw text.
- **Status:** **stable** — committed, hand-edited, and read by every purlis process, every
  hook process, and the frame's panel processes.
- **Tier:** Plane — committed; it travels with every clone.
- **Written by:** `charter/commands.py:1066` `_render_charter_toml` (via `cmd_init`,
  `charter/commands.py:2718`) for a fresh plane; thereafter only
  `charter/instance.py:389` `_set_key` — a **line-span textual edit** used by
  `set_locked_version` (`charter/instance.py:325`), `set_default_persona`
  (`charter/instance.py:331`), `declare_default_persona` (`:342`) and
  `clear_default_persona` (`:355`). `purlis version bump` also commits it
  (`charter/commands.py:3842`).
  **In purlis, also the Settings tab's Project level** (`purlis_core::settings::save`,
  purlis#252, SE-17, SE-19): a setting's change, or the whole text from its Edit as TOML
  link, written whole or not at all (a temp file beside it, then one
  rename), keeping the existing file's mode. A form's change is applied with `toml_edit`, so
  every comment, blank line, key order and spacing it did not touch is kept, and a replaced
  value keeps the decoration it had. It refuses to write text that does not parse as TOML, and
  text in which the next read would refuse something the file on disk does not already have — a finding of `purlis doctor`'s
  `charter.toml` row, a `[harness.<name>]` table — in those readers' own words; it refuses a
  value `secretshape` calls a credential whether or not the file already held it; and it
  refuses to write over a file that changed on disk since the tab read it. (The file marks the
  plane, so the tab is only ever open where it exists; were it deleted under the tab, a save
  would create it at 0600.)
  **And `[project].id`**, minted once into a project that has none, with `toml_edit`
  (`planefile::ensure_project_id`, V76; see its row below).
- **Read by:** `charter/instance.py:105` `load` — and *only* there:
  `charter/config.py:719` (every command/hook, at import), plus direct re-reads in
  `charter/commands.py:211`, `charter/commands.py:3614`, `charter/hooks.py:7260`,
  `charter/statusline.py:2123`, `charter/statusline.py:2141`, `charter/doctor.py:343`,
  `charter/persona.py:1031`, `charter/profiles.py:432`, `charter/forge/registry.py:96`,
  `charter/forge/registry.py:140`, `charter/commands_update.py:604`. In purlis, its
  `[extensions]` table is read by `crates/purlis-core/src/extension/project.rs`
  (`Choices::read`) for the Settings tab, the window's filter on extension panels,
  views and themes, and the executor's gate (purlis#253); its `[theme]` table by
  `crates/purlis-core/src/extension/project/theme.rs` (`Said::read`) for the Settings
  tab and the window's theme (purlis#273). Its `[harness_plugins]` table is
  read by `crates/purlis-core/src/harness_plugin.rs` (`Choices::read`) for the Settings
  tab and for every chat `start::ready` launches (purlis#274). Its `[sandbox]` table is read
  by `crates/purlis-core/src/sandbox.rs` (`Said::read`) for every chat `start::ready` launches
  and for the Settings tab's save (ADR 0067).
- **Git:** committed (nothing ignores it; `_GITIGNORE_BASELINE` ignores its *local* sibling
  only, `charter/commands.py:1104`).
- **Encoding details for a byte-identical writer:**
  - `init` renders exactly: `schema = 1`, blank, `[[forge]]`, `kind = "<kind>"`, optional
    `owner = "..."`, optional `host = "..."`, blank, `[memory]`, `share = "local"`, trailing
    newline (`charter/commands.py:1066`–`charter/commands.py:1078`). String values go through
    `json.dumps` (`charter/commands.py:1058` `_toml_str`) — JSON escaping is used as a
    faithful subset of TOML basic-string escaping.
  - `_set_key` (`charter/instance.py:389`) reads with `splitlines(keepends=True)` and writes
    with `p.write_text("".join(lines))` (`charter/instance.py:441`) — **not** atomic, no temp
    file, no lock, comments and formatting preserved. **In purlis** (#357) the same text
    edit is written whole or not at all: a temp file beside `charter.toml`, flushed, then one
    rename, keeping the file's mode and refusing a read-only file. The read, the edit and the
    rename happen under an advisory `flock` on the plane root directory
    (`crates/purlis-core/src/rewrite.rs`), which the settings tab's save takes too, so two
    writers at once both land.
    - Section located by `^[ \t]*\[<section>\][ \t]*$`; the edit is confined to that section's
      span, ending at the next line matching `^[ \t]*\[` (`charter/instance.py:416`,
      `charter/instance.py:426`).
    - Existing key: only the value is replaced, the `key<spaces>=<spaces>` prefix is kept
      (`charter/instance.py:441`).
    - Key absent, section present: inserted as the **first line after the header**,
      `key = "value"\n` (`charter/instance.py:438`).
    - Section absent: appended as `"\n", "[<section>]\n", 'key = "value"\n'` after a newline
      fixup if the file did not end in one (`charter/instance.py:424`).
    - Removal (`value=None`) deletes the key line and **leaves the emptied section header**
      (`charter/instance.py:436`); a missing section or missing key rewrites nothing.
    - Values are always emitted double-quoted, so only string-valued keys are writable this
      way.
    - **In purlis** the edited text is parsed before it is written, and must say what was
      asked (`[section] key` set to the value, or gone for a removal). The edit finds a section
      only by a plain `[section]` header, so a table written any other way (a header with a
      comment, an inline table, a dotted key), or a `section` that is not a table, would be
      doubled or shadowed: that is refused, and the file is left as it was (FD-26). Measured: `set_locked_version` on the `init` output appends
      `\n[charter]\nversion = "0.62.1"\n`.
- **Schema/refusal:** `plane_version` (`charter/instance.py:79`) — absent `schema` means
  `UNSTAMPED = 1` (`charter/instance.py:63`), a non-`int` (or `bool`) value means "cannot
  place" → `PlaneFormatUnknown`; `found > SCHEMA` → `SchemaTooNew`
  (`charter/instance.py:133`). `config.derive` records it as `PLANE_REFUSAL`
  (`charter/config.py:723`) and `cli` declines every command except
  `doctor`/`update`/`version`/`_version-check` (`charter/cli.py:2224` `_DESPITE_REFUSAL`,
  `charter/cli.py:2239`). Malformed TOML instead raises `ValueError`, is caught, and becomes
  `CONFIG_ERROR` with empty defaults (`charter/config.py:725`, `charter/doctor.py:323`).
- **General rule for every `[section]` below:** an absent or wrong-typed section/value
  **degrades to the shipped default** and never raises — `charter.toml` is on the import path
  of `purlis --version` (`charter/instance.py:1994`, `:2955`, `:3039`). The two exceptions
  that *report* rather than swallow are `[harness] default` (`refused`,
  `charter/instance.py:3097`) and `[[frame.component]]` (whole arrangement refused,
  `charter/instance.py:2400`).

| Field | Type | Required / default | Meaning | Status | Source |
|---|---|---|---|---|---|
| `schema` | int (top level) | optional; absent = 1 | Project format version. charter-app understands 2, and a higher one makes the project read-only to it (V37a). `init` writes 1. A bump follows [Compatibility across charter versions](#compatibility-across-purlis-versions-fr-24). | stable | `charter/instance.py:98` |
| `requires` | array of tables (top level) | optional; absent = none. Declared with `schema = 2` (V37a) | Features a charter must have to write this project, each `{ feature, since }` (V37b). A charter that lacks one opens the project read-only and names `since` ([Compatibility across charter versions](#compatibility-across-purlis-versions-fr-24)). | **stable**, a public format commitment (V37b): read by charter-app from FR-24; written only by the `rename-plane` fix, which adds `purlis-names` (RN-7) | `crates/purlis-core/src/compat.rs` |
| `[project].id` | str, a ULID | optional; absent = none yet, minted at the first ask | **purlis only** (V76, ADR 0068 as amended by FD-26). The project's stable id: what the session protocol names a project by, in `start` and in each listed chat's `project`, never its path, which differs on every clone and device. Minted once, as a ULID like a chat's and a device's (ADR 0066), by `planefile::ensure_project_id` the first time something asks for it, and written into this file with `toml_edit`, as the Settings tab's save writes, under the plane root's lock, so two processes asking at once get one id. A `project` table written any way TOML allows (a header with a comment, spaces or quotes, an inline table, a dotted key, a subtable) gets the id inside it; a `project` that is not a table is refused. The result is parsed again before it is written and must read as holding exactly the new id, or nothing is written. **Tier:** Plane, as the whole file: committed, so every clone and device of the project names it the same. Never written over: a value that is not a ULID is reported and left as it is, and the reader treats it as none. Read in its canonical (upper-case) spelling. | stable, a public format commitment (V76) | `crates/purlis-core/src/scaffold/planefile.rs` `project_id`, `ensure_project_id` |
| `[[forge]]` | array of tables | optional; none = one default GitLab forge | One block per forge tracked. Index 0 is `config.GROUP`/`EXCLUDE`. | stable | `charter/instance.py:143` |
| `[[forge]].kind` | str | default `"gitlab"`; one of `gitlab`, `github` | Backend class. Unknown kind = that block skipped + reported. | stable | `charter/forge/registry.py:69`, `charter/forge/registry.py:15` |
| `[[forge]].group` / `.owner` | str | optional; `group` wins, else `owner`, else `""` | Org/group whose repos are discovered. | stable | `charter/instance.py:162` |
| `[[forge]].host` | str | optional; default `gitlab.com` / `github.com` | Self-hosted host. Must match `_HOST_RE` (bare host, optional `:port`) or the block is refused. | stable | `charter/forge/registry.py:29`, `charter/forge/registry.py:56` |
| `[[forge]].exclude` | array of str | optional; default `()` | Repo names never written to the inventory; per block. | stable | `charter/instance.py:170` |
| `[memory].share` | str | default `"local"`; one of `local`,`commit`,`push` | How far a written memory travels. Unknown value clamps to `local`. **In purlis, a deprecated alias of `[plane].mode`** (`local`→*not set*, `commit`→`commit`, `push`→`push`); `[plane].mode` wins when both are present, and `doctor` warns (ADR 0051). | stable | `charter/instance.py:466`, `charter/instance.py:265` |
| `[workspace].default` | str | default `"default"` | Workspace used when nothing else selected. Validated by `workspace_name_ok` (`^[A-Za-z0-9][A-Za-z0-9._-]*$` + `contain.segment_ok`); invalid → fallback. | stable | `charter/instance.py:243`, `charter/instance.py:181` |
| `[persona].default` | str | optional; blank = absent = `None` | The plane's front door persona. Written by `purlis persona default`. | stable | `charter/instance.py:259` |
| `[plane].worktrees` | str | optional; `None` = `workspaces/<ws>/.worktrees/` | Relocated worktree root. Relative resolves against ROOT; a committed value must satisfy `contain.plane_adjacent` or it is ignored (doctor warns). `$CHARTER_WORKTREES` overrides and is unrestricted. | stable | `charter/instance.py:488`, `charter/config.py:82` |
| `[plane].mode` | str | optional; closed set `off`,`commit`,`push`,`pr`,`pr-merge`; absent = `[memory].share`'s alias when that is `commit` or `push`, else **ask once** (the Saving view asks before anything is pushed); a new plane is written with `push` | **charter-app only.** How far a save of the plane goes, as a ladder: `off` never commits; `commit` commits locally; `push` also pushes to `branch`; `pr` pushes to `save_branch` and opens or updates one PR/MR into `branch`; `pr-merge` also sets that PR to auto-merge. `pr`/`pr-merge` on an origin that is not a GitHub or GitLab forge charter knows is a config error (doctor, the settings tab): saves stop at a local commit, shown as a notice, and the plane is not blocked. Unknown value → refused by the settings tab, read as absent. | stable | ADR 0051; `crates/purlis-core/src/planesave.rs`, `crates/purlis-core/src/planegit/prsave.rs` |
| `[plane].branch` | str | optional; default the branch the plane has checked out | **purlis only.** The *target* branch the plane is saved into. A save whose plane has another branch checked out commits and does not push: pushing would rebase that branch onto this one (ADR 0051). | stable | ADR 0051; `crates/purlis-core/src/planesave.rs`, `crates/purlis-core/src/planegit.rs` |
| `[plane].save_branch` | str | optional; default `purlis/save/<host>-<clone>` (`charter/save/<host>-<clone>` before the rename, which a clone whose PR is still open from it keeps using until a save sees that PR merged or closed, V93j): `<host>` is the machine's short hostname, by the rule the dispatch log used before FD-25 (`dispatch::host`); a branch name is something the operator reads, so it stays a name (ADR 0066), `<clone>` the first six hex digits of the SHA-256 of the plane's absolute path, so two clones, or two machines with one name, never share one | **charter-app only.** The one rolling branch per clone that `pr`/`pr-merge` push to; one PR from it is kept open and updated by every save. Replaces the per-push `purlis/<sha>` branch (`charter/<sha>` before the rename, still advanced while its request waits) for the request modes. Only this branch is ever force-pushed, with `--force-with-lease` against the commit this clone last pushed there (`save-branch.json`); with none kept it is leased as absent, and an existing branch is pushed over only when its tip is already in HEAD's history — otherwise the plane is blocked. Must not be the target branch (a config error, and the plane is blocked). A value shared in `charter.toml` is one branch for every clone, and the lease then blocks all but the first. | stable | ADR 0051; `crates/purlis-core/src/planegit/prsave.rs` (charter-app#298) |
| `[plane].sign` | bool | optional; default `false` | **purlis only.** Sign save commits. (ADR 0051 also has a push refused for an unsigned commit tell the operator to set this; that hint is not built.) | stable | ADR 0051; `crates/purlis-core/src/planesave.rs`, `crates/purlis-core/src/planegit.rs` |
| `[plane].autosave` | bool | optional; default `true` | **purlis only.** Save by itself: after `autosave_after` of quiet, when a session ends, and when the app quits (the push gets about five seconds; the next launch pushes what was left). Also fast-forwards a clean tree from the remote every five minutes and on window focus; with `false`, incoming commits are shown, not pulled. | stable | ADR 0051; `crates/purlis-core/src/autosave.rs`, `app/src-tauri/src/autosave.rs` |
| `[plane].autosave_after` | str | optional; default `"30s"`; a whole number followed by `s` or `m` | **purlis only.** The quiet period after the last change before an auto-save. | stable | ADR 0051; `crates/purlis-core/src/planesave.rs`, `crates/purlis-core/src/autosave.rs` |
| `[plane].assisted_by` | str | optional; default `"full"`; one of `full`, `llm`, in any case; a `[repos.<name>]` that does not set it follows this one | How a commit an agent run made spells its `Assisted-by` trailer: `full` is `<harness>:<model>`, `llm` is the kernel's bare `LLM`. See *Provenance trailers* below. | stable | V67; `crates/purlis-core/src/planesave.rs`, `crates/purlis-core/src/provenance.rs` |
| `[repos.<name>]` | table | optional, one per repo | **charter-app only.** How a workspace repo is saved. `<name>` is the repo's `name` in `inventory/repos.json`, so one table governs every workspace's clone of it. Takes the same keys as `[plane]` (`mode`, `branch`, `sign`, `autosave`, `autosave_after`, `assisted_by`) except `save_branch`, with the defaults `mode = "off"` (charter never commits, pushes or opens a PR for a repo until its mode says how — ADR 0051, amended 2026-09-25) and `autosave = false`. A save commits on the branch the clone is on and `pr` opens its PR from that branch into `branch` (default: the repo's `default_branch`); on that default branch, a request mode first creates `purlis/<workspace>/<short-sha>`, or carries on a `charter/<workspace>/…` branch an earlier save made, while HEAD descends from it. It never runs while a session in that workspace is mid-turn. | stable | ADR 0051; `crates/purlis-core/src/reposave.rs` |
| any other key in `[plane]` or `[repos.<name>]` | — | — | Refused by the Settings tab's save, ignored by readers. | stable | ADR 0051; `crates/purlis-core/src/planesave.rs` `refusals` |
| `[charter].version` | str | optional | The version lock. Reported **as written** (even if malformed); must match `^\d+\.\d+\.\d+$` before it is acted on. | stable | `charter/instance.py:321`, `charter/instance.py:290` |
| `[update].channel` | str | default `"stable"`; closed set `stable`,`dev` | Which purlis this plane tracks. Unknown → `stable`; the matched **constant** is stored, never the file's string. | stable | `charter/instance.py:2948`, `charter/instance.py:2936` |
| `[harness].default` | str | default `None` | What bare `purlis` launches. Matched against the harness registry's `cli_name`s; a non-match is recorded as `refused` (contained) rather than ignored. | stable | `charter/instance.py:3000`, `charter/instance.py:3088` |
| `[harness.<name>]` | table | — | **Refused here**: profiles live in `charter.local.toml`. Reported by name. | stable | `charter/profiles.py:316`, `charter/profiles.py:132` |
| `[extensions.<id>].enabled` | bool | optional; absent = this machine's answer (an approved extension is on) | **purlis only** (purlis#253, ADR 0048). Whether this project has the extension on. It cannot reach past this machine's approval: `true` for an extension this machine has not approved reads as *needs approval here* and contributes nothing. `charter.local.toml`'s value overrides this one, and in a workspace, that workspace's `settings.extensions.<id>.enabled` in `workspace.json` comes between the two (purlis#280). `<id>` is an extension's id (letters, digits, `-`, `_`, `.`, starting with a letter or digit). | stable | `crates/purlis-core/src/extension/project.rs` `resolve` |
| `[extensions.<id>.settings].<key>` | bool or str | optional; absent = the extension's declared default | **purlis only.** A value for a setting the extension's manifest declares (`bool`, `text` of at most 200 bytes, or one of a `choice`'s words), handed to its program with each question as `settings`. A key it does not declare, or a value it would not accept, is ignored with a sentence and the next file down is used. Overridden key by key by `charter.local.toml`. | stable | `crates/purlis-core/src/extension/project.rs` `resolve`, `crates/purlis-core/src/extension.rs` `Setting::accepts` |
| any other key in `[extensions.<id>]` | — | — | Refused by the Settings tab's save, and ignored by the reader. | stable | `crates/purlis-core/src/extension/project.rs` `refusals` |
| `[harness_plugins.<harness>]."<plugin id>"` | bool | optional; absent = not set (the harness decides, from its own settings) | **charter-app only** (charter-app#274, ADR 0050). Whether the chats charter starts in this project have that harness plugin on (`true`) or off (`false`). `<harness>` is a profile `kind`: `claude`, `opencode` or `codex`. `<plugin id>` is the harness's own id (`<name>@<marketplace>` for Claude Code and Codex), one line of at most 200 bytes. Only a plugin this machine has installed is handed on. `charter.local.toml`'s value overrides this one plugin by plugin, and for a chat in a workspace, that workspace's `settings.harness_plugins.<harness>."<plugin id>"` in `workspace.json` comes between the two (charter-app#282). For Claude Code the value goes into the chat's `--settings` `enabledPlugins`. For Codex and opencode it is read, shown as *not supported yet*, and handed to nothing. `charter-app@inline` cannot be `false` and `charter@charter` cannot be `true` under `claude`: a save that says so is refused, and the reader ignores it with a sentence. | stable | `crates/purlis-core/src/harness_plugin.rs` `resolve`, `chosen` |
| any other shape under `[harness_plugins]` | — | — | A harness purlis does not know, a value that is not a bool, or a `[harness_plugins]` or `[harness_plugins.<harness>]` that is not a table is refused by the Settings tab's save and ignored by the reader. | stable | `crates/purlis-core/src/harness_plugin.rs` `refusals` |
| `[sandbox].mode` | str | optional; absent = **not set**: the plane's chats run as they did before the sandbox existed | **charter-app only** (ADR 0067, program map SD-2). `"on"` is the one value a plane may give it: every chat charter starts in the plane runs in a sandbox charter compiles for its harness, or does not start. **A project `charter init` or the app makes writes `[sandbox]` with `mode = "on"` and the default `egress`** (ADR 0067 §1, ruling V21 1 and 5); a project made before that is offered it once in the window, and taking the offer writes the same block (`sandbox::local::answer`). A person may start **one chat** without it, from the window's new-chat picker only ("Start without the sandbox", ruling V78 a; no CLI word); a system with no sandbox backend (Windows) starts every chat without it (V21 3). Each such start, and the sandbox coming back on for that chat's next run, is a `trust.sandbox.off` or `trust.sandbox.on` event in the host's event log. **A plane can never carry `"off"`**: a committed file may restrict what a chat is confined to and never loosen it, so `"off"`, or any value that is not `"on"` (a typo included), is refused by the Settings tab's save and **read as `"on"`**, with a sentence. So is a `sandbox` that is not a table. A `[sandbox]` with no `mode` is not set. Only a person turns the sandbox off, for one chat. What a chat may write (its own directory and a temp directory, for a harness charter wraps whole that harness's own data, and while `toolchains` is in `egress` the project's own package caches, which purlis keeps under its data home and points cargo, npm, pip, Go, Gradle, yarn and pnpm at, never the person's own; that costs disk and a first download per project) and the denial classes (ADR 0067 §5) are not keys: no file can widen them. Read from this file only; `charter.local.toml`'s `[sandbox]` holds this machine's own `hosts` and nothing else. What it covers and what it does not is in `SECURITY.md`. | stable | `crates/purlis-core/src/sandbox.rs` `Said::of` |
| `[sandbox].egress` | array of str | optional; absent = `["model-providers", "forge", "toolchains"]` | **purlis only** (ADR 0067 §3). The named presets whose hosts a sandboxed chat may reach; a host none of them lists is refused. `forge` also holds the hosts of this file's `[[forge]]` blocks. `[]` reaches no host. A word that is not a preset is refused with a sentence and the rest are kept. SD-4 and SD-31 add presets. | stable | `crates/purlis-core/src/sandbox.rs` `Said::of`, `hosts` |
| `[sandbox].hosts` | array of str | optional; absent = `[]` | **purlis only** (ADR 0067 §1 and §3 as amended 2026-10-06; #1341). **The project's own hosts**, beside the presets: every chat of everyone who opens the project reaches them, with no approval, and each teammate's window tells them once when the list changes (`app/sandbox.json` `hosts_seen`). An entry is a domain (`api.example.com`), `*.domain` (every name under it, never the domain itself, and never a whole top-level domain), an IPv4 address or a bracketed IPv6 address (`[fd00::1]`), each with an optional `:port`; private ranges are hosts like any other (`10.100.39.145:6443`). A host without a port is reached as a preset's host is on each harness (any port on Claude Code; HTTPS's and HTTP's through purlis's egress proxy for opencode and Codex); with a port, on that port alone, on every harness. An IPv6 address that carries an IPv4 one (NAT64 `64:ff9b::/96`, IPv4-compatible `::/96`, 6to4 `2002::/16`, SIIT) is judged as that IPv4 address. Never a host, and refused with a sentence that drops the entry: this machine (`localhost`, loopback, `0.0.0.0/8`), link-local addresses, broadcast and multicast, the cloud metadata addresses Claude Code's sandbox refuses (`100.100.100.200`, `168.63.129.16`, `192.0.0.192`, `fd00:ec2::/32` and the rest), a name or wildcard ending in a number (`0x7f.1`, `*.0.0.1`: an address written another way), a range, a URL or a path, a single name with no dot, and an IPv4 address written as IPv6. An address that is one of this machine's own interface addresses is dropped when a chat starts. Through purlis's egress proxy a wildcard never matches an address, and a name is reached only at the addresses it resolves to that a chat may reach — never this machine, link-local, multicast, broadcast or metadata — checked at every connect. Changed in Settings › Sandbox (Add and Remove). A chat never writes it: the manifest is a later-code name a sandboxed chat is denied writing, and that denial is the boundary today; `sandbox::changes_a_sandbox_key` is the rule a brokered write asks once #1333 builds one. | stable | `crates/purlis-core/src/sandbox/hosts.rs` `Host::parse`, `in_force`; `crates/purlis-core/src/settings/hosts.rs` |
| `[sandbox].certificate-checks` | bool | optional; absent = `false` | **purlis only** (#1337). `true` lets a sandboxed chat on macOS ask the system's certificate check, which Go tools such as `gh` need to verify certificates; it also lets a chat reach hosts named inside a certificate, past the egress proxy, so it is off unless set. A value that is not `true` or `false` is refused with a sentence and read as `false`. | stable | `crates/purlis-core/src/sandbox.rs` `Said::of` |
| any other key in `[sandbox]`, or `sandbox` that is not a table | — | — | Refused by the Settings tab's save, and ignored by the reader. | stable | `crates/purlis-core/src/sandbox.rs` `Said::of` |
| `[theme].use` | str | optional; absent = the window's own theme (the operator's `theme.json`, else the first theme from an extension the project has on, else `charter-dark`) | **purlis only** (purlis#273, ADR 0048). The theme the window and its terminals draw while this project is in front: `charter-dark`, `charter-light`, `system` (the built-in matching the operating system's appearance, followed live), or `<extension-id>/<theme name>` — split at the first `/`. An extension's theme is drawn only while the extension is on in this project and approved on this machine, and contributes that theme; otherwise the built-in `charter-dark` is drawn and the Settings tab says why. A value of none of those shapes is ignored with a sentence and the next file down is used. `charter.local.toml`'s value overrides this one, and in a workspace, that workspace's `settings.theme.use` in `workspace.json` comes between the two (purlis#281). | stable | `crates/purlis-core/src/extension/project/theme.rs` `resolve` |
| `[theme].icons` | str | optional; absent = `charter-icons` | **charter-app only** (FM-3, #1106). The icon theme the file trees draw while this project is in front: `charter-icons` (charter's own, from Material Icon Theme, MIT), or `<extension-id>/<icon theme name>`, one an extension contributes as `contributes.icon_themes` — an object per icon theme with a `name` and a `file`, as `contributes.themes` has. Picked apart from `use`, in the same order (`charter.local.toml` over the workspace's `settings.theme.icons` over this file). An extension's icon theme is drawn only while the extension is on and approved; otherwise `charter-icons` is drawn. The file is data — symbols of path outlines coloured by the `icon.*` tokens, mapped from extensions, file names and folder names — read by `app/src/theme/icons.ts`, which keeps nothing that could run or fetch. | stable | `crates/purlis-core/src/extension/project/theme.rs` `resolve_icons` |
| any other key in `[theme]`, or `theme` that is not a table | — | — | Refused by the Settings tab's save, and ignored by the reader: `[theme]` holds `use` and `icons`. `colour` included: a colour is a workspace's (`settings.theme.colour`, purlis#281). | stable | `crates/purlis-core/src/extension/project/theme.rs` `refusals` |

#### `[frame]` — every key, via `FRAME_FIELDS` (`charter/instance.py:1652`)

Merged over `FRAME_DEFAULTS` by `frame_of` (`charter/instance.py:1994`); the section must be a
table or the defaults are returned whole (`charter/instance.py:2064`). Only the **TOML
spelling** is honoured — three keys are hyphenated; the underscore form is not an alias.
Type-checked against the default (`charter/instance.py:2147`: a `bool`/non-`bool` mismatch is
rejected first, then `isinstance(value, type(default))`).

| Field | Type | Default | Meaning | Status | Source |
|---|---|---|---|---|---|
| `slots` | list[str] | `["top","bottom","repos","right"]` | Edges to draw, **in split order (geometry)**. Filtered against `FRAME_SLOTS` (`top`,`bottom`,`repos`,`right`); an empty result is treated as "not written". | stable | `charter/instance.py:1682`, `charter/instance.py:564` |
| `density` | str | `"full"` | Preset over `slots`: `minimal`/`normal`/`full`. Expands to `slots` **only when declared and no usable explicit `slots`**. Carries a `verbosity` (`terse`/`normal`). | stable | `charter/instance.py:1692`, `charter/instance.py:628` |
| `mouse` | bool | `false` | tmux mouse reporting. | stable | `charter/instance.py:1752` |
| `chrome` | str | `"off"` | Pane surface: `off`/`dark`/`light`. Word is a key into `FRAME_CHROME`; never a style string. | stable | `charter/instance.py:1766`, `charter/instance.py:729` |
| `rules` | str | `"hidden"` | Pane-seam treatment: `hidden`/`visible`. | stable | `charter/instance.py:1788`, `charter/instance.py:1161` |
| `text` | str | `"default"` | Frame foreground; one of `FRAME_PANE_FG`'s 17 words (`default`, 8 ANSI names, 8 `bright*`). | stable | `charter/instance.py:1796`, `charter/instance.py:983` |
| `dim` | bool | `true` | Whether SGR 2 is appended to purlis's rules. | stable | `charter/instance.py:1816` |
| `ok` / `warn` / `bad` | str | `"green"` / `"yellow"` / `"red"` | Accent colours; same 17-word vocabulary as `text`. | stable | `charter/instance.py:1845` |
| `hotkey` | str | `"F2"` | Palette key. Must match `_HOTKEY_RE` **and** differ from the frame gate key, or it degrades to the default — a newline here reached tmux config text (measured RCE). | stable | `charter/instance.py:1848`, `charter/instance.py:1937`, `charter/instance.py:2134` |
| `record` | bool | `true` | Whether the frame writes the plane down as it changes. | stable | `charter/instance.py:1858` |
| `restore` | bool | `true` | Whether bare `purlis` puts the recorded plane back. | stable | `charter/instance.py:1870` |
| `history-limit` | int | `50000` | tmux scrollback. | stable | `charter/instance.py:1871` |
| `min-cols` | int | `100` | Below this, slots are dropped. | stable | `charter/instance.py:1872` |
| `min-rows` | int | `20` | Same, vertically. | stable | `charter/instance.py:1873` |

#### `[[frame.component]]` — the arrangement (`charter/instance.py:2421`)

An array of tables; the whole arrangement is **refused as one** on the first bad key
(`charter/instance.py:2400` carries the sentence, and `frame_of` stores it as
`components_refused`, `charter/instance.py:2158`). When accepted it **replaces** `slots`
(`charter/instance.py:2160`). The complete key form is
`("use","edge","size","visible","key","bg","pad")` (`charter/instance.py:2204`) — any other
key refuses.

| Field | Type | Required / default | Meaning | Status | Source |
|---|---|---|---|---|---|
| `use` | str | required, unique | Component id: a built-in (`identity`, `attention`, `repos`, `sidebar`, `personas`, `todos`, `changes`, `chats`, `workspaces` — `charter/frame/builtins.py:985`ff) or an id an installed distribution supplies. | stable | `charter/instance.py:2624`, `charter/instance.py:2802` |
| `edge` | str | required for a non-built-in; for a built-in it may only **echo** its own edge | One of `top`,`bottom`,`left`,`right` (`charter/frame/component.py:55`). | stable | `charter/instance.py:2801`, `charter/instance.py:2758` |
| `size` | int | required (≥1) for a non-built-in; for a built-in only where the built-in's size is `Content`, else it may only echo | Cells. | stable | `charter/instance.py:2811`, `charter/instance.py:2273` |
| `visible` | bool | default `true` | Whether it is drawn at launch (invisible components still get a toggle key). | stable | `charter/instance.py:2661` |
| `key` | str | optional | tmux toggle key; `toggle_key`/`_HOTKEY_RE`, and refused if already bound (palette, hatch, gate, mouse keys, another component). | stable | `charter/instance.py:2667`, `charter/instance.py:2710` |
| `bg` | str | optional | One of `FRAME_PANE_BG`'s 17 words. | stable | `charter/instance.py:2725`, `charter/instance.py:886` |
| `pad` | int | default `0`, max `5` | Horizontal inset, out of the content budget; `bool` refused. | stable | `charter/instance.py:2742`, `charter/instance.py:1046` |

---

### `charter.local.toml`

- **Format:** TOML. Hand-edited (or edited by a chat), and in purlis written by the
  Settings tab and nothing else.
- **Status:** **stable** — the operator edits it by hand, and two different processes read it
  (the CLI/`doctor`, and the frame launcher/selector on a launch).
- **Tier:** Clone state — the operator's per-clone profiles and overrides, gitignored and hand-edited, so nothing can rebuild it.
- **Written by:** nothing in the Python charter — `purlis init`/`reinit` only add the
  `.gitignore` line for it (`charter/commands.py:1803`, `charter/commands.py:1806`).
  purlis's Settings tab (`purlis_core::settings::save`, purlis#252) writes it
  as it writes `charter.toml` (above), and **creates it on the first save**, at mode 0600. It
  refuses to create or write the file where git would commit it — asked before the file exists
  with `git check-ignore -q -- charter.local.toml` (exit 1 is "would commit", and is also git's
  answer for a tracked path), and afterwards with the loader's own `ignore_check` — in the
  loader's own sentences, and refuses anything `profiles` would refuse in it.
- **Read by:** `charter/profiles.py:240` `_read_local` (the only reader), through
  `charter/profiles.py:284` `derive` and `charter/profiles.py:404` `current` (memoized per
  process on `(root, local bytes, charter.toml bytes)`, `charter/profiles.py:428`). Surfaces:
  `purlis harness list` (`charter/commands_harness.py:62`), `purlis doctor`
  (`charter/doctor.py:755`), the launcher/selector (`charter/frame/launcher.py:478`,
  `charter/frame/selector.py:25`). In purlis, its `[extensions]` table is read by
  `crates/purlis-core/src/extension/project.rs` as `charter.toml`'s is (purlis#253), and
  its `[theme]` by `crates/purlis-core/src/extension/project/theme.rs` (purlis#273), and
  its `[harness_plugins]` table by `crates/purlis-core/src/harness_plugin.rs` as
  `charter.toml`'s is (purlis#274), and its `[plane]` and `[repos.<name>]` tables by
  `crates/purlis-core/src/planesave.rs` as `charter.toml`'s are (purlis#292), and its
  `[chat_env]` table by `crates/purlis-core/src/chatenv.rs` on every chat start.
- **Git:** gitignored — the baseline writes `/charter.local.toml`
  (`charter/commands.py:1104`), and `reinit` backfills it
  (`charter/commands.py:1821`). If git *would* carry it (tracked, committable, or git cannot
  say), **every profile in it is refused** (`charter/profiles.py:518` `ignore_check`,
  `charter/profiles.py:456` `with_ignore_check`). In purlis, **nothing in it is read**
  then: its `[extensions]`, `[theme]`, `[harness_plugins]`, `[plane]`, `[repos.<name>]` and
  `[chat_env]` are left out too, and the other
  layers decide (purlis#308, ADR 0048). Every reader takes the file through
  `crates/purlis-core/src/settings.rs` `layer_text`, which applies the same check.
- **Encoding details:** only `[harness]` is read by the profiles loader, and — in purlis
  since purlis#253 — `[extensions]` by `extension::project` and, since purlis#273,
  `[theme]` by `extension::project::theme` (ADR 0048), and, since purlis#274,
  `[harness_plugins]` by `harness_plugin` (ADR 0050), and, since purlis#292, `[plane]`
  and `[repos.<name>]` by `planesave` (ADR 0051), which override `charter.toml`'s values
  **key by key** on ADR 0048's overlay, every surface that shows one naming the file that
  decided it, and `[chat_env]` by `chatenv` (ADR 0047, amendment of 2026-09-29), which has no
  counterpart in `charter.toml`; any other top-level key is refused with a sentence
  (`charter/profiles.py:325`, and in purlis `crates/purlis-core/src/profiles.rs`
  `derive_from`, whose sentence names all seven tables). A missing file declares nothing and is not a refusal
  (`charter/profiles.py:241`). Profile `env` is stored **sorted by name**
  (`charter/profiles.py:363`), and `~` in `command[0]` and in every `env` value is expanded
  only at launch (`charter/profiles.py:474`, `charter/profiles.py:480`) — never in the file
  and never in the launch record.

| Field | Type | Required / default | Meaning | Status | Source |
|---|---|---|---|---|---|
| `[harness].default` | str | optional | Which profile the selector starts on; wins over `charter.toml`'s. Kept only if it names a profile that survived validation, else recorded as `default_refused`. | stable | `charter/profiles.py:76`, `charter/profiles.py:336`, `charter/profiles.py:366` |
| `[harness.<name>]` | table | one per profile | A profile. Name must match `^[A-Za-z0-9][A-Za-z0-9_-]*$` (no dot), may not be `default`, may not collide with a `purlis` command word, and replaces a built-in of the same name (a *refused* one takes the name with it). | stable | `charter/profiles.py:69`, `charter/profiles.py:255`, `charter/profiles.py:443`, `charter/profiles.py:359` |
| `…​.kind` | str | required | The word typed after `charter`: `claude`/`codex`/`opencode` (registry `cli_name`s), or in charter-app the name of a harness the project declares in [`harnesses/<name>.toml`](#harnessesnametoml--a-harness-declaration) (ADR 0073), which a profile then also asks approval for. | stable | `charter/profiles.py:257`; `crates/purlis-core/src/profiles.rs` `refusal` |
| `…​.command` | list[str] | required, non-empty, all non-empty strings | argv. Never a shell string — no shell runs it. Refused if its first word is purlis itself. | stable | `charter/profiles.py:261`, `charter/profiles.py:445` |
| `…​.env` | table of str→str | optional, default `{}` | Environment for the harness process. A name starting `CHARTER_` is refused; a name containing `KEY`/`TOKEN`/`SECRET`/`PASSWORD` (case-insensitive) is refused. | stable | `charter/profiles.py:265`, `charter/profiles.py:89`, `charter/profiles.py:58` |
| any other key in a profile table | — | — | Refuses that profile (e.g. `enviroment`). | stable | `charter/profiles.py:86`, `charter/profiles.py:278` |
| `[extensions.<id>].enabled` | bool | optional | **purlis only** (purlis#253, ADR 0048). This machine's choice for this project, over `charter.toml`'s. Same shape and rules as there; still cannot reach past this machine's approval. | stable | `crates/purlis-core/src/extension/project.rs` `resolve` |
| `[extensions.<id>.settings].<key>` | bool or str | optional | **purlis only.** Overrides `charter.toml`'s value for the same key, key by key; falls through to it (then to the declared default) when the extension would not accept this one. | stable | `crates/purlis-core/src/extension/project.rs` `resolve` |
| `[theme].use` | str | optional | **purlis only** (purlis#273, ADR 0048). This machine's pick of the project's theme, over `charter.toml`'s and over a workspace's `settings.theme.use` (purlis#281). Same values and rules as there; falls through to the next layer's pick when it is none of the shapes. | stable | `crates/purlis-core/src/extension/project/theme.rs` `resolve` |
| `[plane].<key>`, `[repos.<name>].<key>` | as in `charter.toml` | optional | **purlis only.** This machine's value, over `charter.toml`'s, key by key. Same shapes and refusals. | stable | ADR 0051; `crates/purlis-core/src/planesave.rs` `Settings::from_text` |
| `[sandbox].hosts` | list[str] | optional, default `[]` | **purlis only** (#1341, ADR 0067 §1 as amended: the **You** level). Hosts this machine's chats in this project reach besides the project's, written and checked as `charter.toml`'s `[sandbox].hosts` are. Read only while git leaves this file alone and never through a link, and **each host grants only once you confirmed it in Settings on this machine** (`app/sandbox.json` `hosts_mine`): adding it there confirms it, and a host the file holds that you did not add there (a chat's edit, a file from elsewhere) is listed "not yet confirmed", with Confirm. A sandboxed chat can write all of a clone's git state, so no check of git can tell your host from another's; the record is in the integrity class, which no sandboxed chat writes. Any other key of `[sandbox]` here (`mode`, `egress`) is refused with a sentence and read as absent: whether chats run sandboxed, and the presets, are the project's. Changed in Settings › Sandbox › Your hosts. | stable | `crates/purlis-core/src/sandbox/hosts.rs` `personal`; `crates/purlis-core/src/sandbox.rs` `refusals` |
| `[chat_env].pass` | list[str] | optional, default `[]` | **purlis only** (ADR 0047, amendment of 2026-09-29). More of this machine's own environment every chat started in this plane is given, beyond the built-in keep-list and what the chat's harness declares (`crates/purlis-core/src/chatenv.rs` `PASSED`, `Harness::env_passed`). Each entry is a variable's name (letters, digits and `_`, not starting with a digit), or such a name ending in `*` for a prefix. A credential-class name (forge, cloud, model-provider or registry credential, or a name holding `KEY`/`TOKEN`/`SECRET`/`PASSWORD`) passes only by its exact name, never by a prefix. `OP_*`, a vault's declared identity variables, a harness's identity, `CHARTER_WORKSPACE`, `CHARTER_PLANE_ROOT_SESSION` and `TERM` are never passed, whatever is listed. An entry of another shape, a `pass` that is not a list, or another key in the table is refused with a sentence and passes nothing. No counterpart in `charter.toml`. | stable | `crates/purlis-core/src/chatenv.rs` `from_text`, `refusals`, `inherited` |

---

### `.charter/harness-profiles-launched.json`

- **Format:** JSON object, `{ "<profile name>": {"kind": str, "command": [str], "env": {str: str}} }`.
- **Status:** **stable** — it lives under `.charter/` and is written and read by *different*
  processes (a launch, a frame keypress under a hook, a `purlis reopen`), and it decides
  whether a command runs. It is not safe to delete in the "no consequence" sense: deleting it
  makes every declared profile ask for approval again (it fails towards asking, never towards
  running — `charter/profiletrust.py:129`).
- **Tier:** Clone state — the operator's consent to run a profile's command. Deleting it makes every profile ask again.
- **Written by:** `charter/profiletrust.py:155` `record_launched` — `config.replace_for`
  (atomic temp+rename, private 0600) of `json.dumps({**_read(), name: fingerprint}, indent=2) + "\n"`
  (`charter/profiletrust.py:172`). Triggered by approving a profile at the prompt
  (`charter/profiletrust.py:303`) and by a launch that runs one. **In purlis** (#430)
  it is written by `rewrite::replace` at `0600`, which also flushes the directory after the
  rename and **refuses a record that is a symlink** (Python's rename replaces the link); the
  consent is then not recorded, and the profile asks again.
- **Read by:** `charter/profiletrust.py:138` `_read` → `last_launched`
  (`charter/profiletrust.py:144`), `approval_needed` (`charter/profiletrust.py:195`),
  `refusal` (`charter/profiletrust.py:306`), the frame launcher and `purlis reopen`.
- **Git:** gitignored (inside `/.charter/`, `charter/commands.py:1096`).
- **Encoding details:** `indent=2`, trailing newline, `ensure_ascii` default (true); key order
  is insertion order of the existing document with an updated key keeping its position; `env`
  is a dict built from the already-sorted `Profile.env` pairs (`charter/profiletrust.py:126`).
  Whole-file read-modify-write with **no lock** — two concurrent approvals can lose one. Mode
  0600 and the directory 0700 via `config.private_mkdir`/`replace_for`
  (`charter/config.py:389`, `charter/config.py:595`).
  The fingerprint is **as declared, before `~` expansion** (`charter/profiletrust.py:121`).
  A built-in profile is never recorded and never asks (`charter/profiletrust.py:187`).

### `harnesses/<name>.toml` — a harness declaration

- **Format:** TOML, one file per harness, named after the harness it declares. **purlis
  only** ([ADR 0073](adr/0073-a-harness-is-declared-as-data-and-a-chat-runs-it-at-one-of-three-levels.md), FD-14).
  Hand-edited and committed.
- **Status:** **stable** — it is committed and travels to every clone, and it decides which
  program a chat runs.
- **Tier:** Plane — a harness this project runs that purlis does not ship, committed so every clone has it.
- **Written by:** nothing in purlis. The operator writes it, or a chat does, and every clone
  approves it before it runs (below).
- **Read by:** `crates/purlis-core/src/harness_declaration.rs` `read`, which every launch,
  `purlis harness list`, `purlis harness show <name>`, `purlis doctor` and the app's
  new-chat picker go through (`profiles::derive`). Each file is opened with no link on the
  way, must be a plain file of at most 64 KiB, and is read whole. A file whose name does not
  end `.toml` is not a declaration and is skipped.
- **Git:** committed.
- **What it gives:** a profile named after the harness, of `kind` `<name>`, running its
  `program` (`purlis harness list` shows it FROM `harnesses`), and a `kind` a profile in
  `charter.local.toml` may name. **A declared harness runs at level 1**: in its terminal, with
  none of purlis's hooks armed, because level 2 needs an adapter purlis ships (V24c). A
  project that turns the sandbox on does not start one, because nothing compiles a sandbox for
  a harness without an adapter yet (ADR 0067 §1, SD-2).
- **Approval:** a declared harness's program runs on this machine only after the operator
  approves the declaration, and again after any change to its bytes (V24b), recorded in
  [`.charter/harness-declarations-approved.json`](#charterharness-declarations-approvedjson).
  The new-chat picker asks, and **shows every word that will run** (V66): the profile's line,
  the declaration's file and whole digest, its program, and each template's words, escaped
  (`profiletrust::shown`), with a warning beside any word or value that names a file or folder at the project root. The approval records the digest of exactly what was shown: a
  declaration that changed between the dialog and the click is refused and nothing is
  recorded, and a launch reads the declarations once and judges and runs that one read, so a
  change between the check and the start is not what runs (`start::ready_in`). A profile in
  `charter.local.toml` of a declared kind asks for both its own command and the declaration.
- **Built-ins:** Claude Code (`claude`), opencode (`opencode`) and Codex (`codex`) are
  declarations in this format that purlis ships
  (`crates/purlis-core/src/harness_declaration/claude.toml`, `opencode.toml`, `codex.toml`),
  read by the same reader. They are not files in the project, and **a project never takes a
  built-in's name**: a built-in's declaration decides how its chats are armed (ADR 0073 §3).
  `purlis harness show <name>` prints any of them, under a comment line naming its file and
  digest. Each line is printed escaped, so where a file holds a byte outside printable ASCII
  the text printed is not the bytes the digest is of, and a second comment line says so.
- **Decided, not yet read:** a declaration in `charter.local.toml`, for this machine alone or
  replacing a built-in's (ADR 0073 §5, [#968](https://github.com/purlis/purlis/issues/968)).
- **Refusals:** a declaration purlis will not read gives no profile and is listed with the
  refused profiles, under its file, in one sentence. The first failure wins. Every key below
  is the whole schema: a key it does not list refuses the file.

| Field | Type | Required / default | Meaning | Status | Source |
|---|---|---|---|---|---|
| `name` | str | required | The word a profile's `kind` names. Letters, digits, `_` and `-`, starting with a letter or digit, and the file's own name. Not a built-in's. | stable | `crates/purlis-core/src/harness_declaration.rs` `parse`, `read` |
| `title` | str | default `name` | What the operator calls it: the harness card's label, *What <title> can do here* (HP-19). **One line of at most 60 characters with no control, bidi or zero-width character**, and in a project's declaration never the title of a harness purlis ships (ignoring case). | stable | same, `MOST_TITLE`, `drawn_refusal` |
| `program` | str | required | **A bare program name found on `PATH`**: letters, digits, `.`, `_`, `+` and `-`, starting with a letter or digit, at most 128 characters. Never a path, `~`, a flag or a shell string. **Never a shell, an interpreter or a launcher** (`sh`, `bash`, `env`, `node`, `python3`, `npx`, `osascript`, `arch`, `xcrun`, `tmux`, `git`, `vim`, version managers such as `mise` and `pyenv`, and the like, matched without case; V66, V66c). A renamed or linked interpreter is not caught by its name; the approval dialog, which shows every word, is the control for that case. **Never a program of a harness purlis ships** either, which runs only armed, through its adapter. A profile in `charter.local.toml` can point a chat at a path on this machine. | stable | same, `bare_word`, `launcher` |
| `env` | list[str] | default `[]` | The harness's own environment namespace: a variable's name in capitals, digits and `_`, or a prefix ending `_*` (`AIDER_*`). Refused: anything reaching `CHARTER_*`; a name holding `KEY`/`TOKEN`/`SECRET`/`PASSWORD`; anything that could reach a credential family `chatenv::CREDENTIALS` lists (a forge's, a cloud's, a model provider's or a registry's: `AWS_*`, `GITHUB_*`, `OPENAI_*`, `ANTHROPIC_*`, `GEMINI_*`, …), in either direction of a prefix; and anything reaching what changes how a process loads, runs or finds programs (`LD_*`, `DYLD_*`, `NODE_OPTIONS`, `PYTHONPATH`, `PATH`, `GIT_*`, …). **Decided, not yet read** at launch: a chat on a declared harness is started with the built-in keep-list and `[chat_env] pass` until [#967](https://github.com/purlis/purlis/issues/967) passes it. | stable | same, `env_ok` |
| `tested` | str | optional | The harness versions these facts were measured on, as written; a fact on the harness card. TS1 settles what reads it. **One line of at most 80 characters with no control, bidi or zero-width character.** | stable | same, `MOST_TESTED` |
| `login` | str | optional | Where the operator logs in, said when a profile of this kind sets a variable named like a credential. | stable | same; `profiles::refusal` |
| `[session].chosen_by` | `"harness"` or `"purlis"` | default `"harness"` | Who chooses a new session's id. `"purlis"` needs `new` to hand over `{id}`; `"harness"` may not. | stable | same, `session` |
| `[session].new` | list[str] | default `[]` | The words that start a new session. **Every word after the program has an allowed shape** (V66c), here and in `resume` and `acp`: a flag, `-x` or `--name` (letters, digits and `-`); `--name=value` with a plain value (letters, digits and `-_.:,+`, no leading `.`, no drive letter such as `C:`, and no `%`, so no percent-encoded separator), where `{id}` and `{name}` may sit (`--session={id}`, `--title={name}`); or a subcommand, lowercase letters, digits and `-` with no dot (`exec`, `run`). A bare `{id}` or `{name}` word only in a built-in. Everything else is refused: a path anywhere, a bare relative filename, `@file`, a value glued to a short flag (`-I.`), a `VAR=value` assignment, shell syntax or a space, another placeholder. Every word, and every flag's value, is also checked against the shells, interpreters, launchers and built-in programs `program` refuses. **A residual the approval covers** (ruling of 2026-10-03): a value or a subcommand-shaped word can still name a file the program resolves in the chat's folder (`--cfg=rel.toml`); where it names a file or folder at the project root, the approval dialog says so beside it (*`--cfg=rel.toml` names the file rel.toml in this project; the program may read it*). | stable | same, `template`, `word_refusal` |
| `[session].resume` | list[str] | optional | The words that bring session `{id}` back; must name `{id}` (`--resume={id}`). The same shapes as `new`. | stable | same |
| `[session].named_by` | list[str] | default `[]` | The flags or subcommands by which the operator names a session in a profile's own command (`--resume`, or `--resume=<id>`), so purlis adds none of its own. | stable | same |
| `[terminal].newline` | str | optional | The bytes the harness reads as a new line in its input (ESC CR, `"\u001b\r"`). Absent: the terminal's own Enter. **Decided, not yet read** for a declared harness ([#967](https://github.com/purlis/purlis/issues/967)). | stable | same, `terminal` |
| `[terminal].paste_drawn_whole` | inline table | optional | `{ lines = N, chars = N }`, the biggest paste it draws whole, each at least 1; a key left out has no limit. | stable | same |
| `[terminal].ready_to_type` | `"on-start"`, `"raw-and-quiet"` or `"never"` | default `"never"` | When a curation prompt may be typed into a new chat (ADR 0061). `"on-start"` waits for a hook, so only a harness with hooks may say it. | stable | same |
| `[levels].terminal` | bool | default `true` | Level 1. Every declaration offers it; `false` is refused. | stable | same |
| `[levels].hooks` | bool | default `false` | Level 2. `true` only in a built-in: a project declaration that says it is refused (V24c). | stable | same |
| `[levels].acp` | list[str] | optional | Level 3 over ACP: the argv that starts the harness's ACP agent. Its first word follows `program`'s rules and the rest `new`'s shapes. The built-in opencode declaration names `["opencode", "acp"]`, which HP-2's client (`purlis_core::acp`) runs; a chat starts at level 3 from a declaration once #1076 wires it in. | stable | same |
| `[capabilities].<name>` | str | optional | A harness capability (ADR 0073 §6): `"yes"`, `"no: <the reason>"` or `"unknown"`; the reason is drawn on the harness card, so it is **one line of at most 200 characters with no control, bidi or zero-width character**. One left out is unknown, which purlis treats as no. The names: `reports_its_process`, `reports_its_start_before_the_first_prompt`, `keeps_conversations_by_directory`, `reports_waiting`, `resumes_by_id` (`"yes"` needs `[session].resume`), `per_chat_plugins`. A ticket that reads a new one adds it here. | stable | same, `CAPABILITIES` |

---

### `.charter/harness-declarations-approved.json`

- **Format:** JSON object, `{ "<harness name>": {"digest": "sha256:<hex>"} }`. **purlis
  only** (ADR 0073 §5, V24b).
- **Status:** **stable** — it records an operator's consent to run a committed declaration's
  program on this clone, and it decides whether a chat starts.
- **Tier:** Clone state — the operator's consent to run a project's harness declaration on this clone. Deleting it makes every declaration ask again.
- **Written by:** `crates/purlis-core/src/profiletrust.rs` `approve`, from the new-chat
  picker's approval, through the same gated write as
  [`.charter/harness-profiles-launched.json`](#charterharness-profiles-launchedjson):
  `rewrite::replace` at `0600`, refusing a record that is a link or resolves outside the
  project.
- **Read by:** `profiletrust::declaration_approval_needed`, on every launch of a profile whose
  `kind` is a project's declaration, with the same gates as the profile record: no link on the
  way, a plain file, at most 1 MiB. Every unreadable state reads as "nothing approved".
- **Git:** gitignored (inside `/.charter/`).
- **Encoding details:** the digest is of the declaration file's **bytes**, so any change to the
  file, a comment included, asks again. Kept apart from the profile record because that one is
  keyed by profile and records a command, which a declaration does not have (ADR 0073 §5).
  Read-modify-written whole with no lock, as the profile record is.

### `.charter/unattended-logins.json`

- **Format:** **decided, not yet written**
  ([ADR 0087](adr/0087-a-chats-model-is-one-choice-from-four-sources-and-charter-never-carries-a-harness-login.md) §3, SD-28).
  JSON object keyed by harness name, then by the login as the harness reports it, each holding
  when the operator acknowledged it. It holds no credential and no token.
- **Status:** **stable** — the operator's one-time acknowledgement that a harness's subscription
  login may run unattended (a trigger, a schedule, a workflow, a race, headless) in this project.
  Deleting it makes the next unattended `login` run ask again; it fails towards asking.
- **Tier:** Clone state — the operator's consent on this clone, like the profile record above.
- **Written by:** `purlisd`, from the operator's answer on a human scope (SD-28). Chats are
  denied it.
- **Git:** gitignored (inside `/.charter/`).

---

### `.gitignore` (plane root)

- **Format:** plain text, line-oriented, append-only from purlis's side.
- **Status:** **stable** — committed, hand-edited, and it carries two literal anchors other
  purlis code depends on.
- **Tier:** Plane — committed.
- **Written by:**
  - `charter/commands.py:1116` `_ensure_gitignore` (from `cmd_init`): writes
    `_GITIGNORE_BASELINE` verbatim when the file is absent (`charter/commands.py:1129`),
    otherwise appends only the missing whole lines through the one shared appender.
  - `charter/util.py:472` `append_gitignore` — the single writer for additions. It appends
    `"\n\n"`-separated: existing body `rstrip("\n") + "\n\n"`, then `# <header>\n`, then one
    line each (`charter/util.py:499`). Headers seen: ``added by `purlis init` ``
    (`charter/commands.py:1152`), ``added by `purlis guard --local` ``
    (`charter/commands.py:1792`), ``added by `purlis reinit` — harness profiles stay on this machine``
    (`charter/commands.py:1821`), ``added by `purlis browser install` `` (`charter/browser.py:173`).
  - `charter/workspace.py:1408` `_write_live_block` rewrites the managed live-workspace block
    (markers `charter/workspace.py:1338`, `charter/workspace.py:1339`) — **workspaces area**,
    noted here only because it splices at the `!/workspaces/.gitkeep` anchor.
- **Read by:** `charter/workspace.py:1346` `live_workspaces`, the presence checks in
  `charter/commands.py:1134`, `charter/util.py:494`, plus git itself (the real consumer).
- **Git:** committed.
- **Encoding details:** the baseline is a fixed here-doc (`charter/commands.py:1087`–`:1113`),
  comments included, and the exact lines matter:
  - `/workspaces/*/*` (`charter/commands.py:1091`) and `!/workspaces/.gitkeep`
    (`charter/commands.py:1092`) — the second is the **literal anchor**
    `workspace.set_live` splices the managed block after (`charter/workspace.py:1414`).
  - `/.charter/` (`charter/commands.py:1096`)
  - `/.claude/settings.local.json` (`charter/commands.py:1100`, constant at
    `charter/commands.py:1771`)
  - `/charter.local.toml` (`charter/commands.py:1104`, constant at `charter/commands.py:1803`)
  - `/sessions/` — **purlis only** (ADR 0064): the plane root's session records stay on
    this machine. In the baseline under its own comment, added by `init` when missing, and by
    `reinit` under ``added by `purlis reinit` — the plane root's session records stay on this
    machine``. Deleting the line shares them.
  - `__pycache__/`, `*.py[cod]`, `.venv/`, `.DS_Store` (`charter/commands.py:1107`–`:1112`)
  - Presence detection is **whole-line, stripped**, except `.charter/` which is a substring
    test on the body (`charter/commands.py:1136`) — both quirks are deliberate and recorded.
  - Plain `write_text`; no atomic write, no lock. **In purlis** (#358) every write of
    the plane's `.gitignore` (`init`, `reinit`, the LIVE block) is a temp file beside it and
    one rename, under the same `flock` on the plane root as `charter.toml`; a `.gitignore`
    that cannot be read as UTF-8 text is refused rather than rewritten from nothing. The
    same holds for `.gitattributes`.

---

### `.gitattributes` (plane root)

- **Format:** git attributes, line-oriented, inside a managed block with the markers
  `# >>> charter merge rules (managed by charter) >>>` and `# <<< charter merge rules <<<`,
  or, in a project migrated to purlis names (its manifest is `purlis.toml`, or it requires
  `purlis-names`), `# >>> purlis merge rules (managed by purlis) >>>` and
  `# <<< purlis merge rules <<<`. Either pair is read as the block, and the block is rewritten
  in place under the pair the project writes, never left beside a second one (V93g, V93i).
  Lines outside the block are the operator's own.
- **Status:** **stable**. It is committed, and git reads it.
- **Tier:** Plane — committed.
- **Written by:** purlis's `init` and `reinit`, through
  `crates/purlis-core/src/scaffold/mod.rs` `ensure_gitattributes` (purlis#295, ADR 0051):
  the block is replaced where it is, or added at the end, and every other line is kept.
- **Read by:** git, on every merge and rebase a save makes.
- **Git:** committed.
- **Encoding details:** the block holds exactly these lines, in this order. Each names a file
  that only ever grows by whole lines, so git's `union` driver keeps both sides of a conflict:

  ```
  personas/_dispatch/*.jsonl merge=union
  personas/_skills/*.jsonl merge=union
  workspaces/*/pieces/*.jsonl merge=union
  workspaces/*/changes/log/*.jsonl merge=union
  workspaces/*/work/*.jsonl merge=union
  personas/*/memory/MEMORY.md merge=union
  workspaces/*/memory/MEMORY.md merge=union
  ```

  Any other conflict leaves the plane **blocked** (ADR 0051).

---

### Baseline directories: `personas/`, `inventory/`, `workspaces/`

- **Tier:** Plane — committed (through `.gitkeep` where empty).
- **Format:** directories. **Status: stable** — `instance.drift` reports their absence and
  `purlis reinit` heals it, and their names are part of the layout a second implementation
  must resolve.
- **Written by:** `charter/commands.py:2907` `_create_baseline_dirs` (shared by `cmd_init`
  `charter/commands.py:2685` and `cmd_reinit` `charter/commands.py:2934`) over
  `instance.BASELINE_DIRS` (`charter/instance.py:504`). Plain `mkdir(parents=True,
  exist_ok=True)`, never tightened (they are committed content). A baseline path occupied by a
  **file** is reported as blocked and the command exits 1 — purlis never deletes or renames.
- **Read by:** `charter/instance.py:507` `drift`, `charter/doctor.py:441`
  `check_control_plane_schema`, and every consumer of `PERSONAS_DIR`/`WORKSPACES_DIR`/`INVENTORY`.
- **Git:** committed (empty dirs are not, in practice — note below).
- **Defect worth recording:** nothing creates `workspaces/.gitkeep`, although the baseline
  `.gitignore` names it as a negation and as the splice anchor (`charter/commands.py:1092`).
  Measured on a fresh `purlis init`: `workspaces/` is created empty and no `.gitkeep` is
  written, so an empty `workspaces/` does not survive a clone. Only
  `personas/<front-door>/{memory,refs}/.gitkeep` is written (`charter/commands.py:2680`).
  **Fixed in purlis** (#355): `init` and `reinit` create an empty `workspaces/.gitkeep`
  when it is absent, and never truncate one that is there. It is reported as part of
  `workspaces/`, or on its own (`+ workspaces/.gitkeep`) when `workspaces/` already existed.

---

### `personas/<front-door>/` (what `purlis init` scaffolds at the plane root)

- **Status:** **stable** (committed). The persona file *format* is the personas area's; what
  belongs here is only that `init` creates it and declares it.
- **Tier:** Plane — committed.
- **Written by:** `charter/commands.py:2651` `_ensure_front_door`, called from `cmd_init`.
  Default name `steward` (`charter/cli.py:114`), `--front-door NAME` renames it,
  `--no-front-door` suppresses it (`charter/cli.py:117`).
  - Skipped entirely if the plane already has **any** persona (`personas/*/persona.md` or the
    legacy `personas/*.md`) or if `[persona] default` is already declared
    (`charter/commands.py:2669`).
  - Writes `personas/<name>/persona.md` from the `_FRONT_DOOR` template
    (`charter/commands.py:2597`), with `{name}` and a title-cased `{role}` derived from the
    name (`-`/`_` → space, `.title()`), then `memory/.gitkeep` and `refs/.gitkeep`
    (`charter/commands.py:2680`), then `instance.set_default_persona`
    (`charter/commands.py:2681`).
- **Git:** committed.

---

### `inventory/repos.json`

- **Format:** JSON object.
- **Status:** **stable** — tracked in git, shared across machines, and read by commands,
  `doctor` and the status line.
- **Tier:** Plane — committed.
- **Written by:** `charter/inventory.py:292` `save` (only writer), called from
  `charter/commands.py:172` (`purlis discover`). **The Rust purlis only adds to it**
  (ADR 0055): `inventory::add` merges records into what the file lists, `discover` keeps every
  listed repo its run did not see unless an `exclude` names it, and the app's repo picker adds
  the repos it clones that the file does not list yet, leaving listed records as they are. The
  Python replaced the whole list on every `discover`.
- **Read by:** `charter/inventory.py:71` `load` → `charter/inventory.py:173` `repos`
  (`charter/commands.py:218`, `:366`, `:641`, `:880`; `charter/statusline.py:792`;
  `charter/doctor.py:3674`), and `charter/inventory.py:265` `find`.
- **Git:** committed (nothing ignores `inventory/`).
- **Encoding details:** `json.dumps(doc, indent=2, ensure_ascii=False) + "\n"` via
  `write_text` (`charter/inventory.py:315`) — not atomic, no lock; the parent directory is
  created first (`charter/inventory.py:314`). Records are **sorted by `name`**
  (`charter/inventory.py:304`), and `merge` sorts the same way
  (`charter/inventory.py:262`). Deliberately **no generated-at timestamp**
  (`charter/inventory.py:295`). Records whose `source` is `"plane"` are stripped before
  writing (`charter/inventory.py:303`, `charter/inventory.py:81`) — the plane's own repo is
  derived at read time from `git remote get-url origin` (`charter/inventory.py:97`), never
  persisted. Key order is the literal dict order below.

| Field | Type | Required / default | Meaning | Status | Source |
|---|---|---|---|---|---|
| `group` | str | required | `config.GROUP` at write time (first `[[forge]]`'s group/owner). | stable | `charter/inventory.py:306` |
| `count` | int | required | `len(repos)`. | stable | `charter/inventory.py:307` |
| `note` | str | required | Fixed sentence naming the group and `purlis discover`; absent from the empty skeleton `load` returns. | stable | `charter/inventory.py:308` |
| `repos` | array of record | required | The repos, sorted by `name`. | stable | `charter/inventory.py:312` |
| `repos[].name` | str | required | Bare name; the on-disk clone directory. Must pass `contain.segment_ok` at merge or the row is dropped. | stable | `charter/commands.py:86`, `charter/inventory.py:239` |
| `repos[].path_with_namespace` | str | required | `<owner>/<name>`; the identity half of `(forge, path)`. | stable | `charter/commands.py:87` |
| `repos[].ssh_url` | str | required | SSH clone URL as the forge reports it; rewritten to HTTPS at clone time. | stable | `charter/commands.py:88` |
| `repos[].default_branch` | str | default `"main"` | Branch announced and cloned. | stable | `charter/commands.py:89` |
| `repos[].kind` | str | derived | `classify_kind(name)`: `workspace`/`docs`/`frontend`/`service`/`api`/`core`/`app`. | stable | `charter/commands.py:90`, `charter/inventory.py:21` |
| `repos[].stack` | str | derived; `"unknown"` | `classify_stack(root file list)`; `"unknown"` also means the probe failed (a warning, not a field). | stable | `charter/commands.py:91`, `charter/inventory.py:39` |
| `repos[].description` | str | `""` | Forge description, stripped. | stable | `charter/commands.py:92` |
| `repos[].topics` | array | `[]` | Forge topics. | stable | `charter/commands.py:93` |
| `repos[].web_url` | str | `""` | Forge HTML page, no `.git`. Used to build the HTTPS clone URL. | stable | `charter/commands.py:94` |
| `repos[].forge` | str | defaults to the querying forge's `kind` | Which forge the record came from; `gitlab` when absent (legacy). | stable | `charter/commands.py:97`, `charter/forge/registry.py:18` |
| `repos[].source` | str | only on the derived plane repo, value `"plane"` | Marks a record purlis derived rather than discovered; stripped before save. | internal (never persisted) | `charter/inventory.py:164`, `charter/inventory.py:303` |

---

### `docs/topology.md`

- **Format:** Markdown, fully generated.
- **Status:** **stable** — committed, and written by one process to be read by people (and by
  anything regenerating it: a byte-different render is a spurious diff).
- **Tier:** Plane — committed.
- **Written by:** `charter/commands.py:224` (`purlis docs` / `purlis docs generate`, and the
  tail of `purlis discover` unless `--no-docs`), body from `charter/render.py:43`
  `topology_md`; `DOCS_DIR` is created first (`charter/commands.py:223`). Refuses to write at
  all when the inventory is empty (`charter/commands.py:219`).
- **Read by:** people; `purlis status` points at it (`charter/commands.py:918`).
- **Git:** committed.
- **Encoding details:** `render.topology_md(doc) + "\n"` — the renderer already ends with an
  empty element, so the file ends with a blank line then newline. Fixed structure
  (`charter/render.py:45`–`charter/render.py:63`): the `BANNER` comment
  (`charter/render.py:7`), `# Repository Topology`, a count line
  `**N repos** in the `<group>` <label>.`, a blockquote, then a 5-column table
  (`| Repo | Kind | Stack | Branch | Description |`) with one row per repo **sorted by name**
  (`charter/render.py:44`). `<label>` is `"GitLab group"`/`"GitHub org"` only when every record
  agrees on one forge kind, else the neutral `"group"` (`charter/render.py:26`). Cells escape
  `|` and flatten newlines (`charter/render.py:17`). No timestamp anywhere.

---

### `README.md` — the generated persona roster block

- **Format:** Markdown block spliced into a hand-written file between two literal markers.
- **Status:** **stable** — committed, and the markers are a contract with a hand-written file.
- **Tier:** Plane — committed.
- **Written by:** `charter/commands.py:331` `refresh_readme_personas`, called from
  `purlis docs` (`charter/commands.py:229`); body from `charter/render.py:76` `personas_md`,
  splice by `charter/render.py:149` `splice_personas`. Plain `write_text`
  (`charter/commands.py:356`), only when the result differs.
- **Read by:** people. The markers are `<!-- BEGIN personas — GENERATED by `charter docs`; do
  not edit by hand. -->` (`GENERATED by `purlis docs`` in a project migrated to purlis names;
  either is found, and the block is written under the project's; V93g, V93i) and
  `<!-- END personas -->`
  (`charter/render.py:14`); **absent markers mean purlis writes nothing**
  (`charter/render.py:154`), which is the case on purlis's own plane today.
- **Git:** committed.
- **Encoding details:** rows sorted by `(-dispatches, name)` (`charter/render.py:99`), a
  12-cell unicode bar (`charter/render.py:68`), an optional mermaid pie when any dispatch is
  recorded, and a `⚑` legend only when a flag appears. The counts come from the dispatch
  tally and each persona's memory directory — the **personas area** owns those.

---

### `purlis browser install` — three more plane-root paths

`purlis browser` installs a vendor CLI into the plane, and a plane reader meets its
directories at the root. None is purlis's own format; all three are named because an
enumerator that does not expect them has to guess.

- **Tier:** None — the vendor CLI's files, not purlis's; `.playwright-cli/` is gitignored output the vendor rewrites.
- **`.claude/skills/playwright-cli/`** (`SKILL_DIR`, `charter/browser.py:79`) — the pages the
  vendor's generator writes, read by Claude Code as project skills. **stable** in the sense
  that matters here: a harness reads it. Written by the generator, not by purlis.
- **`.playwright/`** (`CONFIG_DIR`, `charter/browser.py:96`) — created by `install` itself
  (`initWorkspace`), holding `.playwright/cli.config.json`. Project configuration. purlis
  deliberately says nothing about whether a plane commits it (ADR 0017), and writes no
  `.gitignore` line for it.
- **`.playwright-cli/`** (`OUTPUT_DIR`, `charter/browser.py:90`) — the vendor's output
  directory: traces under `.playwright-cli/trace`, plus snapshots, screenshots and PDFs.
  Created when something is written there, never by `install`. purlis **does** append
  `.playwright-cli/` to the plane's `.gitignore` (`ensure_output_ignored`,
  `charter/browser.py:151`, header at `charter/browser.py:173`) because a trace carries
  authenticated network traffic — the one of the three purlis takes a position on.

### Plane-root files that exist but are **not** this area

Named so the assembled doc does not lose them: `vaults.json` (committed shared vault registry,
`charter/config.py:774` — vaults area), `.claude/settings.json` and `opencode.json` at the
plane root (written by `purlis init`'s harness wiring — `charter/commands.py:2296`,
`charter/commands.py:2321`, `charter/commands.py:2098`, `charter/commands.py:1233` — harness
area), `workspaces/*` (workspaces area), `personas/*` internals (personas area),
`.charter/*` other than the profile launch record (runtime-state areas). The plane's own
`.claude/skills/**`, `.opencode/agent/**` and `.codex/skills/**` are committed plane content
that purlis copies into a clone rather than authors; they are described where that mirror is
(`charter/harness/claude_code.py:53`).

---

## Workspaces

Everything Python charter reads or writes under `workspaces/`, plus the three places
outside it that decide or record a workspace's state: the plane's `.gitignore` managed
block, the guest checkouts' `.git/info/exclude` block, and the `.charter/` pointers.

All citations are against the clone at commit `50d31dc`.

Two rules hold for the whole area and are not repeated per file:

- **Name rule.** A workspace name is `instance.workspace_name_ok` — `contain.segment_ok`
  plus `^[A-Za-z0-9][A-Za-z0-9._-]*$` (`charter/instance.py:181`, `charter/instance.py:199`),
  asked through `workspace.valid_name` (`charter/workspace.py:211`). Every name read off
  disk is re-checked before it is joined onto a path.
- **Writer helpers.** `config.write_for` (`charter/config.py:504`) writes whole,
  `config.replace_for` (`charter/config.py:595`) writes atomically through a temp named
  `<target>.<pid>.<12 random hex>.tmp` (`charter/config.py:592`, `TEMP_SUFFIX = ".tmp"` at
  `charter/config.py:552`), `config.create_for` (`charter/config.py:517`) creates with
  `O_EXCL` only where nothing is at the name. Files under `.charter/` come out 0600/0700
  (`config.private_mkdir`, `charter/config.py:190`); files inside `workspaces/` keep the umask's mode, because `write_for` dispatches on where the
  path is (`charter/config.py:504`).

---

### `workspaces/`

- **Format:** directory.
- **Status:** stable — the operator's clones live here, and the plane's `.gitignore` names
  it literally.
- **Tier:** Plane when LIVE, Clone state when LOCAL — `workspaces/.gitkeep` is committed; what is inside follows each workspace's LIVE or LOCAL.
- **Written by:** `purlis init` (`charter/commands.py:1091` writes `/workspaces/*/*` and
  `!/workspaces/.gitkeep` into `.gitignore`); the directory itself by
  `workspace.ensure` → `wd.mkdir(parents=True)` (`charter/workspace.py:1313`).
- **Read by:** `workspace.read_workspaces` (`charter/workspace.py:914`),
  `workspace.legacy_flat_clones` (`charter/workspace.py:1290`), `gitpolicy.scan` via
  `charter/commands.py:3268`.
- **Git:** the directory is committed only through `workspaces/.gitkeep`; everything two
  levels down is ignored by `/workspaces/*/*` unless the LIVE block un-ignores it.
- **Encoding details:** a one-time migration renames a legacy `repos/` to `workspaces/`
  (`workspace._ensure_layout`, `charter/workspace.py:53`). An entry whose name starts with
  `.` is never a workspace (`charter/workspace.py:926`); an entry that is itself a clone
  (a `.git` directory) is not a workspace either (`charter/workspace.py:928`).
  `purlis init` does **not** create `workspaces/default/` — the first command that puts
  something in it does; the name is always listable anyway (`charter/commands_workspace.py:116`).

### `workspaces/.default`

- **Format:** plain text — one workspace name, `\n`-terminated.
- **Status:** stable — committed (the default ignore rule `/workspaces/*/*` does not match
  a file directly under `workspaces/`, `charter/workspace.py:508`), hand-editable, and read
  by every process that resolves a workspace.
- **Tier:** Plane — committed.
- **Written by:** `workspace.set_declared_default` (`charter/workspace.py:512`, through
  `contain.writable`), from `purlis workspace default <name>`
  (`charter/commands_workspace.py:669`). Removed by `workspace.clear_declared_default`
  (`charter/workspace.py:524`) for `--clear`.
- **Read by:** `workspace.declared_default` (`charter/workspace.py:487`), which is a rung of
  `workspace.chosen` (`charter/workspace.py:646`) and of `workspace.source`
  (`charter/workspace.py:676`).
- **Git:** committed.
- **Encoding details:** exactly `name + "\n"` (`charter/workspace.py:512`). On read the
  value is `.strip()`ed and re-checked with `valid_name`; a `contain.file_refusal` (symlink,
  FIFO, oversized) makes it read as absent (`charter/workspace.py:488`).

### `workspaces/<ws>/`

- **Format:** directory.
- **Status:** stable — it *is* the workspace.
- **Tier:** Plane when LIVE, Clone state when LOCAL — a LIVE workspace's charter, memory, todos, sessions and changes are committed; a LOCAL one's stay in this clone and are backed up as clone state (FR-10).
- **Written by:** `workspace.ensure` (`charter/workspace.py:1313`), which then calls
  `scaffold` best-effort (`charter/workspace.py:1315`). Reached from `workspace create`,
  `workspace use`, `purlis clone` (`charter/commands.py:385`), `restore`, `fork`, and every
  frame launch.
- **Read by:** everything in this area.
- **Git:** the directory entry is ignored by `/workspaces/*/*`.
- **Encoding details:** `scaffold` refuses the whole directory when it does not resolve
  inside `workspaces/` (`charter/workspace.py:1840`). Baseline creation order is
  memory → refs → workspace.md → workspace.json → harness layer → structure stamp
  (`charter/workspace.py:1851`–`1885`), each path skipped when `_baseline_answers` says it
  cannot be checked or something is in the way (`charter/workspace.py:1849`).
- **Its four stores, `memory/`, `todos/`, `sessions/` and `changes/`, are written with no link on
  the way (V74, #1064).** Every writer, purlis's MCP tools, the `purlis` commands and the window
  alike, and every reader except those #1083 still tracks, opens `workspaces`, the workspace and
  the store one directory at a time without following a link, and then uses each file through the
  directory it holds, never by its path. A store, or a file or directory in one, that is a link is
  never followed by them, even when it lands inside the project's data directories: a write that
  would go through it is refused and writes nothing, and a reader leaves the entry out. A link
  that leaves the project is refused in the words it always was. A path below `workspaces/` that
  does not name `<ws>/<store>` plainly (a `..` in it, say) is refused rather than used by path.
  The MCP tools refuse a store holding any link whole; a command leaves such an entry out and goes
  on, so a link a chat plants cannot stop the operator recording a todo. The commands and the
  window wait a few seconds at most for a store's lock: one a chat keeps is answered with a
  refusal, not a command that never returns. The readers #1083 tracks (the doctor's memory rows,
  the persona and briefing counts, and the landing and pending logs) still open what they list by
  path, under the containment rule. On a platform without directory descriptors (Windows) the MCP
  tools are not offered, and the commands and the window reach the stores by path: a link that
  leaves the project's data directories is refused there, and one that stays is followed.

### `workspaces/<ws>/workspace.md` — the living charter

- **Format:** Markdown; purlis parses and rewrites `## ` sections, no frontmatter.
- **Status:** stable — hand-edited, committed for a LIVE workspace, read by the SessionStart
  digest, the status line/tab strip (vision cell) and `handoff`.
- **Tier:** Plane when LIVE, Clone state when LOCAL
- **Written by:** `workspace.scaffold_charter` (`charter/workspace.py:4360`, template at
  `charter/workspace.py:4295`) and `workspace.set_vision` (`charter/workspace.py:4370`);
  commands `workspace create --vision`, `workspace vision "…"`
  (`charter/commands_workspace.py:1613`). Copied wholesale by `fork`
  (`charter/commands_workspace.py:1689`). **In purlis, also a project template** (FR-17,
  `Workspace::seed_sections`): when the first run takes a repo into a workspace, each `## `
  section of the template's starter replaces the same section while that section still holds
  exactly what purlis's own template (above) first wrote there; today the starters have
  only `## Context & decisions`. A section with anything else in it is left as it is.
- **Read by:** `workspace.read_charter` (`charter/workspace.py:4373`), `read_vision`
  (`charter/workspace.py:4441`), `_vision_cell` (`charter/commands_workspace.py:64`),
  `last_active` (`charter/workspace.py:4421`).
- **Git:** gitignored unless LIVE (`!/workspaces/<ws>/workspace.md`,
  `charter/workspace.py:1397`).
- **Encoding details:**
  - Created only when absent, with `config.create_for` (`charter/workspace.py:4360`); a
    file that exists is never overwritten, only its `## Vision` body replaced.
  - Template: `# <name>`, a 4-line block quote, then `## Vision`, `## Context & decisions`,
    `## Glossary`, `## Log` (`charter/workspace.py:4295`–`4323`). **purlis adds
    `## Sessions` before `## Log`** (ADR 0064), whose body is
    `_No session records yet — a chat's Smart close writes one, and purlis keeps this line
    pointing at them._` until the first record.
  - `## Sessions` is **purlis's one line**, rewritten through the section replacement below
    every time a record is written (`sessionrecord::point`): `<N> session record(s) — the
    latest is [<title>](sessions/<file>) (<YYYY-MM-DD HH:MM>); all of them, newest first, in
    [sessions/index.md](sessions/index.md).` A file without the section gets it appended. The
    other sections are the operator's and the smart-close skill's. The unset vision body is
    the placeholder at `charter/workspace.py:4290` (`_Not set yet — …`).
  - Section replacement: `_replace_md_section` (`charter/workspace.py:4326`) keeps the
    `## <header>` line, replaces everything to the next line starting `"## "` or EOF with
    `"", body.strip(), ""`, rstrips the file and adds one trailing `\n`; a missing section is
    appended as `\n## <header>\n\n<body>\n`. The header match is
    `^##\s+Vision\s*$`, case-insensitive (`charter/workspace.py:4332`).
  - Vision extraction: `^##\s+Vision\s*$(.*?)(?=^##\s|\Z)` with `MULTILINE|DOTALL`, body
    stripped, and a body starting `_Not set yet` reads as unset
    (`charter/workspace.py:4441`–`4444`).
  - Reads are refused (`""`) when the *directory* or the file fails containment
    (`charter/workspace.py:4391`).

### `workspaces/<ws>/workspace.json` — the committed manifest

- **Format:** JSON object, `json.dumps(doc, indent=2) + "\n"` (`charter/workspace.py:1612`).
- **Status:** stable — committed for LIVE workspaces, restored on other machines, and
  hand-editable (`manifest_owner` exists precisely to tell purlis's copy from a hand's).
- **Tier:** Plane when LIVE, Clone state when LOCAL
- **Written by:** `workspace._write_manifest` (`charter/workspace.py:1589`) via
  `write_manifest` (`charter/workspace.py:1577`, the deliberate writer: `snapshot`, `fork`,
  `rename`), `scaffold_manifest` (`charter/workspace.py:1652`, birth) and `record_members`
  (`charter/workspace.py:1684`, a clone landing — `charter/commands.py:477`).
  Commands: `workspace create`/`use`/`clone` (through `ensure`),
  `workspace snapshot` (`charter/commands_workspace.py:942`),
  `workspace fork` (`charter/commands_workspace.py:1719`),
  `workspace rename` (`charter/workspace.py:1488`; in purlis `wscmd::rename`, which
  rewrites `name` and keeps the owner: a manifest purlis stamped is stamped again, one a hand
  wrote stays unstamped),
  `workspace reinit` (backfill, `charter/workspace.py:4645`).
  **In charter-app, also the Settings tab's Workspace level** (`purlis_core::settings::workspace`,
  charter-app#280, NO-7 #1232): a setting's change to `settings` (`save`), or the whole text
  from its Edit as JSON link (`save_text`). Both refuse to write over a manifest that changed on
  disk since the tab read it, refuse what the settings readers would refuse that the manifest did
  not already hold, and refuse a value `secretshape` calls a credential; Edit as JSON also refuses
  text that is not a JSON object. Neither changes who owns the manifest: one a hand wrote is
  written unstamped (Edit as JSON's text byte for byte), and one charter wrote, or a first one,
  is stamped again.
- **Read by:** `workspace.read_manifest` (`charter/workspace.py:1500`), `manifest_owner`
  (`charter/workspace.py:1556`), `restore` (`charter/commands_workspace.py:955`), `fork`
  (`charter/commands_workspace.py:1709`), `merge_repo_rows` (`charter/workspace.py:1739`),
  `last_active` (`charter/workspace.py:4421`). In purlis, its `settings` are read by
  `crates/purlis-core/src/extension/project.rs` (`Choices::read_in`) for Settings at the
  Workspace level, the window's filter on extension panels and views for the focused workspace, and
  the executor's gate for a view on that workspace's strip (purlis#280); by
  `crates/purlis-core/src/harness_plugin.rs` (`Choices::read_in`) for Settings at the Workspace
  level and for every chat started in the workspace (purlis#282); and its `settings.theme`
  by `crates/purlis-core/src/extension/project/theme.rs` (`Said::read_in`, `colour_of`) for
  Settings at the Workspace level, the theme the window draws while the workspace is in front, and
  every workspace tab's colour (purlis#281).
- **Git:** gitignored unless LIVE (`!/workspaces/<ws>/workspace.json`,
  `charter/workspace.py:1397`); it is the first path of the managed block.
- **Encoding details:**
  - Atomic: `config.replace_for` through a pid+random temp beside the file
    (`charter/workspace.py:1612`), path first run through `contain.writable`
    (`charter/workspace.py:1609`), which **raises** on a symlink out of the plane.
    **In purlis** (#430) through `rewrite::replace`: the file keeps the mode it has
    (Python's comes back at the umask's), a read-only one and one that is itself a symlink
    are refused, and the directory is flushed after the rename.
  - Key order is insertion order of the dict the writer built, with `charter_generated`
    appended last (`charter/workspace.py:1611`). The birth document's order is
    `name, description, repos, updated_at, updated_by` (`charter/workspace.py:1677`);
    `snapshot` and `fork` mutate a read document, so a manifest that already existed keeps
    the order it had on disk, and `fork` inserts `forked_from` after `name`
    (`charter/commands_workspace.py:1713`–`1718`).
  - `repos` rows are sorted by name whenever purlis adds one
    (`charter/workspace.py:1729`); `snapshot` writes them in `clones()` order, which is
    already sorted (`charter/workspace.py:1248`).

| Field | Type | Required / default | Meaning | Status | Source |
|---|---|---|---|---|---|
| `name` | string | required; the workspace's own name | identity; rewritten by `rename` | stable | `charter/workspace.py:1677`, `charter/workspace.py:1487` |
| `description` | string | `""` | free text, set by `snapshot --description` | stable | `charter/commands_workspace.py:937` |
| `repos` | list of objects | `[]` | membership, and optionally pinned branches | stable | `charter/workspace.py:1677` |
| `repos[].name` | string | required | clone directory name under the workspace; untrusted, gated by `contain.child` on restore | stable | `charter/workspace.py:1649`, `charter/commands_workspace.py:985` |
| `repos[].branch` | string | **absent** for every row purlis writes itself; set by `snapshot` | the branch `restore` checks out; a row without one restores at the default branch | stable | `charter/commands_workspace.py:939`, `charter/commands_workspace.py:1014` |
| `updated_at` | string | required | UTC ISO-8601, `timespec="seconds"`, e.g. `2026-09-17T14:01:35+00:00` | stable | `charter/workspace.py:1616`, `charter/commands_workspace.py:940` |
| `updated_by` | string | required | `$USER` or `"unknown"` for automatic writes (`charter/workspace.py:1633`); `git config user.name` for `snapshot`/`fork` (`charter/commands_workspace.py:860`) | stable | `charter/workspace.py:1679` |
| `forked_from` | string | present only on a fork | the source workspace | stable | `charter/commands_workspace.py:1714` |
| `settings` | object | **absent** in every manifest written before purlis#280, and in one whose workspace sets nothing; absent = the project's answer | **purlis only** (purlis#280, ADR 0048). The workspace's layer of the project's settings, read between `charter.toml` and `charter.local.toml`. Its keys mirror those files' tables — see [`settings`](#settings--a-workspaces-layer) below. Every writer keeps it: `snapshot`, `fork` (which inherits it) and a clone's `record_members` mutate the document they read. | stable | `crates/purlis-core/src/settings/workspace.rs` |
| `charter_generated` | string | written on every purlis write | sha256 of the rest of the document, canonically serialised. In a project migrated to purlis names the key is `purlis_generated`, and a stamp renames `charter_generated` in place; either is read, `purlis_generated` first (V93g, V93i) | stable | `crates/purlis-core/src/manifest.rs` |

**`charter_generated` computation** (a Rust writer must match it byte for byte), the same over
either key name, so a digest moved to `purlis_generated` still matches:
`sha256(json.dumps({k: v for k, v in doc.items() if k not in ("purlis_generated", "charter_generated")}, sort_keys=True).encode("utf-8")).hexdigest()`
— body at `charter/workspace.py:1540`, digest at `charter/workspace.py:1945`. Note
`sort_keys=True` and Python's default separators (`", "`, `": "`) for the *digest* input,
while the file on disk is `indent=2` in insertion order. Ownership: a document whose
stamp matches is `"charter"` (the `purlis_generated` one when both keys are there); anything else present is `"operator"` and the
automatic writers leave it byte for byte alone; absent is `"absent"`
(`charter/workspace.py:1556`–`1574`). A present-but-unparseable file is `"operator"`.

#### `settings` — a workspace's layer

**purlis only** (purlis#280, ADR 0048). A workspace refines its project for the team:
for each key, the order is this machine's approval, then `charter.toml` (Shared), then this
object, then `charter.local.toml` (Local). A workspace's settings are committed with the
manifest when the workspace is LIVE, and stay on this machine when it is not. Old manifests
have no `settings` and read exactly as before: the project's answer.

The object mirrors the TOML files' tables, so one reader reads all three: JSON `true`/`false`
is a TOML bool, a string is a string, an object is a table. **`null` reads as not set**, so the
next layer down answers.

| Key | Type | Meaning | Source |
|---|---|---|---|
| `settings.extensions.<id>.enabled` | bool | Whether this workspace has the extension on, over `charter.toml`'s `[extensions.<id>] enabled` and under `charter.local.toml`'s. It cannot reach past this machine's approval. | `crates/purlis-core/src/extension/project.rs` `resolve` |
| `settings.extensions.<id>.settings.<key>` | bool or str | A value for a setting the extension declares, over Shared's and under Local's, key by key. A value it would not accept is ignored with a sentence and the next layer down is used. | `crates/purlis-core/src/extension/project.rs` `resolve` |
| any other key in `settings.extensions.<id>` | — | Refused by the Workspace level's save, and ignored by the reader, in the words it refuses `[extensions]` in a TOML file. | `crates/purlis-core/src/extension/project.rs` `refusals_in` |
| `settings.harness_plugins.<harness>."<plugin id>"` | bool | **charter-app#282, ADR 0050.** Whether the chats purlis starts in this workspace have that harness plugin on or off, over `charter.toml`'s `[harness_plugins.<harness>]` and under `charter.local.toml`'s, plugin by plugin. A chat is in the workspace when its directory is under `workspaces/<ws>/`. Everything the TOML key says holds here: only a plugin this machine has installed is handed on, Codex and opencode show it as *not supported yet* and hand it to nothing, and `charter-app@inline` cannot be `false` nor `charter@charter` `true` under `claude`. | `crates/purlis-core/src/harness_plugin.rs` `resolve`, `for_start` |
| any other shape under `settings.harness_plugins` | — | Refused by the Workspace level's save, and ignored by the reader, in the words it refuses `[harness_plugins]` in a TOML file, with the key named at `settings.harness_plugins…`. | `crates/purlis-core/src/harness_plugin.rs` `refusals_in` |
| `settings.theme.use` | str | The theme the window and its terminals draw while this workspace is in front (purlis#281): the same values as `charter.toml`'s `[theme].use`, over it and under `charter.local.toml`'s. An extension's theme is drawn only while that extension is on in this workspace, so a workspace that turns it off draws the built-in `charter-dark` and says why. A value of none of the shapes is ignored with a sentence and the next layer down is used. | `crates/purlis-core/src/extension/project/theme.rs` `resolve` |
| `settings.theme.icons` | str | The icon theme the file trees draw while this workspace is in front (FM-3, #1106): the same values as `charter.toml`'s `[theme].icons`, over it and under `charter.local.toml`'s. | `crates/purlis-core/src/extension/project/theme.rs` `resolve_icons` |
| `settings.theme.colour` | str | The workspace's colour (purlis#281): one of `red`, `orange`, `yellow`, `green`, `teal`, `blue`, `purple`, `pink`, or `#rrggbb`, whose hue is taken. **A workspace's alone**: the project's files cannot set one, and no other layer overrides it. It picks no theme; the window tints the accent, the focus ring and this workspace's tab and chat strip shades of the theme it draws with the hue, at the same luminance, and leaves text and the terminal as they are. Any other value is ignored with a sentence, and the workspace has no colour. | `crates/purlis-core/src/extension/project/theme.rs` `resolve`, `colour_of`; the tint, `app/src/theme/tint.ts` |
| any other key in `settings.theme`, or `settings.theme` that is not an object | — | Refused by the Workspace level's save, and ignored by the reader: a workspace's theme holds `use`, `icons` and `colour`. | `crates/purlis-core/src/extension/project/theme.rs` `refusals_in_workspace` |
| any other key in `settings`, or `settings` that is not an object | — | Refused by the save, and ignored by the reader: a workspace's settings hold `extensions`, `harness_plugins` and `theme` and nothing else. | `crates/purlis-core/src/settings/workspace.rs` `refusals` |

**Written by** Settings at the Workspace level (`settings::workspace::save`, SE-20), which changes only
`settings`: every other key keeps its place and value, a key removed takes every object it
leaves empty with it (so removing the last setting gives the manifest back as it was), and the
`settings` key itself keeps its place. **It keeps the manifest's owner**: a manifest purlis
wrote is stamped again (`charter_generated`), one a hand wrote is written unstamped, so the
automatic writers keep leaving it alone. A workspace with no manifest gets the one
`scaffold_manifest` would write, with the settings in it. The save is refused when the file
changed since the tab read it, for what the reader would refuse (what the file already held
excepted), and for a secret-shaped value, named by its kind.

### `workspaces/<ws>/memory/` — the task journal

- **Format:** directory of Markdown files plus a `MEMORY.md` index.
- **Status:** stable — committed for LIVE workspaces, hand-editable, read by the SessionStart
  briefing, `purlis recall`, `doctor` and `curate`.
- **Tier:** Plane when LIVE, Clone state when LOCAL
- **Written by:** `workspace.scaffold_memory` (`charter/workspace.py:1817`) →
  `memstore.ensure_index` (`charter/memstore.py:86`); `workspace.remember`
  (`charter/workspace.py:4231`) → `memstore.write` (`charter/memstore.py:101`);
  `workspace.forget_memory` (`charter/workspace.py:4257`) → `memstore.forget`
  (`charter/memstore.py:452`); `curate.apply_safe` via `workspace optimize`
  (`charter/commands_workspace.py:724`); `memstore.archive` (`charter/memstore.py:503`).
  Commands: `workspace remember|note`, `workspace forget`, `ws todo done` (writes a closing
  memory, `charter/commands_workspace.py:1516`), `workspace optimize --apply`, `fork` (copy).
  **In purlis, also** `workspace edit <slug>` (`memstore::edit`, an edit in place — see
  [Editing and archiving a memory](#editing-and-archiving-a-memory-purlis)) and
  `workspace archive|unarchive <slug>` (`memstore::archive_one`, `memstore::unarchive`),
  and the window's memory tab, which calls the same functions (ADR 0065). **In purlis,
  also** the first run's *Memory from the repo* tab (FR-18a,
  `purlis_core::repoinstructions::import`): a clone's `CLAUDE.md`, `AGENTS.md` and
  `.cursor/rules/**/*.{md,mdc}`, one memory each through `remember_titled`, titled
  `<file> from <repo>` with the file's text as it is, and only on the tab's press. It keeps no
  store of its own: a file is already imported when a memory's body holds its text.
- **Read by:** `memstore.files`/`entries`/`search` (`charter/memstore.py:169`, `289`, `367`),
  `workspace.recall` (`charter/workspace.py:4244`), `recall.py`, `doctor` (index drift),
  `last_active` (`charter/workspace.py:4424`).
- **Git:** gitignored unless LIVE; the block un-ignores both `memory` and `memory/**`
  (`charter/workspace.py:1398`) — two lines, because un-ignoring the directory alone does
  not re-include its files.

#### `workspaces/<ws>/memory/MEMORY.md`

- **Tier:** Plane when LIVE, Clone state when LOCAL
- **Encoding details:** created once, `config.create_for`, with the header at
  `charter/workspace.py:1802` formatted with the workspace name, and a trailing `\n`
  guaranteed (`charter/memstore.py:97`). Entries are **appended** as
  `- [{title}]({filename})\n` (`charter/memstore.py:144`), in `"a"` mode — no rewrite, so
  the order is the order memories were written. **purlis escapes `\`, `[` and `]` in the
  title** (`\\`, `\[`, `\]`; SI-9d), so a title that holds `](b.md)` is not read as the end of
  its own line's link; the Python charter wrote the title as it was, and both shapes are read. A
  deletion filters the lines containing `({filename})` and rewrites the file joined with `\n`
  plus one trailing `\n` (`charter/memstore.py:489`); **purlis filters, of the lines
  starting `- [`, only those whose leading element links `{filename}`** — the link that closes
  the `- [..]` the line starts with, brackets the title balances and escaped characters not
  counting — so a line whose title mentions another memory's file survives that memory's
  deletion. A line of any other shape is filtered as purlis filters it. Index links are recognised by
  `\(([A-Za-z0-9][\w.-]*\.md)\)` (`charter/memstore.py:214`), every match anywhere in the file;
  **purlis reads a line that starts `- [` by the same leading link a deletion reads it by,
  and nothing else in it** (SI-9e), so a title that mentions `(b.md)` does not list `b.md` — the
  line of a memory unarchived beside it is appended, and `optimize` counts it as unindexed until
  it is. A line of any other shape, or whose leading element never closes, is read by purlis's
  pattern. Every purlis append to the index — `write`, `unarchive`, `optimize --apply`'s
  repair and the legacy `notes.md` line below — runs under the store directory's
  `rewrite::Lock`, which an edit's retitle and a deletion also hold (SI-9d, SI-9e, SI-9f): an
  append between a rewrite's read and its replace went with the old file.
  A legacy `notes.md` is grandfathered into the index once, as
  `- [Task memo (legacy)](notes.md)` (`charter/workspace.py:1824`). purlis appends it when the
  index text holds no `(notes.md)` anywhere; **purlis appends it when the index does not
  list `notes.md` by the reading above** (SI-9f), so a title that mentions `(notes.md)` no
  longer stands in for the memo's own line.

#### `workspaces/<ws>/memory/<YYYYMMDD-HHMMSS>-<slug>.md`

- **Tier:** Plane when LIVE, Clone state when LOCAL
- **Encoding details:**
  - Filename: `now.strftime("%Y%m%d-%H%M%S-")` + `slug(title)` + `.md`
    (`charter/memstore.py:119`–`121`). **The stamp is LOCAL time** — `datetime.datetime.now()`
    with no timezone (`charter/memstore.py:65`).
  - Slug: lowercase, every run of `[^a-z0-9]+` → `-`, stripped of leading/trailing `-`,
    truncated to 48 chars, `"note"` when empty (`charter/memstore.py:23`, `26`–`28`).
  - Collision: `-2`, `-3`, … appended before `.md` while the path exists
    (`charter/memstore.py:122`–`124`).
  - Title: the first non-blank line of the body, stripped, capped at `TITLE_MAX = 72`
    (`charter/memstore.py:33`, `charter/memstore.py:36`), or an explicit `--title`.
  - Body: `"# {title}\n\n_{now:%Y-%m-%d %H:%M} · {kind}_\n\n{text}\n"`
    (`charter/memstore.py:132`); `kind` is `"persistent"` for workspace memories (the
    default, `charter/memstore.py:102`). The text is `.strip()`ed; empty raises.
  - Closed todos: `ws todo done <slug>` writes a memory whose text is
    `f"Closed todo: {title}"` before deleting the todo
    (`charter/commands_workspace.py:1516`–`1517`), so its filename is
    `<stamp>-closed-todo-<slug-of-the-title>.md`.
  - `memory/archive/<name>.md` holds retired memories, moved by `memstore.archive`
    (`charter/memstore.py:510`, `charter/memstore.py:524`); it is out of every listing
    because listings take only `*.md` directly in the directory
    (`charter/memstore.py:208`).
  - `memory/notes.md` is the pre-v2 single-log memo, still read by
    `workspace.read_notes` (`charter/workspace.py:4271`) and never written any more.

#### Editing and archiving a memory (purlis)

**purlis only**; the Python charter has no edit and no unarchive. These are the rules for
every memory store — a workspace's journal, a persona's `memory/` and `personas/_shared/memory/`
— and change no file's shape: an edited memory is a file `memstore.write` could have written,
and an archived one is where `memstore.archive` would have put it (ADR 0065).

- **An edit rewrites one memory file in place.** New title and new text; the **filename does not
  change** — the slug was minted from the first title and is how every command names the memory,
  so retitling it renames nothing. The file becomes
  `# {title}\n\n{stamp line}\n\n{text}\n`, where the stamp line is the one the file already had,
  **verbatim** (its date and its `kind` both kept: an edit does not re-date a memory); a file
  written by hand with no stamp line gets none. The title is stripped and capped at
  `TITLE_MAX = 72`, an empty one is the text's first line (as `write` derives one), and a title
  holding a line break is refused. An empty text is refused, as `write` refuses it. The file is
  replaced whole (`rewrite::replace`), under the store's mode rules.
- **A memory is named exactly.** Edit, archive and unarchive act on the file whose name is the
  slug given (with `.md`), and on no other: `deploy` never reaches `prod-deploy.md` (SI-9d). The
  command line's verbs first turn a typed slug into that name by the lookup `ws todo done` uses
  — an exact name, else the one file whose name ends `-<slug>.md` — and **refuse a slug more
  than one file ends in**, naming them; `forget` and `ws todo done|forget` refuse it too, where
  purlis took the first in sorted order. `MEMORY` is the index and is refused as a slug.
- **The index line is retitled, not moved.** Every line of `MEMORY.md` whose leading
  `- [..](..)` element links `{filename}` (the rule a deletion reads it by, above) has that
  element replaced by `- [{title}]({filename})`, the title escaped; anything after the link
  stays, and the line keeps its place. No line is added for a memory the index
  never listed. The rewrite is the same whole-file replacement as a deletion's.
- **An edit is checked against the file as it was read.** The caller hands back the file's whole
  text as it read it; when the file on disk differs, the edit is refused as **stale** and nothing
  is written, so a window never saves over a change it did not see. An explicit overwrite skips
  the check. The check and the write happen under the store directory's `rewrite::Lock`.
- **Archiving moves the file to `<store>/archive/<filename>` and drops its index line**, exactly
  as `memstore.archive` does (a taken name gets `-2`, `-2-3`, …). Archiving a memory that is
  already in `archive/` under its own name is not an error and changes nothing.
- **Unarchiving moves `archive/<name>` back into the store and appends its index line**
  (`- [{title}]({filename})`, title read as every reader reads one) unless the index already
  lists it. It is restored under its own name, or under a name the caller gives — how an undo
  puts back a memory that archiving had to number. A name the store already holds is refused and
  nothing moves; a memory that is already back, and no longer in `archive/`, is not an error.
- **Neither is a deletion.** `archive/` is still committed and still out of every listing; a
  hard delete is `forget`, and only from the CLI.

#### Moving a memory between scopes (purlis)

**purlis only** (KN-3, #715). A memory moves from one store to another — a workspace's
journal, a persona's `memory/` or `personas/_shared/memory/` — through `memscope::move_memory`,
which the window's Move and `purlis workspace move-memory <slug> --to-…` / `purlis persona
move-memory <name> <slug> [--shared] --to-…` call. No file changes shape: a moved memory is a
file `memstore.write` could have written in its new store.

- **The file is renamed, never copied, and its text is not touched.** Its `# title` and its
  stamp line go with it byte for byte, and no copy is left in the store it left.
- **Its name is its slug in the new store's spelling.** In a persona's store or `_shared` it is
  the name without a journal's `YYYYMMDD-HHMMSS-` prefix. In a journal it keeps its own name when
  it has that prefix already (from another journal); otherwise the prefix is its stamp line's
  date and time with seconds `00`, so the journal lists it where it was recorded, and the time
  of the move only for a file with no stamp line. **A memory moved out of a journal and back
  has its first name again to the minute, not to the second**: the stamp line holds minutes,
  so `20260302-091437-a-fact.md` comes back as `20260302-091400-a-fact.md`, its text byte for
  byte. A memory moved between persona stores, or between journals, keeps its name exactly.
- **Its index line moves with it**: `- [{title}]({filename})` is appended to the new store's
  index (made with that store's header when it has none: the journal's own, or `# Memory Index`
  for a persona's, as `remember` makes one) and dropped from the old one's.
- **Refused, with nothing moved:** a target that already holds a memory of the same name once a
  journal's prefix is taken off either; the store it is in already; a workspace or persona the
  project does not have; a store or an index a link takes out of the project, or an index that
  is not a file; and a store the filesystem will not let purlis write. Both stores are locked
  (`rewrite::Lock`'s flock) for the whole move, in the order of their paths, and reached by
  descriptor without following a link. **Nothing is made until every check has passed**: a
  target store that is not there yet is made only for the rename, and taken away again, empty,
  when the rename fails.
- **The rename never replaces anything** — `renameat2(RENAME_NOREPLACE)` on Linux,
  `renameatx_np(RENAME_EXCL)` on macOS, a plain `renameat` after a look on a filesystem that has
  neither — so a file a writer that does not take the lock put at the name meanwhile is left
  alone and the move is refused. An index append that fails after the rename renames the file
  back the same way; when that fails too, the refusal says the memory stayed in the new store
  with no index line.
- **Persona and shared memory are published with the project.** A memory moved out of a LOCAL
  workspace's journal into a persona's store or `_shared` goes with the project's next save.

### `workspaces/<ws>/todos/`

- **Format:** the same per-file memory store, with its own header. One file per todo plus
  `MEMORY.md`.
- **Status:** stable — committed for LIVE workspaces, listed by `ws todo`, counted by the
  status line and by `workspace remove`'s warning, and inherited by `fork`.
- **Tier:** Plane when LIVE, Clone state when LOCAL
- **Written by:** `todos.scaffold` (`charter/todos.py:73`), `todos.add`
  (`charter/todos.py:93`); commands `purlis ws todo "<text>"`
  (`charter/commands_workspace.py:1462`), `ws todo done|forget <slug>` (deletes —
  `charter/commands_workspace.py:1519`), `fork` (directory copy,
  `charter/commands_workspace.py:1702`).
- **Read by:** `todos.open_todos` (`charter/todos.py:219`), `todos.count_open`
  (`charter/todos.py:236`), `todos.search` (`charter/todos.py:232`),
  `todos.duplicate_of` (`charter/todos.py:133`), `last_active`
  (`charter/workspace.py:4424`).
- **Git:** gitignored unless LIVE — `todos` and `todos/**`
  (`charter/workspace.py:1399`), matched by `_ws_meta_paths`
  (`charter/commands_workspace.py:1118`).
- **Encoding details:** exactly the memory-file rules above (`todos.add` calls
  `memstore.write(..., timestamped=True, index=True)`), so a todo file is
  `<YYYYMMDD-HHMMSS>-<slug>.md` holding `# <title>`, `_<YYYY-MM-DD HH:MM> · persistent_`,
  the body. The index header is at `charter/todos.py:31`. **There is no state field**:
  closing deletes the file and its index line (ADR 0004/0006,
  `charter/commands_workspace.py:1469`). The slug a caller closes by is the file stem, and
  it must be one path segment (`charter/commands_workspace.py:1498`). Ordering is the
  filename's second resolution (whole seconds, ties broken alphabetically,
  `charter/todos.py:93`, `charter/memstore.py:119`). Age comes from the in-body `_YYYY-MM-DD` stamp, falling back
  to the filename prefix (`charter/memstore.py:150`).

### `workspaces/<ws>/work/<device>.jsonl` — the work link log

- **Format:** JSON Lines, one object per line, append-only, one file per device: `<device>` is
  this device's id from the machine store (ADR 0066), never a hostname. Each line is written
  whole, in one write, so git's `union` driver always keeps whole lines.
- **Status:** **written** ([ADR
  0088](adr/0088-a-work-items-identity-is-its-tracker-key-and-the-project-records-its-links.md),
  V40, FW-5 #732). Absent in a workspace nothing has linked or promoted.
- **Tier:** Plane when LIVE, Clone state when LOCAL — committed with a LIVE workspace, as its todos are, because FI6's links must reach the operator's other devices; unlike the piece claim log and the landing log, which stay in their clone.
- **Written by:** `purlis_core::work::log` (`append`, and `append_alias`, which refuses an
  alias that would close a cycle). Its writers today:
  - `purlis ws todo promote`: a `promoted` alias (`purlis_core::work::promote`);
  - the app's host, for the window: a chat's link and unlink (`log::link_chat` and
    `log::unlink_chat`, called by `chat_work_link` and `chat_work_unlink` in
    `app/src-tauri/src/worklinks.rs`), which the window offers as **Link to work item…** and
    **Unlink work item** on a chat tab's menu and in the palette (V60). The line goes in this
    device's log of the workspace the window files the chat under, and names the chat by its
    ULID, which V43 keeps across a relaunch and a move and mints again in a copy. Linking a chat
    to the item it already works on, after both are resolved, writes nothing when that
    workspace's log holds the link, and writes a line when another workspace's does, so this
    workspace's items hold it. An unlink names the item the chat's link resolves to, and a chat
    with no link writes nothing. A chat at the project root, or working outside the project, is
    refused both (ADR 0088 §4).

  **A line is never dated before the last line of the log it goes in**, so a clock that stepped
  back cannot reorder one device's own lines. **A chat's link or unlink is also dated at least
  one second after the last line any log holds for that chat.** The fold orders by `(ts, device
  file, line index, workspace)`, so without that a line in the same second as the chat's link
  in another workspace's log could sort before it, whatever line each sits on. After the line
  is written the logs are folded again. A line synced from another device in that moment can
  still undo it, and that is reported as refused, with what the chat works on now, never as
  done.

  The Work list's workspace links, `workspace rename`'s `renamed` aliases and the item cache's
  `moved` aliases (FW-7) are **decided, not yet written**.
- **Read by:** `purlis_core::work::log::fold`, for the Work list (`work::list::of`, which the
  board, FW-9, will draw), `purlis ws todo` (which finishes closing a promoted todo whose
  close a crash cut short), and `purlis doctor`'s `work links` row, shown only when there is
  something to report: a todo a promote aliased but did not close, skipped lines (named by
  file), or an alias cycle.
- **Git:** committed when LIVE: the LIVE block holds `!/workspaces/<n>/work` and
  `!/workspaces/<n>/work/**`, a LIVE workspace's staged paths hold `work`, and the
  `.gitattributes` block holds `workspaces/*/work/*.jsonl merge=union`. Gitignored when LOCAL, by
  `/workspaces/*/*`. `workspace fork` does not copy it.
- **Encoding details:** each line is one of four closed key sets, `v` being `1` and `ts` UTC
  ISO-8601 seconds (`2026-10-02T08:12:00Z`); `item`, `from` and `to` are tracker keys (ADR 0088
  §1) and `chat` a chat's ULID in its canonical spelling. A line with any other key set, `v`,
  `op`, `cause` or `ts`, or a key or chat id that does not parse, is skipped and counted.
  Readers fold every file in every workspace's `work/`, sorted by `ts`, then file name, then
  line, a tie that leaves going to the workspace's name (ADR 0088's implementation erratum). A line never holds an item's title, body, labels, state or forge id, nor
  the local principal.
- **The fold:** every key is read through its aliases first; a cycle a merge made stops before
  the first key it would repeat. A chat's link is its last chat link across every workspace's
  log, ended by a later chat unlink naming the same item once both are resolved. A workspace's
  items are its own links, less those a later unlink names, plus the item of each chat whose
  current link its log wrote. Every directory and file is read through containment, and one
  that resolves out of the project is not read.

| Line | Keys | Meaning |
|---|---|---|
| workspace link | `v`, `ts`, `op: "link"`, `item` | the workspace holds the item |
| chat link | `v`, `ts`, `op: "link"`, `item`, `chat` | the chat works on the item; the last one per chat, across every workspace, wins |
| unlink | `v`, `ts`, `op: "unlink"`, `item`, and `chat` for a chat's | the link ends |
| alias | `v`, `ts`, `op: "alias"`, `from`, `to`, `cause` | `from` resolves to `to`; `cause` is `promoted`, `moved` or `renamed` |

### `workspaces/<ws>/refs/README.md` (and whatever else the operator drops in `refs/`)

- **Format:** Markdown; the rest of the directory is arbitrary operator content.
- **Status:** stable — it is a baseline component `structure_status` reports and `reinit`
  repairs, and the operator writes into the directory.
- **Tier:** Clone state — `refs/` is never in the LIVE block, so it is per clone even for a LIVE workspace.
- **Written by:** `workspace.scaffold` (`charter/workspace.py:1855`–`1864`), only when
  `_exists(rr, follow=True) is False`, through `config.create_for`.
- **Read by:** `_required_components` (`charter/workspace.py:4511`), `last_active`
  (`charter/workspace.py:4424`), `recall`'s ref search (`charter/recall.py:130` is the
  persona twin).
- **Git:** gitignored always — `refs/` is **not** in the LIVE block
  (`charter/workspace.py:1397`–`1401`).
- **Encoding details:** body is
  `f"# {name} — task references\n\nDrop docs, links, and snippets for this task here (local, gitignored).\n"`
  (`charter/workspace.py:1864`).

### `workspaces/<ws>/sessions/` and `sessions/` — session records

- **Format:** a directory of Markdown files, one per **session record**, plus `index.md`.
- **Status:** stable — **purlis only** (SI-8, ADR 0064); the Python charter never wrote
  one. Written when a chat closes through Smart close, and read by the next chat's briefing.
- **Tier:** Plane when LIVE, Clone state when LOCAL — the plane root's `sessions/` is always committed.
- **Where:** `workspaces/<ws>/sessions/` for a chat in a workspace; `sessions/` at the plane
  root for a chat at the plane root, which is in no workspace. Not `.charter/sessions/`, which
  holds per-chat pointers (below) and has nothing to do with these.
- **Written by:** `purlis session record` alone (`sessionrecord::record`), with the body on
  standard input. The app never writes one.
- **Read by:** `purlis session list|show`, the session-start briefing (one line naming the
  place's newest record, or — for a chat the app's **Resume** started — the resumed record
  quoted whole, up to a bound), the app's Sessions panel and record tab, and the smart-close
  skill's successors. A record is named everywhere by its plane-relative path,
  `sessions/<file>` or `workspaces/<ws>/sessions/<file>`, and `sessionrecord::locate` refuses
  every other spelling. What a reader takes from the frontmatter is held to a shape on the way
  in, because the file can be edited: a persona only as a persona name, a harness only as a
  word, a profile only as a profile name, a conversation only as a session id, and a `cwd`
  only as a plane-relative path of plain components — no `..`, no `.` but the whole value, not
  absolute, no backslash. **Resume** starts in it only where it is still a directory inside the
  record's place once every link is followed; otherwise in the place's own directory, and says
  so.
- **Git:** a workspace's follow the workspace — un-ignored by the LIVE block's `sessions` pair,
  kept on disk for a LOCAL one. The plane root's are ignored by `/sessions/` in the plane's
  `.gitignore`: kept on this machine, because the plane root has no LIVE switch (ADR 0064).
- **A record**, `<YYYYMMDD-HHMMSS>-<slug>.md`: the local time it was written, and
  `memstore::slug` of its title; a name already taken gets `-2`, `-3`, …. Chosen under an
  advisory lock on the directory and written whole (temp file and rename). The file is:

  ```text
  ---
  title: <title>
  date: <YYYY-MM-DD HH:MM:SS>
  chat: <the app's number for the chat | unknown>
  chat-name: <the name its tab shows | unknown>
  persona: <name | none>
  harness: <claude | codex | opencode | unknown>
  profile: <the harness profile the chat ran on | unknown>
  conversation: <the harness's conversation id | unknown>
  workspace: <ws | plane root>
  cwd: <the directory the chat ran in, plane-relative (. for the plane root) | unknown>
  sandbox: off                                       (only for a chat whose run was unsandboxed)
  piece: <repo>/<piece> @ <branch | (detached)>      (zero or more lines)
  chat-id: <the chat's ULID | unknown>                (decided, ADR 0066; not yet written)
  device: <the chat's origin device id | unknown>     (decided, ADR 0066; not yet written)
  run: <the ULID of the run that wrote it | unknown>  (decided, ADR 0066; not yet written)
  handed-from: <the parent chat's ULID | none>        (decided, ADR 0066; not yet written)
  resumed-from: <the ULID of the chat whose record it resumed | none>   (decided, ADR 0066; not yet written)
  ---

  # <title>

  ## Goal
  …
  ## Done
  …
  ## Decisions
  …
  ## Open
  …
  ## How to resume
  …
  ```

  The frontmatter is line-based (`personas::frontmatter`), every key always written, and is
  purlis's alone: the model gives only the title and the body. `conversation` is
  `reopen::conversation_of` — the app's `reopen.json` `resume` for that chat, which the app
  keeps current from the chat's hook reports. `profile` and `cwd` are the app's `reopen.json`
  `profile` and `cwd` for that chat (added 2026-09-28, SI-8e): the profile by its name, never
  its command, and the directory as a path below the plane root — a chat whose directory is
  outside the plane is `unknown`. A record written before them has neither key, and reads as
  `unknown` for both. `sandbox: off` is written only for a chat whose current run started
  without the sandbox in a project that has it on — the app's `reopen.json` `sandbox` — which is
  where ADR 0067 §7, as ADR 0075 amends it, puts an opt-out beside the event log (added
  2026-10-03, SD-2). Every other record has no `sandbox` line.

  **Identity keys, decided and not yet written** ([ADR
  0066](adr/0066-a-chat-is-a-ulid-a-run-is-a-stretch-of-its-conversation-and-a-device-is-random.md),
  FD-22). `chat:` is the app's display number, which is unique only in one plane on one machine:
  two machines both write `chat: 7`. So a record will also name the chat by its ULID
  (`chat-id`), the device that created it (`device`, a random id from the machine store, never a
  hostname), the run that wrote the record (`run`), and its lineage by id (`handed-from`,
  `resumed-from`), each read from the app's `reopen.json` like `conversation`. Every key is
  written, as `unknown` or `none` when not known. A record never carries the local principal or
  the OS user. Records written before these keys are never rewritten: a reader reads each absent
  key as `unknown`, and a reader that needs a key for such a record uses its plane-relative path.
  `piece`
  lines are the piece the command ran in and each `--piece`, with the branch `git worktree
  list` reports; a `--piece` git does not report is refused. The body is exactly the five
  `## ` sections above, in that order, each non-empty, with nothing before the first (a heading
  inside a code fence is text); at most 32 KiB; no control or invisible formatting character
  but a line feed or a tab; and no credential shape `purlis save` would refuse a memory for.
  The title is one line of at most 120 characters. `sessionrecord::check` is the one judgment.
- **`index.md`**, rebuilt from the records every time one is written, so a hand edit lasts
  until the next record:

  ```text
  # Sessions — workspace `<ws>`            (or: # Sessions — plane root)

  One file per session record, newest first. purlis rebuilds this index from the records each time one is written (`purlis session record`), so an edit here does not last.

  - <YYYY-MM-DD HH:MM> · [<title, [ and ] escaped>](<file>)
  ```

  Newest first, by file name. A file whose name is not a record's, any link (a workspace's
  records are held with no link on the way, V74; the plane root's allow no link below the plane
  root), and a file over the plane's 1 MiB bound are not records and are not listed.

### `.charter/sessions/<chat>.saved` and `workspaces/<ws>/.charter/sessions/<chat>.saved` — a saved record to pass on

- **Format:** one JSON object and a line feed:
  `{"chat": <the app's number for the chat>, "conversation": <the harness's conversation id | null>, "session_saved": "<the record, absolute>", "at": <unix seconds>}`.
- **Status:** internal — **purlis only** (ADR 0064, amended 2026-09-28; charter#517). Two
  processes of one chat read it, `purlis session record` and that chat's next `Stop` hook, and
  nothing else ever does. Deleting one costs at most a tab that does not close by itself.
- **Tier:** Clone state, transient — a line waiting for one chat's next `Stop` hook.
- **Why:** a harness can run `purlis session record` in a sandbox that lets it write the
  record and refuses its connect to the app's hook socket — Codex's default `workspace-write`
  does exactly that. The chat's hooks run outside that sandbox, so the record's line is left
  here for the hook that ends the same turn to send.
- **Where:** in the directory of the place the chat works — the one `purlis session record`
  files a record in when it is given no `-w`: `.charter/sessions/` below the plane root for a
  chat at the plane root, `workspaces/<ws>/.charter/sessions/` for a chat in `<ws>`. The chat's
  own directory, which is where a sandbox lets a command write. `<chat>` is the number, as
  `$CHARTER_CHAT` spells it. (At the plane root it sits beside the `<sid>.persona` and
  `<sid>.workspace` pointers below, whose `<sid>` is often the same number; the suffix keeps
  them apart.)
- **Written by:** `purlis session record`, only when it had a hook socket and a chat to tell
  and the line did not get through (`sessionrecord::relay::leave`): 0600, written whole (temp
  file and rename), its directories created 0700.
- **Read by:** `charter hook stop` in that chat (`sessionrecord::relay::take`), which removes it
  whatever it holds and sends the app the `SessionSaved` line only when it is this chat's
  (`chat`), this conversation's (a `conversation` that is not null must be the one the `Stop`
  names), and at most five minutes old — the time the app waits for a record before it gives a
  Smart close up. A future `at`, a relative `session_saved`, a link, a file over 8 KiB or one
  that does not parse are stale, removed and not sent. The app closes a tab on the line only
  while that chat is being smart-closed, so a line sent twice closes it once.
- **Git:** ignored — the plane root's by `/.charter/`, a workspace's by `/workspaces/*/*`,
  which the LIVE block never un-ignores it from.
- **Collected (purlis):** the plane root's, with the other `.charter/sessions/` markers,
  30 days after it was written (`retention::on_open`); a workspace's is not collected.

### `workspaces/<ws>/changes/<slug>.json` — a cross-repo change

- **Format:** JSON object; `json.dumps(ordered, indent=2) + "\n"` (`charter/change.py:781`).
- **Status:** stable — committed for LIVE workspaces, hand-editable, an untrusted input
  validated on read and write.
- **Tier:** Plane when LIVE, Clone state when LOCAL
- **Written by:** `change.write` (`charter/change.py:420`, through `contain.writable` +
  `config.write_for`); commands `purlis change create|add|drop|…`
  (`charter/commands_change.py:175`). Removed by `change.forget`
  (`charter/change.py:435`).
- **Read by:** `change.read` (`charter/change.py:393`), `change.all_for`/`read_all`
  (`charter/change.py:446`, `465`), `change.has_records` (`charter/change.py:368`),
  the frame's gather, `purlis change show|list|land`.
- **Git:** gitignored unless LIVE — `changes` and `changes/**` are un-ignored and
  `changes/log/` is re-ignored inside the block (`charter/workspace.py:1400`–`1401`).
  The directory is created lazily by the first `change create`, never by `scaffold`
  (`charter/change.py:335`).
- **Encoding details:** the serialiser is canonical — top-level keys in `KEYS` order,
  each member's keys in `MEMBER_KEYS` order, each exclusion's in `EXCLUSION_KEYS` order
  (`charter/change.py:778`–`781`), so a record read and written back is byte-identical.
  The slug must satisfy `instance.change_name_ok` — `^[A-Za-z0-9][A-Za-z0-9._-]*$`
  (`charter/instance.py:211`, `charter/change.py:352`) — and must equal `rec["change"]`
  (`charter/change.py:520`). The key set is **closed at both ends**: an unknown or a missing
  key is a `RecordError` (`charter/change.py:569`–`585`). Every string field must be one
  plain line within `contain.PATH_DISPLAY_LIMIT` (`charter/change.py:97`,
  `charter/change.py:599`–`605`). There is no state field of any kind.

| Field | Type | Required / default | Meaning | Status | Source |
|---|---|---|---|---|---|
| `change` | string | required, `== slug` | the change's name | stable | `charter/change.py:83` |
| `why` | string | required, one line | what the work is for | stable | `charter/change.py:526` |
| `created` | string | required | UTC ISO-8601 seconds, from `commands_change._now` | stable | `charter/commands_change.py:108`, `charter/commands_change.py:227` |
| `by` | string | required | `git config user.name`, else `$USER`, else `"unknown"` | stable | `charter/commands_change.py:111` |
| `members` | list | required, `[]` | one row per participating repo | stable | `charter/change.py:759` |
| `members[].repo` | string | required | repo name, `contain.segment_ok` (not `valid_name`: `.github` is legal) | stable | `charter/change.py:609` |
| `members[].branch` | string | required; default `change/<slug>` | this change's branch in that repo; may not start `-` | stable | `charter/change.py:762`, `charter/change.py:626` |
| `members[].needs` | list of strings | required, `[]` | repos that must land first; must name members; no self-loops, no cycles | stable | `charter/change.py:88`, `charter/change.py:652` |
| `excluded` | list | required, `[]` | repos considered and left out | stable | `charter/change.py:102` |
| `excluded[].repo` | string | required | repo name | stable | `charter/change.py:557` |
| `excluded[].why` | string | required, one line | why it is out | stable | `charter/change.py:561` |
| `excluded[].at` | string | required | UTC ISO-8601 seconds when it was dropped | stable | `charter/commands_change.py:321` |

### `workspaces/<ws>/changes/log/<device>.jsonl` — the landing log

- **Format:** JSON Lines, one object per line, `O_APPEND`, no lock.
- **Status:** stable — it is read by a *different* process from the one that wrote it (the
  frame's pane, `change show`, the land gate), and by other hosts' purlis on the same disk;
  never committed.
- **Tier:** Clone state — never committed, and not derived: it is this clone's record of what it landed.
- **Written by:** `commands_change._append_landing` (`charter/commands_change.py:1092`), from
  `purlis change land` (`charter/commands_change.py:1501`), after the merge is read back.
  `change.record_landing` (`charter/change.py:156`) is a second writer with the same shape
  — see the Appendix. In this app: `change::landing::append`, from `purlis change land`
  (`crates/purlis-core/src/change/land.rs`) once the read-back confirms the merge at the head
  the checks passed on, or from a later `purlis change land` that finds merged, at that head,
  a request purlis's pending landing (below) says it asked for: one queued, or one whose
  read-back failed.
- **Read by:** `change.read_landings` (`charter/change.py:217`), `change.landings`
  (`charter/change.py:189`), `commands_change.landings` (`charter/commands_change.py:1109`),
  `change.declared_landings` (`charter/change.py:244`). In this app:
  `change::landing::landings`, for the land gate's blockers, doctor's `changes` row, and
  `purlis change revert`, which reverts each member at its line's `merge` commit and refuses a
  `merge` that is not a commit id (`[0-9a-f]{7,64}`) before it reaches git.
- **Git:** committed **never** — `/workspaces/<ws>/changes/log/` re-ignored inside the LIVE
  block (`charter/workspace.py:1401`), and `_ws_meta_paths` stages `changes/` only when
  `change.has_records` is true (`charter/commands_workspace.py:1119`).
- **Encoding details:** filename is **the device's name for its logs** (FD-25, ADR 0066):
  this device's id from the machine store (`machine.json`'s `device`), a ULID, so two machines
  that share a hostname write two files and a renamed machine keeps its one. Before that id is
  minted (the first launch that finds none), or where the store keeps none (ADR 0031), it is
  the short hostname the Python charter used: `socket.gethostname().split(".")[0]` with every
  character outside `[A-Za-z0-9_-]` removed, truncated to 32, `"unknown"` when empty
  (`charter/change.py:142`, `charter/pieces.py:71`). Writing a line never mints the id
  (`purlis_core::dispatch::log_name`). A file named by a hostname that an earlier version
  wrote is still read beside the new one, since every reader reads every file in the
  directory; FR-9 renames those (#607). The line is
  `contain.json_line(line, sort_keys=True) + "\n"` — **keys sorted, `ensure_ascii=True`**
  (`charter/commands_change.py:1099`, `charter/contain.py:473`), written with
  `os.open(..., O_WRONLY|O_CREAT|O_APPEND, 0o644)` (`charter/commands_change.py:1100`).
  A reader skips any line that is not an object whose key set is exactly `LOG_FIELDS`
  (`charter/change.py:234`), and sorts by `ts` (`charter/change.py:241`).

| Field | Type | Required / default | Meaning | Status | Source |
|---|---|---|---|---|---|
| `ts` | string | required | UTC ISO-8601 seconds | stable | `charter/commands_change.py:1092` |
| `change` | string | required | the change's slug | stable | `charter/change.py:113` |
| `repo` | string | required | the member that landed | stable | `charter/change.py:113` |
| `number` | int | required | the request number purlis merged | stable | `charter/commands_change.py:1093` |
| `merge` | string | required | sha of the merge commit purlis created | stable | `charter/change.py:113` |
| `head` | string | required | the member branch tip it was created from | stable | `charter/change.py:113` |

### `workspaces/<ws>/changes/log/pending/<device>.jsonl` — pending landings

- **Format:** JSON Lines, one object per line, `O_APPEND`, no lock; keys sorted, ASCII only.
- **Status:** stable — written by one `purlis change land` and read by a later one and by
  doctor; never committed. New in this app (#472, D-472a); the Python charter has no such file,
  and its landing-log reader, which reads `log/*.jsonl`, never reads this directory.
- **Tier:** Clone state — never committed, and not derived: it is this clone's evidence that
  purlis started a landing. Deleting it costs only that: a merge purlis queued and has not
  yet recorded is then never recorded as purlis's.
- **Written by:** `change::pending::append`, from `purlis change land`
  (`crates/purlis-core/src/change/land.rs`): `asked` before it asks the forge to merge or to
  queue, `refused` when the forge refused, `merge-later` when GitLab set the request to merge
  later and purlis could not undo it.
- **Read by:** `change::pending::pendings`, for `purlis change land` (its own member, and
  its blockers), doctor's `changes` row, and `purlis change revert`, which names a member
  purlis started landing and has not logged. The latest line per member, by `ts`, wins; a line
  that is not exactly the seven fields below (`PENDING_FIELDS`) is skipped. A line is
  evidence only while it stands (every stage but `refused`) and names the landing asked about:
  `land` matches the request and its head, doctor the head of the member's pushed branch.
- **Git:** committed **never** — under `/workspaces/<ws>/changes/log/`, which the LIVE block
  re-ignores.
- **Encoding details:** filename `<device>.jsonl`, named as the landing log is (FD-25: the
  device id, the short hostname only before one is minted).

| Field | Type | Required / default | Meaning | Status | Source |
|---|---|---|---|---|---|
| `ts` | string | required | UTC ISO-8601 seconds | stable | `change::pending` |
| `change` | string | required | the change's slug | stable | `change::pending` |
| `repo` | string | required | the member | stable | `change::pending` |
| `number` | int | required | the request purlis asked the forge to land | stable | `change::pending` |
| `head` | string | required | the head commit the checks passed on, which the call was pinned to | stable | `change::pending` |
| `via` | string | required | `direct` or `queue` | stable | `change::pending` |
| `stage` | string | required | `asked`, `refused` or `merge-later` | stable | `change::pending` |

### `workspaces/<ws>/pieces/<device>.jsonl` — the piece claim log

- **Format:** JSON Lines, `O_APPEND`, no lock.
- **Status:** stable — written by `purlis wt add`/`wt done` and read by the status line,
  `wt list` and the frame; never committed, but read by processes other than the writer.
- **Tier:** Clone state — never committed; this clone's claims on pieces.
- **Written by:** `pieces.record` (`charter/pieces.py:122`), from
  `charter/commands_worktree.py:167` (`claimed`) and `charter/commands_worktree.py:233`
  (`done`/`abandoned`).
- **Read by:** `pieces.events` (`charter/pieces.py:148`), `claims`
  (`charter/pieces.py:167`), `declarations` (`charter/pieces.py:185`), `silence`
  (`charter/pieces.py:368`).
- **Git:** never committed — `pieces` is deliberately absent from the LIVE block
  (`charter/pieces.py:49`), so `/workspaces/*/*` keeps it ignored.
- **Encoding details:** filename `<device>.jsonl`, named as the landing log is (above:
  the device id, the short hostname only before one is minted; `charter/pieces.py:85`,
  `charter/pieces.py:71`). The line's `host` stays the short hostname: a label for whoever
  reads the claim, never what the file is keyed by. Line is
  `json.dumps(line, sort_keys=True) + "\n"` (`charter/pieces.py:120`), `0o644`,
  `O_APPEND` (`charter/pieces.py:122`). A malformed line is skipped
  (`charter/pieces.py:159`); events are sorted by `ts` (`charter/pieces.py:164`).
  Key set is closed by test (`FIELDS`, `charter/pieces.py:55`); the vocabulary is
  `claimed|done|abandoned` and nothing else (`charter/pieces.py:64`), enforced by a
  `ValueError` on write (`charter/pieces.py:101`).

| Field | Type | Required / default | Meaning | Status | Source |
|---|---|---|---|---|---|
| `ts` | string | required | UTC ISO-8601 seconds | stable | `charter/pieces.py:105` |
| `event` | string | required | `claimed` / `done` / `abandoned` | stable | `charter/pieces.py:64` |
| `repo` | string | required | clone the worktree belongs to | stable | `charter/pieces.py:107` |
| `piece` | string | required | the worktree/piece name | stable | `charter/pieces.py:108` |
| `session` | string or null | required key | `session.current()` at write time | stable | `charter/pieces.py:109` |
| `host` | string | required | short hostname, as a label: the file is named by the device id (FD-25) | stable | `charter/pieces.py:110` |
| `persona` | string or null | required key | `persona.resolve_active()` | stable | `charter/pieces.py:111` |
| `reason` | string | present only when given | a `done`/`abandoned` reason | stable | `charter/pieces.py:113` |

### `workspaces/<ws>/pieces/seen/<repo>.json` and `pieces/seen/<repo>/<piece>.json`

- **Format:** one small JSON object, **overwritten** each turn.
- **Status:** stable — written by a hook process and read by the status line and `wt list`,
  which are different processes.
- **Tier:** Clone state, transient — overwritten every turn; losing it loses only a last-seen age.
- **Written by:** `pieces.seen` (`charter/pieces.py:276`), from `hooks._touch_piece`
  (`charter/hooks.py:6264`), which every turn-level handler calls
  (`charter/hooks.py:6352`, `6488`, `7609`, `7775`, `8040`, `8703`).
- **Read by:** `pieces.last_seen` (`charter/pieces.py:330`), `presence`
  (`charter/pieces.py:293`), `seen_age` (`charter/pieces.py:363`), `silence`
  (`charter/pieces.py:384`).
- **Git:** never committed (under `pieces/`).
- **Encoding details:** `json.dumps(blob, sort_keys=True) + "\n"` via `Path.write_text`
  (`charter/pieces.py:276`). A clone's own record is `seen/<repo>.json`; a piece's is
  `seen/<repo>/<piece>.json` (`charter/pieces.py:236`). Keys: `ts` (UTC ISO seconds),
  `session`, and — only when a persona was resolved — `persona` and `by`
  (`charter/pieces.py:268`–`271`). `by` is `{persona: ts}`, pruned to the last hour
  (`PRESENCE_WINDOW`, `charter/pieces.py:220`) and capped at 8 entries
  (`PRESENCE_KEEP`, `charter/pieces.py:225`). purlis (#434) replaces the record whole
  (temp beside, fsync, rename) and refuses one that is a symlink; `Path.write_text` truncated
  it in place and followed a link.

### `workspaces/<ws>/.charter-structure` — the layout stamp

Written as `.purlis-structure`. A `.charter-structure` (or a pre-charter `.edm-structure`) is
renamed to it the first time it is read, keeping its version (V93i).

- **Format:** plain text: the integer version and `\n`.
- **Status:** **stable** — the status line reads it in a different process from the one
  that wrote it. Deleting it makes the workspace read as version 0, so the status line
  flags `⚠ reinit` and
  `purlis workspace reinit` re-stamps it. Nothing else is lost.
- **Tier:** Clone state, rebuildable — re-stamped by the next scaffold or repair.
- **Written by:** `workspace.scaffold` (`charter/workspace.py:1885`), through a raw
  `os.open(..., O_WRONLY|O_CREAT|O_TRUNC|O_NOFOLLOW|O_NONBLOCK, 0o666)`
  (`charter/workspace.py:1880`) — never through a symlink, never into a FIFO.
- **Read by:** `workspace._stamp` (`charter/workspace.py:4539`, `O_RDONLY|O_NOFOLLOW|
  O_NONBLOCK`, `int(os.read(fd, 4096).strip())`), `structure_version`
  (`charter/workspace.py:4515`), `structure_status` (`charter/workspace.py:4584`),
  `needs_reinit` (`charter/workspace.py:4618`).
- **Git:** gitignored (`/workspaces/*/*`; not in the LIVE block).
- **Encoding details:** `STRUCTURE_VERSION = 5` today (`charter/workspace.py:4473`) — v2
  memory-as-DB, v3 `changes/` in the LIVE block, v4 the harness layer, v5 every workspace
  has a `workspace.json`. A pre-rename `.edm-structure` is renamed in place on first read
  (`charter/workspace.py:4477`–`4500`). "Current" is `version >= STRUCTURE_VERSION` **and**
  no baseline file missing (`charter/workspace.py:4585`).

### `workspaces/<ws>/.charter-generated` — the harness-layer ownership marker

Written as `.purlis-generated`, with its temps as `.purlis-generated.*.tmp`. A record under
`.charter-generated` is read while it is the only one, and renamed, bytes unchanged, the next
time the layer is wired, so every file it vouched for stays purlis's; a leftover old record
beside a purlis one is removed when the record is published (V93i).

- **Format:** JSON object, `json.dumps(marker, indent=2) + "\n"`
  (`charter/workspace.py:2946`); `{relative path: sha256 hex}` or, while a write is pending,
  `{relative path: [sha256, …]}`.
- **Status:** stable — the same marker shape is written into guest checkouts, where another
  process (a later launch, `doctor`, `reinit`) reads it to decide what is purlis's to
  rewrite, withdraw or hide. Delete it and purlis loses its claim: every generated file
  reads as `foreign` and is never rewritten again, which is the failure mode this file
  exists to avoid, so it is not "safe to delete".
- **Tier:** Clone state — the record of which files purlis owns. It is not rebuildable: without it, every file it vouched for reads as the operator's.
- **Written by:** `workspace._publish_marker` (`charter/workspace.py:2943`) from
  `_materialise` (`charter/workspace.py:2807`, `charter/workspace.py:2837`); removed
  entirely when it would name nothing (`charter/workspace.py:2950`) and by `unwire_guest`
  (`charter/workspace.py:4198`).
- **Read by:** `_read_marker_at` (`charter/workspace.py:2212`), `_layer_status`
  (`charter/workspace.py:2568`), `_charter_owned` (`charter/workspace.py:3336`),
  `_unwanted` (`charter/workspace.py:2611`), `_yours_untracked`
  (`charter/workspace.py:3777`).
- **Git:** gitignored in a workspace directory by `/workspaces/*/*`; in a guest checkout it
  is listed in purlis's `.git/info/exclude` block (`charter/workspace.py:3372`). A marker
  git **tracks** is dropped as untrusted (`charter/workspace.py:2176`, `2239`).
- **Encoding details:** written whole through `_write_whole` — temp
  `.charter-generated.<pid>.<12 hex>.tmp` beside the target, `fsync`, mode of the replaced
  file preserved, one `os.replace` (`charter/workspace.py:2913`–`2924`). Keys must be
  relative in-checkout paths: an absolute, drive-qualified, NUL-bearing or `..`-bearing key
  invalidates the whole marker (`charter/workspace.py:2155`–`2173`, `2237`). A value may be
  a string (settled) or a list of strings (pending, published *before* the write)
  (`charter/workspace.py:2440`–`2456`, `2806`). Key order is dict order: the intent dict is
  built from the marker read plus the writes, so it is effectively the order paths first
  appeared. The digest is `content_digest` — sha256 of the UTF-8 text purlis wrote
  (`charter/workspace.py:1945`).

### `workspaces/<ws>/.claude/settings.json` — the generated harness layer

- **Format:** JSON, `json.dumps(doc, indent=2) + "\n"`
  (`charter/harness/claude_code.py:485`).
- **Status:** stable — Claude Code reads it for any chat rooted in the workspace directory.
- **Tier:** Clone state, rebuildable — regenerated from the plane at the next launch.
- **Written by:** `workspace.wire_harnesses` → `_materialise` → `_write_whole`
  (`charter/workspace.py:2677`, `charter/workspace.py:2824`), content from
  `ClaudeCodeHarness.workspace_files` (`charter/harness/claude_code.py:447`). Reached from
  `workspace.scaffold` on every `ensure` (`charter/workspace.py:1870`) and from
  `workspace reinit` (`charter/workspace.py:4644`).
- **Read by:** Claude Code itself; purlis reads it back only to compare against what it
  wants (`_layer_status`, `charter/workspace.py:2562`) and to answer
  `rules_not_in_force` (`charter/workspace.py:3984`).
- **Git:** gitignored — `/workspaces/*/*` covers it and the LIVE block never un-ignores
  `.claude/` (`charter/workspace.py:1397`–`1401`).
- **Encoding details:** a 1:1 mirror of the plane's own `.claude/settings.json`, limited to
  `WORKSPACE_KEYS = ("enabledPlugins", "env")` (`charter/harness/claude_code.py:53`) plus a
  `permissions` object holding only the **restrictive** buckets (`ask`, `deny`), appended
  last (`charter/harness/claude_code.py:484`). Empty plane settings → no file and no marker
  entry at all. opencode and Codex contribute nothing here and declare the
  `WORKSPACE_SCOPE` deficit instead (`charter/harness/opencode.py:751`,
  `charter/harness/codex.py:182`), reported by `workspace.harness_deficits`
  (`charter/workspace.py:1971`).

### `workspaces/<ws>/<repo>/` — a cloned repo (guest checkout)

- **Format:** an ordinary git clone. purlis recognises it by `<dir>/.git` being a
  **directory** (`workspace.is_clone`, `charter/workspace.py:300`, via `_directory`
  `charter/workspace.py:2287`); a linked worktree's `.git` is a file and is excluded there
  on purpose. `guest_trees` is the wider question — anything with a resolvable git dir
  (`charter/workspace.py:3126`, `_children` at `charter/workspace.py:3139`).
- **Status:** stable — the operator's own repository. purlis is a guest and writes only
  the paths below.
- **Tier:** None — the operator's own repository, with its own remote. purlis's files inside it carry their own tiers below.
- **Written by (purlis's own files only):** `workspace.wire_guest`
  (`charter/workspace.py:4059`) from `wire_harnesses` (`charter/workspace.py:2680`),
  `purlis clone` (`charter/commands.py:500`) and `purlis wt add`
  (`charter/commands_worktree.py:194`). Removed by `unwire_guest`
  (`charter/workspace.py:4148`) and `unwire_guests` on `workspace remove`
  (`charter/workspace.py:4211`, `charter/commands_workspace.py:374`).
- **Git:** everything purlis writes there is listed one path at a time in that checkout's
  `.git/info/exclude`; purlis never touches the checkout's `.gitignore` and never stages
  anything in it (`charter/workspace.py:1916`–`1920`).
- **Files purlis generates inside a clone** (`_guest_files`, `charter/workspace.py:2135`):
  - `.claude/settings.json` — the same mirror as the workspace directory's.
  - `.claude/settings.local.json` — the plane's **local** ask/deny rules, checkout only
    (`charter/harness/claude_code.py:487`, `CHECKOUT_LOCAL_SETTINGS` at
    `charter/harness/claude_code.py:95`). It is *co-written*: the harness saves "don't ask
    again" into it, so purlis never rewrites it once edited and never withdraws its
    exclude line (`charter/harness/claude_code.py:387`, `charter/workspace.py:3353`).
  - a 1:1 copy of the plane's `.claude/agents/**`, `.claude/skills/**`,
    `.opencode/agent/**`, `.codex/skills/**` — every registered harness's
    `inherited_paths` (`charter/workspace.py:2105`, `charter/harness/claude_code.py:169`,
    `charter/harness/opencode.py:831`, `charter/harness/codex.py:236`). Keys are
    `<declared path>/<relative posix path>` (`charter/workspace.py:2131`).
  - `.charter-generated` — the marker, as above.
- **Encoding details:** a tree whose root does not resolve inside `workspaces/` (or, for a
  piece, inside the worktree root) is never written to — `_wired_tree_ok`
  (`charter/workspace.py:3302`). Every individual write is refused if the path or its
  parent resolves out of the tree (`_inside`, `charter/workspace.py:2848`;
  `_write_whole`, `charter/workspace.py:2906`). Withdrawals only ever reach a path under a
  harness root (`_generated_roots`, `charter/workspace.py:2587`) whose content still matches
  the recorded digest (`charter/workspace.py:2642`), and empty parent directories are pruned
  (`_prune_empty`, `charter/workspace.py:4123`).

### `workspaces/<ws>/<folder>/` — a folder an extension keeps in a workspace

- **Format:** whatever the extension writes there. purlis reads none of it.
- **Status:** **purlis only** (purlis#343, ADR 0053). `<folder>` is the one name an
  approved extension declares as `contributes.events.workspace_folder`: letters, digits, `-`
  and `_`, and never one of purlis's own names in a workspace (`workspace.md`,
  `workspace.json`, `memory`, `todos`, `refs`, `pieces`, `worktrees`, `README.md`,
  `CLAUDE.md`, `AGENTS.md`, `manifest.json`).
- **Tier:** None — the extension's own folder; purlis reads none of it.
- **Written by:** the extension's own program, which runs as the operator. purlis does not
  create it.
- **Read by:** `purlis workspace fork`, which copies it into the fork after the purlis, the
  memory and the todos — whether or not the extension is on in the project, and only for an
  extension this machine approved whose bytes are unchanged
  (`crates/purlis-core/src/extension/events.rs` `carried`,
  `crates/purlis-core/src/wscmd/fork.rs`). A folder of that name that is a clone (it holds
  `.git`) is never copied.

### `<clone>/.git/info/exclude` — purlis's managed block

- **Format:** plain text; a delimited block inside a file the operator also owns.
- **Status:** stable — git reads it, and purlis reads it back on every launch and repair.
- **Tier:** Clone state, rebuildable — the managed block is rewritten on every launch and repair.
- **Written by:** `workspace._register_excludes` (`charter/workspace.py:3876`) through
  `_write_whole` (`charter/workspace.py:3908`), from `wire_guest`
  (`charter/workspace.py:4102`, `charter/workspace.py:4112`) and `unwire_guest`
  (`charter/workspace.py:4207`).
- **Read by:** `_exclude_state` (`charter/workspace.py:3791`), `unaccounted`
  (`charter/workspace.py:3830`), `unhidden` (`charter/workspace.py:3840`), `guest_layer`
  (`charter/workspace.py:3959`) — and git.
- **Git:** never committed, never tracked; it lives in the **common** git directory, so a
  linked worktree and its clone share one (`git_exclude_file`, `charter/workspace.py:269`,
  resolving `commondir` at `charter/workspace.py:287`).
- **Encoding details:**
  - Markers: `# >>> purlis (generated layer — \`purlis workspace reinit\`) >>>` and
    `# <<< purlis <<<`. The markers charter wrote before the rename
    (`# >>> charter (generated layer — \`charter workspace reinit\`) >>>` and
    `# <<< charter <<<`) delimit the block too, and it is rewritten in place under the purlis
    ones, with the old `/.charter-generated` and `.charter-generated.*.tmp` lines kept while
    such a file is still there (V93i). A fixed three-line note sits
    between the begin marker and the paths (`charter/workspace.py:3118`).
  - Each listed path is written as `/<rel>` (anchored), except the temp pattern
    `.purlis-generated.*.tmp`, written unanchored (`charter/workspace.py:3390`,
    `charter/workspace.py:3381`). Block ends with one `\n` (`charter/workspace.py:3391`).
  - Order: sorted rels, then `.purlis-generated`, then the temp pattern
    (`charter/workspace.py:3756`–`3760`).
  - Replacement is in place between the markers; an unterminated block runs to EOF and is
    replaced whole (`_replace_block`, `charter/workspace.py:3404`; `_block_span`,
    `charter/workspace.py:3431`). A file with no block and nothing to add is handed back
    byte for byte (`charter/workspace.py:3419`).
  - The block holds what **every** tree sharing this exclude needs (`_shared_rels`,
    `charter/workspace.py:3656`); a line is only dropped when its path is proved absent in
    all of them, and a line is never *added* over an untracked file of the operator's in a
    sibling tree (`charter/workspace.py:3718`–`3734`).

### `.charter-scan-allow.toml` — the commit scan's allowlist (any repository)

- **Format:** TOML at the top of a repository.
  - `[[allow]]` tables. Each has a required `reason`, and either:
    - a `rule`, an id `purlis scan --explain` prints (`email`, `forge-token`, …), with
      `paths`. `paths` is a list of globs from the repository's top: `*` stays within one
      directory and `**` crosses them. It is required for a rule entry; the whole repository is
      `paths = ["**"]`.
    - a `fingerprint`: `sha256:` and the hex SHA-256 of one key's value, with optional `paths`.
      Personal data (`email`, `card-number`, `us-ssn`) is never let through by fingerprint,
      because the value can be recovered from its hash by guessing. Those rules take `paths`.
  - An optional `[builtin]` table with `enabled = false`, which turns purlis's own entries off.
  - An entry that can't be read, or that breaks one of these rules, allows nothing, and the scan
    says so.
- **Status:** **stable** (SQ-17, ADR 0074 as amended).
- **Tier:** Plane — committed, in a plane. In any other repository it is committed to that
  repository and reviewed like its code, and that repository as a whole stays tier None.
- **Written by:** the operator, by hand, in a commit made outside a chat. purlis never writes
  it. `purlis scan --explain` prints the entry that would let a finding through.
- **Read by:** `purlis_core::diffscan::checked` and `purlis scan`, **as it is at `HEAD`**
  (`git show HEAD:.charter-scan-allow.toml`), never from the working tree or the index.
- **Git:** committed. A chat's `pre-commit` refuses a commit that changes it, including a move
  (`--no-renames`). The Bash guard refuses a chat's `git revert` of a commit that changed it, and
  `git merge --ff-only` onto one. A merge commit may bring it as it was committed elsewhere.
  This is a mistake guard: ADR 0074 names the ways past it.
- **Beside it:** purlis's own entry, the same in every repository: an `email` in a file named
  `Cargo.toml`, `package.json`, `.mailmap`, `AUTHORS`, `CONTRIBUTORS` or `CHANGELOG` (the last
  three also `.md` and `.txt`), at any depth.

### Provenance trailers — in the message of every commit an agent run makes (any repository)

- **Format:** git trailers, `git interpret-trailers` syntax, in the commit message's last
  paragraph, in this order. Ruling V67 (#702, GL-8).

  ```text
  Assisted-by: <harness>:<model>
  Purlis-Chat: <chat ULID>
  Purlis-Persona: <persona>
  Purlis-Change: <change slug>
  ```

  Commits made before the rename carry `Charter-Chat`, `Charter-Persona` and `Charter-Change`.
  Those spellings are recognised for good (V93j): `change revert` finds a landing by either
  `Charter-Change` or `Purlis-Change`, and a message that already has a `Charter-*` trailer with
  the same value is not given its `Purlis-*` twin.

  - `Assisted-by` is the Linux kernel's original form (`Documentation/process/coding-assistants.rst`,
    78d979db6cef). `<harness>` is one of `claude-code`, `codex` and `opencode` (V67(b)), taken
    from the kind of the harness profile the chat was started on, never from the program it
    runs. `<model>` is the model as its provider names it. When purlis does not know the model, the
    value is `<harness>` alone, with no colon. With `assisted_by = "llm"` (below) it is
    `Assisted-by: LLM`, the kernel's form since 816d9992d9ed.
  - `Purlis-Chat` names the chat by its ULID (ADR 0066), never by its number.
  - `Purlis-Persona` is the persona the chat adopted.
  - `Purlis-Change` is the change whose record has a member for this repo on the branch the
    commit is on (ADR 0060).
  - A trailer whose value purlis does not know is left out. A value that is not one word of
    printable ASCII, or is longer than 100 characters, counts as unknown. A line already in the
    message is not added again, so an amend carries each trailer once.
  - **purlis only appends.** Every byte the message already had stays as written, including
    the agent's own trailers (`Co-authored-by:x` is not reformatted) and a `---` line, which is
    text and not a patch divider. The lines go after the message's last line of text, joining
    its trailer block when that paragraph is one, and after a blank line otherwise. In an
    edited message they go before the closing comment lines, and nothing below git's scissors
    line is touched. `git interpret-trailers` is not run, so a repository's `trailer.*`
    configuration is neither read nor run.
  - There is no on-behalf-of trailer. The human is the commit's author.
  - **Only below the chat's harness** (V82, #1018). A commit is stamped only when the process
    making it descends from the program the app started for the chat, as `chats[].pid` in
    `app/reopen.json` records it. A program that only inherited the chat's environment, such as
    an editor opened from it, stamps nothing. **A sandboxed chat's commits get no trailers yet,
    and neither does anything on Windows**: the check cannot reach the harness there, and it
    fails closed. ADR 0074 lists the limits of that check.
  - **A claim, not proof.** The agent writes its own commit message, so it can type any of these
    lines, change them, or make a commit the hook never sees. Treat them as what the agent run
    says about itself, for reading history. Nothing should take them as a security signal or as
    evidence of who made a change.
- **Status:** **stable**. These are public: they are written into the history of every repository
  an agent commits to, and are hard to take back.
- **Tier:** None — they live in the repository's own history, and purlis keeps no copy.
- **Written by:**
  - a chat's `commit-msg` hook, for every commit the agent makes itself in a chat whose git runs
    purlis's hooks (ADR 0074 as amended by V67). `purlis git-hook commit-msg` reads the chat
    from `$CHARTER_SESSION_ID` and the app's record (`purlis_core::provenance::stamp`). It
    never refuses a commit, and a `purlis` that has gone leaves the message as written;
  - `purlis save`, when it is run inside a chat, for the project save it commits
    (`purlis_core::planegit`).
- **Never written** on a commit the operator makes by hand: their terminal is not armed, a
  save from the window's button or from auto-save is the app's, and a shell tab is no harness.
- **Spelled per repo:** `[plane].assisted_by` and `[repos.<name>].assisted_by`, `"full"` (the
  default) or `"llm"` in any case, in `charter.toml` (see the table above). A `[repos.<name>]`
  that does not set it follows `[plane].assisted_by`. A repository the project does not hold gets
  the full form.
- **Read by:** nothing in purlis yet. `git log --format='%(trailers)'` and
  `git interpret-trailers --parse` read them.

### `workspaces/<ws>/.worktrees/<repo>/<piece>/` — pieces

- **Format:** git linked worktrees.
- **Status:** stable — the layout is a contract: `worktree.locate` and `workspace.from_path`
  derive the active workspace from a path of exactly this shape.
- **Tier:** None — git's worktrees and the operator's work in them (ADR 0027); git is their registry.
- **Written by:** `git worktree add` under `purlis wt add`; the path comes from
  `worktree.path_for` (`charter/worktree.py:77`) and `worktree.root`
  (`charter/worktree.py:34`).
- **Read by:** `worktree.locate` (`charter/worktree.py:37`), `dirs_for`
  (`charter/worktree.py:238`), `list_for` (`charter/worktree.py:128`),
  `workspace._pieces` (`charter/workspace.py:3180`), `_piece_at`
  (`charter/workspace.py:3247`).
- **Git:** `DIR_NAME = ".worktrees"` (`charter/worktree.py:28`) starts with a dot, so it is
  never taken for a repo (`charter/workspace.py:450`) and is gitignored by
  `/workspaces/*/*`.
- **Encoding details:** the in-plane layout is `workspaces/<ws>/.worktrees/<repo>/<piece>`;
  with a relocated root it is `<root>/<ws>/<repo>/<piece>` and `.worktrees` disappears from
  the path (`charter/worktree.py:31`–`34`, `charter/worktree.py:53`–`58`). The root is
  `$CHARTER_WORKTREES` → `[plane] worktrees` → `None` (in-plane)
  (`charter/config.py:82`–`88`); a *committed* `[plane] worktrees` must be plane-adjacent
  (`contain.plane_adjacent`, `charter/config.py:88`), the env var takes anything. **Git is
  the only registry** — nothing records a worktree in workspace state
  (`charter/worktree.py:3`); purlis's own listing spawns
  `git worktree list --porcelain` with the repository-local git env unset and a 5 s timeout
  (`charter/workspace.py:3623`, `_GIT_TIMEOUT` at `charter/workspace.py:3451`,
  `_GIT_ENV` at `charter/workspace.py:3464`). purlis writes the same guest layer into each
  piece (`charter/workspace.py:3136`).

### `.gitignore` (plane root) — the managed live-workspace block

- **Format:** plain text block inside the plane's own `.gitignore`.
- **Status:** stable — committed, so liveness travels with the plane; it *is* the record of
  which workspaces are LIVE.
- **Tier:** Plane — committed.
- **Written by:** `workspace._write_live_block` (`charter/workspace.py:1418`) from
  `set_live` (`charter/workspace.py:1440`) and `refresh_live_block`
  (`charter/workspace.py:1430`, called by every `reinit`, `charter/workspace.py:4650`).
  Commands: `purlis workspace live [--off]`, `workspace create --live`, `fork --live`,
  `rename` (moves the entry, `charter/workspace.py:1490`).
- **Read by:** `workspace.live_workspaces` (`charter/workspace.py:1346`), `is_live`
  (`charter/workspace.py:1366`) — and git.
- **Git:** committed.
- **Encoding details:**
  - Markers, verbatim:
    `# >>> charter live workspaces (managed by \`charter workspace live\`) >>>` and
    `# <<< charter live workspaces <<<`; in a project migrated to purlis names,
    `# >>> purlis live workspaces (managed by \`purlis workspace live\`) >>>` and
    `# <<< purlis live workspaces <<<`. Either pair is read, and the block is rewritten in
    place under the project's pair the next time it is written (V93g, V93i).
  - Eleven lines per LIVE workspace, workspaces sorted by name
    (`charter/workspace.py:1396`–`1401`):
    `!/workspaces/<n>/workspace.json`, `!/workspaces/<n>/workspace.md`,
    `!/workspaces/<n>/memory`, `!/workspaces/<n>/memory/**`,
    `!/workspaces/<n>/todos`, `!/workspaces/<n>/todos/**`,
    `!/workspaces/<n>/sessions`, `!/workspaces/<n>/sessions/**`,
    `!/workspaces/<n>/changes`, `!/workspaces/<n>/changes/**`,
    `/workspaces/<n>/changes/log/`. The `sessions` pair is purlis's (ADR 0064): a
    workspace's session records follow it, LIVE or LOCAL. A block written before it is brought
    up to date by `reinit`, as any older block is.
  - Lines are joined with `\n` and the block has no trailing newline of its own; the rewrite
    is `re.sub(BEGIN .*? END, block, flags=DOTALL)` (`charter/workspace.py:1412`). A first
    write is inserted directly after the literal line `!/workspaces/.gitkeep\n`
    (`charter/workspace.py:1414`) — the anchor `purlis init` writes
    (`charter/commands.py:1091`) — else appended at EOF with one blank-free `\n` on each side
    (`charter/workspace.py:1417`). Written with a plain `gi.write_text`, non-atomically.
    **In purlis** (#358) it is replaced by rename, and `set_live` reads the block and
    writes the next one under the plane root's `flock`, so two workspaces made LIVE at once
    are both LIVE.
  - Liveness is *parsed back* only from the `workspace.json` line:
    `^!/workspaces/([^/]+)/workspace\.json$` inside the block
    (`charter/workspace.py:1360`).
  - What a LIVE workspace actually stages is `_ws_meta_paths`
    (`charter/commands_workspace.py:1094`, list built at `charter/commands_workspace.py:1116`): `workspace.json`, `workspace.md`, `memory`,
    `todos`, `sessions` (purlis; each only if it exists) and `changes` only when `change.has_records`
    (`charter/commands_workspace.py:1119`). The two lists must agree and only a test holds
    them together.
  - **In purlis, going LOCAL also untracks** what the block had published
    (`git rm --cached`, files kept on disk) and saves the plane, and going LIVE saves it at
    once. History already pushed stays on the remote (ADR 0051).

### `.charter/…` — active-workspace pointers and per-plane workspace state

These are `.charter`'s to document in full; named here because they decide which workspace
a command acts on, or record workspace state.

| Path | Tier | What it holds | Status | Source |
|---|---|---|---|---|
| `.charter/sessions/<sid>.workspace` | Clone state, transient | the workspace chosen for that session, `name + "\n"` | stable (read by the status line, hooks, every command) | `charter/workspace.py:66`, written `charter/workspace.py:823` |
| `.charter/sessions/<sid>.lock` | Clone state, transient | the workspace that session is locked to, `name + "\n"` | stable | `charter/workspace.py:709`, written `charter/workspace.py:824` |
| `.charter/terminals/<tid>.workspace` | Clone state, transient | the terminal pane's workspace | stable | `charter/workspace.py:192`, written `charter/workspace.py:819` |
| `.charter/workspace-tab-order` | Clone state, legacy | one workspace name per line, the tab strip's order | stable — the frame and the palette read the order another process wrote; deleting it costs the order, which the next launch recomputes (`charter/workspace.py:1066`) | `charter/workspace.py:978`, written `charter/workspace.py:1023` |
| `.charter/workspace-arrivals/<name>` | Clone state, transient, legacy | empty file; its existence marks "a handoff landed here" | stable — one process records the arrival, another reads it; deleting it clears the mark only | `charter/workspace.py:1113`, `charter/workspace.py:1127` |
| `.charter/unrecorded/<sha256(realpath(tree))[:32]>.json` | Clone state, rebuildable | `{"errno": …, "says": …}` for a marker publish that failed | stable — `doctor` reads it in another process; recomputed on the next failed publish | `charter/workspace.py:2960`, written `charter/workspace.py:2977` |
| `.charter/workspace-rename.json` | Clone state, transient | **purlis only** (charter#367). `{"from": <old>, "to": <new>, "moved": <bool>}`, JSON, 0600, replaced whole by `rewrite::replace`: the journal of a `purlis workspace rename` in progress. Written before `workspaces/<old>` is renamed, marked `moved` right after (the commit point), and removed once every record that names the workspace has followed. While one that got past the commit point is there, every other rename is refused and the same rename finishes it. One that never moved is stale and the next rename replaces it | stable — a second process (the next rename) reads what the first wrote; deleting it after the move leaves the records the rename had not reached yet naming the old name | `crates/purlis-core/src/wscmd/rename.rs` |
| `.charter/ws-autosave/<ws>` | Clone state, transient, legacy | debounce marker (mtime + a float) for the Stop-hook autosave | internal — deleting it costs one extra commit attempt | `charter/commands_workspace.py:1171`, written `charter/commands_workspace.py:1179` |

Resolution order (`workspace.chosen`, `charter/workspace.py:615`–`649`, and `resolve`
adding the built-in fallback, `charter/workspace.py:586`):
`--workspace` → `$CHARTER_WORKSPACE` (stripped; whitespace-only is unset,
`charter/workspace.py:550`) → **the tree you stand in** (`from_path`,
`charter/workspace.py:348`) → `.charter/sessions/<sid>.workspace` → the frame's launch record
(`for_frame`, `charter/workspace.py:114`) → `.charter/terminals/<tid>.workspace` →
`workspaces/.default` → `config.DEFAULT_WORKSPACE` (`[workspace] default`, fallback
`"default"`, `charter/config.py:31`, `charter/config.py:734`). Pointers older than 30 days
are pruned from both directories on every `set_active` (`charter/workspace.py:45`,
`charter/workspace.py:881`).

**The Rust purlis has every rung of this ladder but the frame's launch record**, because
`.charter/frame/**` is the tmux frame's and that binary neither reads nor writes there (the
ruling above). The two can therefore answer differently in exactly one state — a chat the frame
launched, with no session pointer yet — where the Rust side falls through to
`.charter/terminals/<tid>.workspace` and below. ADR 0032 records the decision, what an operator
on a frame-driven plane sees until then, and the one command that closes it.

**Two more differences, both purlis's own (SI-1, 2026-09-26):**

- **`workspaces/<ws>` itself is in `<ws>`.** Python's `from_path` counts only a path with
  something under the workspace (the directory itself is "a container and not a tree"), so a
  session standing in `workspaces/<ws>` fell through to the pointers. The app starts a
  workspace's chats in exactly that directory and files them under that workspace, so the
  Rust cwd rung counts it (`active::workspace_of_tree`, which `Plane::workspace_of` now calls).
  The recorded scenario `workspace-the-workspace-directory-itself-is-in-that-workspace` holds
  the new answer.
- **`$CHARTER_PLANE_ROOT_SESSION=1` puts a session in no workspace.** Asked before the ladder:
  when it is exactly `1` and neither `--workspace` nor a non-blank `$CHARTER_WORKSPACE` names a
  workspace, a command that needs one refuses with a sentence telling the caller to pass `-w`,
  `workspace current` prints nothing and fails, `workspace use` and `workspace create --use` are
  refused, `recall` searches every base but a workspace's, and `status` reports every
  workspace with none marked. Any other value changes nothing. The ladder itself is unchanged.
- **Standing in the plane outside every workspace is the plane root too** (SI-1b,
  2026-09-27). A session whose directory is the plane's or anywhere under it that is no
  workspace's (`docs/`, `.charter/`, `workspaces/` itself), and for which no rung that speaks
  for THAT session answers — no `--workspace`, no `$CHARTER_WORKSPACE`, no tree, neither its
  session pointer nor its terminal pointer — is in no workspace, with every answer of the
  bullet above but one: `workspace use` and `workspace create --use` still work, because
  nothing pinned it there, and from then on its session pointer answers. **The two committed
  defaults, `workspaces/.default` and `[workspace] default`, no longer answer for such a
  session.** They are the plane's answer for a caller that is nowhere — a script run from
  outside the plane with `$CHARTER_ROOT` set — and they still answer there, and still end the
  ladder asked on its own (`active::workspace`); a session standing in the plane is somewhere,
  and the window already filed it on the plane root's tab. The pointers stay above it so a
  session that was never pinned to the root keeps moving with `workspace use`. Python answered
  the plane's default here; the recorded scenarios that stood at the plane root with no pointer
  and needed a workspace hold the new answer, and each says so in its notes.
  (`active::plane_root`, the one question every command, hook and the footer asks.)

---

## Personas, memory and the roster

The modules that own this area: `charter/persona.py`, `charter/memstore.py`,
`charter/recall.py`, `charter/curate.py`, `charter/commands_persona.py`,
`charter/dispatch.py`, `charter/skilluse.py`, `charter/report.py`.

Plane roots used below (`charter/config.py:819`, `:824`, `:828`, `:834`, `:36`):

| Global | Value |
|---|---|
| `PERSONAS_DIR` | `<plane>/personas` — `charter/config.py:819` |
| `SHARED_PERSONA` | `_shared` — `charter/config.py:36` |
| `PERSONA_STATE_DIR` | `<plane>/.charter/persona-state` — `charter/config.py:824` |
| `ACTIVE_PERSONA_FILE` | `<plane>/.charter/active-persona` — `charter/config.py:828` |
| `REPORTS_DIR` | `<plane>/.charter/reports` — `charter/config.py:834` |

A persona name is `[a-z0-9][a-z0-9._-]*`, matched with `fullmatch`
(`charter/persona.py:47`, `charter/persona.py:78`). A leading `_` is reserved for purlis's
own namespaces (`_shared`, `_dispatch`, `_skills`) and `list_personas` skips any directory
starting with `_` (`charter/persona.py:234`).

**In charter-app** the name `charter` is reserved too (`crates/purlis-core/src/personas.rs`,
`RESERVED`), because `charter/<id>` names purlis's own curation actions (ADR 0061).
`purlis persona create purlis` is refused. A persona that already has the name still loads,
runs and can be removed, but `purlis persona lint` reports it as an error and none of its
curation actions is offered.

---

### `personas/`

- **Format:** directory.
- **Status:** stable — committed; the operator creates personas here by hand or with
  `purlis persona create`; every process (CLI, hooks, status line, frame) enumerates it.
- **Tier:** Plane — committed.
- **Written by:** `charter/commands.py:2651` (`_ensure_front_door`, `purlis init`),
  `charter/commands_persona.py:110` (`cmd_persona_create`), `charter/persona.py:2493`
  (`migrate`).
- **Read by:** `charter/persona.py:225` (`list_personas`) — the roster every other surface
  starts from: `charter/statusline.py:1779`, `charter/render.py:95`, `charter/doctor.py`,
  `charter/frame/switch.py`, `charter/report.py:169` (scrub), `charter/hooks.py:8138`.
- **Git:** committed. Nothing in `_GITIGNORE_BASELINE` (`charter/commands.py:1087`) covers
  `personas/`; verified with `git check-ignore` on the live plane (exit 1 = not ignored).
- **Encoding details:** membership = a subdirectory holding `persona.md`, not starting with
  `_` (`charter/persona.py:234`), plus legacy flat `personas/*.md` whose stem is not
  `readme` (case-insensitive, `charter/persona.py:230`). `list_personas` returns a sorted
  list of names.

---

### `personas/<name>/persona.md`

- **Format:** Markdown with a minimal, line-based frontmatter block (NOT YAML: no parser,
  no quote stripping, no nesting, no comments).
- **Status:** stable — hand-edited, committed, and read by the tool gate, the status line,
  hooks, `sync-agents` and (in M1-M3) the app.
- **Tier:** Plane — committed.
- **Written by:** `charter/commands_persona.py:123` (`cmd_persona_create`, template at
  `:36`/`:57`), `charter/commands.py:2677` (`_ensure_front_door`, template at
  `charter/commands.py:2597`), `charter/persona.py:2504` (`migrate` renames the legacy flat
  file here). Plain `Path.write_text` — no atomic rename, no lock. **In purlis, also a
  project template** (FR-17, `crates/purlis-core/src/template.rs` `apply`): each template's
  two personas (`<stack>-engineer` and `<stack>-reviewer`; `docs-writer` and `docs-reviewer`
  for docs only), with `memory/.gitkeep` and `refs/.gitkeep`, only when the persona's
  directory is not there at all, and each file only when it is absent, then its sub-agent in
  `.claude/agents/<name>.md` as `persona create` writes one. They declare
  `name, role, vault: none, delegate-when` and no `tools`, so a template grants nothing the
  plane-trust fingerprint would ask about.
- **Read by:** `charter/persona.py:456` (`load`) is the single reader; through it
  `resolve` (`:872`), `lineage` (`:839`), `tools_of` (`:1580`), `effective_tools` (`:1691`),
  `vault_of` (`:2240`), `is_draft` (`:1846`), `declared_skills` (`:1957`), `routing_level`
  (`:939`), `routes_to` (`:962`), `lint` (`:2018`); `charter/toolgate.py:896`
  (PreToolUse gate), `charter/commands_persona.py:867` (`_render_agent`),
  `charter/hooks.py:7402` (SessionStart identity block), `charter/statusline.py`,
  `charter/render.py:97`, `charter/doctor.py`.
- **Git:** committed.
- **Encoding details:**
  - Frontmatter is parsed only when the file **starts with** `---`; `text.split("---", 2)`
    must yield ≥3 parts (`charter/persona.py:258`-`:260`). `parts[1]` is the frontmatter,
    `parts[2]` the body (stripped, `:268`).
  - Each frontmatter line: skipped if it has no `:`; `key, _, value = line.partition(":")`;
    both sides `.strip()`ed; empty key skipped (`charter/persona.py:261`-`:267`). Quotes are
    **kept** as part of the value (`charter/persona.py:95`).
  - Order is preserved as pairs and then collapsed to a dict — **last line wins** for a
    repeated key, and the duplicate is reported as an error (`charter/persona.py:489`,
    `charter/persona.py:355`).
  - Keys are matched **exactly**; a key differing only by case is an error and blocks agent
    generation (`charter/persona.py:329`, `charter/commands_persona.py:1127`). Unknown keys
    are a lint warning only (`charter/persona.py:2100`).
  - List-valued keys are comma-separated; `_csv_list` strips a surrounding `[…]` and each
    item (`charter/persona.py:510`).
  - `name` defaults to the directory name when absent (`charter/persona.py:484`).
  - Writer's key order (create): `name, role, vault, [extends], [delegate-when], draft`
    (`charter/commands_persona.py:36`, `:113`, `:118` — `extends` is spliced after `vault`,
    then `delegate-when` after `vault` too, so `extends` ends up above `delegate-when`;
    confirmed by running `persona create --extends`). Front door order:
    `name, role, vault, routing, delegate-when` (`charter/commands.py:2597`); purlis's
    `init` writes `name, role, vault, delegate-when`, since `routing` is retired (charter#369).
  - Body: everything after the closing `---`, stripped.

Full frontmatter vocabulary — `KNOWN_KEYS = AGENT_PASSTHROUGH_KEYS | CHARTER_OWN_KEYS`
(`charter/persona.py:305`, `charter/persona.py:309`, `charter/persona.py:322`). Every value
is a string; "type" below is how purlis interprets it.

| Field | Type | Required / default | Meaning | Status | Source |
|---|---|---|---|---|---|
| `name` | string | defaults to directory name | Identity; emitted as the agent's `name:` | stable | `charter/persona.py:484`, `charter/commands_persona.py:870` |
| `role` | string (one line) | none; `lint` warns | Human role; in listings, the agent description and the session identity block | stable | `charter/commands_persona.py:779`, `charter/persona.py:2068` |
| `vault` | string, or the reserved `none` | none; `lint` warns; falls back to a vault tagged with the persona name | Which vault this persona's secrets live in. `none` = deliberately holds none → `vault_of` returns `None` | stable | `charter/persona.py:2218`, `:2240`, `:2221` |
| `extends` | one persona name | absent | Parent whose purlis+tools are inherited; chain resolved root→child, cycle-safe | stable | `charter/persona.py:839`, `:872` |
| `uses` | CSV of persona names | absent | Routing edge; legacy grant (vault + tools + delegation) when `borrows:` is absent | stable | `charter/persona.py:1587`, `:1691` |
| `borrows` | CSV of persona names, or `none` | **absent ≠ empty**: absent = legacy `uses:` grant, `none`/unreadable = nothing | Which personas' tools the gate auto-approves | stable | `charter/persona.py:1656`, `:1597`, `:1616` |
| `delegate-when` | prose (one line) | none; `lint` warns; `create` requires it unless `--extends` | Routing trigger; becomes the generated agent's description | stable | `charter/commands_persona.py:780`, `:806` |
| `description` | string | absent | Agent description override (lower precedence than `agent-description`) | stable | `charter/commands_persona.py:869` |
| `agent-description` | string | absent | Agent description override (wins) | stable | `charter/commands_persona.py:869` |
| `tools` | CSV of program names | absent = none | Programs auto-approved by the PreToolUse gate while active; unioned along `extends`. **purlis:** only as approved on this machine, since the grant is part of the plane-trust fingerprint (ADR 0035, *Grants*) with a digest of any `bin/` script a tool names; a grant that changed since prompts until the plane is approved again | stable | `charter/persona.py:1580`, `charter/toolgate.py:896` |
| `agent-tools` | CSV of harness tool names | absent = sub-agent inherits every tool | Emitted as the agent's `tools:`; MCP grants appended | stable | `charter/commands_persona.py:879`-`:892` |
| `disallowed-tools` | CSV | absent | Emitted as the agent's `disallowedTools:` (denylist) | stable | `charter/commands_persona.py:923` |
| `skills` | CSV of `[plugin:]skill` | absent | Preloaded into the sub-agent; emitted as `skills:`; linted against installed skills | stable | `charter/persona.py:1957`, `charter/commands_persona.py:908` |
| `draft` | `true/yes/1/on` (case-insensitive) truthy set | absent = not a draft | While set, **no** sub-agent is generated and any generated one is removed | stable | `charter/persona.py:1843`, `:1846`, `charter/commands_persona.py:1150` |
| `routing` | `off` \| `advise` \| `require` | absent/unknown → `off` | Drove the Python's UserPromptSubmit roster block. **Retired** in purlis (charter#369): read without error, acted on by nothing, and `purlis doctor` names the personas that still declare it. Personas reach the harness as sub-agents, which is where routing happens | retired | `charter/persona.py:936`, `:939`, `charter/hooks.py:8518` |
| `routes-to` | CSV of persona names | absent | Priority order for the roster (never restricts). **Retired** with `routing` — there is no roster | retired | `charter/persona.py:962`, `:1001` |
| `activity` | `orchestrator` \| `standby` \| `advisory` | absent | Declares memory volume is not a usage signal; changes `persona stats` status | stable | `charter/persona.py:2408`, `:2444` |
| `dispatch-isolation` | `worktree` | absent | Emits `isolation: worktree` into the agent and a sentence into its description | stable | `charter/commands_persona.py:802`, `:916` |
| `model` | string | absent | Passed through verbatim into the agent frontmatter | stable | `charter/persona.py:305`, `charter/commands_persona.py:953` |
| `color` | string | absent | Passed through verbatim | stable | same |
| `memory` | string (truthy) | absent | Passed through verbatim **and** adds the "two memory stores" note to the body | stable | `charter/commands_persona.py:962` |

Inheritance merge (`charter/persona.py:872`-`:909`): iterate the chain root→child; every
truthy scalar key overwrites (child wins, `:892`); `tools`, `agent-tools`, `uses` are
order-preserving unions, parent first, deduped, and `uses` drops self (`:894`-`:896`);
`extends` itself is not copied into the merged meta; charters are concatenated, each
non-root ancestor's body prefixed with
`\n\n---\n\n### ⤷ \`{child}\` extends \`{parent}\` — its own charter\n\n` (`:900`);
`meta["name"]` is forced back to the queried name (`:905`); merged list keys are re-joined
with `", "` (`:907`-`:908`).

---

### `personas/<name>.md` (legacy flat layout)

- **Format:** same file as above, one level up.
- **Status:** stable — still resolved for read on old checkouts.
- **Tier:** Plane, legacy — committed; read on old checkouts, written by nothing.
- **Written by:** nothing any more; `charter/persona.py:2493` (`migrate`) moves it to
  `personas/<name>/persona.md` and scaffolds `memory/` + `refs/`.
- **Read by:** `charter/persona.py:172` (`def_path` prefers the directory layout, falls back
  here, else returns the canonical new path), `charter/persona.py:230` (`list_personas`).
- **Git:** committed.
- **Encoding details:** identical parsing. A stem of `readme` (any case) is not a persona.

---

### `personas/<name>/memory/MEMORY.md`

- **Format:** Markdown; a header block then one link line per memory.
- **Status:** stable — committed, hand-editable, read by the SessionStart hook, `doctor`,
  `curate` and `persona recall`.
- **Tier:** Plane — committed.
- **Written by:** `charter/persona.py:2265` (`scaffold_memory`, header at `:2277`),
  `charter/memstore.py:138` (`index_append`, appends one line), `charter/memstore.py:472`
  (`_drop_index_line`, the only truncating write), `charter/memstore.py:86` (`ensure_index`,
  used by workspace memory; `O_EXCL` create at `:97`).
- **Read by:** `charter/hooks.py:6823` (`_read_index` → SessionStart memory digest,
  `charter/hooks.py:6908`), `charter/memstore.py:259` (`_listed` → `index_drift` at `:217`,
  `doctor`, `curate.report` at `charter/curate.py:71`),
  `charter/commands_persona.py:1267` (`persona recall` prints it verbatim).
- **Git:** committed.
- **Encoding details:**
  - Scaffold header (`charter/persona.py:2277`), exactly:
    `# Memory Index — {name|"shared (all personas)"}\n\nOne line per memory; each links a file holding a single durable fact.\nWritten by the persona as it learns; committed and shared.\n`
    — note **no blank line** between the header and the first appended entry (confirmed on
    the live plane and in a temp plane).
  - `index_append` fallback header when the file is missing: `# Memory Index\n\n`
    (`charter/memstore.py:142`).
  - Entry line: `- [{title}]({filename})\n`, appended in `"a"` mode, chronological by write
    order (`charter/memstore.py:143`-`:144`). No sorting, no dedupe.
  - Readers match `- [` prefix (`charter/hooks.py:6851`) and the link regex
    `\(([A-Za-z0-9][\w.-]*\.md)\)` (`charter/memstore.py:214`). **In purlis** a line
    starting `- [` lists only the file its leading element links, the rule the workspace index
    above records (SI-9e).
  - `MEMORY.md` is excluded from the memory-file glob by name
    (`charter/memstore.py:208`).
  - Deletion rewrites the file as the surviving lines joined with `\n` plus one trailing
    `\n` (none when empty) — `charter/memstore.py:489`-`:490`.
  - **In purlis**, an edit retitles a memory's line in place and an unarchive appends one
    back ([Editing and archiving a memory](#editing-and-archiving-a-memory-purlis)).

---

### `personas/<name>/memory/<slug>.md` (and `personas/_shared/memory/<slug>.md`)

- **Format:** Markdown; fixed 3-part shape.
- **Status:** stable — committed, shared with the team, read by every recall path and by the
  session briefing.
- **Tier:** Plane — committed. `[memory] share` decides only whether purlis commits and pushes it.
- **Written by:** `charter/memstore.py:101` (`write`) via `charter/persona.py:2298`
  (`remember`), from `purlis persona remember` (`charter/commands_persona.py:1188`).
  **In purlis, also rewritten in place** by `purlis persona edit-memory <name> <slug>
  [--shared]` and the window's memory tab (`memstore::edit`), and moved to and from `archive/`
  by `persona archive-memory|unarchive-memory` — the rules are
  [Editing and archiving a memory](#editing-and-archiving-a-memory-purlis) — and moved in
  from or out to another scope by `persona move-memory`, `workspace move-memory` and the
  window's Move ([Moving a memory between scopes](#moving-a-memory-between-scopes-purlis)).
- **Read by:** `charter/memstore.py:169`/`:197` (`files`/`read_files` — the one gate),
  `:289` (`entries`), `:367` (`search`), `:408` (`duplicates`), `:429` (`resolve`);
  `charter/persona.py:2330`/`:2356`/`:2411`; `charter/recall.py:204` (`purlis recall`);
  `charter/curate.py:38`; `charter/statusline.py:1779`; `charter/render.py:95`;
  `charter/hooks.py:6866` (digest counts).
- **Git:** committed. Whether purlis itself commits/pushes it is `[memory] share`
  (below).
- **Encoding details:**
  - Content, exactly (`charter/memstore.py:131`-`:132`):
    `# {title}\n\n_{YYYY-MM-DD HH:MM} · {kind}_\n\n{text}\n`
    — `·` is U+00B7 with single spaces; `kind` is `persistent` for committed persona memory,
    `ephemeral` for scratch (`charter/persona.py:2317`).
  - Timestamp is **local, naive** `datetime.now()` (`charter/memstore.py:64`), minute
    precision. Persona memory is never `timestamped=` (no `YYYYMMDD-HHMMSS-` filename
    prefix) — `charter/persona.py:2320`, `charter/memstore.py:119`.
  - `text` is `.strip()`ed; empty raises `ValueError` (`charter/memstore.py:107`-`:109`).
  - Title = explicit `--title` or the first line of the body, stripped, capped at 72
    (`charter/persona.py:2315`, `charter/memstore.py:33`, `:110`).
  - Filename = `slug(title) + ".md"`; slug = lowercase, every run of non-`[a-z0-9]`
    replaced by `-`, leading/trailing `-` stripped, then **truncated to 48 chars** (so a
    trailing `-` can survive), `"note"` when empty (`charter/memstore.py:23`, `:26`-`:28`).
    Collision → `-2`, `-3`, … before `.md` (`charter/memstore.py:123`-`:124`).
  - Write goes through `config.write_for` (`charter/memstore.py:131`); outside
    `.charter/` that is a plain `open`, so the umask decides the mode. Not atomic.
    purlis (#434) replaces the file whole through `rewrite::replace` (temp beside,
    fsync, rename), under the store's own mode rules, and never writes through a link at
    the file; the index's line removal is replaced the same way.
  - Date used by `stats`/`recall --since`: in-body `_YYYY-MM-DD` stamp
    (`^_(\d{4}-\d{2}-\d{2})[ T]`, multiline — `charter/memstore.py:147`), falling back to a
    `YYYYMMDD-` filename prefix (`:160`).

| Field | Type | Required / default | Meaning | Status | Source |
|---|---|---|---|---|---|
| `# <title>` line | first `# ` line | required (written always) | Title used by index, search (3× weight) and `entries` | stable | `charter/memstore.py:132`, `:325` |
| `_<stamp> · <kind>_` line | `%Y-%m-%d %H:%M` + kind | required (written always) | Recording date; `kind` ∈ `persistent`/`ephemeral` | stable | `charter/memstore.py:132`, `charter/persona.py:2317` |
| body | free Markdown | required, non-empty | The fact | stable | `charter/memstore.py:107` |

---

### `personas/<name>/memory/archive/<slug>.md`

- **Format:** same memory file, moved.
- **Status:** stable — committed; a reversible retire that drops the memory out of every
  glob (the glob is flat, so `archive/` is invisible to `files()`).
- **Tier:** Plane — committed.
- **Written by:** `charter/memstore.py:503` (`archive`) via `charter/curate.py:92`
  (`apply_safe`, exact-duplicate collapse) — reached by `purlis persona optimize --apply`.
  **In purlis, also** `persona archive-memory` / `workspace archive` and the window's
  Delete (`memstore::archive_one`), and emptied back into the store by `unarchive-memory` /
  `workspace unarchive`, the window's Undo and its archive tab's Restore memory
  (`memstore::unarchive`) —
  [Editing and archiving a memory](#editing-and-archiving-a-memory-purlis).
- **Read by:** nothing in purlis (deliberately out of the active set); git history and
  humans only. In purlis, `memstore::unarchive` reads the one file it restores, and the
  window's archive tab lists and shows the store's `archive/`, read-only
  (`Persona::archived_memories` / `Workspace::archived_memories`, KN-4) — never in a briefing,
  a recall or a memory list.
- **Git:** committed.
- **Encoding details:** `rename` into `<mem_dir>/archive/`, created with `mkdir_for`;
  collision → `<stem>-2.md` (`charter/memstore.py:521`-`:523`); the index line is dropped
  (`:525`).

---

### `personas/<name>/memory/.gitkeep`, `personas/<name>/refs/.gitkeep`

- **Format:** empty file.
- **Status:** stable — committed, and the only reason an empty `memory/`/`refs/` survives a
  clone. (A front-door persona has these and **no** `MEMORY.md`/`README.md`.)
- **Tier:** Plane — committed.
- **Written by:** `charter/commands.py:2680` (`_ensure_front_door`, `purlis init` only).
- **Read by:** nothing (`files()` filters to `*.md`, `charter/memstore.py:208`).
- **Git:** committed.

---

### `personas/<name>/refs/README.md` and `personas/<name>/refs/**`

- **Format:** Markdown documents, arbitrarily nested.
- **Status:** stable — committed curated docs; `purlis recall` reads them as a default
  scope; the operator writes them by hand.
- **Tier:** Plane — committed.
- **Written by:** `charter/persona.py:2283`-`:2291` (`scaffold_memory` writes only
  `README.md`); everything else is hand-written, except that **in purlis a project
  template** (FR-17) writes `personas/<stack>-reviewer/refs/REVIEW.md`, the checklist its
  reviewer persona reviews by, when it makes that persona. It is the plane's copy: nothing is
  written into a repo.
- **Read by:** `charter/recall.py:128`-`:133` + `:142` (`_ref_dirs`, recursive: every
  subdirectory is offered as its own source because `memstore.files` is a flat glob);
  `charter/commands_persona.py:322` (`persona show` counts refs, excluding `README.md`).
- **Git:** committed.
- **Encoding details:** `README.md` scaffold text at `charter/persona.py:2287`-`:2291`:
  `# References — {who}\n\nCurated docs, links, and snippets this role collects. Committed and shared. Never store secrets here — those live only in the vault.\n`.
  Refs are read as memory files (`*.md`, `MEMORY.md` excluded), so an `# ` heading is the
  title and a `_date_` line, if present, is the date.

---

### `personas/<name>/mcp.json`

- **Format:** JSON — `.mcp.json`'s schema plus charter-only `secrets` / `secret_files` maps
  per server.
- **Status:** stable — committed, hand-written (often pasted from a server's README), read
  by `sync-agents`, `lint`, `persona use` and the consent flow.
- **Tier:** Plane — committed.
- **Written by:** nothing in purlis — hand-edited only.
- **Read by:** `charter/persona.py:653` (`_mcp_declared`) → `mcp_servers` (`:622`),
  `mcp_refused` (`:641`), `mcp_credentialed` (`:799`), `mcp_withheld` (`:832`),
  `mcp_render_entry` (`:730`); `charter/commands_persona.py:876`/`:926` (render),
  `charter/commands_persona.py:1996` (`--approve-mcp`), `charter/persona.py:2114` (`lint`).
- **Git:** committed.
- **Encoding details:**
  - Only `doc["mcpServers"]` is read and it must be a dict; any `OSError`/`ValueError` is
    swallowed and the file skipped (`charter/persona.py:673`-`:677`).
  - Unioned along the `extends` chain **reversed** (parent first, child wins) —
    `charter/persona.py:667`.
  - Server names are bounded at the boundary: `[A-Za-z0-9_][A-Za-z0-9._-]{0,63}`, `fullmatch`
    (`charter/persona.py:549`, `:552`). A refused name drops the server and is reported as a
    lint error and a `sync-agents` warning.
  - Rendering (`charter/persona.py:730`): `secrets`/`secret_files` keys are removed from the
    emitted entry; with a real vault (`vault: none` → no vault, `:689`) **and** a recorded
    approval (`charter/mcpseen.py`), the entry becomes
    `command: "purlis"`, `args: ["secret","exec",<vault>, "--env","NAME=key"…, "--file","NAME=key"…, "--stream"|"--exec", "--", <original command>, *<original args>]`
    (`charter/persona.py:781`-`:794`). `--stream` whenever any `secret_files` is present.

| Field | Type | Required / default | Meaning | Status | Source |
|---|---|---|---|---|---|
| `mcpServers` | object | required (else no servers) | Map of server name → entry | stable | `charter/persona.py:674` |
| `mcpServers.<name>` | string key | must match `_MCP_NAME_RE` | Server name; also interpolated into `mcp__<name>__*` | stable | `charter/persona.py:549`, `charter/commands_persona.py:890` |
| `…command`, `…args`, `…env`, any other key | as `.mcp.json` | passed through | Emitted verbatim into the agent (all unknown keys are kept) | stable | `charter/persona.py:747` |
| `…secrets` | `{ENV_VAR: vault-key}` | optional | Vault value injected as an env var | stable | `charter/persona.py:751`, `:782`-`:783` |
| `…secret_files` | `{ENV_VAR: vault-key}` | optional | Vault value materialised to a 0600 file whose path is the env var | stable | `charter/persona.py:758`, `:784`-`:785` |

---

### `personas/<name>/bin/<script>`

- **Format:** any executable file.
- **Status:** stable — committed, hand-written, run by the dispatched agent by path and
  vouched for by the Bash tool gate.
- **Tier:** Plane — committed.
- **Written by:** nothing in purlis.
- **Read by:** `charter/persona.py:580` (`bin_scripts`, union along the reversed lineage,
  executable files only), `charter/persona.py:610` (`bin_issues` — a non-executable file is
  a lint warning), `charter/commands_persona.py:978`-`:1010` (the "You carry your own
  executables" block in the generated agent), `charter/toolgate.py` (inode check for a path
  in command position).
- **Git:** committed, mode bit included.
- **Encoding details:** flat `sorted(d.iterdir())` per ancestor; only `f.is_file() and
  os.access(f, os.X_OK)` counts (`charter/persona.py:604`-`:606`). `uses:`/`borrows:` do
  **not** carry scripts. Paths are rendered plane-relative in the agent
  (`charter/commands_persona.py:858`, `:1003`).

---

### `personas/<name>/curation/<id>.md` — a curation action

New in purlis (ADR 0061); the Python charter never read or wrote it. A **curation action**
is a chat the operator can open on a workspace, a persona or the plane with a prompt already
typed into it and never sent. This file declares one, and the persona it sits under is the one
that runs it.

- **Format:** Markdown with the same line-based frontmatter as
  [`persona.md`](#personasnamepersonamd) (`crates/purlis-core/src/personas.rs`,
  `frontmatter` and `charter_body`: not YAML, quotes kept, a key matched exactly). The body
  below the frontmatter is the prompt template.
- **Status:** stable — hand-edited, committed, and read by the app's menus and by
  `purlis curation show`.
- **Tier:** Plane — committed.
- **Written by:** the operator by hand, or `purlis persona curation add <name> <id>`, which
  writes it only when it would read back without an error and never over an existing file.
  Removed by `purlis persona curation remove <name> <id>`, and with the whole persona by
  `purlis persona remove`.
- **Read by:** `crates/purlis-core/src/curation.rs` (`declared`, `resolve`, `lint`), which
  `purlis persona curation list`, `purlis curation show` and `purlis persona lint` all
  call.
- **Git:** committed, like everything else under `personas/<name>/`.
- **Encoding details:**
  - `<id>` is the file's stem and must be a name purlis could mint, the persona-name grammar
    `[a-z0-9][a-z0-9._-]*`. The action's full id is `<name>/<id>`. purlis's own built-in
    actions are `charter/<id>` and are not files at all: they ship inside the binary.
  - Only `*.md` files directly under `curation/` are actions. A dotfile (`.gitkeep`) or a
    subdirectory is not one and is not reported.
  - A file that is not UTF-8, is larger than a memory file may be, or resolves out of the plane
    is an error, not an action.
  - `extends:` does not carry curation actions. Each persona offers only the files in its own
    `curation/`.
  - **The template** substitutes exactly four variables: `{subject.kind}` (`workspace`,
    `persona` or `plane`), `{subject.name}`, `{subject.path}` (the workspace's directory, the
    persona's directory, or the plane root) and `{plane.root}`. Substitution is one plain
    pass: a substituted value is never read again for variables, nothing is expanded by a
    shell, and no environment variable, vault or secret is ever read. Any other
    `{word}` — braces around letters, digits, `_`, `.` or `-` — is an error, so a typo is
    caught rather than typed into a chat. Braces around anything else (`{"a": 1}`) are text.
    `{{` is a literal `{` and `}}` a literal `}`, in the same pass, as in Rust's `format!` and
    Python's `str.format`: `{{word}}` types `{word}`, and `{{{subject.name}}}` types the name
    in braces. A single `}` on its own stays text, as it always was.
  - **An action that has an error is not offered**, and the list that leaves it out says so
    with a warning naming the file. So is one whose id or label is one of purlis's built-in
    actions' (`charter/safe-remove`, `charter/compact`, `charter/add-curation-action`; labels
    compared case-insensitively): a built-in cannot be overridden or impersonated. Nor is any
    action of a persona named `charter`, whose ids would read `charter/<id>`; the name is
    reserved (see [`personas/`](#personas)).
  - **Keep the prompt to one short line.** It is typed to be read before the operator presses
    Enter, and a harness draws a longer paste as a placeholder nobody can read: Claude Code
    2.1.283 over 800 characters or at 4 lines, Codex 0.147.0 over 1,000 characters, opencode
    1.18.32 over 150 characters or at 3 lines (ADR 0061, amended 2026-09-27). Such a prompt is
    still a valid action — the file format does not change — but `purlis persona lint` warns
    about it, naming the harness, and the app opens no chat for it on that harness. The lint
    renders the prompt for a subject with a long name (`LONG_SUBJECT_NAME`, 43 characters) on
    each kind in `on`. Name a skill in the prompt and let the skill hold the steps, as
    purlis's own three do.

| Field | Type | Required / default | Meaning | Status | Source |
|---|---|---|---|---|---|
| `label` | string (one line) | required, non-empty | What the menu and the palette show | stable | `crates/purlis-core/src/curation.rs` |
| `on` | CSV of `workspace`, `persona`, `plane` | required, at least one; an unknown kind is an error | The kinds of subject the action is offered on | stable | same |
| `runs-in` | `subject` \| `plane` | absent → a workspace subject runs in its directory, a persona or the plane at the plane root; an unknown value is an error | The directory the chat starts in. `subject` is the subject's own directory (the workspace's, `personas/<name>/`, or the plane root) | stable | same |

An unknown key is a warning, as it is in `persona.md`, and a repeated key is an error.

---

### `personas/_shared/` (`memory/`, `refs/`)

- **Format:** exactly the per-persona `memory/` and `refs/` above.
- **Status:** stable — committed cross-persona namespace.
- **Tier:** Plane — committed.
- **Written by:** `charter/persona.py:2294` (`ensure_shared` → `scaffold_memory(..., shared=True)`),
  reached from `persona create` (`charter/commands_persona.py:125`) and `persona migrate`
  (`:1842`); memories by `purlis persona remember --shared`.
- **Read by:** `charter/recall.py:124` (`shared` scope, default-on), `:132` (`refs:shared`),
  `charter/hooks.py:6910` (shared half of the SessionStart digest),
  `charter/commands_persona.py:1259` (`persona recall`), `persona stats _shared` /
  `persona optimize` (`charter/commands_persona.py:1571`, `:1757`).
- **Git:** committed.
- **Encoding details:** index header says `# Memory Index — shared (all personas)`
  (`charter/persona.py:2276`). `_shared` is outside the persona alphabet on purpose, so no
  persona can take the name; `list_personas` never returns it.

---

### `personas/.default` (legacy)

- **Format:** plain text — one persona name plus optional whitespace.
- **Status:** stable — committed, hand-written, read on every turn by the persona resolver.
- **Tier:** Plane, legacy — committed, and still read.
- **Written by:** nothing writes it any more; `purlis persona default --clear` unlinks it
  (`charter/commands_persona.py:580`).
- **Read by:** `charter/persona.py:915` (`default_persona`) → `_resolved` (`:1407`),
  `plane_default` (`:1058`).
- **Git:** committed.
- **Encoding details:** read, `.strip()`ed; the value must pass `reference_ok` **and** the
  persona's definition file must exist, else the rung resolves to nothing
  (`charter/persona.py:930`). Ranked **below** `charter.toml` `[persona] default`
  (`charter/persona.py:1404`-`:1409`).

---

### `personas/_dispatch/<YYYY-MM>.<device>.jsonl`

- **Format:** JSON Lines, append-only; one object per line.
- **Status:** stable — committed (so the tally merges across machines), written by a hook
  process and read by the CLI, the status line and the frame switcher.
- **Tier:** Plane — committed, so the tally merges across machines.
- **Written by:** `charter/dispatch.py:65` (`record`), `:98` (`record_advice`), `:134`
  (`record_resume`), `:170` (`record_handoff`) — driven by
  `charter/hooks.py:8170` (PostToolUse Task/Agent), `charter/hooks.py:8141` (PostToolUse
  SendMessage), `charter/hooks.py:8518` (UserPromptSubmit roster block),
  `charter/commands_handoff.py:261`.
- **Read by:** `charter/dispatch.py:322` (`_read_all`, memoised per file by
  `(mtime_ns, size)` at `:261`) → `tally` (`:343`), `last_seen` (`:354`), `advice_tally`
  (`:118`), `resume_tally` (`:208`), `generic_share` (`:368`),
  `routed_since_first_advice` (`:222`); `charter/persona.py:1061` (`_dispatches` → `by_use`
  → status line and frame switcher), `charter/commands_persona.py:1577` (`persona stats`).
- **Git:** committed; `charter/hooks.py:8189` (`_commit_dispatch`) commits/pushes it
  reactively **only** when `[memory] share` is `commit`/`push` (default `local` → left
  uncommitted for the operator).
- **Encoding details:**
  - Path: `personas/_dispatch/{when:%Y-%m}.{host}.jsonl` (`charter/dispatch.py:60`-`:62`),
    `when` is UTC now (`:56`).
  - `<device>` is the device's name for its logs, as the landing log names its file (FD-25:
    the device id from the machine store, never minted by a hook). Only before an id is
    minted, or where the store keeps none, is it Python's `host`: `socket.gethostname()`
    first label, `[^A-Za-z0-9_-]` stripped, 32 chars, `"unknown"` when empty
    (`charter/dispatch.py:50`-`:53`). No env override. Month files an earlier version named
    by a hostname are read beside the new ones; FR-9 renames them (#607).
  - One line = `json.dumps(obj, sort_keys=True) + "\n"` → **keys are alphabetical**, ASCII
    escaped, no spaces beyond `json.dumps` defaults (`", "`/`": "`).
  - Written with `os.open(..., O_WRONLY|O_CREAT|O_APPEND, 0o644)` and one `os.write` — no
    lock; `record_handoff` additionally passes `O_NOFOLLOW` (`charter/dispatch.py:198`).
  - `ts` = `datetime.now(timezone.utc).isoformat(timespec="seconds")` →
    `2026-09-17T14:01:20+00:00`. A row with no timezone is read as UTC
    (`charter/dispatch.py:338`).
  - A row is kept by the reader only if it is a dict with `agent` or `event`
    (`charter/dispatch.py:316`); unparseable lines are skipped silently.

| Field | Type | Required / default | Meaning | Status | Source |
|---|---|---|---|---|---|
| `ts` | ISO-8601 UTC, seconds | always | When | stable | `charter/dispatch.py:77` |
| `agent` | string | on dispatch + resume rows | `subagent_type` — a persona name or a generic (`general-purpose`, `Explore`, `claude`, `Plan`) | stable | `charter/dispatch.py:77`, `:43` |
| `event` | `advice` \| `resume` \| `handoff` | absent on a plain dispatch (that absence *is* what `tally` counts) | Row kind | stable | `charter/dispatch.py:95`, `:131`, `:167`, `:351` |
| `placement` | `here` \| `elsewhere` | handoff rows only | Was the new chat in this workspace | stable | `charter/dispatch.py:190`, `charter/commands_handoff.py:261` |
| `created` | bool | handoff rows only | Whether the handoff created the workspace | stable | `charter/dispatch.py:189` |

Nothing else may appear: no prompt, description, workspace name or persona name on a
handoff row (`charter/dispatch.py:170` docstring).

---

### `personas/_dispatch/<YYYY-MM>.<host>.backfill.jsonl`

- **Format:** JSON Lines, **rewritten whole** (not appended).
- **Status:** stable — committed, read by the same tally readers as the live file.
- **Tier:** Plane — committed.
- **Written by:** `charter/dispatch.py:476` (`backfill`), from
  `purlis persona dispatch-backfill` (`charter/commands_persona.py:1714`, which then
  commits the files at `:1734`-`:1736`).
- **Read by:** the same `_read_all` glob (`charter/dispatch.py:327`); excluded from
  `_live_keys` (`:417`) so it never de-duplicates against itself.
- **Git:** committed.
- **Encoding details:** every existing `*.backfill.jsonl` is unlinked, then each month file
  is written as the **sorted set** of its `json.dumps({"ts","agent"}, sort_keys=True)` lines
  joined by `\n` plus a trailing `\n` (`charter/dispatch.py:500`-`:503`). Rows carry only
  `ts` and `agent`. Source: this project's Claude Code transcripts under
  `~/.claude/projects/<plane path with "/"→"-">` (`charter/dispatch.py:383`-`:394`,
  `:443`); `ts` is the transcript timestamp truncated to 19 chars (`:472`), so backfilled
  rows have **no timezone suffix** and are read as UTC. `last_backfill()` uses the file
  mtime (`:430`).

---

### `personas/_skills/<YYYY-MM>.<device>.jsonl`

- **Format:** JSON Lines, append-only.
- **Status:** stable — committed, written by a hook, read by `persona stats`.
- **Tier:** Plane — committed.
- **Written by:** `charter/skilluse.py:67` (`record`) from `charter/hooks.py:8054`
  (PostToolUse on the `Skill` tool; reads `tool_input.skill` or opencode's `name`).
- **Read by:** `charter/skilluse.py:95` (`_read_all`) → `by_persona` (`:121`), `drift`
  (`:133`); `charter/commands_persona.py:1622` (`persona stats` SKILLS block).
- **Git:** committed. Nothing commits it automatically (no `_commit_dispatch` equivalent).
- **Encoding details:** path `personas/_skills/{%Y-%m}.{device}.jsonl`
  (`charter/skilluse.py:59`, `:62`-`:64`), `<device>` named as the dispatch log's is (FD-25;
  Python's host derivation, `:48`-`:51`, only before a device id is minted); same
  `json.dumps(..., sort_keys=True)` + `\n`, same `O_APPEND`, `0o644` (`:80`-`:85`). Rows
  without a truthy `skill` are ignored on read (`:116`).

| Field | Type | Required / default | Meaning | Status | Source |
|---|---|---|---|---|---|
| `ts` | ISO-8601 UTC, seconds | always | When | stable | `charter/skilluse.py:80` |
| `skill` | string | always (row dropped if empty) | Skill name as the harness reported it; may be `plugin:skill`, compared on the leaf | stable | `charter/skilluse.py:82`, `:128` |
| `persona` | string or **`null`** | always present; `null` when no persona is active | Which persona invoked it | stable | `charter/skilluse.py:82` |

---

### `.claude/agents/<name>.md` (generated sub-agent)

- **Format:** Markdown with YAML frontmatter (the harness's format).
- **Status:** stable — committed, read by Claude Code itself, and regenerated byte-for-byte
  by `lint`'s staleness check (`charter/commands_persona.py:1437` compares
  `path.read_text().strip()` with a fresh render — so a Rust writer must match exactly or
  every persona lints stale).
- **Tier:** Plane, rebuildable — committed, and regenerated byte for byte from the persona.
- **Written by:** `charter/commands_persona.py:1164` (`_write_agent`), from
  `purlis persona sync-agents` (`:2035`/`:2066`), `persona create` (`:129`) and
  `persona migrate` (`:1854`); in purlis, also a project template for each persona it
  makes (FR-17). Removed by `_remove_agent` (`:1168`) for a draft, an
  unreadable frontmatter key, a removed persona, or an orphan sweep (`:2101`-`:2106`).
- **Read by:** Claude Code (sub-agent definition); `charter/commands_persona.py:1434`
  (staleness), `charter/freshness.py:24` (restart hint),
  `charter/workspace.py:2081`/`:4127` (mirrored into a workspace's checkouts).
- **Git:** committed (`.gitignore` covers only `.claude/settings.local.json` —
  `charter/commands.py:1100`; `git ls-files` on the live plane lists all five agents).
- **Encoding details:**
  - Whole file = `"---\n" + "\n".join(fm) + "\n---\n" + body` (`charter/commands_persona.py:1117`);
    `body` starts with the marker comment and ends with a trailing newline.
  - Frontmatter lines, in this fixed order (`charter/commands_persona.py:870`-`:955`):
    1. `name: <name>`
    2. `description: "<escaped>"` — `_yaml_str` (`:773`): newlines → spaces, strip, then
       `\` → `\\` and `"` → `\"`, wrapped in double quotes.
    3. `tools: <agent-tools>[, mcp__<server>__*…]` — only when `agent-tools` is set; MCP
       grants appended for each declared server whose name passes `mcp_name_ok` and is not
       already mentioned (`:879`-`:892`).
    4. `skills: a, b` — when `skills:` is declared (`:908`-`:910`).
    5. `isolation: worktree` — when `dispatch-isolation: worktree` (`:916`-`:917`).
    6. `disallowedTools: <verbatim>` — when `disallowed-tools` is set (`:923`-`:924`).
    7. `mcpServers:` followed by `  - {"<server>": {…}}` per server, **sorted by server
       name**, each a one-line JSON object of the whole single-key mapping via
       `contain.json_line` (`:926`-`:952`).
    8. passthrough keys in declaration order `model`, `color`, `memory` (`:953`-`:955`,
       `charter/persona.py:305`).
  - Marker (the file's identity as generated):
    `<!-- GENERATED by \`charter persona sync-agents\` from <plane-relative persona.md> — edit the persona, not this file. -->`
    (`charter/commands_persona.py:766`, `:1105`). A file without the marker is never
    overwritten or deleted (`:1159`, `:1171`).
  - Body sections, in order (`charter/commands_persona.py:1105`-`:1116`): intro line naming
    the persona and role → the **resolved** purlis (inheritance concatenated) → `## As a
    persona sub-agent` with the forge credential rule (`_credential_rule`, `:811` — derived
    from the plane's declared forges, so `gh` vs `glab` varies per plane), the credential
    block (`:1077` or the `vault: none` variant at `:1085`), the `bin/` block, the
    `uses:`/`borrows:` block (wording differs by whether `borrows:` is declared, `:1020`
    vs `:1029`), the `memory:` note (only when `memory:` is set), the generic handoff line,
    the conventions line → `## Memory`.
  - Description when neither `agent-description` nor `description` is set
    (`charter/commands_persona.py:778`-`:808`): `The {role} persona. Delegate to it for
    {delegate-when}.` (or `Delegate {role.lower()} tasks to it.`) plus one of
    ` Runs {tools} and pulls credentials from the '{vault}' vault.` /
    ` Runs {tools}. Holds no credentials of its own.` /
    ` Pulls credentials from the '{vault}' vault.` / ` Holds no credentials of its own.`,
    plus a worktree sentence when `dispatch-isolation: worktree`.
  - Written with plain `write_text` into `<tree>/.claude/agents/`; `sync-agents` run from a
    linked worktree writes into that worktree (`charter/commands_persona.py:2048`-`:2063`).

---

### `.charter/persona-state/ephemeral/<session>/<name|_shared>/<slug>.md`

- **Format:** the memory-file shape above, `kind` = `ephemeral`.
- **Status:** internal — one module writes and reads it as session scratch, it is
  gitignored, and it is deleted by the GC. Deleting it loses that session's scratch notes
  only; nothing regenerates them, and no other process depends on them.
- **Tier:** Clone state, transient — one session's scratch.
- **Written by:** `charter/persona.py:2298` (`remember(..., ephemeral=True)` →
  `memstore.write(..., index=False)` at `:2320`), from
  `purlis persona remember --ephemeral`.
- **Read by:** `charter/persona.py:2330` (`memories(..., ephemeral=True)`),
  `charter/recall.py:122` (the opt-in `ephemeral` scope),
  `charter/commands_persona.py:1271` (`persona recall`), `charter/statusline.py:1779`.
- **Git:** gitignored — it is under `/.charter/`, ignored by `_GITIGNORE_BASELINE`
  (`charter/commands.py:1095`).
- **Encoding details:** `<session>` is `session.bucket()` — `$CHARTER_SESSION_ID`, else
  `$CLAUDE_CODE_SESSION_ID`, else the literal `nosession`
  (`charter/session.py:65`-`:66`, `:23`, `charter/persona.py:205`-`:215`). No `MEMORY.md` is
  written here (`index=False`). Directories are created 0700 via `config.mkdir_for`
  (`charter/memstore.py:83`), files 0600 via `config.open_for`. GC: a session directory that
  is not the current one and whose newest mtime is older than 6h is `rmtree`d
  (`charter/persona.py:2462`-`:2490`), from `purlis persona _gc`
  (`charter/commands_persona.py:1917`) on SessionStart.

---

### `.charter/persona-state/trace/<session>.jsonl`

- **Format:** JSON Lines, append-only.
- **Status:** stable (by the brief's rule) — written by hooks and CLI commands in one
  process and read by `purlis trace` / `purlis persona recall` in another. Machine-local
  and safe to delete (the history is lost; nothing regenerates it), so it is a borderline
  call — see the Appendix.
- **Tier:** Clone state, transient — per-session history nothing regenerates and nothing but `persona stats` reads.
- **Written by:** `charter/trace.py:69` (`record`) — persona-relevant events:
  `persona-use` (`charter/persona.py:1533`), `memory` (`charter/persona.py:2323`), `note`
  (`charter/commands_persona.py:1821`), `dispatch`/`resume`/`skill` (`charter/hooks.py`),
  plus guard/secret events owned by other areas.
- **Read by:** `charter/trace.py:93` (`read`), `:106` (`for_persona`),
  `charter/commands_persona.py:1280`, `:1824`, `charter/commands_persona.py:1868`
  (`purlis trace`).
- **Git:** gitignored (under `/.charter/`).
- **Encoding details:** one `contain.json_line(rec) + "\n"` per event, appended through
  `config.open_for` (0600 under the state dir). Every record carries
  `ts` = **local naive** `datetime.now().isoformat(timespec="seconds")` and `event`; other
  fields are the call's kwargs with `None` dropped, in insertion order (**not** sorted —
  unlike the `_dispatch` store). `<session>` is `session.bucket()`.
- **Collected (purlis):** when the app opens the plane, a trace last appended to 30 days
  or more before is removed, unless a chat the plane's reopen record will bring back is its
  session, or it records a secret handed out (kept until AU-5 audits those events, V71;
  `retention::on_open`). `nosession.jsonl` follows the same rule.

---

### `.charter/reports/<id>.json`

- **Format:** JSON object, `indent=2`, no trailing newline.
- **Status:** stable — written by `cli.main`'s crash handler in one process and read by
  every later `purlis report …` invocation; the Reporter reads and edits the drafts.
  (Document the **shape only** — contents may quote a Reporter's own prose.)
- **Tier:** Clone state, legacy — the Python charter's report drafts; the Rust `purlis report` keeps none (ADR 0059).
- **Written by:** `charter/report.py:302` (`_write`, via `config.write_for` into a 0700
  dir), from `record_bug` (`:374`) / `record_described` (`:384`) / `mark_sent` (`:630`);
  crash path `charter/cli.py:2028`.
- **Read by:** `charter/report.py:268` (`load`), `:281` (`_all` → `pending`,
  `all_reports`), `charter/commands_report.py`.
- **Git:** gitignored (under `/.charter/`).
- **Encoding details:** filename `<id>.json` where `id` is the 16-hex fingerprint —
  for a bug, `sha256(exception_type + "\n" + deepest purlis frame)[:16]`
  (`charter/report.py:91`-`:110`); for a described report,
  `sha256(kind + "\n" + scrubbed text)[:16]` (`:397`). Same id twice = the same file, with
  `occurrences` incremented (`:351`). Caps: 25 distinct pending reports
  (`charter/report.py:38`, `:359`), unsent drafts older than 30 days pruned on write
  (`:43`, `:325`).
- **Collected (purlis):** nothing here writes these any more, so when the app opens the
  plane it removes every `<16 lowercase hex>.json` last written 30 days or more before, sent or
  not (`retention::on_open`).

| Field | Type | Required / default | Meaning | Status | Source |
|---|---|---|---|---|---|
| `id` | 16 hex chars | always | Fingerprint = filename stem | stable | `charter/report.py:364` |
| `kind` | `bug` \| `gap` | always | Report kind (also an upstream label) | stable | `charter/report.py:365`, `:608` |
| `payload` | object | always | Closed allowlist; see below | stable | `charter/report.py:54` |
| `occurrences` | int | 1 | Times seen on this machine | stable | `charter/report.py:351`, `:367` |
| `first_seen` / `last_seen` | float epoch seconds | `time.time()` | Draft age (drives pruning) | stable | `charter/report.py:368`-`:369` |
| `issue_url` | string \| `null` | `null` | Set by `mark_sent` once filed | stable | `charter/report.py:370`, `:635` |
| `payload.charter_version` | string | always | `charter.__version__` | stable | `charter/report.py:121` |
| `payload.python_version` | string | always | `platform.python_version()` | stable | `charter/report.py:122` |
| `payload.os` | string | always | `platform.platform()` | stable | `charter/report.py:123` |
| `payload.subcommand` | string | bug only | Subcommand name (never argv) | stable | `charter/report.py:124` |
| `payload.exception_type` | string | bug only | Exception class name | stable | `charter/report.py:124` |
| `payload.frames` | list of `charter/<file>:<line> in <fn>` | bug only | purlis frames only, package-relative | stable | `charter/report.py:68`-`:88` |
| `payload.message` | string | bug only | `str(exc)` — free text, unvetted | stable | `charter/report.py:126` |
| `payload.text` | string | gap/described only | Scrubbed prose | stable | `charter/report.py:249` |
| `payload.scrubbed` | list of category labels (`[workspace]`, `[persona]`, `[vault]`, `[token-env]`, `home paths`) | gap/described only | What the scrub removed | stable | `charter/report.py:155`, `:254` |

Persona relevance of the scrub: `charter/report.py:164`-`:175` redacts every persona name
(`persona.list_personas()`) and every workspace/vault identifier out of a described report
before it is stored.

---

### `.charter/handbacks/` — reports back from handed-off chats

- **Format:** one JSON object per file, compact, no trailing newline.
- **Status:** **internal** — written by the app, taken by the `purlis` binary's own hooks.
  A report waits here from the moment a handed-off chat sends it (`purlis handoff report`)
  until the turn it is handed to.
- **Tier:** Clone state, transient — a report waiting to be taken by one chat's hook.
- **Written by:** `purlis_core::handback::leave` (purlis#259), from the app's answer to a
  report, and from `handback::orphan` when a chat with reports waiting is closed.
- **Read by:** `charter hook userpromptsubmit` (`chat-<n>/`, `<n>` from `$CHARTER_SESSION_ID`)
  and `charter hook sessionstart` (`workspace-<ws>/`, the session's workspace, or `plane-root/`
  for a session at the plane root), through
  `handback::take`, which **removes each file it reads**: a report reaches one turn.
- **Git:** gitignored (under `/.charter/`).
- **Layout:** `chat-<n>/` for a report to a chat the app has open, `workspace-<ws>/` for one
  whose chat has closed, and `plane-root/` for one whose chat worked at the plane root and has
  closed (SI-1b). Each file is `<nanoseconds since the epoch, 24 digits>-<uuid>.json`,
  so a directory reads in arrival order; it is written as `.<name>` and renamed into place, and a
  reader skips a name starting with `.`. An empty directory is removed by the reader.

| Field | Type | Meaning |
|---|---|---|
| `from` | str | the chat that reported, by the name it is shown under |
| `from_workspace` | str | the workspace that chat works in, or `plane root` |
| `to` | str | the chat that asked, by the name it is shown under |
| `to_workspace` | str | the workspace that chat handed off from — where the report goes when it is gone — or `plane root` for a chat that handed off from the plane root (SI-1b). Two words with a space, which no workspace name can be, so a reader that holds the field to the name rule drops the report rather than joining the words onto `workspaces/` |
| `summary` | str | the report: trimmed, at most 4,096 bytes, no control character but `\n` and no invisible one |

**Held again on the way in.** A file whose `summary` breaks the report rule, whose names break
the chat-name rule, whose workspaces are neither a workspace's name nor `plane root`, or which
is not JSON at all, is removed and
handed to nobody. What is handed over is quoted as data: every line of the summary behind `> `.

---

### `~/.config/charter/reporting-consent` (outside the plane)

- **Format:** plain text, one sentence.
- **Status:** stable — it is the consent record `purlis report send` gates on, written by
  one command and read by another, and the Reporter deletes it to withdraw.
- **Tier:** Machine, syncable, legacy — the Python `purlis report`'s consent; the Rust one keeps none (ADR 0059).
- **Written by:** `charter/report.py:472` (`grant_consent`) from
  `charter/commands_report.py:104`.
- **Read by:** `charter/report.py:468` (`has_consent`) — existence only; contents are never
  parsed.
- **Git:** outside the plane. Path is
  `$CHARTER_CONFIG_HOME` → `$XDG_CONFIG_HOME` → `~/.config`, then `charter/reporting-consent`
  (`charter/report.py:462`-`:465`) — deliberately per-human, not per-plane and not committed.
- **Encoding details:** exact body at `charter/report.py:475`-`:477`; parent created with a
  plain `mkdir(parents=True)`, written with `write_text` (umask decides the mode).

---

### `.charter/sessions/<sid>.persona`, `.charter/terminals/<tid>.persona`, `.charter/active-persona`

- **Format:** plain text — the persona name plus `\n`.
- **Status:** stable — written by `purlis persona use` in one process and read by every
  other purlis process in that session/terminal (status line, hooks, tool gate, frame).
  (The `sessions/`/`terminals/` directories themselves belong to the workspaces area; the
  `.persona` files in them are `persona.py`'s.)
- **Tier:** Clone state, transient — per-session and per-pane pointers; `active-persona` is Clone state (see its own entry).
- **Written by:** `charter/persona.py:1488` (`set_active`, `config.write_for` at `:1528` →
  0600 under a 0700 dir); the plane-wide file only when there is neither a session id nor a
  terminal id (`:1529`-`:1530`). Removed by `clear_active` (`:1541`).
- **Read by:** `charter/persona.py:1199` (`_read_pointer`) → `_resolved` (`:1379`) →
  `resolve_active`/`selection`, `for_session` (`:1225`), `pointers_naming` (`:1208`).
- **Git:** gitignored (under `/.charter/`).
- **Encoding details:** content is `name + "\n"`; readers `.strip()` and treat empty as
  unset. Resolution order (`charter/persona.py:1379`-`:1410`): `--persona` →
  `$CHARTER_PERSONA` (stripped; whitespace-only = unset, `:1242`) → session pointer →
  terminal pointer → `.charter/active-persona` → `charter.toml` `[persona] default` →
  `personas/.default` → none. A rung naming a persona that does not exist still wins
  (`charter/persona.py:1438`). Session id = `session.bucket()`; terminal id =
  `session.terminal()` (`$TERM_SESSION_ID`/`$TMUX_PANE`/`$STY`/`$SSH_TTY`), and a `%9` pane
  becomes the filename `-9.persona` (observed in the fixture run).

---

### `.charter/mcp-approved.json`

- **Format:** JSON object, `indent=2`, `ensure_ascii=False`, trailing newline.
- **Status:** stable — written by `sync-agents --approve-mcp` and read by the render on a
  later run; machine-local by design (an approval must not travel in git).
- **Tier:** Clone state — an operator's consent record.
- **Written by:** `charter/mcpseen.py:253` (`approve`, replaces the persona's whole set),
  from `charter/commands_persona.py:2029`.
- **Read by:** `charter/mcpseen.py:247` (`approved`) → `charter/persona.py:779`
  (`mcp_render_entry`) and `mcp_withheld` (`:832`).
- **Git:** gitignored (under `/.charter/`).
- **Encoding details:** `{"<persona>": ["<64-hex sha256>", …]}`, values sorted
  (`charter/mcpseen.py:262`, digest at `charter/mcpseen.py:236`). The fingerprint is a SHA-256 of the **consent line** the
  operator was shown. (This file sits on the boundary with the secrets area; included here
  because `persona.py` is its only consumer.)

---

### `[memory] share` — how it affects persona files

`charter.toml`'s `[memory] share` (`local` | `commit` | `push`, default `local`,
`charter/instance.py:460`, clamped at `charter/instance.py:444`) decides only whether purlis *itself* commits
and pushes the files above — never their format:

- `purlis persona remember` (persistent): commits the memory file **and** its `MEMORY.md`
  (`charter/commands_persona.py:1206`-`:1209` → `charter/planegit.py:98`).
- `purlis persona forget`: commits the memory **directory**
  (`charter/commands_persona.py:1308`-`:1310`).
- `purlis persona optimize --apply`: commits the memory directory
  (`charter/commands_persona.py:1786`-`:1787`).
- `purlis persona dispatch-backfill`: commits every `_dispatch/*.jsonl`
  (`charter/commands_persona.py:1734`-`:1736`).
- The dispatch hook: commits the one tally file it appended, under an flock
  (`charter/hooks.py:8189`-`:8216`).
- `purlis persona memory-sync`: commits every uncommitted path matching
  `personas/[^/]+/(?:memory|refs)/` regardless of posture, after a secret scan
  (`charter/commands_persona.py:1316`, `:1345`-`:1400`).

Under `local` (the default) purlis writes the files and commits nothing; `personas/`
therefore shows up in `git status` for a human.

**In purlis** no command commits a memory on its own. `share` is read only as the
deprecated alias of `[plane].mode`, and the whole plane is saved by one save function, by
hand or by auto-save (ADR 0051).

---

## Vaults: the registry, and nothing inside it

This section and the one after it cover two areas that meet: the vault registry — its shape,
never its contents — and every file purlis writes for a harness to read, inside the plane
and outside it. Both use two conventions:

- **plane root** = `config.ROOT`; **state dir** = `config.STATE_DIR` = `<plane>/.charter/`
  unless `$CHARTER_HOME` is set, in which case it is that path verbatim
  (`charter/config.py:791`, `charter/config.py:99`).
- "**IF ABSENT**" = purlis writes only the key(s) it owns, only when they are not already
  there, and never repairs or reformats a file it cannot parse.

No real vault content was read for this section. Shapes come from the writers in
`charter/secrets/*` plus a throwaway plane generated for the survey, whose only value is the
literal `fixture-not-a-secret`.

### `vaults.json` (plane root — the SHARED half)

- **Format:** JSON object.
- **Status:** **stable** — committed to the plane's git, hand-editable, read by every
  purlis process (doctor, statusline, hooks) and written by `purlis vault add --share`.
- **Tier:** Plane — committed, and it holds no values.
- **Written by:** `charter/secrets/registry.py:215` (`save_shared`) → `_write`
  (`charter/secrets/registry.py:173`), from `add_vault`/`remove_vault`
  (`charter/secrets/registry.py:304`, `charter/secrets/registry.py:347`). Commands:
  `purlis vault add <name> … --share`, `purlis vault remove <name>`.
- **Read by:** `charter/secrets/registry.py:65` (`load_shared`) → `load_registry`
  (`charter/secrets/registry.py:95`); `charter/secrets/registry.py:134`
  (`shared_files_outside_plane`, doctor); `charter/secrets/registry.py:119`
  (`malformed_shared`).
- **Path:** `charter/config.py:774` (`SHARED_VAULTS = root / "vaults.json"`).
- **Git:** committed (nothing in `_GITIGNORE_BASELINE`, `charter/commands.py:1087`,
  ignores it — only `/.charter/` is ignored).
- **Encoding:** `json.dump(doc, f, indent=2, ensure_ascii=False)` + one trailing `"\n"`
  (`charter/secrets/registry.py:197`-`198`). Key order = **insertion order**, never
  sorted. Mode **0644**, set on the descriptor with `fchmod` before the truncate
  (`charter/secrets/registry.py:188`-`193`, `charter/secrets/registry.py:215`). Not atomic
  (no tmp+rename), no locking. Parent created with `config.private_mkdir`
  (`charter/secrets/registry.py:187`). **purlis diverges (#434):** both halves are
  replaced whole through `rewrite::replace` (a temp file beside, fsync, rename), and a half
  that is a symlink is refused rather than written through. This half is a committed file, so
  it keeps the mode it has; a new one gets the umask's (0644 under the usual `022`) rather
  than a forced 0644.

| Field | Type | Required / default | Meaning | Status | Source |
|---|---|---|---|---|---|
| `vaults` | object | defaulted to `{}` on read | name → entry | stable | `charter/secrets/registry.py:60` |
| `vaults.<name>` | object | — | one vault. A non-object entry is dropped from the merged view and reported | stable | `charter/secrets/registry.py:92` |
| `vaults.<name>.provider` | string | required | `keyring` \| `plain-file` \| `reference` \| `1password`. `keyring` is purlis's (ADR 0047), the default `vault add` writes; a Python charter reads it as an unknown provider | stable | `charter/secrets/registry.py:39`, `:245`; `crates/purlis-core/src/secrets/registry.rs` (`PROVIDERS`) |
| `vaults.<name>.persona` | string \| null | written always, `null` when no `--persona` | persona tag | stable | `charter/secrets/registry.py:299` |
| `vaults.<name>.config` | object | written always (may be `{}`) | provider config, merged per key over the local half | stable | `charter/secrets/registry.py:113` |

`config` keys (all providers share the namespace):

| Key | Type | Written when | Meaning | Status | Source |
|---|---|---|---|---|---|
| `file` | string | `plain-file` and `reference` always; other providers only with `--file` | vault file; **relative to the plane root when inside it, else absolute** | stable | `charter/commands_secrets.py:141`, `:147`, `:149`; `charter/commands_secrets.py:34` (`_portable_file`); resolved by `charter/secrets/base.py:40` |
| `op-vault` | string | `--provider 1password` (required) | 1Password vault name | stable | `charter/commands_secrets.py:177`, read `charter/secrets/onepassword.py:134` |
| `op-item` | string | `--op-item` only | item title; default `charter-<vault>` | stable | `charter/commands_secrets.py:179`, read `charter/secrets/onepassword.py:149` |
| `account` | string | `--account` | 1Password account pin — **LOCAL_ONLY, never written to the shared half** | stable | `charter/secrets/registry.py:50`, `:298`, `:317` |
| `env` | object `{TARGET: SOURCE}` | `--env TARGET=SOURCE` / `--token-env X` | env var NAMES only (e.g. `{"OP_SERVICE_ACCOUNT_TOKEN": "OP_ACME_TOKEN"}`); never a value | stable | `charter/commands_secrets.py:188`, `charter/commands_secrets.py:80`; read `charter/secrets/base.py:291` |
| `version` | string | hand-written only | `browser://` resolver's npx package version | stable | `charter/secrets/reference.py:104` |
| `identity` | object | the vault tab's token box / *Move…*, into the **local half only** | The moved-token record: `{"held":"keyring","base":"purlis","bindings":{TARGET:SOURCE},"op_vault":…,"account":…,"op_cmd":…,"op_team":…,"ids":{SOURCE:<id>}}`. Each source is read from the keyring item `<base>/@identity/<id>` (account the source name) first and the environment second. **`base` says which base, and only that one is read:** a move made now writes `"base":"purlis"` and its item under `purlis/@identity/<id>`; a record without the field was made before the rename and is read only from `charter/@identity/<id>` during the rename window (#1261). `rename-local`'s keychain copy switches a record to `purlis` only after every item of it read back the same from `purlis/@identity/<id>`, and `purlis migrate --undo` removes the field again (RN-6); a `base` charter does not know is an error. A read happens **only while the vault's effective `env`/op-vault/account still equal `bindings`/`op_vault`/`account`** and the pinned `op_cmd`/`op_team` verify. **Read from `.charter/vaults.json` alone**; a committed one is ignored, so a commit cannot mark or redirect (#271 review, U2/U5). charter-app only (#237, ADR 0047 as amended); a Python charter ignores the key | stable | `crates/purlis-core/src/secrets/identity.rs` (`MARK`, `BASE`, `READ_BASES`, `record`, `record_matches`, `pinned_op`) |

Legacy spellings `op_vault` / `op_item` are still read (`charter/secrets/onepassword.py:134`,
`:149`) and never written.

### `.charter/vaults.json` (the LOCAL half)

- **Format:** JSON object, same schema as above.
- **Status:** **stable** — hand-editable, and read by other processes (statusline counts it
  directly: `charter/statusline.py:799`; doctor; every `purlis secret` call).
- **Tier:** Clone state — the vault registry V2 names.
- **Written by:** `charter/secrets/registry.py:209` (`save_registry`), from `add_vault`
  without `--share` (`charter/secrets/registry.py:331`), from `--share` when local-only
  keys survive (`charter/secrets/registry.py:321`-`327`), and `remove_vault`
  (`charter/secrets/registry.py:350`). Commands: `purlis vault add`, `purlis vault
  remove`, `purlis persona create --vault` (`charter/commands_persona.py:141`).
- **Read by:** `charter/secrets/registry.py:69` (`load_local`), `:95` (`load_registry`),
  `:228` (`scope_of`); `charter/statusline.py:799`.
- **Path:** `charter/config.py:794`.
- **Git:** gitignored via the `/.charter/` line in `_GITIGNORE_BASELINE`
  (`charter/commands.py:1094`, written by `_ensure_gitignore`, `charter/commands.py:1116`).
- **Encoding:** identical writer, mode **0600** (`charter/secrets/registry.py:209`).
  purlis (#434) replaces it whole, 0600 read back before a byte lands, or refuses the
  write — on a filesystem that cannot hold the mode as well as at a symlink.
- **Merge rule (the app must reproduce it):** shared is the base, local is layered **per
  field**; `config` is dict-updated key by key, other fields overwrite when not `None`
  (`charter/secrets/registry.py:104`-`116`). A `--share` publish REDUCES the local entry to
  its `LOCAL_ONLY_KEYS`, or deletes it (`charter/secrets/registry.py:316`-`327`).
  `scope_of` reports `shared` / `local` / `both` (`charter/secrets/registry.py:222`).

### `.charter/vaults/` (directory)

- **Status:** **stable** — the default home for plain-file and reference vault files, named
  by `config.VAULTS_DIR` (`charter/config.py:799`) and by the guard
  (`charter/hooks.py:553`).
- **Tier:** Clone state
- **Modes:** created 0700, **every level purlis itself creates** chmod'ed to 0700
  (`charter/secrets/base.py:61` → `charter/config.py:190` `private_mkdir`). A directory
  that already exists is left as found and merely *reported*
  (`charter/secrets/base.py:105` `loose_dirs`, rendered by `charter/secrets/base.py:195`).
- Measured on a plane generated for this survey (modes as purlis writes them, which is
  not what a git checkout of a fixture reproduces): `.charter` `drwx------`, `.charter/vaults` `drwx------`,
  every file inside `-rw-------`.
- File names: `<vault name>.json` and `<vault name>.meta.json` by default
  (`charter/commands_secrets.py:141`, `charter/secrets/plain_file.py:165`). Any other path
  is legal via `--file`, absolute included (`charter/secrets/base.py:40`).

Live-plane check (names and modes only, no contents): the real plane has
`.charter/vaults.json` and a `.charter/vaults/` directory of `<name>.json` /
`<name>.meta.json` files, all `0600`, matching the fixture.

### `.charter/vaults/<name>.json` — plain-file vault

- **Format:** flat JSON object, `key → secret value` (values may be multi-line).
- **Status:** **stable** — the operator may hand-author it, and the file is the vault.
- **Tier:** Clone state — at its default path, `.charter/vaults/<name>.json`. A vault added with `--file` can be anywhere, inside the plane or outside it, and it is Clone state only when it is inside `.charter/`. **It holds values**, so FR-10 never copies it, wherever it is. A restore brings back the vault's registry entry, marks the vault **file missing**, and names it to the operator; it never drops the entry silently.
- **Written by:** `charter/secrets/plain_file.py:97` (`_save`) →
  `charter/secrets/plain_file.py:42` (`_write_private`). Commands: `purlis secret set`,
  `purlis secret rm`.
- **Read by:** `charter/secrets/plain_file.py:29` (`_load`) — `get`, `keys`, `health`,
  `ages`; `charter/doctor.py` and the status line through `health()`.
- **Git:** default path gitignored via `/.charter/`. A `--file` path inside the plane that
  git does NOT ignore is refused at registration (`charter/commands_secrets.py:159`-`170`,
  using `git check-ignore`).
- **Encoding:** `json.dump(payload, f, indent=2, ensure_ascii=False)` + trailing newline
  (`charter/secrets/plain_file.py:91`-`92`). Insertion order (i.e. previous file order,
  then newly-set keys appended). Mode 0600 settled on the **descriptor** and read back
  before any byte is written; a mode with any of `0o077` set is refused and nothing is
  written (`charter/secrets/plain_file.py:72`-`87`). Not atomic (in-place truncate), no
  locking. `get` first tightens a loose mode (`charter/secrets/plain_file.py:137`); the
  read-only paths report instead of repairing (`charter/secrets/base.py:214` `mode_note`).
  **In purlis** (#429) the vault is replaced whole, never written in place: the text
  goes to a temp file beside it (`.charter-generated.<name>.<pid>.<tag>.tmp`), created
  `0600` with its mode read back before any byte (the same refusal as above when it will
  not hold), flushed, renamed over the vault, and the directory flushed. A crash mid-write
  leaves the old vault whole. **A vault path that is a symlink is refused** ("it is a
  symlink … Nothing was written") and the file it points at is left alone; Python writes
  through the link. A read-only vault is still refused. The `.meta.json` sidecar, the
  keyring provider's `.keys.json` index and the reference vault below share this writer.

Shape with placeholder values:

```json
{
  "API_TOKEN": "fixture-not-a-secret",
  "KUBECONFIG": "fixture-not-a-secret\nline two\n"
}
```

### `.charter/vaults/<name>.meta.json` — rotation sidecar

- **Format:** JSON object `key → {"set_at": "YYYY-MM-DD"}`.
- **Status:** **stable** — read by `purlis secret audit` in a later process; holds no
  values, only key names and dates.
- **Tier:** Clone state
- **Written by:** `charter/secrets/plain_file.py:148` (`set`) and `:158` (`delete`) via the
  same `_write_private` (`charter/secrets/plain_file.py:177`).
- **Read by:** `charter/secrets/plain_file.py:167` (`_load_meta`), `:182` (`ages`) →
  `charter/commands_secrets.py:496` (`cmd_secret_audit`).
- **Name derivation:** `<vault file stem> + ".meta.json"` in the vault file's own directory
  (`charter/secrets/plain_file.py:165`) — so a `--file` outside the plane puts the sidecar
  outside too.
- **Git / encoding:** as the vault file (0600, indent 2, trailing newline). Date is
  `datetime.date.today().isoformat()` — **local date, no timezone**
  (`charter/secrets/plain_file.py:148`).

```json
{ "API_TOKEN": { "set_at": "2026-09-17" } }
```

### `.charter/vaults/<name>.keys.json` — keyring vault's keys index

purlis only (ADR 0047); the Python charter has no keyring provider and never reads it.

- **Format:** JSON object: `service` (string, or `null` before the vault's first write; a
  vault made before the rename names `charter/<vault>/<id>`, and `rename-local`'s keychain copy
  points it at `purlis/<vault>/<id>` once every item read back the same there, RN-6) and
  `keys`, an object `key → {"size": <size band>, "updated": <RFC 3339 UTC, to the second>}`.
  **Never a value.** `size` is `fingerprint::size_band` of the value (`1–15 bytes`,
  `16–31 bytes`, … `1024+ bytes`), never its length.
- **`held`** (on a key, optional, `true` only): its item was written held to purlis's app, so
  only the app reads it without the person's confirmation (ADR 0047 as amended by ruling V90).
  Absent on every key written before, and on every key in a store with no such rule (Linux, a
  test build's stub): on macOS such a key's item is written again, held, the next time purlis
  reads it (one the `purlis` command made, the next time the command reads it).
- **`held_note`** (optional): `"due"` once purlis moved one of the vault's items under that
  rule and has not said so yet, `"said"` after the vault's next `purlis secret get`, `cp` or
  `exec` said it. It is said once.
- **Status:** **stable** — it is the only record of which keys a keyring vault holds and of
  the service its items live under: the keyring cannot be enumerated through the `keyring`
  crate. Deleting it strands the vault's items in the keyring, still there and unnamed.
- **Tier:** Clone state — the only record of which keys a keyring vault holds; the values are in the Keyring tier.
- **Written by:** `crates/purlis-core/src/secrets/keyring.rs` (`set_with`, `delete_with`, and
  `get_with` and `take_note` for `held` and `held_note`), after the keyring write succeeded, through the plain-file provider's `write_private` (0600,
  settled on the descriptor first). Commands: `purlis secret set`, `purlis secret rm`.
- **Read by:** the same module — `keys`, `listed`, `get`, `ages`, `health`. `secret list`,
  `vault list` and `secret audit` read only this file and never the keyring.
- **`service`:** `purlis/<vault>/<8 lowercase hex>`, made randomly at the vault's first write
  and written to this file BEFORE that first item is, so no item is ever under a service no
  index records. An index written before the rename names `charter/<vault>/<8 lowercase hex>`;
  during the rename window that is still read, and the vault's new keys go under it too, until
  the keychain copy (RN-6) moves the vault (#1261). A service that is not
  `purlis/<vault>/<id>` or `charter/<vault>/<id>` for THIS vault (or holds a control character)
  is refused as corrupt: the file is on disk, and one pointing at another program's item would make
  purlis read it.
- **Git / encoding:** under `.charter/`, so gitignored; indent 2, trailing newline, keys
  sorted.

```json
{
  "service": "charter/ops/3f9a2c1b",
  "keys": {
    "API_TOKEN": { "size": "16–31 bytes", "updated": "2026-09-24T11:32:17Z", "held": true }
  }
}
```

### `.charter/keyring-stub.json` — a test build's keyring

**Tier:** Clone state, transient — test builds only; it stands in for the Keyring tier.

In the state directory (`.charter/`, or `$CHARTER_HOME` when set). Written **only** by a fenced
build (every `cargo test` build, and the app's `e2e` build — `crates/purlis-core/src/fence.rs`),
which keeps a keyring vault's values here instead of in the
operating system's store, so no test can reach the operator's keychain. JSON object
`"<service>\n<account>" → value`, 0600. **It holds values in plaintext**; a build anyone is
given never writes it.

### `.charter/vaults/<name>.json` — reference vault (same path, different content)

- **Format:** flat JSON object, `key → reference URI`.
- **Status:** **stable** — designed to be committed by a team (it holds no values) and
  hand-edited.
- **Tier:** Clone state — Plane when a team commits it from a path outside `.charter/`.
- **Written by:** `charter/secrets/reference.py:191` (`_save`). Commands: `purlis secret
  set <vault> <key> --value 'op://…'`, `purlis secret rm`.
- **Read by:** `charter/secrets/reference.py:179` (`_load`) — `get`, `keys`, `health`,
  `reference_for`.
- **Encoding — differs from the plain-file writer and this matters to a byte-identical
  implementation:** `json.dumps(data, indent=2, sort_keys=True) + "\n"` written with
  `Path.write_text`, then `os.chmod(p, 0o600)` **after** the write
  (`charter/secrets/reference.py:194`-`195`). So: **sorted keys** here, insertion order in
  the plain-file vault; and the mode is applied after the content, not before. **In
  purlis** (#356) the encoding is unchanged but the write goes through the plain-file
  provider's writer: since #429 that replaces the file whole through a temp created
  `0600`, and refuses a vault path that is a symlink (see the plain-file vault above).
- **Git:** default path is under `.charter/` and therefore ignored; a team that commits
  these points `--file` at a tracked path, which is allowed for this provider (the
  unignored-path refusal is `plain-file` only, `charter/commands_secrets.py:159`).

```json
{
  "DEPLOY_TOKEN": "op://Engineering/deploy/token",
  "DB_PASSWORD": "vault://secret/data/app#DB_PASSWORD"
}
```

### Secret reference syntax

Registered schemes: `charter/secrets/reference.py:112`
(`_RESOLVERS = {"op", "vault", "browser"}`). A value that is not a string, or whose scheme
is not in that map, is not a reference (`charter/secrets/reference.py:130` `scheme_of`).

| Syntax | Resolved with | Validation | Source |
|---|---|---|---|
| `op://<vault>/<item>/<field>` | `op read --no-newline <uri>` | netloc present and ≥2 path segments | `charter/secrets/reference.py:49` |
| `vault://<path>#<FIELD>` | `vault kv get -field=<FIELD> <path>` | path and fragment both non-empty | `charter/secrets/reference.py:58` |
| `browser://<session>/<source>/<name>` | `npx …` via `charter/browser.py` | `source` ∈ `browser.SESSION_SOURCES`; key is the rest of the path verbatim | `charter/secrets/reference.py:69` |

A resolved value has exactly one trailing `\n` stripped
(`charter/secrets/reference.py:277`). Resolution is bounded by
`RESOLVE_TIMEOUT = 60.0` (`charter/secrets/reference.py:127`).

**Consumption syntaxes** (CLI surface, not files, but they are what a harness's command
line carries): `--env NAME=<key>`, `--file ENVVAR=<key>` (0600 temp file, prefix
`charter-<vault>-<key>-`), `--dotenv ENVVAR=NAME:key` (0600 temp file, prefix
`charter-<vault>-dotenv-`, entries sharing an ENVVAR merged in flag order)
— `charter/commands_secrets.py:1060`, `:1078`, `:1106`, `:1123`.

### `.charter/fingerprint.key`

- **Format:** raw binary, exactly 32 bytes (`KEY_BYTES`, `charter/secrets/fingerprint.py:63`).
- **Status:** **stable** — key material. Losing it silently changes the `fp:` values this
  plane prints, and a purlis process computing one reads it. `charter/hooks.py:554` names
  the file among the paths a harness tool call is refused; that bounds what an agent does
  through the harness, not what a process on the machine can open. Deleting it is not free:
  it is regenerated on next use, and `fp:` values printed before it stop matching.
- **Tier:** Clone state — key material V2 names. Not derived: losing it changes every `fp:` value.
- **Written by:** `charter/secrets/fingerprint.py:105`-`114` (`_key`), lazily on first use.
  Created only by a command that masks a value — `purlis secret get` (without `--reveal`),
  `purlis secret set` does **not** create it (measured: after `secret set` the file was
  absent; after `secret get` it appeared, 32 bytes, `-rw-------`).
- **Read by:** `charter/secrets/fingerprint.py:91` (`_key`) → `fingerprint()`
  (`charter/secrets/fingerprint.py:122`) → `masked()` (`:166`) →
  `charter/commands_secrets.py:604`.
- **Path:** `config.STATE_DIR / "fingerprint.key"` (`charter/secrets/fingerprint.py:65`,
  `:76`). **Git:** ignored with the rest of `.charter/`.
- **Encoding:** `os.urandom(32)`, no newline; 0600 fchmod'ed and read back before the write,
  refusing on any `0o077` bit; a file of the wrong length is regenerated
  (`charter/secrets/fingerprint.py:94`, `:111`). purlis (#434) replaces the key whole
  (temp beside, created 0600 and read back, fsync, rename) and refuses a key that is a
  symlink; a crash mid-write leaves the old file.
- **Fingerprint format on output:** `fp:` + first 12 hex chars of
  `HMAC-SHA256(key, value.encode("utf-8"))` (`charter/secrets/fingerprint.py:134`). The
  masked line is `<size band> · fp:<12 hex>`, or just the band when no key can be made
  (`charter/secrets/fingerprint.py:169`). Size bands: `empty`, `1–15 bytes`, then
  power-of-two bands `16–31`, `32–63`, … up to `1024+ bytes`, counted in UTF-8 bytes, with
  an **en dash** (`charter/secrets/fingerprint.py:155`-`163`).

### 1Password provider — not a file, but a shape another implementation must match

One item per purlis vault, its concealed fields are the secrets
(`charter/secrets/onepassword.py:552` `_write`):

```json
{"title": "charter-<vault>", "category": "PASSWORD",
 "tags": ["purlis", "purlis:<vault>"],
 "fields": [{"id": "<existing id or key>", "label": "<key>",
             "type": "CONCEALED", "value": "…"}]}
```

Title default `charter-<vault>` (`charter/secrets/onepassword.py:149`), tag literals from
`_TAG = "charter"` / `_CATEGORY = "PASSWORD"` (`charter/secrets/onepassword.py:99`, `:102`),
vault tag `charter:<vault>` (`charter/secrets/onepassword.py:159`). purlis now writes the tags
`purlis` and `purlis:<vault>` on every create and edit (#1261), so an item written before the
rename carries `charter` / `charter:<vault>` until its next write; tags are never used to find an
item, which is found by title, and the title stays `charter-<vault>`. Fields sorted by key
(`charter/secrets/onepassword.py:567`). Reads go through `op read --no-newline
op://<op-vault>/<op-item>/<key>` (`charter/secrets/onepassword.py:223`). Legacy
one-item-per-key titles `charter-<vault>-<key>` are detected and reported, never written
(`charter/secrets/onepassword.py:156`, `:581`).

### Guarded paths

`charter/hooks.py:553` — `_VAULT_PATH_RE` matches `.charter/vaults…`, `.charter/browser`,
`.charter/active-`, `.charter/fingerprint`, case-insensitively and in every separator
spelling; `charter/toolgate.py:276` guards `config.STATE_DIR` and `config.VAULTS_DIR` plus
each registered vault's own `file` (`charter/toolgate.py:301`, `:322`). Observed: a
`cat .charter/vaults/<name>.json` tool call was denied by purlis's own PreToolUse guard,
and a differently-shaped shell command reading the same path in another session was not.
The guard reads the shape of a tool call, which is what it says of itself (`docs/hooks.md`,
"When a guard is wrong") — so this section records the paths it names, not a boundary around
the bytes. A rebuild that reimplements the guard reproduces the matching, and inherits the
same bound.

---

## Files purlis writes that a harness reads

### 2a. Inside the plane

### `<plane>/.claude/settings.json`

- **Format:** JSON object (Claude Code's own schema; purlis owns three key paths).
- **Status:** **stable** — committed, operator-owned, read by Claude Code itself.
- **Tier:** Plane — committed and operator-owned.
- **Written by (each key IF ABSENT, never a repair):**
  - `hooks.PreToolUse[]` ← `_GUARD_HOOK` (`charter/commands.py:1186`), by
    `charter/commands.py:1233` (`_ensure_guard_hook`), from `purlis init` /
    `purlis reinit`. **Skipped entirely when an enabled purlis plugin already dispatches
    `charter hook pretooluse`** (`charter/commands.py:1258` via
    `charter/commands.py:1197` → `doctor._plugin_declaring_guard`, which always asks about
    `~/.claude`, not `$CLAUDE_CONFIG_DIR`).
  - `env.CHARTER_HARNESS = "claude-code"` ← `charter/commands.py:2256`
    (`ensure_env_var`), called from `charter/harness/claude_code.py:335` (`wire`) through
    `_wire_harnesses` (`charter/commands.py:2296`).
  - `permissions.ask[]` / `permissions.allow[]` ← `charter/commands.py:1686`
    (`add_permission_rule`), from `purlis guard ask|allow` and, for the one rule `init`
    writes, `ensure_handoff_gate` (`charter/commands.py:2098`, pattern
    `charter/commands.py:1398`).
  - `enabledPlugins` is **written by the `claude` CLI**, not by purlis (see 2b).
- **Read by:** Claude Code; purlis reads it back at `charter/commands.py:1636`
  (`_load_settings`), `charter/harness/claude_code.py:402`, `charter/commands.py:2139`
  (`_rules_in`), `charter/doctor.py`.
- **Git:** committed — explicitly NOT ignored (`charter/commands.py:1098`-`1099`).
- **Encoding:** parsed **as Claude Code parses it** (`NaN`/`Infinity` rejected) —
  `charter/commands.py:1652` → `doctor._json_as_claude_code_parses`; an unparseable file is
  refused whole, never rewritten. Re-dumped through `_json_style`
  (`charter/commands.py:1320`), which copies the file's existing indent and separators, and
  the trailing newline is preserved only if the file had one
  (`charter/commands.py:1305`-`1306`, `charter/commands.py:2291`). `add_permission_rule`
  always appends `"\n"` (`charter/commands.py:1741`). A fresh file is
  `json.dumps(..., indent=2) + "\n"` (`charter/commands.py:1265`).

| Key path purlis owns | Value | Merge rule | Status | Source |
|---|---|---|---|---|
| `hooks.PreToolUse[]` | `{"matcher": "Bash", "hooks": [{"type": "command", "command": "charter hook pretooluse", "timeout": 10}]}` | append once; skipped if the plugin declares it | stable | `charter/commands.py:1186` |
| `env.CHARTER_HARNESS` | `"claude-code"` | set IF ABSENT; an `env` that is not an object is left alone | stable | `charter/commands.py:2279` |
| `permissions.ask[]` | e.g. `"Bash(purlis handoff *)"` | append IF ABSENT; wrong-typed block ⇒ `malformed`, no write | stable | `charter/commands.py:1727`-`1737` |
| `permissions.allow[]` | e.g. `"Bash(gh pr view *)"` | same | stable | `charter/commands.py:1832` |
| `enabledPlugins."charter@charter"` | `true` | written by `claude plugin install`, mirrored by purlis into workspaces | stable | `charter/harness/claude_code.py:53` |

**Rule syntax** (`charter/commands.py:1367` `_as_rule`): a bare command becomes
`Bash(<pattern>)`; a string already matching `Tool(...)` for
`_RULE_TOOLS` (`charter/commands.py:1345`), a bare tool name, or `mcp__[A-Za-z0-9_-]+`
(`charter/commands.py:1354`) is written verbatim; an `mcp__…` pattern with a wildcard or
arguments raises `UnexpressibleRule` and nothing is written.

**Default ask rules** (charter-app): `init` writes three, `Bash(charter handoff *)`,
`Bash(purlis report *--yes*)` and `Bash(purlis *todo*promote*)`. The second is new in
purlis (ADR 0059, amended 2026-09-26), the third with `purlis ws todo promote` (V42,
ADR 0088 §5): `purlis`, then `todo`, then `promote`, with anything between, so it holds for
`ws` and `workspace` and for a `-w` or `--repo` on either side of the verb. `reinit` adds the
second and third to a plane that predates them. `opencode.json` gets the same globs, each
placed so that no allow or ask that matches the same command comes after it (opencode's last
match wins). A **project template** (FR-17) adds its stack's guard defaults the same way,
through `purlis guard ask`'s writer (every harness with command permissions or none, then
every workspace layer). Codex's command rules live in `CODEX_HOME` or a trusted project's `.codex/rules`, which purlis does not write, so purlis's own guard applies there:
the commands that publish or deploy, such as `Bash(cargo publish *)` for Rust or
`Bash(twine upload *)` for Python. The list is each template's `[guard] ask` in
`crates/purlis-core/templates/<stack>/template.toml`, and a monorepo's is its own and every
stack's.

Measured after `init` + `guard ask 'terraform apply *'`:

```json
{ "env": {"PURLIS_HARNESS": "claude-code"},
  "permissions": {"ask": ["Bash(purlis handoff *)", "Bash(terraform apply *)"]},
  "enabledPlugins": {"charter@charter": true} }
```
(no `hooks` block — the plugin install ran first, which is `init`'s deliberate order,
`charter/commands.py:2822`.)

### `<plane>/.claude/settings.local.json`

- **Tier:** Clone state — gitignored, and co-written by Claude Code.
- **Format / status:** as above, **stable** (Claude Code reads it, at the session directory
  **and at the git root**, measured on 2.1.267 — `charter/harness/claude_code.py:76`-`95`).
- **Written by:** `charter/commands.py:1686` with `local=True`
  (`charter/commands.py:1720`), from `purlis guard ask|allow --local`. Path constant
  `charter/commands.py:1766`.
- **Read by:** Claude Code; purlis via `_load_json_settings`
  (`charter/commands.py:1666`), `charter/harness/claude_code.py:403`.
- **Git:** gitignored — baseline line `/.claude/settings.local.json`
  (`charter/commands.py:1771`), also backfilled at the moment a `--local` rule is written
  (`charter/commands.py:1774`).
- **Encoding:** identical writer. Fixture: `{"permissions": {"allow": ["Bash(gh pr view *)"]}}`.

### `<plane>/opencode.json`

- **Format:** JSON object (opencode's schema).
- **Status:** **stable** — committed, read by opencode at the repository root.
- **Tier:** Plane — committed.
- **Written by:** `charter/harness/opencode.py:1183` (`_apply_rule`) → written at
  `charter/harness/opencode.py:1279`; reached from `purlis guard ask|allow` (never
  `--local`, which opencode answers `unsupported`, `charter/harness/opencode.py:1171`).
- **Read by:** opencode; purlis at `charter/harness/opencode.py:479`
  (`_configured_plugins`, for the foreign-plugin report).
- **Git:** committed (nothing ignores it).
- **Encoding:** `json.dumps(doc, indent=2) + "\n"` (`charter/harness/opencode.py:1266`) —
  **no `_json_style` equivalent**, so an existing file is re-dumped at indent 2.
  Unparseable ⇒ `malformed`, nothing written.
- **Key paths purlis owns:** `permission.<tool>.<glob> = "ask"|"allow"`, or for the five
  `FLAT_ONLY_PERMISSIONS` (`charter/harness/opencode.py:142`) `permission.<tool> =
  "ask"|"allow"` and only when the glob is `*` (anything else is `unsupported`,
  `charter/harness/opencode.py:1250`). Pattern translation
  (`charter/harness/opencode.py:992` `ask_rule`): `mcp__<server>__<tool>` →
  (`<server>_<tool>`, `*`); `Tool(pattern)`/`tool(pattern)` → (`<opencode tool id>`,
  `pattern`) using `TOOL_NAMES` (`charter/harness/opencode.py:47`); anything else →
  (`bash`, pattern).
- **Never weaker than a deny (purlis, FR-17's review).** opencode decides a command by
  the **last** rule in `permission.bash` that matches it ("Rules are evaluated by pattern
  match, with the last matching rule winning", opencode.ai/docs/permissions), so where Python
  appends, `crates/purlis-core/src/scaffold/settings.rs` `ensure_opencode_rule` leaves an exact
  `"deny"` for the glob as it is (answered `denied`, said by `purlis guard ask`, and listed in
  a template's `denied`), and puts a **new** glob before the first `"deny"` entry, so every deny
  that matches the same command still comes after it and still decides, **except** that it
  goes right after the last non-deny entry whose pattern matches the glob's own text (`"cargo
  *": "allow"` for `cargo publish *`), so in an allowlist an allow that would answer the same
  command never comes after it; a deny written after that entry still decides. An entry
  already there with another decision (an `allow` made an `ask`) changes where it stands, as
  Python changes it, unless a later non-deny entry matches it; then it is moved by the same
  rule, so that broader allow cannot outrank it. A `deny` is appended. Claude Code weighs `deny` before `ask` and `allow` whatever the order, so its
  writer only says an exact deny and adds nothing beside it.
- Fixture: `{"permission": {"bash": {"purlis handoff *": "ask", "terraform apply *": "ask"}}}`.

### `<plane>/.gitignore` (the lines purlis owns)

- **Status:** **stable** — committed, read by git and by the operator.
- **Tier:** Plane — committed.
- **Written by:** `charter/commands.py:1116` (`_ensure_gitignore`, `init`) from the baseline
  at `charter/commands.py:1087`; backfilled additively by
  `charter/commands.py:1774` (`--local` rules) and `charter/commands.py:1806`
  (`purlis reinit`, `/charter.local.toml`), both through `util.append_gitignore`
  (`charter/util.py:472`).
- **Lines purlis owns:** `/workspaces/*/*`, `!/workspaces/.gitkeep`, `/.charter/`,
  `/.claude/settings.local.json`, `/charter.local.toml`, `/sessions/` (purlis, ADR 0064),
  plus the Python/OS block in the
  baseline. Presence is tested per line (whole-line, except `.charter/` which is a substring
  test) — `charter/commands.py:1133`-`1141`. Never removed or reordered.
  (The managed live-workspace block `# >>> purlis live workspaces …` is
  `charter/workspace.py:1338` — the workspace area's.)

### Generated harness layer in `workspaces/<ws>/` and in clones

One generic materialiser (`charter/workspace.py:2737` `_materialise`), driven by each
harness's `workspace_files` / `checkout_files` (`charter/workspace.py:2020`
`_harness_files`), refusing any relative path that escapes the base
(`charter/workspace.py:2044`-`2047`).

#### `workspaces/<ws>/.claude/settings.json` (generated)

- **Status:** **stable** — read by Claude Code for a chat whose cwd is the workspace
  directory (project settings do not walk up).
- **Tier:** Clone state, rebuildable — regenerated from the plane at the next launch.
- **Written by:** `charter/harness/claude_code.py:447` (`workspace_files`) →
  `charter/workspace.py:2649` (`wire_harnesses`), called from `workspace.scaffold`/`ensure`
  on every launch, from `purlis workspace reinit`, and from `purlis guard ask` via
  `charter/commands.py:1909` (`_mirror_into_workspaces`).
- **Content:** exactly the plane's own `enabledPlugins` and `env` keys
  (`WORKSPACE_KEYS`, `charter/harness/claude_code.py:53`), plus `permissions` filtered to
  `ask`/`deny` only (`RESTRICTIVE_BUCKETS`, `charter/harness/claude_code.py:67`;
  `allow` never travels). Key order: the two mirrored keys in `WORKSPACE_KEYS` order,
  `permissions` appended last (`charter/harness/claude_code.py:484`).
- **Encoding:** `json.dumps(doc, indent=2) + "\n"` (`charter/harness/claude_code.py:485`);
  written whole through tmp+fsync+`os.replace` (`charter/workspace.py:2874` `_write_whole`).
  Empty plane settings ⇒ **no file at all**.
- **Git:** inside `workspaces/<ws>/`, already ignored by `/workspaces/*/*`.

#### `workspaces/<ws>/<repo>/.claude/settings.json` and `.claude/settings.local.json` (generated, in a clone)

**Tier:** Clone state, rebuildable — purlis's part; Claude Code co-writes the local file.

- Same generator; a clone additionally gets `checkout_files`
  (`charter/harness/claude_code.py:487`): `{"permissions": {<ask/deny from the plane's
  LOCAL file>}}` as `json.dumps(..., indent=2) + "\n"`
  (`charter/harness/claude_code.py:499`). `allow` never travels, which is why the fixture
  clone got no local file.
- `.claude/settings.local.json` is also **co-written by Claude Code itself** ("don't ask
  again"), so it is listed in `cowritten` (`charter/harness/claude_code.py:387`) and is
  never rewritten once the harness has edited it (`charter/workspace.py:3336`).

#### Mirrored plane paths in a clone

**Tier:** Clone state, rebuildable — copies of committed plane content.

`charter/workspace.py:2077` (`_inherited_files`) copies, 1:1 as text, every registered
harness's `inherited_paths`: `.claude/agents`, `.claude/skills`
(`charter/harness/claude_code.py:169`), `.opencode/agent`
(`charter/harness/opencode.py:831`), `.codex/skills`
(`charter/harness/codex.py:236`). **`CLAUDE.md` is deliberately never generated or
mirrored, and `AGENTS.md` is never mirrored.** The one project-instructions file purlis
writes is the next heading's `AGENTS.md`, in a piece only (ADR 0085).

#### `<piece>/AGENTS.md`: a chat's guidance, in its own worktree

**Tier:** Clone state, rebuildable — rendered from persona files when a chat starts.

- **Written by:** `purlis_core::start::layered_or_refusal` → `guest::wire_for_chat`, when a
  chat starts in a piece (`workspaces/<ws>/.worktrees/<repo>/<piece>`). Never in a clone, a
  workspace directory or the project root (ADR 0085).
- **Format:** Markdown with no frontmatter. The first line is `<!-- GENERATED by charter for
  one chat, from its persona and its piece. Edit the persona, not this file. -->`, followed by
  `# purlis's briefing for this chat`, and then the allowed briefing blocks, separated by blank
  lines (`briefing::agents_md`):
  - the persona's identity, without its memory digest. It names `personas/<name>/persona.md`,
    a project-relative path that does not resolve inside the repository;
  - the first line of the piece note, which `agents_md` cuts itself.

  It never carries memory, session records, todos, other workspaces, the skills listing or a
  path on this machine.
- **Ownership:** one entry in the piece's `.charter-generated`, and the line `/AGENTS.md` in
  purlis's block in the common `info/exclude`, written before the file is (V35; git has no
  per-worktree exclude). purlis rewrites the file only while its digest is the recorded one.
- **The line hides every worktree root's `AGENTS.md`, the clone's included.** So `purlis
  doctor` (the `hidden AGENTS.md` row, shown only when there is something to say), a notice on
  the chat's pane at every start from the picker (`start::Ready::notices`), and every chat's
  briefing name each untracked `AGENTS.md` that the line hides and purlis did not write
  (`guest::hidden_agents_md`). A place the check could not look is said too, never passed over.
- **Two chats starting in one piece take turns**: an `flock` on the piece's own git directory
  (`.git/worktrees/<id>/`), which is no file and no store.
- **Steps aside, and never refuses a chat.** It is not written where:
  - the repository tracks an `AGENTS.md`;
  - an `AGENTS.md` purlis did not write is there.

  It is withheld where:
  - the exclude cannot be written at all;
  - its exclude line was left out (charter#1072).

  A merge that brings in a tracked `AGENTS.md` replaces purlis's excluded file (measured on
  git 2.50.1), and the next start writes nothing.

#### `<workspace-or-checkout>/.charter-generated`

- **Format:** JSON object, `relative path → sha256 hex` (a **list** of hexes while a write
  is pending) — `charter/workspace.py:2440` (`_recorded`), `charter/workspace.py:2806`.
- **Status:** **stable** — it is the ownership record two processes (a launch and `doctor`)
  read, and deleting it makes purlis treat every generated file as foreign: nothing is
  overwritten, exclude lines are dropped, `doctor` reports the layer as not purlis's.
- **Tier:** Clone state — not rebuildable, for the reason its workspace entry gives.
- **Written by:** `charter/workspace.py:2933` (`_publish_marker`) →
  `_write_whole` with `json.dumps(marker, indent=2) + "\n"`
  (`charter/workspace.py:2946`). Removed when it would name nothing (`:2948`).
- **Read by:** `charter/workspace.py:2212` (`_read_marker_at`) — the next launch, `doctor`,
  `purlis workspace reinit`, `unwire_guest`.
- **Git:** never committed; in a clone it is hidden via `info/exclude`. A marker git
  **tracks** is distrusted whole (`charter/workspace.py:2176`), as is one holding an
  absolute/`..` key (`charter/workspace.py:2155`).
- **Digest:** `sha256(text)` of the text purlis wrote, hex (`charter/workspace.py:1938`).
- Fixture: `{ ".claude/settings.json": "08dcdf9e…" }`.

#### `<clone>/.git/info/exclude` — purlis's managed block

- **Status:** **stable** — read by git in a repo purlis does not own; the operator sees it.
- **Tier:** Clone state, rebuildable — the managed block is rewritten on every launch and repair.
- **Written by:** `charter/workspace.py:3876` (`_register_excludes`) →
  `_replace_block` (`charter/workspace.py:3404`) → `_write_whole`. Reached from
  `wire_guest` (`charter/workspace.py:4059`), i.e. every launch, `purlis clone`
  (`charter/commands.py:483` `_wire_clones`) and `purlis workspace reinit`.
- **Shape** (`charter/workspace.py:3384` `_exclude_block`): begin marker
  `# >>> purlis (generated layer — `purlis workspace reinit`) >>>`
  (`charter/workspace.py:3114`), the three `_EXCLUDE_NOTE` comment lines
  (`charter/workspace.py:3118`), one `/`-anchored line per generated path in
  `_shared_rels` order, the unanchored temp glob `.charter-generated.*.tmp`
  (`charter/workspace.py:3381`), then `# <<< purlis <<<` (`charter/workspace.py:3115`),
  and a trailing newline. Idempotent: the block is replaced, never appended; an
  unterminated block runs to EOF and is replaced whole. A file with no purlis block and
  nothing to add is handed back byte for byte (`charter/workspace.py:3424`-`3425`).
- The block, verbatim, as purlis writes it:
  `/.claude/settings.json`, `/.charter-generated`, `.charter-generated.*.tmp`.
- **Temp files:** every write in a checkout goes through
  `.charter-generated.<pid>.<hex12>.tmp` beside the target, fsynced, then `os.replace`,
  keeping the replaced file's mode (`charter/workspace.py:2913`-`2924`). **In purlis**
  (#430) the temp is `.charter-generated.<name>.<pid>.<hex12>.tmp`, which the same glob
  hides; the replaced file's mode is kept (it used to come back at the umask's), the
  directory is flushed after the rename, and a generated file that is read-only or is itself
  a symlink is refused and reported blocked.

### 2b. Outside the plane (machine-global)

### `~/.config/opencode/plugin/charter.ts` (`$XDG_CONFIG_HOME` honoured)

- **Since the rename (#1266):** `charter plugin install` writes its guard as
  `plugin/purlis.ts` and removes this file when charter or the Python charter wrote it, so
  opencode never loads two.
- **Format:** TypeScript module, generated from `_SHIM_TEMPLATE`
  (`charter/harness/opencode.py:244`, rendered at `charter/harness/opencode.py:403`).
- **Status:** **stable** — opencode loads it; it is what sets `$CHARTER_HARNESS` and routes
  every tool call to `charter hook <handler>`.
- **Tier:** Machine, device-bound, rebuildable — `purlis plugin install` writes it again.
- **Written by:** `charter/harness/opencode.py:626` (`ensure_shim`, create-only) and
  `charter/harness/opencode.py:588` (`refresh_shim`, which replaces a shim stamped with an
  **older** purlis version), from `charter/harness/opencode.py:956` (`wire`) via
  `_wire_harnesses` (`init`, `reinit`, `purlis harness install`), and from `upgrade`
  (`charter/harness/opencode.py:921`).
- **Read by:** opencode. purlis compares it **byte for byte** to `SHIM_BYTES`
  (`charter/harness/opencode.py:417`, `:441`) and refuses to overwrite anything it cannot
  vouch for (`charter/harness/opencode.py:537` `unvouched`).
- **Git:** outside every repo.
- **Encoding:** UTF-8 bytes written with `write_bytes`
  (`charter/harness/opencode.py:622`, `:638`). First line is the stamp
  `// charter-version: <version>` (`charter/harness/opencode.py:401`). The embedded tables
  are `json.dumps(..., indent=2, sort_keys=True)` for `TOOL_NAMES`, `PRE_HOOKS`,
  `POST_HOOKS` and plain `json.dumps` for `DEFAULT_PRE_HOOK` / `EFFECTFUL_TOOLS`
  (`charter/harness/opencode.py:405`-`409`). **The file's bytes therefore change with every
  purlis version** and with any change to those four tables.
- **Realm rule:** anything else in `plugin/` is foreign and reported
  (`charter/harness/opencode.py:501`), because opencode loads the whole directory into one
  module realm.

### `~/.config/opencode/command/charter.md`

- **Format:** Markdown with YAML frontmatter (`description:`), body embeds
  `` !`echo '{}' | charter statusline` ``.
- **Status:** **stable** — opencode reads it as the `/charter` command.
- **Tier:** Machine, device-bound, rebuildable
- **Written by:** `charter/harness/opencode.py:984` (`wire`), **create-only** (never
  refreshed). Constant: `charter/harness/opencode.py:225` (`COMMAND`), path
  `charter/harness/opencode.py:221`.
- **Git:** outside every repo. **Encoding:** the constant verbatim, trailing newline
  included.

### `~/.config/opencode/charter-context.md`

- **Format:** Markdown; an HTML comment header then `hooks.context_block(None)`.
- **Status:** **stable** — opencode reads it (it is named in `instructions`), and it is the
  substitute for a SessionStart hook.
- **Tier:** Machine, device-bound, rebuildable
- **Written by:** `charter/harness/opencode.py:642` (`write_context`) — **always
  overwritten**, the one generated file purlis repairs; called from `wire`
  (`charter/harness/opencode.py:986`).
- **Encoding:** `"<!-- Generated by charter. Do not edit: rewritten whenever the plane's
  state changes. -->\n\n" + body.strip() + "\n"`, or `_No control-plane context._` when the
  body is empty (`charter/harness/opencode.py:655`-`657`). **Content is plane state** —
  active workspace, todos, personas — so it is nondeterministic by design.

### `~/.config/opencode/opencode.json`

- **Status:** **stable** — opencode's own global config.
- **Tier:** Machine, device-bound, rebuildable — opencode's own file; only purlis's keys are rebuilt.
- **Written by:** `charter/harness/opencode.py:660` (`ensure_instructions`): appends the
  **absolute** path of `charter-context.md` to `instructions`, IF ABSENT
  (`charter/harness/opencode.py:683`). Unparseable or wrong-typed ⇒ `malformed`, nothing
  written.
- **Encoding:** `json.dumps(doc, indent=2) + "\n"` (`charter/harness/opencode.py:685`).
- Fixture: `{"instructions": ["<abs path>/charter-context.md"]}`.

### `~/.codex/config.toml` (`$CODEX_HOME` honoured)

- **Format:** TOML; purlis appends **whole tables or nothing**.
- **Status:** **stable** — Codex reads it; it is machine-wide.
- **Tier:** Machine, device-bound, rebuildable — Codex's own file; only purlis's lines are rebuilt.
- **Written by:** `charter/harness/codex.py:100` (`install`) → `p.write_text(raw + sep +
  _block())` (`charter/harness/codex.py:141`). **Only** from `purlis harness install
  codex` / a Codex profile (`charter/commands_harness.py:169`); `CodexHarness.wire` writes
  nothing on purpose (`charter/harness/codex.py:295`).
- **Read by:** Codex; purlis at `charter/harness/codex.py:126` and
  `charter/wiring.py:506` (`_codex`).
- **Block written** (`charter/harness/codex.py:82` `_block`), appended with a separating
  newline only when the file does not end in one (`charter/harness/codex.py:140`):

```toml

[shell_environment_policy]
set = { PURLIS_HARNESS = "codex" }
```

- **Refusals:** a config already declaring `hooks.<event>` (other than `hooks.state`) ⇒
  `doubled`, nothing written (`charter/harness/codex.py:132`-`136`); an existing
  `shell_environment_policy` ⇒ `present`, nothing written (`charter/harness/codex.py:137`).
- **Keys purlis READS but never writes:** `plugins."charter@charter".enabled`,
  `hooks.state."charter@charter:hooks/hooks.json:<event>:<group>:<hook>".trusted_hash`
  (`charter/wiring.py:634` `_codex_marks`, `charter/wiring.py:92` `CODEX_TRUST_PREFIX`).

### Claude Code's own files — purlis does NOT write them

**Tier:** None — Claude Code's files. The one exception is purlis's `purlis plugin install`
(ADR 0057), which does write `~/.claude/settings.json`: `extraKnownMarketplaces.charter-app` and
`enabledPlugins["charter@charter-app"]`, and it removes its entry from
`plugins/known_marketplaces.json` on uninstall (`crates/purlis-core/src/plugin_install.rs`,
`ClaudeCode`). Those keys are Machine, device-bound and rebuildable, like its opencode and Codex
lines above; the rest of this section is the Python charter's.

`~/.claude/plugins/installed_plugins.json`, `~/.claude/plugins/known_marketplaces.json`,
`~/.claude/settings.json` and `~/.claude.json` are written by the `claude` binary. purlis
shells out — `claude plugin marketplace add purlis/purlis` then `claude plugin install
charter@charter --scope project -y` (`charter/plugincache.py:335`-`336`,
`charter/plugincache.py:112`, `:131`) — from `Harness.provision`
(`charter/harness/claude_code.py:338`), i.e. only `purlis init` and `purlis doctor
--fix`. The module docstring states the rule explicitly: *"Everything here talks to `claude
plugin … --json`, never to Claude Code's files"* (`charter/plugincache.py:27`-`32`).
purlis reads them only as **mtime/size stamps** for its wiring cache
(`charter/wiring.py:775` `_claude_stamps`).

Measured in a throwaway `$HOME` after `purlis init` (written by `claude`, shape recorded here
because a Rust implementation will read the same entries through the CLI):

| File | Entry purlis cares about | Source |
|---|---|---|
| `plugins/installed_plugins.json` | `plugins["charter@charter"][] = {scope, installPath, version, installedAt, lastUpdated, gitCommitSha, projectPath}` | `charter/plugincache.py:196` (`_our_entries`), `:257` (`covers`) |
| `plugins/known_marketplaces.json` | `charter.source = {source: "github", repo: "purlis/purlis"}`, `installLocation` | `charter/plugincache.py:456` (`marketplace_clone`) |
| `~/.claude/settings.json` | `extraKnownMarketplaces.charter` — written by `claude`, never by purlis | `charter/commands.py:1240` (names it as a key purlis must not touch) |
| `~/.claude.json` | `mcpServers`; deliberately NOT stamped, because the probe itself writes this file | `charter/harness/claude_code.py:221`, `charter/wiring.py:822` |

Config-folder resolution a second implementation must copy exactly:
`CLAUDE_CONFIG_DIR ?? ~/.claude`, NFC-normalised, **empty value kept**
(`charter/harness/claude_code.py:192`); but for `.claude.json` it is
`CLAUDE_CONFIG_DIR || ~`, un-normalised, with a legacy `<config home>/.config.json`
winning while it exists (`charter/harness/claude_code.py:221`).

### The shipped plugin's `hooks/hooks.json`

- **Format:** JSON, `{"hooks": {<Event>: [{"matcher": …, "hooks": [{"type": "command",
  "command": "charter hook <handler> --plugin-version X.Y.Z", "timeout": N}]}]}}`.
- **Status:** **stable** — it is the file Claude Code and Codex dispatch from, and Codex
  numbers its trust-ledger keys `<event>:<group>:<hook>` against the **installed copy**
  (`charter/wiring.py:573` `_codex_guard_keys`, `charter/wiring.py:613`
  `_guard_positions`). It lives in the purlis repo (release artifact), not in a plane.
- **Tier:** None — shipped inside the app bundle, not a store.
- **Read by:** the harness; purlis at `charter/wiring.py:601` (the installed copy under
  `<CODEX_HOME>/plugins/cache/charter/charter/<version>/hooks/hooks.json`) and
  `charter/hooks.py:8986` (`--plugin-version` skew check).
- At `50d31dc`: SessionStart 1, UserPromptSubmit 1, PreToolUse 4, PostToolUse 5, Stop 1,
  SubagentStop 1 groups.

### 2c. Git config purlis sets

- **Status:** **stable** — git reads it; it is per-repo `.git/config`.
- **Tier:** Clone state, rebuildable — set again on every wiring.
- **Written by:** `charter/gitpolicy.py:170` (`apply`) → `git config --local …`
  (`charter/gitpolicy.py:190`, `:195`). Callers: `purlis clone`
  (`charter/commands.py:615`), `init --clone-this-repo` (`charter/commands.py:2558`),
  `purlis git-policy --apply` (`charter/commands.py:3301`).
- **Scope:** `--local` only — never `--global`/`--system` (`charter/gitpolicy.py:9`-`14`).
- **Keys, resolved per repo from its own forge** (`charter/gitpolicy.py:111` `forge_for`,
  default GitLab when there is no `origin`):

| Key | Value | Source |
|---|---|---|
| `credential.helper` | `!<cli> auth git-credential` (`gh` or `glab`) | `charter/gitpolicy.py:45`, `charter/forge/github.py:488` |
| `commit.gpgsign` | `false` | `charter/gitpolicy.py:50` |
| `tag.gpgsign` | `false` | `charter/gitpolicy.py:51` |
| `url.https://<host>/.insteadOf` | added once per SSH form: `git@<host>:` and `ssh://git@<host>/` (`--add`, multi-valued) | `charter/gitpolicy.py:195`, `charter/forge/github.py:491` |

- Idempotent: a key is rewritten only when its **last** value differs; an insteadOf form is
  added only when absent (`charter/gitpolicy.py:186`-`196`). An unrecognised forge ⇒ nothing
  applied (`charter/gitpolicy.py:180`).
- **Read, never written:** `core.worktree` out of `<git dir>/config`, parsed by purlis's
  own mini-parser with a 262144-byte cap (`charter/gitconfig.py:71`, `:153`,
  `charter/gitconfig.py:61`); relative values resolve against the **git directory**
  (`charter/gitconfig.py:94`).

### 2d. Charter-private caches in this area

### `.charter/cache/harness-wiring.json`

- **Format:** JSON object, `sha256 key → {state, detail, fix, checked_at, stamp}`.
- **Status:** **internal** — written and read only by `charter/wiring.py` for the profile
  selector; deleting it costs one extra probe per profile (the selector re-probes on a
  miss, `charter/wiring.py:844`). A launch never reads it (`charter/wiring.py:849`).
- **Tier:** Clone state, rebuildable, legacy
- **Written by:** `charter/wiring.py:870` (`remember`) →
  `config.write_for(path, json.dumps(doc, indent=2) + "\n")` (`charter/wiring.py:890`),
  under `config.private_mkdir` (`:889`). Only `wired`/`unwired` are stored.
- **Key:** `sha256(json.dumps({**profiletrust.fingerprint(p), "name", "cwd"},
  sort_keys=True))` (`charter/wiring.py:770`).
- **Stamp:** `{str(path): [st_mtime_ns, st_size] | null}` over each kind's stamped paths
  (`charter/wiring.py:820`; Claude Code `charter/wiring.py:786`, Codex
  `charter/wiring.py:807`, opencode `charter/wiring.py:815`).
- **Freshness:** `0 <= now - checked_at < 86400` (`charter/wiring.py:89`, `:858`).

### `.charter/unrecorded/<sha256[:32]>.json`

- **Format:** `{"errno": "<name or int>", "says": "<strerror>"}` + newline.
- **Status:** **stable** — written by the snapshot path to explain why a checkout's marker
  could not be published, and read by `doctor` in a different process
  (`charter/workspace.py:2984`); removed on the first successful publish. Deleting it costs
  the reason on one `doctor` row, and nothing else.
- **Tier:** Clone state, rebuildable — recomputed on the next failed publish.
- **Written by:** `charter/workspace.py:2977` (`_note_unrecorded`), path
  `charter/workspace.py:2956` (`config.STATE_DIR / "unrecorded" / f"{key}.json"`, key =
  `sha256(realpath(tree))[:32]`).

---

## `.charter/` — runtime state

Everything below lives under the plane's **state directory**. It is `<plane root>/.charter/`
unless `$CHARTER_HOME` is set, in which case it is that path **verbatim**
(`charter/config.py:110`, `charter/config.py:114`). A legacy `.edm/` is renamed to
`.charter/` once, on derivation (`charter/config.py:114`).

Out of scope here (other sections): `vaults.json`, `vaults/`, `fingerprint.key`
(secrets), `harness-profiles-launched.json` (config), `persona-state/`, `reports/`
(personas). They are named where a reader in this area touches them.

### Conventions that apply to every file in this area

**Modes.** Every directory purlis creates under the state dir is `0700`
(`config.private_mkdir`, `charter/config.py:190`); every file purlis writes there is
`0600`, settled with `fchmod` on the descriptor *before* any content
(`STATE_FILE_MODE`, `charter/config.py:389`). A Rust writer must do the same — purlis
does not re-tighten a pre-existing directory but does tighten a pre-existing file.

**Write primitives** (all dispatch on "is this path under the state dir"):

| Call | Semantics | Source |
|---|---|---|
| `config.write_for(p, data)` | whole-file write, truncate after `fchmod`, not atomic | `charter/config.py:504` |
| `config.replace_for(p, data)` | **atomic**: write `<name>.<pid>.<12 hex>.tmp` beside, then `os.replace` | `charter/config.py:595`, temp name `charter/config.py:592` |
| `config.create_for(p, data)` | `O_EXCL` create; returns False if it already existed | `charter/config.py:517` |
| `config.touch_for(p)` | create empty (append mode) + `os.utime` — the mtime is the payload | `charter/config.py:644` |
| `config.private_mkdir` / `claim_private_dir` | 0700 mkdir / `O_EXCL`-style claim of a directory | `charter/config.py:190`, `charter/config.py:283` |

**In purlis** (#430) the whole-file replaces — `replace_for`'s equivalent here (the
profile trust record, the push and save journals, hook state), and the plane's committed files,
`workspace.json`, the generated harness layer and the vaults — go through one writer, `purlis_core::rewrite::replace`. Its temp is
`.charter-generated.<name>.<pid>.<12 hex>.tmp` (the prefix a guest checkout's exclude block
already hides); it is flushed before the rename and the directory after it; a target that is a
symlink is refused rather than replaced; and the mode is one of four: kept (committed files),
kept-or-0600-when-new (the settings tab's files), 0600 (state here), or 0600-or-nothing
(vaults).

**Git.** The whole directory is gitignored: `purlis init` writes `/.charter/` into the
plane's `.gitignore` (`charter/commands.py:1096` in `_GITIGNORE_BASELINE`,
`charter/commands.py:1137` for the additive path). So **nothing in this area is committed**;
"stable" below never means "shared through git", it means "a second process reads it".

**Two id shapes are used as file/directory names**

* *session id* — `session.current()`: `$CHARTER_SESSION_ID` → `$CLAUDE_CODE_SESSION_ID`,
  stripped of every character outside `[A-Za-z0-9._-]` (`charter/session.py:65`,
  `charter/session.py:25`). Inside a frame this is the **chat id** (`ide.2`), which shadows
  Claude Code's own session id (`charter/session.py:46`).
* *terminal id* — `session.terminal()`: `TERM_SESSION_ID` → `TMUX_PANE` → `STY` →
  `SSH_TTY` → `os.ttyname(0)`, with every character outside `[A-Za-z0-9._-]` replaced by
  `-` (`charter/session.py:73`, `charter/session.py:119`). So `%9` → `-9`,
  `/dev/ttys032` → `-dev-ttys032` (confirmed against the live plane).

---

### `sessions/` — per-session markers

**Tier:** Clone state, transient — every file below, unless its entry says otherwise.

*Collected (purlis):* when the app opens a plane, every marker here last written 30 days
or more before is removed, except a `.tools` ceiling and its `.gate` (collected at 7 days, see
their entry) and the markers of a chat the plane's reopen record will bring back
(`retention::on_open`; the rule is under [Every store has a tier](#every-store-has-a-tier)).

### `sessions/<sid>.workspace`
- **Format:** plain text, one workspace name + `\n`
- **Status:** **stable** — written by `purlis ws use` / the reconcile path, read by every
  other purlis process (status line, hooks, frame panels) to answer "which workspace is
  this session on".
- **Tier:** Clone state, transient
- **Written by:** `charter/workspace.py:823` (`set_active`), `charter/workspace.py:851`
  (`reconcile`, seeding from the terminal pointer); commands: `purlis workspace use|ws use`
  (`charter/commands_workspace.py:220`), `purlis workspace reconcile`
  (`charter/commands_workspace.py:1377`), frame launch (`charter/commands_frame.py:5535`)
- **Read by:** `charter/workspace.py:66`/`for_session` (`charter/workspace.py:111`),
  `workspace.source` (`charter/workspace.py:669`), rename sweep
  (`charter/workspace.py:1450`), freshness scan (`charter/workspace.py:4431`)
- **Git:** gitignored
- **Encoding:** value + trailing `\n`; non-atomic `write_for`; read is `.read_text().strip()`,
  empty → "no pointer"; name is re-validated on read (`instance.workspace_name_ok`).
- **Lifetime:** pruned when older than 30 days on any `set_active`
  (`_SESSION_MAX_AGE`, `charter/workspace.py:45`, sweep at `charter/workspace.py:887`), and
  in purlis also when the app opens the plane, unless a chat it will reopen is keyed on
  it (`retention::on_open`, see [Every store has a tier](#every-store-has-a-tier)).

### `sessions/<sid>.lock`
- **Format:** plain text, workspace name + `\n`
- **Status:** **stable** — a second process (any later `purlis ws use`, the frame) reads it
  to refuse an unforced switch.
- **Tier:** Clone state, transient
- **Written by:** `charter/workspace.py:824` (`set_active` — "confirming = locking")
- **Read by:** `charter/workspace.py:766` (`is_locked`), removed by `unlock`
  (`charter/workspace.py:781`)
- **Git:** gitignored
- **Encoding:** as `.workspace`. Inside a frame the *launch* lock outranks this file
  (`launch_lock` → `frame/state.own_workspace`, `charter/workspace.py:744`).

### `sessions/<sid>.tools` — the persona tool **ceiling**
- **Format:** JSON object `{"<persona>": ["<tool>", …]}`, `sort_keys=True`, no trailing newline
- **Status:** **stable** — written by the SessionStart hook, read by the PreToolUse guard in
  a *different* process, and by `purlis persona …`. Security-relevant: it is what a
  mid-session `tools:` edit cannot raise.
- **Tier:** Clone state, transient — a gate file: per session, and never backed up.
- **Written by:** `charter/toolgate.py:813` (`snapshot`), called from
  `charter/hooks.py:7620` (`sessionstart`)
- **Read by:** `charter/toolgate.py:857` (`frozen_tools`), `charter/commands_persona.py:411`
- **Git:** gitignored
- **Encoding:** `json.dumps(data, sort_keys=True)`, written with `replace_for` (atomic).
  Present-but-unparseable ⇒ **approve nothing** (`charter/toolgate.py:859`).
- **Only narrows.** The gate grants what is in the live `tools:`, this ceiling **and** the
  grant approved on this machine (ADR 0035, *Grants*), so a stale or stray ceiling cannot widen
  anything.
- **Collected (purlis):** when the app opens a plane, a `<sid>.tools` and its `<sid>.gate`
  last written more than 7 days earlier are removed, the ceiling first
  (`personagate::sweep_ceilings`). A session that resumes takes a fresh one.

### `sessions/<sid>.gate` — "a ceiling was taken for this session"
- **Format:** empty file; existence is the whole payload
- **Status:** **stable** — a deliberately separate fact from `.tools`; deleting `.tools`
  alone must not re-snapshot (#443, `charter/toolgate.py:752`).
- **Tier:** Clone state, transient — a gate file.
- **Written by:** `charter/toolgate.py:808` (`touch_for`, **before** the ceiling)
- **Read by:** `charter/toolgate.py:877` (`_ceiling_was_taken`)
- **Git:** gitignored
- **Encoding:** zero bytes; write ordering matters (marker first, ceiling second).

### `sessions/<sid>.usage` — token/cache trend ring buffer
- **Format:** plain text, one CSV row per turn: `read,write,hit,ctx`; at most 16 rows
  (`_TREND_KEEP`, `charter/statusline.py:363`); trailing newline
- **Status:** **stable** — written by `charter statusline` (fed Claude Code's per-turn
  payload) and read by the frame's panels in another process
  (`recorded_context_gauge`, `charter/statusline.py:735`); `ctx` exists **only** here.
- **Tier:** Clone state, transient
- **Written by:** `charter/statusline.py:414` (`_record_turn`), via
  `statusline.record_usage` (`charter/statusline.py:643`)
- **Read by:** `charter/statusline.py:459` (`_usage_rows`), `charter/statusline.py:467`,
  `charter/statusline.py:735`; panels through `frame/slots.py`
- **Git:** gitignored
- **Encoding:** `"\n".join(rows) + "\n"`; a row is appended only when `(read, write)`
  differs from the last row (`charter/statusline.py:408`); `ctx` may be the empty field
  (`…,100,`) meaning "no percentage", never `0`. 3-field rows (older purlis) still parse.
- **Keyed on Claude Code's own session id from the payload**, not on `$CHARTER_SESSION_ID`
  (`charter/statusline.py:616`) — so in a frame this file's name differs from the chat id.

### `sessions/<sid>.memnudge`
- **Format:** plain text, a decimal integer, no newline
- **Status:** **internal** — a counter written and read only by `hooks` (PostToolUse). Deleted
  ⇒ the memory-cadence nudge restarts its count at 0; nothing else changes.
- **Tier:** Clone state, transient
- **Written by:** `charter/hooks.py:7689` (`_memnudge_set`), bumped `charter/hooks.py:7698`
- **Read by:** `charter/hooks.py:7680`
- **Git:** gitignored

### `sessions/<sid>.configver`
- **Format:** plain text, a 40-char git sha + `\n`
- **Status:** **internal**, Python only — hooks-only baseline for the "control plane updated"
  nudge. Deleted ⇒ the next UserPromptSubmit re-baselines silently and nudges once less.
  purlis writes none: the window marks a chat's tab when `CLAUDE.md`, `AGENTS.md`,
  `.claude/settings.json`, `.claude/agents/*.md` or a `personas/*/persona.md` changed after it
  started, from what it read of them at the chat's start, and keeps that in memory
  (charter#369).
- **Tier:** Clone state, transient, legacy
- **Written by:** `charter/hooks.py:7875` (`_write_configver`)
- **Read by:** `charter/hooks.py:7891`
- **Git:** gitignored

### `sessions/<sid>.<tool_use_id>.<kind>.ask-pending`
- **Format:** empty file; existence is the payload. `<kind>` ∈ `routing-ask`, `dispatch-ask`
  (`_ASK_KINDS`, `charter/hooks.py:213`)
- **Status:** **internal** — written by one hook event and taken by another in the same
  chat, purely to trace "the operator approved the thing we asked about". Deleted ⇒ the
  approval is not traced; no behaviour changes.
- **Tier:** Clone state, transient, legacy
- **Written by:** `charter/hooks.py:285` (`_ask_mark_set`)
- **Read/removed by:** `charter/hooks.py:314` (`_ask_mark_take`)
- **Git:** gitignored
- **Encoding:** every component is stripped of chars outside `[A-Za-z0-9._-]`
  (`charter/hooks.py:275`). `_ask_mark_take` unlinks only on approval, and a declined ask
  **deliberately** leaves its marker: that asymmetry is what makes "asked N, approved M"
  countable (`charter/workspace.py:875`-`:879`, #290). The leftovers are swept with the rest
  of `sessions/` after 30 days by `workspace._prune` (`charter/workspace.py:881`), which runs
  on `set_active` — so a plane nobody switches workspaces in accumulates them (80 were on the
  plane this was written against).

### `sessions/<sid>.route-pending`
- **Format:** plain text, comma-separated persona names + `\n`
- **Status:** **internal**, Python only — set by one hook event and taken by the next in the
  same process family. Deleted ⇒ the routing suggestion is forgotten. purlis neither
  writes nor reads it: `routing:` is retired (charter#369).
- **Tier:** Clone state, transient, legacy
- **Written by:** `charter/hooks.py:8366` (`_route_mark_set`)
- **Read by:** `charter/hooks.py:8377` (`_route_mark_take`), cleared `charter/hooks.py:8388`
- **Git:** gitignored

### `sessions/<sid>.persona`
- **Format:** plain text, persona name + `\n` (personas area — listed because it lives here)
- **Status:** **stable** — written by `purlis persona use`, read by the guard, the status
  line and the frame.
- **Tier:** Clone state, transient
- **Written by:** `charter/persona.py:1528` (`set_active`)
- **Read by:** `charter/persona.py:1238` (`for_session`), `charter/persona.py:1219`
- **Git:** gitignored

*Reaping:* when a frame chat is reaped, every `sessions/<chat>.*` marker is removed
(`charter/frame/state.py:3377` (`_forget_session`, `charter/frame/state.py:3328`)).

---

### `terminals/` — per-pane pointers

**Tier:** Clone state, transient

### `terminals/<tid>.workspace`, `terminals/<tid>.persona`
- **Format:** plain text, one name + `\n`
- **Status:** **stable** — survives closing/reopening the harness in the same pane and is
  read by every later process in that pane.
- **Tier:** Clone state, transient
- **Written by:** `charter/workspace.py:819` (`set_active`), `charter/persona.py:1528`
- **Read by:** `charter/workspace.py:192` + `charter/workspace.py:674`,
  `charter/persona.py:1196`
- **Git:** gitignored
- **Encoding:** filename is the sanitised terminal id (see conventions). Pruned with
  `sessions/` after 30 days (`charter/workspace.py:882`).

---

### `frame/` — the tmux frame's own state (chats, panels, reopen)

**Tier:** Clone state, legacy — the tmux frame's; purlis neither reads nor writes it, so a backup does not carry it.

`frame/` root: `charter/frame/state.py:229`. Two name shapes appear inside it:
a **chat id** `<workspace-prefix>.<n>` (`charter/frame/state.py:316`, prefix rule
`charter/frame/state.py:104`: every char outside `[A-Za-z0-9_-]` → `_`) and the legacy
**frame id** `<prefix>-<pid>` (`charter/frame/state.py:121`). Both are held to
`[A-Za-z0-9._-]+` (`charter/frame/chats.py:69`).

> **Ruling: `frame/` belongs to the tmux frame, and the app stays out of it.**
> `docs/control-plane.md:872` says these files may change shape in any release and carry no
> format version, and that stands. The app does not draw the tmux frame — it replaces it
> (spec decision 3, milestone M1) — so it keeps its own window state and neither reads nor
> writes anything under `.charter/frame/`. The entries below are recorded because a plane
> the app is working on may have a tmux frame running on it, and because "who owns this
> file" is part of the format. They are marked by what they are to *purlis*: a file two
> purlis processes share is stable to them. If the app ever needs one of these, it is
> promoted to stable here first, and purlis stops calling it scratch at the same time.

### `frame/chat-ids.json`
- **Format:** JSON object `{"<prefix>": <highest ordinal used>}`, `sort_keys=True`, trailing `\n`
- **Status:** **stable** — the high-water mark that stops a reused chat id; read and raised
  by every process that mints a chat.
- **Tier:** Clone state, legacy
- **Written by:** `charter/frame/state.py:383` (`_raise_mark`, `replace_for`, under the lock)
- **Read by:** `charter/frame/state.py:363` (`_read_mark`), `highest_ordinal`
  (`charter/frame/state.py:463`)
- **Git:** gitignored
- **Encoding:** `json.dumps(marks, sort_keys=True) + "\n"`, atomic; the mark is only ever
  raised (`max`), never lowered. Allocation also scans directory names, `reopen.json` and
  `sessions/` markers (`charter/frame/state.py:430`, `:437`, `:449`).

### `frame/chat-ids.lock`
- **Format:** empty file used with `flock(LOCK_EX)`
- **Status:** **stable** — the cross-process mutex for id allocation; the app must take the
  same lock to mint an id.
- **Tier:** Clone state, legacy
- **Written by / locked by:** `charter/frame/state.py:349` (`_locked`, opened `"a"`)
- **Git:** gitignored
- **Deleted?** recreated on demand, but deleting it while another process holds it defeats
  the mutual exclusion.

### `frame/<chat>/` — one directory per chat

**Tier:** Clone state, legacy

Claimed with `claim_private_dir` (`charter/frame/state.py:318`) so a claim is a race-free
`mkdir`. Every file below is one value; unless said otherwise the writer is
`config.replace_for` (atomic) and the reader is `.read_text().strip()`.

| File | Format | Written by | Read by | Status |
|---|---|---|---|---|
| `launcher` | pid + `\n` | `charter/frame/state.py:556` (`_record_claim`) | `charter/frame/state.py:3223` (`_claiming_pid`), `chats.is_chat` | stable — distinguishes a launching chat from a live one |
| `version` | `time.time_ns()` + `\n` | `charter/frame/state.py:630` (`bump`) | `charter/frame/state.py:664`, `charter/frame/panel.py:802` | **stable — the repaint clock.** A hook writes it (`notify.plane_changed`, `charter/frame/notify.py:132`) and every panel process `stat`s/reads it to know it must redraw |
| `notice` | `<expiry epoch float>\n<text>\n` | `charter/frame/state.py:738` (`say`) | `charter/frame/state.py:769`, `:803` | stable — one process writes, the panels render it; TTL 4.0s (`charter/frame/state.py:675`), refusals 10.0s (`:683`) |
| `exit` | int + `\n` | `charter/frame/state.py:818` | `charter/frame/state.py:2460` | stable |
| `harness` | tmux pane id (`%172`) + `\n` | `charter/frame/state.py:919` | `charter/frame/state.py:935`, `chat_in_pane` (`:962`) | stable |
| `harness.pid` | int + `\n` | `charter/frame/state.py:1216` | `charter/frame/state.py:1221` | stable — written from the SessionStart hook (`charter/hooks.py:6090`) |
| `session` | harness session id + `\n`, held to `[A-Za-z0-9][A-Za-z0-9_-]{0,127}` (`charter/frame/state.py:1128`) | `charter/frame/state.py:1020` (`record_harness_session`) | `charter/frame/state.py:1044` | **stable — hook writes, frame reads.** Set by `charter/hooks.py:6102` |
| `session.durable` | same id + `\n` | `charter/frame/state.py:1094` (`_record_kept_session`) | `kept_harness_session`, `charter/frame/state.py:1099` | stable — survives `clear_harness_session` |
| `session.adopted` | empty, `create_for` (O_EXCL) | `charter/frame/state.py:1243` (`adopt_report`) | `charter/frame/state.py:1248` | stable — first-writer-wins adoption |
| `session.start` | `resumed\n` or `fresh\n` | `charter/frame/state.py:1277` | `charter/frame/state.py:1282` | stable |
| `conversation` | absolute path to the harness transcript `.jsonl` + `\n` | `charter/frame/state.py:1175`, from the hook's `transcript_path` (`charter/hooks.py:6104`) | `charter/frame/state.py:1181` | **stable — one process writes it, another reads it**; only ever `stat`ed by purlis |
| `server` | tmux socket name (`charter-plane-<hash>`) + `\n` | `charter/frame/state.py:1317` | `charter/frame/state.py:1335` | stable |
| `workspace` | workspace name + `\n` | `charter/frame/state.py:1384` (`record_workspace`) | `charter/frame/state.py:1514` (`frame_workspace`) → membership (`own_workspace`, `charter/frame/state.py:1619`) | **stable — decides a chat's workspace membership** |
| `profile` | harness-profile name + `\n` | `charter/frame/state.py:1415` | `charter/frame/state.py:1420` | stable |
| `launch` | `<int code>\n<text>` | `charter/frame/state.py:1464` | `charter/frame/state.py:1469` | stable |
| `density` | level word + `\n` | `charter/frame/state.py:1819` | `charter/frame/state.py:1840` | stable (operator-visible layout choice) |
| `bar_rows` | int + `\n` | `charter/frame/state.py:1873` | `charter/frame/state.py:1904` | stable |
| `asserted_bars` | JSON `{"window_rows": int, "panes": [str…] (sorted), "rows": {str: int}}` | `charter/frame/state.py:1953` | `charter/frame/state.py:1997` | stable |
| `chrome` | level word + `\n` | `charter/frame/state.py:2037` | `charter/frame/state.py:2063` | stable |
| `change` | change slug + `\n` | `charter/frame/state.py:2084` | `charter/frame/state.py:2107` | stable |
| `selection` | name + `\n` | `charter/frame/state.py:2145` | `charter/frame/state.py:2166` | stable |
| `hidden` | one name per line | `charter/frame/state.py:2195` | `charter/frame/state.py:2222` | stable |
| `identity` | JSON object of the five frame env vars (`CHARTER_SESSION_ID`, `CHARTER_HARNESS`, `CHARTER_ROOT`, `CHARTER_WORKSPACE`, `CHARTER_PERSONA` — `charter/commands_frame.py:3058`) | `charter/frame/state.py:2354` | `charter/frame/state.py:2375`; pins read at `charter/frame/state.py:1616` | **stable — the chat's environment contract** |
| `panes` | JSON `{ "<panel slot>": "<pane id>" }` | `charter/frame/state.py:2422` | `charter/frame/state.py:2446` | stable |
| `cwd` | absolute path + `\n` | `charter/frame/state.py:2501` | `charter/frame/state.py:2506` | stable |
| `brief` | free text (no trailing-newline normalisation) | `charter/frame/state.py:2566` | `charter/frame/state.py:2571` | stable — the handoff brief the next chat is launched with |
| `brief.owed` | `1\n` | `charter/frame/state.py:2635` | `charter/frame/state.py:2640` | internal marker; deleted ⇒ the chat is not asked for a brief |
| `title` | one line + `\n` (clipped) | `charter/frame/state.py:2719` | `charter/frame/state.py:2727` | stable — the tab label |
| `closed` | `1\n` | `charter/frame/state.py:2784` | `charter/frame/state.py:2789` | stable |
| `waiting` | `1\n` | `charter/frame/state.py:2840` | `charter/frame/state.py:2845` | stable |
| `drawn` | `1\n` | `charter/frame/state.py:2916` | `charter/frame/state.py:2921` | internal (first-draw marker) |
| `ended` | `1\n`, `create_for` (O_EXCL, claim) | `charter/frame/state.py:2948` (`claim_ended`) | `charter/frame/state.py:2953` | **stable** — exactly-once "this chat ended" claim |
| `drawer` | pane id + `\n` | `charter/frame/state.py:3015` | `charter/frame/state.py:3020` | stable |
| `respawn/<slot>` | int + `\n`, in a `respawn/` subdirectory (`charter/frame/state.py:3059`) | `charter/frame/state.py:3133` | `charter/frame/state.py:3124` | internal — attempt counter; `clear_respawn` rmtree's it (`charter/frame/state.py:3165`) |
| `gather.json` | JSON snapshot of the workspace scan (see below); `json.dumps(data)` with **no** indent and **no** trailing newline, written atomically | `charter/frame/gather.py:408` | `charter/frame/gather.py:483`, panels via `gather.read`/`cached` | **stable — the frame's panels read what a detached gather wrote** |
| `tmux.conf` | tmux config text | the placeholder at `charter/commands_frame.py:6621`, then the real config (`conf_text`) at `charter/commands_frame.py:6744` | tmux itself | stable (read by another program) |

`gather.json` fields (`charter/frame/gather.py:105` for the empty shape,
`charter/frame/gather.py:191` for a repo row):

| Field | Type | Meaning | Source |
|---|---|---|---|
| `gathered_at` | float epoch | when the scan ran | `charter/frame/gather.py:106` |
| `workspace` | str | workspace the scan is for | `charter/frame/gather.py:107` |
| `current_repo` | str \| null | repo the cwd is in | `charter/frame/gather.py:108` |
| `repos` | list of objects | `name, branch, dirty, tracked_dirty, ahead, behind, ci, change, sigil, current, worktree_count` | `charter/frame/gather.py:192`–`:202` |
| `worktrees` | list | detail worktrees, same row shape | `charter/frame/gather.py:110` |
| `todos` / `todo_count` | list / int | ≤ 20 rows (`charter/frame/gather.py:82`) | `charter/frame/gather.py:111` |
| `changes` | list of objects | `change, why, state, landed, total, excluded, members[]` | `charter/frame/gather.py:162`–`:173` |

Validity test on read is only `repos` and `worktrees` being lists
(`charter/frame/gather.py:430`); anything else unparseable is treated as "unreadable"
rather than empty (`charter/frame/gather.py:487`).

### `frame/reopen.json`
- **Format:** JSON, `indent=2`, `sort_keys=True`, trailing `\n`
- **Status:** **stable** — written by a quit or by the recorder and read by a *later*
  `purlis reopen`; it is the plane's session manifest.
- **Tier:** Clone state, legacy
- **Written by:** `charter/frame/reopen.py:320` (`write`), called from
  `charter/commands_frame.py:11128`
- **Read by:** `charter/frame/reopen.py:344` (`read`), also scanned for ordinals
  (`charter/frame/state.py:437`)
- **Git:** gitignored
- **No lock** — last writer wins, deliberately (`charter/frame/reopen.py:28`).

| Field | Type | Required / default | Meaning | Status | Source |
|---|---|---|---|---|---|
| `version` | int | `1`, exact match or the manifest is ignored | format version | stable | `charter/frame/reopen.py:63`, check `:347` |
| `at` | int epoch | required (0 if absent) | when it was recorded | stable | `charter/frame/reopen.py:313` |
| `focus` | str | `""` | workspace that had focus | stable | `charter/frame/reopen.py:314` |
| `writer` | str | `"quit"` \| `"recorder"`, default `quit` | who wrote it | stable | `charter/frame/reopen.py:89`, `:368` |
| `frames[]` | list | required | one entry per workspace | stable | `charter/frame/reopen.py:316` |
| `frames[].workspace` | str | required | workspace name | stable | `charter/frame/reopen.py:316` |
| `frames[].chats[]` | list | required, non-empty entries kept | the chats | stable | `charter/frame/reopen.py:317` |
| `chat` | str | required, `ID_RE` | chat id | stable | `charter/frame/reopen.py:110` |
| `workspace` | str | required, valid name | its workspace | stable | `charter/frame/reopen.py:117` |
| `persona` | str | `""` | active persona | stable | `charter/frame/reopen.py:121` |
| `harness` | str | `""` | harness name | stable | `charter/frame/reopen.py:125` |
| `cwd` | str | `""` | directory it ran in | stable | `charter/frame/reopen.py:128` |
| `resume` | str | `""` | harness session id to resume | stable | `charter/frame/reopen.py:131` |
| `transcript` | str | `""` | file name of the captured transcript | stable | `charter/frame/reopen.py:133` |
| `active` | bool | `False` | was the focused chat | stable | `charter/frame/reopen.py:137` |
| `profile` | str | `""` | harness profile | stable | `charter/frame/reopen.py:146` |
| `brief` | str | `""` | its brief | stable | `charter/frame/reopen.py:159` |
| `conversation` | str | `""` | harness transcript path | stable | `charter/frame/reopen.py:163` |
| `ended` | bool | `False` | the harness had ended | stable | `charter/frame/reopen.py:171` |
| `title` | str | `""` | tab title | stable | `charter/frame/reopen.py:183` |

Unknown keys are dropped on read; non-string values become `""`
(`charter/frame/reopen.py:383`).

### `frame/<chat>.transcript`
- **Format:** plain text — the tmux `capture-pane` output of the chat's pane
- **Status:** **stable** — written at quit, read later by a pager and by `purlis reopen`
- **Tier:** Clone state, legacy
- **Written by:** `charter/commands_frame.py:10656` (`_capture_transcript`, via
  `capture-pane -p -e -N -S -2000`, `charter/commands_frame.py:10603`/`:10616`); trimmed
  from the **end in bytes** (512 KB cap)
- **Read by:** `charter/commands_frame.py:12503`, `charter/frame/builtin_actions.py:926`
- **Git:** gitignored; pruned to the chats in the manifest
  (`charter/frame/reopen.py:428`)

### `frame/<frame-id>/` for a non-chat frame (e.g. the live plane's `probe-1`)

**Tier:** Clone state, legacy

Same directory shape, name minted by `frame_id(workspace, pid)`
(`charter/frame/state.py:121`). Only `gather.json` + `version` are typically present.

---

### `app/` — the desktop app's own state

**Tier:** Clone state — every file below, unless its entry says otherwise.

`.charter/app/` is the **purlis** rebuild's, the way `.charter/frame/` is the tmux
frame's. Python charter neither reads nor writes anything under it, and the app stays out
of `frame/` in the same way — the app replaces that frame rather than sharing its state
(ADR 0025, and the note at the top of this file).

It is recorded here because this file records every file purlis's implementations put in a
plane, and because the two share a plane during the migration: an operator running both
will see this directory, and whoever next changes the app should find its format written
down rather than read off the code.

### `app/sandbox.json`
- **What:** this machine's answer to the one-time offer of the sandbox, and its count of the
  new chats in this project that started with the sandbox and without it — the opt-out rate
  SD-2's outcome bar is measured by (V12). **Local only, never sent** (ruling V78 d): `purlis
  doctor`'s `sandbox` row and the Settings tab show it, and nothing else reads it.
- **Format:** JSON, pretty-printed, trailing `\n`, read and replaced whole under purlis's lock
  on the directory (`rewrite::update`). A file that is not this shape reads as empty, and the
  next count starts it again.
- **Keys:** `offer` — `"taken"` or `"kept-off"` once a person answered the offer, absent
  before; `chats` — `{"sandboxed": n, "opted_out": n, "no_backend": n}`, each new chat counted
  once as it starts (a relaunch is not a new chat). The rate is `opted_out` of `sandboxed +
  opted_out`, rounded up; a chat on a system with no backend was never a choice and is not in
  it; `hosts_seen` — the project's `[sandbox].hosts` as this machine last told the person of
  them (#1341), each spelled as the sandbox writes it, absent before the first: the window's
  one-time Notice names what was added and taken away since, and "Got it" records the list it
  showed. A change made in this machine's own Settings is recorded as seen as it is written;
  `hosts_mine` — your own `charter.local.toml` hosts you added or confirmed in Settings on this
  machine (#1341), the only ones of that file that grant anything.
- **Who writes it:** the app, at each new chat's start, at the offer's answer and when the
  hosts Notice is read (`sandbox::local`). A sandboxed chat cannot: `.charter/app/` is the integrity class's.
- **Tier:** Clone state — deleting it asks the offer again, if the project still has the
  sandbox off, and starts the count from nothing (ADR 0069).
- **Git:** gitignored (under `/.charter/`).

### `app/sandbox-blocks.json`
- **What:** the sandbox blocks this machine's app heard from this project's chats in the last
  seven days (#1338), each as when, an operation and the kind of path or host, and whether it
  was purlis's own operation. **No path, argument, host or output**: the chat's hook sorted the
  block and kept none of it. **Local only, never sent**: `purlis doctor`'s `sandbox blocks` row
  counts it, and a Report of a block is drafted from the window's Notice, not from this file.
- **Format:** JSON, pretty-printed, trailing `\n`, read and replaced whole under purlis's lock on
  the directory (`rewrite::update`). A file that is not this shape reads as empty, and the next
  block starts it again.
- **Keys:** `blocks` — a list, oldest first, of `{"at": <seconds since 1970>, "operation":
  "write"|"read"|"connect"|"lookup"|"run"|"file"|"other", "kind": "project-files"|
  "project-state"|"protected-file"|"chat-folder"|"toolchain-cache"|"home"|"temp"|"system"|
  "host"|"local-socket"|"certificate-check"|"system-service", "ours": <bool>}`. A block older
  than seven days is let go of when the next is kept, and at most 1000 are held.
- **Who writes it:** the app, as each block arrives on the hook channel
  (`purlis_core::sandboxblock::record`), at most once a minute for one chat's same block and
  ten a minute per chat (`sandboxblock::Throttle`). A sandboxed chat cannot: `.purlis/app/` is the
  integrity class's.
- **Tier:** Clone state — deleting it empties the doctor's count, and nothing else (ADR 0069).
- **Git:** gitignored (under the state folder).

### `app/reopen.json`
- **Format:** JSON, `indent=2`, trailing `\n`. Replaced whole by `rewrite::replace`: written
  to a temp file beside it (`.charter-generated.reopen.json.<pid>.<tag>.tmp`), flushed and
  renamed over, so a launch never reads half of one. Mode 0600. A `reopen.json` that is a
  symlink is refused (#434).
- **Status:** **internal** — written and read by the app alone. Deleting it costs one
  relaunch's worth of chats: the app starts with none open, which is what a first launch
  does anyway. No second process reads it, which is what would make it stable.
- **Tier:** Clone state — the reopen record V2 names (ADR 0069). Deleting it costs the chats'
  restore. It stays Clone state with `clone` in it: the key says which clone the record
  belongs to, and a copy that carries it is told apart, not trusted.
- **Written by:** `app/src-tauri/src/lib.rs` (purlis) — whenever what is open changes
  (a chat started, closed, brought to front, dragged to another place on its strip, or moved
  onto another conversation by its own harness), and again on the way out. Not only on the
  way out: an app that is killed, or crashes, runs no exit handler. **Restart to update**
  (purlis#251) writes it too, with `relaunch_after_update`, before the restart is asked
  for, so a relaunch that fails loses nothing: the next launch reads the same record.
- **Read by:** `app/src-tauri/src/planes.rs`, at a launch, twice: once for the counts the
  launch's question names (how many chats and view tabs, in which projects), and once when the
  operator has answered it and the record is put back (purlis#250). **Nothing it names
  starts before that answer.** "Start fresh" rewrites it holding no chat and no view tab, and
  keeps `dealt`; Esc, closing the question, or a window that never answers all mean "Reopen
  all", and the file is left as it was until then. Also by `purlis_core::reopen::conversation_of`,
  which answers the conversation one chat is in by its `number` — how a chat asks which
  conversation it is, keyed by the `$CHARTER_SESSION_ID` the app gave it, and
  `purlis_core::reopen::identity_of`, which answers its id and current run the same way. Those
  readers ship in the same build as the writer, so the file stays internal. Each of `id`,
  `device`, `run` and `resumed_from` is held to a ULID on the way in, and anything else reads as
  absent. **A launch also reads other clones' `reopen.json`**: the root its `clone` names and the
  projects the machine store remembers, when the record's clone-key is not this clone's, to tell
  a copied project from a moved one (V43, below). It compares chat ids, through the same guarded
  read, and runs nothing it reads there.
- **Git:** gitignored already, by the plane's own `/.charter/` line.
- **No lock** — one app per plane (Tauri's single-instance plugin), and last writer wins.

| Field | Type | Required / default | Meaning |
|---|---|---|---|
| `version` | int | `1`; any other value and the record is ignored whole | format version |
| `at` | int epoch | required | when it was written; nothing reads it |
| `chats[]` | list | required | one entry per chat that was open, **in the order the chat strip drew them**, which is the order a launch puts them back in (ADR 0039, amended for SI-6: the operator can drag a tab). A record written before that lists them in the order they were numbered, which was the strip's order then, so it reads back as the strip its operator last saw |
| `chats[].program` | str | required | the program, as it was launched: a path or a bare name |
| `chats[].args` | list[str] | default `[]` | its arguments, **without** any purlis added — a resume spells those differently from a start, so they are decided again at the reopen |
| `chats[].cwd` | str | default `""` (absent) | the directory it ran in |
| `chats[].name` | str | default `""` | what the operator calls the chat; a harness that takes a name is given it again |
| `chats[].resume` | str | default `""` | the harness session id the chat is in **now**, and so the one a relaunch resumes (Q10): the id purlis chose at the start (Claude Code), then whatever the chat's own harness moved it onto as its hook reported it — the first id a Codex or opencode chat names, the new one a Claude Code chat is in after `/clear`. Written the moment the app's board adopts or follows the report; a report the board refuses (a harness nested in the chat's shell, ADR 0024 C5) never reaches it. Empty for a chat whose harness has not named a conversation yet, and for a shell. Held to `[A-Za-z0-9][A-Za-z0-9_-]{0,127}` — the Python charter's `SESSION_ID_RE` (`charter/frame/state.py:1128`) — on the way in, and a value that is not one reads as empty. It reaches a command line, and one starting with `-` would be a flag the operator never typed. The chat still comes back, as a new one |
| `chats[].active` | bool | default `false` | whether it was the chat in front |
| `chats[].profile` | str | default `""` (absent) | the harness profile the chat started on, by NAME — never its command or its environment, so an edit to `charter.local.toml` takes effect at the reopen and the account it names never reaches this file (ADR 0022). Held to a name purlis would mint; anything else reads as empty |
| `chats[].persona` | str | default `""` (absent) | the persona the chat adopted, under the same rule |
| `chats[].footer` | str | default `""` | `"show"` where this chat draws purlis's footer in its pane, empty otherwise ([ADR 0029](adr/0029-the-pane-footer-is-blanked-by-default-and-a-chat-may-keep-it.md)). The same word the chat's `$CHARTER_FOOTER` carries, so the record and the launch cannot mean different things by it. **Any other value reads as empty** — a record written before this key existed, and one somebody else wrote, both come back blanked, which is what the app did before the setting existed |
| `chats[].label` | str | default `""` (absent) | the name the operator gave the chat (purlis#254), which its tab says instead of the default `<persona> <N>`. purlis's label only: `name` is still what the harness was started with and is resumed under. Written only when one was given, so a plane that never renamed a chat writes the record it always wrote. Held on the way in to the rule a rename is: trimmed, at most 64 characters, and no control or invisible formatting character (`purlis_core::panel::undrawable`); a value that breaks it reads as absent and the chat comes back under its default |
| `chats[].from` | object | absent | the chat a handoff opened this one from (purlis#258, #259): `{"chat": <n>, "name": "<str>", "workspace": "<str>", "report": "owed" \| "sent"}`. `chat` is the app's number for that chat, the key its reports are left under; `name` is the name it was shown under when it handed off (a copy, so the note still reads once it has closed); `workspace` is where it handed off from, where a report goes once it is gone — a workspace's name, or `plane root` for a chat that handed off from the plane root (SI-1b); `report` is absent for a fire-and-forget handoff, `"owed"` for a `--report` one whose report has not been sent, and `"sent"` after it, for good: a handoff gets one report. Written only for a handed-off chat, so a plane that never handed off writes the record it always wrote. Held on the way in: a `chat` of `0`, a `name` the label rule refuses or a `workspace` that is neither a workspace's name nor `plane root` reads as the whole key absent — the note is not drawn and no report is owed |
| `chats[].renamed_from` | str | default `""` (absent) | the workspace `purlis workspace rename` moved this chat away from, where the rename left it with no conversation its harness can find (charter#367, D10). Claude Code keeps a conversation under the folder it ran in, so the rename clears such a chat's `resume` and writes this instead; a Codex or opencode chat keeps its `resume`. The next start is a new conversation whose pane says why, and the chat is written without this key from then on. Held to the workspace-name rule on the way in; anything else reads as absent |
| `chats[].number` | int | default `0` (not known) | the chat's display number in this plane (purlis#90): what `$CHARTER_CHAT` and `$CHARTER_SESSION_ID` carry, and what `.charter/sessions/<n>.*` and `handbacks/chat-<n>/` are keyed on. `0` is a record written before the field, whose chats are dealt numbers in order at the next launch. Unique in this plane on this machine only, and so never what a committed file or an event names a chat by ([ADR 0066](adr/0066-a-chat-is-a-ulid-a-run-is-a-stretch-of-its-conversation-and-a-device-is-random.md)) |
| `dealt` | int | default `0` | the highest number this plane has dealt, open or closed, so none is dealt twice. Held at or above every `number` the record names, both ways. "Start fresh" keeps it |
| `chats[].id` | str | default `""` (absent) | the chat's id, a ULID minted by the app once per clone and device (V43), when it first starts the chat in that clone, and never changed there ([ADR 0066](adr/0066-a-chat-is-a-ulid-a-run-is-a-stretch-of-its-conversation-and-a-device-is-random.md), #834). What an event names the chat by, and what a session record, the audit and OTel will. Absent in a record written before it, and then minted at the launch that reads it, as `number` was. A chat put back, or started again in the place of one whose harness could not bring its conversation back (`lostOnResume`), keeps it |
| `chats[].device` | str | default `""` (absent) | the chat's origin device: the device id, from the machine store, of the machine that minted `id`. Never a hostname. Absent where that machine had none (ADR 0031), which reads as `unknown` |
| `chats[].sandbox` | str | default `""` (absent) | `"off"` where the chat's last run started without the sandbox in a project that has it on: a person's opt-out, or a system with no sandbox backend (ADR 0067 §7). A fact about the run that was, **never read as an opt-out**: a relaunch or a resume starts the chat sandboxed — the host then writes `trust.sandbox.on` for it — or refuses it where the sandbox cannot be applied (a Codex chat in a sandboxed project, a program the sandbox will not bind). The chat's session record says `sandbox: off` from it. Any other word reads as absent |
| `chats[].run` | str | default `""` (absent) | the ULID of the chat's current run: the stretch of its conversation it is in now. Every start begins one (`start`, `reopen` or `fresh`), and a `/clear` the board follows begins another; ADR 0066's `wake` and `switch` will too once SC-4 and the switches exist. The event log's `run.started` names the same id |
| `chats[].resumed_from` | str | default `""` (absent) | the id of the chat whose session record **Resume** started this one from, taken from that record's `chat-id`. Absent for every other chat, and, until session records carry `chat-id`, for every chat. `from` (above) is to gain an `id` key the same way: the parent's chat id, next to the `chat` number reports are still left under; that key is **decided, not yet written** |
| `chats[].pid` | int | default `0` (absent) | the process the app started this chat's program as, while it runs (V82, #1018): what `commit-msg` and `purlis save` check that a commit runs below before they stamp provenance trailers. Taken from the running session at every write of the record, so one a record was put back with is replaced at the next write. It may be a wrapper profile's process; the harness runs below it. Written only while the chat's program runs: the record is written again when it ends, and the record written at quit names no pid. `0` reads as absent, and a chat with none stamps nothing |
| `clone` | object | absent | the clone and the device that wrote this record (V43, amending [ADR 0066](adr/0066-a-chat-is-a-ulid-a-run-is-a-stretch-of-its-conversation-and-a-device-is-random.md)): `{"key": "<16 hex>", "root": "<path>", "device": "<ULID>"}`. The app stamps it onto every record it writes. Absent in a record written before V43, which adopts the clone it is opened in and keeps its ids. Held on the way in: a `key` that is not 16 lowercase hex characters or a `root` that is not absolute reads as the whole key absent |
| `clone.key` | str | required in `clone` | the clone-key ([ADR 0079](adr/0079-search-runs-on-derived-sqlite-indexes-one-per-project-clone-and-one-per-machine.md) §8): the first 16 hex characters of the SHA-256 of the clone's canonical root path, over the path's bytes. **A launch on another device than `clone.device` reads nothing: the project was copied** (below). **A launch on the same device whose key differs looks for another clone holding these chats**: it reads the `reopen.json` at `root`, then at every project the machine store's recents remember opening (D-V43x), skipping any that is gone or is this very directory (same device and inode, so a firmlink, a bind mount or a hand-edited `root` naming this directory is the same clone). If one holds any of these chats' ids, the project was **copied**, and every chat is given a new `id` and `device` (and no `run`, until it starts) before any starts. Otherwise it was **moved**: the ids are kept and `clone` is rewritten. A copy whose original is unreachable **and** not among the projects this machine remembers reads as a move and keeps its ids, which V43 accepts |
| `clone.root` | str | required in `clone` | the clone's canonical root when it wrote this, because a key cannot be followed back to a directory. Written only as the exact path: a clone whose root is not UTF-8 writes no `clone` at all, and every launch of it reads as a record from before V43 |
| `clone.device` | str | default `""` (absent) | the device id of the machine that wrote this, from the machine store; absent where it had none (ADR 0031). **Another device than this launch's means a copy, whatever the key says**: a same-path copy to another machine has the same key. So new ids are also minted when the project moves to another machine, when the machine store is reset, and when another `CHARTER_CONFIG_HOME` opens it (a dev or isolated build). Unknown on either side, the key decides |
| `relaunch_after_update` | bool | default `false`; written only when `true` | the quit that wrote this restarted purlis to install an update (purlis#251, **Restart to update**, the only writer of `true`), so the launch after it says why it is asking ("Reopen all" is the answer in front either way). Every later write is an ordinary one and drops it. **It counts only at the launch that follows the restart**: the restart also leaves an empty `restarted-to-update` file beside the machine store (`$CHARTER_CONFIG_HOME`, else `$XDG_CONFIG_HOME`, else `~/.config`, then `charter/`), and the next launch removes it whatever it opens. A plane that launch did not open keeps the flag, and it says nothing at any later launch |
| `views[]` | list | default `[]`; absent when no view tab is open | one entry per tab that held a **view** rather than a chat, so a launch puts the tab back (ADR 0043, as amended for view tabs); at most 64 are put back. Putting one back runs nothing: an extension's view waits for a press. A line that fails any rule below is dropped whole, and the rest kept |
| `views[].from` | str | default `""` | the extension whose view it is; empty for purlis's own. A workspace-name word of at most 64 characters |
| `views[].view` | str | required | which view of theirs, under the same rule as `from` |
| `views[].key` | str | default `""` | what the view is about inside that: empty for the whole project, else one workspace-name word (a persona's name, a vault's). **purlis's own views keyed by a path of names** are the exception, each held to exactly one shape and nothing else (#1297): `piece-files` (a branch's file tab, FM-2) `workspace/repo/piece`, the piece empty (`workspace/repo/`) for the repo's own folder; `piece-file` and `piece-diff` that branch, then the file's relative path, no segment empty, `.` or `..`; `todo` `<workspace>/<slug>`; `memory` `workspace/<ws>/<slug>`, `persona/<name>/<slug>` or `shared/<slug>`, the slug `\` for a memory not written yet; `memory-archive` `workspace/<ws>`, `persona/<name>` or `shared`; `session` a session record's project-relative path, `workspaces/<ws>/sessions/<file>` or `sessions/<file>`. Every name passes the rule its writer mints it by, and no key holds a control character or is longer than 4096 characters. No other view's key may hold a `/` |
| `views[].title` | str | default the view's id | what its tab said: one line, at most 120 characters, else the view's own id |
| `views[].workspace` | str | default `""` | the workspace whose strip it was on; empty, or a name that is not a workspace's, files it outside every workspace |
| `views[].at` | int | default `0` | where it was on the strip, counted over chats and views together from the left: a place, not a promise |
| `views[].active` | bool | default `false` | whether it was the tab in front |
| `views[].pinned` | bool | default `false` | whether the operator pinned it (ADR 0039) |
| `views[].split` | number | absent | where the view's divider was, as its first side's share of the tab in percent: the file tab's tree beside its preview (FM-2). Written only once the divider was moved. **Read only between 1 and 99** (rounded to a whole percent); any other number is forgotten and the tab still comes back. The window holds the tree between 10% and 70% |
| `focus` | object | absent | the branch the window's explorer was focused on, its cockpit (FM-5, #1108): `{"workspace": "<name>", "repo": "<name>", "piece": "<name>"}`, with `piece` empty for the repo's own folder. Kept with `views`, and written only while a branch is focused, so a window that never focused one writes the record it always wrote. Held on the way in: a name purlis would not mint, or a value that is not such an object, reads as absent. Not something a launch asks about: a record holding only a focus puts nothing back to ask about |

Which harness a chat runs is **not** recorded: it is read from `program`'s file name, so a
record cannot disagree with what is about to be started. Only a harness purlis has
measured a resume for is resumed (`crates/purlis-core/src/harness.rs`, which carries the
same values as `charter/harness/`). A Codex or opencode chat has no `resume` until its first
turn, because each reports its id only through a hook, inside that turn (ADR 0024, ADR 0058);
from then on it is recorded, and a relaunch runs `codex resume <id>` or `opencode -s <id>`.

---

### `app/hooks.sock`
- **Format:** a unix socket, not a file. Each chat's hooks write one JSON line to it with the
  chat's number and token (`hookwire`), and the app answers. One kind of line, `touching`, names
  the file a chat's file tool touched, for the tree's live marker (FM-6). **It is never stored**:
  never spooled, never in the event log (which keeps only the arguments' digest), never in
  `app/reopen.json` or any other file here (D-86a).
- **Status:** **internal** — bound by the app while a plane is open and gone with it. Nothing
  reads it but the connection it serves. **purlis only.**
- **Tier:** Clone state, transient — it lives as long as the app has the plane open.
- **Written by:** `hooks::socket_for` names it and `hookwire::Listener::bind` binds it
  (`app/src-tauri/src/hooks.rs`, `crates/purlis-core/src/hookwire.rs`). When
  `<plane>/.charter/app/hooks.sock` would be longer than a socket path may be (104 bytes on
  macOS), it is `charter-<user>-<16 hex>/hooks.sock` under `$XDG_RUNTIME_DIR`, else the per-user
  temp directory, instead. The path is handed to each chat as `$CHARTER_HOOK_SOCKET`.
- **Git:** gitignored (under `/.charter/`).

### `app/spool/<n>.jsonl`
- **Format:** JSON lines, one per hook line the host did not take within 250 ms (FD-30, ADR 0068
  §6): `{"v": 1, "seq": <n>, "key": "<16 hex>", "line": "<the hook's line, as JSON text>",
  "mac": "<64 hex>"}`. `seq` counts from 1 per key; `key` names the key the line checks under;
  `mac` is HMAC-SHA256, under that key, of the chat number, `seq` and `key` (each followed by a
  newline) and then `line`'s bytes. A line holds no token. `<n>` is the chat's number.
- **Status:** **internal** — appended by `charter hook` and `purlis git-hook` of the same build,
  read by the app.
- **Tier:** Clone state, transient — emptied by the drain at the next open of the project.
- **Written by:** `purlis_core::hookwire::spool::append`, under an exclusive lock on the file,
  `fsync`ed (and a new file's directory too) before the hook answers its harness. That survives
  a host crash. It is the ordinary `fsync`, so on macOS a power loss can still lose the line.
  Mode 0600, opened with `O_NOFOLLOW`. **Only in a project's `.charter/app/spool/`** (V63): beside
  a fallback socket (`charter-<user>-<16 hex>/`) nothing is spooled, and the hook says the line is
  lost.
- **Read by:** `purlis_core::hookwire::spool::drain`, from `Hooks::drain_spool` in the app,
  when a project is opened and before any chat starts. Each line is checked: its key must be one
  `keys.json` holds for this chat, its MAC must be its own, and its line must name this chat. A
  number missing below the file's own highest one is a gap. Lines removed from the end, or
  removed and then followed by new ones, are not found. A line that is not UTF-8 or not a spool
  line is `unreadable`, and the drain carries on past it. Every line the drain handed on is
  recorded before the file is emptied.
- **Git:** gitignored (under `/.charter/`). A sandboxed chat is denied reading and writing the
  whole `spool/` directory (ADR 0067 §5's integrity class, V63).

### `app/spool/keys.json`
- **Format:** JSON, `{"v": 1, "keys": [{"id": "<16 hex>", "chat": <n>, "key": "<64 hex>"}]}`:
  one entry per chat token the app issued since the last drain. `key` is HMAC-SHA256 of the
  token under the label `charter hook spool v1`, and `id` the first 16 hex characters of its
  SHA-256. The token itself is never written.
- **Status:** **internal** — written and read by the app alone.
- **Tier:** Clone state, transient — what lets a later app check a spooled line. Deleting it,
  or a file that does not read as keys, makes every line spooled since the last drain `no-key`.
  The drain still finishes.
- **Written by:** `purlis_core::hookwire::ChatTokens::issue`, through `spool::remember`, before
  the token reaches the chat (`rewrite::replace`, mode 0600, under an exclusive lock on the
  `spool/` directory). The drain removes the keys it drained. Only in a project's
  `.charter/app/spool/`, and a sandboxed chat neither reads nor writes it (V63): it is a verifier
  at rest.
- **Git:** gitignored (under `/.charter/`).

### Top-level markers, gates and ledgers

### `chat-turns/<chat>`
- **Format:** empty file; **the mtime is the value**
- **Status:** **stable** — written by the hooks (Python) on every tool call and read by the
  frame panel to animate "this chat is working". The canonical hook-writes/frame-reads pair.
- **Tier:** Clone state, transient, legacy
- **Written by:** `charter/inflight.py:507` (`turn_begin`, `touch_for`),
  `charter/inflight.py:527` (`turn_bump`, `os.utime`), removed `charter/inflight.py:543`;
  callers `charter/hooks.py:5971`, `charter/hooks.py:6177`, `charter/hooks.py:8699`
- **Read by:** `charter/inflight.py:587` (`working_chats`),
  `charter/inflight.py:474` (`turn_stamp` — one `stat` of the directory),
  `charter/frame/panel.py:657`, `charter/frame/slots.py:5060`
- **Git:** gitignored
- **Encoding:** the file name is the chat id, refused unless it survives `_safe_name`
  unchanged (`charter/inflight.py:454`). Entries older than 10 min
  (`TURN_STALE_SECONDS`, `charter/inflight.py:415`) are deleted on read.

### `dispatch-inflight/<agent>.<random>.json`
- **Format:** JSON `{"agent": str, "kind": str, "ts": float}`, no newline
- **Status:** **stable** — written by the dispatch hook, read by the status line and the
  frame panels in other processes.
- **Tier:** Clone state, transient
- **Written by:** `charter/inflight.py:246` (`start`, `tempfile.mkstemp` + `json.dump`);
  caller `charter/hooks.py:7954`. Also `kind="clone"` (`charter/commands.py:606`),
  `"gl-refresh"` (`charter/commands.py:1027`), `"action"`
  (`charter/frame/actions.py:370`)
- **Read by:** `charter/inflight.py:180` (`live_records`), `charter/statusline.py:1834`,
  `charter/frame/panel.py:580`, `charter/frame/slots.py:2573`
- **Removed by:** `charter/inflight.py:290`/`:305` (`finish`); anything older than 24 h
  (`PRUNE_SECONDS`, `charter/inflight.py:61`) is unlinked on read
- **Git:** gitignored
- **Encoding caution:** the file is created by `tempfile.mkstemp`, i.e. **0600 but not
  through `config`**; the name is `<safe agent name, ≤64 chars>.<mkstemp tail>.json`
  (`charter/inflight.py:107`, `:244`). `kind` defaults to `"dispatch"` when absent
  (`charter/inflight.py:103`).

### `commit-gate/<sid>`
- **Format:** plain text, a small decimal integer (countdown), no newline
- **Status:** **internal** — a per-session cooldown counter for one nudge, written and read
  only by `hooks`. Deleted ⇒ the next prompt may nudge once more.
- **Tier:** Clone state, transient — a gate file.
- **Written by:** `charter/hooks.py:8322` / `charter/hooks.py:8324` (reset to
  `_COMMIT_COOLDOWN` = 3, `charter/hooks.py:8262`)
- **Read by:** `charter/hooks.py:8320`
- **Git:** gitignored; filename is the sanitised session id (`charter/hooks.py:8319`)

### `dispatch-commit.lock`
- **Format:** empty file, used with `flock(LOCK_EX)`
- **Status:** **stable** — a cross-process mutex around committing the dispatch record; any
  process that commits plane memory must take it.
- **Tier:** Clone state, transient, legacy
- **Written/locked by:** `charter/hooks.py:8209`
- **Git:** gitignored; deleted ⇒ recreated, but concurrent committers stop serialising.

### `guard-seen.json`
- **Format:** JSON object, `sort_keys=True`, trailing `\n`
- **Status:** **stable** — written by the guard handler and read by `purlis doctor` in
  another process; it is the evidence that the guard is actually wired.
- **Tier:** Clone state, rebuildable, legacy — the next guarded call writes it again.
- **Written by:** `charter/guardseen.py:180` (`mark`), called from `charter/hooks.py:6222`
- **Read by:** `charter/guardseen.py:283` (`last`), `charter/doctor.py:2002`,
  `charter/doctor.py:2466`
- **Git:** gitignored (`charter/guardseen.py:38`)

| Field | Type | Required | Meaning | Status | Source |
|---|---|---|---|---|---|
| `ts` | str, ISO-8601 UTC, seconds precision | yes (a record without it reads as absent) | when the guard last fired | stable | `charter/guardseen.py:168`, check `charter/guardseen.py:286` |
| `harness` | str \| null | yes | harness registry name | stable | `charter/guardseen.py:168` |
| `source` | `"plugin"` \| `"settings"` | yes | which declaration dispatched it | stable | `charter/guardseen.py:48`, `charter/guardseen.py:50`, decided at `charter/guardseen.py:55` |
| `claude_config_dir` | str \| null | only for Claude Code | `$CLAUDE_CONFIG_DIR` in use | stable | `charter/guardseen.py:62`, written `charter/guardseen.py:176` |

Only the **latest** sighting is kept (whole-file overwrite).

### `mcp-approved.json`
- **Format:** JSON `{"<persona>": ["<sha256 hex>", …]}`, `indent=2`, `ensure_ascii=False`,
  trailing `\n`; the list is sorted
- **Status:** **stable** — an operator consent record; read by the persona renderer in every
  later process. Machine-local and deliberately not committed (`charter/mcpseen.py:42`).
- **Tier:** Clone state — the operator's consent.
- **Written by:** `charter/mcpseen.py:264` (`approve`), from
  `purlis persona … --approve-mcp` (`charter/commands_persona.py:2029`)
- **Read by:** `charter/mcpseen.py:241` (`_read`) → `approved` (`charter/mcpseen.py:250`),
  `charter/persona.py:779`
- **Git:** gitignored
- **Encoding:** the fingerprint is `sha256(consent line)` (`charter/mcpseen.py:236`); the
  per-persona set is **replaced**, never merged. Not present in the live plane.

### `agent-personas.json`
- **Format:** JSON object `{"<agent id>": "<persona>"}`, `sort_keys=True`, no newline
- **Status:** **stable** — written by one hook event (dispatch) and read by a later one to
  attribute a sub-agent's tools to a persona; it crosses processes.
- **Tier:** Clone state, transient
- **Written by:** `charter/hooks.py:8098` (`_agent_map_remember`)
- **Read by:** `charter/hooks.py:8105` (`_agent_map_lookup`)
- **Git:** gitignored
- **Encoding:** capped at 200 entries, oldest dropped (`charter/hooks.py:8096`); agent ids
  match `\bagentId:\s*([0-9a-f]{6,})` (`charter/hooks.py:8076`).

### `plane-push.json`
- **Format:** JSON object, `indent=2`, no trailing newline
- **Status:** **stable** — written by a **detached** background pusher that has no caller to
  tell, and read by `doctor` later (`charter/planegit.py:213`).
- **Tier:** Clone state — the push journal V2 names.
- **Written by:** `charter/planegit.py:280` (`record_push`); **deleted** when the push
  succeeded (`charter/planegit.py:277`)
- **Read by:** `charter/planegit.py:297` (`push_record`), `charter/planegit.py:340`
- **Git:** gitignored

| Field | Type | Meaning | Status | Source |
|---|---|---|---|---|
| `outcome` | str, one of `pushed`/`branched`/`stranded`/`failed`/`conflict`/`unreachable`, and in purlis `pr-open`/`blocked` | what the push did; a record without it reads as absent. `pr-open`: a request mode pushed `head` to the save branch (`landed`) and its PR into `branch` is open (`url`, `number`). `blocked`: a request mode cannot go further without a person — its PR was closed without merging, the merged target no longer matches what was pushed, or somebody else pushed to the save branch (`detail` says which) | stable | `charter/planegit.py:190`–`:195`, `:300`; `crates/purlis-core/src/planegit/prsave.rs` |
| `branch` | str | branch purlis tried to advance | stable | `charter/planegit.py:281` |
| `landed` | str \| null | the branch it actually reached | stable | `charter/planegit.py:281` |
| `url` | str \| null | PR/MR url | stable | `charter/planegit.py:282` |
| `number` | int \| null | **purlis only.** The PR's number (a GitLab MR's `iid`), written with `pr-open` and `blocked`; what the next save or fetch asks the forge about | stable | `crates/purlis-core/src/planegit.rs` `record_push` |
| `detail` | str | git's own words | stable | `charter/planegit.py:282` |
| `head` | str | the sha being pushed | stable | `charter/planegit.py:282` |
| `at` | float epoch | when | stable | `charter/planegit.py:282` |
| `conflicts` | array of str, optional | **purlis only.** The files a rebase onto the remote conflicted in, read before the rebase was undone; present only when there were some (purlis#295). | stable | `crates/purlis-core/src/planegit.rs` `record_push` |

In purlis the save journal (below) takes over this record's job for saves made by
purlis. `purlis save` keeps writing this record until its contract moves (ADR 0051).

A `pr-open` or `blocked` record is judged against the remote-tracking target branch
(`refs/remotes/origin/<branch>`) rather than `@{upstream}`, and only while HEAD is `head` or
descends from it: a plane moved off that commit by hand is no longer the plane the record is
about. The next save or fetch settles a `pr-open` record by asking the forge where the PR stands
(purlis#298).

### `save-branch.json`
- **Format:** JSON object, `indent=2`
- **Status:** **internal** — what a request mode's saves keep about this clone's save branch, apart
  from `plane-push.json` because that record is rewritten by every push, failures included
  (purlis#298). Deleted ⇒ the next push leases the save branch as absent (and is blocked
  if the remote has one whose tip HEAD does not contain), and a PR opened before is not
  settled by charter: the plane stays on its commits until moved by hand.
- **Tier:** Clone state — part of the save journal.
- **Written by:** `crates/purlis-core/src/planegit/prsave.rs` `Kept::write`, with
  `profiletrust::write_private`: after each push to the save branch, after each PR opened or
  updated, and when a PR is settled (merged and moved onto, or closed).
- **Read by:** the same module, on every request-mode save and fetch; `standing` for the PR's link.
- **Git:** gitignored (under `/.charter/`).
- **Fields:**
  - `branch`: the save branch the rest is about; anything kept about another is ignored
  - `pushed`: the commit this clone last pushed to it — what the next push is leased against
  - `pr`: `null`, or the PR last opened or updated and not yet settled: `number`, `url`,
    `head` (the commit it was last pushed at) and `target` (the branch it goes into)

### `save-journal.jsonl`
- **Format:** JSON Lines, one object per save attempt, appended. Capped at the newest 500
  lines.
- **Status:** **internal**. Only the app and `purlis save` write it, and only the Saving view
  reads it. Deleted ⇒ the Saving view's history starts empty.
- **Tier:** Clone state — the save journal V2 names.
- **Written by:** `crates/purlis-core/src/planegit.rs` `save_as`, the one save function
  (purlis#293, ADR 0051), and `crates/purlis-core/src/reposave.rs` `save_as` for a
  workspace repo (purlis#299), once per attempt, with `profiletrust::write_private`.
- **Read by:** the Saving view (its last 50 entries).
- **Git:** gitignored (under `/.charter/`).
- **Fields:**
  - `at`: epoch seconds
  - `target`: `plane`, or `repo:<workspace>/<name>`
  - `trigger`: one of `manual`, `quiet`, `session-end`, `quit`, `launch`, `cli`, `live`, `rename`
  - `mode`
  - `files`: a count
  - `commit`: a sha or null. For a repo, HEAD once the save was done with it — the commit it
    made or pushed
  - `branch`: for a repo, the remote branch the save pushed (the branch the clone is on, or
    `charter/<workspace>/<short-sha>` in a request mode on the default branch); absent or null
    otherwise
  - `pr`: a url or null
  - `ms`: the duration
  - `outcome`: one of `saved`, `committed`, `pushed`, `pr-open`, `blocked`, `offline`,
    `skipped`, `failed`
  - `detail`: git's or the forge's own words

### `ws-edit-nudge/<sid>-<workspace>`
- **Format:** one byte, `1`
- **Status:** **internal** — "this session was already nudged about this workspace". Deleted
  ⇒ one extra nudge. Written and read only by `hooks`.
- **Tier:** Clone state, transient
- **Written by:** `charter/hooks.py:7656`; existence checked `charter/hooks.py:7654`
- **Git:** gitignored
- **Encoding:** key is `f"{session}-{ws}"` with every char outside `[A-Za-z0-9._-]`
  removed (`charter/hooks.py:7652`) — **ambiguous by construction** (see the Appendix).
  Only written for a **live** workspace (`charter/hooks.py:7810`).

### `ws-autosave/<workspace>`
- **Format:** plain text, `str(time.time())`, no newline; the mtime is what is actually read
- **Status:** **internal** — a 90-second debounce for the Stop-hook auto-save. Deleted ⇒ at
  most one extra commit attempt.
- **Tier:** Clone state, transient, legacy
- **Written by:** `charter/commands_workspace.py:1179` (marker path
  `charter/commands_workspace.py:1171`)
- **Read by:** `charter/commands_workspace.py:1173` (mtime only)
- **Git:** gitignored; only ever written when `[memory] share` is not `local`

### `workspace-tab-order`
- **Format:** plain text, one workspace name per line, trailing newline per line
- **Status:** **stable** — the operator's tab order, written by the frame and read by every
  later frame/palette process.
- **Tier:** Clone state, legacy — the operator's order.
- **Written by:** `charter/workspace.py:1023` (`record_tab_order`, `replace_for` — atomic)
- **Read by:** `charter/workspace.py:1060` (`tab_order`); removed by
  `charter/workspace.py:1079`
- **Git:** gitignored
- **Encoding:** `"".join(f"{n}\n" for n in names)`; names failing `valid_name` are dropped
  on read (`charter/workspace.py:1063`).

### `workspace-arrivals/<workspace>`
- **Format:** empty file; existence is the payload
- **Status:** **stable** — one process records the arrival, another (the frame/status line)
  reads and clears it.
- **Tier:** Clone state, transient, legacy
- **Written by:** `charter/workspace.py:1163` (`record_arrival`, `touch_for`)
- **Read by:** `charter/workspace.py:1198` (`arrivals`), cleared `charter/workspace.py:1224`,
  all forgotten `charter/workspace.py:1238`
- **Git:** gitignored; the name must satisfy `valid_name` (`charter/workspace.py:1127`)

### `unrecorded/<sha256(realpath(tree))[:32]>.json`
- **Format:** JSON `{"errno": "<errno name or number>", "says": "<strerror>"}` + `\n`
- **Status:** **stable** — written by the snapshot path and read by a later command/status
  render to explain why a tree could not be recorded.
- **Tier:** Clone state, rebuildable
- **Written by:** `charter/workspace.py:2977` (`_note_unrecorded`, atomic); removed when the
  write succeeds (`charter/workspace.py:2974`)
- **Read by:** `charter/workspace.py:2988` (`unrecorded_reason`)
- **Git:** gitignored
- **Encoding:** filename derivation `hashlib.sha256(os.path.realpath(tree))[:32]`
  (`charter/workspace.py:2959`) — note it uses `mkdir_for`, not `private_mkdir`.

### `locks/harness-wiring-<digest16>.lock`
- **Format:** empty file, `flock(LOCK_EX)`
- **Status:** **stable** — serialises harness wiring across processes.
- **Tier:** Clone state, transient, legacy
- **Written/locked by:** `charter/wiring.py:963` (path `charter/wiring.py:959`)
- **Git:** gitignored
- **Encoding:** digest = first 16 hex of `sha256(json.dumps({**profiletrust.fingerprint(p),
  "name": p.name}, sort_keys=True))` (`charter/wiring.py:957`)

### `active-workspace` (legacy)
- **Format:** plain text
- **Status:** **internal/dead** — derived (`charter/config.py:804`) and read by nothing in
  this tree; kept so old files do not error. Deleting it changes nothing.
- **Tier:** Clone state, legacy

### `active-persona`
- **Format:** plain text, persona name + `\n`
- **Status:** **stable** — the plane-wide persona pointer used only when there is neither a
  session nor a terminal id (`charter/persona.py:1529`); read by `charter/persona.py:1222`.
- **Tier:** Clone state — the plane-wide persona pointer the operator chose.
- **Git:** gitignored

---

### `cache/` — derived data with a TTL

**Tier:** Clone state, rebuildable — every file below, unless its entry says otherwise.

All five are regenerated on demand; deleting any of them costs one slower render or one
extra network/git call. They are **internal** by the rule, but every one of them is written
by one process and read by another (the status line, the frame, a background refresher), so
a Rust reader that only *reads* them is safe while a Rust writer must keep the key and TTL
semantics below.

### `cache/repostate.json`
- **Format:** JSON object keyed by absolute repo path → `{"dirty": bool, "tracked_dirty":
  bool, "ahead": int, "behind": int, "ts": float}`; one line, no trailing newline
- **Status:** **internal** — a 5-second TTL cache of `git status --porcelain=v1 --branch`
  (`_STATE_TTL`, `charter/statusline.py:817`). Deleted ⇒ the next render shells out to git.
- **Tier:** Clone state, rebuildable, legacy
- **Written by:** `charter/statusline.py:844` (`_repo_states`)
- **Read by:** `charter/statusline.py:825`; the whole file is rewritten on any change
- **Git:** gitignored. Values: `charter/statusline.py:902`

### `cache/glstate.json`
- **Format:** JSON object keyed by absolute repo path → `{"branch": str, "ts": float,
  "change": int|null, "ci": str|null, "sigil": str}`
- **Status:** **internal** — forge (CI / open change) cache. Served for up to 2 h
  (`DISPLAY_TTL`, `charter/glstate.py:21`), refreshed after 5 min (`REFRESH_TTL`,
  `charter/glstate.py:22`). Deleted ⇒ the CI column is blank until a refresher runs.
- **Tier:** Clone state, rebuildable
- **Written by:** `charter/glstate.py:150` (`_save`), from the detached
  `purlis gl-refresh` (`charter/glstate.py:288`)
- **Read by:** `charter/glstate.py:141` (`load`), `charter/glstate.py:171` (`read_for`),
  and `gather.scan`
- **Git:** gitignored; an entry whose `branch` no longer matches is ignored
  (`charter/glstate.py:176`).

### `cache/glstate.refreshing`
- **Format:** plain text, the refresher's pid (or empty when done)
- **Status:** **internal** — spawn lock; **mtime carries the age**. Deleted ⇒ at worst a
  duplicate refresher.
- **Tier:** Clone state, transient
- **Written by:** `charter/glstate.py:88` (`_write_lock`), cleared `charter/glstate.py:136`
- **Read by:** `charter/glstate.py:106` (`_read_lock`), liveness via `os.kill(pid, 0)`
  (`charter/glstate.py:119`)

### `cache/update.json` and `cache/update.checking`
- **Format:** `update.json` — JSON `{"latest": str, "ts": float, "head": str}` (one line);
  `update.checking` — empty file whose **mtime** is the spawn cooldown
- **Status:** **internal** — deleted ⇒ one more version check. `latest` is the newest
  released version, `head` the upstream dev sha (dev channel only).
- **Tier:** Clone state, rebuildable, legacy
- **Written by:** `charter/update.py:467` (`fetch_and_store`), lock touched
  `charter/update.py:506`
- **Read by:** `charter/update.py:119` (`load`), `charter/update.py:331` (`newer_head`),
  `charter/update.py:351` (`newer_than`), `charter/update.py:500`
- **Git:** gitignored; refresh at most daily (`REFRESH_TTL`, `charter/update.py:53`)

### `cache/update-baseline`
- **Format:** plain text, a version string, no newline
- **Status:** **stable** — it is what the *news* range is computed from across upgrades, i.e.
  a later, different purlis reads what an earlier one wrote. Deleted ⇒ the news range
  degrades, never the update (`charter/commands_update.py:131`). **The app neither writes nor
  reads it:** its `purlis news` prints the app's own CHANGELOG.md and has no range view
  (purlis #352), so a baseline the Python charter left is history.
- **Tier:** Clone state, legacy — what the Python charter's news range started from; the Rust purlis neither writes nor reads it (`adopt.rs`).
- **Written by:** `charter/commands_update.py:129` (`_stamp_baseline`)
- **Read by:** `charter/commands_update.py:120` (`read_baseline`)
- **Git:** gitignored

### `cache/vaulthealth.json`
- **Format:** JSON object keyed by vault name → `{"ok": bool, "detail": str, "ts": float}`
- **Status:** **internal** — 60-second TTL (`_VAULT_TTL`, `charter/statusline.py:1650`).
  Deleted ⇒ the next render asks the provider again.
- **Tier:** Clone state, rebuildable, legacy
- **Written by:** `charter/statusline.py:1691`
- **Read by:** `charter/statusline.py:1677`
- **Git:** gitignored. **`detail` is provider text and can name paths/accounts** — it is the
  one cache entry a UI must treat as possibly sensitive.

### `cache/harness-wiring.json`
- **Format:** JSON object keyed by a sha256 hex → `{"state": "wired"|"unwired", "detail":
  str, "fix": str, "checked_at": float, "stamp": {"<abs path>": [mtime_ns, size] | null}}`,
  `indent=2`, trailing `\n`
- **Status:** **internal** — remembered wiring verdict; 24 h TTL (`MAX_AGE`,
  `charter/wiring.py:89`) **and** invalidated when any stamped file changed
  (`charter/wiring.py:861`). Deleted ⇒ the selector re-checks (slower, correct).
- **Tier:** Clone state, rebuildable, legacy
- **Written by:** `charter/wiring.py:890` (`remember`)
- **Read by:** `charter/wiring.py:838` (`_read_cache`), `charter/wiring.py:853` (`cached`)
- **Git:** gitignored; key = `sha256(json.dumps({**profiletrust.fingerprint(p), "name":
  p.name, "cwd": str(cwd)}, sort_keys=True))` (`charter/wiring.py:770`)

### `index/<clone-key>/` — the project's search index

- **Format:** **decided, not yet written** ([ADR 0079](adr/0079-search-runs-on-derived-sqlite-indexes-one-per-project-clone-and-one-per-machine.md)).
  `<clone-key>` is the first 16 hex characters of the SHA-256 of the clone's canonical root path, so
  two clones sharing a state directory through `$CHARTER_HOME` never share an index. Inside it:
  `current`, a plain-text file naming the live generation, and one directory per generation
  holding `search.sqlite` (SQLite in WAL mode, with its `-wal` and `-shm`): an item table (path,
  kind, owner, audience, approval state, workspace, persona, content hash, size, modification
  time), an FTS5 contentless table (`detail=full`: each term with its row, column and position,
  enough to reconstruct much of the wording, but no text as written), and a `meta` table (schema
  version, redaction rules version, the commit last reconciled, the time built). A span the
  shape scanner flags is indexed as its kind. A deleted item's rows stop answering at once, and
  its terms leave the file at the next FTS5 `optimize` plus `wal_checkpoint(TRUNCATE)` with
  `secure_delete` on; an unlinked old generation is not overwritten. A chat is answered only
  from its own `<clone-key>`
- **Status:** **internal**. Derived from the project's memory bases, session records and todos,
  read through `memstore::read_files`' gate. Deleted ⇒ `recall` and the briefing scan the files
  until `purlisd` has rebuilt it. A version it did not write is discarded and rebuilt, never
  migrated
- **Tier:** Clone state, rebuildable
- **Written by:** `purlisd`, its only writer (KN-2). A full rebuild writes a new generation and
  swaps `current` in one rename
- **Read by:** the app's panels, the hooks, the CLI and the palette, read-only
- **Git:** gitignored, with the rest of `.charter/`

### `index/<clone-key>/writer.lock`

- **Format:** an empty file, held with an advisory `flock` by the index's writer. **Decided, not
  yet written** (ADR 0079)
- **Status:** **internal**. A second writer waits on it rather than writing alongside
- **Tier:** Clone state, transient
- **Written by:** `purlisd` (KN-2)
- **Git:** gitignored

---

### State charter keeps **outside** the plane

* `$CHARTER_CONFIG_HOME` → `$XDG_CONFIG_HOME` → `~/.config`, then `charter/reporting-consent`
  — a plain text file whose **existence** is the Reporter's consent to open upstream issues
  (`charter/report.py:462`, `charter/report.py:465`, granted `charter/report.py:475`).
  **stable** (operator-visible, deleted by hand to withdraw), per *machine*, not per plane:
  one consent covers every plane the machine works on.
* **The kill switch** (OV-1, ADR 0071), written by the app **and** by `purlis stop --all`:
  `charter/halted`, an **empty** 0600 file whose **existence** means every agent purlis
  started on this machine is stopped; and `charter/kill-switch.jsonl`, JSON Lines, 0600, one
  object per event — `at` (epoch seconds), `event` (`stop`, `rearm` or `tamper`), `by`
  (`window`, `cli` or `app`; on a runner also `link`, **decided, not yet written**, ADR 0078) — capped at the newest 1000 lines. Written by
  `crates/purlis-core/src/halt.rs`. The machine is stopped when the marker is there **or**
  the journal's last event is not a `rearm`, so removing the marker alone re-arms nothing; a
  running app puts a removed marker back and journals a `tamper`. Only the app's window writes
  a `rearm`. While stopped, the app starts no chat and a relaunch puts no chat back. **stable**
  (a second process reads both). A process running as the operator can still edit both files
  (ADR 0071's residual).

* No other **state** in this area is written outside the plane, though purlis does write
  elsewhere: a news probe writes `$TMPDIR/charter-probe-<pid>` (`charter/news.py:1329`).
  Harness-side files
  (`~/.claude/...`, codex/opencode homes) belong to the config/harness area; this area only
  *stamps* them in `cache/harness-wiring.json`.

**What purlis keeps outside the plane** (ADR 0034, ADR 0069). `<config>` is the machine
store's directory: `$PURLIS_CONFIG_HOME`, else `$XDG_CONFIG_HOME`, else `~/.config`, then
`purlis/` once rename-local has moved it, else `charter/` (the `CHARTER_` spellings of both
variables are read too, until 1.0), `0700`, and every file in it `0600` (`crates/purlis-core/src/machine.rs`). On a
platform that is not unix none of it is written (ADR 0031). **Rename window (RN-5):** the
folder is `purlis/` once `rename-local` has moved it (or it is the one there), else `charter/`;
the same rule holds for the session host's `purlisd/`/`charterd/` in it, for `<data>`'s folder and
for the app's log folder (`dev.purlis.app`/`dev.charter.app`). `rename-local` journals every move
in `<config>/rename-local/journal.jsonl` and lists each project whose state folder it moved in
`<config>/rename-local/state-moved`, one resolved root a line; `purlis migrate --undo` replays the
journal backwards. The keychain copy (RN-6) journals each item it writes under `purlis/…`
(`copied`), each keyring vault whose index it points from its `charter/…` service to the purlis
one (`switched`, with each key's `updated` then) and each identity record it gives
`"base": "purlis"` (`rebased`), each only after every item read back the same; the undo points
them back and keeps every item under both names. The copy never lets the Keychain ask (#1306): a
vault or record whose items would ask stays on `charter/…` and waits, held only in the app's
memory, until the window's *Finish moving* copies it with the Keychain's dialogs on; a terminal
copies only a vault whose index marks no key `held`. `.purlis.lock`, beside `<config>` in the config root, is an empty advisory lock:
the app and `mcp` hold it shared while they run, and `rename-local` takes it exclusively, so it
never moves the folders under them. The Tauri directories are the app's, identifier `dev.purlis.app` (`dev.charter.app` before RN-9). The keyring rows are the operating system's store (ADR 0047).
`<data>` is purlis's data home (ADR 0075, amending ADR 0069): `$PURLIS_DATA_HOME`, else
`$XDG_DATA_HOME/charter`, else the OS data directory's `charter/` (`~/Library/Application
Support/charter` on macOS, `~/.local/share/charter` on Linux); in each, the folder is `purlis/`
once rename-local has moved it, else `charter/`. The host's event log is written
there (FD-9, `purlis_core::datahome`). The other rows are **decided, not yet written**: AU-3
writes the audit's, RR-16 a runner's bare repos (ADR 0078), KN-32 the search index's
(ADR 0079), RC-7 the reviews (ADR 0084) and OB-2 the telemetry store (ADR 0083). Every writer
refuses a `<data>` under a project or inside any git work tree.

| Path | Tier | What it holds | Written by |
|---|---|---|---|
| `<config>/machine.json` | Machine, device-bound | the planes this machine opened (absolute paths), the operator's approval of each with what it would do when opened, the windows open at the last quit, pinned planes and workspaces, the update channel, and this device's id with when it was minted (`device`, ADR 0066: a ULID minted at the first launch that asks, and the key events and records name the device by). And the ISO week the updater last read the weekly manifest in (`weekly`, `2026-W41`, ADR 0083 as amended for OB-17): never sent, and the same value on every machine that checked that week. Pins and the channel are preferences, but they are keyed by absolute path and share one file with the approvals, so the file is device-bound as a whole | `machine::update`, from the app's opener and `purlis update --channel` |
| `<config>/machine.json.lock` | Machine, device-bound, transient | the `flock` that makes a read-modify-write of `machine.json` one act | `machine::update` |
| `<config>/layout.json` | Machine, syncable | the window's arrangement of regions, the two text sizes, and your editor (RC-20, ADR 0081 §6: one of four words, never a program) | `windowprefs::write_layout`, `adopt_layout` (which moves the legacy `charter.layout` localStorage key into it once) |
| `<config>/theme.json` | Machine, syncable | the operator's own theme; purlis only reads it | the operator, by hand |
| `<config>/halted` | Machine, device-bound | the kill switch's marker (ADR 0071): an empty file whose existence means every agent purlis started on this machine is stopped | `halt::stop`, from the window's Stop all and `purlis stop --all`; only the window's re-arm removes it |
| `<config>/kill-switch.jsonl` | Machine, device-bound | the kill switch's journal (ADR 0071): one line per stop, re-arm or tamper, the newest 1000. On a runner, `by` may also be `link`: a stop or re-arm sent from a paired desktop (**decided, not yet written**, ADR 0078) | `halt.rs`, from the app and `purlis stop --all` |
| `<config>/restarted-to-update` | Machine, device-bound, transient | an empty file: the last quit was **Restart to update** | `reopen::mark_restart_to_update`; the next launch removes it |
| `<config>/runs/journal.json` | Machine, device-bound | **decided, not yet written** (ADR 0068, ADR 0076; its contents pre-empt FD-29). The run journal: each live run's chat and run ids, its state with its reason or hold, the state a paused run was paused from, its conversation, harness and level; a run hibernated by a quit stays in it. Replaced whole at every move; read at the host's start to account for a crash. Kept out of `<config>/charterd/`, which is transient. Chats are denied it. Backed up by FR-10 | `purlisd`, its only writer (FD-29) |
| `<config>/charterd/<scope>` | Machine, device-bound, transient | **decided, not yet written** (ADR 0068, ADR 0069, ADR 0081). One credential per human client scope (`local-ui`, `terminal`, `fleet-mcp`, `approval` and `editor`), minted fresh at each start of `purlisd`, so a crash or an upgrade rotates them. Chats are denied the directory | `purlisd` (FD-6, FD-27) |
| `<config>/extensions.json` | Machine, device-bound | each installed extension: its absolute path, the fingerprint the operator approved, and whether it is on; for a runner provider, also whether the operator marked its machines as the user's own (**decided, not yet written**, ADR 0089; default not owned, changed only on `local-ui`, never by the provider's own claim) | `extension::install`, `approve`, `set_on`, `forget` |
| `<config>/forge-etags/native-<account>/<sha256>.json` | Machine, device-bound, rebuildable | the native forge transport's ETag store (ADR 0070 §3, FW-2a): per forge account (`<kind>-<host>-<login>`), one file per request path and query, named by its SHA-256, holding the answer's ETag, its `Link` header and its body, so that a `304 Not Modified` is answered from here. Never a credential. **A chat is denied it, to read and to write**: its sandbox denies the directory under ADR 0067's human-powers class (`sandbox::Denied`), and in purlis only the native transport reads or writes it, which only a human in the window resolves to. A CLI call never shares an entry with it. Separate from FW-7's item cache; deleting it costs one full answer per request | `forge::etag::EtagDir`, from `forge::http::Http` |
| `<config>/forge-budget/<account>.json` | Machine, device-bound, transient | the request budget per forge account (FI14, FW-4): this hour's start, the requests sent as the account, those its forge counts, the `304`s, the background requests held back, and the forge's last stated limits. Never a request, an answer or a credential. Written by the process that sends (`forge::budget::Meter`), read by `purlis doctor`'s `forge budget` rows; a chat cannot write it (the machine store is denied to a chat's writes). Deleting it starts a fresh hour | `forge::budget::Meter` |
| `<config>/network-log/<YYYY-MM-DD>.jsonl` | Machine, device-bound | the network log (OB-15, X50, ADR 0083 §9): one JSON line per forge call, `purlis report` filing and updater read purlis made on this machine, a file a UTC day, the newest 30 kept, so it is retained rather than transient and FR-10 may back it up. A line holds the time, the feature (`forge`, `updater`, `report`), the route (`https`, `gh`, `glab`), the host, the method, the path as a template (no query; the names after `repos`, `orgs` and the like, and everything after `branches`, `contents` and the like, shown as `{}`, and any other segment that is not a word of a forge's API too), the status or that no answer came, the time it took, and whether it went to purlis or to a third party. **Never** a body, a header value or token, a query, a name from a path, the account, or a chat's content. A purlis line keeps its path whole, because purlis's addresses are public. Nothing sends it anywhere and nothing reads it but a person (`netlog::entries`). A sandboxed chat's own process cannot write it, as it cannot write anything under `<config>` | `purlis_core::netlog`, from the forge transports, `purlis report` and the app's updater |
| `<config>/plugin/` | Machine, device-bound, rebuildable | the copy of the bundled plugin that chats started outside the app load (ADR 0057). Its hooks name this binary by absolute path | `plugin_install`, refreshed at launch |
| `<config>/local-plane/` | Plane | the **local project** the first run makes on a machine that has none (FR-4, #603), so nobody is asked where it goes. An ordinary plane in every respect this document records — its own `charter.toml`, its own git — opened through the same trust gate as any other; only its location is fixed. It has **no remote**, so the Plane tier's backup (the remote) does not exist for it until the operator shares it: **FR-10 must cover it** or it has no backup at all. A repo opened from the first run or from New project is cloned into its `workspaces/<name>/<name>/`, `<name>` being the repo's, with `-2`, `-3`… when a different repo already holds that name | `firstrun::ensure_local_plane`, `firstrun::take_in` (`crates/purlis-core/src/firstrun.rs`) |
| `<app data>/shims/` | Machine, device-bound, rebuildable | the `PATH` shims that warn when a harness is started by hand in a shell tab (ADR 0062) | `shellguard`, rewritten at every launch |
| `<app data>/git-hooks/` | Machine, device-bound, rebuildable | the git hooks a harness chat's git runs through `core.hooksPath`: purlis's scan of a commit and of what a push sends, then the repository's own hook of each name (ADR 0074) | `githooks`, rewritten at every launch |
| `<app log>/panics.log` | Machine, device-bound, transient | panic records: thread, place, message, backtrace, version. `$CHARTER_PANIC_LOG` moves it | `app/src-tauri/src/panics.rs`; `purlis report` reads it |
| `<app log>/charter.<YYYY-MM-DD>.log` | Machine, device-bound, transient | the app's diagnostic log (#647): one line per thing the app or the core noticed and nobody asked to see, with its time, level and source, from purlis's own code at `info` and up and from its libraries at `warn` and up. A new file each day, the newest seven kept. An event that looks like it holds a credential or personal data (`secretshape::found`, `secretshape::leaks` per line, or a field named for a token or a secret) is replaced whole by a line naming its kind. The same messages, without time or level, also go to standard error. Not the audit and not telemetry (O1, ADR 0075): nothing reads it but a person. `$CHARTER_LOG_DIR` moves it | `purlis_core::applog`, installed first thing by the app |
| `<extension dir>/<state>/facts.json` | None | an extension's footer facts, written by the extension's own program wherever the operator installed it; purlis only reads it | the extension |
| keyring `purlis/<vault>/<8 hex>` (written now), or `charter/<vault>/<8 hex>` (still read during the rename window, #1261), account = the key | Keyring | a keyring vault's values. The random service name is recorded only in `.charter/vaults/<name>.keys.json` | `secrets::keyring` |
| keyring `purlis/@identity/<16 hex>` (written now), or `charter/@identity/<16 hex>` (still read during the rename window for a record without `base`, #1261), account = the variable's name | Keyring | a vault provider's identity, such as a 1Password service-account token | `secrets::identity` |
| `<config>/forge-accounts.json` | Machine, device-bound | **decided, not yet written** (ADR 0077). Each forge account: its id (a ULID), kind, host, login, how it was signed in (`device`, `pkce`, `pat` or `import`), the client id of a registration made on a host purlis has none compiled in for, whether it is signed in and the scopes last read; and each repo's or owner's binding to one account. Nothing secret. Device-bound because each entry points into this machine's keyring. Backed up by FR-10 and restored only onto a machine that replaces the old one (ADR 0069 §5), with every account signed out, since no token is backed up | the process holding the human scope: the window, then `purlisd` on `local-ui` (FW-3a, FW-3b) |
| keyring `charter/@forge/<host>/<id>`, `<id>` = the forge account's id | Keyring | **decided, not yet written** (ADR 0070, ADR 0077). A forge account's token: the access token, and the refresh token and expiry where the flow gives them. The human's; only a `local-ui` caller reads or refreshes it, and a chat's sandbox denies it. The `@` keeps it apart from any vault's items | FW-3a, FW-3b |
| `<data>/events/<device>/events.jsonl` | Machine, device-bound | the host's event log (FD-9, ADR 0066, ADR 0068): one JSON line per event in ADR 0066's envelope (`v`, `device_id`, `seq`, `ulid`, `chat`, `run`, `parent_run`, `kind`, `body`), `seq` from 1 and never reused, one writer per device holding a lock on `events.lock` beside it. This file is the segment being written; at 16 MiB it is sealed as `events.<first seq>.jsonl` and a new one begun. A line a crash tore is cut off when the log is next opened, and a file with lines but no readable `seq` is refused rather than counted from 1 again. Kinds written today: `run.started` (body `cause`: `start`, `clear`, `reopen` for a chat a relaunch put back in its conversation, `fresh` for one started again without it, or `child` for a sub-agent's run, whose `parent_run` is the run that was current; `wake` and `switch` are named and not yet written), `hook.<word>` for every state hook (`sessionstart` adds `started`), `hook.<word>` for every tool hook purlis answers, `hook.commit_refused` for each commit purlis's git hook refused (no text, only the chat), `trust.sandbox.off` and `trust.sandbox.on` (ADR 0067 §7, ADR 0075 §4) under the run a start just began and before its program runs — `off` for a chat in a sandboxed project that starts without the sandbox (`actor_kind` `human`, `actor` `operator`, `scope` `local-ui` for a person's opt-out from the picker; `actor_kind` `host`, `actor` `purlis (no backend on this OS)` and `os` for a system with no backend, ruling V78 b), with `harness`, `persona`, the `reason` typed (one line, at most 200 characters, or null) and the classes `lifted`; `on` (`actor_kind` `host`, `actor` `purlis`, `harness`, `persona`, `restored`) for a chat whose last run was unsandboxed starting sandboxed — and `hook.unknown` (with the `word`, shortened) for a tool hook word it does not. A tool event has `tool`, `call`, `args` (the HMAC of the arguments' SHA-256, never the arguments), `decision` (`allow`, `ask`, `deny` or `none`), `rule` for a denial (`guard-crashed`, `guard-unanswered` and `unknown-hook` among them), `hook_ms`, and `tool_ms` on whichever of a call's pre and post hooks is heard second, within the hour, by the hooks' own clocks. Lines are written in the order their hooks connected, best effort: ordered unless recording a line takes longer than 50 ms. Chat and run ids are ULIDs. A chat's id is the one `app/reopen.json` keeps for it, so it is the same across a relaunch, and the host writes a chat's `run.started` as it starts the chat, before its program can send a line; a chat the host was never told of is given ids at its first line. Each event is `fsync`ed (the operating system's ordinary `fsync`; on macOS not `F_FULLFSYNC`) before the host tells the hook its line is taken, and the hook answers its harness only after that or after spooling the line (FD-30, `app/spool/`). What a drain of the hook spool finds is recorded at the project's open: each line that checked as it would have been live, with `spooled` its number, under the chat and run the reopen record names (with no chat or run, and `chat_number`, for a chat it does not), `hook.spool.gap` (`from`, `to`), `hook.spool.rejected` (`seq`, `why`: `unreadable`, `no-key`, `another-chats-key`, `mac`, `not-this-chats` or `repeated`) and `hook.spool.drained` (`from`, `to`), each with `chat_number`. A line the host took after the hook stopped waiting and that was spooled too is recorded twice. Never committed and never sent. The audit (ADR 0075) and OTel logs (ADR 0083) are written from it. Retention (FD-24): every event of the last 30 days is kept, and more; a sealed segment is deleted, when the log is opened or a segment sealed, once its newest event is older than that, and the segment being written and the newest sealed one never are. A client reads it with `subscribe(since)` from the last `seq` it holds and gets every later event in order and once, across segments and across a host killed and started again; a cursor older than what is kept, or one that falls in a segment no longer kept, gets a `missed` marker and carries on from the next event kept. Backed up by FR-10 (ADR 0069 row 63) | `purlis_core::eventlog::Recorder`, held by the app, its only writer; the app's doctor has an `event log` row |
| `<data>/events/<device>/events.<first seq>.jsonl` | Machine, device-bound | a sealed segment of the event log above: the same lines, from the `seq` its name gives (20 digits, so names sort as numbers), `fsync`ed before it is renamed, and the directory after the rename, never written again (while the next segment cannot be made, appends fail rather than go into this one), deleted by the event log's retention. Backed up by FR-10 with the log | `purlis_core::eventlog::Log`, which seals and deletes them; `eventlog::read` and `eventlog::subscribe` read them |
| `<data>/events/<device>/events.lock` | Machine, device-bound, rebuildable | empty; the event log's one writer holds a lock on it for as long as it writes, so a second host is refused (ADR 0068). A file of its own so the lock outlives each sealed segment | `purlis_core::eventlog::Log` |
| `<data>/events/<device>/args.key` | Machine, device-bound | 32 random bytes, `0600`, that key the event log's args digests, so a digest of `ls -la` cannot be matched by anyone who lacks the key and stays comparable from one launch to the next. Anything running as the same OS user can read it, and so can whoever holds a backup that carries it; the key protects the digests from everyone else. The hook sends the host the arguments' *unkeyed* SHA-256 on the chat's hook channel, which only the same user can reach. Backed up with the log, which it is useless without (AU-19 may move it into the keyring) | `eventlog::ArgsKey`, made on first use |
| `<data>/audit/<device>/active.jsonl` | Machine, device-bound | **decided, not yet written** (ADR 0075). The audit segment being written: one JSON line per audit entry, metadata only, people as keyed pseudonyms. Chats are denied it. Backed up by FR-10 | `purlisd`, its only writer (AU-3) |
| `<data>/audit/<device>/<first>-<last>.jsonl.zst` | Machine, device-bound | **decided, not yet written** (ADR 0075). A sealed audit segment, zstd, named by its first and last entry numbers; pruned only whole, oldest first, and only once a checkpoint covers it. Backed up by FR-10 | `purlisd` (AU-3) |
| `<data>/audit/<device>/checkpoints/` | Machine, device-bound | **decided, not yet written** (ADR 0075). The audit chain's signed checkpoints. Backed up by FR-10 | `purlisd` (AU-7) |
| `<data>/audit/retention.json` | Machine, syncable | **decided, not yet written** (ADR 0075). The audit's retention and disk cap (defaults one year and 2 GiB); every change is itself an audit entry | `purlisd`, from the viewer's setting (AU-8) |
| `<data>/index/` | Machine, device-bound, rebuildable | **decided, not yet written** (ADR 0079). The machine's search index over the transcript archive (KN-31) only: `current`, naming the live generation, and one directory per generation holding `search.sqlite` (WAL, FTS5 contentless at `detail=full`: terms with their positions and row metadata, no text as written; a span the shape scanner flags is indexed as its kind). Each row carries its chat, run, turn and the `<clone-key>` of the clone the chat ran in, which the host filters a chat's search on. Rows stop answering when the archive drops their chat, and leave the file at the `optimize` and checkpoint that follow every erasure. Chats are denied it. Not backed up | `purlisd`, its only writer (KN-32) |
| `<data>/index/writer.lock` | Machine, device-bound, transient | **decided, not yet written** (ADR 0079). The machine index writer's advisory `flock` | `purlisd` (KN-32) |
| `<data>/reviews/` | Machine, syncable | **decided, not yet written** (ADR 0069, ADR 0084). One file per review of a branch: the operator's review draft (each comment's path, side, line or range, the commit it was written against and the operator's text; never code), the *Viewed* ticks, the last reviewed head, where the operator was, and the comments already sent. A review is named by project, workspace, repo and branch, never by an absolute path. Chats are denied it, reading and writing. Backed up by FR-10 | the window (RC-7) |
| keyring item for a human's audit pseudonym key | Keyring | **decided, not yet written** (ADR 0075). One key per human principal on this device, which turns that person's principal into the pseudonyms the audit stores. Deleting it is erasure. Its item name is AU-18's | AU-18 |
| keyring item for the device key | Keyring | **decided, not yet written** (ADR 0066, ADR 0075). The key that signs this device's audit chain; on a headless host, an age-encrypted file stands in for it. Its item name is AU-3's | AU-3 |
| `<config>/runners.json` | Machine, device-bound | **decided, not yet written** (ADR 0078). The runners this machine uses: each one's name, connector (an argument vector, never a shell string), preset, provider if any, its role label (`harness`, `app` or `browser`) and tags (ADR 0089), the runner's device id and its pinned public link key. Written only from a human scope, and denied to chats. A project names a runner and never defines one | `purlis runner add` and `remove`, and the window (RR-1) |
| `<config>/peers.json` | Machine, device-bound | **decided, not yet written** (ADR 0078). On a runner: the devices paired with it, each one's device id, pinned public link key and when it was paired. Its host refuses a link from any other key. Denied to chats | the runner's `purlisd`, at pairing; `purlis runner peers` (RR-1) |
| `<config>/server/<ver>/` | Machine, device-bound, rebuildable | **decided, not yet written** (ADR 0068, ADR 0078). On a runner: the verified `purlis` binary of each host version, side by side while an old one drains. Denied to chats | the desktop's bootstrap, through the connector (RR-14) |
| `<data>/repos/<workspace>/<repo>.git` | Machine, device-bound, rebuildable | **decided, not yet written** (ADR 0078). On a runner, in purlis's data home: one bare repo per workspace repo, which the desktop pushes to over the link and fetches from. The desktop's clone is the truth. Denied to chats | the runner's `purlisd` (RR-16) |
| keyring item for the link key | Keyring | **decided, not yet written** (ADR 0078). This device's static X25519 key for the Noise `XX` handshake of a runner link, apart from the device key. On a headless runner the link key is held in the same age-encrypted form as the device key. Denied to chats | `purlis runner add`, or the runner's `purlisd` at pairing (RR-1) |
| `<data>/runners/<runner-id>.json` | Machine, device-bound | **decided, not yet written** (ADR 0089). One short-lived runner's record, made by a provider for a human or a chat: its id, role label, tags, provider and the provider's handle for the machine, its owner (a chat and run for an agent-started runner, or the workspace for one a human started or pinned), workspace and source ref, its pinned device id and public link key, its forwarded ports and public URLs with their expiries, its state, and its runner-hours and spend so far. Read at every start of `purlisd` to stop or destroy orphans. Denied to chats. Backed up by FR-10, so a restore onto a replacing machine can stop what still runs or bills | the desktop's `purlisd`, its only writer (RR-29) |
| `<config>/runner-grants.json` | Machine, device-bound | **decided, not yet written** (ADR 0089). Per project and workspace, each **grant**: the repo and spec path that declared it, the vault and entry it approves, its test credential if any, the commits the operator approved for it (a grant reaches only a runner building a ref a human chose), and when and by whom. Never a value. Written only on `local-ui` or by `purlis` on that scope; a chat cannot add, widen or read it. Device-bound because each grant points into this machine's vaults; a restore marks a grant whose entry is missing. Backed up by FR-10 | the window and `purlis runner grant` (RR-33) |
| `<config>/runner-budgets.json` | Machine, device-bound | **decided, not yet written** (ADR 0089). Per project and workspace, the runner budget agents start runners within, which counts and stops only agent-started runners: concurrent agent-started runners, runner-hours a day, spend a month for a provider that reports cost (zero until a human sets it), the idle timeout, and the period's usage. Written only on `local-ui` (the window, or `purlis` on that scope); denied to chats. Backed up by FR-10 | the window and `purlis runner budget` (RR-31) |
| runner images and stopped containers | None | **decided, not yet written** (ADR 0089). The images the core provider builds from a devcontainer spec with the verified `purlis` in a layer of its own, and the containers of runners not yet destroyed. They are the container engine's, in its own store; purlis names them by runner id and prunes them, and owns no file there | the core Docker and Podman provider (RR-3) |
| `<data>/telemetry/` | Machine, device-bound | **decided, not yet written** (ADR 0083). The telemetry store: OTel records from the harnesses, purlis's own spans, the host's counters and logs derived from the event log, after people and gated content are removed; rolling segments and an index the views read. 30 days and 1 GiB by default, oldest segment deleted first. Never the audit. Not backed up, as a named exception to ADR 0069 §2: losing it costs past charts. Denied to chats (ADR 0067 §5 class 2) | `purlisd`'s telemetry receiver, its only writer (OB-2) |
| `<data>/telemetry/cursors.json` | Machine, device-bound, rebuildable | **decided, not yet written** (ADR 0083). Each export destination's place in the telemetry store; a lost cursor restarts that destination at the oldest record | `purlisd` (OB-9) |
| `<config>/telemetry.json` | Machine, device-bound | **decided, not yet written** (ADR 0083). Whether telemetry is collected, its retention and cap, each project's open content gates, and the export destinations: each one's name, OTLP/HTTP URL, signals and a pointer to its auth header in the keyring. No project file opens a gate or names a destination. Device-bound because of those pointers and consents. Denied to chats (ADR 0067 §5 class 2). Backed up by FR-10 | a `local-ui` caller only (the window, or `purlis` on that scope), through `purlisd` (OB-2, OB-9) |
| keyring item for a telemetry export destination's auth header | Keyring | **decided, not yet written** (ADR 0083). The header value an export destination sends, such as a backend's API key. Denied to chats | OB-9 |

### Environment variables that move or key this state

| Variable | Effect | Source |
|---|---|---|
| `CHARTER_HOME` | **In the Python charter, replaces the state directory outright**: every file in this section moves, verbatim, with no migration. **In purlis only part of it moves** ([#750](https://github.com/purlis/purlis/issues/750)). The local vault registry, `vaults/`, `fingerprint.key`, the push and save journals, the gate files (`sessions/<sid>.tools`, `.gate`, `commit-gate/`), `sessions/<sid>.memnudge`, `dispatch-inflight/`, `ws-edit-nudge/`, `agent-personas.json`, `mcp-approved.json` and `unrecorded/` move, because their writers call `plane::state_dir`. The session and terminal pointers, `active-persona`, `sessions/<sid>.usage`, `sessions/<chat>.saved`, `persona-state/`, `handbacks/`, `cache/glstate.json`, `harness-profiles-launched.json`, `app/` and `workspace-rename.json` stay in `<plane>/.charter`, because their writers join `.charter` to the root themselves. **Decided, not yet written** (ADR 0079): the project's search index, `index/<clone-key>/`, moves with the state directory (V22b), keyed by clone so that clones sharing one directory never share an index | `charter/config.py:42`, `charter/config.py:110`; `crates/purlis-core/src/plane.rs` (`state_dir`) |
| `CHARTER_ROOT` | Picks the plane (hence `<root>/.charter`); a bad value raises rather than falling back | `charter/root.py:20` |
| `CHARTER_SESSION_ID` | Names `sessions/<sid>.*`, `commit-gate/<sid>`, `ws-edit-nudge/<sid>-…`, the trace bucket, and inside a frame it is the **chat id** that names `frame/<chat>/` and `chat-turns/<chat>` | `charter/session.py:65`, shadowing explained `charter/session.py:46` |
| `CLAUDE_CODE_SESSION_ID` | Fallback for the above | `charter/session.py:66` |
| `TERM_SESSION_ID` / `TMUX_PANE` / `STY` / `SSH_TTY` (then `ttyname`) | Name `terminals/<tid>.*` | `charter/session.py:75`–`:79`, `charter/session.py:111` |
| `CHARTER_WORKSPACE` | Overrides the resolved workspace **without writing anything**; also the `identity` pin read per chat. **purlis sets it on every chat it starts in a workspace** (SI-1), and never lets a chat inherit the app's own | `charter/workspace.py:550`, `charter/frame/state.py:1616`; `crates/purlis-core/src/start.rs` |
| `CHARTER_PLANE_ROOT_SESSION` | **purlis only** (SI-1). `1` on a chat the app started at the plane root — the plane's own directory or anywhere under it that is no workspace's (SI-1b) — so the session is in no workspace (see the workspace resolution order). Any other value is ignored. A variable of its own rather than a value of `CHARTER_WORKSPACE`, so nothing that reads that one as a name ever sees it; set by the app, never inherited, and refused in a profile's `env` like every `CHARTER_` name | `crates/purlis-core/src/active.rs` (`PLANE_ROOT_ENV`, `at_plane_root`) |
| `CHARTER_PERSONA` | Same, for personas | `charter/persona.py:1260` |
| `CHARTER_HARNESS` | Which harness the registry reports; stored per chat in `frame/<chat>/identity` | `charter/harness/registry.py:46`, `charter/commands_frame.py:3058` |
| `CHARTER_WORKTREES` | Moves worktrees (not state) | `charter/config.py:82` |
| `CHARTER_NO_BACKGROUND_CHECKS` | Suppresses the spawners that write `cache/glstate.*` and `cache/update.*` | `charter/util.py:382`, gates at `charter/glstate.py:209`, `charter/update.py:491` |
| `CLAUDE_CONFIG_DIR` | Recorded into `guard-seen.json` | `charter/guardseen.py:62` |
| `CLAUDE_PLUGIN_ROOT` | Decides `guard-seen.json`'s `source` field (`plugin` vs `settings`) | `charter/guardseen.py:55` |
| `CLAUDE_PID` | Adopting a harness pid into `frame/<chat>/harness.pid` | `charter/hooks.py:6084` |
| `CHARTER_CONFIG_HOME` / `XDG_CONFIG_HOME` | Move `reporting-consent` | `charter/report.py:462` |
| `CHARTER_DATA_HOME` / `XDG_DATA_HOME` | Move `<data>`, purlis's data home: the event log in it today (FD-9), and the audit once it is written (ADR 0075). A value under a plane or inside a git work tree is refused | ADR 0075, `crates/purlis-core/src/datahome.rs` |
| `EDM_HOME` / `EDM_WORKSPACE` / `EDM_PERSONA` | Legacy names, warned about only | `charter/legacyenv.py:39` |

---

### What the tmux frame and the status line read that a **hook** wrote

One process writes each of these and a different one reads it, which is what makes them
stable to purlis. It is also the shape of the problem the app has: Python's hooks keep
writing a session's state, and the app has to learn the same facts. It does **not** inherit
this surface file by file — `frame/` is the tmux frame's own (the ruling above) — so where a
row below names a `frame/<chat>/` file, the app needs its own answer to the same question,
and the row says what that question is.

| Written by (Python hook) | File | Read by |
|---|---|---|
| `sessionstart` → `toolgate.snapshot` (`charter/hooks.py:7620`) | `sessions/<sid>.tools`, `sessions/<sid>.gate` | the PreToolUse guard, `purlis persona` |
| `sessionstart` → `_record_harness_session` (`charter/hooks.py:6102`) | `frame/<chat>/session`, `session.durable`, `session.adopted`, `harness.pid`, `conversation` | frame panels, `purlis reopen`, tab menu |
| every hook → `notify.plane_changed` (`charter/frame/notify.py:132`) | `frame/<chat>/version` **and** `frame/<chat>/gather.json` | `charter/frame/panel.py:802` (the repaint loop) |
| `_turn_begin`/`_turn_bump` (`charter/hooks.py:5971`, `:6177`) | `chat-turns/<chat>` (mtime) | `charter/frame/panel.py:657`, `charter/frame/slots.py:5060` |
| `pretooluse-dispatch` (`charter/hooks.py:7954`) | `dispatch-inflight/*.json` | `charter/statusline.py:1834`, `charter/frame/panel.py:580` |
| the guard (`charter/hooks.py:6222`) | `guard-seen.json` | `charter/doctor.py:2002` |
| `charter statusline` (fed the harness payload) | `sessions/<sid>.usage` | `charter/statusline.py:735` → frame panels (the `ctx %` exists nowhere else) |
| `purlis ws use` in the chat's shell | `sessions/<sid>.workspace`, `sessions/<sid>.lock` | every purlis process in that chat |

Two of these are *not* keyed the same way: `.usage` is keyed on **Claude Code's** session id
out of the payload, while everything frame-side is keyed on the **chat id**
(`$CHARTER_SESSION_ID`). A chat's context gauge therefore needs a
chat → harness-session mapping; `frame/<chat>/session` is where the tmux frame keeps its
copy, and an app that does not read `frame/` needs one of its own.

---

## Appendix: what this survey found in the code

Recording the format meant reading every writer and reader. These are the things
that surprised the survey: stale docs, latent divergences, and writes that are not
atomic where their neighbours are. **Nothing here was fixed** — the Python charter is
frozen (spec decision 16), and a rebuild has to reproduce the behaviour as it is, not
as it was meant to be. They are recorded so the rebuild copies them deliberately and
so each one can be filed on its own merits.

### In the plane root

1. **`[harness] default` has two readers with two vocabularies.**
   `instance.harness_of` accepts only a registry `cli_name` and records anything else as
   `refused` (`charter/instance.py:3088`), while `profiles.derive` treats the same key as
   naming *any* profile, including one only `charter.local.toml` declares
   (`charter/profiles.py:318`). `doctor` papers over it by checking the refused value against
   the profile set (`charter/doctor.py:390`). `docs/control-plane.md:96` documents it as
   "which row bare `purlis`'s profile selector starts on" and lists only the three harness
   words. A Rust reader has to implement both readings. Flagged as **stable** either way.
2. **`workspaces/.gitkeep` is ignored-negated and anchored on, but never created** by `init`
   or `reinit` (measured). Whether the app should create it is a question for the workspaces
   area; the anchor line matters to `.gitignore` splicing either way. *purlis creates
   it (#355).*
3. **`charter.toml` is rewritten non-atomically** (`p.write_text`, `charter/instance.py:441`)
   with no lock, while every `.charter/` writer goes through `config.replace_for`. Two
   concurrent `persona default` / `version bump` runs can interleave. Marked stable; noting the
   write discipline because a second implementation writing the same file needs to know
   purlis does *not* hold a lock. *purlis replaces it by rename under a lock (#357).*
4. **`.charter/harness-profiles-launched.json` is read-modify-written whole with no lock**
   (`charter/profiletrust.py:172`). Its own module documents that it is not a boundary
   (`charter/profiletrust.py:18`). Marked **stable** because a second process reads it and
   it gates command execution — confirm that is the classification the doc wants for a
   `.charter/` file.
5. **`instance.SCHEMA` vs `workspace.STRUCTURE_VERSION`** are deliberately never compared
   (`charter/instance.py:33`). Only the plane number refuses. Worth stating in the assembled
   doc so a Rust reader does not gate on the workspace number.
6. **`[[forge]] version`** appears in a code comment as an example of a key `_set_key` must not
   clobber (`charter/instance.py:398`), but no forge code reads a `version` key. It is an
   example, not a setting, and the survey found no reader for it.
7. **`config.GROUP`/`EXCLUDE` are "the first `[[forge]]` block's"** (`charter/config.py:728`,
   `charter/instance.py:148`) while `discover` asks per block (`charter/commands.py:120`). Two
   meanings of one word in one file; both are live.
8. **The empty-inventory skeleton has no `note` key** (`charter/inventory.py:74`) while the
   saved document always does (`charter/inventory.py:308`). A strict reader must treat `note`
   as optional.
9. **`purlis docs` exits 1 on an empty inventory** (`charter/commands.py:220`) and also
   exits 1 when the README roster could not be refreshed (`charter/commands.py:231`) even
   though `docs/topology.md` was written. Not a config-format issue, but it affects any
   fixture generator that checks exit codes.

### In workspaces

1. **`change.record_landing` has no caller in `charter/`.** `charter/change.py:156` is a
   complete landing-log writer (`config.open_for`, `json.dumps(sort_keys=True)`), and the
   suite calls it (`tests/_changerepo.py:122`, `tests/test_change_surface.py:573`) but the
   product does not: the only caller of the landing log is `commands_change._append_landing`
   (`charter/commands_change.py:1092`, called at `charter/commands_change.py:1501`), which
   uses `contain.json_line` and a raw `os.open(..., 0o644)`. The two agree on the line's
   fields and key order but differ in escaping (`ensure_ascii=True` only in the live one)
   and in file mode dispatch. A Rust writer should follow `_append_landing` — and deleting
   `record_landing` as dead would take the behaviour those tests pin with it. Flagged, not
   fixed.
2. **The brief says `refs/` holds `README.md` and `repos.json`.** There is no `repos.json`
   under a workspace's `refs/`. `repos.json` is `inventory/repos.json`
   (`charter/config.py:783`). `refs/` holds the generated `README.md` and whatever the
   operator drops in.
3. **`.charter-generated` is stable**, although only purlis reads it: a *different
   process* (a later launch, `doctor`, `reinit`) does, and deleting it is not harmless —
   every generated file then reads as `foreign` and is never refreshed or withdrawn again.
4. **`.charter/workspace-tab-order` and `workspace-arrivals/` are stable**, and the survey
   first had them as internal because they are regenerated at the next launch
   (`charter/workspace.py:1066`, `charter/workspace.py:1229`). Every frame process on the
   plane reads them, which is what the rule turns on; the regeneration is what deleting one
   costs, and the entries say so.
5. **Two different manifest orderings.** A manifest purlis creates has
   `name, description, repos, updated_at, updated_by, charter_generated`; one written by
   `snapshot`/`fork` keeps whatever order the document on disk had and appends new keys
   (`fork` inserts `forked_from`). A byte-identical Rust writer has to preserve the order it
   read rather than impose a canonical one. `charter_generated` is always last because it is
   assigned after the copy (`charter/workspace.py:1611`).
6. **`updated_by` has two sources** — `$USER` for automatic writes and `git config user.name`
   for `snapshot`/`fork` — so the same workspace's manifest can alternate between two
   spellings of the same person. Intentional per `charter/workspace.py:1620`, but worth
   recording in the spec.
7. **Local vs UTC time in one store.** A memory's filename prefix is local time and its body
   stamp is local; every other timestamp in the area is UTC. `memstore.memory_date` reads the
   body first and the filename second (`charter/memstore.py:150`), so the two must agree —
   they do only because both come from the same local `now`.
8. **`charter_generated` digest input uses Python's default JSON separators.** A Rust
   implementation emitting `{"a":1}` instead of `{"a": 1}` produces a different digest and
   every manifest reads as the operator's. This is unwritten anywhere but in the code
   (`charter/workspace.py:1540`).
9. **`_live_block` and `_ws_meta_paths` are two lists of the same set** in two modules, held
   together only by `tests/test_todos_are_committed.py` (`charter/workspace.py:1390`,
   `charter/commands_workspace.py:1094`). A second implementation should derive both from
   one list.
10. **The LIVE block is written non-atomically** (`gi.write_text`,
    `charter/workspace.py:1418`) where every other committed file purlis writes goes through
    `config.replace_for`. A kill mid-write truncates the plane's `.gitignore`. *purlis
    replaces it by rename under a lock (#358).*
11. **Legacy migrations a Rust reader will meet:** `repos/` → `workspaces/`
    (`charter/workspace.py:53`) and `.edm-structure` → `.charter-structure`
    (`charter/workspace.py:4477`, `charter/workspace.py:4494`). Both are best-effort renames
    performed on read.
12. **`workspaces/.gitkeep` is an anchor that `purlis init` never creates.** The line
    `!/workspaces/.gitkeep` is written into `.gitignore` (`charter/commands.py:1091`) and is
    the literal insertion point for the LIVE block (`charter/workspace.py:1414`), but no
    command writes the file itself — measured on a fresh `purlis init`.

### In personas and memory

1. **`.charter/persona-state/trace/<session>.jsonl` — stable or internal?** Marked stable
   (written by hooks, read by `purlis trace`/`persona recall` in other processes), but it
   is machine-local, gitignored and losing it costs only history — but the status line
   reads it in another process, which is what the rule turns on.
2. **`.charter/sessions/<sid>.persona` / `terminals/<tid>.persona` / `active-persona`** —
   written by `persona.py`, but the directories belong to the workspaces area. Someone must
   decide which section owns them so they are not documented twice or dropped.
3. **`.charter/mcp-approved.json`** — written by `mcpseen.py`, but
   `persona.py` is its only consumer and it decides whether a generated agent carries the
   vault wrapper. Overlaps the secrets agent.
4. **`personas/_dispatch` vs `inflight`** — `charter/inflight.py` notes that `_dispatch`
   records a dispatch when it *finishes*; `.charter/dispatch-inflight/` is a separate
   (gitignored) store, documented in the `.charter/` section.
5. **Docs vs code — `persona-state/log/<name>.jsonl`.** `charter/persona.py:15` (module
   docstring) still advertises "an activity log (`log/<name>.jsonl`)"; there is none —
   `charter/persona.py:2458` says so, and activity lives in `trace/`. Docstring is stale.
6. **Docs vs code — frontmatter table.** `docs/personas.md:112`-`:123` lists only `role`,
   `vault`, `delegate-when`, `tools`, `agent-tools`, `extends`, `uses`, `activity`. The
   code reads eight more (`name`, `description`, `agent-description`, `disallowed-tools`,
   `skills`, `draft`, `dispatch-isolation`, plus `routing`/`routes-to`/`borrows`, which are
   documented in later sections) and passes through `model`, `color`, `memory`.
7. **Docs vs code — `purlis persona use` help.** `charter/cli.py:1658` says it "writes
   `.charter/active-persona`"; it writes that file only when there is neither a session id
   nor a terminal id (`charter/persona.py:1529`). In a normal session it writes the two
   pointers instead.
8. **Slug truncation can leave a trailing `-`.** `charter/memstore.py:26`-`:28` strips `-`
   *before* `[:48]`, so a 48-char cut mid-word keeps the hyphen: a memory titled
   "1097 is worse than the issue says: F2 detach ran the wrong command" is filed as
   `1097-is-worse-than-the-issue-says-f2-detach-ran-.md`. Harmless,
   but a byte-identical writer must reproduce it (not fixed here).
9. **No atomic writes anywhere in this area.** `persona.md`, `.claude/agents/<name>.md`,
   memory files and `MEMORY.md` are all plain `write_text`/`open`; the only concurrency
   defence is `O_APPEND` in the two jsonl stores and an flock around the dispatch *commit*
   (`charter/hooks.py:8206`). `MEMORY.md` is explicitly expected to drift and is reconciled
   by `index_drift` (`charter/memstore.py:217`).
10. **`_skills` rows are never committed by purlis**, unlike `_dispatch`
    (`charter/hooks.py:8189` has no skill equivalent), although both are committed paths. So
    on a `share = "push"` plane the skill tally lags behind the dispatch tally. Possibly
    deliberate, possibly an omission.
11. **`_render_agent` reads `meta.get("vault")` for the MCP wrapper (`charter/commands_persona.py:877`)
    but `persona.vault_of(name)` for the credential prose (`:1067`).** `mcp_render_entry`
    normalises `none` via `mcp_vault`, so behaviour is right; the two spellings are still a
    latent divergence if `vault:` is unset and a tagged vault exists (the MCP path would see
    no vault, the prose would see the tagged one).
12. **Front-door personas get `.gitkeep` but no `MEMORY.md`/`refs/README.md`**
    (`charter/commands.py:2678`-`:2680`) while `persona create` gets the opposite
    (`charter/persona.py:2265`). Two scaffolds for one layout — a reader (and `doctor`'s
    index check) sees two shapes.

### In the vaults and the harness wiring

1. **Two different JSON encodings for the two vault-file providers.** `plain-file` writes
   insertion order (`charter/secrets/plain_file.py:91`); `reference` writes `sort_keys=True`
   (`charter/secrets/reference.py:194`). Both are 0600 `.json` files under
   `.charter/vaults/` with the same naming rule, so a byte-identical writer must branch on
   the provider. Deliberate or accidental is not stated anywhere.
2. **The reference provider chmods after writing** (`charter/secrets/reference.py:194`-`195`
   — `write_text` then `os.chmod`), which is exactly the window
   `PlainFileProvider._write_private` was rewritten to close (#437,
   `charter/secrets/plain_file.py:42`). A reference file holds no value, but it is the same
   pattern the codebase documents as a defect. Not fixed here; flagged.
3. **`purlis secret set` never creates `fingerprint.key`; `purlis secret get` does.**
   Measured. Any fixture that wants the key must run a masking read.
4. **`_plugin_dispatches_guard` always asks about `~/.claude`** even under
   `$CLAUDE_CONFIG_DIR` (`charter/commands.py:1230`), while `doctor` follows the
   variable. The code names this as a known divergence pending "per-profile wiring task 4".
   A second implementation must copy the asymmetry, not the variable.
5. **Whether `<plane>/opencode.json` should be marked stable-committed is decided by
   purlis's own output, not by `.gitignore`** — nothing ignores it and `guard` prints
   "These files are committed". Confirmed by reading `_GITIGNORE_BASELINE`; no explicit
   statement in code that it is *meant* to be committed.
6. **`~/.config/opencode/command/charter.md` is create-only** (`charter/harness/opencode.py:982`)
   while the shim and the context file are refreshed. A command file from an older purlis
   is never updated and nothing reports it stale — unlike the shim, which has `unvouched`.
   Possible gap; not fixed.
7. **`ensure_instructions` writes an absolute path into a machine-global config**
   (`charter/harness/opencode.py:682`). Moving `$XDG_CONFIG_HOME` leaves a dead entry, and
   nothing prunes it. Flagged, not fixed.
8. **Marker values are `str | list[str]`** (`charter/workspace.py:2440`). A consumer that
   assumes `str` will mis-read a plane killed mid-write. Worth stating explicitly in the
   doc.
9. `.charter/cache/harness-wiring.json` is marked **internal**
   (one module writes and reads it, safe to delete). It is borderline — two *processes*
   (the selector and a later selector run) read it, and a chat can write it, which is why
   `cached()` re-validates every field. If the doc's rule is "another process reads it", it
   would make it stable; the front matter's cache ruling is why it is not.
10. `.charter/unrecorded/<hash>.json` — same borderline: written by one module, read by
    `doctor` in a *different* process (`charter/workspace.py:2984` `unrecorded_reason`). I
    marked it internal because it is a diagnostic that regenerates; a stricter reading of
    the rule makes it stable.

### In `.charter/`

1. **`docs/control-plane.md:868` disclaims any format stability for `.charter/frame/`**
   ("no format version and never will … may change shape in any release"). The survey
   raised this as a conflict with an app that reads plane files; the ruling in "How to read
   it" settles it the other way — the app replaces the tmux frame rather than reading its
   state, so the disclaimer stands untouched. Recorded because the next person to want a
   fact that lives only in `frame/` will meet it: that fact has to be promoted here first,
   and `control-plane.md` amended in the same change.
2. **Nothing guarantees the `sessions/` sweep runs.** `_prune` unlinks any file in
   `sessions/` and `terminals/` past 30 days, `.ask-pending` included
   (`charter/workspace.py:881`-`:889`) — the survey's first reading of this was wrong, and
   the leave-behind on a declined ask is deliberate and tested, not a leak
   (`charter/workspace.py:875`-`:879`, #290). What is true is that the sweep only runs from
   `set_active`, so a plane whose operator does not switch workspaces never sweeps: 80
   markers had accumulated on the plane this was written against.
3. **`ws-edit-nudge/<sid>-<ws>` keys are ambiguous.** The key is
   `re.sub(r"[^A-Za-z0-9._-]", "", f"{session}-{ws}")` (`charter/hooks.py:7652`) — session
   `a` + workspace `b-c` and session `a-b` + workspace `c` collide. Harmless (one nudge),
   but a Rust reimplementation must copy the *exact* derivation to stay compatible.
4. **`dispatch-inflight` files are written by `tempfile.mkstemp`, not through `config`**
   (`charter/inflight.py:244`), so they bypass `open_for`/`STATE_FILE_MODE`. They come out
   0600 anyway, but they are the one writer in this area outside the dispatch. Is that
   deliberate?
5. **`unrecorded/` uses `config.mkdir_for`, not `private_mkdir`** (`charter/workspace.py:2976`)
   — equivalent today (the path is under the state dir, so `mkdir_for` dispatches to
   private), but it is the only state writer relying on the dispatch rather than saying
   what it means.
6. **Is `active-workspace` dead?** `config.py:804` derives it and nothing in the package
   reads or writes it. If the app should ignore it, say so in the spec; if any older
   purlis still writes it, the app will see a file this doc calls dead.
7. **Where does `sessions/<sid>.persona` belong** — this area (it is a `sessions/` file) or
   the personas section, which documents them in full; the entries here are the
   `.charter/` view of the same files.
8. **`persona-state/trace/<sid>.jsonl` is read by the status line**
   (`charter/statusline.py:2078`) even though `persona-state/` is the personas section's.
   The "recorded ✎N" chip on the footer depends on it, so the app needs that file's format
   too — flagging the cross-area dependency.
9. **`cache/vaulthealth.json:detail` carries provider prose** that can include paths and
   account hints (visible in the live plane). If the app surfaces cache contents verbatim,
   that field needs the same handling as any vault-adjacent string.
10. **`frame/<chat>/brief` is written with no trailing-newline normalisation**
    (`charter/frame/state.py:2566`) while every sibling appends `\n`. A byte-identical
    writer must not "fix" it.
11. **Marker-file ordering is load-bearing in at least one place:** `.gate` must be touched
    *before* `.tools` (`charter/toolgate.py:808`). Any Rust writer that reorders them
    reopens #443. The survey did not enumerate every order dependency (the
    `session`/`session.durable`/`session.adopted` trio is the likely second one,
    `charter/frame/state.py:1023`).
12. **`frame/probe-1/` on the plane this was written against** is a `<prefix>-<pid>`-shaped
    frame id, not a chat id, and holds only `gather.json` + `version`. `frame_id()` gives
    the shape and `purlis frame-probe` (`charter/news.py:1052`, `cmd_probe` at
    `charter/commands_frame.py:599`) is where the name plausibly comes from, but no writer
    in this tree mints `probe-1` itself — most likely an older release left it. A reader
    enumerating `frame/` should expect directory names it cannot account for.
