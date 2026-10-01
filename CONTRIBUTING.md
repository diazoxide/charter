# Contributing to charter

Thank you for helping. Contributions of every size are welcome: a typo, a failing test that
shows a bug, a fix, a feature, or an answer to someone's question in
[Discussions](https://github.com/diazoxide/charter/discussions).

Everyone taking part follows the [Code of Conduct](CODE_OF_CONDUCT.md). If you have found a
security vulnerability, do not open an issue or a pull request; follow
[SECURITY.md](SECURITY.md) instead.

## Before you write code

- **A bug fix** needs no discussion first. If there is no issue for the bug yet, the pull
  request can say what was wrong.
- **Anything larger**, such as a new feature, a new setting or a change to what charter does,
  starts as an [issue](https://github.com/diazoxide/charter/issues/new/choose) or an
  [idea in Discussions](https://github.com/diazoxide/charter/discussions/categories/ideas), so
  we can agree on it before you spend time on it.
- **A change to a decision** that is already recorded goes through a new record in
  [`docs/adr/`](docs/adr/). Read the records near what you are changing first: they say why
  things are the way they are.

## Building and testing

[README.md](README.md#develop) lists what to install and every check CI runs, with the commands
to run them locally. [CLAUDE.md](CLAUDE.md) lists the rules that are easy to break, such as
keeping the core free of the UI and changing a recorded answer only on purpose. The
specification is [`docs/spec.md`](docs/spec.md), and [`CONTEXT.md`](CONTEXT.md) defines the
words charter uses.

A good pull request:

- **comes with a test** that fails without the change and passes with it, named for the
  behaviour it checks;
- **passes what CI runs**: formatting, clippy with `-D warnings`, and the Rust and app tests;
- **adds a line to [CHANGELOG.md](CHANGELOG.md)** under `## [Unreleased]` when people using
  charter would notice the change;
- **does one thing**, so it can be reviewed in one sitting.

Coding agents are welcome to help write a contribution. The person who opens the pull request
is responsible for it and signs it off.

## Sign your commits off (DCO)

charter uses the [Developer Certificate of Origin](https://developercertificate.org/) (DCO)
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

charter is released under the [MIT licence](LICENSE). Contributions are accepted under the
same licence as the project: what you contribute is licensed to everyone under the terms you
received it under.

## Getting an answer

[SUPPORT.md](SUPPORT.md) gives the time we aim to take to first respond to a pull request, an
issue or a discussion.
