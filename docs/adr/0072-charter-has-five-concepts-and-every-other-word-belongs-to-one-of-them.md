# purlis has five concepts, and every other word belongs to one of them

**Accepted 2026-09-30** by the operator (ruling V23), drafted for program-map ticket FR-2
(#601). It follows the operator's rulings **X22** (*"The five core concepts are Project,
Workspace, Chat, Persona, Memory. Vault is a setting under Project. Every new ruling must
name which concept it belongs to."*), **G5** (the core concepts cut to five), **W10**'s surface budget and **V6**, which
amended it: *"W10's first-hour words become the five concepts plus 'Save' and 'branch' ('Sync'
already means `charter sync`); FR-2's ADR fixes the exact piece ↔ branch wording; CONTEXT.md
gets Chat, Persona and Memory entries."*, and **V4**, which gives memory its owners and
audiences.

## Where purlis is today

`CONTEXT.md` defines about thirty words, and the internal audit of 2026-09 lists thirty-odd
concepts in the product (plane, project, plane root, workspace, LIVE/LOCAL, repo, piece, change,
persona, memory scopes, todos, session, needs you, guards, vault, handoff, curation action,
session record, save modes, trust gate, extension, and more). The program adds more: device,
run, work item, budget, race, trigger. Every one of them is precise, and most of them are
correct. None of them says which of the others it belongs to, so a new user meets them all at
the same height.

Three of the words collide:

- **Plane and Project.** `CONTEXT.md` has both. A plane is the git repo; a project is "one
  plane as the app has it open" (ADR 0033). The window already says both: the title bar has
  `New project…` and `Open a project…`, while the vault panel's empty state says *"No vaults on
  this plane"* and a chat with nowhere to start says *"charter has no plane open"*. They are one
  thing seen from two sides, and the operator sees both names.
- **Sync.** `purlis sync` fetches and fast-forwards a workspace's clones (`repocmd/sync.rs`).
  `purlis version sync` moves a plane to the purlis version it pins. `purlis persona
  sync-agents` regenerates the harness sub-agents from the persona files. `workspace remember
  --no-sync` means *do not commit and push it*, which is a save. W10 put "Sync" in the
  first-hour budget as the plane's two-way state; `CONTEXT.md` already tells authors to avoid
  "sync" for a save because *"that word is for fetching repos"*. V6 settled it for `purlis
  sync`. The other three are still there.
- **Piece and branch.** A piece is a git worktree (ADR 0027), and `CONTEXT.md` says to avoid
  "branch" for it. W10 wanted "the chat's branch" on the first-hour screen. The two are not
  one-to-one: a chat can touch several pieces (`sessionrecord::Touched` is a list), a
  cross-repo change has a branch in each member repo (ADR 0060), most chats today have no piece
  (cutting one from the window is not shipped), a branch outlives its worktree, and a piece is on
  one machine while its branch is pushed.

The product is also measured on its first ten minutes (W10, FR-1). A new user's first hour has
room for a few nouns, and every noun past those is a cost the user pays before purlis does
anything for them.

## The decision

**purlis has five concepts: Project, Workspace, Chat, Persona and Memory. Every other word in
purlis is a part, a view or a setting of exactly one of them. The first hour shows the five
plus "Save" and "branch", and no other purlis noun. "Sync" means only what `purlis sync`
does. A chat's piece is shown as its branch, one per repo.**

### 1. The five

| Concept | What it is | Where it lives |
|---|---|---|
| **Project** | The git repo that holds a team's (or one person's) workspaces, personas, memory and settings, and the tab the app shows it in. Called the *plane* until the rename in §1 lands. | a directory with `charter.toml` |
| **Workspace** | A named piece of work inside a project, with its own charter (`workspace.md`), memory, todos and repos. | `workspaces/<ws>/` |
| **Chat** | One conversation with an agent, in a tab, in one workspace or at the project root. It is where work happens, and what "needs you". | the session host and `reopen.json`; its identity is ADR 0066's |
| **Persona** | A role a chat can take: its own charter (`persona.md`), memory and vault, handed to the harness as a sub-agent. | `personas/<name>/` |
| **Memory** | What purlis keeps so that the next chat starts knowing what the last ones learned. Each memory has an **owner**: a workspace, a persona, everyone (shared) or **me**, one person's own. It also has an **audience**: this machine, me on all my machines, or the team. Approval follows the audience (V4). Session records are memory too (V23c). | the `memory/` and `sessions/` directories; *me* in a personal overlay project on the person's own private remote (V4) |

