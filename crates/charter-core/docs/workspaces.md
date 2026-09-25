# Workspaces

A **workspace** is one task's worth of the world: a directory of repo clones, the notes
made while working on them, and a written account of what the task is for.

    workspaces/<name>/
      <repo>/           a clone, on its own branch
      .worktrees/       further splits of those clones (see below)
      workspace.md      the living charter: Vision, Context & decisions, Glossary
      workspace.json    the committed manifest: which repos, which branches
      memory/           durable notes, one file per fact, with a MEMORY.md index
      todos/            what this task still means to do
      .claude/          charter's harness layer, generated (see below)

`default` always exists. `charter workspace create <name> --use` starts another, and
`charter ws` is the same command as `charter workspace`.

**"Always exists" is about the name, not about the directory.** `default` is the rung the
resolution below terminates on, so a plane that has never selected anything is standing in
it; `charter init` does not create `workspaces/default/`, and the first command that puts
something in it does (`charter clone -w default`, `charter workspace use default`,
`charter workspace create default`). `charter workspace list` lists it either way, with
`—` for its repos, because a table that draws a "you are here" mark has to have a row for
where you are. The name is `[workspace] default` if your `charter.toml` sets one.

## A chat standing here gets charter

Claude Code reads project settings from the session's working directory and **does not walk
up** for them. A chat started in `workspaces/<name>/` would therefore get none of the plane's
plugins and none of its `env`, while its agents and skills arrived anyway, because those *do*
walk up and this directory is not a git boundary. (Charter's own plugin and
`$CHARTER_HARNESS` do not ride on this file: the app puts them on the chat's command line and
in its environment.)

So charter generates `.claude/settings.json` here: the plane's own `enabledPlugins` and
`env`, plus the **restrictive half** of its `permissions`. It is written when the workspace is
created and again by `charter workspace reinit`. Skills come with the plugin and agents
already walk up; a second copy of either would shadow the plugin's.

### The plane's ask and deny rules come with it

`permissions.ask` and `permissions.deny` travel. **`permissions.allow` never does.** A grant
copied sideways puts a permission in force in a directory nobody clicked for it in; a
restriction is the opposite — it adds a prompt or a refusal and can make nothing run.

A `--local` rule reaches a workspace directory without being copied. Measured on Claude Code
2.1.267 (git 2.50.1), with `claude -p` against a `deny` in throwaway repositories:

| the rule is in | the session starts in | result |
|---|---|---|
| the git root's `.claude/settings.local.json` | a subdirectory | blocked |
| the subdirectory's own `.claude/settings.local.json` | that subdirectory | blocked |
| the git root's `.claude/settings.json` | a subdirectory | ran |
| the outer repo's `.claude/settings.local.json` | a clone nested inside it | ran |
| the clone's own `.claude/settings.local.json` | that clone | blocked |
| the main checkout's `.claude/settings.local.json` | a linked worktree of it | blocked |

So the local file is read at the git root as well as in the starting directory, and the
shared file is not. `workspaces/<name>/` sits inside the plane's own repository, so the
plane's `.claude/settings.local.json` is already in force there and charter generates no
copy. A clone is a git root of its own, so it gets a generated `.claude/settings.local.json`
holding the plane's local `ask` and `deny` — separate from the shared file, so a decision that
is yours on this machine does not arrive looking like the team's.

**In a clone that file is shared with Claude Code**, which saves "Yes, and don't ask again"
into it. Charter writes the clone's `.git/info/exclude` entry *before* the file, and writes
no local file at all where the exclude cannot be written: a machine-local rule it cannot hide
would be committable, and `workspace reinit` and `clone` each say so. Once Claude Code has
added its own approvals, charter keeps the file hidden for as long as it exists, never
rewrites it and never merges into it — and "hidden" holds across the repository: a clone and
its linked worktrees read one `.git/info/exclude`, so charter's block there lists what every
one of them needs, and the local file's line stays while that file exists in any of them.
Once the file is gone and the plane no longer declares it, its line goes.

