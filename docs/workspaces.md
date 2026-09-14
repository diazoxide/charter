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

`default` always exists. `charter workspace create <name> --use` starts another.

**"Always exists" is about the name, not about the directory.** `default` is the rung the
resolution below terminates on, so a plane that has never selected anything is standing in
it; `charter init` does not create `workspaces/default/`, and the first command that puts
something in it does (`charter clone -w default`, `charter workspace use default`,
`charter workspace create default`). `charter workspace list` lists it either way, with
`—` for its repos, because a table that draws a "you are here" mark has to have a row for
where you are — and because the `F2` picker inside a frame offers exactly the same names.
The name is `[workspace] default` if your `charter.toml` sets one.

## A chat standing here gets charter

Claude Code reads project settings from the session's working directory and **does not walk
up** for them. A chat launched in `workspaces/<name>/` — which is where the `+` and every
workspace tab put it — would therefore get no plugin and no `$CHARTER_HARNESS`, while its
agents and skills arrived anyway, because those *do* walk up and this directory is not a
git boundary.

So charter generates `.claude/settings.json` here, at launch: the plane's own
`enabledPlugins` and `env`, plus the **restrictive half** of its `permissions`. Skills come
with the plugin and agents already walk up; a second copy of either would shadow the
plugin's.

`statusLine` was a third key mirrored here until 0.57.0. Charter no longer writes one
into any settings file, so there is nothing of charter's under that key to carry sideways
— and a `statusLine` an operator wired themselves stays where they put it.

### Your `guard ask` rules come with it

`permissions.ask` and `permissions.deny` travel. **`permissions.allow` never does.** A grant
copied sideways puts a permission in force in a directory nobody clicked for it in; a
restriction is the opposite — it adds a prompt or a refusal and can make nothing run — so
leaving it behind was what made `charter guard ask 'terraform apply *'` *not* prompt in the
workspace chat that would run `terraform apply`, while the command said the rule applied to
everyone on the repo.

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

So the local file is read at the git root as well as in the starting directory — the docs
date that to 2.1.211 — and the shared file is not. `workspaces/<name>/` sits inside the
plane's own repository, so the plane's `.claude/settings.local.json` is already in force
there and charter generates no copy. A clone is a git root of its own, so it gets a
generated `.claude/settings.local.json` holding the plane's local `ask` and `deny` — separate
from the shared file, so a decision that is yours on this machine does not arrive looking
like the team's.

**In a clone that file is shared with Claude Code**, which saves "Yes, and don't ask again"
into it. Charter writes the clone's `.git/info/exclude` entry *before* the file, and writes
no local file at all where the exclude cannot be written: a machine-local rule it cannot hide
would be committable, and `guard ask`, `workspace reinit` and `clone` each say so in a
sentence of its own. Once Claude Code has added its own approvals, charter keeps the file
hidden for as long as it exists, never rewrites it and never merges into it — and "hidden"
holds across the repository: a clone and its linked worktrees read one `.git/info/exclude`,
so charter's block there lists what every one of them needs, and the local file's line stays
while that file exists in any of them, whatever charter's own records say. Once the file is
gone and the plane no longer declares it, its line goes. `doctor` only mentions it once the
plane has moved on since — the plane's newer rules are then not in that file — and never
suggests deleting a file that holds your approvals, nor one it cannot read.

**A file charter wrote in a clone stays hidden while it is there**, whatever charter's own
records say — a generated file, its `.charter-generated` record, a temp file an interrupted
write left — in every checkout charter wires that reads the same `.git/info/exclude`. That
includes a generated file you have since rewritten: charter never overwrites it, `doctor` names
it `foreign`, and if it is your own file and you mean to commit it, `git add -f` it. A line goes
only once its path is confirmed absent in all of those checkouts and the plane no longer
declares it. While git cannot list the worktrees in time, or lists one charter cannot look into,
every line stays, and so does a line whose path cannot be checked; a worktree git calls
prunable is advised `git worktree prune` only when charter itself finds it gone.

Charter writes each of those files whole — to a temp file beside it, flushed to disk, then
renamed over it — so a kill leaves the old content or the new and never half of either. A temp
file a kill leaves behind is named `.charter-generated.<pid>.<random>.tmp`, and the block hides
that pattern anywhere in the checkout, so a file of your own whose name matches it
(`.charter-generated.notes.tmp`) is hidden too. Before rewriting a generated file charter
records the write as pending, and it overwrites only content one of its records lists; anything
else is yours or the harness's. Where the record cannot be published — a checkout root that is
not writable, a read-only or full disk — charter writes nothing there and keeps the lines.
`doctor`'s `workspace layer` row calls the first kind of block `unaccounted` and that marker
`unrecorded`, with the error the publish failed with, and says what clears each. A chat that
starts in a workspace or a clone where one of the plane's ask/deny rules is not in force is
told which rules are missing, and what fixes it.

