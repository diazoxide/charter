# Git is the only registry for a chat's worktree

M1.4 gives a chat that writes to a repo its own git worktree, cut and listed by the Rust
core. Five of its choices sit against rules written elsewhere in this repo, and each will be
re-proposed by whoever picks up the next milestone. They are written down here, together,
because most of them share one reason.

**Charter shells out to the git binary and withholds git's whole repository-local
environment by a definition git itself prints; it records nothing of its own about a
worktree; it confines every worktree operation to one workspace rather than to the plane; it
ships worktrees that carry no harness layer; and it keeps M1.4's differential comparing
trees, with the divergences named as ignored paths rather than compared away.**

## Git through the binary, not a library

The alternatives are libgit2 through `git2`, and gitoxide. Neither is chosen.

`git worktree` is not one operation; it is a registry under `.git/worktrees`, a set of
gitlink files pointing both ways, a `prunable` state for a tree whose directory went away,
and a locking protocol between concurrent `add`s. The binary is the definition of that
behaviour. libgit2 reimplements it and diverges in the corners that matter here — which is
the wrong kind of difference to discover from a bug report about a worktree charter created
and git cannot see. Gitoxide has no `worktree add` at all.

Priority 1 is development experience, and `git2` costs a C toolchain on every machine and in
CI, plus a libgit2 version to track. Priority 2 is standard practice, and the standard
practice for driving git is git.

**The objection is parsing.** `git worktree list --porcelain` has to be read, and this repo's
decision 12 says nothing parses output to decide anything. That decision is about *harnesses*
— programs whose output is prose written for a person, whose shape is not promised and
changes when a vendor feels like it. `--porcelain` is the opposite: git documents it as a
machine format and holds it stable across releases precisely so that tools may depend on it.
Python charter has parsed it since the feature existed. Reading a documented machine format
is not inferring a fact from prose.

### The real cost of the binary, which is not parsing

Spawning git means inheriting git's environment, and that surface is larger and sharper than
it looks. It is written here rather than in the design document because it is the price of
this ADR's first decision, and because this repo has already paid it twice.

`git rev-parse --local-env-vars` prints **fifteen** variable names on git 2.50.1. Three of
them — `GIT_CONFIG`, `GIT_CONFIG_PARAMETERS`, `GIT_CONFIG_COUNT` — are not redirection at
all. They inject configuration into a call that already named its repository with `-C`, and
configuration includes `core.hooksPath`. Measured, in a throwaway repo:

```console
$ printf '#!/bin/sh\necho PWNED > /tmp/rv/PWNED\n' > hooks/post-checkout && chmod +x …
$ GIT_CONFIG_COUNT=1 GIT_CONFIG_KEY_0=core.hooksPath GIT_CONFIG_VALUE_0=/tmp/rv/hooks \
    git -C r worktree add -q /tmp/rv/wt -b p1
$ cat /tmp/rv/PWNED
PWNED
```

`git worktree add` runs `post-checkout`; `merge` runs `post-merge`; `publish` runs
`pre-push`. Charter's own binary runs **from a hook**, where an attacker-set environment is
the ordinary case rather than an exotic one. So this is arbitrary code execution as the
operator, reached through an environment charter never looked at.

**The defence is a definition, not a list.** Python charter arrived here the hard way:
`charter/workspace.py:3453` records that naming the variables one by one — review round 3's
`GIT_DIR` and `GIT_WORK_TREE` — missed `GIT_COMMON_DIR` in round 4. Its answer is
`util.GIT_REPOSITORY_ENV` plus the three `GIT_CONFIG*` names, held to reality by a test that
runs `git rev-parse --local-env-vars` and fails when git grows a sixteenth variable. The Rust
core does the same, with the same test, because a hand-written list is a list that is wrong
the next time git is released.

## Nothing records a worktree except git

Python charter's `worktree.py` opens with the rule and the reason: *"Git is the only registry.
Nothing here writes state."* A worktree made by hand with plain git is visible to charter, and
one removed by hand cannot leave charter reporting a tree that is not there. The alternative
is a marker that can disagree with reality, and this project has been bitten by that shape
before.

