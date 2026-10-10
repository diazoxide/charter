# Contributing to purlis

Thank you for helping. Contributions of every size are welcome: a typo, a failing test that
shows a bug, a fix, a feature, or an answer to someone's question in
[Discussions](https://github.com/purlis/purlis/discussions).

Everyone taking part follows the [Code of Conduct](CODE_OF_CONDUCT.md). If you have found a
security vulnerability, do not open an issue or a pull request; follow
[SECURITY.md](SECURITY.md) instead.

## Before you write code

- **A bug fix** needs no discussion first. If there is no issue for the bug yet, the pull
  request can say what was wrong.
- **Anything larger**, such as a new feature, a new setting or a change to what purlis does,
  starts as an [issue](https://github.com/purlis/purlis/issues/new/choose) or an
  [idea in Discussions](https://github.com/purlis/purlis/discussions/categories/ideas), so
  we can agree on it before you spend time on it.
- **A change to a decision** that is already recorded goes through a new record in
  [`docs/adr/`](docs/adr/). Read the records near what you are changing first: they say why
  things are the way they are.

## Building and testing

[README.md](README.md#develop) lists what to install and every check CI runs, with the commands
to run them locally. [CLAUDE.md](CLAUDE.md) lists the rules that are easy to break, such as
keeping the core free of the UI and changing a recorded answer only on purpose. The
specification is [`docs/spec.md`](docs/spec.md), and [`CONTEXT.md`](CONTEXT.md) defines the
words purlis uses.

A good pull request:

- **comes with a test** that fails without the change and passes with it, named for the
  behaviour it checks. When the pull request meets an issue's acceptance lines, it names the
  test behind each one;
- **passes what CI runs**: formatting, clippy with `-D warnings`, and the Rust and app tests;
- **is looked at, when it changes the window.** A change to what the window draws is done only
  once someone has looked at a screenshot of the real window, in the light and the dark theme,
  and narrow and wide where that matters. Attach it to the pull request, or say what it showed:
  the app's tests draw no pixels;
- **adds a changelog fragment, `changes/<slug>.md`,** when people using purlis would notice
  the change. It holds a `### Added`, `### Changed`, `### Fixed` or `### Security` heading (or
  another of Keep a Changelog's) and the entry under it, written as it will read in
  [CHANGELOG.md](CHANGELOG.md). Don't edit CHANGELOG.md itself: release prep folds the
  fragments into it, so pull requests never conflict there. [`changes/README.md`](changes/README.md)
  has an example;
- **does one thing**, so it can be reviewed in one sitting.

Coding agents are welcome to help write a contribution. The person who opens the pull request
is responsible for it and signs it off.

## How maintainers land work: trains

A pull request from outside the maintainers is reviewed and merged on its own, as above.
Maintainers batch work into **trains**, so CI runs once for many tickets instead of once per
ticket:

- **A ticket is a branch, not a pull request.** Push the ticket's branch
  (`git push -u origin <branch>`) and open no pull request for it. A branch with no pull request
  starts no CI: `ci.yml`, `docs.yml` and `stress.yml` run on `pull_request` and on a push to
  `main` only.
- **One commit per ticket.** Squash the ticket's work into one commit: a title line, a body,
  `Closes #<n>` (or `Refs #<n>` when not every acceptance line is met) and any trailers. Rebase
  it onto `origin/main` before handing it over.
- **Run the checks locally before pushing**, because nothing runs them for a ticket branch:
  `cargo fmt --all --check`, clippy with `-D warnings` on the crates you touched, and the test
  files you touched or added. For app changes, also `npm ci`, then typecheck, lint, format and
  the vitest files you touched. A change to recorded behaviour or the CLI also runs
  `cargo test -p purlis-cli --test recorded_behaviour`. README.md's "Develop" has the commands.
- **A train is one pull request, `train/<date>-<n>`** (for example `train/2026-10-03-1`), that
  stacks the tickets' commits on `main`. CI runs on it once, and it is merged **by rebase**, so
  each ticket stays one commit on `main` and its `Closes #<n>` closes its issue.

## Releases

The maintainer cuts every release and is the only one who pushes a `v*` tag. Before a version
is promoted from the dev channel to stable, the maintainer opens a
[release checklist](https://github.com/purlis/purlis/issues/new?template=release.yml), one
issue per release, and checks every box: the technical gate (CI, the nightly, signing,
provenance and the SBOM), at least 3 days of the maintainer running it as their daily app on
the dev channel, the user signal, the test behind each acceptance line the release meets, the
outcome bars of the features it ships, the folded changelog and its news entry. Each box names
where its evidence is read. The form is
[`.github/ISSUE_TEMPLATE/release.yml`](.github/ISSUE_TEMPLATE/release.yml), and
`node tools/release-form.mjs` fails when it loses a gate.

## Sign your commits off (DCO)

purlis uses the [Developer Certificate of Origin](https://developercertificate.org/) (DCO)
instead of a contributor licence agreement. By signing a commit off you state that you wrote
it, or otherwise have the right to submit it under the project's licence. Every commit in a
pull request from a contributor outside the maintainers carries a line like this at the end of
its message:

```
Signed-off-by: Your Name <you@example.com>
```

`git commit -s` adds it for you, using the name and email in your git configuration. To sign
off commits you have already made on your branch:

```sh
git rebase --signoff main
git push --force-with-lease
```

**Maintainers are exempt.** A pull request opened by a maintainer (someone with write
access to this repository) needs no sign-off, and that includes the commits the
maintainer's coding agents make in it.

## Licence

purlis is released under the [MIT licence](LICENSE). Contributions are accepted under the
same licence as the project: what you contribute is licensed to everyone under the terms you
received it under.

## Getting an answer

[SUPPORT.md](SUPPORT.md) gives the time we aim to take to first respond to a pull request, an
issue or a discussion.
