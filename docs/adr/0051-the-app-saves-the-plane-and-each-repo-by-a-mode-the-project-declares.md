# The app saves the plane and each repo by a mode the project declares

**Accepted 2026-09-24**, by the operator, answer by answer in a grilling session (Q1–Q29).
Nothing below is built yet. `docs/plane-format.md` records the keys first, as `CLAUDE.md`
requires.

The plane is the project's database. The operator works in it, and so do their chats, and what
they write only counts once it has reached the plane's remote. Before this record, three things
said three different things about how that happens:

- **`[memory] share`** (`local` | `commit` | `push`, default `local`) promised a posture.
- **charter-app never honoured it.** `memory.rs` warns that "this version of charter does not
  commit memory yet". The tool hook still tells agents that `push` means "committed and pushed
  immediately". `workspace _autosave` is an empty stub.
- **`charter save`** stages the whole tree, commits it, and pushes `HEAD` to whatever branch is
  checked out, falling back to a `charter/<sha>` branch when a protected branch refuses the push.

Nothing configured the branch. No PR was ever opened. Nothing in the window said what was
unsaved.

Repos differ. Some take a push to `main`. Some take only a merged PR. Some take only signed
commits. The operator's ruling: *"we need to be ready for all scenarios, everything should be
configurable"*.

## The decision

**One mode, a ladder.** `[plane] mode` is one of five values. Each value includes everything
the one before it does.

- `off`: charter never commits.
- `commit`: local commits only.
- `push`: commit, then push to `branch`.
- `pr`: commit, push to a save branch, then open or update a PR against `branch`.
- `pr-merge`: the same as `pr`, and the PR is also set to auto-merge.

The options this rejected were separate booleans (`commit`, `push`, `pr`, `auto_merge`). They
can express contradictions, such as auto-merge with no PR, and they make every reader handle
those contradictions.

**`[plane]`, not `[memory]`.** A save commits the whole plane tree: memory, todos, workspaces,
dispatch logs. `[plane]` already exists (`worktrees`). `[memory] share` stays readable as a
deprecated alias:

| `[memory] share` | reads as `[plane] mode` |
|---|---|
| `local` | *not set* (see below) |
| `commit` | `commit` |
| `push` | `push` |

`charter doctor` warns about the alias. When both keys are present, `[plane] mode` wins.

**Every key can be set in either file, and `charter.local.toml` wins key by key.** This is ADR
0048's overlay, extended to `[plane]` and `[repos.<name>]`. Every place that shows a value
names the file that decided it, and an overridden shared value is visibly marked. The rejected
alternative split the keys by owner: `mode` shared only, `branch` and `autosave` local only.
The operator ruled against that split. The marker is what stops a local `mode = "push"` from
silently disagreeing with a team whose repo takes only PRs.

**Each workspace repo has its own table.** `[repos.<name>]`, keyed by the repo's name in
`inventory/repos.json`, takes the same keys as `[plane]`, but its defaults are `mode = "off"`
and `autosave = false`.

> **Amended 2026-09-25, by the operator.** The default was `mode = "pr"`, and the title bar's
> save button saved the active workspace's repos along with the plane. One press could commit a
> developer's half-finished work in several repos, push their branches and open pull requests,
> and the button did not say it had become *Save all*. Now:
> - the title bar's save saves **the plane only**;
> - *Save all* is in the Saving tab alone, behind a confirmation that names each repo, its
>   branch, how many files, and where its save goes;
> - a repo nobody configured is **`off`**: charter never commits, pushes or opens a PR for it
>   until `[repos.<name>] mode` says how;
> - each repo row says where its own Save goes before it is pressed.

- **Why it is keyed by repo, not by workspace:** a repo's rules belong to its remote, not to
  whichever workspace it was cloned into.
- **Why the defaults are safer than the plane's:** code is not a database, and a developer's
  branch is theirs. Even a commit on it is an action nobody asked for, so the default does
  nothing at all.

**One save function.** `charter-core` saves. The window's buttons, auto-save and `charter save`
all call it, so the CLI follows `mode` too. What a save does:

- **Stages:** the whole tree (`git add -A`). `.gitignore` is the only way to leave something out.
- **Checks:** the secret scan still refuses. A workspace repo's save uses a narrower rule,
  because the plane's credential-assignment rule refuses ordinary code: it refuses a staged file
  whose name is a credential's (`.env`, an SSH private key, `*.pem`, `credentials.json`, a
  `.npmrc` holding a token, and the like) and any staged text holding a private key block or a
  token with a forge's own prefix (charter-app#299).
- **Commit message:** generated from what changed, grouped as `charter save` already prints it.
  A manual save may replace it.
- **Signing:** `sign`, default `false`. A push refused for an unsigned commit tells the operator
  to set it.

Commits an agent makes with plain git are fine: the next save pushes them.

**Where each mode lands:**

- **The plane in `pr` and `pr-merge`:** one rolling save branch per machine
  (`save_branch`, default `charter/save/<host>`) and one open PR, updated by every save. It
  replaces `charter/<sha>`.
- **A squash merge:** once the PR has merged, charter fetches `branch`. If the fetched tree
  matches ours on every path the PR touched, the local branch moves to the remote one, keeping
  newer uncommitted work on top. Otherwise the plane is **blocked**.
