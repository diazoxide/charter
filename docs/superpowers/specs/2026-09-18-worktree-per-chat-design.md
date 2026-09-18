# M1.4 — a chat that writes to a repo gets its own git worktree

**Status:** agreed 2026-09-18 in a grill between the operator and the `steward` persona
(workspace `ide`). The choices that sit against written rules are
[ADR 0027](../../adr/0027-git-is-the-only-registry-for-a-chats-worktree.md). The product
decision is decision 4 of [the charter-app spec](2026-09-17-charter-app.md); the merge-back
shape follows [ADR 0020](../../adr/0020-there-is-no-cross-repo-merge-loop.md).

## The goal

A chat that works in a repo no longer shares one checkout with every other chat. It gets its
own git worktree on its own branch, cut and listed by the Rust core with no Python anywhere.
Its branch shows on the chat's row. Merging back is something the operator does on purpose.
This is what makes tons of parallel sessions produce parallel work rather than parallel
conflicts.

## Sequencing

M1.4 waits for M1.2 to merge: a chat has no repo association until the start dialogue exists.
Three pull requests, in order:

1. **Core and CLI**, with the adversarial tests. Touches none of M1.2's files, so it can land
   while M1.2 is still in flight.
2. **Tauri commands and UI** — the start dialogue's worktree section, the sidebar row, the
   remove and merge actions.
3. **Scenario tests and the narrowed differential.**

The containment work is in PR 1 alone, and deliberately: burying it under a React diff is how
the M1.1 holes survived five review rounds.

## Where a worktree lives

`<plane>/workspaces/<ws>/.worktrees/<repo>/<piece>` — the layout `docs/plane-format.md`
records as stable, outside every clone so that build tools never recurse into it and a
`git clean -xfd` inside the clone cannot destroy live work.

**In-plane only.** A plane that relocates its worktree root (`[plane] worktrees`, or
`$CHARTER_WORKTREES`) is refused by name rather than followed: the relocated root is outside
the directories `contain::writable` allows, and Python gates it with a second rule
(`contain.plane_adjacent`) that the Rust core does not have. Adding a second containment
boundary to the module whose last five review rounds found four containment holes is not a
rider on this milestone.

```
error: this plane relocates its worktree root ([plane] worktrees = "../charter.worktrees"),
  and the app does not follow that yet. Use `charter wt add` from the Python charter, or
  unset [plane] worktrees to keep worktrees in the plane.
```

## Containment

Paths are the danger here. A worktree path is built from a repo name and a branch name, both
of which can arrive from a clone.

Every component is checked **as a string, before it is joined onto a path**:

| Component | Rule |
| --- | --- |
| `ws` | `contain::workspace_name_ok` |
| `repo` | `repo_name_ok` — `segment_ok` plus charter's alphabet |
| `piece` | the same alphabet; no leading `.`, no leading `-` |
| `branch` | not a path segment. Leading `-` refused by charter; the rest delegated to `git check-ref-format --branch` |

Then `contain::writable(plane_root, path)` on **the exact path** charter is about to create or
remove, and separately on the exact parent it is about to `mkdir`. Not the directory above it.
The gate one level shallower than the write is the shape four of the five M1.1 holes had.

The branch name is checked for a leading `-` by charter and not by git, because a name
starting with `-` is an *argument* by the time git sees it — `git check-ref-format --branch
--upload-pack=…` is the injection, and delegating the check does not prevent it. Everything
after that first rule is git's own ref grammar, which charter does not reimplement.

Charter reuses `contain::no_link_on_the_way` rather than adding a sixth path walk to this
repository.

### The slug

A chat's name becomes its piece name: lowercased, runs of anything outside the alphabet
collapsed to `-`, trimmed to 40 characters, leading and trailing `-` stripped. `🔥 hotfix`
gives `hotfix`; `../../etc/passwd` gives `etc-passwd`. If nothing survives, charter refuses
and asks for a name.

The slug is shown in an editable field before anything is created, so the operator sees what
they are about to get and can change a branch name they will have to live with.

**Sanitising is a convenience, never the containment.** The gate above runs on the result
regardless, and the tests drive it with names that are already legal-looking as well as with
hostile ones.

## Git

Through the binary (ADR 0027). Every invocation:

- runs with the repository-local git environment cleared — `GIT_DIR`, `GIT_WORK_TREE`,
  `GIT_INDEX_FILE` and their relatives. Inherited from a hook, these silently point
  `git -C <clone>` at a different repository.
