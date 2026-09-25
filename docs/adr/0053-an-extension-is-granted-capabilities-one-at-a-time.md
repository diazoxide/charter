# An extension is granted capabilities, one at a time

**Accepted 2026-09-25**, from the operator's grill of the same day (charter-app#336). The first
change under it is charter-app#338.

charter has an extension runtime (ADR 0041, ADR 0043, ADR 0048). Today an extension can add a
theme, a side panel, or a view. A view's program is asked one question per click and answers
with blocks charter draws, and those blocks cannot be acted on. The operator chose to give
extensions their full set of capabilities first, and to build the extensions after that. The
other option was to start with read-only extensions. This record fixes how every capability is
added, so each one reads like the last. It follows ADR 0041's threat model as its **2026-09-23
amendment** states it: stage 2 exists and the gate items are met. **That amendment supersedes the
2026-09-22 one** where the two disagree about which gate items are met.

## The decision

**A capability is something charter does for an extension that asked for it and was
approved.** It describes charter's conduct. It is never a limit on what the extension can do.
An extension still runs as the operator, and `RUNS_AS_YOU` stays true.

**The manifest names its capabilities.** `charter-extension.json` has a top-level
`capabilities` list of words, such as `["badges", "actions"]`. Each capability that needs a
shape declares it in `contributes`, under the same word. All of it sits inside the manifest's
bytes, so the fingerprint already covers it. Adding a capability to an approved extension
therefore asks the operator again. The approval prompt names each capability first, one line
each, in the core's words (`Capability::asks`). The dialog and the Extensions list both read
that one list.

**An unknown word refuses the whole manifest.** The refusal is a sentence naming the word.
charter never loads the rest. If it did, the operator would have approved the extension for less
than it asked for, and the same extension would behave differently on a charter that knows the
word. A manifest with no `capabilities` asks for none, and loads exactly as it did before the
list existed. Its fingerprint is unchanged.

**`version` is the protocol the extension speaks.** Until now it was the manifest format's
version. It was also the same number as the executor's request protocol (`"charter": 1`), and
from here on that is the only thing it means. The protocol goes up once for each capability that
changes what a request or an answer holds. charter keeps answering every version up to its own,
and asks each extension in the version its manifest names. The first capability that bumps it also
makes the executor ask in the declared version, and hashes the declared version into the
fingerprint in place of the executor's own. charter-app#341 did both (see its amendment below).
A capability that needs a later protocol is refused in a manifest that names an earlier one.

**Protocol 2 holds three request kinds.** charter-app#341 made it with *run action*.
charter-app#343 added two more while it was still unreleased, rather than bumping to 3: an event
request carries `event` (and `workspace`, `from` where the event has them) and is answered
`{"charter": 2}` or an `error`; a briefing request carries `briefing` (`workspace`, `persona`)
and is answered with `section`, a string. Both carry `writes` like every protocol-2 request, and
charter watches the plane while each is answered. `events` and `briefing` need protocol 2.

**The vocabulary grows one capability per change.** Each change adds the capability to
`crates/charter-core/src/extension/capability.rs`. The same change adds it to the
`extension-probe` crate's manifest and proves it through the real registry and executor:
declared, fingerprinted, approved, run, answered, and refused when asked for wrongly. It also
writes the capability's own amendment to ADR 0041 in that change. Until the first real
capability lands, the vocabulary holds one word, `probe`. It grants nothing, and only a build
carrying the plane fence knows it: a test build, or the app's `e2e` build that the scenario tests
drive. A release build does not know it and refuses it like any unknown word.

**A capability with a shape comes as a pair with it.** A manifest that has `contributes.<word>`
for a capability it does not list is refused, and so is one that lists a capability with a shape
and declares nothing under its word. The first would be a contribution the prompt never named;
the second would be a yes to nothing.

**The vocabulary so far:**

| Word | What charter does | Its shape, under `contributes` | Change |
|---|---|---|---|
| `probe` | nothing (test builds only) | none | charter-app#338 |
| `badges` | draws values from the facts file as badges in the status bar and the terminal footer | `badges`: `id`, `label`, `surfaces` (`status-bar`, `footer`), `fresh_seconds` | charter-app#340 |
| `repo-columns` | draws values from the facts file as extra columns in the repo table | `repo-columns`: `id`, `title`, `fresh_seconds` | charter-app#340 |
| `events` | asks the program one question after each core action it hears has finished; a fork carries the folder it keeps in each workspace (protocol 2) | `events`: `hears` (`workspace-focused`, `workspace-created`, `workspace-forked`, `workspace-removed`, `handoff-created`, `session-started`, `plane-saved`), `workspace_folder` | charter-app#343 |
| `briefing` | adds text to every chat's first message, quoted as data under the extension's name (protocol 2) | `briefing`: `title` | charter-app#343 |

### Process life

**It stays one process per question.** The executor is unchanged: no long-running extension
process, no timers, no push. An event is delivered as a question of its own, after the core
action it reports has finished, with the normal deadline. A failed delivery becomes a note. It
never changes the core action's result. Hot paths never start a process: the status badges and
the repo cells read the facts file.

### The facts file

**The facts file is a JSON file in the extension's state directory, and charter reads it
without starting the extension.** It has a size cap, and each field has a declared freshness.
It supplies values only for fields the manifest declared. charter reports undeclared fields, and
they contribute nothing. The extension rewrites the file whenever it is asked a question or sent
an event. One reader in the core serves the status bar, the terminal footer and the repo table.

