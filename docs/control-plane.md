# The control plane: `charter.toml`

A **control plane** is any directory marked by a `charter.toml` file. `charter` finds it
by walking up from your current directory — the same contract git, cargo, and npm use —
so once you're anywhere inside one, every command just works. There is nothing else
special about the directory: no required name, no fixed location. `charter init` creates
one from nothing (see the README's quickstart).

A fresh `charter init` writes the minimal file:

```toml
schema = 1

[[forge]]
kind = "gitlab"

[memory]
share = "local"
```

## Every key, in full

```toml
# Layout version this file was written for. `charter` refuses to run against a
# schema NEWER than it understands (upgrade the CLI instead of guessing); it has no
# problem reading an OLDER schema. Omit it and 1 is assumed.
schema = 1

# Which of charter's two deployments this plane is. See "Fleet and embedded" below.
[plane]
shape = "fleet"                  # "fleet" | "embedded". Default: "fleet".
worktrees = "../plane.worktrees" # Where worktrees live. A relative path resolves against
                                  # this file's directory; $CHARTER_WORKTREES overrides it.
                                  # Default: fleet keeps them per-workspace under
                                  # workspaces/<ws>/.worktrees/; embedded puts them in a
                                  # SIBLING of the repo — see "Where worktrees live" below.

# One [[forge]] block per code-hosting forge this control plane tracks. A single-forge
# control plane (the common case) declares exactly one; see "Mixed-forge" below for more
# than one. Each block is independent — its own owner, host, and excludes.
[[forge]]
kind = "gitlab"                  # "gitlab" | "github". Default: "gitlab".
group = "my-org"                 # the GitLab group (or GitHub org/user) this forge tracks.
                                  # `owner` is accepted as a synonym — see "group vs owner" below.
host = "gitlab.com"              # optional: a self-hosted forge's host (GitLab Enterprise,
                                  # GitHub Enterprise Server). Default: the forge's own public
                                  # host (gitlab.com / github.com). See docs/forges.md.
exclude = ["this-control-plane"] # repo names never written into the inventory — typically
                                  # the control plane's own repo, so `discover` doesn't list
                                  # itself as a clone target.

# How far a written memory (persona or workspace) travels by default. See "Memory
# posture" below — this is the single most consequential default in this file.
[memory]
share = "local"                  # "local" | "commit" | "push". Default: "local".

# The workspace selected when none is active yet (no --workspace, no $CHARTER_WORKSPACE,
# no prior `charter workspace use`).
[workspace]
default = "default"              # Default: "default".
```

### `group` vs `owner`

GitLab calls the thing a repo lives under a **group**; GitHub calls it an **org** (or a
personal **user** account). `charter` accepts either key name in a `[[forge]]` block —
`group` and `owner` mean the same thing — so you can use whichever reads naturally for
the forge in question. If a block sets both, `group` wins. `charter init --owner <name>`
always writes the field as `owner`, since it works for either forge kind.

## Fleet and embedded: `[plane].shape`

charter is deployed two ways, and a plane declares which one it is.

**`fleet`** (the default, and the original) — the control plane is its own directory. A
workspace holds many **clones**, one per repo the task touches, each of which can carry
worktrees. This is what the rest of this document assumes.

**`embedded`** — charter installed *inside* the single codebase it serves, which may
itself be a monorepo of backend, frontend and the rest. There is nothing to clone: the
tree you edit is the plane's own root, and a workspace holds **worktrees** of it rather
than clones. Teams reach for this when there is one repo, not a fleet, and they still
want personas, memory and a vault.

```toml
[plane]
shape = "embedded"
```

The status line follows the declaration. An embedded plane shows its own repo as the
first row — branch, dirty state, CI, open change — with each worktree on its own row
beneath it. A fleet plane shows the workspace's clones, exactly as before.

### Why this is declared rather than detected

A fleet plane's root is *also* a git repo in the normal case: its personas carry
committed memory, and the `exclude` example above exists precisely because the plane's
own repo lives on the forge. So the presence of `.git` cannot separate the two shapes —
in one it means "the repo I work in", in the other "the plane itself, which I do not".

Every filesystem signal that might substitute has the same defect in a worse place.
"No clones anywhere" and "empty inventory" both describe a *fresh fleet plane*, before
its first `discover` or `clone` — the moment a new user can least afford charter to
guess. What separates the shapes is intent, and `charter init` is where the intent
exists: running it inside a repo you already work in is the choice. So it is written
down, once, instead of re-derived forever.

An unrecognised value falls back to `fleet`, on the same principle as `[memory].share`
falling back to `local`: a typo should cost a feature, not rearrange something that works.

`charter init` decides it for you, because init is the only moment it is decidable: run
inside a directory that is already a git repo, it writes `shape = "embedded"` and says so;
run anywhere else, it writes nothing and you get a fleet plane. Afterwards the evidence is
gone — a fleet plane's root becomes a git repo too — which is exactly why it is recorded
rather than re-derived. `charter init --shape fleet|embedded` overrides the detection.

An embedded plane gets no `inventory/` directory: the inventory lists a forge owner's
repos so `clone` has targets, and an embedded plane clones nothing. `doctor` and
`reinit` both know this, so neither reports its absence as drift.

### Where worktrees live

Worktrees normally sit at `workspaces/<ws>/.worktrees/<repo>/<piece>` — deliberately
**outside every clone**, so that nx, jest and maven never recurse into them and a
`git clean -xfd` inside a clone cannot destroy live work.

That path assumes the clone and the control plane are different directories. In an
embedded plane they are the same directory, so the identical rule now requires leaving the
plane as well — otherwise a worktree is a complete second copy of the codebase sitting
inside the tree that every root-level glob walks. This is not hypothetical: charter's own
checkout, with two worktrees, answered a root-level test glob with 214 files of which 142
were duplicates. `.gitignore` hides that from git and from nothing else — pytest, jest,
nx, tsc and every IDE indexer read the working tree directly.

So an embedded plane defaults its worktrees to a **sibling of the repo**, `<root>.worktrees/`,
keeping the layout below it identical:

```
my-app/                                 the plane, and the codebase
my-app.worktrees/<workspace>/my-app/<piece>
```

Set `[plane].worktrees` (or `$CHARTER_WORKTREES`) to put them somewhere else. Only the
worktrees move — `workspaces/<ws>/memory/` and `refs/` stay inside the plane, because they
are a few KB of text and `charter workspace live` exists precisely to un-ignore them so a
team can commit them.

Worktrees created before this are **not** relocated automatically: moving one means
rewriting git's own `gitdir` pointer, and `git worktree move` is the command that does it
correctly. `charter doctor` finds any that are still inside the codebase and prints that
command with the paths filled in.

### What a workspace is, in each shape

A workspace is **a set of working trees** — one task's code, kept apart from every other
task's. How it gets those trees is the only thing that differs:

| | fleet | embedded |
| --- | --- | --- |
| a workspace's trees | clones, via `charter clone` | a **worktree** of the one repo |
| `default` | its own clones like any other | **the repo itself** |
| creating one | `workspace create` then `clone` | `workspace create` materialises the tree |

In an embedded plane there is nothing to clone apart, so a workspace that materialised
nothing isolated nothing: every workspace resolved to the same root tree, and `ws use
alpha` then `ws use beta` changed the memory namespace while leaving you on exactly the
same files. Two agents in two workspaces edited the same branch.

So `charter workspace create feature-x` now creates the tree that makes it a workspace:

```
$ charter workspace create feature-x
✓ Workspace 'feature-x' ready (LOCAL …) → workspaces/feature-x/
✓ Working tree → ../app.worktrees/feature-x/app/feature-x
  enter:  cd ../app.worktrees/feature-x/app/feature-x && claude
  Being in that directory IS this workspace — no pointer needed.
```

Its branch is the workspace name unless you pass `--branch`.

`default` deliberately stays the repo itself, so a solo user's path is `charter init`, work
in your repo — charter starts materialising trees only when you ask for a *second*
concurrent thing.

**Selecting a workspace with no tree is refused**, because it would put you on the same
files as every other workspace — the thing workspaces exist to prevent. `charter workspace
create <name>` gives it one.

### The directory you are in decides the workspace

A workspace's trees live at paths that name it — `workspaces/<ws>/<repo>/` for a fleet
clone, `<worktrees-root>/<ws>/<repo>/<piece>/` for a worktree — so **being inside one is
the answer**, ahead of any pointer:

```
--workspace → $CHARTER_WORKSPACE → the tree you are standing in → session → terminal → default
```

You cannot be in two directories at once, which is precisely the property the pointers
lacked: a session that had never chosen anything could inherit another session's choice.
`charter workspace current` reports `via cwd` when the directory decided it.

### Working inside a worktree

`charter.toml` is a tracked file, so an embedded plane checks a copy of it into every
worktree — which means each worktree *looks* like its own control plane. It is not: a
worktree is a view of a repo, not a repo.

So the plane's identity follows the **main working tree**. Standing anywhere inside a
worktree, `charter` resolves the plane to the repo the worktree belongs to, and personas,
the vault, workspaces and memory all stay attached to it. Without that they resolved into
the worktree itself — a directory `git worktree remove` deletes, taking any memory written
there with it — and the status line rendered `repos 0`, because a linked worktree's `.git`
is a file rather than a directory.

`$CHARTER_ROOT` is never redirected, so pointing it at a worktree is the escape hatch if
you genuinely want a plane of its own there.

### Nested planes

`charter` takes the **innermost** `charter.toml` above your working directory. That is the
right rule — it is the one git, cargo and npm use — but the embedded shape puts a
`charter.toml` into ordinary product repos, and a fleet plane clones product repos into its
workspaces. So `cd`-ing into `workspaces/<ws>/<repo>` can land you in a *different* control
plane: different personas, a different vault, and memories written somewhere you did not
choose.

The rule does not change; charter just stops being quiet about it. When the active plane
sits inside another plane's `workspaces/`, the status line carries a warning naming both
planes and the `$CHARTER_ROOT` export that pins you to the outer one.

## A self-hosted example

A GitLab Enterprise instance behind your own domain, excluding the control plane itself
from the inventory:

```toml
schema = 1

[[forge]]
kind = "gitlab"
host = "gitlab.internal.acme.com"
group = "platform"
exclude = ["charter-control-plane"]

[memory]
share = "commit"

[workspace]
default = "default"
```

## Mixed-forge: tracking GitLab and GitHub together

A control plane isn't limited to one forge. Declare a `[[forge]]` block per forge and
`charter discover` queries each in turn, merging the results into one
`inventory/repos.json`. This is the non-obvious case — most control planes need only one
block — but it's fully supported:

```toml
schema = 1

# Internal platform repos, on a self-hosted GitLab.
[[forge]]
kind = "gitlab"
host = "gitlab.example.com"
group = "platform-team"
exclude = ["legacy-monolith"]

# Open-source repos, on github.com.
[[forge]]
kind = "github"
owner = "acme-oss"

[memory]
share = "commit"
```

Repos are exposed under their **bare name** (the final path segment) everywhere —
`charter clone api`, not `charter clone gitlab:platform-team/api` — so every command
keeps working unchanged. If two forges (or two blocks of the same kind — e.g. two GitHub
orgs) happen to expose a repo with the same bare name, `charter discover` refuses to
guess: it names both candidates and tells you to either qualify the name
(`github:api` vs `gitlab:api`) when cloning, or exclude one via that block's `exclude`.
Full detail, including exactly which collisions can and can't be qualified away:
`docs/forges.md`.

## Memory posture: `[memory].share`

Every persona and workspace can write **memory** — durable notes recorded with
`charter persona remember` / `charter workspace remember`. Where those notes end up is
controlled by one setting, `[memory].share`, with three modes:

| Mode | What happens |
| --- | --- |
| `local` | The memory file is written to disk and nothing else. It never enters git, never leaves the machine. |
| `commit` | The file is `git add`ed and committed **locally** (secret-scanned first) — it becomes part of your history, but is never pushed. |
| `push` | Committed, then pushed to `origin` in the background, so it reaches the shared repo moments after it's written. |

**The default is `local`**, and that default is deliberate: a stranger who just ran
`charter init` has not decided yet whether this control plane's notes should be shared
with a team, and the failure mode of guessing wrong runs only one direction — publishing
an agent's working notes to a remote nobody reviewed. Defaulting to `push` (or even
`commit`, which still commits to *your* history) would mean the very first memory an
agent records, before anyone has looked at what it's about to write, could already be on
its way to a shared remote. `local` costs nothing but a manual `charter persona
memory-sync` (or flipping `share` once a team actually wants to see this control plane's
notes) later. Every reactive commit path (`charter persona remember`, `charter workspace
remember`, the SessionStart dispatch tally) re-checks this value and falls back to
`local` on anything it doesn't recognise — a typo in this file fails *safe*, not loud.

