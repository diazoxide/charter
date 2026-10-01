# A review is a comparison git computes, and its draft is the operator's until it is sent or published

**Proposed 2026-10-01**, drafted for program-map ticket RC-1 (#703). It follows these of the
operator's rulings:

- **U3:** *"A review surface: a diff per piece in a view tab, inline comments back to the chat,
  ending in save or PR/MR. v1 is pre-PR for own agents; teammates come via Live sessions. Forge
  review stays the review of record."*
- **R1:** *"One diff engine; compare (a) chat branch vs base, (b) any two refs, (c) uncommitted,
  (d) a single agent turn (N3), (e) a cross-repo change, (f) a GitHub PR / GitLab MR. Entry
  points: a Changes button on every chat tab, the Explorer branch, a "Compare…" palette entry,
  pasting a PR/MR link."*
- **R2:** *"A Review view tab per branch or comparison, splittable beside its chat; preview-tab
  semantics, kept once a review starts."*
- **R3:** *"GitHub-level basics in v1: file tree with A/M/D/R markers, unified/split toggle,
  syntax + word-level highlighting, hide whitespace, "Viewed" ticks, collapse generated and
  lockfiles, binary/image previews, keyboard navigation, virtualised rendering (1,000-file diff
  first paint under 1 s)."*
- **R4:** *"Diff computed by git in the Rust core; rendered with CodeMirror 6 + its merge view,
  the same component as the light editor (one editor stack, TS only)."*
- **R5:** *"Line/range comments go into a local review draft (in .charter/, never the plane);
  then Send to agent (one prompt with file:line refs), Publish to PR/MR (GitHub and GitLab via
  the forge extension), or Keep."*
- **R6:** *"Approve → the chosen next step (save, PR/MR, land via queue/merge train); Request
  changes = Send to agent; "Changes since my last review"; review position remembered per
  branch."*
- **R7:** *"The right side of the diff is editable while the chat is idle (warning otherwise);
  save uses the stale-check with Reload/Overwrite; the edit is recorded as a human edit in the
  chat's history."*
- **R8:** *"The agent's next turn starts with a structured note of human edits and review
  comments (hook where supported, MCP otherwise)."*
- **R9:** *"Race-mode comparison: N candidates side by side, keep one, pull hunks across, with
  test/eval scores."*
- **R10:** *"A full forge review client as the second step: open a PR/MR, see and reply to
  threads, resolve, "fix this" to an agent. GitHub and GitLab."*
- **R11**, in part: *"reviewing together live rides on Live sessions"*.
- **R12:** *"Non-goals kept (X26): no LSP, debugger, refactors or editor plugins; deep work opens
  in the user's editor at the same file:line via the editor integrations."*
- **X26:** *"F2 carries a non-goals list (no LSP, no debugger, no project-wide refactors, no
  editor extension API, no terminal multiplexing beyond chats and shells). Public positioning is
  **"the agent IDE"**: the environment where your agents work, not a code editor."*
- **W7**, in part: the Next gate *"names **RC-lite** only: RC-1, RC-2, RC-4, RC-5, RC-6 (without
  the 1,000-file budget), RC-7 and a URL-scheme "open in editor at line""*.
- **OQ-10:** *"`.charter/` holds only derived, rebuildable indexes. Review drafts (R5) and
  private memory (KN-7) move to the app data dir."* **V22b**, in part: *"review drafts and
  private memory are Machine and syncable"*.
- **V23d:** *"\"Piece\" leaves the window. The piece ↔ branch wording is ADR 0072 §4 as
  drafted."*
- **V11**, in part: *"queue and steer while a turn runs"*. The map row HP-15 adds: *"RC-7 and
  RC-11 route through it (V11)"*.
- **FI3:** *"The human's UI token never reaches an agent; agents keep their own narrow identity
  (SD-7a/b, gap G3), and gain no new power from this feature before those land."*
- **FI12:** *"RC-17a/b (full PR/MR client) move from Later into this lane, on the same client and
  sign-in."*
- **U4:** *"Reviewer persona reading a repo-owned REVIEW.md, posting on the PR/MR via gh/glab
  under the operator's identity, moving to a per-agent forge identity (GitHub App / GitLab token)
  with SD-7."*
