---
version: unreleased
headline: A release retried by hand can finish a publish, and can no longer quietly become one
---

`release.yml` has two ways to reach an upload PyPI cannot take back: push a version tag, or
run the workflow by hand. The point of #558 was that the second carried weaker checks than
the first. One part of that closed already — a hand-run release has to name the version it
is publishing, and the step that asks can no longer be skipped. This is the part underneath
it, and it decides *what gets uploaded* rather than what gets typed.

## Naming a version is a claim about a string

Every check in `guard` compared one version against another version: the tag, or the
`version` input, against `pyproject.toml`. All of those are numbers, and agreeing about a
number is not agreeing about a commit — but a commit is what PyPI receives and keeps.

On the tag path the two questions are one. `github.ref` **is** `refs/tags/v0.53.0`, so every
job checks out the tree that tag names and nothing else can be built.

On the hand-run path they came apart, because `workflow_dispatch` takes any ref. The
documented recovery includes:

```
gh workflow run release.yml --ref main -f version=0.53.0
```

and that command is genuinely needed. When a release fails *after* the upload — the shape
#665 described, where the notes cannot be turned into a GitHub Release — the fix lives on
`main` and the tagged tree cannot carry it. So the retry rests on one sentence, written into
the workflow when that recovery was built: *the dispatch retry is not there to publish, it is
there to finish a publish that may already have happened.*

Nothing made that sentence true. If PyPI does not already hold the version, such a run is not
finishing anything — it **is** the publish. `skip-existing` skips nothing, because there is
nothing there to skip. And what it publishes is whatever `main` holds at that moment: the
tagged tree plus everything merged since, or a version with no tag cut at all. It passes every
check, because the `pyproject.toml` sitting in that tree agrees with the number you typed.

That last case is the scenario #558 opened with, surviving both of its fixes: between a
version bump merging to `main` and the tag being pushed, `main` carries a version that has
never been published. Requiring the run to state a version made it state one. It did not make
the statement true of anything but a file the run had already chosen.

## So an off-tag run has to prove it cannot be the first upload

`guard` now asks one more question, and only of the runs that need it:

- **Standing on `v<X.Y.Z>`** — a tag push, or `--ref v<X.Y.Z>` — nothing further is asked.
  What the run uploads is the tree that tag names. That is the whole property, and it holds
  with no network: an ordinary release cannot be stopped by pypi.org being unreachable.
- **Standing anywhere else** — the run may only *finish* a release, so `guard` asks PyPI
  whether the version is already there. If it is, the upload is a no-op and the recovery
  proceeds exactly as before. If it is not, the run is refused, and the refusal hands back
  the dispatch that would have been accepted.
- **PyPI does not answer** — refused as well. An answer that did not arrive is not a yes,
  and this is the last step before an act with no way back, where re-running `guard` costs a
  minute and being wrong costs a version number permanently.

```
::error::this run would be the FIRST upload of charter-cp 0.53.0 and it is not standing on
v0.53.0 — what it would upload is whatever branch main holds right now, which no tag names
and PyPI will never let go of — dispatch on the tag instead: gh workflow run release.yml
--ref v0.53.0 -f version=0.53.0 — refusing to publish
```

Nothing about the `--ref main` recovery changes for the case it was written for. What changes
is that the assumption it was resting on is now checked rather than described.

## What the tests hold

`tests/test_workflows.py` pulls the check's own script out of the YAML and runs it against a
synthetic tree, on both triggers, with `curl` stubbed — which is also the only way to exercise
the answers that are hard to arrange for real: a version PyPI has never seen, and PyPI not
answering at all. Runs are sorted into tables by which of the two things they get wrong, and
every refusal is asserted by **the reason it gives** as well as by its exit code.

Two of those assertions are about the absence of a call rather than its result, and both are
properties rather than bookkeeping. A run standing on the tag must reach the network *not at
all* — otherwise a release could be blocked by an outage that has no bearing on whether the
tree is the tagged one. And a run refused for its version must be refused before any lookup,
so the check does not fail differently when PyPI is slow.

There is also an assertion about the tables rather than the script: an allowed run either
stands on the tag for the version it publishes, or has been told by PyPI that the version is
already there. The cheapest way back to green after a change like this is a permissive row,
and that is now itself a failure.

## Still not held here

The `pypi` environment has no protection rule — no required reviewer, no wait timer — so
nothing pauses between the build and the upload on either path, and its deployment-branch
policy is unset, so any ref may deploy to it. Those are repository settings rather than lines
in a file, and no test in this suite can hold them.