## `[workspace].default`

The workspace name `charter` resolves to when nothing else has selected one — no
`--workspace` flag, no `$CHARTER_WORKSPACE`, no prior `charter workspace use` in this
session or terminal pane. Defaults to `"default"` (and `default` always exists — `charter
clone` creates it on first use). See `docs/personas.md`'s sibling in spirit, workspaces,
for the full precedence chain.

## `[charter].version` — pinning the CLI

**Opt-in.** Absent, charter does nothing: you track whatever you have installed. Present,
this control plane pins one charter version and every machine conforms to it — shared the
way a lockfile is.

```toml
[charter]
version = "0.7.1"
```

| Command | What it does |
| --- | --- |
| `charter version` | Shows installed / locked / latest, and the exact next command |
| `charter version sync` | Installs the locked version on this machine |
| `charter version bump [--to X] [--push]` | Moves the pin, after verifying the target installs |

**It is exact, not a floor — so it downgrades.** That is the point: pinning a team back to a
known-good release is precisely the case you want to be automatic.

**Auto-conformance runs once per session.** The `SessionStart` hook installs the locked
version when it differs from what is running, and says so:

```
⬢ charter: auto-updated 0.7.1 → 0.8.0 to match this control plane's lock.
  The next `charter …` call uses it.
```

