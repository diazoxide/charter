"""A release that fails after the PyPI upload has to have a way back to `announce`.

`release.yml` runs `guard → test → build → publish → announce`. The upload to PyPI happens
in `publish`; the GitHub Release is created in `announce`; and `announce` is
`needs: publish`. So **everything that can fail after the irreversible step sits behind
it**, and the documented retry —

    gh workflow run release.yml --ref main -f version=<X.Y.Z>

— re-enters `publish`. `pypa/gh-action-pypi-publish` was invoked with no `with:` block at
all, so `skip-existing` was off, so PyPI rejected a version it already held, so `announce`
was unreachable on that version **forever**. Not just for the one failure that motivated
it: #665 was a Release body over GitHub's 125,000-character limit and #672 moved that
particular refusal ahead of the upload, but a network blip, an expired token, a `gh`
regression or a GitHub API outage all land in the same trap. The only exit was the one
`personas/release/persona.md` forbids — writing the Release body by hand, which forks the
published notes from the shipped entry that is supposed to be their single source (#673).

## Why the fix is scoped to the trigger rather than turned on

`skip-existing: true` on its own is the wrong shape, and this module is mostly about that.
It makes "PyPI already has this version" a success **on every path**, including the tag
push — and on the tag push a version PyPI already holds has one realistic cause:

    a tag deleted and re-pushed over a changed tree, with the version not bumped.

Which is what gets reached for when a release went wrong. Unconditionally, that run skips
every file, reaches `announce`, finds the Release already standing, leaves it alone, and
reports **green having shipped nothing** — while `pip install charter-cp==<that version>`
still serves the old code. A loud, correct refusal becomes a quiet lie in precisely the
situation where somebody is already under pressure and reading a green tick.

Scoped to `workflow_dispatch`, both cases come out right. A dispatch run is not there to
publish; it is there to *finish* a publish that may already have happened, and "PyPI
already holds these files" is the expected state on that path. A tag push keeps the strict
behaviour it has always had.

## Why the two cannot be told apart by looking at the artifacts

The tempting alternative is to compare the built distributions against what PyPI already
holds for that version, and refuse only when the bytes differ. It does not work, and not
for a reproducibility reason — hatchling builds this package reproducibly, so an unchanged
tree rebuilds byte-identical files. It fails because **a legitimate retry rebuilds
different bytes by construction**: `docs/news/` ships inside the sdist, so the #665
recovery (fix the entry, re-dispatch) changes it, and `--ref main` picks up whatever else
merged besides. A byte comparison would refuse the exact retry it exists to enable.

PyPI never lets a file be REPLACED either way, so the rebuilt bytes cannot displace the
published ones, and that is the right outcome: a retry's business is the Release, not the
artifacts.

That sentence is about replacement, and for a while it was read as saying a published
version cannot change at all. It does not (#835). `--skip-existing` skips the files PyPI
already holds *by name* and uploads the rest, so what bounds a retry is not PyPI's refusal
but the artefact set being determined by the project name and the version — an arithmetic
that lives in `pyproject.toml`, which the publishing workflow never reads. `build` now
asserts it, and `WhatARetryCanUploadIsBoundedByTheSetTheBuildEmits` below is that half.

So the discriminator is which trigger asked — the only honest one available — and a
trigger nobody has taught this rule gets the strict path. That is asserted below rather
than trusted, in both directions, because a rule that softened everything and a rule that
softened nothing each pass half of these tests.
"""

from __future__ import annotations

import os
import re
import shlex
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path

from tests.test_workflows import Unparsed, _release, _step, needs

#: The action whose `skip-existing` this module is about. Matched by what the step *is*
#: rather than by an `id:`, so the assertions below cannot be satisfied by a second step
#: quietly added beside it.
UPLOAD_ACTION = "pypa/gh-action-pypi-publish"

#: The one expression shape this reader renders: `${{ github.event_name == '<literal>' }}`.
#: Anything else is refused by name rather than guessed at — see `Unparsed`, and see
#: `test_workflows.py`, which takes the same line about YAML it does not model.
_EVENT_IS = re.compile(r"^\$\{\{\s*github\.event_name\s*==\s*'([^']*)'\s*\}\}$")


