# A memory opens in a view tab, and delete in the window is archive

**Accepted 2026-09-28**, by the operator's rulings on SI-9 (smart IDE), grilled question by
question (Q1–Q14 below).

Memory is most of what a plane knows, and the window could not touch it. It listed one persona's
memories on that persona's card and showed a row's body as plain text in a popover; workspace
memory was not read by the window at all, the shared store was never shown, and nothing in the
window could write, change or remove a memory. The command line could remember, recall and forget
in all three stores — a workspace's journal, a persona's `memory/` and `personas/_shared/memory/` —
and could not edit a single one.

This PR (SI-9a) is the core, the command line and the documents. The window's side is SI-9b (the
memory tab) and SI-9c (the workspace and shared lists), and its rules are recorded here so they
have one place to build from.

## The decision

### A memory is a view tab (Q1–Q4, Q12)

**Q1. A memory opens in a preview tab**, the VS Code model: a single click on a row replaces the
one preview tab; a double-click, or starting an edit, pins it into an ordinary tab. It is a view
tab in ADR 0043's sense — a pane holding a view, not a chat — so every rule a tab already has
holds for it without a copy.

**Q2. The header carries what the file's first lines carry.** Title, a scope badge (the persona's
name, `shared`, or the workspace's name), the stamp, the file's path, and Edit and Delete buttons.
The body is rendered as Markdown (react-markdown, `skipHtml`, as the session record tab renders
one). The `# title` and `_stamp · kind_` lines are taken out of the body, because the header
already says them.

**Q3. Edit flips the same tab into an editor**: a title field capped at `TITLE_MAX` (72) and the
raw Markdown body in a textarea, with Save and Cancel. No second tab and no modal.

**Q4. The memory row's popover goes, and so does the row's native truncation title.** The tab
replaces both. **This supersedes ADR 0043 for memory rows**: its 2026-09-23 amendment kept "every
other row's card" a popover, because six short rows are what a popover is for; a memory is not a
short row once it can be read at length and edited, and a popover anchored in a 260 px column is
the thin surface ADR 0043 itself called it. Other rows keep their popover.

**Q12. A memory row's context menu** is Open, Edit, a separator, Delete. The tab's header has Edit
and Delete.

### Where the lists live (Q5, Q6, Q9–Q11)

**Q5. Workspace memory is a "Memory" section in the right column, directly under Todos**, for the
focused workspace, newest first, in the same row style and with the same search. It is hidden at
the plane root, which is in no workspace and has no journal (SI-1).

**Q6. Shared memory is one "shared" row in the Personas section** ("shared · N memories"), which
opens its own list as a view tab. `_shared` is not a persona, and a card of its own would say it
is one.

**Q9. Create is a `+` in each memory section's heading** — the workspace's Memory section, a
persona's tab, the shared list. It opens a new memory tab in edit mode, and Save writes through
the existing `remember` write (`memstore::write`), so a memory made in the window is exactly the
file `charter … remember` would have made.

**Q10. The lists refresh on the trigger Todos refreshes on.** A save is checked against the file
as it was when the tab opened it; when the file has changed since, the save is refused and the
tab offers **Reload** or **Overwrite**.

**Q11. Search matches a memory's body too**, not only its title, and while a search is typed a
short matching snippet is shown under the row.

### What the core does (Q7, Q8)

**Q7. Update is a new core operation: an edit in place.** New title and new body; **the filename
— the slug — does not change**, because it was minted from the first title and is how every
command names the memory, so a retitle renames nothing. The index line is retitled where it
stands. The original stamp line is kept verbatim, date and `kind` both: an edit does not re-date
a memory. The title is capped at `TITLE_MAX`. The caller hands back the file's text as it read
it, and a file that differs on disk is refused as **stale**, a refusal of its own the window turns
into Reload/Overwrite; an explicit overwrite skips the check. The check and the write run under
the store directory's `rewrite::Lock`, the lock `settings::save` takes for the same question.
The whole contract is `docs/plane-format.md` → *Editing and archiving a memory* — written
before the code, as every change to the plane is.