**"Project" stays (V23a), and "plane" is retired everywhere (V23b).** The word leaves every
surface a user reads (the window, `purlis --help` and the user docs, which FR-3 renames), and
also the code, the plane format, `docs/plane-format.md` and the name of the `purlis-plane`
repo. Project is the one word for the thing, in every place.

**The rename is follow-up work, not part of this record.** It has four parts:

- **A mechanical code rename.** Types, modules, functions and identifiers that say `plane` say
  `project`. Where the app already has a `project` that holds a `plane` (the window's project
  tab carries `project.plane`), the pair becomes one project with its root.
- **A project-format migration.** Every file, key, directory and environment variable named for
  the plane gets a project name (for example `CHARTER_PLANE_FENCE` and
  `PURLIS_PLANE_ROOT_SESSION`). For a compat window of at least one release, purlis still
  reads the old names, writes only the new ones, and says so once when it reads an old one.
- **A docs rename.** `docs/plane-format.md` becomes the project format's specification, and
  every ADR and doc written from now on says project. Accepted ADRs keep their words, since
  they are records.
- **Renaming the `purlis-plane` GitHub repo.** The operator does this, at the time of their
  choosing, and names the new repo. GitHub's redirect keeps old clones working.

**Nothing else is called a project.** A code repo is a **repo** (§6). Claude Code's own *project
settings* (its repo-level `.claude/settings.json`) are always named with Claude Code's name
beside them, so that they are never read as a purlis project's settings.

First-run copy that explains it, shown once beside the first project's tab:

> **A project is where purlis keeps your workspaces, personas and memory.** It is a git repo
> of its own, separate from your code. purlis made this one for you, on this machine only.

### 2. Everything else belongs to one of the five

Each row is where a word already in purlis, or ruled in the program, belongs. A part is
inside its concept; a view shows it; a setting changes how it behaves.

| Concept | Parts | Views | Settings |
|---|---|---|---|
| **Project** | the project root; the inventory; extensions; the machine store and the device (below) | alerts, doctor, the status line, the needs-you menu (a view of every project's chats), the work board across workspaces (FI5) | vaults (X22), save mode and auto-save, shared and local settings, the trust gate, harness plugins, profiles and the default harness, sandbox policy, preferences |
| **Workspace** | repos, pieces (as branches, §4), changes, members and requests, todos and work items (FW-5, V3, FI5) | the explorer, the bottom bar, the workspace's Work section | LIVE / LOCAL, a repo's own save mode |
| **Chat** | runs and child runs (ADR 0066), its branches (§4), its link to a work item | the chat strip, split windows, the transcript, the context gauge | its harness, profile and model, its persona, its sandbox opt-out |
| **Persona** | curation actions, the persona's sub-agent file, its dispatch and skill logs | the persona card, persona statistics | its vault (a named vault from the project's), its skills, delegate-when |
| **Memory** | its four owners' memories (workspace, persona, shared, me), session records, the archive, the personal overlay project | the memory tab, recall's results, the briefing a chat starts with, the approval queue for memory bound for a wider audience | each memory's audience (this machine, me on all my machines, team, V4); LIVE / LOCAL for a workspace's memory; what a persona's memory shares (`[memory] share`) |

**Machine-wide things belong to Project. Settled by X22 and ST6.** The device, the machine
store, app preferences and updates hold for every project on this machine. ST6 puts the settings
that hold beyond one workspace on the Project page, and the program map already files these
items under Project.

**Handoff and Smart close belong to Chat**: each is something one chat does. **Resume** starts a
chat from a session record: it is a Chat action that reads Memory.

**A new word needs a home before it ships.** A ticket or ruling that adds a user-facing noun says
which concept it is a part, view or setting of (ST9's definition of done already asks for the
owning concept). A noun with no home is a sixth concept, and a sixth concept is a ruling, not a
ticket.

### 3. The surface budget: the first hour shows seven purlis words