def upload_step(job: dict) -> dict:
    """The step in `publish` that hands the distributions to PyPI."""
    found = [s for s in job["steps"]
             if isinstance(s, dict) and str(s.get("uses", "")).startswith(
                 f"{UPLOAD_ACTION}@")]
    if len(found) != 1:
        raise AssertionError(
            f"expected exactly one `{UPLOAD_ACTION}` step in `publish`, found "
            f"{len(found)}. That step is the irreversible one; if it moved, move these "
            f"tests with it rather than deleting them.")
    return found[0]


def skip_existing_on(step: dict, event: str) -> bool:
    """Whether `--skip-existing` reaches twine, for a run triggered by *event*.

    Read at the pinned SHA rather than assumed. `action.yml` gives the canonical
    `skip-existing` input **no default** and forwards
    `${{ inputs.skip-existing || inputs.skip_existing }}` to the generated Docker action,
    where the deprecated alias's default `'false'` fills in; `twine-upload.sh` then adds
    the flag with

        if [[ ${INPUT_SKIP_EXISTING,,} != "false" ]]

    So an absent `with:` and an explicit `false` are the same answer — off — and every
    other string is on. That is why the absent case below returns ``False`` instead of
    raising: it is not a gap in this reader, it is what the action does.

    `or {}` and not a default, because `with:` with nothing under it but comments is a
    real shape — it is exactly what deleting the input line leaves behind — and it reads
    as ``None`` rather than as a missing key. Without it that deletion surfaced as an
    `AttributeError`, which is red for the wrong reason and names nothing.
    """
    raw = (step.get("with") or {}).get("skip-existing")
    if raw is None:
        return False
    value = str(raw).strip()
    if value.lower() in ("true", "false"):
        return value.lower() == "true"
    named = _EVENT_IS.match(value)
    if named is None:
        raise Unparsed(0, f"a `skip-existing:` this reader cannot render: {value!r}. It "
                          f"models a constant and `github.event_name == '<trigger>'`, "
                          f"and refuses the rest rather than guessing which triggers a "
                          f"new expression softens the upload for")
    return event == named.group(1)


class OnlyTheRetryTriggerToleratesAVersionPyPIAlreadyHas(unittest.TestCase):
    """The fix, and the half of it that is easy to lose by simplifying."""

    def setUp(self):
        self.release = _release()
        self.jobs = self.release["jobs"]
        self.upload = upload_step(self.jobs["publish"])

    def test_the_release_job_is_still_behind_the_irreversible_one(self):
        """The premise. If `announce` ever stops waiting on `publish` this whole module is
        asserting about a trap that no longer exists — which would be fine, but it would
        be asserting it silently, and the reasoning above would read as current."""
        self.assertIn(
            "publish", needs(self.jobs["announce"]),
            "`announce` no longer waits on `publish`, so the failure this module is about "
            "has changed shape. Re-read it before trusting the assertions below.")

    def test_the_documented_retry_is_not_rejected_for_a_version_pypi_already_has(self):
        """#673 itself. Delete the input and a release that failed in `announce` can never
        be finished by the retry this workflow documents."""
        self.assertTrue(
            skip_existing_on(self.upload, "workflow_dispatch"),
            "a `workflow_dispatch` retry hands PyPI a version it already holds and is "
            "rejected, so `announce` is unreachable on that version forever and the only "
            "exit left is writing the Release body by hand (#673)")

    def test_a_tag_push_is_still_refused_a_version_pypi_already_has(self):
        """The half that makes the input safe rather than merely convenient.

        Turn this on for `push` as well — `skip-existing: true`, the obvious spelling —
        and a tag deleted and re-pushed over a changed tree with the version unbumped runs
        green from end to end: every file skipped, the Release already there and left as
        it stands, nothing shipped, and no red anywhere to say so.
        """
        self.assertFalse(
            skip_existing_on(self.upload, "push"),
            "a tag push would now treat a version PyPI already holds as a success. That "
            "run publishes nothing, leaves the existing Release untouched, and reports "
            "green — which is what a re-pushed tag over an unbumped version looks like")

    def test_a_trigger_added_later_does_not_inherit_the_retrys_licence(self):
        """Completeness, in the shape `guard` already uses for the version check: a third
        entry under `on:` has to be thought about rather than fall through to whichever
        branch was written first. This one falls through to the strict path, which is the
        safe default — but only while the expression stays keyed on the event name."""
        softened = {t for t in self.release["on"] if skip_existing_on(self.upload, t)}
        self.assertEqual(
            softened, {"workflow_dispatch"},
            "exactly one trigger may tolerate a version PyPI already holds: the retry")

    def test_the_upload_is_reached_rather_than_conditioned_away(self):
        """The other shape of the same idea, and it is the one #558 was.

        Making `announce` reachable by putting `if:` on this step — or on the job, which
        `test_workflows.py` already refuses — would mean the retry reports success for an
        upload it never attempted. That is the same colour as an upload that worked, and
        a retry after a `publish` that genuinely *failed* must still upload.
        """
        self.assertNotIn(
            "if", self.upload,
            "a condition on the upload step. A skipped step is a green step, so this is a "
            "way for `publish` to report an upload it never made (#558) — and a retry "
            "after a failed upload needs the step to run. `skip-existing` is idempotence; "
            "an `if:` is a lie about it.")


