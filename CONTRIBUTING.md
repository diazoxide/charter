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
  filesystems. If your change touches paths or shells out, think about both.
- **Docs move with the code.** A flag that is not in `docs/` does not exist. If you change
  what a command does, update the page that describes it in the same PR.

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
