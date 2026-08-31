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

`default` always exists. `charter workspace create <name> --use` starts another.

**"Always exists" is about the name, not about the directory.** `default` is the rung the
resolution below terminates on, so a plane that has never selected anything is standing in
it; `charter init` does not create `workspaces/default/`, and the first command that puts
something in it does (`charter clone -w default`, `charter workspace use default`,
`charter workspace create default`). `charter workspace list` lists it either way, with
`—` for its repos, because a table that draws a "you are here" mark has to have a row for
where you are — and because the `F2` picker inside a frame offers exactly the same names.
The name is `[workspace] default` if your `charter.toml` sets one.

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

Three ways past it, in the order you should reach for them:

- **start a new session** — the honest one, and usually what you meant;
- `charter workspace unlock` — release the lock, then select;
- `charter workspace use <name> --force` — switch and re-lock in one step.

## workspace.md, memory, and workspace.json

Three stores, three jobs. Putting a thing in the wrong one is the common mistake:

| | holds | changes |
| --- | --- | --- |
| `workspace.md` | Vision, Context & decisions, Glossary | when the *task* changes |
| `memory/` | facts learned while working | constantly, one file per fact |
| `workspace.json` | repos + branches | when you `snapshot` |

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

**`workspace.json` is a snapshot, not an inventory.** It records what was there when you
asked, and is what `restore` rebuilds from. It does not track drift; see ADR 0010.

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

## Structure versioning, and `reinit`

A workspace's layout carries a version stamp. When charter learns to create something new
(a `todos/` directory, a memory index), existing workspaces do not have it, and the status
line says so:

    ⚠ reinit 3 ws · charter ws reinit --all

`charter workspace reinit <name>` creates what is missing and re-stamps the marker. It is
additive: nothing you wrote is touched.

## Moving a workspace around

- **`fork <src> <new>`** — a new workspace pre-loaded with the source's charter, manifest
  and memory, so a branch of the work starts with the context rather than without it.
  Repos come with `--restore`, or on demand later.
- **`snapshot`** — write the current repos and branches into `workspace.json`.
- **`restore <name>`** — rebuild from that manifest on another machine: clone each repo,
  check out each branch.
- **`rename <old> <new>`** — moves the clones and the memory, and commits the move if the
  workspace is LIVE.
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

## See also

- [control-plane.md](control-plane.md) — `charter.toml`, and the plane's view of a workspace
- [personas.md](personas.md) — the other memory base, and how a persona is dispatched
- [adr/0010](adr/0010-the-manifest-is-a-snapshot-not-an-inventory.md) — why `workspace.json` is not an inventory
