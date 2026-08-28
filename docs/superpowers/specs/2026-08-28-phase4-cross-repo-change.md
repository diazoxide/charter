# Phase 4 — the cross-repo change

**Date:** 2026-08-28 · **Status:** proposed, unimplemented
**Parent spec:** `docs/superpowers/specs/2026-08-25-agentic-ide-foundation.md` — §3.2, §4d, §4e,
§4f, §4g, §4i, §4j, and "Phase 4 — the cross-repo change", which says *"Needs its own spec."*
This is it.
**Depends on:** Phases 0–3. Phase 5 (§4j) depends on this.

---

## 1. What this is for

The project's origin sentence, verbatim:

> This is foundation for future IDEA that handling complex projects and making
> **monorepo on top of many repos**.

charter has a control plane, workspaces, repo clones per workspace, personas with vaults and
memory, a harness registry, and a component-based frame with a command palette. What it does
not have is a way to **make one change that spans several repositories and land it
coherently**. Everything below is that, and only that.

The parent spec already fixed the shape in four sentences, across four rounds of grilling:

- **§4d** — *"A cross-repo change is a stored object, not a view over git."*
- **§4e** — *"A change is committed, like a workspace… a change's name and description are
  untrusted committed values."*
- **§4f** — *"Store intent, derive facts… if git or the forge knows it, do not store it."*
- **§4g** — *"Actions are fire-and-report, never blocking."*

This spec does not reopen those. It answers what was left: **what identifies a change on
disk, what its lifecycle is, what charter does with it, and — the larger half — what charter
refuses to do with it.**

---

## 2. Where we actually are

Honest assessment, because four of the seven decisions below turn on it.

**What exists and is load-bearing.**

- `workspaces/<ws>/<repo>/` is a clone; `workspace.clones(ws)` and `workspace.repo_trees(ws)`
  are the one list anything asking *"which repos am I on?"* uses. The plane root is
  deliberately not in it (ADR 0008).
- `workspaces/<ws>/.worktrees/<repo>/<piece>` is a **piece** — one unit of parallel work whose
  creation *is* the claim, arbitrated by git (ADR 0011).
- Three per-workspace stores already sit side by side, split by lifetime: `memory/` (facts),
  `todos/` (intent that expires, ADR 0004), `pieces/` (an append-only event log).
- A workspace's shareable metadata is committed or not by one switch — `charter workspace
  live` splices a managed block into `.gitignore` between `_LIVE_BEGIN` and `_LIVE_END`.
- `workspace.STRUCTURE_VERSION` plus `charter workspace reinit` is the shipped way to grow the
  workspace layout without breaking existing planes.
- `gitpolicy` makes "one credential, per forge, over HTTPS" mechanically true per clone, and
  resolves the forge from **each clone's own `origin`**, never from the plane's first
  `[[forge]]` block.
- `hooks._release_floor_reason` already denies `gh pr merge` / `glab mr merge` /
  `gh release create`, `glab release create` and tag pushes under `bypassPermissions`, and
  denies rather than asks, because since
  0.46.0 an unattended `_ask` falls back to `allow`.
- `contain.py` has the whole containment vocabulary: `segment_ok`, `child`, `within_data`,
  `dir_refusal`/`file_refusal` as a pair (#336), `one_line`, `readable`, `writable`.

**What does not exist, and each of these constrains a decision below.**

1. **`Forge` is read-only.** The protocol is eight methods. `open_change(path, branch)`
   returns an `int | None` — a number, with no title, base, state, mergeability, review
   decision or head sha attached. There is **no PR creation, no PR update, no merge, no
   per-check enumeration** anywhere in `charter/forge/`.
2. **`planegit._compare_url` declined to add PR creation on purpose** — its comment says
   charter *"has no PR-creation capability in any forge adapter, and this closes the PR-gated
   workflow **without** adding one."* §5 argues why Phase 4 must add one anyway, and why that
   is not a reversal.
3. **`ci_status` collapses six worlds into `None`.** A CLI failure, a timeout, a non-zero
   exit, malformed JSON, an auth failure, and *"no check ever ran"* all return `None`, and
   `statusline._ci_part` renders all six as an empty column. GitHub's `statusCheckRollup` is
   `null` for a commit with zero check-runs, which parses to exactly the same `None` as a
   rate-limited API call. **This is #561, one layer below where #561 was found.**
4. **#561 is knowledge, not code.** It lives in one persona memory and one exit criterion in
   the Phase 2 plan. `gh pr checks` reports *"no checks reported"* and `mergeStateStatus`
   reports `CLEAN` **identically when no run was ever created**. Charter never reads either —
   its own exposure is narrower and honest-but-lossy — but nothing in the tree yet knows how
   to tell "passed" from "did not run".
5. **`glstate` is the only refresh path**, keyed per clone directory and current branch,
   cached in `.charter/cache/glstate.json` with `REFRESH_TTL=300`. A repo not cloned in this
   workspace has no refresh path at all.
6. **`pieces.FIELDS` is a closed eight-key set** with no todo id and no change id, pinned by
   tests. ADR 0012's *"a piece may record which todo it came from"* is unimplemented.

### Vocabulary, because "change" is already taken

`charter/forge/base.py` uses **change** for *one pull request or merge request* — that is what
`open_change` opens and what `change_sigil` (`#` on GitHub, `!` on GitLab) prefixes. The parent
spec uses **change** for the cross-repo object. Both are entrenched and neither is renamed
here: `open_change` and `change_sigil` are shipped API, and §3.2 of the parent spec fixed the
noun for the new one.

So the disambiguation is by discipline rather than by rename. In everything Phase 4 writes:

- a **change** is the cross-repo object — one intent, N repositories;
- a **member** is one repository's part of a change;
- a member's pull or merge request is a **request** (`PR`/`MR` in passing), never a change.

A builder who writes "the member's change" has written a sentence with two meanings, and this
paragraph exists so the collision is met on page two rather than in review.

---

## 3. The seven decisions

Each states the decision, then the strongest counter-argument I could build against it, then
the answer. Where the counter-argument won, the decision above it is already the changed one.

---

### 3.1 The unit is a **change**: a committed record, per workspace, holding intent only

A change is one JSON file:

```
workspaces/<ws>/changes/<slug>.json
```

```json
{
  "change":  "component-api-2",
  "why":     "component.API_VERSION 1 -> 2; providers must declare the new integer",
  "created": "2026-08-28T09:14:02+00:00",
  "by":      "Aaron Yordanyan",
  "members": [
    {"repo": "charter",         "branch": "change/component-api-2", "needs": []},
    {"repo": "charter-metrics", "branch": "change/component-api-2", "needs": ["charter"]},
    {"repo": "charter-jira",    "branch": "change/component-api-2", "needs": ["charter"]}
  ],
  "excluded": [
    {"repo": "charter-slack", "why": "no components; only an action provider", "at": "2026-08-28T09:20:11+00:00"}
  ]
}
```

**Those six keys are the whole set, and they are matched exactly.** An unknown key stops the
read and is named, rather than being ignored — the `docs/news/` rule from #503, for the same
reason: a key charter does not read reads as nothing at all, and `need` where `needs` was
meant is an ordering constraint that silently ceased to exist.

**Every field is something git and the forge cannot know.** Which repositories are
*intended* to be part of this work; which branch in each is *this change's* branch as against
the eleven others in that clone; which member must land before which; which repo was
considered and deliberately left out, and why. There is **no state field, no PR number, no CI
result, no branch position, and no "landed" flag** — §4f's line, and ADR 0011's rule, which
says the same thing harder: *"The moment a derivable fact is cached for convenience, this ADR
has been reversed whether or not anyone says so."*

**And a change ends.** `charter change forget <slug>` deletes the record, the way
`charter workspace forget` deletes a memory by slug. This does not contradict the argument two
paragraphs down that a change's *name* must outlive the intent: the name lives in commit
trailers, in request bodies and in merged history, none of which the record owns. What the
record holds is the working state of an ongoing piece of work, and a store with no way to end
an entry grows without bound — `all_for(ws)` reads all of it on every gather and the picker
lists all of it. ADR 0004's todos got forget-by-slug for the same reason; this is that
precedent, not an exception to it.

**Scope: a change belongs to its workspace for life.** §4j settled the identical question for
chats — *"Moving a chat between workspaces sounds convenient and means the harness's own
context is suddenly about a different plane"* — and the reasoning transfers exactly. A
change's members are clones, clones live in a workspace, and a change whose members are
somewhere else is a change nobody can work. A change wanted elsewhere is a new change.

**Commitment: none of its own, and the condition matters later.** A change is committed
**exactly when its workspace is LIVE** — and a workspace is LOCAL until somebody runs `charter
workspace live`, so the default is a file on one machine. That single condition is load-bearing
in §6.1 and is stated there rather than assumed. It is committed by
the mechanism that already exists. `changes/*.json` joins `workspace._live_block` and
`commands_workspace._ws_meta_paths`, and `workspace.STRUCTURE_VERSION` goes 2 → 3 so every
existing workspace is flagged for one `charter workspace reinit`. No new commit story, no new
switch, no new config key. (`changes/log/` joins neither — see just below.)

**One part ready and another not is the normal state, not an exception.** It is not recorded,
because it is derived: a member is ready when its branch is pushed, its PR is open, its checks
PASSED **at the current head sha**, and every repo in its `needs` has landed. §3.5 defines
each of those words; §3.3 defines what charter does with the answer.

#### Landing is a declaration, joined against git at read time

"Has this member landed" cannot be answered from the record, and must not be stored in it. But
it also cannot be answered from the forge alone: a merged pull request's source branch is
routinely deleted, and a branch-keyed lookup then finds nothing at all — which is
indistinguishable from a member that was never pushed.

So charter uses the shape `planegit` already ships for exactly this problem. `charter change
land` appends one past-tense line to an append-only, **never-committed** log beside the
records:

```
workspaces/<ws>/changes/log/<host>.jsonl
  {"ts": …, "change": "component-api-2", "repo": "charter",
   "number": 601, "merge": "e0c9d13", "head": "4b1e77a"}
```

Same file shape, same `O_APPEND`-with-no-lock discipline and same per-host filename as
`pieces/<host>.jsonl`; not committed, for the same reason `pieces/` is not. This is **not** a
state store, and the distinction is ADR 0011's: the log holds a **declaration git cannot
make** — *charter merged this commit, for this change* — and the present tense is reconstructed
at read time by asking git whether the default branch still contains that sha. `planegit`
already does precisely this with `record_push` and `is_spent(head, run)`.

**"Landed", defined once, because six sections consume the word:** a member is *landed* when
the forge reports its request **merged** and git shows the member's default branch **containing
the sha the log recorded**. Both halves, because the forge alone cannot see a revert and the log
alone cannot see a browser merge. A member the forge calls merged with no log line is landed
*and* divergent (§3.2). A member with a log line git no longer contains is not landed any more,
and nothing had to notice or update a flag for that to become true.

Three things fall out, and each is worth the file:

- **`revert` has a merge sha to revert** (§3.7) without storing one in the committed record.
- **A member landed outside charter is detectable** — the forge says merged, the log has no
  line — which is what §3.2's named divergence is built on.
- **A merge that was itself reverted stops reading as landed**, because git no longer contains
  it, without anything having to notice or update a flag.

The committed record still holds no PR number, no sha and no landed flag. Nothing on disk can
disagree with git, because nothing on disk claims to know.

#### Counter-argument: "You already have workspaces. A change *is* a workspace. This is a second noun for one thing."

The overlap is real and it is the strongest attack on this document. A workspace already
holds N repo clones, a `## Vision` saying what the task is for, todos, and a manifest listing
repos and branches. Adding a change looks like inventing `workspace-lite`.

**Three properties separate them, and each one alone would be enough.**

