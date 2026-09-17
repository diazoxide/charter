"""**The runner's tmux is a thing this suite depends on, so this suite states it.**

`ubuntu-latest` ships tmux 3.4, and tmux 3.4 can lose a SIGCHLD. When it does,
`server_destroy_pane` still runs — it gets the pty EOF — and closes `wp->fd`, which is the
entirety of `#{pane_dead}`. It then RETURNS at `server-fn.c:329`, on
``remain_on_exit != 0 && (~wp->flags & PANE_STATUSREADY)``, **before**
``notify_pane("pane-died")``. `PANE_STATUSREADY` is set only by `server_child_exited`, and
that is the function the lost signal never reaches. The pane is left reading
``#{pane_dead}`` `1` with ``#{pane_dead_status}`` and ``#{pane_dead_signal}`` BOTH EMPTY,
and the `pane-died` hook has not fired and never will.

Charter drives its entire ended-tab presentation off that hook, so on tmux 3.4 a share of
runs simply never see the event the tests wait for. Measured, with charter stripped out
altogether — one server, ``remain-on-exit on``, a ``sleep 300`` pane, one `pane-died` hook,
SIGKILL on the pane's pid:

* tmux 3.4 — **8 misses in 60**, and 5 in 40 on the same image and the same libevent;
* tmux 3.5 — **0 in 40**, same image, same libevent, same script.

And charter's own gate case,
`test_an_ended_harness_keeps_its_tab_on_a_real_server.AnExitOnARealServer
.test_a_signal_death_ends_the_tab`: tmux 3.4 — **5 misses in 30**; tmux 3.5 — **0 in 30**.

**That is why this module exists rather than a longer timeout.** The bound is not the
variable. On 3.4 the event is never emitted, so patience cannot reach it, and #1116 — which
read one such failure as runner contention on the strength of a diagnostic that printed
``dead=1`` and neither of the two empty fields beside it — raised a wall-clock bound that
reduced the rate without touching the cause.

**Two halves, and they are different kinds of check.**

`TheRunnerRunsATmuxThatCanReportADeath` is the loud one. It runs on the runner itself and
asks what tmux is actually there. Without it, a downgrade — the action edited, a base image
changed under us, a `$PATH` nobody meant — comes back as flakes in the real-tmux modules,
which is the most expensive shape of red there is: it teaches people to re-run.

`EveryJobThatRunsTheSuiteInstallsThatTmux` is the static one, and it is not redundant with
it. The loud check can only speak for the job it runs in; a NEW job that runs the suite
without the pin would never reach it. So the workflows are read here too, and the set of
suite-running jobs is asserted whole.

**And `NoJobsTestInvocationGoesUnclassified` is what makes that true rather than nearly
true.** "A new job would be caught" holds only for the spellings the reader knows, and the
first version of this module knew three. `python -m unittest tests.test_x`, `pytest tests/`
and `make test` all answered "runs no tests", which put such a job outside the set the
per-job assertion quantifies over AND outside the pinned list — both halves green, no
coverage, running on tmux 3.4. So :func:`classify` has a third answer: a run block that is
about the tests and matches no runner this module knows is `UNKNOWN`, and `UNKNOWN` FAILS.
The default for a shape nobody anticipated is "this must be pinned", not silence.

**This is a CI floor and deliberately not charter's own.** `charter.frame.tmuxctl.FLOOR` is
what charter refuses an operator below, and raising it to 3.5 would turn a CI defect into a
refusal aimed at people running 3.4 perfectly happily. What tmux 3.4 costs an operator is a
separate question from what it costs this suite, and only the second one is settled here.
"""

from __future__ import annotations

import os
import re
import unittest
from pathlib import Path

from charter.frame import tmuxctl

from tests.test_workflows import GITHUB, REPO, load

#: The oldest tmux CI may run the suite on.
#:
#: 3.5 because that is the version the miss rate was measured at zero on, not because of
#: where the fix landed in tmux's history — a floor this suite can point at a measurement
#: for is worth more than one it has to point at a changelog for.
#:
#: **Written here, and every other copy is pinned to it.** `.github/actions/tmux/
#: action.yml` states the same two numbers in `FLOOR_MAJOR`/`FLOOR_MINOR` so that the job
#: fails before the suite even starts, and `TheActionAndThisModuleAgreeAboutTheFloor` holds
#: those two together. `charter.frame.tmuxctl.ENDED_TAB_FLOOR` is the third — the same
#: measurement read as *what the ended tab needs* rather than *what CI must run* — and
#: `TheCIFloorIsTheEndedTabFloor` holds that one. One measurement written in several files
#: and drifting is this repository's #670; none of the three is free to move alone.
CI_TMUX_FLOOR = (3, 5)