- runs with `GIT_TERMINAL_PROMPT=0`. A credential prompt inside a subprocess the UI cannot
  show is an infinite hang. Keychain and `gh` credential helpers still work.
- has a timeout: 5 s for local verbs, 120 s for `publish`, which crosses a network.

## The verbs

```
charter wt add     <repo> <piece> [--branch <name>]
charter wt list    <repo>
charter wt remove  <repo> <piece> [--force] [--delete-branch]
charter wt merge   <repo> <piece>     # local, --ff-only, into the recorded base. Never pushes.
charter wt publish <repo> <piece>     # push the branch. Never merges.
```

The piece name is **required on the CLI** and derived in the UI: the slug below comes from a
chat's name, and the CLI has no chat.

Same spelling as Python's, because having to remember which binary you are talking to is worse
than the collision it avoids. All five reach the UI as well: a merge the operator has to leave
the app to run is a merge they will do in a terminal forever, and then the app's picture of
their branches is permanently behind. One refusal message constant, two call sites.

**Neither `merge` nor `publish` takes `--all`, and the parser refuses it.** A test asserts
that. ADR 0020's argument — a flag the agent can pass is a flag the agent will pass, and a
batched call is one ungated operation where N gated ones belong — applies one command down
from where it was written.

### `add`

Records `branch.<branch>.charterBase` with the clone's current branch — keyed on the branch
the worktree is really on, which `--branch` makes different from the piece name.

- A **dirty clone warns**, and does not refuse: uncommitted changes stay in the clone and are
  not carried into the worktree, and a dirty clone is the normal state of the tree a worktree
  exists to escape. (This diverges from the brief M1.4 was handed, which asked for a refusal.)
- A **detached HEAD is allowed** — cutting a piece to investigate an old commit is legitimate,
  and it is what a bisect looks like. The sha is recorded instead of a branch, and `merge`
  later refuses with a sentence that says why.
- A **branch that already exists** is refused, with the reuse repair:
  `charter wt add <repo> <piece> --branch <piece>`.
- **Submodule drift is reported.** `git worktree add` hands the new tree the superproject's
  gitlinks and initialises none of them, always — charter #817 measured a piece whose
  `dev-scripts` was empty while the clone's was in sync. Charter reports it and does not run
  `git submodule update`: that is a network fetch nobody asked for, on the path where the
  operator is waiting for a chat to start.
- **A race for one piece** is decided by git, not by charter's check. Where `git worktree add`
  fails, charter re-reads the repository; if the path or the branch is now held, that is the
  cause it reports, established by looking rather than by reading git's English (ADR 0009).

### `merge`

`git merge --ff-only <piece-branch>` in the clone.

Refuses, each with the repair named: an unreadable or dirty worktree; a dirty clone; a clone
that is not on the recorded base; a piece cut from a detached HEAD; a branch that does not
fast-forward.

**Allowed while the chat is still running.** It writes only in the clone, and refusing it
would mean stopping a chat to land its own work — the friction that sends the operator back
to the terminal.

Fast-forward only, and charter does not resolve the divergence for you:

```
error: 'fix-login' does not fast-forward into main — main has moved on.
  Update the piece where its conflicts belong:
    git -C workspaces/ide/.worktrees/charter-app/fix-login merge main
```

Doing it for the operator would resolve conflicts in a tree they are not looking at; doing it
as a merge commit in the clone would resolve them in the *shared* checkout. The value of
`--ff-only` is that every conflict is dealt with inside the piece, by whoever owns it, and
that the operation can always be undone by moving a ref.

`merge` never pushes. Pushing stays the operator's.

### `publish`

`git push` to `origin`, setting upstream on the first run. Refuses when there is no `origin`.
Prints the compare URL when the remote is a recognisable GitHub or GitLab URL, and stays
silent when it is not rather than guessing.

**Plain `git push`, never `--force` and never `--force-with-lease`.** `publish` is the verb
that reaches another machine, and a rejected non-fast-forward push is the human that ADR 0020
insists on having in the loop. `--force-with-lease` remains the operator's to type.

### `remove`

Refused while the chat's session is alive, naming the chat: the dirt guard cannot see a write
that has not happened yet.

That guard is **the app's**, not the CLI's. Only the app owns sessions; the `charter wt
remove` a person runs in a terminal has no way to know a chat is live, and inventing a
lock file for it would be the second registry ADR 0027 refuses. The CLI keeps the guards
that read the tree, which are the ones that protect the work itself.