**A workspace is a place; a change is a piece of work.** The parent spec says it in §3.2 and
it is not wordplay: `workspaces/platform/` can carry the auth migration *and* the logging
cleanup at the same time, in the same five clones, on ten branches. Making them two
workspaces means ten clones of five repos, and switching between them is a session lock and a
re-clone. Making them one workspace with no change concept means the ten branches are a flat
list with nothing saying which five go together — which is the state charter is in today.

**A workspace has no landing lifecycle.** It is never *merged*. It has no notion of "three of
five in, two to go", nothing to be blocked on, and nothing to be reverted. Bolting those onto
the workspace is the ADR 0012 move — hanging a second lifetime off a store that has one — and
that ADR already explains what it costs: *"Intent and execution have different lifetimes, and
merging them destroys the shorter one."*

**A change's name must outlive the intent; a workspace's need not, and a todo's must not.**
This is the decisive one and it is ADR 0004's own argument run one level out. A todo is
deleted when it is done — intent expires, by design. But `Charter-Change: component-api-2`
is in a merge commit on someone else's default branch **forever**, and in six months it is
the only thread back to why that commit exists. An identity that vanishes the moment the work
succeeds cannot be the identity of an artifact that persists in five repositories' histories.
Two lifetimes, two stores. That is ADR 0004's rule, not an exception to it.

#### Counter-argument: "ADR 0012 says the plan is todos, with no new store and no new noun. You just added both."

ADR 0012 governs two things by name: **the plan** (*"this task is eight pieces"*) and **the
lifecycle** (claimed → done/abandoned). A change is neither, and Phase 4 changes neither:

- The plan for a change is still workspace todos. `charter change` writes no todo, reads no
  todo, and closes no todo. *"Nothing closes automatically"* stands unaltered.
- The lifecycle of the work inside each member is still a **piece** — a worktree, claimed by
  its own existence, declared through `charter worktree done`. `pieces.EVENTS` gains nothing.
  `pieces.FIELDS` gains nothing: **linking a piece to its change was considered and
  declined**, because widening a closed, test-pinned field set is a decision with its own
  cost, and the branch name plus the landing commit's trailer already carry the association
  for every reader who needs it.

What a change stores is the thing ADR 0012 has no place for: *which repositories*, and *the
name the artifacts carry across them*. A todo cannot hold either. It has no repo field, its
slug is workspace-local and timestamp-derived, and it is deleted on completion.

---

### 3.2 Ordering is declared, derived at read time, and enforced only where charter itself acts

A member declares `needs: ["charter"]` — repo names of members that must **land** first.

- **Declared**, because only a human knows it. Git cannot see that repo B's change needs repo
  A's merged; no amount of reading either repository reveals it. This is ADR 0011's test for
  what belongs in a record, and it passes.
- **Never stored as state.** "Blocked" is computed at read time from `needs` plus a read of
  each blocker's landing. Nothing writes `blocked` anywhere. ADR 0011 forbids inventing
  `failed`/`blocked`/`timed-out` as *recorded* states because charter can verify none of them;
  a value computed from a declaration and a fresh read is verified by construction, and
  disappears the moment either changes.
- **A cycle is refused at write time**, by name, with both members named. A cycle is not a
  state to render; it is a record that cannot be true.
- **Enforced exactly where charter is the actor.** `charter change land` refuses a member
  whose blockers have not landed — exit 2, naming the blocker. It does not and cannot stop a
  human merging in the browser, and it does not pretend to.

**When it happens anyway, it is a named divergence, not a silent one.** A member that landed
while a blocker had not is reported as such, at FAIL, from the same read that produces every
other row — ADR 0013: *"A divergence charter can see, charter names… WARN is not a surface."*

#### Counter-argument: "Enforcement with a browser-shaped hole is theatre. Either enforce it or drop it."

If the hole made the guard useless, `gitpolicy` would be useless too — a developer can always
`git -c credential.helper= push`. The project's stated posture is `SECURITY.md`'s: *"guard
rails, not guarantees… a guard against mistakes, not an attacker with shell access as your
user."* The mistake this guard catches is real and common: an agent working five branches
merges the one whose CI happened to go green first. The person who opens a browser, finds the
PR, and clicks merge has made a decision; the loop that merges whatever is green has not.

What would be theatre is a guard that **reports** enforcement it does not have. So the same
read that refuses also reports out-of-order landings that happened anyway, which is the half
that makes the guard honest rather than decorative.

#### Counter-argument: "Three of five merged and the fourth rejected. Your ordering model has nothing to say about that."

It has exactly one thing to say, and refusing to say more is the point. The three that landed
are landed; git says so and no record disagrees. The fourth is `REJECTED` — derived from its
PR being closed unmerged, not invented. The fifth, if it `needs` the fourth, is `BLOCKED` and
`charter change land` refuses it.

What charter does **not** do is decide what happens next, because there are three reasonable
answers — narrow the change and drop the fourth member, revert the three, or wait — and they
differ by facts charter has no access to. ADR 0009: *"a vague error keeps you looking; a
confident wrong one tells you to stop."* Picking one here is the confidently-wrong answer.

The operator's move is `charter change drop <slug> <repo> --why "…"`, which is a first-class
verb precisely for this: the member leaves `members`, joins `excluded` with its reason and a
timestamp, and the record permanently says a repo was considered and excluded — §4f's own
requirement, and the only artifact that makes the resulting partial world explicable.

---

### 3.3 Atomicity is replaced by legibility — and by the refusal of the loop

There is no cross-repo transaction and there never will be. The honest guarantee is three
sentences:

> **Charter cannot make a cross-repo landing atomic.**
> **It can make the window visible, bounded and named, and it can refuse to report a partial
> landing as anything but partial.**
> **And it does not offer the one operation that would turn N revertible merges into one
> irreversible transition with no human between them.**

**The refusal, concretely: there is no `--all`.** `charter change land <slug> --repo <name>`
lands **one** member. It is not a flag that defaults off; the flag does not exist, and a test
asserts it does not exist.

Two reasons, and the second is the stronger:

