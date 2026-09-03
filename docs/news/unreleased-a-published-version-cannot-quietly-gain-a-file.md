---
version: unreleased
headline: A release refuses a build whose artefact set a retry could add to a version PyPI already published
---

`release.yml` passes `--skip-existing` to the PyPI upload, on the `workflow_dispatch`
trigger and only there (#673). The sentence beside it read:

> PyPI keeps what it has. That is what makes a version one immutable thing.

That is a claim `--skip-existing` does not make. **The flag does not skip a version.** It
skips the individual files PyPI already holds *by name* and uploads the rest — so a
dispatch against a published version is a no-op only while every file the build can emit
for that version is already up there.

## What was actually holding it up

Nothing in the publishing workflow. `guard` asks
`pypi.org/pypi/charter-cp/<version>/json` and reads the HTTP status: a `200` says the
version exists, and says nothing about which files are under it. (An upload that got the
sdist up and died before the wheel leaves exactly that state — and finishing it is what
the retry is *for*.)

What held was arithmetic in `pyproject.toml`. Hatchling emits

```
charter_cp-<version>.tar.gz
charter_cp-<version>-py3-none-any.whl
```

and nothing else, both determined by the project name and the version alone. So the set a
retry offers is either already published or the missing half of the same two files.

Add a second wheel tag, a platform wheel or an `--python-tag` and that stops being true
**silently**: nothing fails, and a file appears under a version that is already out. Same
shape as #681 — a claim true of every run it was written for, with nothing making it true.

## What happens now

`build` refuses an artefact set that is not exactly the two names the project name and
version determine, and says why the consequence lands two jobs later:

```
::error::this build did not emit the artefact set release.yml's retry path assumes.
Expected exactly the two files the project name and version determine, and got something
else … A dispatch retry uploads every built file PyPI does not already hold BY NAME, so an
artefact set that depends on more than name-and-version means a run against an
already-published version can ADD a file to it …

expected:
charter_cp-0.55.0-py3-none-any.whl
charter_cp-0.55.0.tar.gz

built:
charter_cp-0.55.0-cp312-cp312-manylinux_2_17_x86_64.whl
charter_cp-0.55.0-py3-none-any.whl
charter_cp-0.55.0.tar.gz
```

Both names are derived from `pyproject.toml` — including the PEP 625 normalisation that
makes `charter-cp` into `charter_cp` on disk — so the check has no opinion about the
version, which is the release's own input.

**In `build`, not beside the upload.** `publish` holds `contents: none` and never checks
the repository out, so it has no `pyproject.toml` to derive the names from. `build` is
also the safe end: the refusal costs a red run *before* anything irreversible, where the
same refusal one job later would arrive with half a release shipped.

## What a release refuses that it did not refuse before

Exactly one thing: a build whose `dist/` holds anything other than those two files.
Today's build emits precisely them, so no release that would have succeeded now fails.
The step carries no `if:` — a skipped step is a green step, which is #558 — and it runs
before `upload-artifact`, so the set `publish` receives is the set that was measured.

The claim is asserted from both ends. `tests/test_a_half_finished_release_can_be_finished.py`
executes the step's own script over a fake `dist/` (a third artefact, a missing wheel, a
version that disagrees with the packaged one, a project name needing normalisation), and
reads `pyproject.toml` for the two configuration changes that would end the invariant — a
different build backend, or a wheel target pinned to a tag. That second half is a *prompt*
rather than a proof, the same distinction `.github/publish-closure.json` already draws:
what hatchling emits is settled by hatchling, and nothing offline reads that. It is why
the proof runs over the files that actually came out.

Nothing to adopt.