**The first-hour surfaces** are what a new user sees before they open Settings or an advanced
menu: the first-run flow (FR-4), the title bar and its project tab, the workspace and chat
strips, the new-chat picker, a chat's tab and header, the save indicator, the needs-you menu,
and the palette's rows that are not under "Advanced".

**On those surfaces, purlis's own nouns are exactly these:** Project, Workspace, Chat,
Persona, Memory, **Save** and **branch**. "Needs you" is a state a chat is in, and is allowed.
Ordinary words (open, new, close, file, folder) are not purlis's nouns and are free, and so
are the products purlis drives, named by their own names (Claude Code, Codex, opencode,
GitHub, GitLab).

The first-hour surfaces say **none of**: plane, piece, worktree, run, device, vault, mode,
harness, profile, extension, capability, curation, change, member, inventory, strip,
show-more, LIVE or LOCAL. Each of those is still used where it belongs: in Settings, in a view
tab, in the CLI, in the docs. **Settled by V6**, since "harness" and "profile" are outside its
budget: the new-chat picker names the harness by its product name, under a verb label
(*Start with*), and never says "Harness" or "Profile".

**The capability card W10 rules stays, in the budget's words.** W10 puts a harness capability
card in the picker, the chat header and disabled controls' tooltips. It appears in all three.
Its label is *What <product> can do here* (for example, *What Codex can do here*), and each line
says what the user will or will not see in plain words and the budget's nouns: *Codex does not
tell purlis when it is waiting, so this chat will not show needs you.* The word "capability"
stays in the card's code and in Settings, not on the card. A disabled control's tooltip is one
such line, followed by the card's label as a link.

FR-3 enforces the budget with a UI-string test over the first-hour components' copy: a listed
word in any of their strings fails the build. DS-8's UX audit checks what the test cannot, such
as a word inside an image.

### 4. A chat's piece is shown as its branch, one row per repo

A piece is still a git worktree, and `purlis worktree` still says so. On screen (V23d):

- **A chat is shown with its branches, one row per repo it works in.** A row reads
  *`fix-login` in api*: the branch, then the repo. A chat that works in one repo, which GL-1
  makes the common case, has one row. A cross-repo change shows one row per member.
- **A chat with no piece of its own** has no row of its own. Where it works in a repo's shared
  clone, the row reads *`main` in api · shared*: the branch the clone has checked out, marked
  as shared with every other chat that works there.
- **The action that cuts a piece is "New branch"**, and it names the branch and the repo it will
  be cut in. The piece's directory is shown only where a path is needed (Reveal in Finder, a
  copied path), and is called the branch's *folder*.
- **"Done" and "Abandon"** are the piece log's `done` and `abandoned`, said of the branch.
  **"Remove folder"** is `purlis worktree remove`, and it says the branch stays. Deleting a
  branch is git's and the forge's, never this row's.
- **A branch whose piece is gone** is no longer purlis's: the row goes, and the branch is left
  to git and the forge. A branch pushed from another machine is shown as a branch, never as a
  piece, since pieces are per machine.
- **The chat header and the session record** list the same rows, from the same `Touched` list,
  so what a chat said it worked on and what the window shows are one list.

"Piece" leaves the window and `purlis --help`'s summaries. It stays in the project format, the
piece log, the code and `purlis worktree`'s own help, where the difference between a
directory and a branch is the point.

### 5. "Sync" means only what `purlis sync` does

**Sync is: fetch every clone in a workspace, and fast-forward the ones that hold no work.** No
other operation, on screen or on the command line, is called Sync.

- **The window.** When the window offers `purlis sync`, it is labelled *Sync repos*. The project's
  own commit-and-push is **Save**, and the commits it has not fetched yet are **Incoming**, as
  `CONTEXT.md` has them.
- **The command line. Settled by V6 and FR-3's alias rule:** V6 makes "Sync" mean one
  operation, and FR-3 keeps each renamed spelling as an alias for one release. The new names
  are proposed here, and FR-3 may pick better ones:

  | Today | Becomes | Why |
  |---|---|---|
  | `workspace remember --no-sync`, `workspace note --no-sync` | `--no-save` | it means *do not commit and push this now*, which is a save |
  | `purlis version sync` | `purlis version apply` | it moves this project to the version its pin names |
  | `purlis persona sync-agents` | `purlis persona write-agents` | it writes the harness's sub-agent files from the personas |