- **Q13**, in part: *"PRs stay central."*
- **N61:** *"git GUI basics (history, blame, hunks)"*. **N3**, in part: *"cross-harness
  checkpoints/rollback via a git ref per turn"*.
- **X22:** *"Every new ruling must name which concept it belongs to."*

It builds on [ADR 0060](0060-a-cross-repo-change-is-a-record-of-intent-and-charter-lands-it-one-member-at-a-time.md)
(a cross-repo change),
[ADR 0061](0061-a-curation-action-is-a-chat-with-its-prompt-typed-and-never-sent.md) (typing a
prompt), [ADR 0069](0069-every-store-charter-writes-is-in-one-of-four-tiers.md) (tiers),
[ADR 0070](0070-a-forge-is-one-seam-with-a-native-client-per-forge-and-gh-and-glab-are-its-fallback.md)
(the forge seam),
[ADR 0072](0072-charter-has-five-concepts-and-every-other-word-belongs-to-one-of-them.md) (the
five concepts, the first-hour words, a piece shown as its branch),
[ADR 0075](0075-an-audit-entry-is-metadata-in-a-store-of-its-own-and-telemetry-never-reads-it.md)
(the audit), [ADR 0076](0076-a-run-moves-only-by-a-named-cause-and-a-chats-state-is-read-from-its-runs.md)
(run states), [ADR 0077](0077-a-forge-sign-in-is-the-humans-through-one-registration-per-forge-host-and-it-mints-nothing-for-an-agent.md)
(the human's forge sign-in) and
[ADR 0081](0081-charter-is-where-agents-work-and-writing-code-by-hand-opens-in-your-editor.md)
(the light editor, your editor, the editor protocol and X26's lines). It **amends ADR 0070 and
ADR 0075**, each in a section of its own below. RC-2 to RC-16, RC-18 to RC-20 and FW-16a/b build
on it. Its concept is **Workspace**: a review is a view of a workspace's repos and its chats'
branches, as the light editor is (ADR 0081).

## Where charter is today

charter has no diff view, no review and no editor (ADR 0081, *Where charter is today*). What it
has near review:

- **The commit scan reads a diff**, of the lines a chat's commit adds, from inside git's
  `pre-commit` hook (`crates/charter-core/src/diffscan.rs`, ADR 0074). It is a scan, not a view,
  and reads only the index being committed.
- **A cross-repo change** is recorded intent (`workspaces/<ws>/changes/<slug>.json`, ADR 0060):
  which repos, which branch in each, which must land first. Its state is read from git and the
  forge (`crates/charter-core/src/change/`).
- **The forge seam is decided and not built.** ADR 0070 reserves `pub trait Reviews { /* FG-5a/b,
  FW-16a/b */ }` and nothing implements it. Today's forge code reads a request's state and its
  checks (`crates/charter-core/src/forge/pr.rs`, `checks.rs`).
- **charter runs git as a program** (`std::process::Command::new("git")` throughout
  `charter-core`), with no git library in any crate.

## What the rulings already settle

| RC-1 asks | Settled by |
|---|---|
| One diff engine, git in the Rust core, and its six kinds of comparison | R1 and R4, quoted above |
| One editor stack: CodeMirror 6 and its merge view, the light editor's component, TypeScript only | R4; ADR 0081 §1 and its *What changes where* row for RC-1 and RC-5 |
| The Review tab, splittable beside its chat, a preview until a review starts | R2 |
| The v1 basics, and that the 1,000-file budget is checked after RC-lite | R3 and W7 |
| Where a draft lives | OQ-10 and V22b: the Machine tier, syncable. ADR 0069 §4 already gave way R5's *"in .charter/"* to it, and its row 71 holds it |
| Send, Publish and Keep; Approve and Request changes; changes since my last review | R5 and R6 |
| Editing the right side while the chat is idle, and telling the agent | R7 and R8 |
| A message for a busy chat waits for its turn to end | V11 and HP-15's row: RC-7 and RC-11 route through HP-15 |
| The full forge review client | R10 and FI12: it is FW-16a/b, on FW's client and sign-in |
| Who a published review is from | FI3 and ADR 0070 §4: the operator's sign-in, from a human client scope only; a chat posts under its own credential (U4) |
| Reviewing together | R11 and U3: through Live sessions (RC-18), outside this record |
| The non-goals | X26 and R12. ADR 0081 §2 draws each line, and this record adds none |
| "Piece" in the window | V23d: a review is of a **branch** (ADR 0072 §4) |

What is left is **what a comparison is**, **how git computes it safely**, **the draft and what
it may hold**, **how Send, Publish and Approve reach the chat and the forge**, **what idle
means for an edit**, **the note an agent gets**, **the audit and the tiers**, and **the words**.

## The decision

**A review is a comparison, a base and a head in one repo (or one per member repo for a
cross-repo change), and git computes every comparison, in `charter-core`, with no driver or
program from the repo under review. The Review tab draws it with the light editor's component.
The operator's comments go into a review draft in charter's data home, which only the window
writes and chats can neither read nor write. A draft leaves only by the operator's act: Send to
agent delivers it to the chat as one prompt, at a turn boundary; Publish posts it to the request
as one review under the operator's own sign-in; Keep keeps it. An edit made beside an idle chat
is a human edit, recorded for the chat and told to its agent at its next turn.**

### 1. A comparison

**A comparison is a base and a head in one repo of a workspace.** Each of R1's kinds names its
two sides:

| Kind (R1) | Base | Head | Ticket |
|---|---|---|---|
| (a) a chat's branch | the merge base of the branch and its base branch (the repo's default branch, or the branch it was cut from) | the branch's tip | RC-2 |
| (b) any two refs | the merge base of the two, or the first ref itself when the operator chooses *exact* | the second ref | RC-2 |
| (c) uncommitted | `HEAD` of the branch folder | the working tree, untracked files that git does not ignore included | RC-2 |
| (d) one agent turn | the turn's start checkpoint (N3) | its end checkpoint | RC-12 (Later) |
| (e) a cross-repo change | one comparison per member, as (a), in the order its `needs` give (ADR 0060) | | RC-2 |
| (f) a request | the merge base of the request's target branch and its head | the request's head, fetched (§2) | RC-2, FW-16a/b |

- **The merge base, not the base branch's tip, for (a), (b) and (f).** It shows what the branch
  changed and not what landed on the base since, which is what both forges show for a request.
- **(c) never touches the index.** Untracked files are diffed as added, from the working tree,
  and nothing is staged. Staging is hunk staging's (RC-16), the operator's act.
- **A comparison is always in a workspace's repo.** A pasted request link for a repo no
  workspace holds offers to add the repo to a workspace first, and opens the review once it is
  cloned. The review needs the repo's objects, and a workspace is where charter keeps repos.
- **A comparison of a branch is live.** While the Review tab is open, a move of the head (a
  chat's commit, an edit in the folder for (c)) is offered as *Updated: show the new changes*,
  and never redrawn under the operator's cursor. FD-11's per-repo watcher says when.

### 2. git computes it, and nothing from the repo under review runs

**The engine is a module of `charter-core`** (RC-2) that runs git as a program, as the rest of
`charter-core` does. The window calls it for the Review tab, and `charterd` calls it for the
editor protocol's `changes` (ADR 0081 §4). There is one implementation and no second diff in
TypeScript.

- **In two steps, so a large comparison paints early.** First the file list: each path with its
  A/M/D/R mark (renames detected, as both forges do), its added and removed counts, and whether
  git calls it binary. Then each file's hunks, when its row is drawn. R3's virtualised rendering
  and its 1,000-file budget rest on the first step being one git call.
- **CodeMirror's merge view draws git's hunks.** Which lines changed is git's answer. The merge
  view's own diffing is used only inside a hunk git reported, for R3's word-level highlighting.
  RC-2's fixtures check each kind against `git diff` itself.
- **Hide whitespace is git's** (`-w`), recomputed, not filtered in the window.
- **Generated and lock files** are collapsed when the repo's `.gitattributes` marks them
  `linguist-generated`, or their name is on charter's list of lock files. Collapsed is drawn on
  request.
- **Nothing from the repo under review runs.** Every call disables external diff programs and
  text conversion filters, whatever the repo's or the operator's git configuration says. A
  request's head is someone else's content, and so is its configuration. A file's contents are
  drawn as text and never as markup; an image is drawn as an image, and an SVG is never put into
  the window's page as markup. This is class-level: content under review is data, never code.
- **A request's head is fetched into the workspace's clone, under `refs/charter/review/`.** The
  objects must be local for git to diff them. The fetch uses the clone's own git credentials,
  which ADR 0070 §4 keeps outside the seam. The refs are removed when no Review tab and no draft
  needs them.

### 3. The Review tab

R2's Review tab is a view tab. It holds one comparison, or one per member for a cross-repo
change, each a section in `needs` order.

- **A review starts** at the first comment, the first *Viewed* tick or the first edit. Until
  then the tab is a preview, and the next comparison opened replaces it. A started review's tab
  is kept (RC-4's acceptance).
- **One review per branch.** Opening the comparison of a branch that already has a review opens
  that review, with its draft and its ticks.
- **A *Viewed* tick is for a file at a version.** When the head changes a ticked file, its tick
  clears, as both forges' do.
- **Every line has *Open in your editor*** at that file and line (RC-20, ADR 0081 §3).
- **What it does not do** is X26's, drawn by ADR 0081 §2: no language server in a diff, no
  refactor across files, no code from an extension inside it. An extension's review decorations
  are data charter draws (E1, E6, ADR 0081 §2).

### 4. The review draft

**A review draft is the operator's unsent comments on one review**, with that review's state:

- each comment: its path, its side (head, or base for a removed line), its line or range, the
  commit it was written against, and the operator's text;
- an overall comment, if any;
- the *Viewed* ticks, each with the version of the file it was given for;
- **the last reviewed head**: the head at the operator's last Send, Publish or Approve (§7);
- where the operator was: the file and line, so a review reopens there (R6).

**A draft holds no code.** A comment points at a line by its path, line and commit; the lines
are read from git when drawn. A draft is text the operator typed and pointers into git.

**A draft names its review by project, workspace, repo and branch**, never by an absolute path,
so it means the same thing on another machine (V22b's *syncable*).

**A comment follows its line.** When the head moves, each comment's line is mapped through git's
diff from the commit it was written against to the new head. A comment whose line the new head
changed or removed is marked *outdated*, kept and shown at its old place, as both forges show
one.

**Only the window writes a draft, and chats are denied the store, reading and writing.** A
draft is published under the operator's identity (§6), and what an agent could write there, it
could post as the operator. A reviewer persona (U4, GL-12) is a chat: its findings are a review
of its own, shown beside the operator's under the persona's name, and the operator copies a
finding into their draft or does not.

### 5. Send to agent

**Send to agent delivers the draft to the chat whose branch it reviews, as one prompt.** The
prompt names the review (*Review of `<branch>` in `<repo>` at `<short commit>`*), then each
comment as `path:line` (or `path:line-line`, and *base* for a removed line) followed by the
operator's text, in file order, then the overall comment. It carries no code; the agent reads
the files.

When it is delivered depends on the chat's current run (ADR 0076 §1):

| The run is | Send to agent |
|---|---|
| `input-required (ready)` or `(turn-ended)` | delivers now: `charterd` checks the state and delivers the prompt as one step under the run's state lock, as ADR 0081 §4 checks `place` |
| `working` | holds it and delivers it when the turn ends, with *Take back* while held: HP-15's queue (V11). Until HP-15 lands, Send is disabled, saying *the chat is working; send when its turn ends* |
| `input-required (asked)` | refused: a prompt delivered then could be read as the ask's answer |
| `hibernated` | wakes the chat as focus would (ADR 0076 §5, SC-20), then delivers at `ready` |
| `paused`, `queued`, `starting` | refused, naming the state |
| ended | offers *Start a chat on this branch* with the review typed into its prompt and never sent (ADR 0061) |

- **It is the operator's act, from the window.** Delivery is a call on the window's own
  connection (`local-ui`), never a session-protocol command and never one the `editor` scope has.
  Unlike ADR 0081's `place`, it does send: the operator pressed *Send*.
- **At level 2 it is HP-15's delivery**: one bracketed paste, control characters taken out, then
  Enter, under the state lock. At level 3 the host delivers the prompt through the protocol.
  TS1's per-harness measure from ADR 0081 §4 covers it.
- **Request changes is Send to agent** (R6). Once sent, the comments move from the draft to the
  review's list of sent comments, marked with the run they went to.
- **A review past 64 KiB of text is refused**, asking the operator to send it in parts, since
  the prompt carries the operator's own words and no code.

### 6. Publish to the request

**Publish posts the draft to the branch's request as one review, through the forge seam's
`Reviews` area, with a human `Caller`** (ADR 0070 §4), so under the operator's own sign-in
(ADR 0077) and never a chat's. See *ADR 0070, amended*.

- **One act on both forges.** On GitHub, one pull request review with its line comments,
  submitted at once (RC-8a). On GitLab, one draft note per comment, then a bulk publish, so the
  comments appear together (RC-8b). Where a host lacks a forge capability this needs, the
  capability card says so (FG-16), and the fallback is one discussion per comment.
- **A comment is placed against the request's head.** A comment written against an older commit
  is mapped to the request's head first (§4). One that cannot be mapped is not posted; the
  window lists it, and the operator posts it as part of the overall comment or drops it.
- **No request, no Publish.** A branch without one is offered GL-3a/b's *push and open a
  request* first.
- **Published comments leave the draft.** The forge holds them from then on; the forge's review
  is the review of record (U3). Reading and answering its threads in charter is FW-16a/b's.
- **Every publish is audited** as ADR 0070 §6's `forge.write`.

### 7. Approve, and changes since my last review

**Approve runs the next step the operator chooses** (R6): **Save** (the branch's repo's save
mode), **Open request** (GL-3a/b), or **Land** (GL-4a/b: the merge queue or merge train where
the repo has one). With a draft that is not empty, Approve offers to Publish it first. A forge
approval is posted only when the operator chooses *Publish and approve*; a forge that refuses
it (an account may not approve its own request) says so and the next step does not run.

**Changes since my last review** compares the last reviewed head (§4) with the current head.
When the last reviewed head is an ancestor of the current one, it is a comparison of kind (b)
between the two. When the branch was rewritten, it compares the two heads' trees and says
*The branch was rewritten; this shows how its files differ from what you last reviewed.*

### 8. Editing beside a chat

R7's edit is the light editor's, in a review or in a file opened from a chat's branch
(RC-5, RC-10). **Idle** means the runs of every chat working in that folder are
`input-required (ready)`, `input-required (turn-ended)`, `hibernated` or ended (ADR 0076 §1).
A folder in a repo's shared clone is idle when every chat working there is. Otherwise the editor
warns, naming the chat and its state, and saving is still the operator's choice.

- **The stale check** compares the file on disk with what the editor opened. If it changed, the
  save offers *Reload* or *Overwrite*, and never merges.
- **A saved edit is a human edit.** charter records, as an event in the event log (FD-9): the
  chat, its run, the path relative to the repo, the changed line ranges, and whether the chat was
  busy. No contents. The edit is left in the folder uncommitted; charter does not commit it.
- **The agent is told at its next turn** (R8, RC-11): a note listing the human edits since its
  last turn, path and line ranges, and that a review was sent, if one was. It carries no file
  contents. It is added at the next prompt, by the harness's prompt hook where it has one and
  through charter's MCP server (HP-7) otherwise.

### 9. Race-mode comparison

RC-13 (Later) compares N candidates, each a chat's branch from the same base, side by side.
**Keep one** lands that branch's next step (§7) and removes the others' folders, keeping their
branches (ADR 0072 §4's *Remove folder*). **Pulling a hunk across** applies it in the kept
branch's folder, which is a human edit (§8). This record fixes only that a race is N comparisons
of kind (a) against one base.

## ADR 0070, amended

**§1, the `Reviews` area.** Its comment gains RC-8a/b: `pub trait Reviews { /* RC-8a/b, FG-5a/b,
FW-16a/b */ }`. Its first method, added with RC-8a and RC-8b, publishes a draft as one review
with its line comments, each anchored to the request's head commit, on both forges (§6 of this
record). It takes a human `Caller` for the operator's review and a chat `Caller` for a reviewer
persona's (FG-5a/b), and §4's rules for each `Caller` are unchanged. GitLab's draft notes and
bulk publish are a forge capability, with one discussion per comment as its fallback.

The rest of ADR 0070 stands.

## ADR 0075, amended

**§4, the registry.** It gains three actions:

| Action | From | `meta` |
|---|---|---|
| `review.sent` | ADR 0084 §5 | the chat, the number of comments, the head commit, whether it was held for the turn's end |
| `review.approved` | ADR 0084 §7 | the repo, the branch, the next step (`save`, `request`, `land`), whether it was published |
| `review.edit.saved` | ADR 0084 §8 | the chat, the path relative to the repo, whether the chat was busy, whether the save overwrote a changed file |

None carries a comment's text, a prompt or a diff, which §4 never allows. Publishing is
`forge.write` and gains no action of its own.

The rest of ADR 0075 stands.

## The audit and the events

- **Audit** (ADR 0075): the three actions above, and `forge.write` for a publish.
- **Events** (FD-9): the human edit (§8), which RC-11's note reads. An event is not an audit
  entry, and telemetry never reads the audit (ADR 0075).

## Tiers

| Store | Tier | Why |
|---|---|---|
| `<data>/reviews/`: one file per review, holding its draft, ticks, last reviewed head, position and sent comments | **Machine, syncable**, backed up by FR-10 | V22b and ADR 0069 row 71. It names reviews by project, workspace, repo and branch and holds no path or code (§4). It is in charter's data home, beside the audit, because it is the operator's data and not a preference. Chats are denied it. **Added to `docs/plane-format.md` in this PR** |
| `refs/charter/review/` in a workspace's clone | **None** | the repo's own git, as its worktrees are (ADR 0069 row 72); removed when nothing needs it (§2) |
| the human-edit event | **Machine, device-bound** | the event log (ADR 0069 row 63) |
| the three audit actions | **Machine, device-bound** | the audit store (ADR 0075) |
| the stale check's copy, the file list, hunks, RC-11's note | **None** | in memory for one view or one turn |
| a published review | **None** | on the forge |
| which editor RC-20 opens | **Machine, syncable** | ST1's registry, ADR 0081 §6 |

## What changes where

The code does not change with this record.

| Where | What changes |
|---|---|
| `charter-core` | RC-2: the comparison types and the engine, git with external diff and text conversion off; the fetch under `refs/charter/review/`. RC-7: the review store, written through one module the window calls |
| `charterd` | RC-7: Send to agent's check and delivery under the run's state lock, on `local-ui`; later HP-15's queue. The sandbox denies chats `<data>/reviews/` |
| The window | RC-4: the Review tab. RC-5: one CodeMirror 6 component for files and diffs. RC-6: R3's basics. RC-10: the edit and its warning. RC-3: the entry points |
| The forge seam | RC-8a/b: the `Reviews` area's first method (*ADR 0070, amended*) |
| Hooks and MCP | RC-11: the next-turn note |
| Audit | AU-2 registers the three actions (*ADR 0075, amended*) |
| `docs/plane-format.md` | The `<data>/reviews/` row (in this PR) |
| `CONTEXT.md` | Gains **Review**, **Comparison**, **Review draft** and **Human edit**, each with its concept (in this PR) |

## What this costs

- **A request for a repo outside every workspace needs the repo added first.** One step more
  than pasting a link into a forge's own page.
- **Send to agent waits for a turn to end**, and until HP-15 it is disabled while the chat
  works.
- **A comment that cannot follow its line is not published.** The operator decides about each
  one.
- **A repo's own diff drivers never apply.** A file type the repo diffs through a text
  conversion shows as git's plain diff or as binary.
- **Fetched request refs sit in the workspace's clone** while a review needs them.

## What was rejected

- **A git library (`git2`, `gix`) for the engine.** charter runs git as a program everywhere
  else, and RC-2's acceptance is *matches `git diff`*; the program is the reference.
- **Computing the diff in the window.** R4 puts it in the Rust core, and two engines would
  disagree.
- **Diffing against the base branch's tip.** It shows what landed on the base as if the branch
  had reverted it.
- **Drafts in `.charter/`** (R5's words). ADR 0069 §4 already moved them to the Machine tier.
- **Letting a reviewer persona write into the operator's draft.** The draft is published under
  the operator's identity.
- **Copying the commented lines into the draft.** It would put code from a private repo in a
  syncable store, and git already has the lines.
- **Typing the review into the prompt unsent**, as ADR 0081's `place` does. The operator pressed
  *Send*, in charter's window.
- **Delivering into an `asked` run.** The prompt could answer the ask.
- **Committing a human edit for the operator.** Saving or committing is the operator's or the
  agent's, and the agent is told about it.
- **Publishing a GitLab review as separate discussions by default.** Draft notes publish
  together, as GitHub's review does.

## Decided in drafting

Each of these is inside the rulings above and easy to change later, so it is decided here, with
its reason:

1. **A comparison is a base and a head in one workspace repo**, one per member for a change. R1's
   six kinds fit it, and the editor protocol's `changes` (ADR 0081 §4) already returns per-branch
   base and head refs.
2. **The merge base is the base for (a), (b) and (f)**, with *exact* on offer for (b). It matches
   both forges.
3. **A pasted request for a repo no workspace holds offers to add it.** The review needs the
   objects locally.
4. **git with external diff programs and text conversion off, on every call.** Content under
   review is someone else's.
5. **Request heads are fetched under `refs/charter/review/`**, tier None, removed when unneeded.
6. **The merge view draws git's hunks** and diffs only inside them for word-level highlighting.
   It keeps R4's one engine.
7. **A review starts at the first comment, tick or edit.** R2 needs a moment for *kept*.
8. **A draft holds pointers and the operator's words, never code**, and names its review without
   a path, so it is syncable.
9. **The review store is `<data>/reviews/`**, in charter's data home. ADR 0069 says Machine;
   `<data>` holds the operator's data, and `<config>` its preferences.
10. **Only the window writes a draft, and chats are denied it.** A draft goes out under the
    operator's identity.
11. **A comment follows its line, and one that cannot is *outdated*** and never published by
    itself.
12. **Send to agent is refused in an `asked` run, held in a `working` one (through HP-15), and
    otherwise delivered under the run's state lock.**
13. **A review over 64 KiB is refused**, to be sent in parts.
14. **Publish is one act on both forges** (GitHub's review; GitLab's draft notes and bulk
    publish), with one discussion per comment as the fallback.
15. **Approve's forge approval is posted only on *Publish and approve*.**
16. **Changes since my last review** use the head at the last Send, Publish or Approve, and
    compare trees when the branch was rewritten.
17. **Idle for an edit** is ready, turn-ended, hibernated or ended, for every chat in the folder.
18. **A human edit is an event, and its save is an audit action**, both without contents; the
    edit stays uncommitted.
19. **Three audit actions**, `review.sent`, `review.approved`, `review.edit.saved`.

## For the operator's ruling

1. **The chat tab's button is *Review*, not *Changes*.** R1 names *"a Changes button on every
   chat tab"*. A chat's tab and header are first-hour surfaces, and ADR 0072 §3 (V23) lists
   **change** among the words those surfaces never say, because a cross-repo change is a charter
   noun; FR-3's UI-string test would fail the button. **Recommended: *Review***, an ordinary
   verb, with the tooltip *Review what this chat changed on its branch*. It names what the
   button opens, the Review tab (R2). The rejected options: *Changes* (fails the budget, or
   needs an exception to it), *Diff* (a developer's word, and the tab is more than a diff), and
   *What changed* (passes the test only if it matches whole words, and reads as a question). The
   Explorer branch and the *Compare…* palette entry keep R1's words; neither is a first-hour
   surface that says *change*.

## Later decisions

- **How long sent comments stay in a review's file.** Until the branch's folder is removed is
  the starting point; RC-7 may cap it.
- **Live reviewing together** (RC-18) is Live sessions' (R11).
- **The forge review client's threads** in the Review tab, and *fix this*, are FW-16a/b's
  (FI12), on this record's comparison and tab.
- **Per-turn comparison** (RC-12) waits on MH-6's checkpoint refs.
