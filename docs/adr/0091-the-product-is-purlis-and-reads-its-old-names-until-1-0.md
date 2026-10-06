# The product is purlis, and it reads its old names until 1.0

**Accepted 2026-10-05**, by the operator's rulings V92 and V93a to V93p. This record writes them
down for RN-11b (#1270). The spec they were turned into is #1253. It amends
[ADR 0056](0056-the-repo-named-charter-is-the-app.md), whose rule against reusing an old repo
name it keeps.

Until 2026-10-05 the product was called charter. The app's repo was `diazoxide/charter` and the
plane it is developed from was `diazoxide/charter-plane`. The name was in about 63,000 places:
the app, the command line, the docs, the files written into people's projects, the folders kept
on their machines, the keychain names secrets are stored under, the branches and markers made on
their forges, and the plugin installed into their harnesses. About 1 to 2% of those names are
load-bearing. Renamed blindly, they would lose an installed user's secrets, approvals, generated
files and open pull requests, and would strand teammates on older builds.

## The decision

**The repos moved to the `purlis` org (V92).** `diazoxide/charter` became **`purlis/purlis`**
(public) and `diazoxide/charter-plane` became **`purlis/purlis-plane`** (private). Each was one
move with one redirect. **No repo is ever created under an old name again**:
`diazoxide/charter`, `diazoxide/charter-plane` and `charter-app` stay unused for good, because
installed updaters reach new releases through GitHub's redirects from them (ADR 0056 and ADR
0042). The updater's signing key does not change.

**Everything is renamed, 100% (V93a).** The app, the command line, files, folders, markers,
keychain prefixes, environment variables, the plugin, the docs, the ADRs, the CHANGELOG and the
tracker, closed issues included, all say purlis. The name is always lowercase `purlis`, in the
UI, the docs and prose (V93n). The only things left as they are cannot physically change: commit
trailers already in git history, and markers on pull requests that already exist. They are
recognised forever. The aim is that nothing writes "charter" again, but during the window some
things still do: a project not yet migrated keeps its committed files under their old names,
local state stays under its old names until `rename-local` moves it, and the hook and statusLine
commands purlis writes into a harness's settings stay `charter …` until 1.0 (D-RN7-11 below).

**There is a recovery path.** A rename that cannot be undone is not shipped. Every move made on
an engineer's machine is journalled and can be undone (V93f), keychain items are copied and never
moved (V93h), and a project's committed files change only in one commit its owner chooses to make
(V93g).

**One compatibility window, until 1.0 (V93l).** purlis reads both names and writes only the new
one. The window closes at 1.0, or later while the doctor still sees an old name in a project that
was opened. Each version's notes announce the removal, and removing the old names is a ticket of
its own.

## How the two names live together

- **purlis wins when both exist (V93e).** An old name is read only when the purlis name is
  absent. When both exist, the old one is a leftover, and the doctor reports it with a Fix. The
  exception is a committed file in a project that has not been migrated: writes go to whichever
  file exists, so a project's state never splits across two names.
- **One module holds every renamed name.** For each name it gives the purlis name to write, the
  old names still read during the window, and the names recognised forever. Every caller asks
  it, and nothing else spells an old name.
- **`rename-local` is automatic, journalled and undoable (V93f).** On first launch, and as the
  doctor Fix `rename-local`, purlis moves local, uncommitted state to its purlis names: the state
  folder `.charter/`, `charter.local.toml`, the config home, the data home and the log folder.
  Every move is written to a journal, and `purlis migrate --undo` replays it backwards. Nothing
  is deleted. A move that fails leaves the old name in place, and it is still read.
- **The keychain is copied and verified (V93h).** Each item under the `charter/` prefix is copied
  to `purlis/` and read back before purlis switches to it. The old items are kept until the
  window closes, and both prefixes are read meanwhile.
- **`rename-plane` is one explicit commit (V93g).** A project's committed files are migrated only
  by the doctor Fix `rename-plane`, never automatically. It renames `charter.toml`,
  `.charter-scan-allow.toml`, the managed `.gitignore`, `.gitattributes` and exclude blocks, the
  AGENTS.md markers, adds a `purlis` twin beside each `charter` permission rule in the
  project's `.claude/settings.json`, leaving its hook commands as they are (D-RN7-11), and
  adds `requires = ["purlis-names"]`. A build that knows the `requires` mechanism but not this
  requirement opens the project read-only. Builds 0.4.1 and earlier do not read `purlis.toml`
  at all, so a migrated project is not a project to them: they find nothing there to open, and
  so cannot damage it either. `rename-plane` refuses to run inside a chat (D-RN7-12): migrating
  a project's committed files is the operator's act.
- **Hook and statusLine commands stay `charter …` until 1.0 (D-RN7-11).** A harness's settings
  name the command it runs, and teammates on older builds share those settings, so the hooks and
  the statusLine that purlis writes keep calling `charter`, which the alias runs. RN-14 renames
  them when the window closes.
- **Markers are recognised under both names (V93i).** A block written under the old marker is
  rewritten in place when it is next touched, never duplicated, and a generated file's digest
  carries over, so purlis keeps updating the files it wrote.
