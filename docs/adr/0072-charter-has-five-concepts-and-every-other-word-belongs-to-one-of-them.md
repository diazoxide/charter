# Charter has five concepts, and every other word belongs to one of them

**Proposed 2026-09-30.** An agent drafted it for program-map ticket FR-2 (#601), and the
operator rules it (W7: an ADR merges only after the operator's ruling). It follows the
operator's rulings **X22** (*"The five core concepts are Project, Workspace, Chat, Persona,
Memory. Vault is a setting under Project. Every new ruling must name which concept it belongs
to."*), **G5** (the core concepts cut to five), **W10**'s surface budget and **V6**, which
amended it: *"W10's first-hour words become the five concepts plus 'Save' and 'branch' ('Sync'
already means `charter sync`); FR-2's ADR fixes the exact piece ↔ branch wording; CONTEXT.md
gets Chat, Persona and Memory entries."*

## Where charter is today

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
- **Sync.** `charter sync` fetches and fast-forwards a workspace's clones (`repocmd/sync.rs`).
  `charter version sync` moves a plane to the charter version it pins. `charter persona
  sync-agents` regenerates the harness sub-agents from the persona files. `workspace remember
  --no-sync` means *do not commit and push it*, which is a save. W10 put "Sync" in the
  first-hour budget as the plane's two-way state; `CONTEXT.md` already tells authors to avoid
  "sync" for a save because *"that word is for fetching repos"*. V6 settled it for `charter
  sync`. The other three are still there.
- **Piece and branch.** A piece is a git worktree (ADR 0027), and `CONTEXT.md` says to avoid
  "branch" for it. W10 wanted "the chat's branch" on the first-hour screen. The two are not
  one-to-one: a chat can touch several pieces (`sessionrecord::Touched` is a list), a
  cross-repo change has a branch in each member repo (ADR 0060), most chats today have no piece
  (cutting one from the window is not shipped), a branch outlives its worktree, and a piece is on
  one machine while its branch is pushed.

The product is also measured on its first ten minutes (W10, FR-1). A new user's first hour has
room for a few nouns, and every noun past those is a cost the user pays before charter does
anything for them.

## The decision

**Charter has five concepts: Project, Workspace, Chat, Persona and Memory. Every other word in
charter is a part, a view or a setting of exactly one of them. The first hour shows the five
plus "Save" and "branch", and no other charter noun. "Sync" means only what `charter sync`
does. A chat's piece is shown as its branch, one per repo.**

### 1. The five

| Concept | What it is | Where it lives |
|---|---|---|
| **Project** | The git repo that holds a team's (or one person's) workspaces, personas, memory and settings, and the tab the app shows it in. Called the *plane* in code and in the plane format. | a directory with `charter.toml` |
| **Workspace** | A named piece of work inside a project, with its own charter (`workspace.md`), memory, todos and repos. | `workspaces/<ws>/` |
| **Chat** | One conversation with an agent, in a tab, in one workspace or at the project root. It is where work happens, and what "needs you". | the session host and `reopen.json`; its identity is ADR 0066's |
| **Persona** | A role a chat can take: its own charter (`persona.md`), memory and vault, handed to the harness as a sub-agent. | `personas/<name>/` |
| **Memory** | What charter keeps so that the next chat starts knowing what the last ones learned: a workspace's memory, a persona's, shared memory, and session records. | the `memory/` and `sessions/` directories |

**"Project" stays** (open question 1 recommends keeping it). The word the plane format and the
code use for a project's repo stays **plane**; it leaves every surface a user reads: the
window, `charter --help`, and the user docs (FR-3 does the renaming). `docs/plane-format.md`
keeps its name, because it is the format's specification and is read by the people who write
code against it.

First-run copy that explains it, shown once beside the first project's tab:

> **A project is where charter keeps your workspaces, personas and memory.** It is a git repo
> of its own, separate from your code. charter made this one for you, on this machine only.

### 2. Everything else belongs to one of the five

Each row is where a word already in charter, or ruled in the program, belongs. A part is
inside its concept; a view shows it; a setting changes how it behaves.

| Concept | Parts | Views | Settings |
|---|---|---|---|
| **Project** | the project root; the inventory; extensions; the machine store and the device (below) | alerts, doctor, the status line, the needs-you menu (a view of every project's chats), the work board across workspaces (FI5) | vaults (X22), save mode and auto-save, shared and local settings, the trust gate, harness plugins, profiles and the default harness, sandbox policy, preferences |
| **Workspace** | repos, pieces (as branches, §4), changes, members and requests, todos and work items (FW-5, V3, FI5) | the explorer, the bottom bar, the workspace's Work section | LIVE / LOCAL, a repo's own save mode |
| **Chat** | runs and child runs (ADR 0066), its branches (§4), its link to a work item | the chat strip, split windows, the transcript, the context gauge | its harness, profile and model, its persona, its sandbox opt-out |
| **Persona** | curation actions, the persona's sub-agent file, its dispatch and skill logs | the persona card, persona statistics | its vault (a named vault from the project's), its skills, delegate-when |
| **Memory** | workspace memory, persona memory, shared memory, session records, the archive | the memory tab, recall's results, the briefing a chat starts with | what a persona's memory shares (`[memory] share`) |