- **Later features pick another word.** U10's device-state feature, a relay's catch-up and a
  vendor-memory import do not say Sync. Each names what moves and where to.

**The test (FR-3's acceptance):** a UI-string test reads every user-visible string
in `app/src` (text nodes, `aria-label`, `title`, placeholder, and copy constants) and fails on
the word "sync" in any case, except the label bound to the `purlis sync` command. A CLI
counterpart checks `purlis --help`'s command and flag names the same way, and allows the
aliases only while they are marked deprecated.

### 6. How purlis's words map to other tools'

The same word means different things in the tools purlis's users already use. The table maps
each concept to the nearest term, and says where a user's reading of purlis's word would be
wrong.

| purlis | GitHub | GitLab | Claude Code | Cursor | Codex, Zed, Amp | Conductor | Risk |
|---|---|---|---|---|---|---|---|
| **Project** | *Projects*: a board or table of issues and pull requests; a view over work, holding no code | *project*: one repository, with its issues and CI | *Projects* (beta): a coordinating conversation that starts threads, with instructions and memory | *Projects* | the folder an editor opens (Zed's project) | — | **High.** On GitLab a code repo is a project. purlis's UI never calls a code repo a project, on any forge: it is always a **repo** (V23a) |
| **Workspace** | — (a Codespace is an environment) | *Workspaces*: a remote development environment | — | — | Amp's *workspace* is the team | *workspace*: one task's branch, files and terminal, which is purlis's **piece** | **Medium.** A Conductor user reads a purlis workspace as one branch. First-run copy says a workspace holds repos, chats and their memory |
| **Chat** | a Copilot coding agent's *session* | — | *session*, *conversation* | *agent*, *chat* | *thread* | a workspace's chat | **Low.** "Session" stays out of UI text (it is the process, ADR 0066) |
| **Persona** | *custom agent* | — | *subagent* | — | — | — | **Low.** opencode calls the same thing an *agent*. A persona is handed to the harness *as* a sub-agent, and the persona card says so |
| **Memory** | *custom instructions* | — | *memory* (`CLAUDE.md`), a Project's memory | *Rules*, *Memories* | — | — | **Low.** Devin and Antigravity call it *knowledge*. FR-18a imports `CLAUDE.md`, `AGENTS.md` and Cursor rules as workspace memory, so the mapping is also the import path |

The tools named here change their words often. This table is a snapshot of 2026-09 for the
first-run copy's sake, not a claim about any of them, and a wrong row is corrected in place.

## What changes where

The code does not change with this record. FR-3 applies it; FR-4 writes the first-run copy;
DS-8 audits the budget.

| Where | What changes |
|---|---|
| `CONTEXT.md` | Gains **Chat**, **Persona**, **Memory**, **Work item**, **Branch** and **Sync** entries, and a new first section, "The five concepts", that holds the five entries. **Project** is the one word, and **Plane** says it is being retired. **Piece** and **Save** point at the branch and Sync wording. Run and Device already have entries (ADR 0066). The wholesale renaming of `CONTEXT.md` is FR-3's |
| The window's copy (`app/src`) | FR-3: "plane" becomes "project" in every user-visible string; "piece" becomes the branch wording (§4); the first-hour budget holds (§3); the UI-string tests (§3, §5) |
| `purlis --help` | FR-3: the three `sync` renames with aliases; "plane" becomes "project" in summaries |
| Recorded behaviour (`tests/fixtures/recorded/behaviour.jsonl`) | The `persona-sync-agents-…` scenarios keep passing through the alias. A scenario that records a renamed word moves only with the sentence ADR 0046 requires |
| Code, the plane format, `docs/plane-format.md`, the `purlis-plane` repo | The follow-up rename in §1: code, a format migration with a compat window for the old names, docs, and the repo rename the operator does |
| Tickets and rulings | Each names its concept (X22). ST9's definition of done already carries it |

## What this costs

- **A rename across everything.** Retiring "plane" touches most modules, every plane's files
  and instructions, and a public repo's name. The compat window keeps old projects opening
  while it lands, and the rename is mechanical, so it is large but not hard.
- **"Project" collides with GitLab's project and GitHub's Projects.** The first-run line and the
  rule "a code repo is always a repo" are what pays for keeping it. A GitLab user will still
  read it wrong once.
- **The branch rows are more than a word.** §4 is a view, not a rename: the chat header lists
  rows it does not list today. GL-1 and FR-3 build it.
- **Three CLI renames**, each with a deprecation release, and a project's instructions that name
  the old spelling keep working only for that release.
- **Advanced users see the old words in advanced places.** "Piece" and "vault" stay where they
  are precise. The budget covers the first hour, not the whole product.

## What was rejected

- **Vault as the fifth concept** (the consistency review's proposal). X22 ruled Memory in and
  made Vault a setting under Project. A vault is something a persona or command *uses*; memory
  is what a chat *starts with*, which is what a new user has to understand.
- **"Sync" as the plane's two-way state** (the phase-2 critique's proposal, renaming `charter
  sync` to `charter pull`). V6 kept `charter sync`'s meaning, and "Save" already names the
  plane's side.
- **"The chat's changes" instead of "branch"** (the same critique). V6 put "branch" in the
  budget. §4 takes the critique's point, one row per repo, and keeps the word the user already
  knows from git.
- **"The chat's branch", singular.** It is wrong for a cross-repo change, for a chat with no
  piece and for any chat before GL-1.
- **Keeping "plane" in the code and the format** (the draft's recommendation). The operator
  ruled for one word everywhere (V23b).

## Ruled (V23, 2026-09-30)

1. **Keep "Project"**, with the first-run line in §1. A code repo is always a "repo" in the UI,
   on GitLab too (V23a).
2. **"Plane" is renamed everywhere**: the code, the plane format, `docs/plane-format.md` and
   the `charter-plane` repo name, not only the UI. The rename is follow-up work; the repo rename
   is the operator's (V23b).
3. **Session records belong to Memory** (V23c).
4. **"Piece" leaves the window, and the piece ↔ branch wording is §4 as drafted** (V23d).

## Amendment, 2026-10-01: the rename's compat window is six months (V37c)

§1's project-format migration said: *"For a compat window of at least one release, charter
still reads the old names"*. **Ruled by V37c:** *"Old forms are read for six months from the
first release that writes the new form. #762's plane→project read-compat window uses the same
six months. This amends ADR 0072's "at least one release"."*

The window is therefore **six months from the first release that writes the new names**. During
it purlis still reads the old names, writes only the new ones and says so once when it reads an
old one, as §1 says. It is the same window `docs/plane-format.md` gives every format change
(*Compatibility across purlis versions*, FR-24). The rest of ADR 0072 stands.

## Amendment, 2026-10-07: a persona is a role a chat runs as for its whole life (#1435)

§1's table defined **Persona** as *"A role a chat can take: its own charter (`persona.md`),
memory and vault, handed to the harness as a sub-agent."* **Ruled by the operator on
2026-10-07**, with the acceptance of [ADR 0090](0090-agents-work-together-as-chats-that-are-listed-observable-and-stoppable-and-no-agents-word-is-consent.md)
as amended:

| Concept | What it is | Where it lives |
|---|---|---|
| **Persona** | A role a chat runs as for its whole life: its own charter (`persona.md`), memory and vault. A chat's persona is fixed when the chat starts, and work for another persona goes to a chat of its own, by dispatch. | `personas/<name>/` |

A persona is never a harness sub-agent. A sub-agent runs inside its chat, so it holds what that
chat's persona was given and nothing of its own. Three other lines of this record read with
that change:

- **The parts table** lists *"the persona's sub-agent file"* and *"delegate-when"* among a
  persona's parts. The sub-agent file goes when purlis stops writing it (ADR 0090 as amended,
  change 1). What becomes of each persona key that fed that file is decided where that is built.
- **The Sync table** renames `purlis persona sync-agents` to `purlis persona write-agents`. The
  command is retired with the files it writes, so the rename is not carried out.
- **The harness table** says *"A persona is handed to the harness as a sub-agent, and the persona
  card says so"*. A harness's *custom agent* or *subagent* is now a helper inside a chat, which
  carries that chat's persona. It is not purlis's Persona, and the two words no longer meet.

The rest of ADR 0072 stands: Persona is still one of the five concepts, and the words ADR 0090
adds are parts of Chat.
