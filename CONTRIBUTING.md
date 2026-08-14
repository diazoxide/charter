# Contributing to charter

Thanks for looking. charter is small, opinionated, and dependency-free on purpose — the
guidance below is mostly about keeping it that way.

## Getting set up

charter needs **Python ≥ 3.11** (stdlib `tomllib`) and has **no runtime dependencies**.

```bash
git clone https://github.com/diazoxide/charter
cd charter
python3 -m unittest discover -s tests        # the whole suite, stdlib unittest
python3 -m unittest tests.test_statusline    # one module
python3 -m charter doctor                    # run the CLI from the checkout
```

CI runs that same command on 3.11, 3.12, 3.13 and 3.14. There is no linter to appease and
no formatter to run — match the surrounding code.

If you use charter itself to work on charter: work in a **workspace clone**, not in the
plane root. A CLI installed with `uv tool install` shadows the checkout, so live-testing a
change means running `python3 -m charter …` from the clone rather than the `charter`
command.

## What a good change looks like

- **Tests first, and they must fail first.** Every behavioural change lands with a test
  that fails without the fix. The suite is plain `unittest` — no fixtures framework, no
  network, no writing outside a tempdir.
- **Comments explain *why*.** The codebase leans hard on this: a comment that restates the
  code earns nothing, one that records the failure a line prevents is worth several
  paragraphs of docs. Read a few modules before writing your first one.
- **No new runtime dependencies.** charter installs into other people's toolchains; a
  dependency there is a cost they did not choose. Build-time and test-time are stdlib too.
- **Cross-platform care.** macOS and Linux diverge in ways that have bitten this repo
  before — path normalisation under `/tmp`, `seq` counting down on BSD, case-insensitive
  filesystems. If your change touches paths or shells out, think about both. See *Your
  machine is not the runner* below, which this keeps turning out to be a case of.
- **Docs move with the code.** A flag that is not in `docs/` does not exist. If you change
  what a command does, update the page that describes it in the same PR.

## Your machine is not the runner

Three times now a change has been green locally and red on `main`, and each time the cause
was the same shape: **the test inherited something from the developer's machine instead of
pinning it.** Not an OS difference in the end — a *configuration* difference the test never
declared it depended on.

The most recent one is the clearest. A fixture built a bare git remote with
`git init --bare`, so that remote's `HEAD` followed whatever `init.defaultBranch` the
machine happened to set. `git fetch` copies that into `refs/remotes/origin/HEAD`, and that
ref is the first thing charter's default-branch resolver trusts. On a machine setting
`main`, the fixture was one repository. On a runner leaving it unset, it was a different
one, whose default branch was `master`. The assertions were correct throughout; they were
describing something that only existed on one machine.

The others: `seq 1 0` counts *down* on BSD and prints nothing on GNU, so a "zero iterations"
loop silently ran twice on macOS. And `/tmp` is a symlink to `/private/var/...` on macOS, so
a path comparison that passes locally fails on Linux unless both sides are `.resolve()`d.

**So: a test pins what it depends on.** Anything that changes behaviour and comes from
outside the test — git config, `$PATH`, locale, the default shell, an env var, the
filesystem's case sensitivity — is either set explicitly by the test or is a bug waiting
for someone else's machine.

For git specifically, pin it on the invocation rather than trusting the environment:

```python
_PINS = ["-c", "init.defaultBranch=main", "-c", "commit.gpgsign=false",
         "-c", "tag.gpgsign=false"]
subprocess.run(["git", *_PINS, "-C", str(cwd), *args], ...)
```

Signing belongs in that list even when the test has nothing to do with signing: a developer
with a signing helper configured gets a fixture commit that stops to ask for a passphrase,
and a suite with nobody at the keyboard hangs rather than fails. That is the same failure
`docs/git-policy.md` exists to prevent in charter itself.

### Reproducing the runner before you push

You can hand git a config the way the runner has it, without touching your own:

```bash
GIT_CONFIG_COUNT=1 GIT_CONFIG_KEY_0=init.defaultBranch GIT_CONFIG_VALUE_0=master \
  python3 -m unittest discover -s tests
```

If a change touches git behaviour, run the suite that way once. It is cheaper than a red
`main`, and it is how the last one was diagnosed.

**Watch the run on `main` after a merge, not only the one on your PR.** PR checks run
against the PR branch; `main` is a different commit. Every one of these three was green on
the PR and red immediately after merging.

## Architecture decisions

Decisions that were hard to reverse are written down in [`docs/adr/`](docs/adr/), and
`CONTEXT.md` holds the project's vocabulary — the words charter uses and the ones it
deliberately avoids. Both are worth skimming before a design change: if your PR
contradicts an ADR, that is fine, but say so in the description and explain what changed.

## Pull requests

1. Branch from `main`.
2. Keep the change focused — one concern per PR.
3. Make sure `python3 -m unittest discover -s tests` passes locally.
4. Describe the failure your change prevents, not just the code you wrote.

Small PRs get read quickly. Large ones are welcome too, but open an issue first so nobody
spends a weekend on something that turns out to be against the grain.

## Reporting bugs

Use the issue templates — they ask for your charter version (`charter version`), your OS,
and `charter doctor` output, which together answer most of the first round of questions.

Something security-sensitive belongs in [SECURITY.md](SECURITY.md) instead, not a public
issue.

## Code of conduct

By participating you agree to the [Code of Conduct](CODE_OF_CONDUCT.md).