**Machine-wide things belong to Project.** The device, the machine store, app preferences and
updates hold for every project on this machine, and the program map already files them under
Project. This keeps X22's rule, *every ruling names its concept*, without a sixth answer.

**Handoff and Smart close belong to Chat**: each is something one chat does. **Resume** starts a
chat from a session record: it is a Chat action that reads Memory.

**A new word needs a home before it ships.** A ticket or ruling that adds a user-facing noun says
which concept it is a part, view or setting of (ST9's definition of done already asks for the
owning concept). A noun with no home is a sixth concept, and a sixth concept is a ruling, not a
ticket.

### 3. The surface budget: the first hour shows seven charter words

**The first-hour surfaces** are what a new user sees before they open Settings or an advanced
menu: the first-run flow (FR-4), the title bar and its project tab, the workspace and chat
strips, the new-chat picker, a chat's tab and header, the save indicator, the needs-you menu,
and the palette's rows that are not under "Advanced".

**On those surfaces, charter's own nouns are exactly these:** Project, Workspace, Chat,
Persona, Memory, **Save** and **branch**. "Needs you" is a state a chat is in, and is allowed.
Ordinary words (open, new, close, file, folder) are not charter's nouns and are free, and so
are the products charter drives, named by their own names (Claude Code, Codex, opencode,
GitHub, GitLab).

The first-hour surfaces say **none of**: plane, piece, worktree, run, device, vault, mode,
harness, profile, extension, capability, curation, change, member, inventory, strip,
show-more, LIVE or LOCAL. Each of those is still used where it belongs: in Settings, in a view
tab, in the CLI, in the docs. The new-chat picker names the harness by its product name, and
its label is a verb (*Start with*), never "Harness" (open question 5).

FR-3 enforces the budget with a UI-string test over the first-hour components' copy: a listed
word in any of their strings fails the build. DS-8's UX audit checks what the test cannot, such
as a word inside an image.

### 4. A chat's piece is shown as its branch, one row per repo

A piece is still a git worktree, and `charter worktree` still says so. On screen:

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
  **"Remove folder"** is `charter worktree remove`, and it says the branch stays. Deleting a
  branch is git's and the forge's, never this row's.
- **A branch whose piece is gone** is no longer charter's: the row goes, and the branch is left
  to git and the forge. A branch pushed from another machine is shown as a branch, never as a
  piece, since pieces are per machine.
- **The chat header and the session record** list the same rows, from the same `Touched` list,
  so what a chat said it worked on and what the window shows are one list.

"Piece" leaves the window and `charter --help`'s summaries. It stays in the plane format, the
piece log, the code and `charter worktree`'s own help, where the difference between a
directory and a branch is the point.

### 5. "Sync" means only what `charter sync` does

**Sync is: fetch every clone in a workspace, and fast-forward the ones that hold no work.** No
other operation, on screen or on the command line, is called Sync.

- **The window.** When the window offers `charter sync`, it is labelled *Sync repos*. The plane's
  own commit-and-push is **Save**, and the commits it has not fetched yet are **Incoming**, as
  `CONTEXT.md` has them.