#: The local composite action that installs it, as a `uses:` reference.
TMUX_ACTION = "./.github/actions/tmux"

ACTION_FILE = REPO / ".github" / "actions" / "tmux" / "action.yml"

#: Every workflow CI can run. Read off disk rather than listed, so a fourth one added
#: beside them is read too — `EveryJobThatRunsTheSuiteInstallsThatTmux` is only as complete
#: as this glob.
WORKFLOWS = sorted((GITHUB / "workflows").glob("*.y*ml"))


#: What a `run:` body does about charter's tests.
RUNS = "runs"          #: it executes them, so this job must have the pinned tmux
NOTHING = "nothing"    #: recognised, and it runs none
UNKNOWN = "unknown"    #: test-shaped and unrecognised — a RED TEST, never a silent pass

#: Every spelling this repository could plausibly run its tests by.
#:
#: **Deliberately wider than the three spellings CI uses today, and the first version of
#: this reader is why.** It recognised `unittest discover` and two `sweep.py` flags, and
#: nothing else — so `python -m unittest tests.test_x`, `pytest tests/` and `make test` all
#: answered "does not run the suite". A job written any of those ways was therefore absent
#: from the pinned list AND from the per-job assertion, and would have run on tmux 3.4 with
#: both halves of this module still green.
#:
#: That is the failure mode this module exists to prevent, arriving through the module
#: itself. A guard that is silent about what it cannot see is worse than no guard, because
#: the green tick is read as coverage.
_RUNNERS = re.compile(
    r"\bunittest\b|\bpytest\b|\bpy\.test\b|\bnosetests?\b|\bnose2\b|\btox\b"
    r"|\bmake\s+(?:test|check)\b|\brun[-_]?tests?\b", re.IGNORECASE)

#: Anything that so much as LOOKS like it touches the tests.
#:
#: **This is the fail-closed half.** A script that matches this and is NOT recognised as a
#: runner above is :data:`UNKNOWN`, and `NoJobsTestInvocationGoesUnclassified` fails on it
#: by name. So the next test runner somebody reaches for — `nox`, a shell script, a
#: `tests/` driver invoked directly — is a red test that says what to do, rather than a job
#: quietly running the real-tmux modules on whatever tmux the image ships.
#:
#: Every alternative in :data:`_RUNNERS` appears here too, which
#: `test_nothing_can_be_a_runner_without_being_test_shaped` holds: were a runner missing
#: from this pattern, dropping it from `_RUNNERS` later would make it `NOTHING` — silence
#: again, by exactly the route this is built to close.
_TEST_SHAPED = re.compile(
    r"\btests/|\btests\.test_|\btest_\w+\.py\b|\bsweep\.py\b|\bcoverage\b"
    r"|\bunittest\b|\bpytest\b|\bpy\.test\b|\bnosetests?\b|\bnose2\b|\btox\b"
    r"|\bmake\s+(?:test|check)\b|\brun[-_]?tests?\b", re.IGNORECASE)


def classify(script: str) -> str:
    """:data:`RUNS`, :data:`NOTHING` or :data:`UNKNOWN` for one `run:` body.

    The `sweep.py` clause is stated as *"every mode but `--verdict`"* rather than as a list
    of the modes that do run tests, and that direction is the point: `--plan` traces the
    whole suite for the selection map and `--gate` runs the unmutated baseline before it
    deals a mutation, but a sweep mode added next year is pinned by default instead of by
    somebody remembering this line. `--verdict` is the one mode that starts no interpreter
    — it downloads artifacts other jobs uploaded and adds them up.
    """
    if _RUNNERS.search(script):
        return RUNS
    if "sweep.py" in script:
        return NOTHING if "--verdict" in script else RUNS
    if _TEST_SHAPED.search(script):
        return UNKNOWN
    return NOTHING