It is reached three ways, by one function each (`memstore::edit`, through
`Workspace::edit_memory` and `Persona::edit_memory`): the window (SI-9b's Tauri command), and the
command line for parity — `charter workspace edit <slug>` and `charter persona edit-memory <name>
<slug> [--shared]`, each taking `--title` and the body as an argument or, as `-`, from standard
input.

**Q8. Delete in the window is archive, with an Undo.** The file moves to `<store>/archive/` and
its index line is dropped — what `memstore.archive` has always done for `optimize`'s duplicate
collapse — with no confirmation and an Undo toast for a few seconds. Undo moves it back and
appends its index line. **A hard delete stays `forget`, and only on the command line.** An
archived memory is still committed, still out of every listing, and one `unarchive` from being
back; a deleted one is in git history only.

Archive and unarchive are core operations too (`memstore::archive_one`, `memstore::unarchive`,
through `archive_memory` and `unarchive_memory` on `Workspace` and `Persona`), with command-line
verbs for parity: `charter workspace archive|unarchive <slug>` and `charter persona
archive-memory|unarchive-memory <name> <slug> [--shared]`. Both are safe to repeat: archiving a
memory already in `archive/` and unarchiving one already back change nothing and succeed, and a
slug in neither place is an error that names it. An archive that has to number the file (a
memory of that name was archived before) returns the numbered path, and unarchive takes the name
to restore it as — which is how Undo puts it back under its own slug. A restore onto a name the
store already holds is refused and nothing moves.

### What is out (Q13, Q14)

**Q13. Moving a memory between scopes** — workspace to persona, persona to shared — is out of
this build and a follow-up todo. **Q14. Browsing and restoring the archive** is out too, and a
follow-up todo; until it lands, `unarchive` on the command line restores any archived memory by
name.

## Why these, and not the alternatives

- **In place, not delete-and-rewrite.** A remember-then-forget "edit" would mint a new slug from
  the new title (and, in a workspace, a new timestamp), so every reference to the old name —
  `ws todo done <slug>`, a link in `workspace.md`, a chat's memory of it — would break on a typo
  fix, and the memory would be re-dated to the moment it was corrected.
- **The file's text as the stale token, not a modification time.** It is what `settings::save`
  compares, so the two save paths in the core that refuse a changed file refuse by the same
  rule, and it cannot be fooled by a clock or a filesystem that rounds times.
- **Archive, not delete, behind a button with no confirmation.** The operator's rule is that
  work is not discarded by a click; an Undo that is a move back is honest, a "confirm delete"
  dialog trains people to click through it.

## Consequences

- **The command line gains six verbs**, one edit, archive and unarchive per scope. `persona`
  spells them `edit-memory`, `archive-memory`, `unarchive-memory` because a bare `persona edit`
  reads as editing the persona's definition; `workspace` spells them `edit`, `archive`,
  `unarchive`, as it spells `forget`.
- **`memstore::archive` is `memstore::archive_one` answered as an `Option`**, so `optimize`'s
  collapse and the window's Delete move a file by one rule.
- **A memory's body, to every reader, now starts after its heading when it has no stamp line**
  (`workspaces::parse_entry`): a memory written by hand with a heading and no stamp no longer
  shows its heading twice, and an edit of one does not write it twice.
- **An index line after its link keeps what follows the link** on a retitle. A line a hand wrote
  as `- [Old](x.md) — see also y` becomes `- [New](x.md) — see also y`.
- **Nothing in the Python charter reads an edit or an unarchive differently**: the files are
  shapes it already writes and reads, so the recorded behaviour (ADR 0046) moves nowhere.

## As built in the window (SI-9b)

What building the tab settled, inside the rulings above rather than beside them:

- **The preview is one per strip.** A single click replaces the strip's own preview tab — a tab
  showing one view and nothing else, marked `preview` — and never a kept tab, a tab split beside
  a chat, or another workspace's preview (`tabs.openPreview`). A memory already open anywhere is
  brought forward instead. The preview is drawn in italics.
- **A memory row in a persona's tab keeps on the tab, not on the row.** Its first click brings
  the memory's tab forward, so the persona's list is off the screen before a second click can
  land; a double-click on the preview tab keeps it (`tab.keep:<id>`, also in the palette), as
  VS Code's does. A row's own double-click keeps too (`actions.toKeep`), which is what a list
  that stays on screen — SI-9c's, in the side region — uses. `tab.keep` is not on the tab's
  menu, which is held to five rows a tab at fifty tabs (charter-app#174).
- **A row that runs a catalogue row opens no card, and carries no native tooltip**, whichever
  list draws it: a persona's row and a session's already opened a tab and had no card, and a
  memory's now does too. Its detail, where it has one, is what the search reads. A row with a
  card and no verb — a todo, an extension's — keeps its popover and its tooltip.
- **Every list's search matches the body**, not only a memory list's, and shows the matching
  part under a row whose own words do not show the match.
- **A memory is addressed as a store and a slug**, `workspace/<ws>/<slug>`, `persona/<name>/<slug>`
  or `shared/<slug>`: the tab's view key and the catalogue rows' suffix
  (`memory.open|edit|delete:<key>`), spelled by `memories::view_key` in the core and
  `memories.memoryKey` in the window. A new memory's tab is the slug `+`, which no slug can be.
- **An edit in progress outlives its tab being out of sight**: only the tab in front has panes
  on screen, so the draft is kept outside it, for as long as the window runs.
- **Undo is offered for eight seconds**, in a line where the window says its other news, and
  puts the memory back under its own slug even when archiving had to number it.