Otherwise it carries Python's guards whole — they exist because parallel agents are how work
gets orphaned:

- uncommitted changes → refuse.
- *unreadable* status → refuse. A tree charter could not read is not a tree charter has
  cleared for deletion (charter #917).
- commits reachable from HEAD and from no other ref → refuse, with the count. This is the
  work that would actually cease to exist. "Has no upstream" is the wrong test: it fires on a
  piece created a minute ago that has nothing to lose, and a guard that fires on the harmless
  common case is how `--force` becomes a habit (charter #104).

`--force` is how the operator says to discard them. **The worktree goes; the branch stays**,
and the dialogue says so in a sentence — a branch costs nothing, and a deleted one costs a
reflog hunt. `--delete-branch` remains on the CLI.

A **prunable** registration — the directory was deleted by hand — has no tree to check and
gets a path that skips every tree-safety check. It is shown as `stale` with an explicit clear,
never cleared automatically on launch: charter removing a git registration nobody asked it to
touch, during a launch, with nothing to read afterwards, is the wrong shape.

## The UI

**Start dialogue.** A worktree section with a checkbox, on by default, disabled when the
workspace holds no repo. One repo is selected automatically; several require a pick, because
charter guessing from the chat's text is charter inferring a cause it cannot establish.

**Sidebar row.** The branch, and nothing else that costs a subprocess. The branch comes from
path arithmetic on the chat's cwd — the layout is a contract, and this renders for every chat
on every update. Fifty rows times a `git status` is the kind of cost that shows up as the app
feeling slow. Dirty and ahead counts belong to M1.5's repo panel, where it is one focused
workspace rather than every row.

Two labels, both read from the tree rather than remembered:

- `unwired` when the worktree has no `.charter-generated` marker (ADR 0027).
- `stale` for a prunable registration.

**Relaunch with the worktree gone** drops the chat from the reopen record and says so in the
launch report. It is never reopened in the clone instead: a chat that believes it is on its
own branch, silently resuming in the shared checkout, is precisely the failure this milestone
removes. `reopen.rs` already treats an unreadable record as nothing to put back; this is that
rule one level down.

## Tests

**Adversarial first, and each gate hand-mutated to confirm a test goes red.** Green CI is not
evidence — it was green through every round of M1.1.

Containment:

- repo and piece names that are `..`, `a/b`, `a\b`, absolute, drive-qualified, NUL-bearing,
  empty, leading `.`, leading `-`.
- `.worktrees` a symlink out of the plane; `.worktrees/<repo>` a symlink out of the plane —
  on `add` and on `remove`.
- a removal target that resolves outside the workspace through a link chain, asserting **the
  outside file still exists** after the refusal.
- a branch name starting with `-` never reaching git's argv; `a..b`, `a b`, `refs/../x`
  refused by the ref-format check.
- `GIT_DIR` and `GIT_WORK_TREE` in the environment not redirecting any operation.
- a link chain longer than the budget refusing rather than falling through.

Behaviour:

- `remove` refused for dirt, for unreadable dirt, for unique commits — each leaving the
  directory in place.
- `merge` refused for a non-fast-forward, for a dirty clone, for a clone on the wrong branch,
  for a detached-HEAD piece — each leaving the clone's HEAD unmoved.
- `--all` refused by the parser on both `merge` and `publish`.
- a plane declaring `[plane] worktrees` refused by name.
- not a git repository, a repository with no commits, and a branch that already exists — each
  refused with the sentence naming its repair.
- the slug function, including the names that sanitise to nothing.

**Differential** (ADR 0027): `tests/differential/run.py` gains a scenario kind that compares
captured command output instead of trees. Both implementations cut a piece in a throwaway
repository built with pinned author and dates; `git worktree list --porcelain`, the branch and
the commit must match.

**Scenario:** the app creates a chat with a worktree against a throwaway git repo the test
builds, the branch shows on its row, the merge-back action runs, and the clone's HEAD has
moved.

## What M1.4 does not do

- It does not write the harness layer into a worktree. A chat in a charter-cut worktree runs
  without charter's guards, the row says `unwired`, and a persona cannot be attached to such a
  chat. Recorded as a todo in the `ide` workspace; the reasoning is ADR 0027.
- It does not keep piece history. Git is the only registry.
- It does not follow a relocated worktree root.
- It does not batch anything, and will not grow a flag that does.