- **The command line.** FR-3 renames the three other uses, and keeps each old spelling as an
  alias for one release (FR-3's rule):

  | Today | Becomes | Why |
  |---|---|---|
  | `workspace remember --no-sync`, `workspace note --no-sync` | `--no-save` | it means *do not commit and push this now*, which is a save |
  | `charter version sync` | `charter version apply` | it moves this project to the version its pin names |
  | `charter persona sync-agents` | `charter persona write-agents` | it writes the harness's sub-agent files from the personas |

- **Later features pick another word.** U10's device-state feature, a relay's catch-up and a
  vendor-memory import do not say Sync. Each names what moves and where to.

**The test (FR-2's acceptance, and FR-3's):** a UI-string test reads every user-visible string
in `app/src` (text nodes, `aria-label`, `title`, placeholder, and copy constants) and fails on
the word "sync" in any case, except the label bound to the `charter sync` command. A CLI
counterpart checks `charter --help`'s command and flag names the same way, and allows the
aliases only while they are marked deprecated.

### 6. How charter's words map to other tools'

The same word means different things in the tools charter's users already use. The table maps
each concept to the nearest term, and says where a user's reading of charter's word would be
wrong.

| charter | GitHub | GitLab | Claude Code | Cursor | Codex, Zed, Amp | Conductor | Risk |
|---|---|---|---|---|---|---|---|
| **Project** | *Projects*: a board or table of issues and pull requests; a view over work, holding no code | *project*: one repository, with its issues and CI | *Projects* (beta): a coordinating conversation that starts threads, with instructions and memory | *Projects* | the folder an editor opens (Zed's project) | — | **High.** On GitLab a code repo is a project. charter's UI never calls a code repo a project, on any forge: it is always a **repo** |
| **Workspace** | — (a Codespace is an environment) | *Workspaces*: a remote development environment | — | — | Amp's *workspace* is the team | *workspace*: one task's branch, files and terminal, which is charter's **piece** | **Medium.** A Conductor user reads a charter workspace as one branch. First-run copy says a workspace holds repos, chats and their memory |
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
| `CONTEXT.md` | Gains **Chat**, **Persona**, **Memory**, **Work item**, **Branch** and **Sync** entries, and an opening paragraph that names the five. **Project** and **Plane** say which is the user's word. **Piece** and **Save** point at the branch and Sync wording. Run and Device already have entries (ADR 0066). The wholesale renaming of `CONTEXT.md` is FR-3's |
| The window's copy (`app/src`) | FR-3: "plane" becomes "project" in every user-visible string; "piece" becomes the branch wording (§4); the first-hour budget holds (§3); the UI-string tests (§3, §5) |
| `charter --help` | FR-3: the three `sync` renames with aliases; "plane" becomes "project" in summaries |
| Recorded behaviour (`tests/fixtures/recorded/behaviour.jsonl`) | The `persona-sync-agents-…` scenarios keep passing through the alias. A scenario that records a renamed word moves only with the sentence ADR 0046 requires |
| `docs/plane-format.md` | Keeps its name; its introduction says a plane is a project's repo |
| Tickets and rulings | Each names its concept (X22). ST9's definition of done already carries it |

## What this costs

- **Two words for one thing.** Code and the plane format say *plane*; users read *project*. The
  split is the same as ADR 0066's number and id: one word for the people who use it, one for the
  code that stores it, and a rule that keeps them apart.
- **"Project" collides with GitLab's project and GitHub's Projects.** The first-run line and the
  rule "a code repo is always a repo" are what pays for keeping it. A GitLab user will still
  read it wrong once.
- **The branch rows are more than a word.** §4 is a view, not a rename: the chat header lists
  rows it does not list today. GL-1 and FR-3 build it.
- **Three CLI renames**, each with a deprecation release, and a plane's instructions that name
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
- **Renaming "plane" in the code and the format.** It would touch every module and every plane's
  instructions for no change a user can see, and `charter-plane` is already public under that
  name.

## For the operator's ruling

Each of these goes beyond the words of X22, W10 and V6. Each has a recommendation.

1. **Keep "Project"**, with the first-run line in §1, rather than rename it. *Recommend keep.*
   The window already says `New project…` and `Open a project…`, editors use the word for the
   thing you open, and every candidate (Space, Hub, Home, Team, Charter) collides as badly or
   says less.
2. **A code repo is always a "repo" in charter's UI, on GitLab too**, never a "project".
   *Recommend yes.* It is the one rule that makes keeping "Project" safe for GitLab users.
3. **"Plane" leaves the window, `charter --help` and the user docs, and stays in code, the
   plane format and `charter-plane`'s name.** *Recommend yes.* Users gain one word; renaming the
   code buys nothing a user sees.
4. **Session records are part of Memory, not of Chat.** *Recommend Memory.* They outlive the
   chat, the next chat's briefing reads them, and Resume reads them as memory.
5. **The first hour never says "harness" or "profile"**: the picker names the product (Claude
   Code, Codex, opencode) under the label *Start with*. *Recommend yes.* Both are charter nouns
   outside the budget, and the product name is what the user installed.
6. **The piece ↔ branch wording in §4**: one row per repo reading *`<branch>` in `<repo>`*,
   *· shared* for a clone's checked-out branch, **New branch** to cut a piece, **Remove folder**
   to remove it, and "piece" gone from the window. *Recommend yes.* It is true for every case the
   critique listed, and it uses only "branch".
7. **The three CLI renames in §5** (`--no-save`, `version apply`, `persona write-agents`), each
   with a one-release alias. *Recommend yes.* V6 makes "Sync" mean one operation; the flag
   already means a save.
8. **Machine-wide things (device, preferences, updates) belong to Project.** *Recommend yes.* The
   program map already files them there, and a sixth "the app" answer would break X22's rule.