`charter guard ask` refreshes every workspace's layer as it writes, so the rule is in force
before the command returns rather than at the next launch. It runs both ways: drop a rule
from the plane's settings and the mirror of it is withdrawn — a file charter generated and
no longer generates is removed, but only while its content still matches what charter wrote.
A plane settings file that does not parse is not a plane that declares nothing: every
workspace keeps its last good copy, and `doctor`'s `workspace layer` row names the file.

Codex has no command-pattern permissions at all, so there is nothing to carry there and
`guard ask` already says so. **opencode has one gap that stands**: it resolves `opencode.json`
at the repository root, so a workspace *directory* already resolves to the plane's own copy
and the rule is in force — but a clone at `workspaces/<name>/<repo>/` is a repository root of
its own, and charter generates no `opencode.json` there. An opencode session rooted inside a
clone does not have the plane's rules. Charter does not mirror the plane's file into a repo
it does not own, because that file also holds grants; a *generated* checkout `opencode.json`
carrying only the restrictive half is the honest way to close it and is not built.

It is **charter's file, and only while it stays charter's**. A `.charter-generated` sidecar
records a hash of what charter wrote. A file that still matches is refreshed when the
plane's settings move; one that does not is yours — left completely untouched, never
repaired, and named by `charter doctor`'s `workspace layer` row. `charter workspace reinit`
(or `--all`) is the repair.

**A clone gets the layer too, and pays a cost this directory does not.**
`workspaces/<name>/<repo>/` is a repo of its own, so the walk-up that carries agents and
skills into a workspace directory stops at its root and a session there got none of the
layer — not the settings, and not the personas either. Charter writes both, and registers
every path it generated in that checkout's **`.git/info/exclude`**: per-checkout, never
committed, not itself tracked, and the one file a guest may write. Charter never edits the
clone's `.gitignore`, hides only the exact paths it wrote (never a `.claude/` glob, which
would take your own untracked files with it), never touches a file it did not generate, and
removes its files and its exclude block when the workspace goes. `git status` in your repo
is unaffected. Linked worktrees included — a piece `charter wt add` cut as well as a clone (see
*A session in a piece gets the layer a clone gets* below) — and their `info/exclude` is the main
repo's, which is also why removal is not just a `rm -rf`, and why removing a workspace takes away
only the lines no other checkout of that repository still needs. **`CLAUDE.md` is deliberately left behind**: a guest
hides its own files, it does not narrate the host's.

**And it is every harness's layer, not Claude Code's.** What a clone cuts off is spelled by
each harness, measured against the installed binary: `.claude/agents/` and `.claude/skills/`
(Claude Code 2.1.259), `.opencode/agent/` (opencode 1.18.23), `.codex/skills/` (codex-cli
0.147.0). A harness registered tomorrow is carried the day it declares a surface. What
travels is **capability**, never a grant: `opencode.json` is read at a repository root and is
carried by nothing, because `charter guard` keeps this plane's permission rules in it and
copying those sideways would put an `allow` in force in a repo nobody granted it in — the
same reason `.claude/settings.json`'s mirror is a list of keys and not the file. A mirror
cannot drop a key, which is the whole difference between the two mechanisms: charter
*generates* the Claude Code settings and can therefore carry the restrictive half of
`permissions` and leave the grants behind, and it *copies* `.opencode/agent/`, which it
cannot.

That is about a **clone**. A workspace **directory** is a different question, and there the
ceiling stands: only Claude Code binds config to the directory a session starts in.
opencode's project config is keyed to the repository root, and Codex has no project config
file at all — so on both, charter's layer is already live in every workspace and, by the
same fact, cannot be made to differ between two of them. `charter harness list` names that
ceiling.

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

`charter workspace current` prints the answer *and the rung that produced it*, which is
usually the faster question to ask than "which workspace am I in".

