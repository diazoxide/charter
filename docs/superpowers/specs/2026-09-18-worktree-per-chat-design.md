# M1.4 — a chat that writes to a repo gets its own git worktree

**Status:** agreed 2026-09-18 in a grill between the operator and the `steward` persona
(workspace `ide`), and revised the same day after an adversarial review found a code-execution
hole, a cross-workspace deletion and four contradictions in the first draft. The choices that
sit against written rules are
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
3. **Scenario tests and the differential.**

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
of which can arrive from a clone — and the destructive verb hands its path to **git**, which
resolves symlinks itself.

### Names, before any of them is joined

| Component | Rule |
| --- | --- |
| `ws` | `contain::workspace_name_ok` |
| `repo` | `contain::repo_name_ok` — `segment_ok` plus charter's alphabet |
| `piece` | the same alphabet; no leading `.`, no leading `-` |
| `branch` | not a path segment. Leading `-` refused by charter; the rest delegated to git, with the caveat below |

The branch name is checked for a leading `-` by charter and not by git, because a name
starting with `-` is an *argument* by the time git sees it — `git check-ref-format --branch
--upload-pack=…` is the injection, and delegating the check does not prevent it.

**Every ref charter names to git is fully qualified**: `refs/heads/<branch>`, never the bare
name. A branch may legally be called `@`, and `git merge --ff-only @` then resolves HEAD
rather than the branch — measured: "Already up to date", exit 0, HEAD unmoved, while
`refs/heads/@` merges the work correctly. A bare name makes charter report a successful merge
that landed nothing. `--` does not help; qualification does.

**`git check-ref-format --branch` resolves as well as validates.** Measured on git 2.50.1,
`git check-ref-format --branch '@{-1}'` prints `other` and exits 0: the `@{-n}` syntax names
the previously checked-out branch. A name that passes the check is therefore not necessarily
the name git will use. Charter takes **the name git printed**, not the one it was given, and
records and reports that one — so `add` on `@{-1}` either operates on the branch git resolved,
under its real name, or is refused because that name is taken. Nothing is recorded under a
string that names no branch.

### Paths, stated per path

Three distinct checks, and which one runs where is not left to a reader's inference:

| Path | Check |
| --- | --- |
| `.worktrees` (created by `create_dir_all`) | `within_workspace` **and** `no_link_on_the_way` |
| `.worktrees/<repo>` (created) | `within_workspace` **and** `no_link_on_the_way` |
| `.worktrees/<repo>/<piece>` (created, removed) | `within_workspace` **and** `no_link_on_the_way` |
| the clone `workspaces/<ws>/<repo>` (git writes `.git/worktrees/<id>` into it) | `contain::writable` **and** `no_link_on_the_way` |

Both checks on every row, including the ones the first draft gave only one. `.worktrees` is
the component whose corruption defeats everything below it, and it was the row with the
weakest check. And all four run in **`add`, `remove`, `merge` and `list` alike** — `remove` is
the destructive one and was the verb the first draft left on `contain::writable`.

`within_workspace` is new and narrow, and its anchor matters more than its comparison: the
resolved path must start with **`workspaces/<ws>` resolved**, and `.worktrees` must itself be
link-free. Resolving the worktree root and comparing against *that* would be vacuous exactly
when it counts — point `.worktrees` at another workspace and the root resolves there too, so
everything under it "starts with" it and passes. The anchor has to be a path the attack cannot
move, which is the workspace directory charter created.

`contain::writable` is not sufficient here either — it confines to the plane's *data
directories*, so a path resolving into **another workspace** passes it. That is not
theoretical:

```console
wsB/victim              ← a live, registered worktree of workspace B
wsA/.worktrees/r/piece  → wsB/victim   (a symlink)

$ git -C r worktree remove …/wsA/.worktrees/r/piece   # exit 0; B's tree is gone
```

Every tree-safety guard passes in that run, because they ran against B's tree. So the
worktree path is additionally required to be **link-free** (`no_link_on_the_way`), and what is
handed to git is the **resolved, absolute** path — never a relative one and never a bare piece
name, because `git worktree remove` falls back to suffix matching on anything that is not a
working tree, which would make a relative argument mean "any worktree on this machine whose
path ends this way".