- **A repo:** a save commits on the branch the repo is on, and `pr` opens a PR from that
  branch. On the repo's default branch, a PR mode first creates `charter/<workspace>/<short-sha>`,
  so a PR mode never pushes straight to a protected branch.
- **A repo with a session mid-turn:** a save of that workspace's repos waits until no session in
  the workspace is mid-turn, by the hook state. A manual save says whom it is waiting for. An
  auto-save skips that cycle.
- **PR and MR creation:** through the existing GitHub and GitLab adapters. On any other host,
  `pr` and `pr-merge` are a config error, which doctor and the settings tab report, and saves
  fall back to `commit` with the plane shown as blocked.
- **Auto-merge:** the app requests it for the configured mode, preferring the repo's rebase,
  then merge-commit, then squash. An agent never does, which `floorguard` already enforces.

**Auto-save lives in the app**, because there is no daemon (ADR 0025). When `autosave` is on:

- **It saves:**
  - `autosave_after` of quiet after the last change (default `30s`)
  - when a session ends
  - when the app quits
- **It runs the whole mode each time,** so nothing sits committed but unpushed where nobody can
  see it.
- **At quit:** the commit happens, the push gets about five seconds, and the next launch pushes
  whatever was left.
- **Incoming changes:** it fetches every five minutes and on window focus, and fast-forwards a
  clean tree. With auto-save off, incoming changes are shown and not pulled.

**Outside the app, the plane is saved only through `charter save`** (#375). A chat the app did
not start (a terminal `claude` or `codex`) has no auto-save and no incoming loop, and no
daemon stands in for them. `charter save --pull` runs the incoming loop's fetch and
fast-forward first, through the same core function, so one command does both from a terminal.
The fast-forward waits on the same conditions. If the tree has unsaved work, what came in is
left where it is and the save goes ahead. If the tree has conflicts, the command refuses and
saves nothing, because a save would stage the conflict markers. The pull is asked for, so it
runs whatever the mode, `off` included, where the save after it then commits nothing.

Defaults:

| Scope | `autosave` |
|---|---|
| The plane | on |
| Repos | off |

A plane with no `[plane] mode` whose `share` is absent or `local` is asked once in the Saving
view before pushing starts, and the answer is written as `[plane] mode`. That covers every plane
`charter init` ever made, since it always wrote `share = "local"`. Reading that as `off` would
have kept auto-save off on every existing plane. A new plane starts at `push`.

**Conflicts.** The plane format gains a `.gitattributes` with `merge=union` for the files that
only grow: the `_dispatch` and `_skills` `*.jsonl` logs, the `pieces` and `changes/log` logs,
and the `MEMORY.md` indexes. Any other conflict makes the plane or repo **blocked**:

- auto-save pauses for it;
- the Saving view offers *Resolve in a chat* (a steward session there) or *Open terminal here*;
- nothing rewrites anyone's history.

**What the window shows.** Work sits at one of four stages:

1. *changed*
2. *committed*
3. *pushed, PR open*
4. *saved*

It can also be **blocked** or **offline**. In the window:

- **The title-bar indicator** shows the furthest-back stage across the plane and the active
  workspace's repos, with a count and incoming changes (↓N). Its save button saves the plane
  only.
- **The Saving view** is a view tab (tabs hold views, not only chats). It has:
  - one row per repo, built on the existing `workspace_repos`, with a stage, a branch, a PR link,
    where its save goes, and its own save button;
  - *Save all*, behind a confirmation that lists what it would save;
  - the last 50 entries of a local save journal in `.charter/`, which records the trigger, mode,
    files, commit or PR, duration and outcome.
- **Blocked:**
  - It turns the indicator red.
  - It raises an alert after ten minutes, or at once for a secret-scan refusal.
  - It is never needs-you: a stuck save isn't a session waiting on the operator.
- **The "plane root" alert:** the indicator replaces its dirty and unpushed part.
- **Several projects:** each plane saves on its own. The project switcher marks one with unsaved
  or blocked work, and quitting saves every plane (repos only where their auto-save is on).

**Liveness shows, and it can be changed at any time.** A LIVE workspace is marked in the tab
strip, the breadcrumb and the Explorer, and LOCAL is unmarked. The marker's action, the
workspace settings page, and the create and fork dialogs all change it through the existing
`workspace live` core function.

- **Going LIVE:** a confirmation says what goes where, including whether the remote is public,
  then a plane save runs at once. Going live is an explicit intent to publish.
- **Going LOCAL:** the files stop being tracked (`git rm --cached`, kept on disk) and a save
  runs. Pushed history stays on the remote, and the confirmation says so.
- **Create and fork:** Live is off by default, and a fork of a LIVE workspace defaults to LIVE.

**Where the settings live.** The Project settings tab gets two sections: *Plane* and *Repos*.
The workspace settings page gets only *Live*.

## Consequences

- **The contradiction is fixed:** the `memory.rs` warning and the hook text that promises `push`
  both change to describe `mode`.
- **`charter save` changes contract.** It reads `mode`, `branch` and `save_branch`, so its
  recorded answers move on purpose (ADR 0046).
- **The forge adapters gain their first write that isn't a push:** creating and updating a PR or
  MR, and requesting auto-merge.
- **Out of scope:** per-workspace overrides of a repo's policy, per-file exclusion from a save,
  and rewriting pushed history.