`$CHARTER_WORKSPACE` is stripped of surrounding whitespace, so `" billing "` selects
`billing`. A value that is empty or holds only whitespace names nothing and counts as unset:
the rungs below it decide (#1055). That is what `export CHARTER_WORKSPACE=$(…)` leaves when
the command prints only a space or a tab. Before, it hid every rung below it and the session
fell to `default`, `charter workspace use` warned that `' '` takes precedence, and neither
the session briefing nor the launch picker asked for a workspace, because the variable
counted as a pin. `charter workspace current` says when it ignored one, after the rung that
decided:

```
billing
• LOCAL (private) · resolved via session, 🔒 locked for this session
! $CHARTER_WORKSPACE is set but holds only whitespace, so charter ignored it and the rungs below it decided. Unset it, or set it to the workspace you meant.
```

An empty `$CHARTER_WORKSPACE` is not reported, because a frame starts every chat with the
variable set and empty when the launch pinned no workspace.

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

**A Codex session outside a frame has no lock.** The lock is written under the session's
id, and no per-session id reaches a Codex shell, so `charter workspace use` there selects a
workspace for the terminal and locks nothing: the next `use` switches. The session briefing
still asks for a workspace, and where it sees no session id and `$CHARTER_HARNESS` names
Codex, it does not say confirming locks it. Inside a frame the chat's
id reaches the shell, and the chat is locked as described under *Inside a chat* (#954).

Three ways past it, in the order you should reach for them:

- **start a new session** — the honest one, and usually what you meant;
- `charter workspace unlock` — release the lock, then select;
- `charter workspace use <name> --force` — switch and re-lock in one step.

### Inside a chat

A chat in a charter frame is locked to the workspace it was launched in. It makes no
difference whether you picked that workspace at the launch prompt or it came from
`--workspace`, a pointer or a reopen. There is nothing to confirm, and the session briefing
does not ask. `charter workspace use <other>` there is refused with a different message:

    Workspace is 🔒 locked to 'billing', the workspace this chat was launched in.
    Switching its commands to 'search' would leave the chat itself in 'billing'
    (a chat belongs to its workspace for life). To work in another workspace, open
    a chat there: its tab on the workspace strip, or F2 → workspace. For one
    command, pass `--workspace search`.

Of the three ways past the lock above, one changes and one is gone:

- **start a new session** becomes **open a chat in that workspace**;
- `charter workspace unlock` refuses, because the lock is the chat's launch record and
  there is nothing to release;
- `charter workspace use <name> --force` still moves the chat's **commands**. It never
  moves the chat, and `charter workspace use <own>` takes you back with no second `--force`.

An agent the chat spawns inherits the chat's session id, so the same lock holds for it: the
pointer its `workspace use` would write is the chat's own. It works by the directory it
stands in, which outranks every pointer, or by `--workspace` on each command. See
`docs/frame.md` for why (#936).

Two limits:

- **A pinned launcher pins the chat.** If `$CHARTER_WORKSPACE` was set in the shell that
  opened the chat, the chat is locked to that pin rather than to the workspace the launch
  resolved — including one named with `--workspace`. That is the precedence the pin has
  everywhere else in charter.
- **Not opencode, yet.** Its plugin replaces `$CHARTER_SESSION_ID` with opencode's own
  session id, so inside a frame charter cannot tell which chat it is in. An opencode chat
  holds no lock, is still asked to confirm a workspace, and `charter workspace use <other>`
  in it still succeeds (#946).

## workspace.md, memory, and workspace.json

Three stores, three jobs. Putting a thing in the wrong one is the common mistake:

| | holds | changes |
| --- | --- | --- |
| `workspace.md` | Vision, Context & decisions, Glossary | when the *task* changes |
| `memory/` | facts learned while working | constantly, one file per fact |
| `workspace.json` | which repos, and which branches | membership when a repo is cloned; branches when you `snapshot` |

Two more stores sit beside them with their own lifetimes: `todos/` (intent, which expires —
[ADR 0004](adr/0004-intent-is-its-own-store.md)) and `changes/`, one file per cross-repo
change: which repositories are in it, which branch in each, which must land before which,
and which were considered and left out. A change's name outlives the work — it is in a merge
commit's trailer in five repositories forever — which is why it is not a todo. See
[changes.md](changes.md).

**`workspace.md` is the charter** — what this task is for and what has been decided. Seed
it with `create --vision "…"`; a `fork` inherits it. It answers "why does this workspace
exist", so it should be short enough that a newcomer reads all of it.

**Memory is a store, not a log.** One file per fact with an index, because a single
growing notes file is written once and searched never. `charter workspace remember "…"`
adds one; `charter recall <query>` searches **across every base at once** — workspace
memory, persona memory, shared memory and refs — which is the only search you need to
remember.

**`workspace.json` is there from the start.** Every workspace has one — `workspace.ensure`
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
Nothing but `snapshot` ever writes a branch here; see ADR 0010 for why that line is where it
is. `restore` treats an unpinned repo as restored by being cloned at all.

**A manifest you wrote is yours.** charter stamps the file with a digest of what it wrote;
a `workspace.json` whose contents no longer match is left completely untouched — no
membership is recorded into it — until you rewrite it with `snapshot`.

## LIVE or LOCAL — where your notes go

The most consequential per-workspace choice, and it is one flag.

- **LOCAL** (the default): everything above stays on disk. Nothing is committed. Nothing
  reaches anyone.
- **LIVE** (`charter workspace live <name>`): `workspace.json`, `workspace.md`, `memory/`,
  `todos/` and `changes/` are un-ignored, and a memory written to a LIVE workspace is
  committed and pushed **immediately** — reactively, not on a later save.

`changes/log/` is un-ignored by neither switch and is committed **never**, exactly as
`pieces/` is not. It is the landing declaration — *charter merged this commit, for this
change* — per host, describing merges made from one disk; a portable file describing a
local reality is the mismatch [ADR 0010](adr/0010-the-manifest-is-a-snapshot-not-an-inventory.md)
dissects.

`charter workspace live <name> --off` puts it back. `charter workspace save` is the manual
counterpart for a LIVE workspace whose writes were deferred with `--no-sync`. Writing to a
LOCAL workspace tells you it stayed put, rather than letting you assume it travelled.

The clones themselves are never committed by any of this. Only the metadata is.

## Worktrees

A clone can be split into git worktrees, so parallel workers each get their own branch of
the *same* repo without re-cloning it:

    workspaces/<ws>/.worktrees/<repo>/<piece>

Each is a **piece** — one unit of work whose creation *is* the claim, because git already
arbitrates who wins the path. See [adr/0011](adr/0011-the-record-holds-only-what-git-cannot-know.md).

### A session in a piece gets the layer a clone gets

A piece is a git root of its own, so a session started in it — where `charter wt add` tells you
to start one — reads none of what charter wrote into the clone beside it. Charter writes the same
layer into the piece: the plane's `enabledPlugins`, `env` and `ask`/`deny` rules in
`.claude/settings.json`, the plane's `--local` rules in `.claude/settings.local.json`, and its
agents and skills. Every path goes in the clone's `.git/info/exclude`, which the piece reads too,
so `git status` stays clean in both.

- **`charter wt add`** writes it before printing the `enter:` line.
- **A launch and `charter workspace reinit`** write it into every worktree git lists for the
  workspace's clones at `<root>/<repo>/<piece>`. That covers a worktree made with plain git at
  that path and one an older charter cut. `<root>` is `.worktrees/`, or `<root>/<ws>/` under
  `[plane] worktrees` or `$CHARTER_WORKTREES`, and pieces cut under `.worktrees/` before the root
  moved are still covered. A worktree git no longer lists, or one of another workspace's roots,
  is not.
- **`charter doctor`'s `workspace layer` row** lists a piece's files as
  `<ws>/.worktrees/<repo>/<piece>/<file>`, or by absolute path under a relocated root. A chat
  started in a piece is told which of the plane's ask/deny rules are not in force there.
- **A file of yours in the clone stays in its `git status`.** The line that hides a piece's
  `.claude/settings.json` hides that path in the clone too, because both read one exclude. Where
  the clone, or any other checkout reading that exclude, holds an untracked file at that path that
  charter did not write, charter leaves the line out. What happens to charter's file then depends
  on the file. The shared `.claude/settings.json` and the mirrored agents and skills are still
  written, so the plane's committed rules reach the piece, and they show in the piece's
  `git status`. The machine-local `.claude/settings.local.json` is withheld (`doctor` shows
  `withheld`), because an unhidden local file is one `git add` from being committed. `wt add`,
  `reinit` and `doctor` (`.git/info/exclude (unhidden)`) name your file. Commit it or move it, and
  the next `charter workspace reinit` hides charter's files and writes the local one. Until then
  `charter wt remove` counts a shown file as uncommitted work in the piece and needs `--force`.
- **A clone whose worktrees git cannot list is named.** When `.git/worktrees` cannot be read,
  or git fails to answer, none of those worktrees gets the layer. `doctor`
  (`<clone>/.git/worktrees (unlisted)`), `reinit` and `clone` name the clone and what clears it.
- **`charter workspace remove`** takes charter's files out of every piece first. Under a
  relocated root the piece directory outlives the workspace, and it keeps nothing of charter's.

The local file is written for the same reason a clone gets one, and one reading of it is not
measured. The table above shows a linked worktree reads its main checkout's
`.claude/settings.local.json`. Whether it also reads the one at its own root has not been
measured. The piece's copy holds the same rules as the clone's, so neither answer puts a rule in
force that the plane does not declare. `doctor` and the chat line judge a piece by its own copy.

**A piece whose path leads out of its base is named and gets nothing written.** For a piece
under `.worktrees/`, the base is `workspaces/`. For a piece under a relocated root, the base is
that root itself, not `<root>/<ws>`. The root is the directory you set, or that charter checked
against the plane before using it, while `<root>/<ws>` is a directory anyone can replace with a
link. A `.worktrees` or a `<root>/<ws>` linked elsewhere makes `wt add` warn that the layer was
not written. `reinit` then reports the piece `blocked`, `doctor` reports it `foreign`, and
`workspace remove` takes nothing out of it.

## Structure versioning, and `reinit`

A workspace's layout carries a version stamp. When charter learns to create something new
(a `todos/` directory, a memory index), existing workspaces do not have it, and the status
line says so:

    ⚠ reinit 3 ws · charter ws reinit --all

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
  Repos come with `--restore`, or on demand later.
- **`snapshot`** — pin the current repos *and branches* into `workspace.json`. Refuses
  while a repo has unpushed work, so a recorded branch is one `restore` can check out.
- **`restore <name>`** — rebuild from that manifest on another machine: clone each repo,
  check out each branch.
- **`rename <old> <new>`** — moves the clones and the memory, and commits the move if the
  workspace is LIVE. A linked worktree records its clone by absolute path, and the clone
  records it back the same way, so after the move charter runs `git worktree repair` from
  each moved clone and reads both links back. That covers pieces under `.worktrees/`, a
  worktree you made by hand inside the workspace, and one outside it that did not move. A
  worktree git still cannot follow is named with the command that repairs it, `git -C
  <clone> worktree repair <tree>`, and the rename is not undone. Until then git calls that
  worktree prunable, and `git worktree prune` deletes its record while it may hold
  uncommitted work.

  With `[plane] worktrees` or `$CHARTER_WORKTREES` set, a piece lives at
  `<root>/<workspace>/<repo>/<piece>`, so that directory is named after the workspace too.
  The rename moves `<root>/<old>` to `<root>/<new>` along with `workspaces/<old>`, then
  relinks as above. Before either move, it checks that both `workspaces/<new>` and
  `<root>/<new>` are free, and if either one is taken it refuses and renames nothing:

  ```
  ✗ <root>/<new> already exists, and it is where the worktrees of a workspace named '<new>' live — pick another name or move it first; nothing was renamed.
  ```

  If `workspaces/<old>` moved but `<root>/<old>` could not, the rename stays and the output
  has no ✓. Charter names the directory it left behind with the OS's reason, prints the
  command that finishes the job, and exits 1:

  ```
  ! Renamed workspace '<old>' → '<new>' (clones, memory, and manifest moved), but not its worktrees: <root>/<old> is still at the old name (Cross-device link).
  ! Finish the rename: mv <root>/<old> <root>/<new> && git -C <clone> worktree repair <root>/<new>/<repo>/<piece>
  ```
- **`remove <name>`** — deletes the workspace and its clones, and refuses if that would
  lose unpushed work.

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

`charter workspace list` shows the same line, on every workspace, in a `VISION` column:

```
  WORKSPACE  MODE   CLONES  REPOS          VISION
* billing    local  2       api, webhooks  Bill every org correctly and on time
  default    local  0       —              —
  docs-site  local  1       site           —
```

That column is what a **handoff proposal is matched against** — the model reads these
visions to decide which workspace an ask belongs in ([handoff.md](handoff.md)), which is
why a workspace with no vision is never proposed and shows a dash instead. It is the
trailing field and it is not truncated: matching an ask against half a sentence is matching
against half the evidence. It is the vision's first line only, escaped, for the reason every
committed value charter prints is — a newline in one would draw a row charter did not.

## See also

- [control-plane.md](control-plane.md) — `charter.toml`, and the plane's view of a workspace
- [handoff.md](handoff.md) — `charter handoff`: opening a chat in another workspace on a brief
  you approved, and why the consent is your harness's own prompt
- [personas.md](personas.md) — the other memory base, and how a persona is dispatched
- [adr/0010](adr/0010-the-manifest-is-a-snapshot-not-an-inventory.md) — why `workspace.json` is not an inventory
