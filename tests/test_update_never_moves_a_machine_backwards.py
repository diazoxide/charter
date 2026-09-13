"""#1017: `charter update` on a plane with no pin installed an OLDER charter, and exited 0.

Measured with stubs, nothing installed: running 0.60.0, the update cache holding ``latest:
0.58.0``, PyPI's GET failing, no ``[charter] version`` pin. `charter update` installed
0.58.0. Nothing had to go wrong for that state to exist: ``uv tool upgrade`` and ``pipx
upgrade`` move the binary without touching charter's cache, and the cache is per plane.

Two things were wrong, and each class below pins one of them on its own:

* **The version to move to came from a cache the run had not written.** `_latest` called
  `update.fetch_and_store`, ignored what it returned, and re-read the cache, which a failed
  GET leaves untouched. #941 removed exactly that pattern from `charter version bump`. Now
  the target is what this run fetched, and when nothing came back #1013's refusal applies,
  unchanged.
* **Nothing compared the target with the charter running.** With no pin, whatever PyPI's
  answer said became the target. A fresh answer can be older too, and `charter update` is
  what a person runs to get fixes, so moving them backwards can take a security fix away
  from a machine that already had it. Now it installs nothing, says the two versions, and
  exits 1 like #1013's refusal. A version asked for by number with `--to` still installs.
  `charter version bump` had the same gap with more reach, because it also writes the pin
  the whole team conforms to, and it now refuses the same answer the same way.

ADR 0009: the refusal names no cause for PyPI's older answer, because charter checked none.
ADR 0013: the stale cache's number is never printed as something PyPI said.

Nothing here reaches the network or installs anything: `NoNetwork` refuses every route
out, PyPI's GET is stubbed where a test needs an answer, and `_sync_to`, `_move_harness` and
`_handoff` are recorders.
"""

from __future__ import annotations

import io
import json
import os
from contextlib import redirect_stderr, redirect_stdout
from types import SimpleNamespace
from unittest import mock

from charter import commands_update as cu
from charter import config, update
from tests._isolation import PersonaIso, pin_update_channel
from tests.test_dev_channel import NoNetwork

RUNNING = "0.60.0"
OLDER = "0.58.0"
NEWER = "0.62.0"


class UpdateRun(NoNetwork, PersonaIso):
    """`cmd_update` with the real `_latest` and `fetch_and_store`, and PyPI's GET stubbed."""

    def setUp(self):
        super().setUp()
        pin_update_channel(self, "stable")
        self.enterContext(mock.patch.object(cu, "_installed_version", lambda: RUNNING))
        # A published wheel, so "already on the target" means what it says (#537).
        self.enterContext(mock.patch("charter.channel.is_dev_build", return_value=False))
        self.moved: list[str] = []
        self.enterContext(mock.patch.object(
            cu, "_sync_to", side_effect=lambda v: (self.moved.append(v), (True, v))[1]))
        self.harness = self.enterContext(mock.patch.object(cu, "_move_harness"))
        self.handoff = self.enterContext(
            mock.patch.object(cu, "_handoff", return_value=(True, "")))
        self.bump_pin = self.enterContext(
            mock.patch.object(cu, "_bump_pin", return_value=True))

    def cache(self, latest: str) -> None:
        p = update._cache_file()
        p.parent.mkdir(parents=True, exist_ok=True)
        p.write_text(json.dumps({"latest": latest, "ts": 1.0}))

    def pin(self, version: str) -> None:
        (self.tmp / "charter.toml").write_text(
            f'schema = 1\n\n[charter]\nversion = "{version}"\n')
        self.enterContext(mock.patch.object(config, "HAS_CONTROL_PLANE", True))

    def update(self, pypi: str | None, **kw) -> tuple[int, str]:
        args = SimpleNamespace(**{"to": None, "bump": False, **kw})
        err = io.StringIO()
        with mock.patch.object(update, "_fetch_latest", return_value=pypi) as fetch, \
                mock.patch.dict(os.environ, {"CHARTER_HARNESS": "claude-code"}), \
                redirect_stderr(err), redirect_stdout(io.StringIO()):
            code = cu.cmd_update(args)
        self.fetches = fetch.call_count
        return code, err.getvalue()

    def assertNothingRan(self) -> None:
        self.assertEqual(self.moved, [])
        self.harness.assert_not_called()
        self.handoff.assert_not_called()
        self.bump_pin.assert_not_called()