class TheReaderRendersOnlyWhatItUnderstands(unittest.TestCase):
    """`skip_existing_on` is the whole tooth of the class above, so it gets its own.

    A reader that answered `True` for everything, or `False`, would pass one of the two
    directions asserted above and fail the other; a reader that quietly returned `False`
    for an expression it did not model would pass **both** while pinning nothing.
    """

    def _step(self, value):
        return {"uses": f"{UPLOAD_ACTION}@sha", "with": {"skip-existing": value}}

    def test_an_absent_input_reads_the_way_the_action_reads_it(self):
        """What `release.yml` said before #673: no `with:` block at all, which the action
        resolves to its deprecated alias's default of `'false'`.

        Both shapes of absent, because they are not the same object and the second is the
        one a regression actually produces: delete the input line and `with:` stays
        behind holding its comments, which reads as ``None`` rather than as a missing key.
        """
        for name, step in (("no `with:` at all", {"uses": f"{UPLOAD_ACTION}@sha"}),
                           ("an empty `with:`", {"uses": f"{UPLOAD_ACTION}@sha",
                                                 "with": None}),
                           ("a `with:` holding other inputs",
                            {"uses": f"{UPLOAD_ACTION}@sha", "with": {"verbose": "true"}})):
            for event in ("push", "workflow_dispatch"):
                with self.subTest(shape=name, event=event):
                    self.assertFalse(skip_existing_on(step, event))

    def test_a_second_upload_step_is_refused_rather_than_read_past(self):
        """`upload_step` takes the step by what it is, so "the one this module is about"
        has to stay unambiguous. A `publish` job uploading twice — a TestPyPI dry run
        added beside the real one, say — would otherwise have its first step measured and
        its second left unexamined, and every assertion above would go on passing while
        an upload ran under rules nobody had looked at."""
        with self.assertRaises(AssertionError):
            upload_step({"steps": [
                {"uses": f"{UPLOAD_ACTION}@a", "with": {"skip-existing": "false"}},
                {"uses": f"{UPLOAD_ACTION}@b", "with": {"skip-existing": "true"}}]})

    def test_a_publish_job_with_no_upload_step_at_all_is_refused(self):
        """The other end of the same guard, and the direction that hides more: a reader
        that found nothing and defaulted would report the strict answer for a job whose
        upload it never located — which looks exactly like a correctly scoped one."""
        with self.assertRaises(AssertionError):
            upload_step({"steps": [{"uses": "actions/checkout@sha"},
                                   {"name": "Build", "run": "true"}]})

    def test_a_constant_is_rendered_as_a_constant_on_every_trigger(self):
        for value, expected in (("true", True), ("false", False),
                                ("True", True), ("FALSE", False)):
            for event in ("push", "workflow_dispatch"):
                with self.subTest(value=value, event=event):
                    self.assertIs(skip_existing_on(self._step(value), event), expected)

    def test_an_event_comparison_is_true_for_that_event_and_no_other(self):
        step = self._step("${{ github.event_name == 'workflow_dispatch' }}")
        self.assertTrue(skip_existing_on(step, "workflow_dispatch"))
        for event in ("push", "schedule", "repository_dispatch", ""):
            with self.subTest(event=event):
                self.assertFalse(skip_existing_on(step, event))

    def test_whitespace_inside_the_expression_does_not_change_the_answer(self):
        for spelling in ("${{github.event_name=='workflow_dispatch'}}",
                         "${{   github.event_name  ==  'workflow_dispatch'   }}",
                         "  ${{ github.event_name == 'workflow_dispatch' }}  "):
            with self.subTest(spelling=spelling):
                self.assertTrue(skip_existing_on(self._step(spelling),
                                                 "workflow_dispatch"))

    def test_an_expression_outside_the_subset_stops_the_suite_by_name(self):
        """The alternative is a reader that reports `False` for an expression it cannot
        read, which is indistinguishable from a workflow that turned the input off — and
        would let `skip-existing: ${{ github.event_name != 'push' }}` through as strict."""
        for value in ("${{ github.event_name != 'push' }}",
                      "${{ github.actor == 'diazoxide' }}",
                      "${{ inputs.version }}",
                      "${{ github.event_name == 'push' || true }}",
                      "yes"):
            with self.subTest(value=value):
                with self.assertRaises(Unparsed):
                    skip_existing_on(self._step(value), "push")