As built in charter-app#340: the file is `<state>/facts.json`, at most 64 KiB, and holds
`{"badges": {"<id>": {"value", "at"}}, "repo-columns": {"<id>": {"<repo>": {"value", "at"}}}}`,
where `at` is Unix seconds. A manifest that declares a badge or a column must name a state
directory. The reader (`extension::facts::gather`) asks the record, then the manifest, then the
project and workspace (ADR 0048), and only then re-takes the fingerprint and reads the file, so
an extension with nothing for a surface costs that surface no hash. A value older than its
`fresh_seconds` is drawn dimmed with its age.

### Write scope

**An extension declares plane-relative globs it writes to**, such as `workspaces/*/todos/`.
Each request carries the resolved absolute paths. After each question the core reads the plane's
git status, and it reports any write outside the declared paths. That is detection, not
confinement. ADR 0041 already ruled out a sandbox.

### Naming

**An extension's CLI commands run as `charter <extension-id> <command> …`**, and an extension's
id may never be a core command word. The registry refuses one that is. A core word that forwards
to an extension, such as `charter ws todo` once todos moves, is core code. Palette commands carry
the extension's name. "Plugin" alone always means a harness's plugin, never charter's own
extension (ADR 0041, `CONTEXT.md`).

### What no capability grants

No capability hands an extension a secret. No capability adds a permission, a hook or a
harness setting. ADR 0041's table still holds: the machine store, `reopen.json`, harness
profiles and vaults cannot be granted at any level.

### The build order

1. built-in extensions, bundled with the app and trusted through its signature
2. the facts file (badges and repo cells)
3. palette commands
4. row actions and plane writes
5. CLI commands
6. events
7. briefing sections
8. Windows, separately

### Nothing core-critical moves before Windows

**Extensions don't run on Windows yet** (ADR 0031: refuse rather than degrade). So nothing that
a plane's instructions or the session-start briefing depend on moves into an extension until the
Windows executor exists. Personas and vaults stay core permanently. Todos moves only after the
capabilities it needs exist, extensions run on Windows, and todos has had its own grill.

## Amended 2026-09-25: palette commands, actions and writes (charter-app#341)

The first three real capabilities, built together because each needs the others to be useful.
The approval prompt names each one and then lists what it declares, one line each.

- **`palette`** adds commands to the palette (`contributes.palette`: `id`, `title`, and exactly
  one of `view` or `action`, each one the same manifest declares). The palette names a command
  `<extension's name>: <title>`, so where it came from is on the row.
- **`actions`** puts the extension's own actions on the rows of its views
  (`contributes.actions`: `id`, `title`, `confirm`, and optional `deletes`). An answered row
  names the actions it offers by id. The executor refuses an answer whose row names one the
  manifest does not declare. The button's title and whether charter asks first come from the
  manifest, never from the answer.
- **`writes`** declares the plane paths the extension writes (`contributes.writes`:
  plane-relative globs, `*` and `?` within one segment). A path that starts with a pattern,
  names anything hidden, or covers a file charter reads settings, grants or vaults from
  (`charter.toml`, `charter.local.toml`, `vaults.json`, a workspace's `workspace.json`) is
  refused at parse.

**A capability's shape is under its own word, and is there exactly when the word is asked
for.** A manifest with `contributes.actions` but no `actions` in its list is refused, and so is
one that lists `actions` and declares none, empty list included. This is the pairing check
charter-app#340 built (`Capability::has_shape`), and every shaped capability keeps it.

**The protocol is 2**, because an action is a second kind of request: *run action `<id>` on
`<subject>`*, with the view and the row it was pressed on, and an answer that may carry the view's
blocks refreshed. Every protocol-2 request also carries `writes`, the declared paths resolved
against the plane. The two companion changes this record called for are made. The executor
asks each program in the protocol its manifest names and reads its answer in that protocol. The
fingerprint hashes the manifest's protocol, not the executor's. A protocol-1 extension such as
persona statistics is asked the same bytes as before and keeps its approval: a test pins its
fingerprint. `actions` and `writes` need protocol 2, so a manifest that names version 1 and asks
for either is refused. `palette` needs only 1.

**Confirmation is charter's, and a delete always asks.** An action says whether charter asks
first (`confirm`, required). It also says whether it deletes (`deletes`, default false). An
action that deletes is asked about whatever `confirm` says. The executor refuses to run an
action that asks first without the operator's yes, so a window that forgot to ask gets a
refusal, not a delete. **How charter knows an action deletes is that the manifest says so.** An
extension that deletes without saying so skipped the question. The write report catches that
case: a path deleted by any question except an action declared as deleting is reported, even
inside the declared paths.

**The write report is detection** (ADR 0041). Before the program starts and after it stops, the
executor lists what `git status` shows in the plane, with each path's size and modification
time. It reports a change outside the declared paths as a sentence naming the extension, beside
the answer or the refusal. It sees what git would commit. It does not see an ignored path, a
write that keeps size and time, or a plane that is not a git repository. It cannot tell the
extension's write from a chat's in the same moment, and the sentence says so.

## Considered options

- **Read-only extensions first.** Rejected by the operator: the extensions worth building, todos
  among them, need to write, act and brief.
- **Capabilities inferred from `contributes`.** Rejected. The list is what the operator reads
  and what a charter that lacks a capability refuses by name. An inferred list would load a
  manifest with a `contributes` key an older charter ignores, which is the half-load this record
  refuses.
- **A separate manifest version and protocol version.** Rejected. Both numbers change for the
  same reason, when a request or an answer gains a field, so two numbers would only drift apart.
