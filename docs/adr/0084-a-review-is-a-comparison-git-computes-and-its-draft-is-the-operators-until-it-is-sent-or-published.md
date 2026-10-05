# A review is a comparison git computes, and its draft is the operator's until it is sent or published

**Accepted 2026-10-01** by the operator (rulings V34a, V34b), drafted for program-map ticket RC-1 (#703). It follows these of the
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
- **V34a:** *"The chat-tab button is **Review**. From it the operator reviews, edits by hand, or asks the AI for changes. **A hand edit the operator saves is announced to that chat's harness at its next turn**: the briefing lists the files changed since its last turn, so no prompt is sent on the operator's behalf."*
- **V34b:** *"Send to agent comes from the window (`local-ui`) only, never from the `editor` scope, which stays "typed, never sent". It presses Enter only on harnesses that pass TS1's paste+Enter test. This amends V31a for this one action."*

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
(the light editor, your editor, the editor protocol and X26's lines). It **amends ADR 0061, ADR 0069, ADR 0070, ADR 0075 and ADR 0081**, each in a section of its
own below. RC-2 to RC-16, RC-18 to RC-20 and FW-16a/b build
on it. Its concept is **Workspace**: a review is a view of a workspace's repos and its chats'
branches, as the light editor is (ADR 0081).

## Where purlis is today

purlis has no diff view, no review and no editor (ADR 0081, *Where purlis is today*). What it
has near review:

- **The commit scan reads a diff**, of the lines a chat's commit adds, from inside git's
  `pre-commit` hook (`crates/purlis-core/src/diffscan.rs`, ADR 0074). It is a scan, not a view,
  and reads only the index being committed.
- **A cross-repo change** is recorded intent (`workspaces/<ws>/changes/<slug>.json`, ADR 0060):
  which repos, which branch in each, which must land first. Its state is read from git and the
  forge (`crates/purlis-core/src/change/`).
- **The forge seam is decided and not built.** ADR 0070 reserves `pub trait Reviews { /* FG-5a/b,
  FW-16a/b */ }` and nothing implements it. Today's forge code reads a request's state and its
  checks (`crates/purlis-core/src/forge/pr.rs`, `checks.rs`).
- **purlis runs git as a program** (`std::process::Command::new("git")` throughout
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
The operator's comments go into a review draft in purlis's data home, which only the window
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
  cloned. The review needs the repo's objects, and a workspace is where purlis keeps repos.
- **A comparison of a branch is live.** While the Review tab is open, a move of the head (a
  chat's commit, an edit in the folder for (c)) is offered as *Updated: show the new changes*,
  and never redrawn under the operator's cursor. FD-11's per-repo watcher says when.

### 2. git computes it, and nothing from the repo under review runs

**The engine is a module of `charter-core`** (RC-2) that runs git as a program, as the rest of
`charter-core` does. The window calls it for the Review tab, and `purlisd` calls it for the
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
  `linguist-generated`, or their name is on purlis's list of lock files. Collapsed is drawn on
  request.
- **git run by purlis on a repo an agent can write must not execute programs named by that
  repo's config.** Every repo a review reads is one an agent can write: a chat's branch folder,
  the workspace's clone it shares, or a request's head from someone else. A repo's configuration
  can name programs for git to run (for diffing, filtering, paging, watching the file system,
  hooks, fetching and its transport), so every git call the engine makes runs with **one fixed
  set of overrides that turns each such program off**, whatever the repo's configuration says,
  held in one place in `charter-core` and never assembled per call. Where an override cannot
  cover a key, the engine refuses to run on that repo and says why. RC-2's tests include **a
  fixture repo whose configuration names a program for each of those places**, and assert that no
  comparison, file list, hunk or fetch on it runs any of them. The same rule for purlis's other
  git calls is #810's.
- **Content under review is data, never code.** A file's contents are drawn as text and never as
  markup; an image is drawn as an image, and an SVG is never put into the window's page as
  markup.
- **A request's head is fetched into the workspace's clone, under `refs/charter/review/`.** The
  objects must be local for git to diff them. The fetch runs under the same overrides, so it
  authenticates with the operator's own git credentials and never with a program the repo's
  configuration names. ADR 0070 §4 keeps git's credential path outside the forge seam. The refs
  are removed when no Review tab and no draft needs them.

### 3. The Review tab

**The chat tab's button is *Review*** (V34a), with the tooltip *Review this chat's work on its
branch*. It is a plain verb, so ADR 0072 §3's first-hour budget needs no exception, and it opens
the Review tab. R2's Review tab is a view tab. It holds one comparison, or one per member for a cross-repo
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
  are data purlis draws (E1, E6, ADR 0081 §2).

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

**Send to agent is the one place purlis presses Enter in a chat on the operator's behalf, and
it changes what ADR 0081 §4 settled for `place`.** See *ADR 0061 and ADR 0081, amended*.
Settled by V34b, quoted above.

When it is delivered depends on the chat's current run (ADR 0076 §1):

| The run is | Before HP-15 | Once HP-15 lands |
|---|---|---|
| `input-required (ready)` or `(turn-ended)` | delivered now: `purlisd` checks the state and delivers the prompt as one step under the run's state lock, as ADR 0081 §4 checks `place` | the same |
| `working` | refused: *the chat is working; send when its turn ends* | held in HP-15's queue, shown with *Take back*, and delivered at the turn's end (V11) |
| `input-required (asked)` | refused: a prompt delivered then could be read as the ask's answer | held, as for `working`. An ask keeps it held, and it is delivered only at `ready` or `turn-ended` |
| `hibernated` | refused: *open the chat first*. Opening it resumes it (ADR 0076 §5, SC-20), and the operator sends again | held, and the chat is woken. The hold is checked again under the **new** run's state lock and delivered only at `ready` or `turn-ended`; an ask on resume keeps it held |
| `paused` (any cause) | refused, naming the cause. Send never unpauses; only the operator does (ADR 0076 §2) | the same. A message already held when a budget pause comes stays held, and is delivered only after the operator unpauses and the run reaches a turn boundary |
| `queued`, `starting` | refused, naming the state | held, delivered at `ready` |
| ended | offers *Start a chat on this branch* with the review typed into its prompt and never sent (ADR 0061) | the same |

A held message is HP-15's, not this record's: it is visible, can be taken back, and is dropped
if the chat ends. This record defines no hold of its own.

- **It is the operator's act, from the window only.** Delivery is a command on the window's own
  connection (`local-ui`, the UI RPC), never a session-protocol command, and the `editor` scope
  never has it. An editor's `place` stays typed and never sent (V31a).
- **How it is delivered.** At level 3 the host delivers the prompt through the protocol. At level
  2, one bracketed paste with control characters taken out, then Enter, as one step under the
  state lock.
- **Enter is delivered only on a harness that has passed TS1's Enter measure.** ADR 0081 §4's
  residual risk was a paste without a line end landing in a dialog the harness opened between
  turns and had not yet reported. Enter is worse: it accepts a dialog's default choice. So TS1
  measures, per harness and harness version, that **a bracketed paste followed by Enter, sent
  into each dialog the harness can open between turns, chooses nothing**. A harness that has not
  passed, or that fails, gets the paste without Enter: the review is typed into the prompt and
  the operator presses Enter in the chat, which is then the operator's own key press.
- **The residual risk.** The state lock orders purlis's view of the run, not the harness's
  screen. On a harness that passed TS1, a dialog that TS1 does not know (a new harness version
  before the nightly runs, a dialog from a harness plugin) could still take Enter as its default.
  TS1 runs nightly against each supported version, and a failure turns Enter off for that
  harness until it passes again.
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
  is the review of record (U3). Reading and answering its threads in purlis is FW-16a/b's.
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
- **A saved edit is a human edit.** purlis records, as an event in the event log (FD-9): the
  chat, its run, the path relative to the repo, the changed line ranges, and whether the chat was
  busy. No contents. The edit is left in the folder uncommitted; purlis does not commit it.
- **The edit is announced to the chat's harness at its next turn** (R8, RC-11, V34a). It is
  never a prompt sent for the operator: it is context the harness reads at the start of a turn
  the chat begins anyway. The line is:

  > *The operator edited these files by hand since your last turn. Read them again before you
  > change them: `<path>`, `<path>` in `<repo>`.*

  - **Paths only.** It names each file relative to its repo, one clause per repo, and never
    contents, line ranges or a diff. The agent reads the files.
  - **Edits accumulate.** Every human edit saved since the chat's last turn began is in it, from
    one save or from many across several of the operator's sittings, each path once. Once a turn
    has carried the line, those edits are announced and are not repeated.
  - **Where it goes.** At level 2, in the harness's `UserPromptSubmit` hook output as
    `additionalContext`, beside a handed-back report (`crates/purlis-core/src/handback.rs`), or
    the harness's equivalent hook; on a harness with no prompt hook, in its `SessionStart`
    briefing at the next run and through purlis's MCP server (HP-7) in between. At level 3, in
    the structured context of the next prompt the host delivers.
  - **A chat that is working** when the edit is saved gets the line at its next turn boundary,
    with the next prompt, never in the middle of the turn it is in.
  - **A review sent** (§5) is its own prompt and is not repeated in the line.
  - **What it is built from.** The human-edit events in the event log (FD-9) since the chat's
    last `prompted` move (ADR 0076 §2). It adds no store.
  - **No ADR owns the briefing's parts as a whole.** `briefing::parts`
    (`crates/purlis-core/src/briefing.rs`) is a port of the Python charter's session-start
    briefing, and ADR 0063 and ADR 0064 each added one part to it. This line is a per-turn part,
    so it lives in the prompt hook's output, and the `SessionStart` briefing carries it only on a
    harness without one.

### 9. Race-mode comparison

RC-13 (Later) compares N candidates, each a chat's branch from the same base, side by side.
**Keep one** lands that branch's next step (§7) and removes the others' folders, keeping their
branches (ADR 0072 §4's *Remove folder*). **Pulling a hunk across** applies it in the kept
branch's folder, which is a human edit (§8). This record fixes only that a race is N comparisons
of kind (a) against one base.

## ADR 0061 and ADR 0081, amended

ADR 0061 types a prompt and never sends it. ADR 0081 §4 extended that to an editor's `place`:
typed into an idle chat, never sent, never held to be typed later, never woken (V31a's *"typed
and never sent"*). **Send to agent (§5) is a different act, from a different client, and it
reverses three of those rules for it alone:**

1. **It sends.** At level 2 it delivers a bracketed paste followed by Enter, on a harness that
   has passed TS1's Enter measure (§5); on any other it types without Enter, as `place` does.
2. **It holds.** Once HP-15 lands, a Send to a `working`, `asked`, `hibernated`, `queued` or
   `starting` run is held in HP-15's queue and delivered at a turn boundary, visible and taken
   back at will. `place` is never held.
3. **It wakes.** Once HP-15 lands, a Send to a `hibernated` chat wakes it and delivers at the
   new run's turn boundary, checked again under that run's lock. Before HP-15 it is refused.
   `place` refuses a `hibernated` chat and never wakes one.

It also **adds one command to the UI RPC on `local-ui`**: deliver a review to a chat, with the
check and the delivery as one step under the run's state lock. The session protocol does not
gain it, so the `editor` scope never has it (ADR 0068 §5 as ADR 0081 amended it), and no other
scope does either.

**What stands:** for ADR 0061's curation actions and ADR 0081's `place`, a prompt is typed and
never sent, never held and never wakes a chat. The `editor` scope still makes no agent act.
Ruled by V34b, which amends V31a for this one action.

The rest of ADR 0061 and ADR 0081 stands.

## ADR 0069, amended

**FR-10's backup.** ADR 0075 amended ADR 0069 so that FR-10 backs up `<data>/audit/` as well as
the machine store. **FR-10 also backs up `<data>/reviews/`** (§4, *Tiers*): the operator's
review drafts, which ADR 0069 row 71 already marks Machine, syncable and backed up, now have a
place, and it is in `<data>`. On a restore it comes back like `layout.json`, as a syncable store
(ADR 0069 §5).

The rest of ADR 0069 stands.

## ADR 0070, amended

**§1, the `Reviews` area.** Its comment gains RC-8a/b: `pub trait Reviews { /* RC-8a/b, FG-5a/b,
FW-16a/b */ }`. Its first method, added with RC-8a and RC-8b, publishes a draft as one review
with its line comments, each anchored to the request's head commit, on both forges (§6 of this
record). It takes a human `Caller` for the operator's review and a chat `Caller` for a reviewer
persona's (FG-5a/b), and §4's rules for each `Caller` are unchanged. GitLab's draft notes and
bulk publish are a forge capability, with one discussion per comment as its fallback.

**A chat `Caller` on `Reviews` adds no power (FI3).** Before SD-7a/b it resolves exactly as
§4 says for any chat `Caller`: to the CLI transport with the CLI login the chat already reaches,
and to no forge credential on a host whose login was imported. So a reviewer persona can post
nothing it could not already post by running `gh` or `glab` itself, and never under the
operator's sign-in token. After SD-7a/b it posts under its own per-agent identity.

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
| `<data>/reviews/`: one file per review, holding its draft, ticks, last reviewed head, position and sent comments | **Machine, syncable**, backed up by FR-10 (*ADR 0069, amended*) | V22b and ADR 0069 row 71. It names reviews by project, workspace, repo and branch and holds no path or code (§4). It is in purlis's data home, beside the audit, because it is the operator's data and not a preference. Chats are denied it. **Added to `docs/plane-format.md` in this PR** |
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
| `charter-core` | RC-2: the comparison types and the engine; the one fixed set of git overrides, and the fixture repo with a hostile configuration; the fetch under `refs/charter/review/`. RC-7: the review store, written through one module the window calls |
| `purlisd` | RC-7: Send to agent's one UI RPC command on `local-ui`, its check and delivery under the run's state lock, Enter only where TS1 passed; later HP-15's queue, which holds and wakes. The sandbox denies chats `<data>/reviews/` |
| The window | RC-4: the Review tab. RC-5: one CodeMirror 6 component for files and diffs. RC-6: R3's basics. RC-10: the edit and its warning. RC-3: the entry points |
| The forge seam | RC-8a/b: the `Reviews` area's first method (*ADR 0070, amended*) |
| Hooks and MCP | RC-11: the next-turn note |
| Harness checks | TS1: per harness and version, a bracketed paste followed by Enter into each between-turn dialog chooses nothing; a failure turns Send's Enter off for that harness |
| Other git calls | #810: the same rule for every git call purlis makes on a repo an agent can write |
| Audit | AU-2 registers the three actions (*ADR 0075, amended*) |
| `docs/plane-format.md` | The `<data>/reviews/` row (in this PR) |
| `CONTEXT.md` | Gains **Review tab**, **Comparison**, **Review draft** and **Human edit**, each with its concept (in this PR) |

## What this costs

- **A request for a repo outside every workspace needs the repo added first.** One step more
  than pasting a link into a forge's own page.
- **Send to agent waits for a turn to end**, and until HP-15 it is refused while the chat
  works, asks or hibernates.
- **On a harness that has not passed TS1's Enter measure, Send only types.** The operator
  presses Enter in the chat.
- **A dialog TS1 does not know could take Enter as its default** on a harness that passed (§5).
  TS1 runs nightly, and a failure turns Enter off for that harness.
- **A comment that cannot follow its line is not published.** The operator decides about each
  one.
- **A repo's own diff drivers never apply.** A file type the repo diffs through a text
  conversion shows as git's plain diff or as binary.
- **Fetched request refs sit in the workspace's clone** while a review needs them.

## What was rejected

- **A git library (`git2`, `gix`) for the engine.** purlis runs git as a program everywhere
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
- **Typing the review into the prompt unsent everywhere**, as ADR 0081's `place` does. The
  operator pressed *Send*, in purlis's window; it is the fallback only where TS1 has not passed.
- **Send from the `editor` scope.** Any program that reads the editor credential could then drive
  an agent (ADR 0081, *What was rejected*).
- **A hold of this record's own before HP-15.** Two queues would disagree about what a held
  message is; until HP-15, Send is refused instead.
- **Disabling repo-config programs call by call.** One missed call runs a program the repo
  names; one fixed set in one place, with a test, does not drift.
- **Delivering into an `asked` run.** The prompt could answer the ask; it is held instead.
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
4. **One fixed set of git overrides, held in one place, so no program named by a reviewed
   repo's configuration runs**, with a refusal where an override cannot cover a key. An agent can
   write every repo a review reads.
5. **Request heads are fetched under `refs/charter/review/`**, tier None, removed when unneeded.
6. **The merge view draws git's hunks** and diffs only inside them for word-level highlighting.
   It keeps R4's one engine.
7. **A review starts at the first comment, tick or edit.** R2 needs a moment for *kept*.
8. **A draft holds pointers and the operator's words, never code**, and names its review without
   a path, so it is syncable.
9. **The review store is `<data>/reviews/`**, in purlis's data home. ADR 0069 says Machine;
   `<data>` holds the operator's data, and `<config>` its preferences.
10. **Only the window writes a draft, and chats are denied it.** A draft goes out under the
    operator's identity.
11. **A comment follows its line, and one that cannot is *outdated*** and never published by
    itself.
12. **A review over 64 KiB is refused**, to be sent in parts.
13. **Publish is one act on both forges** (GitHub's review; GitLab's draft notes and bulk
    publish), with one discussion per comment as the fallback.
14. **Approve's forge approval is posted only on *Publish and approve*.**
15. **Changes since my last review** use the head at the last Send, Publish or Approve, and
    compare trees when the branch was rewritten.
16. **Idle for an edit** is ready, turn-ended, hibernated or ended, for every chat in the folder.
17. **A human edit is an event, and its save is an audit action**, both without contents; the
    edit stays uncommitted. Its announcement (§8, V34a) names paths only.
18. **Three audit actions**, `review.sent`, `review.approved`, `review.edit.saved`.

## Ruled (V34, 2026-10-01)

1. **The chat tab's button is *Review*** (V34a, as recommended). It is a plain verb and needs no
   exception to ADR 0072 §3. The rejected options were *Changes* (R1's word, which needed an
   exception) and no button on the chat tab. **The operator added a requirement:** a hand edit
   the operator saves is announced to that chat's harness at its next turn, listing the files
   changed since its last turn, and no prompt is sent on the operator's behalf. §8 gives the line,
   which names paths only.
2. **Send to agent comes from the window (`local-ui`) only, never from the `editor` scope, which
   stays "typed, never sent"; it presses Enter only on harnesses that pass TS1's paste and Enter
   test** (V34b, as recommended). This amends V31a for this one action (*ADR 0061 and ADR 0081,
   amended*). The rejected options were typing the review unsent everywhere and sending from the
   `editor` scope.

## Later decisions

- **How long sent comments stay in a review's file.** Until the branch's folder is removed is
  the starting point; RC-7 may cap it.
- **Live reviewing together** (RC-18) is Live sessions' (R11).
- **The forge review client's threads** in the Review tab, and *fix this*, are FW-16a/b's
  (FI12), on this record's comparison and tab.
- **Per-turn comparison** (RC-12) waits on MH-6's checkpoint refs.

## Amended 2026-10-04: the engine reads with gitoxide in the bounded reader (RC-2, V88a)

§2 said the engine *"runs git as a program, as the rest of `charter-core` does"*, and *What was
rejected* listed a git library. Two days after this record, V88a amended ADR 0027: automatic,
read-only reads of a branch an agent can write run in-process through gitoxide, in a short-lived
child of purlis's own binary with a deadline and a memory cap (D-88f, D-88h), because a git
process started there reads config the agent can write, and blanking its programs with `-c`
lost a race to a config swapped mid-read. The explorer's change markers are such a read, and R1
asks for **one** engine, so RC-2 builds the engine on that reader rather than beside it:

- **Every comparison is read by gitoxide in the reader's child** (`charter_core::files::compare`,
  `compare_file`, `compare_change`), with the config cut to the reader's allow-list. No git and
  no other program starts, so §2's "one fixed set of overrides" is the allow-list, held in one
  place, and the fixture with a hostile config is RC-2's
  (`tests/a_comparison_matches_git_diff.rs`).
- **The lines are git's algorithm, not git's program**: Myers, then git's indent heuristic, on
  lines that keep their line ends. RC-2's acceptance, *matches `git diff`*, is held by fixtures
  that run `git diff` as the oracle for every kind.
- **A file's working-tree side is read through the project root held open**, one component at
  a time following no link, as search reads (D-88i).
- **The explorer's markers are the engine's file list** without its line counts
  (`files::status`), so the two can never disagree.
- **Writes, and git commands the operator starts, stay on the git binary** (V88a). So does
  §2's fetch of a request's head under `refs/charter/review/` (RC-2's remaining slice, with
  kind (f)).

What it costs: a repo's own `diff.algorithm`, `diff.renames` and the like are not read, so a
comparison is always git's defaults; and an `eol` or filter conversion is not applied to the
working-tree side, so a file checked out with CRLF line ends compares as its bytes; and a
submodule's pointer moving is not shown, since a comparison lists files only (the #704
checklist carries both). Past 128 MiB of changed working-tree content in one comparison, a file
is marked and not read, so it has no line counts and is never taken for a rename; and a file
the branch changed and the working tree then put back to its base content is marked changed
there, where git shows nothing.

The rest of this record stands.