def _artefact_check() -> str:
    """`build`'s script for the artefact set, refused if it is not there.

    By `id:` rather than by what the step *is*, unlike `upload_step` above, because there
    is nothing else to match on: it is a `run:` block, and matching it on its own text
    would mean this reader agreeing with the script about the script.
    """
    return _step(_release()["jobs"]["build"], "artefact-set")["run"]


def _run_artefact_check(built, packaged: str = "0.55.0",
                        project: str = "charter-cp") -> tuple[int, str]:
    """Run that step over a `dist/` holding exactly *built*, in a tree whose pyproject
    says *project* / *packaged*.

    A `python` shim for the same reason `_execute` has one — the step parses TOML, which
    is 3.11+, and the runner's default `python` is a version nobody chose. Nothing else is
    stubbed: this step reaches no network and asks nobody anything, which is most of why
    it belongs in `build` rather than beside the upload.
    """
    with tempfile.TemporaryDirectory() as raw:
        tmp = Path(os.path.realpath(raw))
        (tmp / "pyproject.toml").write_text(
            f'[project]\nname = "{project}"\nversion = "{packaged}"\n')
        (tmp / "dist").mkdir()
        for name in built:
            (tmp / "dist" / name).write_bytes(b"")
        (tmp / "bin").mkdir()
        shim = tmp / "bin" / "python"
        shim.write_text(f'#!/bin/sh\nexec {shlex.quote(sys.executable)} "$@"\n')
        shim.chmod(0o755)
        done = subprocess.run(["bash", "-e", "-c", _artefact_check()], cwd=tmp,
                              env={"PATH": f"{tmp / 'bin'}:/usr/bin:/bin"},
                              capture_output=True, text=True)
        return done.returncode, done.stdout + done.stderr


def _emitted(version: str = "0.55.0", project: str = "charter_cp") -> list[str]:
    """The artefact set hatchling emits for this package — the two names the retry path's
    whole safety rests on being all of them."""
    return [f"{project}-{version}-py3-none-any.whl", f"{project}-{version}.tar.gz"]


