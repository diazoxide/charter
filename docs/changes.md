# A change that spans several repos

One piece of work often touches five repositories. Git has no word for that, and neither
did charter: what you got was eleven branches across five clones and nothing saying which
five go together, which one has to land first, or what any of it was for.

A **change** is that missing word. One intent, N repositories, stored as a file per
workspace:

```bash
charter change create component-api-2 --why "component.API_VERSION 1 -> 2"
charter change add component-api-2 charter
charter change add component-api-2 charter-metrics --needs charter
charter change drop component-api-2 charter-slack --why "no components; only an action provider"
charter change show component-api-2
charter change list
charter change forget component-api-2
charter change revert component-api-2
```

Vocabulary, because the noun collides: a **change** is the cross-repo object, a **member**
is one repository's part of it, and a member's pull or merge request is a **request**.

## What the record holds, and what it refuses to

`workspaces/<ws>/changes/<slug>.json` holds six keys and no seventh: the name, the `why`,
when and by whom, the members (each with its repo, its branch and what must land before
it), and the repositories considered and deliberately excluded, with reasons and dates.

**There is no state field, and none is representable.** No request number, no CI result, no
branch position, no `landed` flag — the key set is closed at both ends, so an unknown *or*
missing key stops the read and is named rather than ignored. `need` where `needs` was meant
is an ordering constraint that would otherwise have silently ceased to exist.

Ordering is the case worth naming twice, because it looks like state and is not. `needs` is
**declared**, because only a human knows that repo B's change needs repo A's merged.
*Blocked* is **derived** at read time from that declaration and a fresh reading of what has
landed, and nothing writes it anywhere. A cycle is refused at write time with every member
in it named: that is not a condition to render, it is a record that cannot be true.

**Membership is committed. Destination is local.** The record carries bare repository
names — never a URL, a host, a remote, a forge kind or a base branch. A remote in a
committed file is a *destination* that arrives from someone else's machine. There is also
**no expansion**: no glob, no pattern, no `--all-repos`, and nothing in this surface
enumerates repositories. Every member was typed by somebody, and it must resolve to a clone
already in this workspace.

## Landing is a declaration, joined against git when you read it

"Has this member landed" cannot be answered from the record and must not be stored in it —
but it cannot be answered from the forge alone either, because a merged request's source
branch is routinely deleted and a branch-keyed lookup then finds nothing, which is
indistinguishable from a member that was never pushed.

So `charter change land` appends one past-tense line to an append-only, **never-committed**
log beside the records:

```
workspaces/<ws>/changes/log/<host>.jsonl
  {"ts": …, "change": "component-api-2", "repo": "charter",
   "number": 601, "merge": "e0c9d13", "head": "4b1e77a"}
```

Same shape, same per-host filename and the same not-committed rule as `pieces/`. It holds
the declaration git cannot make — *charter merged this commit, for this change* — and the
present tense is reconstructed by asking git whether the default branch still contains that
sha. A member the forge calls merged with no log line is landed **and divergent**. A member
with a log line git no longer contains is not landed any more, and nothing had to notice or
update a flag for that to become true.

## What charter refuses, collected

The refusals are the design, so they are in one place.

1. **No `--all` on `land`.** The flag does not exist and the parser refuses it. See
   [ADR 0020](adr/0020-there-is-no-cross-repo-merge-loop.md).
2. **No automatic cross-repo rollback.** A revert is a new change.
3. **No synthetic monorepo on disk.** No symlink farm, no union mount, no subtree. The
   monorepo is the workspace directory — the clones are already siblings, so `rg` already
   spans the change; what was missing is knowing which of them are one piece of work, and
   that is `charter change show`.
4. **No aggregate green.** A change is never reported greener than its worst member, and
   one member charter could not read makes the change `UNKNOWN`.
5. **No `gh pr checks`, no `mergeStateStatus`.** Both report a run that never happened
   identically to a clean pass. Checks are read per head sha or not at all.
6. **No stored state.** No request number, no CI result, no branch position, no `landed`
   flag in the record.
7. **No destination in a committed file.** No URL, host, remote or base branch.
8. **No expansion.** No glob, no pattern, no repository enumeration anywhere in this
   surface.
9. **No force-push, no branch deletion, no default-branch reset, no closing a request
   charter did not open** — in any change action, during a revert included.
10. **No unattended landing.** `charter change land` is on the release floor: under
    `bypassPermissions` it is **denied**, not asked, because an unattended ask falls back
    to allow. See [hooks.md](hooks.md).
11. **No auto-closing of todos**, and no todo state.
12. **No claim of atomicity**, anywhere, in any wording.