**A file charter wrote in a clone stays hidden while it is there**, whatever charter's own
records say — a generated file, its `.charter-generated` record, a temp file an interrupted
write left — in every checkout charter wires that reads the same `.git/info/exclude`. That
includes a generated file you have since rewritten: charter never overwrites it, and if it is
your own file and you mean to commit it, `git add -f` it. A line goes only once its path is
confirmed absent in all of those checkouts and the plane no longer declares it. While git
cannot list the worktrees in time, or lists one charter cannot look into, every line stays,
and so does a line whose path cannot be checked.

Charter writes each of those files whole — to a temp file beside it, flushed to disk, then
renamed over it — so a kill leaves the old content or the new and never half of either. A temp
file a kill leaves behind is named `.charter-generated.<pid>.<random>.tmp`, and the block hides
that pattern anywhere in the checkout, so a file of your own whose name matches it
(`.charter-generated.notes.tmp`) is hidden too. Before rewriting a generated file charter
records the write as pending, and it overwrites only content one of its records lists; anything
else is yours or the harness's. Where the record cannot be published — a checkout root that is
not writable, a read-only or full disk — charter writes nothing there and keeps the lines.

`charter workspace reinit` runs both ways: drop a rule from the plane's settings and the
mirror of it is withdrawn — a file charter generated and no longer generates is removed, but
only while its content still matches what charter wrote. A plane settings file that does not
parse is not a plane that declares nothing: every workspace keeps its last good copy.

Codex has no command-pattern permissions at all, so there is nothing to carry there.

It is **charter's file, and only while it stays charter's**. A `.charter-generated` sidecar
records a hash of what charter wrote. A file that still matches is refreshed when the
plane's settings move; one that does not is yours — left completely untouched and never
repaired. `charter workspace reinit` (or `--all`) is the repair for a file that still is
charter's. `charter doctor` has a `workspace layer` row, and in this version it says the
layer is not checked there yet.

**A clone gets the layer too, and pays a cost this directory does not.**
`workspaces/<name>/<repo>/` is a repo of its own, so the walk-up that carries agents and
skills into a workspace directory stops at its root and a session there would get none of the
layer — not the settings, and not the personas either. Charter writes both, when it clones
and on `reinit`, and registers every path it generated in that checkout's
**`.git/info/exclude`**: per-checkout, never committed, not itself tracked, and the one file a
guest may write. Charter never edits the clone's `.gitignore`, hides only the exact paths it
wrote (never a `.claude/` glob, which would take your own untracked files with it), never
touches a file it did not generate, and removes its files and its exclude block when the
workspace goes. `git status` in your repo is unaffected. Linked worktrees get it as well (see
*A session in a piece gets the layer a clone gets* below), and their `info/exclude` is the
main repo's, which is also why removal is not just a `rm -rf`, and why removing a workspace
takes away only the lines no other checkout of that repository still needs. **`CLAUDE.md` is
deliberately left behind**: a guest hides its own files, it does not narrate the host's.

What a clone cuts off is Claude Code's `.claude/agents/` and `.claude/skills/`, and those
are what charter mirrors into it.

## Which workspace am I in

Resolved fresh on every command, in this order. The first rung that answers, wins:

1. `--workspace <name>` on the command
2. `$CHARTER_WORKSPACE`
3. **the tree you are standing in**
4. the per-session pointer
5. the per-terminal pointer
6. the nominated default (`charter workspace default <name>`)
7. `default`

The cwd sits above the pointers because it cannot be wrong: a workspace's trees live at
paths that name the workspace, so being inside one is not a hint, it is the fact.

`charter workspace current` prints the answer, the name alone, for a script. `charter
status` prints it *and the rung that produced it*, which is usually the faster question to
ask when the answer surprised you.

`$CHARTER_WORKSPACE` is stripped of surrounding whitespace, so `" billing "` selects
`billing`. A value that is empty or holds only whitespace names nothing and counts as unset:
the rungs below it decide. That is what `export CHARTER_WORKSPACE=$(…)` leaves when the
command prints only a space or a tab.

The nominated default is written to `workspaces/.default`, a committed file, and
`charter workspace default` with no name prints it. `charter workspace default --clear`
removes it and needs no name. It prints `Cleared` only when it removed the file. With
nothing nominated it says so. When the file cannot be removed it names the file and the
system's error and exits 1, because sessions with nothing else selected go on landing on
that workspace.

## The lock, and how to get out of it