class TheTargetIsWhatThisRunFetched(UpdateRun):
    def test_a_failed_fetch_over_an_older_cache_installs_nothing(self):
        """The report, line for line. The refusal is #1013's, word for word, and the cached
        number appears nowhere: printing it as PyPI's answer would present as checked a
        value this run never received (ADR 0013)."""
        self.cache(OLDER)
        code, out = self.update(pypi=None)
        self.assertEqual(code, 1, out)
        self.assertNothingRan()
        self.assertIn("no version came back from PyPI to check against", out)
        self.assertNotIn(OLDER, out, "the stale cache was reported as an answer from PyPI")

    def test_a_failed_fetch_over_a_newer_cache_installs_nothing_either(self):
        """The same rule from the other side, with no downgrade in it: a cache is what an
        EARLIER run was told, and this run was told nothing. Without this case the first
        one would pass on the downgrade refusal alone, with the cache still being read."""
        self.cache(NEWER)
        code, out = self.update(pypi=None)
        self.assertEqual(code, 1, out)
        self.assertNothingRan()
        self.assertIn("no version came back from PyPI to check against", out)

    def test_a_successful_fetch_is_the_target_whatever_the_cache_held(self):
        self.cache(OLDER)
        code, out = self.update(pypi=NEWER)
        self.assertEqual(code, 0, out)
        self.assertEqual(self.moved, [NEWER])
        self.assertEqual(self.fetches, 1)

    def test_a_padded_answer_is_trimmed_before_it_reaches_the_installer(self):
        """It becomes the right-hand side of ``charter-cp==``. The cache read it replaces
        was trimmed, and a trailing newline there is a version nothing can install."""
        code, out = self.update(pypi=f" {NEWER}\n")
        self.assertEqual(code, 0, out)
        self.assertEqual(self.moved, [NEWER])


class NothingMovesAMachineBackwardsUnasked(UpdateRun):
    def assertRefusedTheOlderVersion(self, code: int, out: str) -> None:
        self.assertEqual(code, 1, out)
        self.assertNothingRan()
        self.assertIn(f"PyPI reported {OLDER} as the newest release, which is older than the "
                      f"{RUNNING} this machine runs, so nothing was installed", out)
        self.assertIn("charter update --to X.Y.Z", out)
        # PyPI DID answer, so #1013's sentence would claim a check failed that succeeded.
        self.assertNotIn("no version came back", out)
        # No cause is offered for the older answer, because none was checked (ADR 0009).
        self.assertNotIn("offline", out)

    def test_a_fresh_answer_older_than_the_running_charter_is_refused(self):
        code, out = self.update(pypi=OLDER)
        self.assertRefusedTheOlderVersion(code, out)

    def test_bump_does_not_turn_the_older_answer_into_the_teams_pin(self):
        """With no pin, `--bump` writes one after the install, so an older answer would be
        installed AND pinned for every teammate."""
        self.enterContext(mock.patch.object(config, "HAS_CONTROL_PLANE", True))
        code, out = self.update(pypi=OLDER, bump=True)
        self.assertRefusedTheOlderVersion(code, out)

    def test_an_answer_equal_to_the_running_charter_is_current_not_older(self):
        """The boundary. Equal is a machine already on the newest release: it installs
        nothing and succeeds, as it did before, without a word about older versions."""
        code, out = self.update(pypi=RUNNING)
        self.assertEqual(code, 0, out)
        self.assertEqual(self.moved, [])
        self.assertNotIn("older", out)
        self.handoff.assert_called_once()

    def test_to_an_older_version_installs_it_because_it_was_asked_for(self):
        """Going back to a release is a real case, and `--to` is how it is asked for. It
        asks PyPI nothing, so no answer from PyPI can refuse it."""
        code, out = self.update(pypi=NEWER, to=OLDER)
        self.assertEqual(code, 0, out)
        self.assertEqual(self.moved, [OLDER])
        self.assertEqual(self.fetches, 0)
        self.assertNotIn("nothing was installed", out)

    def test_a_pin_older_than_the_running_charter_is_not_refused_as_a_downgrade(self):
        """A pin is the team's decision, so the refusal is not about pins. What `update` does
        on a machine ahead of its pin stays what #1013 settled: it does not move it, and the
        drift is `charter version`'s to report. `charter version sync` is what conforms down."""
        self.pin(OLDER)
        code, out = self.update(pypi=OLDER)
        self.assertEqual(code, 0, out)
        self.assertEqual(self.moved, [])
        self.assertNotIn("nothing was installed", out)
        self.assertNotIn("older than", out)

    def test_a_pin_newer_than_the_running_charter_still_conforms(self):
        self.pin(NEWER)
        code, out = self.update(pypi=OLDER)
        self.assertEqual(code, 0, out)
        self.assertEqual(self.moved, [NEWER])
        self.assertNotIn("older than", out)