Python then breaks its own rule in one place: it records `claimed` / `done` / `abandoned` per
piece so that `charter wt history` can show pieces that no longer exist. The Rust core does
not carry that. History is a genuine want, but it is a plane-format write, and a plane-format
write needs its own differential test and its own entry in `docs/plane-format.md`. Riding it
in on a milestone about worktrees is how a second registry gets created by accident.

**The base branch is the hard case.** Merging back targets the branch a piece was cut from
(see the design doc), and something must remember which that was. Inferring it later does not
work: `git merge-base` yields a commit, and a commit that two branches both reach names
neither. So it is recorded — in git, as `branch.<branch>.charterBase` in the clone's own
config, keyed on the branch the worktree is really on rather than on the piece name, which
`--branch` makes a different string.

That key needs no `extensions.worktreeConfig`, which is the reason it is not
`git config --worktree`: measured on git 2.50.1, `--worktree` fails with exit 128 the moment
a repository has a linked worktree unless that extension is enabled, and enabling it mutates
the operator's clone repository-wide to store one string. A `branch.*` key costs nothing, git
ignores keys it does not know, and `git branch -d`/`-D` drops the whole section while
`git branch -m` carries it across — so the record follows its branch with nobody maintaining
it.

Three things about that key are **not** free, and the design document says what is done about
each. A config key can hold more than one value, and `git config --get` then returns the last
one with exit 0 and no warning — so charter reads with `--get-all` and treats more than one
value as a refusal, and writes with `--replace-all`. The *value* is a branch name, whose
lifetime is its own: a base that is renamed or deleted leaves a record naming something that
is no longer there, which `merge` must report rather than act on.

And a worktree with no such key is **not** necessarily one charter did not cut. Python
charter writes no `charterBase` at all, so throughout the cutover — when both implementations
drive one plane, which decisions 14 to 17 arrange for deliberately — every Python-cut worktree
lacks it. The honest reading of an absent key is "the base was not recorded", and the honest
response is to say so and name the repair, not to infer a base and not to claim the worktree
is foreign.

## A worktree operation is confined to one workspace, not to the plane

`contain::writable` asks whether a resolved path lands inside the plane's data directories:
`personas/`, `workspaces/`, `.charter/persona-state`. For everything charter has written so
far that is the right question. For this feature it is not sufficient, and the gap is a
deletion.

`git worktree remove` resolves symlinks in its argument, and git — not charter — does the
removing. Measured:

```console
wsB/victim              ← a live, registered worktree of workspace B
wsA/.worktrees/r/piece  → wsB/victim   (a symlink)

$ git -C r worktree remove …/wsA/.worktrees/r/piece
$ echo $?
0
```

Workspace B's tree is gone and its registration with it. `contain::writable` returns `Ok`
throughout, because the resolved target *is* under `workspaces/`. Every tree-safety check
passed too — they ran against B's tree, which was clean, so "nothing to lose" was true of the
wrong tree.

This is the failure shape M1.1's five review rounds kept finding, one notch sideways: not a
gate one level shallower than the write, but a gate on the name charter built while the write
lands on what the kernel resolves it to. So worktree paths get a stricter boundary than the
plane's data directories — the workspace's own worktree root — and the path handed to git is
resolved, link-free and absolute before git sees it. The design document states, per path,
which check runs.

## A worktree charter cuts carries no harness layer, and says so

This is the accepted gap, and it is a guard hole, so it is stated plainly rather than left to
be discovered.

Python charter writes a *guest layer* into every tree it owns, including every piece:
`workspace.wire_guest` materialises the harness plugin, `$CHARTER_HARNESS`, the persona's
agents and the plane's ask/deny rules, and hides them through a managed block in
`.git/info/exclude`. A session started in a tree without it runs without them. Charter #951 is
the bug report for exactly this, from when pieces were missed.

The Rust core does not port that in M1.4. It is a four-hundred-line subsystem with its own
ownership model — which paths charter may claim, which are co-written, what happens when the
block cannot be written — and folding it into this milestone would put it in the same review
as the containment work, where neither gets read properly.