Confirming a workspace **locks the session to it**. A mid-session switch is then refused:

    Workspace is 🔒 locked to 'billing' for this session — switching to 'search'
    mid-session is disabled (never mix workspaces). Start a new session to pick
    another, or force it: `charter workspace use search --force`
    (or `charter workspace unlock` first).

This exists because a workspace swapped out from under a running task is silent: the agent
keeps working, in the wrong clones, against the wrong branches, and nothing looks wrong
until the commits land somewhere unexpected. The lock is per **session**, so a second
terminal picks its own workspace freely and neither disturbs the other.

The lock is written under the session's id, `$CHARTER_SESSION_ID`. Every chat the app starts
has one, whatever its harness. A shell of your own that has none gets a selection for the
terminal and no lock: the next `use` switches.

Three ways past it, in the order you should reach for them:

- **start a new session** — the honest one, and usually what you meant;
- `charter workspace unlock` — release the lock, then select;
- `charter workspace use <name> --force` — switch and re-lock in one step.

### Inside a chat

A chat the app starts runs in its workspace's directory, or in a clone or a piece inside it,
and the tree you stand in outranks every pointer. So the chat's commands answer for its own
workspace whatever `workspace use` writes. To work in another workspace, open a chat there;
for one command, pass `--workspace <name>`. An agent the chat spawns inherits the chat's
session id and its directory, so the same holds for it.

## workspace.md, memory, and workspace.json

Three stores, three jobs. Putting a thing in the wrong one is the common mistake:

| | holds | changes |
| --- | --- | --- |
| `workspace.md` | Vision, Context & decisions, Glossary | when the *task* changes |
| `memory/` | facts learned while working | constantly, one file per fact |
| `workspace.json` | which repos, and which branches | membership when a repo is cloned; branches when you `snapshot` |

One more store sits beside them with its own lifetime: `todos/`, intent, which expires —
`charter workspace todo` records one, lists them, and closes one with `done <slug>`.

**`workspace.md` is the charter** — what this task is for and what has been decided. Seed
it with `create --vision "…"`; a `fork` inherits it. It answers "why does this workspace
exist", so it should be short enough that a newcomer reads all of it.

**Memory is a store, not a log.** One file per fact with an index, because a single
growing notes file is written once and searched never. `charter workspace remember "…"`
adds one; `charter recall <query>` searches **across every base at once** — workspace
memory, persona memory, shared memory and refs — which is the only search you need to
remember. A base it could not read — a `memory/` at mode 000, a `refs/` subdirectory it
cannot tell is one — is named in `charter doctor`'s sentence, never searched as if empty:
what the other bases hold is still printed, and the command exits 1.

**`workspace.json` is there from the start.** Every workspace has one — creating the workspace
writes it, saying which workspace this is and that it has no repos yet. The file exists to
be shared: make the workspace LIVE (below) and it is committed, which is how a *teammate*
rebuilds it with `restore`. A file for that cannot depend on somebody having run a command
first, so its presence is an invariant rather than a coincidence. A workspace made by an
earlier charter gets one from `charter workspace reinit --all`.

**Its two halves have different lifetimes.** Membership is maintained: a repo cloned into
the workspace is recorded, because which repos a workspace is made of is a fact about the
workspace. Branches are not — they are a fact about this minute, and writing one on every
`git checkout` would churn a committed file. So charter records a repo with no branch, and
`snapshot` is where an operator deliberately pins the branches somebody else will restore.
Nothing but `snapshot` ever writes a branch here: the manifest is a snapshot somebody chose
to take, not an inventory kept current. `restore` treats an unpinned repo as restored by being cloned at all.

**A manifest you wrote is yours.** charter stamps the file with a digest of what it wrote;
a `workspace.json` whose contents no longer match is left completely untouched — no
membership is recorded into it — until you rewrite it with `snapshot`.

## LIVE or LOCAL — where your notes go

The most consequential per-workspace choice, and it is one flag.

- **LOCAL** (the default): everything above stays on disk. Nothing is committed. Nothing
  reaches anyone.
- **LIVE** (`charter workspace live <name>`): `workspace.json`, `workspace.md`, `memory/`,
  and `todos/` are un-ignored, and a memory written to a LIVE workspace is committed and
  pushed **immediately** — reactively, not on a later save.