class WhatARetryCanUploadIsBoundedByTheSetTheBuildEmits(unittest.TestCase):
    """The half of #673 that was true by arithmetic nobody had written down (#835).

    `skip-existing` reads like "tolerate a version PyPI already has". It is not what the
    flag does. **It skips the individual files PyPI already holds BY NAME and uploads the
    rest** — so a dispatch against a published version is a no-op only while every file
    the build can emit for that version is one PyPI already has. Otherwise the run adds a
    file to a version that is already out, and nothing anywhere reports it.

    Two things that look like they close that gap and do not:

    * **`guard` does not.** It asks `pypi.org/pypi/<project>/<version>/json` and reads the
      HTTP status. A `200` says the version exists; it says nothing about which files are
      under it. An upload that got the sdist up and died before the wheel leaves exactly
      that state, and finishing it is what the retry is *for*.
    * **The action does not.** `--skip-existing` is per file, by construction: twine
      uploads one distribution at a time and this flag is how a `400`/`409` on one of them
      stops being fatal.

    What actually holds is that hatchling emits `<name>-<version>.tar.gz` and
    `<name>-<version>-py3-none-any.whl` and nothing else, both fully determined by the
    project name and the version — so the set a retry offers is either already up there or
    is the missing half of the same two files. That is a fact about `pyproject.toml`, in a
    file the publishing workflow does not read, and it would stop being true silently:
    add a second wheel tag, a platform wheel or an `--python-tag` and a published version
    quietly gains a file on the next dispatch.

    So it is asserted, in `build`, where the files exist and before any of them is
    uploaded — the same move #681 made one file along, where the claim was "the tree a
    dispatch publishes agrees with the version beside it" and the answer was to ask rather
    than to write the assumption down.

    **In `build` and not in `publish`, and that is not a preference.** `publish` holds
    `contents: none` and never checks the repository out — it downloads the artifact and
    uploads it — so it has no `pyproject.toml` to derive the two names from. `build` is
    also the safe end: a refusal there costs a red run before anything irreversible, where
    the same refusal beside the upload would arrive with half a release already shipped.
    """

    def setUp(self):
        self.release = _release()
        self.build = self.release["jobs"]["build"]

    # ------------------------------------------------------------------ the premise

    def test_a_trigger_still_tolerates_a_version_pypi_already_holds(self):
        """If `skip-existing` ever comes off every trigger, PyPI refuses an upload into a
        published version on every path and this class guards a door already locked. That
        would be fine; it would also leave the reasoning above reading as current."""
        upload = upload_step(self.release["jobs"]["publish"])
        self.assertTrue(
            any(skip_existing_on(upload, t) for t in self.release["on"]),
            "no trigger passes `skip-existing` any more, so nothing can add a file to a "
            "published version. This class's subject has changed shape — re-read the "
            "module rather than deleting the assertions.")

    # ------------------------------------------------------------------ the shape

    def test_the_bound_is_measured_in_the_job_whose_output_publish_uploads(self):
        """`publish` uploads the artifact `build` produced, so a check anywhere else is a
        check of something other than the bytes PyPI is offered."""
        self.assertIn("build", needs(self.release["jobs"]["publish"]),
                      "`publish` no longer takes its distributions from `build`, so this "
                      "step is measuring a set nobody uploads")
        _artefact_check()      # raises by name if the step is gone from `build`

    def test_the_bound_is_taken_before_the_artifact_publish_downloads_is_uploaded(self):
        """Order, not merely presence. `upload-artifact` is what leaves `build`; a check
        after it would pass or fail over a set that had already been handed on."""
        steps = self.build["steps"]
        checked = [i for i, s in enumerate(steps) if s.get("id") == "artefact-set"]
        handed = [i for i, s in enumerate(steps)
                  if str(s.get("uses", "")).startswith("actions/upload-artifact@")]
        self.assertEqual(len(checked), 1, "one step bounds the artefact set")
        self.assertEqual(len(handed), 1, "one step hands it to `publish`")
        self.assertLess(checked[0], handed[0],
                        "the artefact set is handed to `publish` before anything has "
                        "asked what is in it")

    def test_the_bound_cannot_be_skipped(self):
        """#558's rule, applied to the step that carries #835's claim: a conditional step
        reports success without running, which is indistinguishable here from a build
        whose artefact set was checked and found correct."""
        step = _step(self.build, "artefact-set")
        self.assertNotIn(
            "if", step,
            "a condition on the step that bounds what a retry can upload. A skipped step "
            "is a green step, so this is a way for the bound to report success unrun")

    # ------------------------------------------------------------------ the behaviour

    def test_the_two_files_name_and_version_determine_are_accepted(self):
        """The direction that has to keep working: today's build must still release. A
        step that refused everything would pass every other case in this class."""
        code, said = _run_artefact_check(_emitted())
        self.assertEqual(code, 0, said)

    def test_the_order_the_files_are_found_in_does_not_decide_the_answer(self):
        """`ls` orders by locale and the check compares a sorted listing, so this is the
        difference between asserting about a SET and asserting about a directory read."""
        code, said = _run_artefact_check(list(reversed(_emitted())))
        self.assertEqual(code, 0, said)

    def test_a_third_artefact_is_refused_and_named(self):
        """#835 itself: the platform wheel that would otherwise be added to a version
        already on PyPI by the next `workflow_dispatch`, with nothing failing."""
        extra = "charter_cp-0.55.0-cp312-cp312-manylinux_2_17_x86_64.whl"
        code, said = _run_artefact_check(_emitted() + [extra])
        self.assertNotEqual(code, 0, f"a third artefact was accepted:\n{said}")
        self.assertIn(extra, said, "the refusal does not name the file it is about")

    def test_the_refusal_says_why_an_extra_file_matters_on_this_path(self):
        """An exit code says a build changed; it does not say that the consequence lands
        two jobs later, on a trigger, against a version already published. Whoever added
        the wheel is the person who needs that sentence."""
        code, said = _run_artefact_check(_emitted() + ["charter_cp-0.55.0-py2-none-any.whl"])
        self.assertNotEqual(code, 0, said)
        self.assertIn("ADD a file to it", said)
        self.assertIn("skip-existing", said)

    def test_an_artefact_that_did_not_get_built_is_refused_too(self):
        """The other direction, and it is not symmetry for its own sake. A build that
        emitted only the sdist would satisfy "nothing unexpected is here" while shipping a
        release with no wheel — and would leave the wheel as a file a later dispatch could
        still add."""
        for missing, kept in (("the wheel", [_emitted()[1]]),
                              ("the sdist", [_emitted()[0]]),
                              ("both", [])):
            with self.subTest(missing=missing):
                code, said = _run_artefact_check(kept)
                self.assertNotEqual(code, 0, f"a build missing {missing} was accepted:\n"
                                             f"{said}")

    def test_a_hidden_file_in_dist_is_refused_rather_than_reasoned_about(self):
        """Stricter than the upload, on purpose, and measured so it stays a decision.

        The action globs `dist/*`, which does not match a dotfile, so a hidden file would
        never reach PyPI — and this refuses it anyway. A dotfile in `dist/` means the
        build changed (`uv build` writes a `.gitignore` into its output directory; the
        `python -m build` this job runs does not), and the alternative is a release gate
        whose correctness depends on whose glob rules decide. It costs a red run before
        anything is uploaded.
        """
        code, said = _run_artefact_check(_emitted() + [".gitignore"])
        self.assertNotEqual(code, 0, f"a dotfile in dist/ was accepted:\n{said}")

    def test_a_dist_that_is_not_there_at_all_is_a_refusal_and_not_a_pass(self):
        """`ls` fails and the pipeline's status is `sort`'s, so `set -e` does not fire and
        `built` comes out empty — which has to read as "not the expected set" rather than
        as nothing to check. Fails closed, and this is the case that says so."""
        with tempfile.TemporaryDirectory() as raw:
            tmp = Path(os.path.realpath(raw))
            (tmp / "pyproject.toml").write_text(
                '[project]\nname = "charter-cp"\nversion = "0.55.0"\n')
            (tmp / "bin").mkdir()
            shim = tmp / "bin" / "python"
            shim.write_text(f'#!/bin/sh\nexec {shlex.quote(sys.executable)} "$@"\n')
            shim.chmod(0o755)
            done = subprocess.run(["bash", "-e", "-c", _artefact_check()], cwd=tmp,
                                  env={"PATH": f"{tmp / 'bin'}:/usr/bin:/bin"},
                                  capture_output=True, text=True)
        self.assertNotEqual(done.returncode, 0,
                            f"no dist/ at all was accepted:\n{done.stdout}{done.stderr}")

    def test_the_names_are_read_from_pyproject_rather_than_written_down(self):
        """A step with the current version baked into it would pass every case above and
        refuse the next release. The version is the release's own input, so it is the one
        thing this check may not have an opinion about."""
        code, said = _run_artefact_check(_emitted(version="9.9.9"), packaged="9.9.9")
        self.assertEqual(code, 0, said)
        code, said = _run_artefact_check(_emitted(version="0.55.0"), packaged="9.9.9")
        self.assertNotEqual(code, 0, f"a dist built for another version was accepted:\n"
                                     f"{said}")

    def test_the_project_name_is_normalised_the_way_a_built_filename_is(self):
        """`charter-cp` is `charter_cp` on disk: PEP 625 and PEP 427 both name the file
        from the NORMALISED project name. A check spelling `p["name"]` straight through
        would refuse every real build of this package, and one spelling the underscore by
        hand would be a second answer to a question `pyproject.toml` already answers."""
        code, said = _run_artefact_check(_emitted(project="charter_cp"),
                                         project="charter-cp")
        self.assertEqual(code, 0, said)
        for spelling in ("Charter.CP", "charter__cp"):
            with self.subTest(name=spelling):
                code, said = _run_artefact_check(_emitted(project="charter_cp"),
                                                 project=spelling)
                self.assertEqual(code, 0, f"{spelling!r} normalises to charter_cp and the "
                                          f"check did not read it that way:\n{said}")

    def test_the_set_it_expects_is_the_set_this_repository_asks_for(self):
        """The only assertion here that reaches past the workflow into `pyproject.toml`,
        and it is a PROMPT rather than a proof — the same distinction `.github/publish-
        closure.json` draws about the pins it records.

        The proof runs in `build`, over the files that actually came out. This reads the
        configuration that produces them, so the two changes #835 names in the abstract —
        a backend whose filenames follow other rules, a wheel target pinned to a tag that
        is not `py3-none-any` — arrive in a pull request rather than waiting for a release
        to go red. It cannot be complete, because "what hatchling emits" is settled by
        hatchling, and nothing offline reads that. Which is why it is not the thing being
        relied on.
        """
        import tomllib
        root = Path(__file__).resolve().parent.parent
        with (root / "pyproject.toml").open("rb") as fh:
            pkg = tomllib.load(fh)
        self.assertEqual(
            pkg["build-system"]["build-backend"], "hatchling.build",
            "the backend changed, and a backend names its artefacts by its own rules — so "
            "`build`'s artefact-set step is now encoding somebody else's convention (#835)")
        wheel = ((pkg.get("tool", {}).get("hatch", {}).get("build", {})
                  .get("targets", {}).get("wheel", {})) or {})
        self.assertNotIn(
            "tag", wheel,
            "the wheel target pins a `tag`, so the built filename no longer follows from "
            "the project name and the version alone. That is exactly the change that lets "
            "a `workflow_dispatch` retry add a file to a version PyPI already holds — "
            "decide what a retry should mean before widening `build`'s artefact-set step "
            "to accept it (#835).")
        code, said = _run_artefact_check(_emitted(version=pkg["project"]["version"]),
                                         packaged=pkg["project"]["version"],
                                         project=pkg["project"]["name"])
        self.assertEqual(code, 0, f"the step refuses the set this repository's own name "
                                  f"and version determine:\n{said}")