**Decision 14 is not fully satisfied, and pretending otherwise would be the worse failure.**
It forbids calling Python, which nothing here does, and it also says whatever a feature needs
is ported *before* the feature that needs it. The guest layer is something this feature needs.
The choice made is to ship the feature with the gap named at every point an operator could
meet it, rather than to hold M1.4 behind a subsystem port — and to record that this is a debt
against decision 14 and not a reading of it that makes the debt disappear.

The bounds are three, and each reads the world rather than remembering what charter did, so
each stops firing on its own when the layer is ported:

- **`charter wt add` prints a warning** naming what the new tree does not have. This is the
  path with no UI in it: a person cuts a piece in a terminal, `cd`s in and runs a harness, and
  nothing else in this list would ever be shown to them. It matters more because the verb is
  spelled the same as Python's, and Python's `charter wt add` wires the layer before it prints
  `enter: cd <path> && claude`. Same command, same operator, opposite guarantee, decided by
  which binary is first on `PATH`.
- **A chat's row reads `unwired`** when the worktree has no harness layer. Derived from the
  tree, so a worktree Python cut reads as wired and one that *becomes* wired later stops
  showing the label with no code to delete. The marker is not taken at face value: charter's
  own is per-checkout and untracked, so a **tracked** `.charter-generated` is content some
  cloned repository committed and is not charter's word about anything — `workspace.py:2176`
  already treats it that way, and so does this.
- **A persona cannot be attached to a chat in an unwired worktree.** A persona whose agents
  and vault rules are silently absent is worse than no persona: the operator believes a
  charter is loaded and it is not.

What the bounds do **not** cover is stated too: an ordinary chat with no persona still runs in
a tree with none of the plane's ask/deny rules, and the label is passive. And the repair
available today — `charter reinit`, which writes the layer — is the Python implementation,
which decision 17 has frozen. A milestone the spec calls "daily driver on macOS, on Rust
alone" has one gap whose only closing move is the implementation being retired. That is the
debt, stated at its full size.

## The differential keeps comparing trees, with the divergences named

Decision 15 requires every ported module to pass a differential test: the same fixture plane
and the same input give the same output and the same resulting plane in both implementations.

A first version of this ADR narrowed that, for M1.4, to git's own answer about its own state —
`git worktree list --porcelain`, the branch and the commit — on the reasoning that Python
writes the guest layer and the piece history while Rust writes neither, so a byte-for-byte
tree comparison would fail on a *correct* implementation, and that an exclusion list is a
comparison nobody reads.

**That reasoning was wrong about this repository's own harness, and the narrowing would have
cost the thing M1.4 most needs tested.** `tests/differential/run.py` already compares more
than trees: `_outside`/`_escaped` is a sentinel for writes that land outside the plane
entirely, added because *"a containment bug writes where `rglob` over the plane cannot see
it, so it has to be looked for on purpose"*. Dropping that in the one milestone whose stated
main risk is containment is the wrong trade at any price. The runner also compares the
directory set, which is what would notice the Rust side growing a second registry file — the
thing this ADR exists to forbid.

And the harness already has the mechanism the narrowing was reaching for.
`Scenario.ignore` is *"paths (relative to the plane) neither side is compared on, **with the
reason**"*, and its existing divergence notes carry a live self-invalidating check, so the day
a divergence is ported the harness says "drop the note" rather than quietly agreeing.

So M1.4's differential stays a tree comparison. The two deliberate divergences — the guest
layer and the piece history — become `ignore` entries carrying their reason, `.git/**` becomes
one more with the reason that a git repository does not compare byte for byte against itself
(reflog entries take the real clock, which `--now` does not pin, because charter shells out to
git), and the escape sentinel and the directory set are kept. The scenarios cover `remove`,
`merge` and `publish` as well as `add`, because those are the destructive ones and decision 15
says every ported module, not every ported verb charter found convenient.

## What this rules out

- A git library, unless someone first shows a behaviour the binary cannot give.
- A hand-maintained list of git environment variables, in either implementation.
- A charter-side file listing worktrees, for any reason, including performance.
- Inferring a piece's base branch instead of recording it — and inferring that an absent
  record means the worktree is foreign.
- Gating a worktree path against the plane when the write lands in a workspace.
- A silent unwired worktree: if a warning, the label or the persona refusal is removed, the
  layer is ported first.
- Replacing the differential's tree comparison with a comparison of command output.