That wording is literal — a running process cannot replace itself mid-call, so *this*
invocation finishes on the old build and every later `charter …` in the session uses the
new one.

It never runs on the status line (which renders every turn) and never mid-turn: the
install replaces the binary enforcing the credential guard, and a session boundary is the
only safe moment for that.

**A failed auto-update never blocks you.** Offline, no `uv`, or a pin that does not exist
— charter warns, names the manual command, and the session proceeds on whatever is
installed. Being on a plane is not a defect. The drift stays visible in `charter doctor`
until it is resolved.

**Bumping is deliberate, because it is team-wide.** `charter version bump` installs and
verifies the target *before* writing the lock, so you cannot pin colleagues to a build you
have not run; `--push` commits and pushes, and everyone conforms on their next session.
charter only ever *shows* you that command — it never bumps on its own.

## Schema drift and healing

`schema` is a stamp, not just a version number: a control plane written by a *newer*
charter than the one you have installed refuses to load (`charter --version` still
works; every command that needs the control plane fails with a clear "upgrade charter"
message) rather than silently misreading a layout it doesn't fully understand. Going the
other way — an *older* control plane opened by a newer charter — is always fine: newer
charter versions can add baseline directories (`personas/`, `inventory/`, `workspaces/`)
a control plane predates, and `charter reinit` creates whatever's missing, additively,
never touching what's already there.

