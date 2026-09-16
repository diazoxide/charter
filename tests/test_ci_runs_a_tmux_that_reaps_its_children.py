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
#: **Written here and nowhere else.** `.github/actions/tmux/action.yml` states the same
#: two numbers in `FLOOR_MAJOR`/`FLOOR_MINOR` so that the job fails before the suite even
#: starts, and `TheActionAndThisModuleAgreeAboutTheFloor` holds the two copies together.
#: One measurement written in two files and drifting is this repository's #670 and it is
#: not repeated here.
CI_TMUX_FLOOR = (3, 5)

#: The local composite action that installs it, as a `uses:` reference.
TMUX_ACTION = "./.github/actions/tmux"

ACTION_FILE = REPO / ".github" / "actions" / "tmux" / "action.yml"

#: Every workflow CI can run. Read off disk rather than listed, so a fourth one added
#: beside them is read too — `EveryJobThatRunsTheSuiteInstallsThatTmux` is only as complete
#: as this glob.
WORKFLOWS = sorted((GITHUB / "workflows").glob("*.y*ml"))


def _runs_the_suite(script: str) -> bool:
    """Does this `run:` body execute charter's test suite?

    Three spellings, and the third is the one worth naming. `unittest discover` is the
    suite outright. `sweep.py --plan` traces it once to build the selection map, and
    `sweep.py --gate` runs the unmutated baseline before it deals a single mutation — a
    shard whose baseline is red refuses its whole slice, which is exactly what three of
    them did.

    `sweep.py --verdict` is the one sweep mode that runs no tests at all: it adds up
    artifacts other jobs uploaded. It is excluded by being absent from the list rather than
    by a denial, and `TheReaderKnowsWhichStepsRunTests` proves that both ways round —
    because a reader that answered `True` for everything would satisfy every assertion
    below while checking nothing.
    """
    return ("unittest discover" in script
            or ("sweep.py" in script and ("--plan" in script or "--gate" in script)))


def _jobs(workflow: Path) -> dict:
    loaded = load(workflow.read_text(encoding="utf-8"))
    return loaded.get("jobs", {}) if isinstance(loaded, dict) else {}


def _steps(job: object) -> list:
    return job.get("steps", []) if isinstance(job, dict) else []


def suite_jobs() -> list[tuple[str, str]]:
    """``(workflow file name, job name)`` for every job that runs the suite."""
    return [(path.name, name)
            for path in WORKFLOWS
            for name, job in _jobs(path).items()
            if any(_runs_the_suite(step["run"])
                   for step in _steps(job) if isinstance(step, dict) and "run" in step)]


class TheReaderKnowsWhichStepsRunTests(unittest.TestCase):
    """The control. Every assertion below is about a set this reader produces, and a
    reader that had gone blind would hand back an empty set that satisfies all of them —
    the same shape of defect `TheWorkflowQuotesNoCostTheToolDoesNotState` guards next
    door. So the predicate is exercised on the exact strings the workflows carry."""

    def test_the_three_spellings_that_run_the_suite(self):
        self.assertTrue(_runs_the_suite("python -m unittest discover -s tests -v"))
        self.assertTrue(_runs_the_suite("python3 tools/sweep.py --plan --warm-map"))
        self.assertTrue(_runs_the_suite("python3 tools/sweep.py --gate --jobs 4"))

    def test_the_sweep_mode_that_runs_nothing_is_not_one_of_them(self):
        """`collect` downloads artifacts and adds them up. Requiring a tmux build there
        would be a minute per run spent on a job with no pane in it."""
        self.assertFalse(_runs_the_suite("python3 tools/sweep.py --verdict shards"))
        self.assertFalse(_runs_the_suite("charter --version"))
        self.assertFalse(_runs_the_suite("uv tool install --force git+https://x@main"))

    def test_it_finds_jobs_in_the_real_workflows_at_all(self):
        self.assertTrue(WORKFLOWS, "no workflows found — every check here is vacuous")
        self.assertTrue(suite_jobs(), "no job in this repository appears to run the "
                                      "suite, which means this reader is broken")


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
