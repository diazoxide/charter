"""`charter update` — move charter, then say what moved and what it brought.

The command exists because three things called "updating charter" were three separate
commands with three different blast radii: a machine-global binary, a per-project harness
artifact, and a pin shared with teammates. This one converges them in the only order that
is safe, and stops at the one decision that is not charter's to make.

The pin decides the target. Moving the machine PAST a pin manufactures the drift
`charter version` reports as an error, so a plane that pins nothing goes to latest, a plane
behind its pin goes to the pin (conforming to a pin affects nobody), and a plane already ON
its pin with something newer available is asked — because that move is the team's.
"""

from __future__ import annotations

import io
import os
import unittest
from contextlib import redirect_stderr, redirect_stdout
from pathlib import Path
from types import SimpleNamespace
from unittest import mock

from charter import commands_update as cu
from tests._isolation import PersonaIso

INSTALLED = "0.44.1"


class UpdateCase(PersonaIso):
    def setUp(self):
        super().setUp()
        for target, new in (
            ("charter.commands_update._installed_version", lambda: INSTALLED),
            ("charter.commands_update._latest", lambda live=True: "0.46.0"),
        ):
            pt = mock.patch(target, new); pt.start(); self.addCleanup(pt.stop)
        self.moved: list[str] = []
        self.baseline_when_moved: list[str | None] = []

        def fake_sync(version: str):
            self.moved.append(version)
            self.baseline_when_moved.append(cu.read_baseline())
            return True, version
        pt = mock.patch("charter.commands_update._sync_to", fake_sync)
        pt.start(); self.addCleanup(pt.stop)
        pt = mock.patch("charter.commands_update._handoff", return_value=(True, ""))
        pt.start(); self.addCleanup(pt.stop)

    def pin(self, version: str) -> None:
        (self.tmp / "charter.toml").write_text(
            f'schema = 1\n\n[charter]\nversion = "{version}"\n')

    def update(self, **kw) -> tuple[int, str]:
        args = SimpleNamespace(to=None, bump=False)
        for k, v in kw.items():
            setattr(args, k, v)
        err = io.StringIO()
        with mock.patch.dict(os.environ, {"CHARTER_HARNESS": "claude-code"}), \
             redirect_stderr(err), redirect_stdout(io.StringIO()):
            code = cu.cmd_update(args)
        return code, err.getvalue()


class ThePinDecidesTheTarget(UpdateCase):
    def test_a_plane_that_pins_nothing_goes_to_latest(self):
        code, _ = self.update()
        self.assertEqual(code, 0)
        self.assertEqual(self.moved, ["0.46.0"])

    def test_a_plane_behind_its_pin_conforms_to_the_pin_not_to_latest(self):
        """Conforming to a pin somebody already chose affects nobody — this is exactly
        what `version sync` does, so it needs no confirmation."""
        self.pin("0.45.0")
        code, _ = self.update()
        self.assertEqual(code, 0)
        self.assertEqual(self.moved, ["0.45.0"])

    def test_a_plane_on_its_pin_is_asked_before_the_team_moves(self):
        """The pin is shared. Moving past it means every teammate conforms on their next
        session, so charter proposes and stops."""
        self.pin(INSTALLED)
        code, out = self.update()
        self.assertEqual(self.moved, [])
        self.assertIn("charter update --bump", out)

    def test_bump_is_the_yes(self):
        self.pin(INSTALLED)
        with mock.patch("charter.commands_update._bump_pin", return_value=True) as bump:
            code, _ = self.update(bump=True)
        self.assertEqual(self.moved, ["0.46.0"])
        bump.assert_called_once_with("0.46.0")

    def test_to_overrides_everything(self):
        self.pin("0.45.0")
        self.update(to="0.43.0")
        self.assertEqual(self.moved, ["0.43.0"])