For a `prunable` registration the path charter checks is the path **git reports**, not the one
charter would construct, because the reported one comes from `.git/worktrees/<id>/gitdir` —
attacker-writable inside the clone, and able to name anywhere on the filesystem — and it is
the one git will act on.

### The slug

A chat's name becomes its piece name: lowercased, runs of anything outside the alphabet
collapsed to `-`, trimmed to 40 characters, and any leading character that is not a letter or
digit stripped — `_` included, because `_wip` slugs to `_wip` and the piece rule refuses a
name that does not start alphanumeric. The invariant test drives names beginning with `_`
and `.` for exactly that reason; without them it passes while proving nothing. `🔥 hotfix`
gives `hotfix`; `../../etc/passwd` gives `etc-passwd`. If nothing survives, charter refuses
and asks for a name.

The slug is shown in an editable field before anything is created, so the operator sees what
they are about to get and can change a branch name they will have to live with.

**Sanitising is a convenience, never the containment.** The checks above run on the result
regardless, and a test drives the slug with hostile names to pin that it can never hand them
something they would have to refuse.

## Git

Through the binary (ADR 0027). Every invocation:

- **is given an environment charter constructed**, not the one charter inherited. Git is run
  with `env_clear()` and then the few variables it needs: `HOME`, a `PATH` charter pinned,
  the locale, and the credential-helper variables charter's auth design requires. Nothing
  else is passed through, so a variable git grows next year is absent by default.

  Subtracting a denylist does not work here. `git rev-parse --local-env-vars` defines the
  **redirection** surface (fifteen names on git 2.50.1, `GIT_CONFIG`,
  `GIT_CONFIG_PARAMETERS` and `GIT_CONFIG_COUNT` among them). It does not define the
  **execution** surface, and measured on the same git: `GIT_EXEC_PATH` makes `publish` run an
  attacker's `git-remote-https`; `GIT_TRACE` and the `GIT_TRACE2*` family append to any path
  on *every* verb, read-only ones included; `GIT_SSH_COMMAND`, `GIT_ASKPASS` and
  `GIT_PROXY_COMMAND` each name a program git runs; and `PATH` decides which `git` runs at
  all. None of those is repository-local, so no definition git prints will ever mention them.
- **runs a `git` resolved to an absolute path**, for the same reason.
- keeps the denylist **as a test, not as the mechanism**: nothing
  `git rev-parse --local-env-vars` prints may survive into the child, so a mistake in the
  allowlist is a red test rather than a redirection.
- runs with `GIT_TERMINAL_PROMPT=0`. A credential prompt inside a subprocess the UI cannot
  show is an infinite hang. Keychain and `gh` credential helpers still work.
- passes `--` before user-supplied values wherever the subcommand accepts it.

**`git worktree add` and `git merge` run with no deadline at all; `list`, `status` and
`config` get 5 s; `publish` gets 120 s.** Stated as the rule rather than left to a constant's
name, because the first draft defined an untimed constant and then passed the 5 s one anyway.

 `git worktree list` gets 5 s, matching Python's
`_GIT_TIMEOUT`, which is a listing timeout and always was. `git worktree add` performs a full
checkout and `merge` rewrites a working tree; on a large repo or a cold cache either exceeds
5 s routinely, and a killed `worktree add` leaves the registration written and the checkout
half-done — an operator with a broken piece and a "branch already exists" error on the retry.
Python imposes no timeout on those calls, and neither does this. `publish` gets 120 s because
it crosses a network and a hung push is a different failure from a slow checkout.

## The verbs

```
charter wt add     <repo> <piece> [--branch <name>]
charter wt list    <repo>
charter wt remove  <repo> <piece> [--force] [--delete-branch]
charter wt merge   <repo> <piece>     # local, --ff-only, into the recorded base. Never pushes.
charter wt publish <repo> <piece>     # push the branch. Never merges.
```

The piece name is **required on the CLI** and derived in the UI: the slug comes from a chat's
name, and the CLI has no chat.

Same spelling as Python's, because having to remember which binary you are talking to is worse
than the collision it avoids — with one consequence that has to be paid rather than accepted
silently, in `add` below. All five reach the UI as well: a merge the operator has to leave the
app to run is a merge they will do in a terminal forever, and then the app's picture of their
branches is permanently behind. One refusal message constant, two call sites.