def _verdict(job: object) -> str:
    """One answer for a whole job, from its steps.

    :data:`RUNS` outranks :data:`UNKNOWN`: a job already required to carry the pinned tmux
    gains nothing from also being reported as unclassifiable, and the requirement is the
    stronger of the two outcomes.
    """
    seen = {classify(step["run"])
            for step in _steps(job) if isinstance(step, dict) and "run" in step}
    return RUNS if RUNS in seen else UNKNOWN if UNKNOWN in seen else NOTHING


def _jobs_where(verdict: str) -> list[tuple[str, str]]:
    return [(path.name, name)
            for path in WORKFLOWS
            for name, job in _jobs(path).items()
            if _verdict(job) == verdict]


def _jobs(workflow: Path) -> dict:
    loaded = load(workflow.read_text(encoding="utf-8"))
    return loaded.get("jobs", {}) if isinstance(loaded, dict) else {}


def _steps(job: object) -> list:
    return job.get("steps", []) if isinstance(job, dict) else []


def suite_jobs() -> list[tuple[str, str]]:
    """``(workflow file name, job name)`` for every job that runs the suite."""
    return _jobs_where(RUNS)


def unclassified_jobs() -> list[tuple[str, str]]:
    """``(workflow, job)`` for every job this reader cannot answer for."""
    return _jobs_where(UNKNOWN)


class TheReaderKnowsWhichStepsRunTests(unittest.TestCase):
    """The control. Every assertion below is about a set this reader produces, and a
    reader that had gone blind would hand back an empty set that satisfies all of them —
    the same shape of defect `TheWorkflowQuotesNoCostTheToolDoesNotState` guards next
    door. So the classifier is exercised on real strings, in all three directions."""

    def test_the_spellings_ci_uses_today(self):
        for script in ("python -m unittest discover -s tests -v",
                       "python3 tools/sweep.py --plan --warm-map",
                       "python3 tools/sweep.py --gate --jobs 4"):
            with self.subTest(script=script):
                self.assertEqual(classify(script), RUNS)

    def test_the_spellings_the_first_version_of_this_reader_missed(self):
        """**The regression this class exists for.** Each of these answered "runs no
        tests" when this module was first written, so a job spelled any of these ways was
        invisible to BOTH halves of the guard and would have run on tmux 3.4 green."""
        for script in ("python -m unittest tests.test_workflows",
                       "python -m unittest tests.test_frame_tmux_integration -v",
                       "pytest tests/",
                       "pytest -q tests/test_frame_launcher.py",
                       "make test",
                       "make check",
                       "tox -e py312",
                       "./run-tests.sh"):
            with self.subTest(script=script):
                self.assertEqual(classify(script), RUNS,
                                 "a spelling that runs charter's tests must demand the "
                                 "pinned tmux, not be waved through as unrecognised")

    def test_an_invocation_this_reader_cannot_name_is_loud_rather_than_silent(self):
        """The whole of the fail-closed property. None of these is a runner this module
        knows, and every one of them is plainly about the tests — so the answer is
        UNKNOWN, which fails, rather than NOTHING, which would be a job running the
        real-tmux modules on an unpinned tmux with this file still green."""
        for script in ("python tests/harness_driver.py --all",
                       "bash tests/smoke.sh",
                       "python -c 'import tests.test_workflows'",
                       "nox -s tests/",
                       "coverage run -m mystery_runner",
                       "python tests/test_frame_launcher.py"):
            with self.subTest(script=script):
                self.assertEqual(classify(script), UNKNOWN)

    def test_what_genuinely_runs_no_tests_is_still_quiet(self):
        """Fail-closed must not mean fail-always: a reader that called everything UNKNOWN
        would make this module a permanent red and be turned off within a week.
        `collect` downloads artifacts and adds them up — a tmux build there is time spent
        on a job with no pane in it."""
        for script in ("python3 tools/sweep.py --verdict shards",
                       "charter --version",
                       "uv tool install --force git+https://x@main",
                       "pipx run build",
                       "printf 'deletion sweep: %s\\n' \"$HEADLINE\""):
            with self.subTest(script=script):
                self.assertEqual(classify(script), NOTHING)

    def test_nothing_can_be_a_runner_without_being_test_shaped(self):
        """`_RUNNERS` must be a subset of `_TEST_SHAPED`. Were a runner missing from the
        wider pattern, deleting it from `_RUNNERS` later would make it NOTHING rather than
        UNKNOWN — silence again, by the exact route this module closes."""
        for script in ("unittest", "pytest", "py.test", "nosetests", "nose2", "tox",
                       "make test", "make check", "run-tests", "runtests"):
            with self.subTest(script=script):
                self.assertTrue(_RUNNERS.search(script), "not a runner — fix the case")
                self.assertTrue(_TEST_SHAPED.search(script),
                                f"`{script}` is a runner that is not test-shaped")

    def test_it_finds_jobs_in_the_real_workflows_at_all(self):
        self.assertTrue(WORKFLOWS, "no workflows found — every check here is vacuous")
        self.assertTrue(suite_jobs(), "no job in this repository appears to run the "
                                      "suite, which means this reader is broken")