def _announce_script(job: dict | None = None) -> str:
    job = _release()["jobs"]["announce"] if job is None else job
    steps = [s for s in job["steps"] if isinstance(s, dict) and "run" in s]
    if len(steps) != 1:
        raise AssertionError(
            f"`announce` has {len(steps)} script steps, not one — the executor below "
            f"runs the whole job's script and would be running only part of it")
    return steps[0]["run"]


def _run_announce(release_exists: bool) -> tuple[int, list[str], str]:
    """Run `announce`'s script with `python` and `gh` stubbed, and report what `gh` was
    asked to do.

    *release_exists* is the answer `gh release view` gives, which is the one input that
    decides whether this job is being run for the first time or the second.
    """
    with tempfile.TemporaryDirectory() as raw:
        tmp = Path(os.path.realpath(raw))
        (tmp / "pyproject.toml").write_text(
            '[project]\nname = "charter-cp"\nversion = "0.54.0"\n')
        (tmp / "bin").mkdir()
        log = tmp / "gh.log"
        (tmp / "bin" / "python").write_text(
            "#!/bin/sh\n"
            'case "$1" in\n'
            "  -c) echo 0.54.0 ;;\n"                      # the pyproject read
            '  -m) echo "NOTES BODY" ;;\n'                # charter news --for
            "esac\n")
        (tmp / "bin" / "gh").write_text(
            "#!/bin/sh\n"
            f'echo "$*" >> {log}\n'
            f'if [ "$1 $2" = "release view" ]; then exit '
            f'{0 if release_exists else 1}; fi\n'
            "exit 0\n")
        for name in ("python", "gh"):
            (tmp / "bin" / name).chmod(0o755)
        env = {"PATH": f"{tmp / 'bin'}:/usr/bin:/bin", "RUNNER_TEMP": str(tmp),
               "GH_TOKEN": "stub"}
        done = subprocess.run(["bash", "-e", "-c", _announce_script()],
                              cwd=tmp, env=env, capture_output=True, text=True)
        asked = log.read_text().splitlines() if log.exists() else []
        return done.returncode, asked, done.stdout + done.stderr


