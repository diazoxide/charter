"""A test run must leave no row in the operator's trace (#372).

`charter trace --summary` reported 581 guard denials for one session, 556 of which were
recorded while charter's own test suite ran. A suite run resolves its plane the way every
command does — walk up for `charter.toml`, then hop outward through `workspaces/` — so a
charter checkout that lives inside somebody's plane resolves to THAT plane. Any test that
reaches a real handler without redirecting `config` therefore appends to the developer's
own `.charter/persona-state/trace/`, under the ambient `$CHARTER_SESSION_ID`: the live
session bucket the summary reads.

**The boundary is the writer, and it stays there.** #227 put it there — it pinned
`$CHARTER_ROOT` for the hook subprocesses and moved `test_plugin` onto `config.use` — and
that is what killed the denials. The two alternatives were considered and refused:

* *Filter the summary.* A count that drops rows makes "quiet because nothing happened" and
  "quiet because we filtered" print the same thing, which is the one confusion an
  observability tool may not create.
* *Mark the rows and say "556 of these were the test suite".* Honest to read, but the mark
  has to come from something the runtime can see — an env var or a config key — and that is
  an override the agent under observation could set on its own denials.
  `hooks._OVERRIDE_NOTE` refuses exactly that trade for the guards; the record of the guards
  does not get a weaker rule than the guards.

So nothing is filtered and nothing is marked, and the invariant that makes that safe is
asserted here: no fixture may write into the plane the suite resolved to.

Both defects below have the same shape — a fixture that hand-picks the `config` attributes
it redirects and misses `PERSONA_STATE_DIR`, which is the attribute the trace writes
through. `config.DERIVED` exists so that a fixture cannot miss one (its own docstring names
the vault registry a hand-picked list already orphaned), so the fix is to redirect through
`config.use` rather than to lengthen the list by one.

They were found by running the suite with `$CHARTER_ROOT` pinned to an empty plane and
listing what appeared under its `persona-state/`. That is also how a third would be found;
this module only pins the two that exist.
"""

from __future__ import annotations

import shutil
import tempfile
import unittest
from pathlib import Path

from charter import config, trace
# The MODULES, never the classes: `from … import TestSessionLock` would bind a TestCase
# subclass in this module's namespace, and `TestLoader` collects by namespace — the
# borrowed fixtures would then run a second time in every suite run, under this file's name.
from tests import test_inflight_dispatch, test_workspace_lock


class _AFixtureThatForgetsPersonaStateDir(unittest.TestCase):
    """The positive control: a fixture with the exact defect, kept so the probe below is
    known to SEE a leak rather than merely to report none.

    Its probe method is deliberately not named ``test_*``. Discovery collects this class
    (a leading underscore does not hide it from `TestLoader`) and would otherwise run the
    leak for real, into the developer's plane, every suite run.
    """

    def setUp(self) -> None:
        self.tmp = Path(tempfile.mkdtemp(prefix="charter-372-control-"))
        self._orig = config.STATE_DIR
        config.STATE_DIR = self.tmp / ".charter"      # ...and PERSONA_STATE_DIR is missed
        self.addCleanup(shutil.rmtree, self.tmp, True)
        self.addCleanup(lambda: setattr(config, "STATE_DIR", self._orig))

    def runProbe(self) -> None:
        trace.record("probe", session="control")


class TraceLeakProbe(unittest.TestCase):
    def files_left_in_the_operators_plane(self, case_class, method: str) -> list[str]:
        """Run one real test method with the ambient plane pointed at a sentinel, and
        return the trace files the run left behind there.

        The sentinel is a throwaway plane, never the real one: a RED run of this module
        must not commit the very act it exists to forbid. It is a plane rather than a bare
        directory (`charter.toml` present) so `HAS_CONTROL_PLANE` is what it would be on a
        developer's machine — the guards and nudges these fixtures drive are gated on it,
        and a fixture that stopped reaching its handler would report "no leak" for the
        wrong reason.
        """
        sentinel = Path(tempfile.mkdtemp(prefix="charter-372-sentinel-"))
        (sentinel / "charter.toml").write_text("schema = 1\n")
        outer = config.use(sentinel)
        try:
            result = unittest.TestResult()
            case_class(method).run(result)
            self.assertTrue(
                result.wasSuccessful(),
                f"{case_class.__name__}.{method} did not pass, so it proves nothing about "
                f"leaking: {result.errors or result.failures}")
            d = sentinel / ".charter" / "persona-state" / "trace"
            return sorted(p.name for p in d.glob("*.jsonl")) if d.is_dir() else []
        finally:
            config.restore(outer)
            shutil.rmtree(sentinel, ignore_errors=True)


class TestTheProbeCanSeeALeak(TraceLeakProbe):
    """Without this, every assertion below would pass just as well if the probe were blind."""

    def test_a_fixture_that_misses_persona_state_dir_is_caught(self):
        self.assertEqual(
            ["control.jsonl"],
            self.files_left_in_the_operators_plane(
                _AFixtureThatForgetsPersonaStateDir, "runProbe"))


class TestNoFixtureWritesIntoTheOperatorsTrace(TraceLeakProbe):
    def test_the_workspace_lock_fixture_writes_nowhere_real(self):
        """`workspace.set_active` records `workspace-use`. This fixture redirected five
        attributes and not `PERSONA_STATE_DIR`, so twenty rows a run landed in the
        developer's plane — under `sess-lock-test` and `fresh-sess`, buckets that are still
        sitting in real `.charter/persona-state/trace/` directories."""
        self.assertEqual(
            [],
            self.files_left_in_the_operators_plane(
                test_workspace_lock.TestSessionLock,
                "test_first_confirm_locks_the_session"))

    def test_the_dispatch_nudge_fixture_writes_nowhere_real(self):
        """The worst of the two, because it has no session id of its own: the payload
        carries none, so `_trace("dispatch-ask", ...)` falls through to the ambient
        `$CHARTER_SESSION_ID` and the row lands in the operator's LIVE session — exactly
        where `charter trace --summary` counts it."""
        self.assertEqual(
            [],
            self.files_left_in_the_operators_plane(
                test_inflight_dispatch.DispatchNudge,
                "test_overlapping_code_writer_is_nudged"))


if __name__ == "__main__":
    unittest.main()