**Charter cannot make a cross-repo landing atomic**, and it does not pretend to. What it
does instead is make the window visible, bounded and named, and refuse to report a partial
landing as anything but partial.

## Reverting

```bash
charter change revert component-api-2
```

derives a **new** change, `revert-component-api-2`, whose members are the landed members of
the original. Each is seeded with a branch off that clone's default branch carrying a
revert of the sha the landing log recorded — with `-m 1` **only when git says that sha has
more than one parent**, because a squash landing is an ordinary commit and `-m` on one
fails. The parent count is asked of git, never remembered.

**The ordering is the original's, reversed.** If `charter-metrics` needed `charter` to land
first — because its code depends on charter's new API — then undoing it goes the other way:
reverting `charter` while `charter-metrics` still depends on the API it removes leaves the
dependent broken, which is the world the revert exists to restore.

From there it is an ordinary change: pushed, reviewed, gated and landed one member at a
time, by the same commands with the same refusals.

Two things it cannot do, said plainly rather than discovered:

- **It cannot revert a deploy.** A merge to a default branch on a repo with continuous
  deployment has already had an effect in the world. Charter has no model of deployment and
  will not acquire one to make this sentence shorter.
- **It cannot revert what it did not record.** A member merged in the browser has no
  `Charter-Change` trailer and no landing record; `revert` names it as needing a human
  rather than guessing which merge commit was the one.

It also refuses over uncommitted work, by name: `git revert` commits into the checkout, and
folding somebody's work in progress into a revert commit is worse than the wait.

## Divergence is named, at FAIL

`charter doctor` has a `changes` row, and it fails rather than warns — a divergence worth
naming is worth an exit code, and that exit code is the only thing that makes the
SessionStart wrapper print. Five things it names:

- a member whose branch is already in its default branch and which **charter did not
  land** — so there is no trailer and no merge sha, and `revert` cannot reach it;
- a declared landing the default branch **no longer contains** — reverted, or the branch
  was rewritten;
- a declared landing whose commit **carries no `Charter-Change` trailer** — the log names a
  commit charter did not author for this change;
- a member that **landed while a blocker had not**. Charter refuses that landing and cannot
  stop a browser; naming it is the half that makes the guard honest rather than decorative;
- a member's **branch name in a clone that is a member of no change** — either a member
  somebody forgot to add, or a name collision worth knowing about.

It reads what is already on this disk and never fetches, because it runs at SessionStart.
The honest consequence: it can **under**-report, and it can never invent.

## In the frame

`changes` is a section of the persona sidebar, beside the todos:

```
▪ changes 2 UNKNOWN 4m
  component-api-2  2/3
* metrics-panel-f  0/1
```

**It draws nothing at all when this workspace has no changes**, exactly as the todo
section does — so it costs a plane that never uses the feature no rows and no attention.
That is why it is a section rather than a pane of its own: a placed component has to be in
`[frame] slots`, and the shipped `slots`, `density = full` and the slot list are pinned to
agree, so placing it would put a pane saying "no changes in <ws>" on every operator's
frame. (The frame also supports exactly one variable-height *pane* by construction —
`resize-pane -y` moves one boundary, so one pane has to be the remainder, and that pane is
the repo table.)

The heading carries the count, the worst member of the worst change, and how old the
reading is. Each row is one change and how much of it is in. `F2` then the **change** row
picks one, and the picked one carries the `*` — it does not move to the top, because a
list whose rows reorder with state is a list nobody learns. `charter change show <slug>` is
the whole picture.

**Nothing on the repaint path calls a forge, and nothing on it starts a process at all.**
The section reads the one gather snapshot — the records and the landing declarations, both
file reads — under that snapshot's single timestamp, which is the age the heading draws.

`UNKNOWN` is a word rather than a blank because it means *charter did not look*: what
stands between a member and its landing is a request state and its checks at its head sha,
and those are a forge read the frame does not make.

## Optional: a prompt before each landing

```bash
charter guard ask --local 'charter change land *'
```

`charter doctor` names this command and **does not run it**: both answers are legitimate,
and a tool that settles a legitimate choice by writing a line while nobody is looking has
made the decision for you.

`--local` is load-bearing. `guard ask` writes the plane's *committed* `.claude/settings.json`
by default; `--local` writes the gitignored `.claude/settings.local.json` instead. Consent
that travels in a commit enrols a whole team on one person's click. A team that genuinely
wants the prompt for everybody drops the flag — a decision, made once, visible in a diff.

Because Claude Code matches on the full command string, the prompt shows **which change and
which repo**.
