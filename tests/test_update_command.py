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
        """Write the lock — and say so, because `HAS_CONTROL_PLANE` is DERIVED at config
        load. Writing charter.toml afterwards leaves the flag reading False, which is a
        control plane the code correctly does not believe in."""
        from charter import config

        (self.tmp / "charter.toml").write_text(
            f'schema = 1\n\n[charter]\nversion = "{version}"\n')
        pt = mock.patch.object(config, "HAS_CONTROL_PLANE", True)
        pt.start(); self.addCleanup(pt.stop)

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


class TheNetworkIsReadOnce(PersonaIso):
    """`_latest` is a LIVE read — an explicit command a person is waiting on, unlike the
    status line's cached one. Asking twice to answer one question doubles that wait for
    nothing, and the propose path (pin == installed, something newer published) is the
    one most likely to be run repeatedly."""

    def test_proposing_a_bump_reads_pypi_once(self):
        (self.tmp / "charter.toml").write_text(
            f'schema = 1\n\n[charter]\nversion = "{INSTALLED}"\n')
        with mock.patch("charter.commands_update._installed_version", return_value=INSTALLED), \
             mock.patch("charter.update.fetch_and_store", return_value="0.46.0") as fetch, \
             redirect_stderr(io.StringIO()), redirect_stdout(io.StringIO()):
            cu.cmd_update(SimpleNamespace(to=None, bump=False))
        self.assertEqual(fetch.call_count, 1)


class ThingsItRefusesOrDegrades(UpdateCase):
    def test_it_refuses_when_the_charter_it_is_running_is_the_tree(self):
        """`CONTRIBUTING.md` tells contributors to run `python3 -m charter` from the
        clone. Installing over that is never what "let me try the update command" meant,
        and the damage is silent — the news phase would then hand off to a binary that is
        not the tree being edited.

        Asked of `channel.running_inside`, which is where the running charter says where it
        loaded from. Patched here because this suite cannot make the charter it imported
        live under a temporary directory; the *unpatched* half is
        `TheRefusalIsAboutTheRunningInstall` in
        `tests/test_update_asks_the_running_install.py`, which measures the real one.
        """
        from charter import channel
        with mock.patch.object(channel, "running_inside", return_value=True):
            code, out = self.update()
        self.assertEqual(self.moved, [])
        self.assertNotEqual(code, 0)
        self.assertIn("charter version", out)

    def test_standing_in_a_charter_clone_is_not_running_it(self):
        """#537, reproduced from the files rather than from a patch.

        The plane root here is a charter checkout by every test `doctor` applies — the
        source tree and a `pyproject.toml` naming the distribution — and the charter running
        this process is somewhere else entirely, which is the ordinary state of a maintainer
        who has both a clone and the `uv tool` install the dev channel documents.

        The command used to read the first and claim the second: *the charter you run is
        this checkout, moved by git*, about a binary `git pull` cannot reach. It installed
        nothing and said nothing was needed, and `charter --version` reported `main @
        e17801c` while the clone's `HEAD` was `97163fb` — two commits that could not
        disagree if the claim were true.
        """
        from charter import channel, doctor
        (self.tmp / "charter").mkdir(parents=True, exist_ok=True)
        (self.tmp / "charter" / "docsrc.py").write_text("")
        (self.tmp / "pyproject.toml").write_text('name = "charter-cp"\n')
        # Both halves of the disagreement, stated: the directory says checkout, the running
        # install says it is not from here. Nothing is patched to make either one true.
        self.assertTrue(doctor._is_charter_checkout(self.tmp))
        self.assertFalse(channel.running_inside(self.tmp))
        code, _out = self.update()
        self.assertEqual(code, 0)
        self.assertEqual(self.moved, ["0.46.0"])

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