## The status line

Wired by `charter init` into `.claude/settings.json`, rendered by Claude Code on every
turn. It is grouped by **scope** — each zone answers exactly one question, and a count
always sits next to the thing it counts:

```
⬢ umbrella-improvements · todo 3 · ws 9                           ← WHERE am I
◫ repos 2/38                    │ ◈ personas 13 · vaults 6 · shared ✎130
├─ easysender-ui-workspace  main  ✗ failed │ ◆ steward · ✎2       ← WHAT I'm on × WHO I am
└─ iam-service ⑂2           main  ✓ passed │ ○ devops ✓ ✎192
ctx 22% · ⚡90% · ⛊1 denied · ✎1 recorded            ⬢ charter 0.10.0   ← THIS session
```

- **Top — identity and navigation.** The active workspace, how many todos it still has
  open (`charter ws todo`), and how many other workspaces exist. Nothing else: the repo
  count describes the left column, the vault count describes the right one, and the
  gauges describe the session, so each lives with what it describes. `todo N` belongs
  here for the same reason — open todos are a property of *this* workspace. It is absent
  when the count is zero, like everything else that would otherwise render every turn.
- **Columns — the Role × Task axes.** Left is the *task* (repos cloned into this
  workspace, their branch, CI and MRs); right is the *role* (the persona roster, each
  chip carrying its vault state and memory counts). Personas are global, repos are
  workspace-scoped — two independent axes, two columns.
- **Bottom — this session.** Context and prompt-cache health, plus counters for what has
  happened: in-flight sub-agents, guard denials, memories recorded, dispatches. Absent
  entirely when there is nothing to report, so a denial appearing there is *news*.
  The brand and version sit at its right edge.
- **Alerts** (a pinned-version mismatch, workspaces needing `reinit`) get their own
  full-width lines above the strip — actionable problems carrying the command that
  fixes them, not telemetry.

Two columns need 131 columns of width; below that everything stacks in the same order.
Nothing here reads the network or shells out to git — it renders every turn.