- **On the forge (V93j)**, new work uses `purlis/…` branches and `Purlis-*` trailers and markers.
  A save pull request still open from a `charter/…` branch carries on until it merges. Old
  trailers and markers are recognised forever.
- **The command line is `purlis`, with a `charter` alias until 1.0 (V93k).** `PURLIS_*`
  environment variables are canonical, and `CHARTER_*` is read as a fallback and passed to
  harnesses as well during the window. The guards recognise both names on a command line, as
  they recognised `edm` after the last rename.
- **The plugin becomes `purlis` (V93m).** Its skills are `purlis:*` and its MCP tools
  `mcp__purlis__*`. The old plugin ids are pinned off, the precedent ADR 0056's amendment set,
  and `rename-local` installs the purlis plugin. `rename-plane` rewrites personas' `charter:`
  skill references.
- **The bundle identifier becomes `dev.purlis.app` (V93b).** On first launch the app takes its
  keychain items over again, the way ruling V90 set up, and moves its old log folder.
- **The docs site is purlis.github.io/purlis (V93o)**, served from `purlis/purlis`. A custom
  domain is a later decision.

## How it ships (V93p)

One scope, in four trains: the compatibility layer (both names read, the `charter` alias, the
`CHARTER_*` fallback); the migrations (`rename-local`, `rename-plane`, undo, the keychain copy,
the marker rewrite, the bundle id and the plugin); every visible name (CLI, UI, docs, history and
the re-recorded scenarios), with the tracker rename script running alongside; and last, the
crates and packages (V93c), in one mechanical change.

## What stays as written

- **Commit trailers and pull request markers already made**, because nothing can rewrite them.
- **URLs and repo slugs that record history**, such as `diazoxide/charter` in an ADR or the
  CHANGELOG. They still resolve through GitHub's redirects.
- **Version numbers released under the old name.** There never was a purlis 0.4.2.
- **Recorded output** quoted in the docs as evidence of what a build printed at the time.
- **The persona's charter**, the English word: a role's charter is what it is for, not the
  product.
- **The Python implementation's files and modules** (`charter/commands.py` and the rest),
  retired at the tag `cli-final`, and the Python implementation itself when it is named as such:
  "the Python charter", "the old charter".
- **Every name the code still spells `charter`**, until the code renames it (D-RN11b-8).

The docs sweep uses the tracker rename's own rules (`tools/tracker-rename.mjs`). What they keep:
fenced and indented blocks in the ADRs and the CHANGELOG, URLs and 1Password references, old repo
slugs and `charter#N` references, old repo names inside paths, existing branch names, versions
released under the old name, old plugin ids, `Charter-*` commit trailers, the persona's charter,
"the Python charter" and "the old charter", the retired Python package, HTML comments (records
something reads back), the GitLab label `charter::ws::<name>`, 1Password item titles, and any
line where two old names would read the same afterwards, which is left for a person to word.

## Decided in implementation

- **D-RN11b-1: ADR file names keep their slugs.** The text inside every ADR is renamed, but
  a file such as `0056-the-repo-named-charter-is-the-app.md` keeps its name. Its path is
  linked from commit messages, pull requests and issues that will not be rewritten, and from
  the docs site, so a rename would break every one of those links for no reader's benefit.
- **D-RN11b-2: crate names and crate paths in the docs follow the crates.** The crates became
  `purlis-core` and `purlis-cli` (`crates/purlis-core/…`) in the same change as the docs
  rename (RN-13), so the docs name them as they are, every link out of `docs/` resolves and
  the docs site's link check passes. The ADRs written before it keep the crate names they
  were written with; only their links' paths moved.
- **D-RN11b-3 and D-RN11b-8: the docs rename only prose.** Any identifier the code still
  spells `charter` stays exactly as the code spells it today: paths, folder names, file names,
  environment variables without a `PURLIS_` spelling, labels, sockets, the bundle id
  `dev.charter.app` (RN-9 changes it), the manifest's `[charter] version` table, the session
  host's `charterd.sock` and `charterd/` folder, and 1Password item titles. Where a name moves
  during the window, the docs say so: "`purlis/` once rename-local has moved it, else
  `charter/`". Recorded evidence keeps its words too: measured output, quoted operator rulings,
  and the paths and names in CHANGELOG entries for versions already released.
- **D-RN11b-4: shown commands and example output are renamed**, because the build prints
  purlis once its visible names are renamed. Only output quoted as evidence of what an earlier
  build printed keeps its old text.
- **D-RN11b-5: ADR 0056 keeps its old names.** Its subject is the names themselves, so renaming
  its text would make it say that `purlis` must never be reused. It gains a note pointing here
  instead.
- **D-RN11b-6: AGENTS.md, CONTRIBUTING, SECURITY, SUPPORT and the PR template are swept as
  docs.** SKILL.md files, templates and test fixtures are shipped text, and RN-11a's.
- **D-RN11b-7: passages that already described both names keep their authors' wording**,
  apart from the product's own name in plain prose.
- **D-RN11c-1: the docs site's deploy job runs only by hand.** Ruling V38 deferred publishing.
  The job is ready for when the operator turns Pages on for `purlis/purlis`, and it runs only
  when someone starts it by hand on `main`.
- **D-RN11c-2: the site's npm package keeps the name `charter-site`** until RN-13 renames the
  packages.