class ThingsItRefusesOrDegrades(UpdateCase):
    def test_it_refuses_inside_a_charter_checkout(self):
        """`CONTRIBUTING.md` tells contributors to run `python3 -m charter` from the
        clone. Installing over that is never what "let me try the update command" meant,
        and the damage is silent — the news phase would then hand off to a binary that is
        not the tree being edited."""
        (self.tmp / "charter").mkdir(parents=True, exist_ok=True)
        (self.tmp / "charter" / "docsrc.py").write_text("")
        (self.tmp / "pyproject.toml").write_text('name = "charter-cp"\n')
        code, out = self.update()
        self.assertEqual(self.moved, [])
        self.assertNotEqual(code, 0)
        self.assertIn("charter version", out)

    def test_outside_a_harness_it_says_the_artifact_was_not_checked(self):
        with mock.patch.dict(os.environ, {}, clear=True), \
             mock.patch("charter.harness.current", return_value=None):
            err = io.StringIO()
            with redirect_stderr(err), redirect_stdout(io.StringIO()):
                cu.cmd_update(SimpleNamespace(to=None, bump=False))
        self.assertIn("not checked", err.getvalue())

    def test_a_failed_handoff_says_the_install_did_not_take(self):
        """PyPI's simple index lags its metadata endpoint. In that window an install can
        succeed against a cached index and leave you on the old version reporting success
        — so the news phase, which must run on the NEW binary anyway, doubles as the
        proof that it is there."""
        with mock.patch("charter.commands_update._handoff",
                        return_value=(False, "reports 0.44.1, expected 0.46.0")):
            code, out = self.update()
        self.assertNotEqual(code, 0)
        self.assertIn("did not take", out)


class TheBaselineIsStampedBeforeAnythingMoves(UpdateCase):
    def test_an_interrupted_update_still_knows_where_it_started(self):
        self.update()
        self.assertEqual(self.baseline_when_moved, [INSTALLED])


class InstallerDetection(unittest.TestCase):
    """`uv tool install` is not everyone's install. `docs/install.md` documents pipx and
    pip as fallbacks, so assuming uv would kill the automatic path for anyone who took a
    documented route."""

    def test_a_uv_tool_install_is_recognised(self):
        name, argv = cu.installer_for(Path("/home/x/.local/share/uv/tools/charter-cp/bin/python"))
        self.assertEqual(name, "uv")
        self.assertIn("uv", argv[0])

    def test_a_pipx_install_is_recognised(self):
        name, argv = cu.installer_for(Path("/home/x/.local/pipx/venvs/charter-cp/bin/python"))
        self.assertEqual(name, "pipx")

    def test_an_unrecognised_install_is_named_not_guessed(self):
        """Ambiguity resolves to *named, not run* — the same restraint charter keeps for
        a host's plugin command."""
        name, argv = cu.installer_for(Path("/usr/bin/python3"))
        self.assertEqual(name, "unknown")
        self.assertIsNone(argv)


class TheDoctorRow(PersonaIso):
    """Where "suggest new features" reaches somebody who did not just type `update`.

    Only `doctor` — not the session-start hook. A probe is real work, and running N of them
    on every session start is exactly the cost `update.py` was built to keep off the status
    line's clock. `doctor` is already where you go to ask "is this plane in good shape?".
    """

    def _row(self, entries):
        from charter import doctor, news

        with mock.patch.object(news, "released", return_value=entries):
            return doctor.check_news_adoption()

    def test_nothing_pending_is_a_clean_row(self):
        from charter import doctor

        r = self._row([])
        self.assertEqual(r.status, doctor.OK)

    def test_pending_entries_are_counted_and_name_the_command(self):
        from charter import doctor, news

        e = news.Entry("0.44.0", "delegate-when", "h", "persona lint", "", "b", Path("x"))
        with mock.patch.object(news, "probe", return_value=(news.PENDING, "")):
            r = self._row([e])
        self.assertEqual(r.status, doctor.WARN)
        self.assertIn("1", r.detail)
        self.assertIn("charter news --pending", r.hint)

    def test_an_unprobeable_entry_is_reported_as_unchecked_not_as_health(self):
        from charter import doctor, news

        e = news.Entry("0.44.0", "x", "h", "persona gone", "", "b", Path("x"))
        with mock.patch.object(news, "probe", return_value=(news.UNKNOWN, "no such")):
            r = self._row([e])
        self.assertEqual(r.status, doctor.WARN)
        self.assertIn("unchecked", r.detail)

    def test_the_row_is_registered_so_it_actually_runs(self):
        from charter import doctor

        self.assertIn("news", [r.name for r in doctor.run_all()])


if __name__ == "__main__":
    unittest.main()