**ADR 0003's reasoning, applied unchanged.** That ADR rejected a `--yes` flag on `charter
report send` with one sentence: *"a flag the agent can pass is a flag the agent will pass
unprompted, which is exactly the failure being prevented."* `--all` is that flag, for an
operation whose blast radius is five repositories rather than one issue.

**`--all` would have to answer a question that has no answer.** When member 3 of 5 is
rejected mid-loop, `--all` must do something: stop and leave two landed, continue and land the
independents, or roll back what it did. Each is wrong in a case the others handle. ADR 0009's
rule is that charter may name a cause it recognised and must not assert one it inferred; a
flag that must guess a policy is the same defect wearing an argument parser. **Not offering
the flag is not offering a lie.**

**This belongs in an ADR**, not only here, because it will be proposed again by everybody who
has ever written the shell loop. Phase 4 ships it as **ADR 0020 — there is no cross-repo merge
loop.**

**What replaces atomicity, in four parts:**

1. **One name, on every artifact charter authors.** The change's slug is in each member's
   branch name (by default), in each request's title and cross-link block, and in the trailer of
   the commit charter creates when it lands a member. `git log --grep='Charter-Change: <slug>'`
   on any member's default branch finds that member's half.

   **Which forces a decision about merge method, and charter takes it.** A landing is a merge
   commit or a squash; **`--rebase` is refused**. A rebase merge replays the author's own
   commits and charter authors none of them, so there is no commit to carry the trailer and no
   single sha to revert — the guarantee above would be false, and §3.7's mechanism would have
   nothing to run against. Where a repository permits only rebase merges, charter says so and
   does not land that member. That is a repository whose settings are incompatible with this
   kind of archaeology, and naming it is better than shipping a promise that is false in a
   reachable configuration. Note this is charter constraining **its own act**, not the
   repository's policy — a human merging by rebase is unaffected, and shows up as a member
   landed outside charter (§3.2).
2. **The window is named while it is open.** A change with some members landed and some not
   renders as `PARTIALLY LANDED (3 of 5)` with the outstanding members listed. There is no
   percentage, no bar, and no single word for the change as a whole that a member could hide
   behind.
3. **The change is never reported greener than its worst member.** §3.5.
4. **The archaeology does not depend on the record.** The PR bodies carry a cross-link block
   and the trailers are in git — both inside the repositories themselves. The record adds the
   `why` and the exclusions, and in a LIVE workspace it is committed alongside them; in a LOCAL
   one it is a file on the machine that did the work. Nothing about the archaeology requires
   charter to be installed, or even to still exist — which is why the record is the third of
   three here and not the first.

#### Counter-argument: "Every guarantee you offer is a guarantee you will break."

Then count them. The guarantees above are: charter's own reads are per head sha; charter's own
report is never greener than its worst member; charter itself lands one member per invocation;
charter itself writes the trailer on the landing commit it creates. Every one is a statement
about **what charter does**, verifiable in charter's own test suite, and none is a statement
about the world.

The statements about the world are all negative — *charter cannot make this atomic; charter
cannot stop a browser; charter cannot revert a deploy* — and they are in the document rather
than in a footnote. The security assessment's own verdict on the last time this project got
that balance wrong is the standard being applied: *"A security claim that is false in a
reachable configuration is worse than no claim."*

#### Counter-argument: "No `--all` is a speed bump. The agent runs the command five times in a loop."

It can, and nothing stops it. Three things are still true and they are why the refusal earns
its place.

The loop the agent writes has **no gates in it** — but the command it calls does, on every
call: the blocker gate, the check gate, the read-back. A five-iteration shell loop over
`charter change land` is five gated landings; a `--all` flag is one ungated one. **The
refusal is not of repetition, it is of a code path that batches the gates.**

Every call is traced independently, so the record shows five decisions rather than one.

And an agent that writes the loop has *chosen* to, in a session someone can read, rather than
passing a flag charter itself put on the command as the obvious way to do the obvious thing.
That difference is exactly what ADR 0003 is about, and this project has already decided it
once.

---

### 3.4 The work happens in the clones the workspace already has. Charter builds no monorepo on disk.

- **Where:** `workspaces/<ws>/<repo>` — the clones that are already there. A member must
  resolve to an existing clone; `charter change add` refuses a repo the workspace does not
  have, and names `charter clone <repo> -w <ws>` as the fix.
- **On what:** one branch per member, its name **stored** in the record. Stored, not derived
  by convention, for the reason §4d gives — *"Branch-name conventions break the moment someone
  names one differently"* — and for ADR 0011's: git knows the branch exists; it cannot know
  the branch is *this change's*. The default offered at `add` time is `change/<slug>`.
- **Splitting further:** unchanged. A member's work can be cut into pieces
  (`workspaces/<ws>/.worktrees/<repo>/<piece>`) exactly as today. A change does not compete
  with pieces; it sits above them.
- **What charter does not build:** no symlink farm, no union mount, no `git subtree`, no
  synthetic root with the five repos underneath it. Not as an option, not behind a flag.

**Why no synthetic tree.** A directory of symlinks into five repositories is a tree where
`git status` answers about whichever repo you happen to be standing in, a relative path
crosses a repository boundary that will not exist at build time, and `contain.within_data`'s
fast path — one `lstat`, parent-is-a-root and not a symlink — is defeated by construction. The
project has already paid for symlink-shaped surprises under a work directory (#572). A fake
monorepo is a lie told to the filesystem, and the filesystem tells it back to the compiler.

**How the agent sees all of it at once, then.** Two things, and they are enough:

> **The monorepo is the workspace directory. What charter adds is knowing which of its
> subdirectories are one change.**

`workspaces/<ws>/` already puts the clones side by side under one parent, so `rg` and `find`
already span the change and always did. What was missing is the *knowing*, and that is one
command: `charter change show <slug>` prints the change's why, its members, each member's
branch, diffstat, PR and check state, and its blockers — one screen, one moment, one
timestamp. That is the monorepo view. It is a view, not a mount, and being a view is what
makes it correct.

---

### 3.5 CI: charter aggregates presence and state per member, and never averages

**What charter reads.** For each member: the branch's current head sha, then the **check runs
at that exact sha**. On GitHub that is `repos/<owner>/<repo>/commits/<sha>/check-runs`.

It also needs the pull request as more than a number. `open_change(path, branch)` returns
`int | None` today — open pull requests only, no state, no head, no merge commit — which
cannot answer "did this land" and cannot tell a closed-unmerged member from a member with no
pull request at all. So the read side gains a lookup returning the number, the state
(`open` / `merged` / `closed`), the head sha and, when merged, the merge commit. That is a
read, so it is not gated on §8.2's answer, and it is the minimum for `PARTIALLY LANDED` and
`REJECTED` to be derivable rather than guessed.

**What charter never reads for this:** `gh pr checks` and `mergeStateStatus`. Both report
"no checks reported" and `CLEAN` respectively when no run was ever created (#561). They are
named as forbidden inputs in the spec and in the test, because the failure they cause is a
green light rather than an error.

**The state model. Five values, closed, and no two collapse:**

| value | means | evidence |
|---|---|---|
| `PASSED` | at least one check run at this head sha, all concluded success or neutral | `total > 0`, no non-success conclusion |
| `FAILED` | a run at this head sha concluded failure, cancelled, timed out, or startup failure | `total > 0`, at least one such |
| `RUNNING` | a run at this head sha is queued or in progress | `total > 0`, at least one unconcluded |
| `NOT RUN` | **zero** check runs exist at this head sha | `total == 0` |
| `UNKNOWN` | charter could not ask — CLI failure, timeout, auth, rate limit, malformed reply | `total is None` |

**Three conclusions the table would otherwise leave to an implementer's judgement, decided
here:** `skipped` and `neutral` **count as passed** — they are how a forge says *nothing to do
here*, and a `paths:` filter or an `if:` condition produces them constantly, so any other
reading refuses the gate on most real repositories. `action_required` is **`FAILED`** — it is
the forge asking for a human, and a check waiting on a person did not pass. `stale` **does not
count toward `total` at all**: it is a run the forge itself has disowned, and if it is the only
run at the head then `NOT RUN` is the honest answer. Anything charter does not recognise is
`UNKNOWN`, never `PASSED`.

**Precedence is fixed and stated, so two people reading the table agree:** `UNKNOWN` beats
`FAILED` beats `RUNNING` beats `NOT RUN` beats `PASSED`. `UNKNOWN` is first because it is the
only value that means charter did not look, and a value that means "I did not look" must never
be outranked by one that means "I looked and it was fine".

**`NOT RUN` and `UNKNOWN` are different, and neither is green.** This is the whole of #561
and it is also ADR 0011's rule one level out: *"no threshold ever converts silence into a
verdict."* Zero check runs is silence. Six months of zero check runs is still silence.

#### The blind spot in the endpoint, named here rather than discovered later

`commits/<sha>/check-runs` returns **Check Runs only** — GitHub Actions and GitHub Apps. CI
reporting through the **Commit Statuses** API instead (Jenkins, Buildkite, CircleCI's status
integration, anything POSTing to `/statuses`) yields `total_count: 0` at a fully green head.
That is this section's own failure arriving from the other direction: a permanent `NOT RUN`, a
gate that never opens, and §5.2's deliberate refusal of a `--force`. GitLab has the mirror
image — **merged-results pipelines run against `refs/merge-requests/:iid/merge`, whose sha is
not the branch head**, so a head-sha query is empty on a green merge request.

So the requirement is a **property, not an endpoint**: *charter's read must see every check the
forge would show a human at that head.* On GitHub that is two reads — check runs and the
combined commit status — summed into one `total`. On GitLab it is the merge request's own
pipelines rather than a bare sha filter. Both are named as steps in Task 5 rather than left to
be found in review.

**And where charter cannot satisfy that property, the answer is `UNKNOWN`, never `NOT RUN`.**
`NOT RUN` asserts that nothing ran; charter may only assert it having enumerated everything. A
forge, a self-hosted instance or a future check kind charter cannot enumerate completely
produces *I could not look*, which refuses the gate just as firmly and sends the reader
somewhere useful. The asymmetry decides it: a false `NOT RUN` costs a re-run, a false `PASSED`
merges untested code.

**"Not yet" and "never" read the same, and charter does not wait.** The commonest cause of zero
runs at a freshly pushed sha is a workflow that has not been created yet — a matter of seconds.
Charter cannot tell that from a repository with no CI, and ADR 0011 forbids converting the wait
into a verdict: *"no threshold ever converts silence into a verdict."* So the gate refuses,
names the sha, and stops. It does not sleep, poll or retry. The operator looks and runs the
command again, which is a second of attention against a class of bug that has already cost this
repository two investigations.

**This is also the honest limit on §5.1's "correctness, not ergonomics" claim.** Charter's read
is better than `gh pr checks` in the direction that merges untested code, and it has a failure
mode `gh pr checks` does not — refusing a green repository whose checks it cannot see. Both
directions are stated because only one of them is a merge.

**The protocol must carry two fields, not one string.** `ci_status` returning `str | None`
is the shape that made `None` mean six things, and adding a `"not_run"` member to `CI_STATES`
would repeat the mistake with an extra name — a single string still cannot distinguish "there
are no runs" from "I could not look". So `Forge` gains a method returning a small record with
**`total: int | None`** and a state derived from the runs, and `total is None` is the only way
to say "could not ask". `ci_status` stays exactly as it is for the status line, where the
permissive discipline is correct; the change surface does not use it.

**Checks are keyed to the head sha, and that is the whole staleness story.** A check run at
any other sha is not a check on this head. So a pushed fixup returns its member to `NOT RUN`
immediately and loudly, rather than leaving the previous sha's green result standing. There is
no `STALE` state because there is nothing for it to describe.

**The change's state is its worst member's, and it names that member.** `PARTIALLY LANDED
(3 of 5) — NOT READY: web (checks NOT RUN)`. There is no aggregate green unless every member is
individually `PASSED`, and one `UNKNOWN` member makes the change `UNKNOWN`, not "green with an
asterisk".

**`BLOCKED` and `NOT READY` are different words on purpose.** `BLOCKED` is the ordering sense
of §3.2 — a declared blocker has not landed — and nothing but `needs` produces it. `NOT READY`
is everything else standing between a member and its landing gate. One word for both would put
"someone else has to go first" and "your own checks have not run" in the same bucket, and the
operator's next action differs completely between them.

**What charter leaves entirely alone.** Required reviewers, branch protection, CODEOWNERS,
merge method, review policy, labels, milestones. Those belong to each repository and its
owners — ADR 0014's rule, one host out: policy that fits the host's own pattern belongs to the
host. Charter reports *whether* each member's PR is approved and by the forge's own reckoning
blocked, and it never computes "the change is approved", because approval is granted per repo
by different people and a single word would hide the one who has not answered.

**When one of them says no.** That member is `REJECTED`, its dependents are `BLOCKED`, and
`charter change land` refuses them. The change does not become "failed" — there is no such
state. The operator drops the member with a reason (§3.2) or narrows the change. If members
already landed, the world stays partially changed, permanently, and the record says so
permanently. That is not a gap in the design; it is the design refusing to pretend the
rejection did not happen.

---

### 3.6 The surface: one component, one picker, ≤2 keystrokes

A **`changes` component** in the Phase 1 registry, a **change picker** over Phase 2's overlay,
and palette actions. Reachable by its own direct toggle key and through `F2`, which is the
Phase 2 ceiling and this component does not get an exception.

```
component-api-2 · 2/3 landed · NOT READY: charter-jira (checks NOT RUN) · read 4m ago
  charter          landed  #601  merged 2026-08-30
  charter-metrics  landed  #14   merged 2026-09-01
  charter-jira     open    #7    NOT RUN at 9f3a1c2   needs: charter ✓
```

From it: show, refresh, open a member's PR in a browser, land one member (with its refusal
stated in place rather than as a silent no-op), and jump to a member's clone — which in Phase
5's vocabulary is a chat in this workspace.

**Three constraints it does not get to negotiate.**

**Containment, because §4e already said so.** *"A change's name and description are untrusted
committed values."* The change slug, each member repo name, **each member branch name** and the
`why` line go through `contain.one_line` **before** any width arithmetic, and repo names go through
`contain.segment_ok` — never `workspace.valid_name`, which rejects `.github`, a real and
common repository name that comes from a forge rather than from charter. That distinction has
already cost this project once. The `why` field is **one line by construction**: a `why` that
cannot be one line is a `why` that belongs in `workspace.md`, which is where prose lives.

**Cost, because §4g's idle-tick property is not negotiable either.** A five-member change is
five forge reads per refresh, and those never happen on the repaint tick. The repaint reads the
**one gather snapshot**, which carries both the change records (cheap file reads) and the last
observed member states, under **the gather's single timestamp**; the component draws that
timestamp's age. Refresh is a Phase 2 action — fire-and-report, progress through `inflight` —
and what it does is write a new snapshot.

**Explicitly not `glstate`.** It is tempting, because it already caches forge state, and it is
wrong twice: it is keyed on each clone's *currently checked-out* branch, which is frequently
not the member's branch, and what it caches is `ci_status`, which §7 item 5 forbids the change
surface from reading. Reusing it would be a second clock **and** the forbidden source — §4f's
two rules broken in one line.

**Both halves of §4f, not one.** Its store rule — *"if git or the forge knows it, do not store
it"* — is about the **committed** record, which still holds no request number and no check
result. Its clock rule — *"one snapshot… with one timestamp"*, because *"this codebase has paid
for that twice already"* — is why the observed states go into the gather rather than beside it.
A design that answered only the first rule would be §4f half-read, which is how the second one
gets broken every time.

**`needs` must be served before it is declared.** §4i is explicit that `component.NEEDS` ships
as the slices `gather.scan` actually carries, and that a name answered with an empty tuple
would let a component *"declare it, draw nothing, pass its own tests against an empty fixture,
and be indistinguishable from a plane that genuinely has none."* So Phase 4 extends
`gather.scan` to serve `changes` **in the same task** that adds the name to `NEEDS`, and
`ctx.SERVES` and `component.NEEDS` are asserted against each other so they cannot drift.

---

### 3.7 Rollback: nothing automatic, deliberately — and a revert is a new change

A change lands in three repos and turns out wrong. What charter offers:

**`charter change revert <slug>` derives a *new* change.** Its members are the landed members
of the original; each is seeded with a branch carrying a revert of the sha the landing log
recorded — **`-m 1` only when git says that sha has more than one parent**, because a squash
landing is an ordinary commit and `-m` on one fails. The parent count is asked of git, not
remembered. Its `why` names the original slug. From there it is an ordinary
change: pushed, reviewed, gated, landed one member at a time, by the same commands with the
same refusals.

**What charter refuses, in an emergency, by name:** force-push to any branch; deletion of any
branch, merged or not; `reset --hard` on a default branch; closing a PR it did not open; and —
unchanged from §3.3 — landing more than one member per invocation.

**Two things it cannot do, said plainly rather than discovered:**

- **It cannot revert a deploy.** A merge to a default branch on a repo with continuous
  deployment has already had an effect in the world. Charter has no model of deployment and
  will not acquire one to make this sentence shorter.
- **It cannot revert what it did not record.** A member merged in the browser has no
  `Charter-Change` trailer and no landing record; `revert` names it as needing a human, rather
  than guessing which merge commit was the one.

#### Counter-argument: "'Nothing automatic' is the answer of a tool that has not finished. Every other decision here is defended; this one is an excuse."

The reason is the same one that produced §3.3, and it is stronger here rather than weaker:

**The moment you most want the loop is the moment you are least able to judge it.** An
automatic cross-repo rollback is `--all` with the safety off, offered to an operator who has
just discovered something is wrong and does not yet know what. Every argument against the loop
applies, plus a new one: the reverts themselves need review and CI, because a revert can break
a repo just as effectively as the change did.

**And a magic rollback would destroy the only thing that makes the state explicable.** Force-
pushing three default branches back past the merges leaves a world where the change happened,
was undone, and no repository's history mentions either — the exact failure this whole spec
exists to prevent. A revert-as-a-new-change leaves `component-api-2` and
`revert-component-api-2` in the log, both named, both cross-referenced, in every repo they
touched. Six months later that reads as a decision. A force-push reads as corruption.

So the honest form of the answer is not "nothing automatic". It is: **the automatic thing is
offered, and it is a new change, because that is the only rollback whose result a stranger can
read.**

---

## 4. One real change, walked end to end

An abstract spec here is worthless. This one is not hypothetical in its substance: **§4g of
the parent spec already committed charter to exactly this change and gave it no way to
perform it.**

> *"Provider compatibility is a single integer, refused at load. Charter bumps the integer
> when the contract changes; a provider declaring a different one does not load."* — §4g

Bumping that integer is a change spanning charter and every component-provider distribution.
It is the first real cross-repo change charter's own roadmap creates, and it has the awkward
shape that makes it worth walking: **an ordering constraint charter can enforce, and a
blocker charter is forbidden to resolve.**

The two provider distributions below are named illustratively — the extension model is
approved and nobody has shipped a provider yet. The *change* is not illustrative: the sentence
quoted above is a commitment to perform it, made by an approved spec, with no tool behind it.

The workspace is `providers`, a standing place holding three clones. It is also carrying an
unrelated change, `metrics-panel-fix`, which touches one repo — which is why the change is not
the workspace.

**1. Create the change and enumerate it. Literal names, no expansion.**

```
$ charter change create component-api-2 --why "component.API_VERSION 1 -> 2; providers declare the new integer"
• workspace: providers  (via session)
✓ change 'component-api-2' created