class NoJobsTestInvocationGoesUnclassified(unittest.TestCase):
    """**The half that makes the rest fail closed**, and it is not redundant with the
    per-job assertion next door.

    That one asks "does every job I RECOGNISE as running tests carry the pin?", and a job
    written in a spelling this reader does not know is not in the set it quantifies over —
    so it passes, and the pinned list passes too, and nothing anywhere says a job is
    running the real-tmux modules on an unpinned tmux. Both halves green, no coverage.

    So a run block that is about the tests and matches no runner this module knows is a
    failure with the job's name in it, and the way out is to teach `_RUNNERS` the spelling
    (if it runs tests) or to make the script say plainly that it does not.
    """

    def test_every_job_this_reader_meets_it_can_answer_for(self):
        found = unclassified_jobs()
        self.assertEqual(
            found, [],
            f"{found} run something test-shaped that this module cannot classify. That "
            f"is refused rather than skipped: an unrecognised runner is exactly how a job "
            f"comes to run the real-tmux modules on `ubuntu-latest`'s tmux 3.4 — where "
            f"they flake about one run in eight (#1116) — with this file still green. If "
            f"it runs charter's tests, add its spelling to `_RUNNERS` AND `_TEST_SHAPED` "
            f"and give the job `uses: {TMUX_ACTION}`. If it does not, it should not be "
            f"reading like it does.")


class EveryJobThatRunsTheSuiteInstallsThatTmux(unittest.TestCase):
    """A job that runs charter's tests runs them on a tmux that can report a death."""

    def test_every_one_of_them_uses_the_action(self):
        for workflow, job in suite_jobs():
            with self.subTest(workflow=workflow, job=job):
                uses = [step.get("uses") for step in _steps(_jobs(GITHUB / "workflows"
                                                                 / workflow)[job])
                        if isinstance(step, dict)]
                self.assertIn(
                    TMUX_ACTION, uses,
                    f"{workflow}'s `{job}` runs the suite on whatever tmux the runner "
                    f"image happens to ship — 3.4 today, which loses `pane-died` about "
                    f"one run in eight. Add `- uses: {TMUX_ACTION}` before the step that "
                    f"runs the tests.")

    def test_the_jobs_it_finds_are_the_ones_this_repository_has(self):
        """Pinned whole, so that a new suite-running job is a deliberate edit here rather
        than a job that quietly runs on an unpinned tmux. `dev-install` is absent on
        purpose: it installs charter from a git URL and runs `--version`, `--help` and
        `doctor`, and opens no pane."""
        self.assertEqual(suite_jobs(), [
            ("release.yml", "test"),
            ("sweep.yml", "plan"),
            ("sweep.yml", "sweep"),
            ("test.yml", "test"),
        ])