**Neither `merge` nor `publish` takes `--all`, and the parser refuses it.** A test asserts
that. ADR 0020's argument — a flag the agent can pass is a flag the agent will pass, and a
batched call is one ungated operation where N gated ones belong — applies one command down
from where it was written.

### `add`

Records the base with `git config --replace-all branch.<branch>.charterBase <base>`, keyed on
the branch the worktree is really on, which `--branch` makes different from the piece name.

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
- **The missing harness layer is warned about, here, in the command's own output.** This is
  the path with no UI in it, and it is the path where the same command name means the opposite
  thing: Python's `charter wt add` wires the layer before it prints `enter: cd <path> &&
  claude` (charter #951 is the bug report for not doing so). The Rust one does not wire it, so
  it says so:

  ```
  ! this worktree has no charter layer: no persona agents, no ask/deny rules, no
    $CHARTER_HARNESS. A harness started here runs without them. Wire it with the Python
    charter: charter reinit
  ```

- **A race for one piece** is decided by git, not by charter's check. Where `git worktree add`
  fails, charter re-reads the repository; if the path or the branch is now held, that is the
  cause it reports, established by looking rather than by reading git's English (ADR 0009).

### `merge`

`git merge --ff-only <piece-branch>` in the clone.

The base is read with `git config --get-all branch.<branch>.charterBase`. **More than one
value is a refusal**, not a last-one-wins: `.git/config` is writable by anything in the clone,
and `--get` returns the final value with exit 0 and no warning, so a second line is somebody
else choosing what charter merges into. **No value is a refusal too**, and its sentence says
the base was never recorded rather than implying the worktree is foreign — Python charter
records no `charterBase`, so every Python-cut piece is in exactly this state during the
cutover, and the repair is to name the base explicitly.

Refuses, each with the repair named: an unreadable or dirty worktree; a dirty clone; a clone
that is not on the recorded base; a recorded base that no longer exists, or that is not an
ancestor of the clone's current branch (a base renamed or deleted and recreated leaves the
record naming something else); a piece cut from a detached HEAD; a branch that does not
fast-forward.

**Allowed while the chat is still running.** It writes only in the clone, and refusing it
would mean stopping a chat to land its own work — the friction that sends the operator back
to the terminal. What it does write in the clone is a working tree, so a chat *reading* the
clone mid-build can see it change underneath. That is the price of not blocking, it is
smaller than the alternative, and it is named here so nobody discovers it as a bug.

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

### `list`

`git worktree list --porcelain` in the clone, parsed, and **filtered to registrations whose
resolved path lies under this workspace's worktree root**. The clone's own entry, a bare
repository's entry and anything registered elsewhere on the machine are all reported by git
and are none of charter's business; without the filter, a worktree somebody made in another
location would be listed as a piece and could then be handed to `remove`.

### `remove`

Refused while the chat's session is alive, naming the chat: the dirt guard cannot see a write
that has not happened yet.

That guard is **the app's**, not the CLI's. Only the app owns sessions; the `charter wt
remove` a person runs in a terminal has no way to know a chat is live, and inventing a lock
file for it would be the second registry ADR 0027 refuses. The CLI keeps the guards that read
the tree, which are the ones that protect the work itself.

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

**The clear is `git worktree remove` on the reported path, and it fails when anything exists
there.** Measured: with the registered directory absent, `git worktree remove` clears the
registration and exits 0; with a directory recreated at that path, it exits 128 with
`validation failed … '.git' does not exist`, and `--force` does not change that. So the clear
works in the benign case and refuses in the case where something has been put back — which is
the racy one, and the right place to stop. The refusal says so and names
`git -C <clone> worktree prune` as the operator's move, without charter running it: `prune` is
repository-wide and would clear registrations charter never made.

## The UI

**Start dialogue.** A worktree section with a checkbox, on by default, disabled when the
workspace holds no repo. One repo is selected automatically; several require a pick, because
charter guessing from the chat's text is charter inferring a cause it cannot establish.

**Sidebar row.** The branch — and it comes from `list`, not from path arithmetic. The layout
gives the *piece*, and `--branch` makes the piece and the branch different strings, so
arithmetic would show a wrong branch as fact on the one row this milestone is named after.
The cost is one `git worktree list --porcelain` per repo, cached and refreshed when the app
already has reason to, not one `git status` per row: fifty rows times a subprocess is the kind
of cost that shows up as the app feeling slow. Dirty and ahead counts stay out, and belong to
M1.5's repo panel where it is one focused workspace.

Two labels:

- `unwired` when the worktree has no harness layer. The marker is **not** taken at face value:
  charter's own `.charter-generated` is per-checkout and untracked, so a *tracked* one is
  content a cloned repository committed and says nothing about this tree —
  `workspace.py:2176` already treats a tracked marker as untrusted, and so does this. Without
  that rule, any repo carrying a committed `.charter-generated` would make every piece of it
  read as wired, which would silence this label *and* the persona refusal at once.
- `stale` for a prunable registration.

**Relaunch with the worktree gone** drops the chat from the reopen record and says so in the
launch report. It is never reopened in the clone instead: a chat that believes it is on its
own branch, silently resuming in the shared checkout, is precisely the failure this milestone
removes. `reopen.rs` already treats an unreadable record as nothing to put back; this is that
rule one level down.

## Tests

**Adversarial first, and each gate hand-mutated to confirm a test goes red.** Green CI is not
evidence — it was green through every round of M1.1.

Containment and environment:

- repo and piece names that are `..`, `a/b`, `a\b`, absolute, drive-qualified, NUL-bearing,
  empty, leading `.`, leading `-`.
- `.worktrees` and `.worktrees/<repo>` as symlinks out of the plane, on `add` and on `remove`.
- **a worktree path resolving into another workspace, asserting that workspace's tree still
  exists afterwards** — the `within_workspace` boundary, which `contain::writable` does not
  provide.
- a removal target replaced by a symlink after it was cut, asserting the file outside is still
  there.
- a branch name starting with `-` never reaching git's argv; `a..b`, `a b`, `refs/../x`
  refused by git; **`@{-1}` not recorded under the string it was given.**
- **every variable `git rev-parse --local-env-vars` prints is withheld**, driven by running
  that command rather than by a literal list, plus the three `GIT_CONFIG*` names — and a test
  that a `core.hooksPath` injected through `GIT_CONFIG_COUNT` does not run a hook.
- a link chain longer than the budget refusing rather than falling through.

Behaviour:

- `remove` refused for dirt, for unreadable dirt, for unique commits — each leaving the
  directory in place; and the stale clear refusing when something exists at the reported path.
- `merge` refused for a non-fast-forward, for a dirty clone, for a clone on the wrong branch,
  for a detached-HEAD piece, for **two** `charterBase` values, for **no** `charterBase` value,
  and for a recorded base that no longer exists — each leaving the clone's HEAD unmoved.
- `--all` refused by the parser on both `merge` and `publish`.
- `list` not reporting the clone itself, a bare entry, or a worktree registered elsewhere.
- a plane declaring `[plane] worktrees` refused by name.
- not a git repository, a repository with no commits, and a branch that already exists — each
  refused with the sentence naming its repair.
- `add` printing the unwired warning; a tracked `.charter-generated` not reading as wired.
- the slug function, including the names that sanitise to nothing.

**Differential.** A tree comparison, kept (ADR 0027), with the deliberate divergences as
`ignore` entries carrying their reason — the guest layer, the piece history, and `.git/**`
because a git repository does not compare byte for byte against itself. The escape sentinel
(`_outside`/`_escaped`) and the directory-set comparison are kept, because they are what would
catch a containment bug and a second registry respectively. Scenarios cover `add`, `remove`,
`merge` and `publish`, against a throwaway repository built with pinned author and dates.

**Scenario:** the app creates a chat with a worktree against a throwaway git repo the test
builds, the branch shows on its row, the merge-back action runs, and the clone's HEAD has
moved.

## What M1.4 does not do

- It does not write the harness layer into a worktree. A chat in a charter-cut worktree runs
  without the plane's ask/deny rules and persona agents; `add` warns, the row says `unwired`,
  and a persona cannot be attached. This is a debt against decision 14, not a reading of it —
  ADR 0027 states it at full size, including that today's only repair is a Python command.
  Recorded as a todo in the `ide` workspace.
- It does not keep piece history. Git is the only registry.
- It does not follow a relocated worktree root.
- It does not batch anything, and will not grow a flag that does.