$ charter change add component-api-2 charter
✓ charter          branch change/component-api-2
$ charter change add component-api-2 charter-metrics --needs charter
✓ charter-metrics  branch change/component-api-2   needs: charter
$ charter change add component-api-2 charter-jira --needs charter
✓ charter-jira     branch change/component-api-2   needs: charter

$ charter change add component-api-2 charter-slack
✗ charter-slack: no clone in workspace 'providers'.
  Clone it first: charter clone charter-slack -w providers
```

The refusal is the containment property doing its job, not an inconvenience: a member must
resolve to a clone the operator put there. The operator decides `charter-slack` is out of
scope and records that, so the record says a repo was considered rather than forgotten:

```
$ charter change drop component-api-2 charter-slack --why "no components; only an action provider"
✓ excluded (never a member)
```

**2. Work happens in the clones.** Nothing new: `workspaces/providers/charter/`,
`workspaces/providers/charter-metrics/`, each on `change/component-api-2`. Cut into pieces
with `charter worktree add` if parallel workers want them. Charter builds no combined tree,
and `rg 'API_VERSION' workspaces/providers/` already spans the change because the clones are
already siblings.

**3. Push and open the pull requests.**

```
$ charter change push component-api-2
✓ charter          pushed  -> #601  https://github.com/diazoxide/charter/pull/601
✓ charter-metrics  pushed  -> #14
✓ charter-jira     pushed  -> #7
✓ cross-link block written into 3 PR bodies
```

Each PR body gains a block between two markers, and charter owns only what is between them:

```
<!-- BEGIN charter change — GENERATED by `charter change push`; do not edit by hand. -->
**Cross-repo change: `component-api-2`** — component.API_VERSION 1 -> 2; providers declare the new integer

| repo | pull request | needs |
|---|---|---|
| charter | diazoxide/charter#601 | — |
| charter-metrics | acme/charter-metrics#14 | charter |
| charter-jira | acme/charter-jira#7 | charter |
<!-- END charter change -->
```

If the markers are absent or unbalanced, charter refuses to write and says so — the same
posture as `workspace._LIVE_BEGIN`/`_LIVE_END` in `.gitignore` and `render.PERSONAS_BEGIN` in
the README, both of which exist because editing outside your own delimiters means editing
somebody's prose. This block is the reason PR creation is worth adding to the protocol at all:
it is the cross-repo link that survives charter being uninstalled, and no human maintains five
of them by hand.

**4. The blockers bite.**

```
$ charter change land component-api-2 --repo charter-metrics
✗ charter-metrics: blocker 'charter' has not landed.
  (exit 2)
```

Correct, and unarguable. `charter` has no blockers, so it goes first:

```
$ charter change land component-api-2 --repo charter
• charter: checks PASSED at 4b1e77a (7 runs)
✓ merged #601 as e0c9d13, trailer Charter-Change: component-api-2
```

**5. And now the part atomicity would have hidden.** `charter-metrics`' CI installs charter
from PyPI, so it cannot go green until charter *releases* — and a release is ADR 0003's floor,
a human, attended, never charter alone.

```
$ charter change show component-api-2
component-api-2 · PARTIALLY LANDED (1 of 3) · read just now
  why: component.API_VERSION 1 -> 2; providers declare the new integer

  charter          landed  #601  e0c9d13  2026-08-30
  charter-metrics  open    #14   FAILED at 771ab90 (1)   needs: charter ✓
  charter-jira     open    #7    FAILED at c02de55 (1)   needs: charter ✓
```

Between this moment and the release, the world is inconsistent: anyone installing charter from
git gets API 2, and their installed providers stop loading. **That window is what a
cross-repo transaction would have closed and cannot.** What charter delivers instead is that
the window has a name, a member list, a `why` a stranger can read, and a state that says
`PARTIALLY LANDED` rather than a green tick and a percentage.

The operator cuts the release — attended, by hand, under the standing rule that autonomy stops
at the tag. CI reruns. Both providers go `PASSED`.

**6. #561's moment, exactly as it happens in this repository.** Someone pushes a fixup to
`charter-jira`'s branch:

```
$ charter change land component-api-2 --repo charter-jira
✗ charter-jira: checks NOT RUN at 9f3a1c2 — this head sha has no check run and no
  commit status. Nothing ran, or nothing has run yet.
  (exit 2)