class TheActionAndThisModuleAgreeAboutTheFloor(unittest.TestCase):
    """#670's shape, applied to a version instead of a duration: the runner refuses below
    a floor and the suite refuses below a floor, and there is no reading under which those
    should be two different numbers."""

    def setUp(self):
        self.text = ACTION_FILE.read_text(encoding="utf-8")

    def _env(self, key: str) -> str:
        found = re.search(rf"^\s*{key}:\s*(\S+)\s*$", self.text, re.MULTILINE)
        self.assertIsNotNone(found, f"{ACTION_FILE.name} states no `{key}`")
        return found.group(1)

    def test_the_action_states_the_same_floor_this_module_does(self):
        self.assertEqual((int(self._env("FLOOR_MAJOR")), int(self._env("FLOOR_MINOR"))),
                         CI_TMUX_FLOOR)

    def test_the_version_it_installs_is_at_or_above_that_floor(self):
        """`3.5a` and its kin: tmux spells a patch release with a trailing letter, which
        is not part of the ordering this floor is about."""
        said = self._env("TMUX_VERSION")
        parsed = re.match(r"^(\d+)\.(\d+)", said)
        self.assertIsNotNone(parsed, f"cannot read a version out of `{said}`")
        self.assertGreaterEqual((int(parsed.group(1)), int(parsed.group(2))),
                                CI_TMUX_FLOOR,
                                f"{ACTION_FILE.name} installs tmux {said}, which is below "
                                f"the floor it then asserts — so every job using it would "
                                f"fail at the check rather than at the edit")

    def test_the_tarball_is_pinned_by_content_and_not_merely_by_name(self):
        """A release asset is a name its publisher can re-point; a sha256 is not. The
        build step pipes this into `sha256sum -c`, which exits non-zero on a mismatch."""
        self.assertRegex(self._env("TMUX_SHA256"), r"^[0-9a-f]{64}$")
        self.assertIn("sha256sum -c", self.text,
                      "the digest is recorded but never checked against the download")


class TheCIFloorIsTheEndedTabFloor(unittest.TestCase):
    """The THIRD copy of the same measurement, held to the other two.

    `CI_TMUX_FLOOR` above says *"written here and nowhere else"*, and names
    `.github/actions/tmux/action.yml` as the one second copy that exists because a job has
    to refuse before the suite starts. `tmuxctl.ENDED_TAB_FLOOR` is now a third, and it is
    the same 3.5 for the same reason: the rate at which `pane-died` is missed was measured
    at zero there and non-zero below.

    **Two directions, one fact.** This module's constant is the tmux CI must RUN — without
    it the suite is red for a reason that is not the branch's. That one is the tmux the
    ended tab NEEDS — without it an operator's harness exits and nothing is presented.
    Nothing says those must move together forever, and that is exactly why the pin is here
    rather than a comment: the day they diverge, somebody has to come and argue with this
    test, which is where the argument belongs. #670 is what an unpinned second copy costs.
    """

    def test_the_floor_ci_runs_is_the_floor_the_ended_tab_needs(self):
        self.assertEqual(CI_TMUX_FLOOR, tmuxctl.ENDED_TAB_FLOOR)


@unittest.skipUnless(os.environ.get("GITHUB_ACTIONS") == "true",
                     "not on a GitHub runner — this is about what CI installs")
class TheRunnerRunsATmuxThatCanReportADeath(unittest.TestCase):
    """The loud half, and the reason a silent downgrade cannot come back as a flake.

    **Scoped to GitHub's runners rather than to `$CI`.** The pin is a property of these
    workflows, and a developer whose shell exports `CI=1` has not agreed to have their
    suite refused on the tmux their distribution ships. Off a runner this is a skip, which
    is the honest answer: nothing here has been arranged for.

    It reads the ambient environment on purpose, which `CONTRIBUTING.md` asks a test to
    say out loud. This is the saying.
    """

    def test_the_tmux_on_this_runner_is_at_or_above_the_floor(self):
        found = tmuxctl.version()
        self.assertIsNotNone(
            found,
            "no tmux on this runner, or a `tmux -V` charter cannot parse. Every "
            "real-tmux module in this suite is `skipUnless` a tmux, so this run would "
            "have reported green while measuring none of them.")
        self.assertGreaterEqual(
            found, CI_TMUX_FLOOR,
            f"this runner has tmux {found[0]}.{found[1]}, below the {CI_TMUX_FLOOR[0]}."
            f"{CI_TMUX_FLOOR[1]} floor. Below it the server can destroy a pane without "
            f"firing `pane-died` — `#{{pane_dead}}` 1 with `#{{pane_dead_status}}` and "
            f"`#{{pane_dead_signal}}` both empty — measured at 8 misses in 60 on 3.4 "
            f"against 0 in 40 on 3.5 (#1116). The real-tmux modules would not fail "
            f"cleanly on that, they would flake. `.github/actions/tmux` is what installs "
            f"it; this failure means that action did not run, or did not win the `$PATH`.")


if __name__ == "__main__":
    unittest.main()