class TheReleaseJobIsSafeToReachTwice(unittest.TestCase):
    """The other half of the fix, and the half that was only ever a comment.

    Making `publish` idempotent is worth nothing unless the job behind it is idempotent
    too — the retry's whole purpose is to arrive at `announce` a second time. `announce`
    already leaves an existing Release exactly as it stands, and said so in a comment,
    and nothing anywhere asserted it. Delete the early exit and the retry stops being a
    repair and becomes a second attempt at a Release that is already published: `gh
    release create` refuses a tag that already has one, so the retry fails *for having
    worked the first time*, and the trap closes again one job further along.

    Everything else in the job is a read: `actions/checkout`, `actions/setup-python`, the
    `pyproject.toml` parse, and `charter news --for`, which is a pure function of the
    tree. The Release is the only thing this job creates, so it is the only thing that
    needs to survive being reached twice.
    """

    def test_a_release_that_already_exists_is_left_exactly_as_it_stands(self):
        code, asked, said = _run_announce(release_exists=True)
        self.assertEqual(code, 0, f"the retry failed for having worked the first "
                                  f"time:\n{said}")
        self.assertFalse(
            [c for c in asked if c.startswith("release create")],
            f"`announce` tried to create a Release that already exists, so the retry "
            f"fails at the last job instead of finishing the release: {asked}")
        self.assertIn("already exists", said,
                      "the run says nothing about why it created nothing, which reads as "
                      "a job that did no work rather than one that found none to do")

    def test_and_a_version_with_no_release_yet_still_gets_one(self):
        """The other direction, and not decoration: a job that created nothing under any
        condition would pass the case above and never publish a Release again."""
        code, asked, said = _run_announce(release_exists=False)
        self.assertEqual(code, 0, said)
        self.assertTrue([c for c in asked if c.startswith("release create")],
                        f"no Release was created for a version that has none: {asked}")

    def test_a_job_that_grew_a_second_script_step_is_refused_rather_than_half_run(self):
        """The executor runs one `run:` block and calls it the job. Split `announce` in
        two — render in one step, create in the next — and running only the first would
        report that no Release was created, which is this class's own pass condition for
        the retry case. So the reader refuses rather than measuring half a job."""
        with self.assertRaises(AssertionError):
            _announce_script({"steps": [{"run": "render"}, {"run": "create"}]})

    def test_the_existence_check_asks_about_the_tag_this_run_publishes(self):
        """A check against a fixed or empty tag would report "already exists" for every
        version, or for none, and both directions above would still pass."""
        _, asked, _ = _run_announce(release_exists=True)
        self.assertIn("release view v0.54.0", asked,
                      f"`announce` did not ask about the tag it is publishing: {asked}")


if __name__ == "__main__":
    unittest.main()
