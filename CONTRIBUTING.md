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
  network, no writing outside a tempdir. That last one is enforced rather than asked for:
  a checkout inside a control plane resolves to *that* plane, so `tests/_planeguard.py`
  refuses any write into the real `.charter/` — and into the real `charter.toml`, the one
  file outside that directory whose loss changes what your next launch draws (#726) — and
  fails the test that tried. Working in a **linked worktree** the same file pins the suite
  to the checkout it was loaded from, because `root._plane_of` sends a worktree's plane back
  to the clone it was cut from: right for charter, wrong for a suite, whose assertions would
  otherwise read the operator's uncommitted `charter.toml` instead of the branch's committed
  one (#785). A case that wants this repository's own committed config reads the file off
  disk from the repository root, as `test_frame_config._COMMITTED` does, never through
  `config`. The same file
  refuses one kind of *read*: a setting your own `charter.toml` declares — today
  `[update] channel` — because a test that reads it is asserting against a fixture written
  by whoever happens to run the suite. `tests/_envguard.py` is the third of the same
  shape, for the third fixture a test can inherit without noticing: **the shell you
  launched the suite from.** Charter's own variables are removed from the environment
  before charter is imported, so the answer is identical inside a live frame and on a CI
  runner; and a test that then *reads* one of the identity variables a session
  exports — `$CHARTER_SESSION_ID`, `$CHARTER_WORKSPACE`, `$TMUX` and their kin — without
  saying what it holds is refused by name. Derive from `tests._isolation.PersonaIso` and
  none of the three ever comes up; a case that runs against the real plane on purpose calls
  `isolate_state_dir(self)` and `pin_update_channel(self)`; a plain `TestCase` says it is
  outside a frame with `_envguard.unset_all()`, or states the value it needs with
  `mock.patch.dict(os.environ, …)`; a test that spawns a subprocess has to hand it the
  throwaway plane as `$CHARTER_ROOT`, which no guard in this process can do for you.
  `tests/_ttyguard.py` is the fourth, for what that shell *is* rather than what it says: it
  answers whether your streams are a terminal, and **how wide they are**. `$COLUMNS` is
  scrubbed with the rest, and — because removing it only moves the reading to an ioctl on
  stdout — `os.get_terminal_size()` answers what a pipe answers, so a render is the same
  width in your window as it is in CI. A test that wants a size states one with
  `mock.patch("os.get_terminal_size", …)`.
  `tests/_gitguard.py` is the fifth, for the one config file charter never writes and every
  `git` it spawns reads: **your own `~/.gitconfig`.** `$GIT_CONFIG_GLOBAL` points at a file
  this repository writes, so a fixture repo commits as the suite, on `main`, and — the part
  that matters — **unsigned**. Inheriting `commit.gpgsign = true` with a hardware or
  1Password signer behind it does not fail a test, it *hangs* it, and no runner can ever
  see that. A test that builds a child environment by hand carries
  `tests._gitguard.environment()` into it, or is refused by name at the spawn.
- **The suite spends nothing it was not asked to.** `tests/_planeguard.py` also refuses a
  charter child started with `start_new_session=True` — charter's own shape for a
  background refresh that outlives the process that started it. Those fire in tests and
  almost never in the field, because both spawners are throttled by state in
  `config.STATE_DIR` and every test gets a fresh temp one, so `charter _version-check`
  (a PyPI request) and `charter gl-refresh` (the forge client) went off 63 times in one
  green run. If your case renders a status line and does not care, call
  `tests._isolation.no_background_refresh(self)`; if it is *about* the child, call
  `tests._planeguard.allow_background_children(self)`. A test that starts a real tmux
  server names its socket with `tests._tmuxreap.name("<slug>")`, so the next run can reap
  it when this one is killed before its cleanup runs — **every** socket it starts, including
  a second one, and never by decorating a name that helper already produced (#770). The slug
  is lowercase letters, digits and single hyphens; `name()` refuses anything else rather than
  handing back a socket the reaper cannot see. **Nor does it spend a credential.**
  `doctor`'s preflight asks a forge whether your token is still good, and eighteen modules
  reach that line — 28 authenticated round trips to github.com and gitlab.com per run, and
  the sweep gate spends that again per mutation. `tests/_forgeprobe.py` answers the probe
  with a recorded reply, and `tests._planeguard.RealForgeReach` refuses every other `gh` or
  `glab` invocation: a forge test drives `charter.util.run` with a recorded reply instead.
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
- **A user-visible change ships a news entry.** One file, `docs/news/unreleased-<slug>.md`,
  in the shape of [`docs/news/0.44.0-delegate-when.md`](docs/news/0.44.0-delegate-when.md):
  flat frontmatter with a `headline`, plus an optional `check:` (a charter subcommand that
  exits 0 where the thing is already adopted) and `adopt:` (the one that adopts it). It
  says `version: unreleased` because your PR cannot know which release will carry it — the
  next one may be a patch, or your PR may sit through three of them — and the release runs
  `charter news stamp <version>` to move it onto the version that ships it. Entries travel
  inside the wheel, so `charter news` works offline on any harness.

  Two optional ordering fields, both `true`/`false` and both off by default. `security:
  true` says this entry is a security fix: it sorts above the ordinary entries of its
  version and renders as `security: <headline>`, in the Release body and in `charter news`
  alike. Any number of entries may say it. `lead: true` says this entry goes first, and
  only one entry per version may — a second is refused at the release gate rather than
  resolved by a coin toss. Say `security:` if you know what your entry *is*; leave `lead:`
  to the release, which is the only vantage point from which "first" means anything.
  Without either, entries sort by filename as they always have.

  **The frontmatter is flat `key: value`, and it is not YAML — do not quote the value.**
  It opens and closes with `---`, so it looks exactly like YAML frontmatter, and a headline
  usually starts with a backtick, which real YAML would need quoted. charter takes
  everything after the first colon verbatim and unquotes nothing, so `headline: 'a thing'`
  publishes as `### 'a thing'`, quotes and all. 0.56.0 shipped six of those before anything
  checked. A backtick needs no quoting here; a value that must really begin and end with a
  quote has to be reworded, because a flat format has no way to say which pair is yours.
  The suite asks this of every entry in the tree, and `charter news --for` refuses at the
  release gate (#902).

  **Those six keys are the whole set, and they are matched exactly.** `version`,
  `headline`, `check`, `adopt`, `lead`, `security` — anything else in an entry's
  frontmatter is reported at the release gate rather than ignored, `Security:` and
  `securiy:` included. Charter does not guess which one you meant: a key it does not read
  reads as nothing at all, and an entry that renders as nothing is indistinguishable from
  an entry nobody wrote (#503). The same goes for the file itself — every `.md` in
  `docs/news/` is an entry, and one that declares no `version:` charter can read stops the
  release rather than quietly sitting the release out.

  Write it in the PR that builds the thing: you are the only person who knows why it
  matters and what would prove somebody has taken it up, and reconstructing that from
  commit titles at release time is how notes become a changelog nobody reads. Nothing in CI
  blocks a PR that ships none — most PRs are refactors, and a required entry per PR
  manufactures filler. The gate is at the release, where a version with no entry does not
  publish at all.

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

The fourth was charter's own environment, and it ran both ways. Inside a live charter
frame — where anyone working on charter actually runs the suite — sixteen tests failed that
fail nowhere else, reading `$CHARTER_SESSION_ID`, `$TMUX` and `$TMUX_PANE` out of the
developer's terminal. And once, more quietly, both sides of an assertion collapsed to an
ambient `$CHARTER_WORKSPACE` and the test agreed with itself: a mutation that dies with a
clean environment survived under the pin. `tests/_envguard.py` closes both directions, but
it only knows about charter's own names; yours is still yours to pin.

The fifth was the *size* of that terminal, and it is the one worth studying, because the
obvious fix was half a fix. `$COLUMNS` is exported by many shells and `charter/tui.py`
reads it, so it was added to the scrub — after which `term_width()` fell straight through
to `os.get_terminal_size()`, an ioctl on stdout. Measured with both variables unset: three
modules gave three failures and an error on a 40-column pty and passed on a 200-column one.
Nobody had hit it because that ioctl raises when stdout is a pipe, which it is under CI and
under every agent-launched run — so the only person who could see it was the one running
the suite in their own narrow window. Scrubbing a name is not the same as ending a reading,
and a scrub that is a list of spellings will always be one name behind.

**So: a test pins what it depends on.** Anything that changes behaviour and comes from
outside the test — git config, `$PATH`, locale, the default shell, an env var, the
filesystem's case sensitivity — is either set explicitly by the test or is a bug waiting
for someone else's machine.

**For git specifically, that is now done for you** — and the fifth entry above is the
reason it had to be. Thirty-one modules ran `git commit`, 1,384 children in one green run,
each protected by hand in one of three different spellings or not at all; the module that
remembers none of them does not fail, it hangs. `tests/_gitguard.py` points
`$GIT_CONFIG_GLOBAL` at a file this repository writes — `init.defaultBranch = main`,
signing off, no global hooks path, none of your filter drivers — before any test module is
collected, and `tests._planeguard.AmbientGitConfig` refuses a git child that steps outside
it. So a plain `subprocess.run(["git", "commit", …])` in a fixture repo is now correct on
every machine, and the per-invocation `-c commit.gpgsign=false` several modules still carry
is belt and braces rather than the only thing standing between the suite and a biometric
prompt.

Signing was always the one worth the paranoia: a developer with a signing helper configured
gets a fixture commit that stops to ask for a passphrase, and a suite with nobody at the
keyboard hangs rather than fails. That is the same failure `docs/git-policy.md` exists to
prevent in charter itself.

### Reproducing the runner before you push

You can hand git a config the way the runner has it, without touching your own:

```bash
GIT_CONFIG_COUNT=1 GIT_CONFIG_KEY_0=init.defaultBranch GIT_CONFIG_VALUE_0=master \
  python3 -m unittest discover -s tests
```

`GIT_CONFIG_COUNT` is applied as if by `git -c`, so it still wins over the suite's own
`$GIT_CONFIG_GLOBAL` redirect — the recipe means what it says.

If a change touches git behaviour, run the suite that way once. It is cheaper than a red
`main`, and it is how the last one was diagnosed.

**Watch the run on `main` after a merge, not only the one on your PR.** PR checks run
against the PR branch; `main` is a different commit. Every one of these three was green on
the PR and red immediately after merging.

### Sweeping the guards your branch adds

For every `if` you add that refuses, clamps, contains or falls back, there should be a test
that goes RED when that line is deleted. `tools/sweep.py` checks that mechanically, against
the diff with your merge-base:

```bash
python3 tools/sweep.py                  # this branch, against origin/main
python3 tools/sweep.py --second-order 24 # survivors in one function, applied together
python3 tools/sweep.py --all             # the standing debt across the tree, as a number
python3 tools/sweep.py --gate            # exactly what CI runs on your pull request
```

It deletes one guard at a time, runs only the test modules measured to execute that
function, and re-runs the **whole** suite for anything that survives before reporting it.
A survivor is a line you can delete with the suite still green. There is no suppression
list, on purpose: if deleting a line genuinely changes nothing observable, delete the line
— "equivalent mutant" and "dead code" are the same finding.

It is stdlib-only, it makes its own clones, and it never writes to your checkout.

**CI runs it on every pull request** (`.github/workflows/sweep.yml`), scoped to the lines
your branch added, and writes the result onto the run's summary page — the survivors, what
the covering tests assert about each one, and the categories it keeps apart: *unpinned*, a
*masked cluster* (two survivors in one function, which hide each other and have to be read
together), *platform-deferred* (a catch the runner's kernel may never reach), *unresolved*
(the run timed out, so there is no verdict) and *not applied* (a bug in the sweep, not a
finding about your branch).

**It blocks nothing yet, on purpose.** A gate whose numbers nobody has read gets switched
off the first time it is inconvenient, so it reports first. Adding `--enforce` to that
workflow's step is what makes it blocking, and that is a decision to take once the numbers
on real branches have been looked at and believed.

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