```

The refusal says only what charter read. (`gh pr checks` would have said "no checks reported"
here, `mergeStateStatus` would have said `CLEAN`, and the merge button would have been offered —
but that belongs in this spec, not in a refusal charter prints, because it is an assertion about
tools charter did not run. ADR 0009's grain.) Charter says `NOT RUN` because it asked the
head sha, and it refuses. **This is the single clearest thing charter adds over the shell
loop**, and it is a correctness difference, not an ergonomic one.

`charter-metrics` lands. Two of three.

**7. Somebody says no.** `charter-jira`'s maintainer does not want API 2 and will pin API 1
instead. They close #7.

```
$ charter change show component-api-2
component-api-2 · PARTIALLY LANDED (2 of 3) · REJECTED: charter-jira (#7 closed unmerged)

$ charter change drop component-api-2 charter-jira --why "maintainer pins API 1; will migrate in their own time"
✓ charter-jira excluded — 2 members, 2 landed
```

The change is now fully landed across its two members, and the record permanently says a third
was considered, attempted, and excluded, with the reason. Nothing was rolled back, because
nothing needed to be: the ordering held, and the member that said no was downstream of the
ones that did not.

**8. Six months later**, from any of three starting points:

- From `charter`'s history: `git log main --grep='Charter-Change: component-api-2'` → `e0c9d13`.
- From either pull request: the cross-link block names the other, and names `charter-jira`.
- From the plane: `workspaces/providers/changes/component-api-2.json` holds the `why`, the two
  members, and the exclusion with its reason and its date.

None of the three needs charter to be installed. That is the "state no single repo's history
explains" adversary answered — the state is explained in three places, two of which are inside
the repositories themselves.

**The skeleton generalises** — one intent, an ordering, N reviews, and one member who may say
no — which is why a renamed forge field, a moved config key and a rotated credential all walk
this same path.

---

## 5. The objections

Most are answered inline, under the decision they attack. Two are big enough to need their own
room.

### 5.1 "This is `gh pr create` in a loop with extra steps."

Start with the part that is true: **for a two-repo change you will do once, the shell loop is
correct and you should write it.** Charter earns its keep at three or more members, or when
the change outlives one sitting — which is the case that produced this spec, because the
interesting state is never "create five PRs". It is *"it is Thursday, three landed, one was
rejected, and the fourth's checks never ran."*

Five differences, in descending order of how much they matter.

**1. Correctness, not ergonomics.** A script reads `gh pr checks` or `mergeStateStatus`,
because those are what the CLI puts in front of you. Both report a run that never happened
identically to a clean pass, and the merge button is offered anyway (#561). Charter reads
check runs at the head sha and has a word for the case the CLI has no word for. That is the
difference between a loop that merges an untested branch and one that refuses.

**2. The loop is the thing charter refuses.** The script's whole competence is the loop.
Charter's is the refusal, plus the three gates the loop would batch into one call (§3.3).
"What does charter add over the loop" is the question with the answer in it.

**3. A script has no memory between runs.** Which repositories were considered and excluded
and why; which member blocks which; what the change is *for*. None of it survives the script
exiting. The record does — and in a LIVE workspace it is committed, so it survives the laptop
and the author too.

**4. The cross-link is maintained, not written once.** Five PR bodies written by a loop are
stale the first time membership changes. Charter regenerates the block between its own
markers, and refuses to write at all when they are absent or unbalanced.

**5. Names in a script go straight into a shell.** In charter a change name, a `why` and a
repo name pass `contain.one_line` before they reach a row, a table cell or an argv. This
repository has already shipped that class of bug: a newline reaching tmux config text through
`[frame] hotkey` achieved code execution at launch. #498 is the neighbouring case — a committed
name reaching a *display* surface, where the damage is a mangled row rather than a command — and
the two share the boundary, not the blast radius.

And the honest sixth, which is the smallest: a script has no surface. No component in the
frame, no picker, and no way to answer *"what am I in the middle of"* three days later.

### 5.2 "Different owners, different CI, different review rules, and one of them says no."

That is the normal case, not the failure case, and three decisions above exist for it.

**Different owners** is why charter never computes "the change is approved". Approval is
granted per repository by different people, and a single word would hide the one who has not
answered. Charter reports each member's review state and names the ones outstanding (§3.5).

**Different CI** is why the state model has five values rather than a boolean, and why
`UNKNOWN` is distinct from `NOT RUN`. A repository with no CI at all sits permanently at
`NOT RUN`, which is correct, and which refuses the landing gate — the fix being a human who
decides that repository needs no check and merges it themselves. **Charter offers no `--force`
on the check gate**, because a force flag on a gate is the gate deleted with extra steps.

**Different review rules are the host's, not charter's** — ADR 0014's rule, one host out.
Charter never queries or reasons about branch protection, required checks or CODEOWNERS. It
pushes, it opens, it reads back, and where the forge refuses, **the refusal is the evidence**.
`planegit` already works exactly this way, declining to predict a protected branch because
*"guessing it from the branch name is precisely the unearned diagnosis ADR 0009 forbids."*

**And one of them says no** is `charter change drop <slug> <repo> --why "…"`. The member
leaves `members` and joins `excluded` with a reason and a date. If members had already landed,
the world stays partially changed, permanently — and the only thing that makes that state
readable six months later is that somebody wrote the reason down at the moment they still knew
it. That is what `--why` being **required** is for, and it is the cheapest line in this spec.

---

## 6. Security

Priority one on this project, and a cross-repo change means charter acting with the operator's
authority across **several** remotes. Read `docs/superpowers/specs/2026-08-24-security-assessment.md`
first; this section is written against its standard, which is that a claim false in a reachable
configuration is worse than no claim.

### 6.1 The containment property, stated as a property

> **Membership is committed. Destination is local.**

This is §4b's *"Arrangement is committed. Execution is local."* applied to a different
committed file, and the reasoning transfers without modification. `charter.toml` may say
*which* components to place and never *what code runs*, because a command string in a
committed file is executable content that arrives from someone else's machine. A change record
may say *which repositories* are members and never *where they are*, for exactly the same
reason: a remote URL in a committed file is a **destination** that arrives from someone else's
machine.

Five consequences. Together they deliver a property rather than a list of repository names —
and the property, stated at the strength the mechanism actually has: **charter's change surface
can only touch a repository the operator already cloned into this workspace, and only the branch
and remote that clone itself defines.**

That is narrower than "a repo the operator did not intend to touch", and the gap is worth
naming: a record can name a repository the operator cloned for some *other* reason, and it will
resolve. What stops that being silent is rule 2 — membership is enumerated by hand and, in a LIVE
workspace, reviewable in a diff — not the resolver.

**1. The record carries no URL, no host, no remote name, and no forge kind.** It carries bare
repo names. The push destination is resolved from the clone's own `origin`, under
`gitpolicy`'s one-credential rule, which already resolves the forge per clone rather than from
the plane's first `[[forge]]` block. A hostile committed record can name a repository you do
not have — and be refused for that — but it can never name a place.

**2. No expansion, anywhere.** Membership is enumerated by a command carrying literal names.
There is no glob, no pattern, no `--all-repos`, no "every repo in the workspace", and nothing
in the change surface ever calls `list_repos`. Every member in the record was typed by
somebody, and in a **LIVE** workspace the record is committed, so it is reviewable in a diff.

That condition is not a weakness, because it cuts both ways: a record can only have arrived from
someone else's machine if the workspace is LIVE, and a LIVE workspace's record is exactly the one
a reviewer sees. **A record is untrusted precisely when it is reviewable.** In a LOCAL workspace
nothing arrives from anywhere and there is nothing to review; the containment in rules 1, 3, 4 and
5 still applies, because a record written by an agent on this machine is not more trustworthy for
being local.

**3. No escape.** Each member resolves through `contain.child(workspace_dir, name)` — which
refuses rather than sanitises, because *"silently rewriting a name invents a second
identity"* — and the resolved path must satisfy `workspace.is_clone()` under
`workspaces/<ws>/`. A member naming `..`, an absolute path, a drive, or a symlink out of the
workspace is refused **by name**, and that refusal is the thing a test asserts.

**4. The base branch is read from the clone, never from the record.** A PR opens against the
clone's own default branch. A base branch in a committed record is a destination in a
committed file, which is what rule 1 forbids — so charter does not support stacked bases in a
change, and says so. Stacking inside one repository is a piece's problem, not a change's.

**5. A branch name out of the record is argv, and ref grammar is not argv safety.**
`git check-ref-format` **accepts** `refs/heads/-b` — a leading dash is legal inside a ref,
measured against git 2.50.1. A member's branch is read from a committed file and handed
straight to git, so `{"branch": "-b"}` in a record someone else wrote is a flag, not a branch.
Two mechanisms, not one: the record boundary refuses a branch beginning with `-`, **and** every
git invocation that carries one places it after `--`. Either alone has been enough to ship a
bug in this repository, and `check-ref-format` would have passed this record.

### 6.2 What an agent may do without asking, and what always needs the human

**Without asking** — because each is a single-repo act the standing rule already permits, or
reads nothing: create a change; add and drop members that resolve to clones already present;
create and check out member branches; commit; push a member branch; open a member's PR; update
the delimited cross-link block; refresh derived state; and **land one member** when every gate
passes.

**`land` appears in both lists, and the split is attended versus unattended.** Attended — a
person at a keyboard, whatever they are or are not watching — an agent may land one member,
because that is the merge the standing rule already permits for a single repo. Unattended, under
`bypassPermissions`, it may not land at all, because that is where the existing floor sits. The
two sentences are not in tension; they are the same rule read in the two modes the harness
reports.

**Always the human** — and the important finding is that **the line already exists in shipped
code and Phase 4 does not move it**:

`hooks._release_floor_reason` denies all four of `_PUBLISH_FORGE` — `gh pr merge`,
`glab mr merge`, `gh release create`, `glab release create` — plus tag creation and tag pushes,
under `bypassPermissions`, and denies rather than asks because an unattended `_ask`
falls back to `allow`. It does **not** deny `gh pr create`. So the project's floor already
sits between *opening* a pull request and *merging* one, which is precisely where the standing
rule — **implement, PR, merge, but never release alone** — puts it for a single repo.

Phase 4's obligations follow mechanically:

- **`("charter", "change", "land")` joins `hooks._PUBLISH_FORGE` — and the set's reader has to
  widen with it.** `_release_floor_reason` consults that set only inside
  `elif base in ("gh", "glab")`, so a tuple whose base is `charter` is unreachable today: adding
  the entry alone ships a dead line, and a mutation that deletes it would stay green. **In a
  phase whose thesis is that a guard nobody pins is a comment with a runtime cost, shipping a
  floor that never runs would be the document refuting itself.** The widening is one condition
  and it is budgeted in Task 9. Without it, `charter change land` is a documented way around a
  floor charter itself wrote.
- **`change` joins `toolgate._DANGEROUS["charter"]`**, beside `secret` and `vault`, so a
  persona declaring `tools: charter` never gets a landing auto-approved. The grain is coarse —
  it will also decline to auto-approve `charter change show` — and the asymmetry decides it:
  over-refusing costs one permission prompt on a read-only command, while under-refusing
  auto-approves a merge. The gate never denies, so the cost of the coarse grain is bounded at
  a prompt.

**Where cross-repo landing sits relative to release, reasoned rather than asserted.** What
makes a release different from a merge is irreversibility: a merge is undone by another
commit, a published artifact is not. A cross-repo landing is N merges, each individually
revertible — so it is *not* a release, and treating it as one would be stricter than the
standing rule and would push agents onto raw `gh pr merge`, which is outside every guard in
this document. But the **set** has one property a single merge does not: between the first
merge and the last, the world is in a state no repository's history explains, and a repo that
deploys on merge has already shipped its half. That property is what `--all` would make
unattended, and it is the reason `--all` does not exist — not a reason to require a human per
merge.

**So: no new human gate, deliberately.** A second consent the agent satisfies by running one
more command is theatre, and the security assessment already named that failure mode — an
operator who is prompted constantly rubber-stamps within a day, *"which is worse than no
gate"*. The operator who wants a prompt has one, for free, today:

```
charter guard ask --local 'charter change land *'
```

That writes a host `permissions.ask` rule — ADR 0014's rule, that policy expressible as a
command pattern belongs to the host — and because Claude Code matches on the full command
string, the prompt shows **which change and which repo**.

**`--local` is load-bearing in that line, not a convenience.** `guard ask` writes the plane's
*committed* `.claude/settings.json` by default; `--local` writes the gitignored
`.claude/settings.local.json` instead. Consent that travels in a commit enrols a whole team on
one person's click, which is exactly why ADR 0003 put reporting consent in user-level config
and out of `charter.toml`. A team that genuinely wants the prompt for everybody can drop the
flag — that is a decision, made once, visible in a diff.

`charter doctor` names this command in the change section. **Charter does not run it**, for the
same reason: charter states the trade-off in its own output and does not settle a legitimate
choice by writing a line while nobody is looking (ADR 0017).

### 6.3 Every landing is traced, refusals included

```
trace.record('change-land', change=…, repo=…, number=…, head=…, checks=…, refused=…)
```

Unconditionally, on the refusal path as well as the success path.

The security assessment found `charter/commands_secrets.py` had **no trace calls at all**, so
after the fact charter could not answer *"which command received the prod token"*, and called
fixing that *"independently the highest-value observability change in the repo"*. **That hole
is closed** — `commands_secrets.py` now records against a `trace.SECRET_USE_EVENTS` vocabulary,
scrubbing values out of what charter is about to write into its own trace. So this is not a new
argument; it is the shape charter already adopted for the last surface that acted with the
operator's authority, applied to the next one. A surface that merges code into several
repositories does not get to ship without it.

### 6.4 What is honestly still open

- The tool gate matches on the binary, so `bash -c 'charter change land …'` hides `prog` from
  the same guard that `--reveal` bypasses hide from. Unchanged by this phase, unfixable by
  name-based matching, and already stated in `SECURITY.md`'s own limits list, which says in as many words that the
  guard *"reads the argv it is given, and does not re-parse a shell string"*.
- `hooks._release_floor_reason` reads the harness's own `permission_mode`. A harness that does
  not report one is treated as attended — failing toward asking, which is correct, and which
  means the floor's strength varies with the harness. `doctor` names harness deficits already
  (ADR 0015); the change floor is one more line in that report.
- Nothing here is a boundary. `SECURITY.md`'s position — *"guard rails, not guarantees"* — is
  unchanged, and this spec claims no more than the guards it names actually do.

---

## 7. What charter refuses, collected

Gathered in one place because the refusals are the design, and a reader who skims should hit
them together.

1. **No `--all` on `land`.** The flag does not exist, and a test asserts its absence. (§3.3,
   ADR 0020.)
2. **No automatic cross-repo rollback.** A revert is a new change. (§3.7.)
3. **No synthetic monorepo on disk.** No symlink farm, no union mount, no subtree. (§3.4.)
4. **No aggregate green.** A change is never reported greener than its worst member, and one
   member charter could not read makes the change `UNKNOWN`. (§3.5.)
5. **No `gh pr checks`, no `mergeStateStatus`.** Both report a run that never happened as a
   pass. Checks are read per head sha or not at all. (§3.5, #561.)
6. **No stored state.** No PR number, no CI result, no branch position, no `landed` flag in
   the record. (§3.1, §4f, ADR 0011.)
7. **No destination in a committed file.** No URL, host, remote or base branch in the record.
   (§6.1.)
8. **No expansion.** No glob, no pattern, no `list_repos` anywhere in the change surface.
   (§6.1.)
9. **No force-push, no branch deletion, no default-branch reset, no closing a PR charter did
   not open** — in a change action, including during a revert. (§3.7.)
10. **No unattended landing.** `charter change land` joins the release floor. (§6.2.)
11. **No auto-closing of todos**, and no todo state. ADR 0012 is untouched. (§3.1.)
12. **No claim of atomicity**, anywhere, in any wording. (§3.3.)

---

## 8. Open for the operator

Two, both stated with a recommendation so they can be answered in one word.

### 8.1 Does `charter change land` exist at all?

The alternative is that charter stops at *"everything is green and in order — here are the
merge commands"* and the human runs them.

**Recommendation: it exists, one member per invocation. — YES.**

**Reasoning.** The standing rule already permits merge for a single repo, and the gates that
make cross-repo landing safe — the blocker check, the per-head-sha check gate, the read-back,
the trailer, the trace — only exist if charter is the one merging. Withholding the verb does
not remove the merges; it routes them through raw `gh pr merge`, which has none of those
gates and produces no record. The security assessment made the same argument about
`secret exec` and reached the same conclusion: *"the answer is consent, not enumeration"*, and
a refusal that pushes work outside the guard is a worse outcome than the guard doing the work.
The floor that matters — no unattended merge — is enforced by `hooks._release_floor_reason`
whichever way this is answered.

**If the answer is NO**, Tasks 8 and 10 of the plan shrink to printing the exact commands and
the gates become a `charter change check` that exits non-zero. Everything else is unchanged.

### 8.2 Does Phase 4 extend the `Forge` protocol with writes, or ship read-only?

Read-only Phase 4 would push each member branch and print N compare URLs — the shape
`planegit._compare_url` already ships — and derive everything else from the **reads** Task 5
adds, which are not gated on this answer. (It could not derive it from `open_change` alone:
that returns a number for an open request and `None` for merged, closed and never-opened
alike, so `PARTIALLY LANDED` and `REJECTED` would both be unavailable.) It is roughly half the
phase. It cannot produce the cross-link block, because that needs a PR body
update.

**Recommendation: extend it. — EXTEND.**

**Reasoning.** The cross-link block is the answer to the strongest objection in this document
("a partial landing leaves a state no repository's history explains"), and it is the one
artifact that must be *maintained* rather than written once — membership changes, and five
hand-written PR bodies go stale the first time it does. ADR 0002 does not forbid this: it
forbids putting *upstream issue filing* on the protocol, because that targets one repo on
github.com and is not polymorphic. A change's PRs land on **the operator's own forges**,
which is the exact axis ADR 0002 says the protocol exists for. `planegit._compare_url`'s
comment records a scope decision — it declined a capability it did not need — not a
prohibition.

**It does, however, invalidate ADR 0002's *consequences*, and that needs saying rather than
happening.** That ADR records: *"The reporting module is the only place in charter that writes
to a forge… That concentration is deliberate: it is a single seam, which is what makes the
feature testable without touching the network."* After Task 6 there are two write seams. The
concentration argument survives in spirit — both seams are narrow, both are stdlib subprocess
calls to the forge's own CLI, both are testable without a network — but the sentence as written
becomes false, and an ADR is not left quietly false. Task 13 amends it.

The extension comes with one hard requirement, stated by ADR 0002 itself: **a write needs a
loud failure path.** `_api`'s "return `None` on any failure" is correct for the status line
and catastrophic for a write, so the protocol gains a third discipline beside permissive and
strict, in the shape of `report.ReportingError` — the existing in-tree precedent for exactly
this.

**If the answer is READ-ONLY**, drop Task 6, reduce Task 7 to push-and-print-URLs, and the
cross-link block becomes a `charter change show` output the operator pastes. §4's step 3
changes; nothing else in this spec does.

---

## 9. What this spec deliberately does not do

- **It does not reverse §4d, §4e, §4f or §4g.** Where it extends them (a fourth gather slice,
  a `changes` component) it does so in the terms §4i set.
- **It does not touch ADR 0011 or ADR 0012.** No new lifecycle, no piece state, no todo state,
  no automatic closing, no widening of `pieces.FIELDS`.
- **It does not add a `charter.toml` key.** The change surface is code and a per-workspace
  store; §8 of the parent spec ("does not add a config key per component") holds here too.
- **It amends ADR 0002's consequences rather than reversing its decision** (§8.2), and adds ADR
  0020. Those are the only two ADRs this phase touches, and it touches both in writing.
- **It does not answer §4j's question** — *"show me the chats working on this change"*. Phase
  5 makes that askable; neither phase answers it, and that is recorded rather than left to be
  discovered.

---

## 10. The plan

> **For agentic workers:** REQUIRED SUB-SKILL: `superpowers:subagent-driven-development`.
> Steps use checkbox (`- [ ]`) syntax.

**Goal:** one change spanning N repositories is a committed record charter can create, read,
push, gate and land one member at a time — with `NOT RUN` distinguishable from `PASSED`
everywhere, and no path that lands more than one member per invocation.

**Spec:** this document, §3–§8. **§8's two answers gate Tasks 6, 7, 8 and 10** — do not start
them until both are answered.
**Parent spec:** `docs/superpowers/specs/2026-08-25-agentic-ide-foundation.md` §4d–§4j.
**Depends on:** Phases 0–3 complete.

*(When Phase 4 is approved, this section moves to
`docs/superpowers/plans/<date>-phase4-cross-repo-change.md` unchanged. It lives here because a
plan in `plans/` reads as ready to execute, and this phase is not approved.)*

### Global constraints

- `dependencies = []`, stdlib only. stdlib `unittest`, never pytest.
- Run the suite in **two environments** and say which; they must agree. **The count is
  deliberately not pinned here**: Phase 2's 5880 is stale by the time this phase starts, and a
  number carried forward from another branch is a fixture, not a baseline. Measure it at this
  branch's head as Task 1 Step 0 and write it into this line then. **Measured at
  `2fe663a` (the merge-base for Tasks 1–4): 7385 tests, OK, in both — CPython 3.14.4, and
  CPython 3.12 with `CHARTER_*`/`CLAUDE_*`/`ANTHROPIC_*`/`TMUX*` unset.**
- `PersonaIso`; new `patch.dict(os.environ, …)` MUST pass `clear=True`.
- `tui.width()` never `len()`. `contain.one_line` **before** width arithmetic.
- Repo names use `contain.segment_ok`, **never** `workspace.valid_name` — `org/.github` is a
  real repository name and `valid_name` rejects it. A change slug is validated at creation by a
  rule of `instance.WORKSPACE_NAME_RE`'s shape, in `instance`, one rule in one place — and
  **contained again on read**, because the operator types it, the record is committed, and a
  record can arrive from an older charter or a hand edit. Creation-time validation and
  containment answer different questions; #442 and #503 are both what happens when one is
  mistaken for the other.
- State writes go through `config.write_for` / `contain.writable`; committed-file reads ask
  **both** `contain.dir_refusal(parent)` and `contain.file_refusal(path)` (#336).
- Mutation-test every guard: apply, RED, restore, GREEN, `__pycache__` cleared. **Report the
  mutation actually run and its actual result.**
- **A refusal test asserts WHICH refusal fired**, not the exit code. Two guards in sequence
  mask each other: on #558, deleting the `-z "$claimed"` refusal still exited 1, for a worse
  reason. This phase has three gates in sequence on one command; an exit-code assertion cannot
  tell them apart and will stay green over a real deletion.
- No version bump, no stamping, no tag.

### The deletion sweep — required before any PR in this phase

**For every `if` you add that refuses, clamps, contains or falls back, write the test that
goes RED when that line is deleted.** Then run the sweep yourself — delete each new guard in
turn, run the full suite, and report any that stayed green **before** submitting.

```
python3 tools/sweep.py                 # this branch, against its merge-base
python3 tools/sweep.py --path charter  # the default scope
```

See `docs/superpowers/specs/2026-08-27-deletion-sweep-harness.md`. Rounds one through three
found thirty-six unpinned guards by hand, and round three's own fix commit — the one whose
message says every added guard now has a test — added six more. **This phase is unusually
exposed to it:** almost every line it adds is a refusal, and a refusal nobody pins is a comment
with a runtime cost. Exit `3` (unresolved) is not a pass — a gate must not treat "I could not
look" as "nothing to see", which is this phase's own subject matter one level up.

---

### Task 1: The change record

**Files:** create `charter/change.py`, `tests/test_change_record.py`.

- [ ] **Step 0: measure and record the suite count** at this branch's head, in two
      environments, and write it into Global constraints. A baseline nobody measured is a
      fixture.
- [ ] **Step 1: write the failing test.**

```python
def test_the_key_set_is_closed_and_an_unknown_key_is_named(self):
    p = change.path_for("ws", "component-api-2")
    p.write_text('{"change":"component-api-2","why":"x","created":"…","by":"y",'
                 '"members":[{"repo":"api","branch":"b","need":["web"]}],"excluded":[]}')
    with self.assertRaises(change.RecordError) as cm:
        change.read("ws", "component-api-2")
    self.assertIn("need", str(cm.exception))     # names the key, does not ignore it
```

- [ ] **Step 2: run it, confirm it fails** (`ModuleNotFoundError`).
- [ ] **Step 3: implement read/write.** `changes_dir(ws)`, `path_for(ws, slug)`,
      `read(ws, slug)`, `write(ws, slug, rec)`, `all_for(ws)`. JSON, 2-space indent, trailing
      newline — `workspace.write_manifest`'s shape, through `contain.writable`.
- [ ] **Step 4: the closed key set**, top level and per member, refused by name. #503's rule:
      a key charter does not read reads as nothing at all, and `need` for `needs` is an
      ordering constraint that silently ceased to exist.
- [ ] **Step 5: read through both refusals** — `dir_refusal(parent)` **and**
      `file_refusal(path)`. When the *directory* is the link, every file inside it is an
      ordinary regular file with nothing to object to (#336). Not #442 — that one is the
      *name* rungs reaching `workspace_dir()` uncontained, which is a different guard.
- [ ] **Step 6: no state field is representable.** A record carrying `state`, `landed`, `pr`
      or `ci` is refused by the same closed-set check. Test each name explicitly.
- [ ] **Step 7: a failed read raises; it never answers an empty record.** `add` and `drop` are
      read-modify-write, and a read that degrades to `{}` writes back a record holding only the
      new member and drops every sibling. That is not hypothetical: `onepassword._fields`
      returned `{}` for every non-zero `op item get`, a rate-limited vault printed "has no
      secrets" (#322), and the read-modify-write behind it piped back a template holding only
      the new key. Test the exact sequence — unreadable file, then `add` — and assert nothing
      was written.
- [ ] **Step 8: `create` requires `--why`.** A change with no stated reason is unreadable six
      months later, which is the one job the record has that git cannot do.
- [ ] **Step 9: mutations** — accept an unknown key; drop the `file_refusal` half; sanitise a
      bad slug instead of refusing; make a failed read answer `{}`; make `--why` optional.
      Each RED, each restored GREEN.
- [ ] **Step 10: commit.**

---

### Task 2: The workspace grows a fourth store

**Files:** modify `charter/workspace.py`, `charter/commands_workspace.py`,
`tests/test_todos_are_committed.py` (extend, do not replace).

- [ ] **Step 1: failing test** — a fresh workspace has `changes/`; a workspace at
      `STRUCTURE_VERSION` 2 reports `needs_reinit`, and `reinit` creates `changes/` without
      touching anything written.
- [ ] **Step 2: run it, confirm it fails.**
- [ ] **Step 3: `changes/` is created lazily, by the first `charter change create` — NOT by
      `scaffold()`, and it is NOT in `_required_components`.** `_ws_meta_paths`' own docstring
      says why: those paths go to git as literal arguments, and `git rm --cached` on one that
      was never tracked *"fails the whole call"*. `todos/` is safe only because it is created
      lazily, so its existence filter doubles as a non-emptiness filter. An always-present,
      always-empty `changes/` breaks that proxy and makes `charter workspace live --off` fail
      whole — untracking nothing, and leaving the manifest and memory committed on a workspace
      the operator just made private. `changes/log/` is created with the first landing, for the
      same reason.
- [ ] **Step 3b: `STRUCTURE_VERSION` 2 → 3 anyway.** Not to create a directory — to make every
      existing workspace flag itself so `reinit` runs `refresh_live_block()` and a LIVE
      workspace picks up the new un-ignore lines. Test that a v2 workspace reports
      `needs_reinit`, that `reinit` adds the lines, and that it creates no `changes/`.
- [ ] **Step 4: the LIVE half, and the asymmetry inside it.** `changes` joins
      `workspace._live_block` **and** `commands_workspace._ws_meta_paths`. `changes/log/` joins
      **neither** — it is a declaration log holding merge shas, never committed, exactly as
      `pieces/` is never committed.
- [ ] **Step 4b: the parity test, in the direction nobody wrote.** The two lists cannot be
      compared as sets — `_live_block` emits `!/`-prefixed lines including a `/**` half, and
      `_ws_meta_paths` emits bare repo-relative paths, existence-filtered. The shipped test
      (`tests/test_todos_are_committed.py`) asserts **one direction only**: everything
      committed is un-ignored. Add the reverse — everything un-ignored is committable — after
      normalising both sides, and assert `changes/log` is in neither. Without the reverse
      direction, Step 6's first mutation stays green, which is a mutation that proves nothing
      in the phase whose constraint is "report the mutation actually run and its actual
      result".
- [ ] **Step 5:** full suite, no existing test modified.
- [ ] **Step 6: mutations** — add to `_live_block` but not `_ws_meta_paths` (RED on the new
      reverse-direction test from Step 4, **not** on the shipped one, which only checks the
      other way round); drop `changes/log` from the exclusion so it is un-ignored (RED); create
      `changes/` in `scaffold()` (RED on the `live --off` test from Step 3); do not bump
      `STRUCTURE_VERSION` (RED on the stale-workspace test). Each RED.
- [ ] **Step 7: commit.**

---

### Task 3: `charter change create | add | drop | list | show | forget` — records only, no forge

**Files:** create `charter/commands_change.py`, `tests/test_commands_change.py`; modify
`charter/cli.py` (`_add_change_parser`), `charter/instance.py` (`change_name_ok`).

- [ ] **Step 1: failing test** — `create`, then `add` a repo with a clone, then `show` prints
      the member; `add` of a repo with **no** clone exits 2 and names `charter clone`.
- [ ] **Step 2: run it, confirm it fails.**
- [ ] **Step 3: implement**, house style: `cmd_change_<verb>(args) -> int`, `-w/--workspace`,
      `print()` for the answer and `util.*` for everything else, `workspace.banner` on entry.
- [ ] **Step 4: the containment property (§6.1).** A member resolves through
      `contain.child(workspace_dir, name)` — refuse, never sanitise — and the result must
      satisfy `workspace.is_clone()`. Test with `..`, an absolute path, a NUL, a name that is
      a symlink out of the workspace, and `.github` (which must be **accepted**, because
      `segment_ok` is the right rule and `valid_name` is not).
- [ ] **Step 5: exit 2 is the specific refusal**, as `commands_worktree.CLAIM_TAKEN` is —
      no-clone, unknown-change, duplicate-member and cycle each get their own message, and the
      test asserts **which** one fired.
- [ ] **Step 6: `drop` writes the exclusion**, with `--why` required, into `excluded` with a
      timestamp. A dropped member never silently vanishes.
- [ ] **Step 6c: `forget` deletes the record**, by slug, through `contain.segment_ok` and
      `memstore.resolve`'s shape. It deletes no landing-log line and no branch — the log is a
      past-tense declaration and deleting history to tidy a list is how a store starts lying.
- [ ] **Step 6b: a branch name is argv, not a ref.** Refuse a `branch` beginning with `-` at
      the record boundary. `git check-ref-format` **accepts** `refs/heads/-b` — measured on git
      2.50.1 — so ref grammar proves nothing here. Test with `-b`, `--upload-pack=…`, a
      newline, and a name `check-ref-format` accepts but argv does not.
- [ ] **Step 7: no expansion exists.** Assert by grep-shaped test that
      `charter/commands_change.py` calls no `list_repos` and accepts no pattern argument.
- [ ] **Step 8: mutations** — swap `segment_ok` for `valid_name` (RED on `.github`); accept a
      member with no clone; accept `..`; make `drop` optional-`--why`; replace the leading-dash
      refusal with `git check-ref-format` (RED, because it accepts `refs/heads/-b`). Each RED.
- [ ] **Step 9: commit.**

---

### Task 4: Ordering, declared and derived — never stored

**Files:** modify `charter/change.py`, `charter/commands_change.py`.

- [ ] **Step 1: failing test** — `needs` refuses a cycle at write time, naming both members;
      `blocked_members(rec, landed)` returns the derived set and nothing writes it.
- [ ] **Step 2: run it, confirm it fails.**
- [ ] **Step 3:** `needs` accepts only repo names that are already members; a cycle is refused
      by name; the derivation is a pure function of the record plus a landing map.
- [ ] **Step 4: nothing derived reaches disk.** Test that `write` of a record that has been
      *rendered* produces bytes identical to the record as read — the round-trip is the
      assertion that ADR 0011 has not been reversed by convenience.
- [ ] **Step 4b: `write` validates the closed key set too**, not only `read`. Without it the
      Step 5 mutation below is a mutation that proves nothing: caching a derived set on the
      in-memory record leaves the serialised bytes identical unless something on the write path
      objects.
- [ ] **Step 5: mutations** — allow a cycle; allow `needs` to name a non-member; drop the
      write-side key check and cache the derived blocked set on the record. Each RED.
- [ ] **Step 6: commit.**

---

### Task 5: The forge learns to say "did not run"

**Files:** modify `charter/forge/base.py`, `github.py`, `gitlab.py`; create
`tests/test_forge_checks_at.py`.

*Gates nothing in §8; do this even if both answers change the rest of the phase.*

- [ ] **Step 1: write the failing test.**

```python
def test_zero_check_runs_is_not_a_pass_and_is_not_unknown(self):
    with fake_gh({"total_count": 0, "check_runs": []}):
        r = forge.checks_at("o/r", "9f3a1c2")
    self.assertEqual(r.total, 0)
    self.assertEqual(r.state, "not_run")

def test_a_failed_call_is_unknown_and_is_not_not_run(self):
    with fake_gh_failure():
        r = forge.checks_at("o/r", "9f3a1c2")
    self.assertIsNone(r.total)                 # the ONLY way to say "could not ask"
    self.assertEqual(r.state, "unknown")
```

- [ ] **Step 2: run it, confirm it fails** (`AttributeError`).
- [ ] **Step 3: implement `checks_at(path, sha)`** returning a frozen record with
      `total: int | None` and `state`. Two fields, because a single string is what made `None`
      mean six things. GitHub: `repos/<o>/<r>/commits/<sha>/check-runs`. GitLab: pipelines
      filtered to that sha.
- [ ] **Step 4: the pull request as more than a number.** Add a read returning the number,
      the state (`open` / `merged` / `closed`), the head sha and the merge commit when merged.
      `open_change` answers `int | None` for **open** requests only, so today a closed-unmerged
      member and a member with no pull request at all are the same value — and `PARTIALLY
      LANDED` and `REJECTED` are both underivable. Leave `open_change` in place for the status
      line; do not widen it.
- [ ] **Step 5: `ci_status` is left exactly as it is.** The permissive discipline is correct
      for the status line; the change surface simply does not use it. Do not "improve" it in
      this task — that is a separate change with its own blast radius.
- [ ] **Step 4b: see every check the forge would show a human.** On GitHub, check runs **and**
      the combined commit status at that sha, summed into one `total` — the check-runs endpoint
      alone misses every Jenkins/Buildkite/CircleCI status and would render a green head as
      `NOT RUN`, permanently. On GitLab, read the merge request's own pipelines: merged-results
      pipelines run against `refs/merge-requests/:iid/merge`, so a head-sha filter is empty on a
      green MR. Test both with a fixture that has zero check runs and one passing status.
- [ ] **Step 4c: incomplete enumeration answers `UNKNOWN`, never `NOT RUN`.** `NOT RUN` asserts
      nothing ran, and charter may only assert it having looked everywhere it knows to look.
- [ ] **Step 6: forbidden inputs.** Assert the change path never invokes `gh pr checks` and
      never reads `mergeStateStatus` — both report a run that never happened identically to a
      pass (#561). **Write this assertion in Task 8, not here**: at Task 5 there is no landing
      path, so the test would pass against a module that could not fail it — §4i's convincing
      empty, in the phase that quotes §4i approvingly.
- [ ] **Step 6b: `charter change show` gains its derived columns.** Task 3 built it from records
      alone; §3.4 calls it the monorepo view and §4 shows it printing request numbers, check
      state and landing dates. Those fields exist as of this task, and the command that §3.4
      names as the answer to "how does the agent see all of it at once" is unusable without
      them.
- [ ] **Step 7: map every conclusion the forge can return**, including `neutral`, `skipped`,
      `cancelled`, `timed_out`, `startup_failure`, `action_required`, `stale`. An unmapped
      conclusion degrades to `unknown`, never to `passed`.
- [ ] **Step 8: mutations** — make `total == 0` render as passed; collapse `None` and `0` into
      one value; derive state from `mergeStateStatus`; map an unknown conclusion to `passed`;
      make a closed-unmerged request read as "no request". Each RED.
- [ ] **Step 9: commit.**

---

### Task 6: The forge learns to write, loudly

*Gated on §8.2 = EXTEND. If READ-ONLY, skip this task.*

**Files:** modify `charter/forge/base.py`, `github.py`, `gitlab.py`; create
`tests/test_forge_writes.py`.

- [ ] **Step 1: failing test** — `create_change(...)` returns a number; a failing CLI call
      **raises** and does not return `None`.
- [ ] **Step 2: run it, confirm it fails.**
- [ ] **Step 3: implement** `create_change(path, base, head, title, body) -> int` and
      `update_change_body(path, number, body) -> None`.
- [ ] **Step 4: the third discipline.** A write never routes through `_api`. ADR 0002:
      *"A write needs to fail loudly, which means it needs a path that is not `_api`."* Model
      the exception on `report.ReportingError`.
- [ ] **Step 5: `-f` never `-F`** on `gh api` (#323) — `-F` gives an `@`-prefixed value
      file-read semantics, which turned a status refresh into an arbitrary local file read by
      a process holding the forge token. A change's title and body carry the `why` and the
      member names, all committed values from someone else's machine, so this applies harder
      on a write than it did on the read it was found in. Test the argv.
- [ ] **Step 6: mutation** — route a write through `_api` so a failure returns `None`;
      confirm RED. A swallowed write failure must fail a test, not a review.
- [ ] **Step 7: commit.**

---

### Task 7: `charter change push` — branches, PRs, and the cross-link block

*Shape depends on §8.2.*

**Files:** modify `charter/commands_change.py`; create `tests/test_change_push.py`.

- [ ] **Step 1: failing test** — push writes the block between the markers and leaves prose
      above and below it byte-identical.
- [ ] **Step 2: run it, confirm it fails.**
- [ ] **Step 3: push each member branch** to the clone's own `origin`, under `gitpolicy`. The
      remote comes from the clone, never from the record (§6.1). **Every git invocation that
      carries a branch name from the record places it after `--`** — the boundary check in Task
      3 and the argv position are two mechanisms, and this phase ships both because either
      alone has already been enough to ship a bug here.
- [ ] **Step 4: the base is the clone's default branch**, never a record field. Test that no
      record key can influence it.
- [ ] **Step 5: the delimited block.** `BEGIN`/`END` markers in the shape of
      `workspace._LIVE_BEGIN`/`_LIVE_END` and `render.PERSONAS_BEGIN`/`END`. **Refuse to write
      when the markers are absent or unbalanced** — anything else edits a human's words. Test
      with: no markers, one marker, markers in the wrong order, markers inside a fenced code
      block.
- [ ] **Step 6: containment on the way out.** The change name, the `why`, every repo name and
      **every branch name** go through `contain.one_line` before they reach the block. The
      branch field is the one that also crosses into argv (§6.1 rule 5), so it needs both
      treatments and neither substitutes for the other. A `why` containing a newline,
      U+2028, a pipe or a backtick renders as one row and closes no table.
- [ ] **Step 7: mutations** — read the base from the record; write with a marker missing;
      write outside the markers; skip containment; drop the `--` from a git argv carrying the
      branch (assert with a `-b` branch that the mutation reaches git as a flag). Each RED.
- [ ] **Step 8: commit.**

---

### Task 8: `charter change land` — one member, three gates, and the flag that does not exist

*Gated on §8.1 = YES. If NO, this task prints the exact commands and the gates become
`charter change check`, exiting non-zero.*

**Files:** modify `charter/commands_change.py`; create `tests/test_change_land.py`.

- [ ] **Step 1: write the failing test.**

```python
def test_there_is_no_all_flag(self):
    p = cli.build_parser()
    with self.assertRaises(SystemExit):
        p.parse_args(["change", "land", "component-api-2", "--all"])

def test_a_head_with_zero_check_runs_is_refused_by_the_check_gate(self):
    rc, err = run_land(checks=Checks(total=0, state="not_run"))
    self.assertEqual(rc, 2)
    self.assertIn("NOT RUN", err)          # WHICH gate, not just that one fired
    self.assertNotIn("blocker", err)       # and not the gate above it
```

- [ ] **Step 2: run it, confirm it fails.**
- [ ] **Step 3: the three gates, in order**, each with its own message: (a) the member has a
      request and it is open; (b) every repo in `needs` has landed; (c) `checks_at(head).state
      == "passed"` — `not_run` and `unknown` both refuse.
- [ ] **Step 3b: there is deliberately no mergeability gate.** Charter attempts the merge and
      the forge's refusal **is** the evidence — `planegit` already declines to ask whether a
      branch is protected for exactly this reason, and the only field that would answer it is
      `mergeStateStatus`, which §7 item 5 forbids by name. A rejected merge is reported in the
      forge's own words (ADR 0009), never re-diagnosed.
- [ ] **Step 3c: the merge method.** `--merge` or `--squash`; **refuse `--rebase`** (§3.3) and
      say why, because a rebase landing leaves charter no commit to put the trailer on and no
      single sha for `revert`. A repository that permits only rebase gets a named refusal, not
      a landing with a silently missing trailer.
- [ ] **Step 4: assert which gate fired.** Three gates in sequence mask each other and an
      exit-code assertion cannot tell them apart. Every gate gets a test that asserts its own
      message **and** the absence of its neighbours'.
- [ ] **Step 5: the trailer.** The landing commit carries `Charter-Change: <slug>`. It is the
      only trailer charter promises, because charter does not author the member's own commits
      and will not claim what it cannot guarantee. Test it on both permitted methods: a merge
      commit and a squash.
- [ ] **Step 6: read back before reporting** (ADR 0013). A success line reports what charter
      confirmed, not what it asked for: re-read the PR state and the merge sha before printing
      anything.
- [ ] **Step 7: append the declaration** to `changes/log/<host>.jsonl` — one past-tense line
      carrying the change, the repo, the number, the head and the merge sha. `O_APPEND`, no
      lock, best-effort, in `pieces.record`'s exact shape. It is written **after** the read-back
      and only for a merge charter confirmed, because a declaration of something that did not
      happen is worse than none.
- [ ] **Step 8: trace unconditionally**, success and refusal alike (§6.3). The trace and the
      log are different things: the trace records that charter was *asked*, the log records
      what git can be asked to confirm.
- [ ] **Step 9: mutations** — add `--all`; skip the blocker gate; treat `not_run` as passed;
      treat `unknown` as passed; report landed without reading back; write the log line before
      the read-back; permit `--rebase`; drop the trace on the refusal path. Each RED.
- [ ] **Step 10: commit.**

---

### Task 9: The floors

**Files:** modify `charter/hooks.py`, `charter/toolgate.py`, `charter/doctor.py`.

- [ ] **Step 1: failing test** — `charter change land …` under `permission_mode ==
      "bypassPermissions"` is **denied**, with the release-floor message.
- [ ] **Step 2: run it, confirm it fails.**
- [ ] **Step 3:** `("charter", "change", "land")` joins `hooks._PUBLISH_FORGE`, **and the
      reader widens to reach it.** The set is consulted only under
      `elif base in ("gh", "glab")` (`charter/hooks.py`), so the entry is unreachable until
      `charter` is admitted to that branch. Deny, not ask — an unattended `_ask` falls back to
      `allow`, so an ask here is indistinguishable from no guard.
- [ ] **Step 3b: prove the entry is live before trusting it.** Delete only the tuple, leaving
      the widened base check, and confirm RED. A test that passes with the entry removed is a
      test of the base check, not of the floor.
- [ ] **Step 4:** `change` joins `toolgate._DANGEROUS["charter"]`. Record in the comment why
      the coarse grain is deliberate: over-refusing costs one prompt on a read-only command;
      under-refusing auto-approves a merge, and the gate never denies, so the cost is bounded.
- [ ] **Step 5: `doctor` names the optional prompt** — `charter guard ask --local 'charter
      change land *'` — and **does not run it**. `--local` is part of the recommendation:
      without it `guard ask` writes the plane's *committed* `.claude/settings.json`, and
      consent that travels in a commit enrols a team on one person's click (ADR 0003). Test
      both halves: the text names `--local`, and doctor writes no file.
- [ ] **Step 6: mutations** — remove the `_PUBLISH_FORGE` entry; **revert the base check to
      `("gh", "glab")` while keeping the entry** (this is the mutation that would have caught
      the dead line); make it ask instead of deny; remove the `_DANGEROUS` word; have doctor
      write the rule. Each RED.
- [ ] **Step 7: commit.**

---

### Task 10: `charter change revert`

*Gated on §8.1.*

**Files:** modify `charter/commands_change.py`, `charter/change.py`; create
`tests/test_change_revert.py`.

- [ ] **Step 1: failing test** — `revert` creates a **new** change whose members are the
      landed members of the original, and whose `why` names the original slug.
- [ ] **Step 2: run it, confirm it fails.**
- [ ] **Step 3: implement.** Each member seeded with `git revert -m 1 <merge-sha>` on a new
      branch. From there it is an ordinary change with the ordinary gates.
- [ ] **Step 4: the refusals.** No force-push, no branch deletion, no default-branch reset, no
      closing a PR charter did not open. Test each by name.
- [ ] **Step 5: a member landed outside charter is named, not guessed.** No line in
      `changes/log/`, so no merge sha — `revert` says so and hands that member to a human
      rather than picking a merge commit that looks about right. ADR 0009: it degrades to
      silence, never to a confident wrong answer.
- [ ] **Step 6: mutations** — allow force-push; allow branch deletion; guess the merge sha
      from the branch name. Each RED.
- [ ] **Step 7: commit.**

---

### Task 11: The surface

**Files:** modify `charter/frame/gather.py`, `charter/frame/component.py`
(`NEEDS = ("gather", "repos", "todos")` today), `charter/frame/ctx.py` (`SERVES`); add the
`changes` component to the registry, rows to `charter/frame/picker.py`, and actions to
`charter/frame/builtin_actions.py`.

- [ ] **Step 1: failing test** — `component.NEEDS` contains `changes` **and** `ctx.SERVES`
      serves it; the two are asserted against each other in one assertion so neither can be
      fixed without the other.
- [ ] **Step 2: run it, confirm it fails.**
- [ ] **Step 3: extend `gather.scan`** to carry the change records — file reads only. §4i:
      a `needs` name answered with an empty tuple lets a component draw nothing and be
      indistinguishable from a plane that genuinely has none.
- [ ] **Step 4: no forge call on the repaint path.** The component draws the *age* of what it
      is showing; refresh is a Phase 2 action, fire-and-report, surfacing through `inflight`.
      Test that a repaint performs no subprocess.
- [ ] **Step 5: containment.** A change slug, a `why`, a repo name **or a branch name** with a
      newline, U+2028, an escape sequence or a `#` renders as exactly one row and runs nothing. `contain.one_line`
      **before** width arithmetic. **Test with hostile names, do not reason about them.**
- [ ] **Step 6: ≤2 keystrokes** — a direct toggle key for the component and a palette entry
      for the picker; an unavailable action says why.
- [ ] **Step 7: never greener than the worst member.** Test a change with one `unknown` member
      renders `UNKNOWN`, and one with a `not_run` member never renders as passed.
- [ ] **Step 8: mutations** — declare `changes` in `NEEDS` without serving it; skip
      containment; let the repaint call the forge; render the aggregate as the *best* member.
      Each RED.
- [ ] **Step 9: commit.**

---

### Task 12: Divergence, named

**Files:** modify `charter/commands_change.py`, `charter/doctor.py`; create
`tests/test_change_divergence.py`.

- [ ] **Step 1: failing test** — a member whose merge commit carries no `Charter-Change`
      trailer is reported as landed **outside charter**, at FAIL.
- [ ] **Step 2: run it, confirm it fails.**
- [ ] **Step 3: implement two checks.** (a) the forge says merged and `changes/log/` has no
      line for it — landed outside charter, so no trailer and no merge sha; (b) a branch
      matching a member's recorded branch name in a workspace clone that is **not** a member of
      any change. Both are ADR 0013 rule 2: a divergence charter can see, charter names.
- [ ] **Step 4: FAIL, not WARN.** *"WARN is not a surface. A divergence worth naming under
      rule 2 is worth FAIL."*
- [ ] **Step 5: an out-of-order landing is named** — a member that landed while a blocker had
      not. This is the honest half of §3.2's guard.
- [ ] **Step 6: mutations** — downgrade either check to WARN; drop the out-of-order report.
      Each RED.
- [ ] **Step 7: commit.**

---

### Task 13: ADR 0020, docs, and the news entry

**Files:** create `docs/adr/0020-there-is-no-cross-repo-merge-loop.md`, `docs/changes.md`,
`docs/news/unreleased-cross-repo-change.md`; modify `docs/adr/0002-…`, `docs/workspaces.md`,
`docs/forges.md`, `docs/hooks.md`.

- [ ] **Step 0: amend ADR 0002.** Its consequences say the reporting module is *"the only
      place in charter that writes to a forge"* and call that concentration deliberate. Task 6
      makes it false. Append an "Amended by Phase 4" note saying what changed and why the
      reasoning still holds; do not edit the original decision. An ADR left quietly false is a
      silent reversal wearing good manners.
- [ ] **Step 1: `docs/adr/0020-there-is-no-cross-repo-merge-loop.md`.** The decision, ADR
      0003's reasoning applied unchanged, and the "what would `--all` do at member 3 of 5"
      argument. It will be proposed again; that is what an ADR is for.
- [ ] **Step 2: `docs/changes.md`** — the user-facing page. A flag not in `docs/` does not
      exist.
- [ ] **Step 3: update `docs/workspaces.md`** (the fourth store, the structure bump),
      `docs/forges.md` (`checks_at`, and that charter now writes to a forge), and
      `docs/hooks.md` (the new floor entry).
- [ ] **Step 4: `docs/news/unreleased-cross-repo-change.md`** — fenced `---` frontmatter with
      flat keys, `version: unreleased`, `adopt: workspace reinit --all`, and **no `check:`**.
      `check:` is restricted to `news._PROBEABLE` — four command paths, none of them under
      `workspace` — and widening that allowlist is its own decision with its own reasoning,
      which this phase does not make in passing. `check:` is optional; a wrong one is not. Six
      keys are the whole set and they are matched exactly.
- [ ] **Step 5: commit.**

---

### Task 14: The deletion sweep, run by the repository

**Files:** none — this task changes code only where it finds a survivor.

- [ ] **Step 1:** `python3 tools/sweep.py` on this branch against its merge-base.
- [ ] **Step 2: report every survivor**, with the test ids that cover the mutated symbol.
      This phase is almost entirely refusals; a survivor here is a guard a later refactor can
      delete in silence.
- [ ] **Step 3: exit 3 is not a pass.** An unresolved verdict means the sweep could not look,
      and a gate that treats that as clean is this phase's own subject matter one level up.
- [ ] **Step 4: fix or pin every survivor.** No suppression list — if deleting a line genuinely
      changes nothing observable, delete the line.
- [ ] **Step 5: commit.**

---

## Exit criteria

- **A change spanning three repositories can be created, pushed, gated and landed one member
  at a time**, and the record on disk holds no PR number, no CI result and no landed flag.
- **`NOT RUN`, `UNKNOWN` and `PASSED` are three different words everywhere**, and a head sha
  with zero check runs refuses the landing gate. The suite pins this against a recorded
  `total_count: 0` reply — it reaches no network, per CONTRIBUTING — and the branch **also**
  reports one manual observation against a real head sha with no run, because the suite can
  only prove charter reads a fixture correctly and #561 is a claim about what the forge does.
- **`--all` does not parse**, and a test says so.
- **An unattended run cannot land a member**, verified through the real
  `charter hook pretooluse` with `permission_mode: "bypassPermissions"`, not through the
  function in isolation.
- **A hostile change name, `why` or repo name** renders as one row, closes no table in a PR
  body, and runs nothing.
- **A member landed outside charter is named at FAIL**, and so is an out-of-order landing.
- **The `changes` component costs no subprocess on the repaint path**, measured, not asserted.
- **The suite gives the same answer in two environments and CI is green at the head sha under
  review.** Two local environments cannot see a CI-only failure: #554's overlay module passed
  12/12 locally while CI was red at that exact head, and `gh pr checks` reports "no checks
  reported" and `mergeStateStatus: CLEAN` identically when no run was ever created (#561).
  Read `gh api repos/diazoxide/charter/commits/<HEAD_SHA>/check-runs`, which cannot confuse
  the two. **A phase whose subject is that distinction does not get to merge on the reading
  that cannot make it.**
- **The deletion sweep is run by the repository, not promised by whoever wrote the branch.**
  See `docs/superpowers/specs/2026-08-27-deletion-sweep-harness.md`. The rule does not hold
  while the only thing checking it is the person it constrains.