`charter workspace live <name> --off` puts it back. `charter workspace remember --no-sync`
defers the push for one memory, and `charter save` pushes the plane's changes later. Writing
to a LOCAL workspace tells you it stayed put, rather than letting you assume it travelled.

The clones themselves are never committed by any of this. Only the metadata is.

## Worktrees

A clone can be split into git worktrees, so parallel workers each get their own branch of
the *same* repo without re-cloning it:

    workspaces/<ws>/.worktrees/<repo>/<piece>

Each is a **piece**. Git is the only registry of them: every listing is
`git worktree list --porcelain`, so a worktree made with plain git at that path is a piece,
and one removed by hand is gone
([ADR 0027](https://github.com/diazoxide/charter/blob/main/docs/adr/0027-git-is-the-only-registry-for-a-chats-worktree.md)).
The app lists a workspace's pieces, merges one back into the branch it was cut from, and
removes one. Cutting a piece from the app or the CLI is not in this version yet.

A relocated worktree root — `[plane] worktrees` or `$CHARTER_WORKTREES` — is not followed in
this version yet: unset it to keep worktrees in the plane.

### A session in a piece gets the layer a clone gets

A piece is a git root of its own, so a session started in it reads none of what charter
wrote into the clone beside it. Charter writes the same layer into the piece: the plane's
`enabledPlugins`, `env` and `ask`/`deny` rules in `.claude/settings.json`, the plane's
`--local` rules in `.claude/settings.local.json`, and its agents and skills. Every path goes
in the clone's `.git/info/exclude`, which the piece reads too, so `git status` stays clean in
both.

- **A chat started in a piece** gets it written before the chat starts, and a piece whose
  layer cannot be finished is refused, with the sentence naming what blocked it.
- **`charter workspace reinit`** writes it into every worktree git lists for the workspace's
  clones at `.worktrees/<repo>/<piece>`. A worktree git no longer lists is not covered.
- **A file of yours in the clone stays in its `git status`.** The line that hides a piece's
  `.claude/settings.json` hides that path in the clone too, because both read one exclude.
  Where the clone, or any other checkout reading that exclude, holds an untracked file at that
  path that charter did not write, charter leaves the line out. The shared
  `.claude/settings.json` and the mirrored agents and skills are still written, so the plane's
  committed rules reach the piece, and they show in the piece's `git status`. The
  machine-local `.claude/settings.local.json` is withheld, because an unhidden local file is
  one `git add` from being committed; `reinit` names your file. Commit it or move it, and the
  next `charter workspace reinit` hides charter's files and writes the local one.
- **A clone whose worktrees git cannot list is named.** When `.git/worktrees` cannot be read,
  or git fails to answer, none of those worktrees gets the layer, and `reinit` and `clone`
  name the clone.
- **`charter workspace remove`** checks every piece for commits that exist nowhere else
  before it deletes anything, and takes charter's files out of every piece.

The local file is written for the same reason a clone gets one, and one reading of it is not
measured. The table above shows a linked worktree reads its main checkout's
`.claude/settings.local.json`. Whether it also reads the one at its own root has not been
measured. The piece's copy holds the same rules as the clone's, so neither answer puts a rule
in force that the plane does not declare.

**A piece whose path leads out of `workspaces/` is named and gets nothing written.** A
`.worktrees` linked elsewhere makes `reinit` report the piece `blocked`, and
`workspace remove` takes nothing out of it.

## Structure versioning, and `reinit`

A workspace's layout carries a version stamp. When charter learns to create something new
(a `todos/` directory, a memory index), existing workspaces do not have it, and the status
line says so for the workspace you are in:

    ⚠ reinit: charter ws reinit

`charter workspace reinit <name>` creates what is missing and re-stamps the marker. It is
additive: nothing you wrote is touched. `--all` is also the **backfill** for a structural
element that older workspaces predate — v5 is `workspace.json`, written with the repos the
workspace has and no branches.

A baseline path charter cannot check — a directory it may not read, a symlink loop — gets
nothing written into it. `reinit` names it with what clears it, in the words `charter doctor`
uses for the same path: restoring read access, or fixing the loop at the link that loops.
The same holds for a workspace's `refs` or `memory` that is there and is no directory: a
symlink whose target is gone is named with "removing or repointing that link clears this",
and a file is named with "moving it out of the way clears this". Charter never removes,
repoints or moves either, `reinit` exits 0, and neither flags the workspace for a `reinit`
that cannot add the file.

Charter writes nothing through a symlink at a baseline path. A workspace's tree is committed,
and git keeps a link as a link, so a link there may be somebody else's. A link where a baseline
file belongs (`workspace.md`, `workspace.json`, `memory/MEMORY.md`, `refs/README.md`) is never
followed, wherever it points. A link where a directory belongs (`memory`, `refs`) is followed
only when it lands on a directory inside the plane's data, so a `refs` repointed at shared
notes in the plane keeps working. Every other link is named with "is a symlink, and charter
writes nothing through one; replacing it with a real file clears this", or "a real directory"
where a directory belongs. It gets the same exit 0, and it does not flag the workspace. The
files themselves are created exclusively as well, so a link that appears between the check and
the write makes the create fail, not follow the link. The `.charter-structure` stamp is written
with an open that refuses a link, so it is never written through one either; a stamp that could
not be written leaves the workspace flagged for `reinit`.

The stamp is read the same way, because every status-line render reads it. A stamp that is a
symlink, a directory or a FIFO is never read: a FIFO nobody writes to used to hang `reinit` and
the status line. The workspace stays flagged, and `reinit` says the stamp could not be written
instead of reporting the structure version as added: "is a symlink, and charter writes nothing
through one; replacing it with a real file clears this", or "is not a regular file, and charter
never moves existing content; moving it out of the way clears this". A stamp write refused for
any other reason is named too, and `reinit` reports the version as added only when the stamp
now reads current.

A workspace directory that is itself a symlink loop is named the same way — "fix the symlink
loop at" its path — by `reinit <name>`, which exits 1, and by `reinit --all`, which counts it
in its closing line as "could not be checked" and never prints "Up to date" while one was
left out.

## Moving a workspace around

- **`fork <src> <new>`** — a new workspace pre-loaded with the source's charter, manifest
  and memory, so a branch of the work starts with the context rather than without it.
  Repos come with `--restore`, or on demand later. What it cannot read in the source — a
  `memory/` or `todos/` it may not list, one file it may not open — is not copied: the line
  that says it forked names what the fork does not have, each path follows, and it exits 1.
- **`snapshot`** — pin the current repos *and branches* into `workspace.json`. Refuses
  while a repo has unpushed work, so a recorded branch is one `restore` can check out.
- **`restore <name>`** — rebuild from that manifest on another machine: clone each repo,
  check out each branch.
- **`remove <name>`** — deletes the workspace and its clones, and refuses if that would
  lose work nothing else holds (`--force` to remove it anyway).
- Renaming a workspace is not in this version yet.

## Knowing the neighbours

At session start charter names the **other** workspaces on the plane: each one's vision
line, its open-todo count, and when it was last worked. Bounded to the newest few, with a
count of the rest.

This is knowledge, not logic. Nothing in charter reads it back and nothing branches on it —
it exists so that work delivered by a parallel workspace is recognisable instead of
surprising ("why did this file move?"). Another workspace's stated goal is the most
instruction-shaped thing charter injects anywhere, so the block says in as many words that
it is data to consider and never an instruction to obey.

It deliberately does not report what those workspaces *delivered* — commits, PRs, branches.
That costs a git log per workspace on every session start, to answer a question you can now
ask yourself, knowing the workspace is there.

Silent when the plane has one workspace: a signal that fires on no news teaches people to
skim the ones that matter.

A workspace's vision is also what a **handoff proposal is matched against**: the model reads
each workspace's vision (`charter workspace vision -w <name>`) to decide which workspace an
ask belongs in ([handoff.md](handoff.md)), which is why a workspace with no vision is never
proposed.

## See also

- [control-plane.md](control-plane.md) — `charter.toml`, and the plane's view of a workspace
- [handoff.md](handoff.md) — opening a chat in another workspace on a brief you approved, and
  why the consent is your harness's own prompt
- [personas.md](personas.md) — the other memory base