class VersionBumpDoesNotPinTheTeamBackwards(NoNetwork, PersonaIso):
    """The same rule, where it costs more. `version bump` with no `--to` takes the version
    its own GET returned (#941), and until #1017 nothing compared that with the charter
    running. So an older answer was installed over it AND written as the pin, and with
    `--push` every teammate conformed to it on their next session."""

    def setUp(self):
        super().setUp()
        pin_update_channel(self, "stable")
        self.enterContext(mock.patch("charter.commands._installed_version", lambda: RUNNING))
        self.calls: list[tuple] = []
        self.enterContext(mock.patch(
            "charter.commands.sync_to",
            side_effect=lambda v: (self.calls.append(("sync_to", v)), (True, v))[1]))
        self.enterContext(mock.patch(
            "charter.instance.set_locked_version",
            side_effect=lambda root, v: (self.calls.append(("set_locked_version", v)), True)[1]))
        self.enterContext(mock.patch(
            "charter.commands.commit_push",
            side_effect=lambda root, add, msg: (self.calls.append(("commit_push", msg)), 0)[1]))

    def bump(self, pypi: str | None, **kw) -> tuple[int, str]:
        from charter import commands

        args = SimpleNamespace(**{"to": None, "push": True, **kw})
        out = io.StringIO()
        with mock.patch.object(update, "_fetch_latest", return_value=pypi) as fetch, \
                redirect_stderr(out), redirect_stdout(out):
            code = commands.cmd_version_bump(args)
        self.fetches = fetch.call_count
        return code, out.getvalue()

    def test_a_fetched_answer_older_than_the_running_charter_is_neither_installed_nor_pinned(self):
        code, out = self.bump(pypi=OLDER)
        self.assertEqual(code, 1, out)
        self.assertEqual(self.calls, [], "bump installed or pinned a version older than running")
        self.assertIn(f"PyPI reported {OLDER} as the newest release, which is older than the "
                      f"{RUNNING} this machine runs, so nothing was installed or pinned", out)
        self.assertIn("charter version bump --to X.Y.Z", out)
        self.assertNotIn(f"--to {OLDER}", out)
        self.assertNotIn("no version came back", out)
        self.assertNotIn("offline", out)

    def test_a_fetched_answer_equal_to_the_running_charter_is_pinned(self):
        """The boundary: pinning the team to the charter this machine already runs is what
        bump is for, and nothing needs installing to verify it."""
        code, out = self.bump(pypi=RUNNING)
        self.assertEqual(code, 0, out)
        self.assertEqual(self.calls, [("set_locked_version", RUNNING),
                                      ("commit_push", f"charter: pin to {RUNNING}")])

    def test_a_fetched_answer_newer_than_the_running_charter_is_installed_and_pinned(self):
        code, out = self.bump(pypi=NEWER)
        self.assertEqual(code, 0, out)
        self.assertEqual(self.calls, [("sync_to", NEWER), ("set_locked_version", NEWER),
                                      ("commit_push", f"charter: pin to {NEWER}")])

    def test_to_an_older_version_pins_it_because_it_was_named(self):
        """Pinning a fleet back to a known-good release is a real case, and `--to` is how it
        is asked for. It asks PyPI nothing, so no answer from PyPI can refuse it."""
        code, out = self.bump(pypi=NEWER, to=OLDER)
        self.assertEqual(code, 0, out)
        self.assertEqual(self.fetches, 0)
        self.assertEqual(self.calls, [("sync_to", OLDER), ("set_locked_version", OLDER),
                                      ("commit_push", f"charter: pin to {OLDER}")])


if __name__ == "__main__":
    import unittest
    unittest.main()