class NothingToCheckAgainstIsSaidNotSwallowed(UpdateCase):
    """#950. With no `--to` and no pin to move to, the target is whatever PyPI says is
    latest — and when nothing came back, there is no target.

    `_resolve_target` used to answer the installed version there instead, so `charter
    update` moved the harness, ran the news phase and exited 0 over a check it never made:
    the same output as a plane that is current. The refusal written for this case sat one
    line below and could not fire, because nothing ever handed it a missing target.

    It names the same two candidates `version bump` names for the same condition (#941),
    never "offline?": `_latest` also comes back empty when PyPI answered and the cache write
    failed, and a cause charter did not verify tells the reader to stop looking (ADR 0009).
    """

    def setUp(self):
        super().setUp()
        pt = mock.patch("charter.commands_update._latest", lambda live=True: None)
        pt.start(); self.addCleanup(pt.stop)
        pt = mock.patch("charter.commands_update._move_harness")
        self.harness = pt.start(); self.addCleanup(pt.stop)
        pt = mock.patch("charter.commands_update._handoff", return_value=(True, ""))
        self.handoff = pt.start(); self.addCleanup(pt.stop)

    def assertSaidNothingWasChecked(self, code: int, out: str) -> None:
        self.assertEqual(code, 1, out)
        self.assertEqual(self.moved, [])
        self.assertIn("no version came back from PyPI", out)
        self.assertIn("either it did not answer, or its answer could not be cached", out)
        self.assertIn("charter update --to X.Y.Z", out)
        self.assertNotIn("offline", out)
        # Nothing that follows a resolved target ran, so nothing printed that reads as
        # "already current": no artifact "already on", no news phase.
        self.harness.assert_not_called()
        self.handoff.assert_not_called()

    def test_a_plane_that_pins_nothing_is_told_nothing_was_checked(self):
        code, out = self.update()
        self.assertSaidNothingWasChecked(code, out)

    def test_bump_on_its_pin_is_told_rather_than_left_where_it_was(self):
        """`--bump` asks for the pin to move to what is published. Nothing came back, so
        there is nothing to move it to — and exiting 0 with the pin unmoved is the reply a
        plane already on the newest release gets."""
        self.pin(INSTALLED)
        with mock.patch("charter.commands_update._bump_pin", return_value=True) as bump:
            code, out = self.update(bump=True)
        self.assertSaidNothingWasChecked(code, out)
        bump.assert_not_called()

    def test_a_machine_on_its_pin_succeeds_and_says_only_what_it_checked(self):
        """Without `--bump`, a machine already on its pin has nothing to conform: the pin
        is the plane's answer and it is met, so an offline `charter update` on a team
        machine is not a failure. What PyPI would have decided is only whether to PROPOSE a
        bump, and that is the one thing the line owns up to not knowing — with no word
        that reads as "latest", which nothing here established."""
        self.pin(INSTALLED)
        code, out = self.update()
        self.assertEqual(code, 0, out)
        self.assertEqual(self.moved, [])
        self.assertIn(f"this machine is on the plane's pin {INSTALLED}", out)
        self.assertIn("whether a newer release is published could not be checked", out)
        self.assertIn("either it did not answer, or its answer could not be cached", out)
        self.assertNotIn("latest", out.lower())
        self.assertNotIn("offline", out)
        self.assertNotIn("charter update --to", out)

    def test_a_machine_ahead_of_its_pin_succeeds_and_says_so(self):
        """Ahead of the pin, `update` never moved the machine either: with no `--bump`,
        PyPI only decides whether to propose moving the pin. The drift is `charter
        version`'s to report, so the line states the two versions and nothing more."""
        self.pin("0.44.0")
        code, out = self.update()
        self.assertEqual(code, 0, out)
        self.assertEqual(self.moved, [])
        self.assertIn(f"this machine runs {INSTALLED}, ahead of the plane's pin 0.44.0", out)
        self.assertIn("whether a newer release is published could not be checked", out)
        self.assertIn("either it did not answer, or its answer could not be cached", out)
        self.assertNotIn("on the plane's pin", out)
        self.assertNotIn("latest", out.lower())

    def test_bump_ahead_of_its_pin_is_still_refused(self):
        self.pin("0.44.0")
        code, out = self.update(bump=True)
        self.assertSaidNothingWasChecked(code, out)

    def test_an_explicit_target_on_the_pin_claims_no_check_it_never_asked_for(self):
        """`--to` never asks PyPI, so a line about what PyPI did not answer would describe
        a request this run did not make."""
        self.pin(INSTALLED)
        code, out = self.update(to=INSTALLED)
        self.assertEqual(code, 0, out)
        self.assertNotIn("could not be checked", out)

    def test_a_plane_behind_its_pin_still_conforms_without_pypi(self):
        """The pin is a target PyPI has no say in, so the refusal waits until the answer
        actually depends on PyPI — conforming to what a teammate already chose must keep
        working with nothing to check against."""
        self.pin("0.45.0")
        code, out = self.update()
        self.assertEqual(code, 0, out)
        self.assertEqual(self.moved, ["0.45.0"])
        self.assertNotIn("no version came back", out)


class APinnedMachineWithAnAnswerFromPyPI(UpdateCase):
    """The other side of both #950 conditions: PyPI DID answer, with nothing newer than
    this machine.

    Only a STRICTLY newer release moves past the pin. Equal is the machine already on the
    newest one, and proposing a bump there asks the team to move to where it already is.
    And a line saying the check could not be made belongs to runs where it was not: here
    it was, so printing it would be the ADR 0013 failure in the other direction.
    """

    def setUp(self):
        super().setUp()
        pt = mock.patch("charter.commands_update._move_harness")
        pt.start(); self.addCleanup(pt.stop)

    def answer(self, latest: str) -> None:
        pt = mock.patch("charter.commands_update._latest", lambda live=True: latest)
        pt.start(); self.addCleanup(pt.stop)

    def test_on_its_pin_with_that_release_newest_nothing_is_proposed(self):
        self.pin(INSTALLED)
        self.answer(INSTALLED)
        code, out = self.update()
        self.assertEqual(code, 0, out)
        self.assertEqual(self.moved, [])
        self.assertNotIn("charter update --bump", out)
        self.assertNotIn("is published", out)

    def test_bump_on_its_pin_with_that_release_newest_moves_nothing(self):
        self.pin(INSTALLED)
        self.answer(INSTALLED)
        with mock.patch("charter.commands_update._bump_pin", return_value=True) as bump:
            code, out = self.update(bump=True)
        self.assertEqual(code, 0, out)
        self.assertEqual(self.moved, [])
        bump.assert_not_called()

    def test_a_check_that_was_made_is_not_reported_as_missing(self):
        for latest in (INSTALLED, "0.44.0"):
            with self.subTest(latest=latest):
                self.pin(INSTALLED)
                self.answer(latest)
                code, out = self.update()
                self.assertEqual(code, 0, out)
                self.assertNotIn("could not be checked", out)
                self.assertNotIn("no version came back", out)


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
