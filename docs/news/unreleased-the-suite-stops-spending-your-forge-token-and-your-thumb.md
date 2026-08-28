---
version: unreleased
headline: charter's own test suite stops spending your forge token and your signing key — 28 authenticated round trips a run become zero, and the git it runs 9,735 times a run reads this repository's config instead of yours
---

These are the last two of the family the last two releases have been closing: the suite
reading or spending the machine it runs on instead of the repository. The plane, the shell,
the filesystem, the clock, the working directory, PyPI, a real keyboard, the operator's
1Password vault and leaked tmux servers all went the same way. These two are a credential
and a private key.

## The forge token: 28 round trips a run, ~2,200 per pull request's gate

`doctor.check_forge_auth` runs `gh auth status --hostname github.com`. That is not a leak —
`auth status` never prints the token — but it is the operator's real authority reaching a
real remote from a unit test, on every run.

**The number in the issue was wrong, and re-measuring is why the fix works.** It said 23,
all `gh`. Counted in-process — `subprocess.Popen.__init__` wrapped before the test package
is imported, because a `ps | grep` of a running suite matches its own command line, which
is how the PyPI issue arrived at a figure that was an artefact — one green run forks **28
forge-auth children from 18 test modules**, identically on CPython 3.12 and 3.14:

| child | per run |
|---|---|
| `glab auth status --hostname gitlab.com` | 20 |
| `gh auth status --hostname github.com` | 8 |

The split is a property of the fixtures, not of the machine: a plane that declares no
`[[forge]]` block falls back to the historical single-GitLab default, and every plane the
shared test base class hands out is one of those. A fix aimed at the `gh` the issue named
would have left twenty of the twenty-eight in place.

**And the multiplier is what made it urgent.** The deletion sweep runs the suite once per
mutation and its gate runs on every pull request, so 28 becomes 28 × the mutation count —
around **2,200 authenticated requests to a forge from one pull request's gate**, on the
operator's own token, for a diff of ordinary size.

### The preflight is answered, not stubbed

`mock.patch("charter.doctor.check_forge_auth")` would have been one line, and it would have
made a function that never runs: its `"Logged in" in blob` branch, its summary-line
extraction, its `stdout + stderr` concatenation and its timeout arm would all have gone dark
for the whole suite, and the next case written against any of them would pass without
running anything. That is the trap the PyPI fix names, and it is why that one refused the
fork rather than stubbing the spawner.

So the answer sits at the child instead. One wrapper on `charter.util.run` recognises
`<forge cli> auth status` — and nothing else — and returns a recorded reply, on stderr,
which is where `gh auth status` really writes it. Every line of `check_forge_auth` still
executes. The backends' own `check_auth` builds the identical argv and is answered by the
same wrapper. A test that wants a different answer patches `charter.util.run` itself, which
replaces the fixture and puts it back afterwards.

The reply names itself, so a doctor row printed from a test run says `Logged in to
github.com account charter-test-fixture (tests/_forgeprobe.py — no forge was contacted)`
rather than looking like a real session belonging to whoever ran the suite.

### The allowance came out on the same commit as the reach

The forge tripwire that found this shipped **allowing** `auth status`, refused `auth login`
and `auth token`, and wrote the residual into its own docstring rather than hiding it —
the right call for a change that had no business stubbing eighteen modules.

It is now refused with everything else. The rule is a one-item allowance rather than a
denial list: an argv that names no subcommand at all (`gh --version`, the local probe that
reports which CLI is installed) is allowed, and anything else — `api`, `pr merge`,
`mr merge`, `repo clone`, `auth status`, and a subcommand charter has never heard of —
fails the test that spawned it, by name, having contacted nothing.

A guard that permits what nothing does any more is a guard that will quietly re-permit it.
The answer is what made the refusal affordable, and the refusal is what stops the answer
from being a stub the next test walks around.

**After: zero.**

## The signing key: a hang, and the one CI can never see

A fixture repository created by a test inherits `commit.gpgsign = true` from the machine's
own `~/.gitconfig`. With 1Password's `op-ssh-sign` as `gpg.ssh.program` — the setup on the
machine this was found on — `git commit` parks on a biometric prompt and the suite never
returns. Not a failure. A **hang**: no pass, no fail, no verdict, and no line saying why.

Measured, in a bare temp repository with charter's own checkout out of the picture:

```
$ git init -q . && echo x > a && git add a && git commit -m probe
error: 1Password: failed to fill whole buffer
fatal: failed to write commit object
git commit -m probe  0.01s user 0.01s system 0% cpu 1:00.36 total
```

Sixty seconds and a failed commit with stdin closed; indefinite with a terminal attached.
**A runner has no signing config**, so CI cannot see this and never will — the same reason
122 blocked `input()` calls passed in CI and hung for a human.

### Thirty-one modules, three spellings, and no shared answer

31 test modules run `git commit`, **1,384 children in one green run**, and each was
protected by hand or not at all, in three different spellings: `-c commit.gpgsign=false` on
the argv (1,068 of them), a repo-local `git config commit.gpgsign false` in the fixture's
`setUp` (208 more, all in one module), and `GIT_CONFIG_GLOBAL=/dev/null` in a hand-built
child environment. A module that remembers none of the three is not refused, not reported
and not slow — it hangs, and the thirty-second module was always going to be the one.

So the fix is not thirty-one patches. `$GIT_CONFIG_GLOBAL` now points at a file this
repository writes and `$GIT_CONFIG_SYSTEM` at `os.devnull`, installed before any test module
is collected and above the import that pulls charter in. Every `git` this suite runs — all
**9,735 of them per run** — reads a config the repository controls: signing off, an identity
at a reserved `.invalid` domain, `init.defaultBranch = main` so a fixture repo's first branch
is the same on every machine, no global hooks path, and none of the machine's filter or diff
drivers.

**Redirected, not merely defaulted**, for the reason the config-directory redirect already
gives: a developer who has these set is exactly the case that hangs, and inheriting theirs
would keep the hole open for them.

### And then the tripwire found the module the redirect could not reach

A default is something a new module can walk past, so a `git` spawned with an environment
that drops the redirect now fails the test that spawned it, by name, before the child
starts. Turning it on across 7,984 tests found exactly one module — ten cases in
`test_frame_owns_the_surface`, which builds its environment with `clear=True` and so handed
`git remote get-url origin` an empty one. Those ten really were reading the operator's own
git config; they now carry the redirect explicitly, and the eleventh case that forgets will
say so instead of hanging.

The suite also gained the thirty-second module on purpose: a fixture repository, a plain
`git commit`, and no neutraliser of its own anywhere. Before the redirect that case took
60.2 seconds and failed on this machine. After it, the module runs in 2.7 seconds and
asserts the commit is unsigned, authored by the suite, and on `main`.

None of this reaches you unless you run charter's own test suite. Nothing to adopt.
