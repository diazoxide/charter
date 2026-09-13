"""#1018: a dev-channel plane that already carries a pin was still sent to the pin.

A plane that declares ``[update] channel = "dev"`` AND pins ``[charter] version`` asks for
two different charters. #947 stopped `charter version bump` WRITING that pair, but a plane
can already hold it: a pin from before that fix, a hand edit, an older charter. Three
surfaces still read it three ways:

* **`charter version sync` conformed the machine to the pin**, installing a PyPI release
  onto a plane that follows `main`. It now refuses before anything moves.
* **`charter version`, with the pin unequal to the install, said `conform this machine:
  charter version sync`**, which is the command above. With the two equal it said "in sync
  with the lock". It now names the conflict, and exits 1 as drift does.
* **Session start refused only when the pin differed from the running version.** A dev
  build carries the version of the wheel it was built from, so a pin equal to that number
  raised nothing, and the plane carried a contradiction the next release would surface.

All of them now print `update.pin_beside_dev()`, the sentence `version bump` prints, so the
four cannot describe one plane four ways.

Nothing here reaches the network or installs anything: `NoNetwork` refuses every route out,
`commands.sync_to` is a recorder, and the harness `version sync` would ask is a recorder too.
"""

from __future__ import annotations

import io
from contextlib import redirect_stderr, redirect_stdout
from types import SimpleNamespace
from unittest import mock

from charter import __version__, commands, config, hooks, update
from tests._isolation import PersonaIso
from tests.test_dev_channel import NoNetwork

#: Unequal to any charter that can be running this suite, in the direction session start
#: would install unattended. The exact number is not the point.
_AHEAD = "99.0.0"


def _run(fn, args) -> tuple[int, str]:
    out, err = io.StringIO(), io.StringIO()
    with redirect_stdout(out), redirect_stderr(err):
        rc = fn(args)
    return rc, out.getvalue() + err.getvalue()


class _DevPlaneWithAPin(NoNetwork, PersonaIso):
    """The state itself, written into the fixture plane's `charter.toml` the way a teammate's
    commit would leave it, rather than patched into `config`: every surface below re-reads
    the pin from that file, and `config.use` derives the channel from the same file."""

    def setUp(self):
        super().setUp()
        self.moved: list[tuple] = []

        def sync_to(v):
            self.moved.append(("sync_to", v))
            return True, v

        class _Harness:
            name = "recorder"

            def upgrade(_, root):
                self.moved.append(("upgrade", str(root)))
                return "moved", "recorded"

        self.enterContext(mock.patch("charter.commands.sync_to", side_effect=sync_to))
        self.enterContext(mock.patch("charter.harness.get", return_value=_Harness()))

    def _declare(self, pin: str, channel: str = "dev") -> None:
        (self.tmp / "charter.toml").write_text(
            f'schema = 1\n[charter]\nversion = "{pin}"\n[update]\nchannel = "{channel}"\n')
        config.use(self.tmp)

    def assertNamesTheConflictAndBothWaysOut(self, said: str) -> None:
        self.assertIn("two different charters", said)
        self.assertIn('`[update] channel = "dev"`', said)
        # Both ways out: drop the channel, or keep no pin and follow `main`.
        self.assertIn("drop", said)
        self.assertIn(update.dev_remedy(), said)


class VersionSyncRefusesADevPlaneWithAPin(_DevPlaneWithAPin):
    def test_the_machine_is_not_conformed_to_the_pin(self):
        """The reported case: `--cli` installed the pinned release over a plane on `main`."""
        self._declare(_AHEAD)
        rc, said = _run(commands.cmd_version_sync, SimpleNamespace(cli=True))
        self.assertEqual(self.moved, [], "sync moved a charter on a plane it should refuse")
        self.assertEqual(rc, 1, said)
        self.assertNamesTheConflictAndBothWaysOut(said)
        self.assertNotIn("installed charter", said)

    def test_this_plane_s_artifact_is_not_moved_either(self):
        """Without `--cli`, sync asks the harness to move this plane's artifact and says it
        follows the pin. Refused before that question too: the pin is what is in dispute."""
        self._declare(_AHEAD)
        rc, said = _run(commands.cmd_version_sync, SimpleNamespace(cli=False))
        self.assertEqual(self.moved, [])
        self.assertEqual(rc, 1, said)
        self.assertNamesTheConflictAndBothWaysOut(said)

    def test_a_pin_equal_to_the_running_version_is_refused_the_same_way(self):
        """"already on the locked version" is true of a number and says nothing about which
        charter it is: a dev build and the release it was built from print the same one."""
        self._declare(__version__)
        rc, said = _run(commands.cmd_version_sync, SimpleNamespace(cli=True))
        self.assertEqual(rc, 1, said)
        self.assertNotIn("already on the locked version", said)
        self.assertNamesTheConflictAndBothWaysOut(said)

    def test_a_stable_plane_with_a_pin_still_conforms(self):
        """The other side of the gate, or no plane could honour a pin at all."""
        self._declare(_AHEAD, channel="stable")
        rc, said = _run(commands.cmd_version_sync, SimpleNamespace(cli=True))
        self.assertEqual(rc, 0, said)
        self.assertEqual(self.moved, [("sync_to", _AHEAD)])


class VersionNamesTheConflictInsteadOfASync(_DevPlaneWithAPin):
    def _version(self) -> tuple[int, str]:
        rc, said = _run(commands.cmd_version, SimpleNamespace())
        self.assertEqual(self.moved, [], "`charter version` only reports")
        return rc, said

    def test_a_pin_unequal_to_the_install_is_not_sent_to_version_sync(self):
        """The reported case: drift, and `conform this machine: charter version sync`."""
        self._declare(_AHEAD)
        rc, said = self._version()
        self.assertNotIn("version sync", said)
        self.assertNamesTheConflictAndBothWaysOut(said)
        # A refused state exits the way drift does, so a script reading the code sees the
        # plane is not in the state it asked for.
        self.assertEqual(rc, 1, said)

    def test_a_pin_equal_to_the_install_is_not_called_in_sync(self):
        """"in sync with the lock" compared two numbers, and a dev build prints the number of
        the release it was built from. The plane is in the same refused state either way."""
        self._declare(__version__)
        rc, said = self._version()
        self.assertNotIn("in sync", said)
        self.assertNamesTheConflictAndBothWaysOut(said)
        self.assertEqual(rc, 1, said)

    def test_a_stable_plane_s_drift_still_names_version_sync(self):
        self._declare(_AHEAD, channel="stable")
        rc, said = self._version()
        self.assertEqual(rc, 1, said)
        self.assertIn("conform this machine:  charter version sync", said)
        self.assertNotIn("two different charters", said)


class SessionStartSaysTheConflictWhateverTheNumbers(_DevPlaneWithAPin):
    """`hooks._autosync_version_lock`, the one surface nobody types.

    It used to return before its dev check whenever the pin equalled the running version.
    `test_dev_update_command` pinned that silence as benign: equal numbers meant nothing to
    install and nothing to undo. What it left was a plane carrying a contradiction that
    `charter version` called "in sync" and that surfaced only when a teammate bumped the
    pin, as a refusal about a `charter.toml` edit nobody connected to it.
    """

    def test_a_pin_equal_to_the_running_version_is_named_and_nothing_installs(self):
        self._declare(__version__)
        msg = hooks._autosync_version_lock()
        self.assertEqual(self.moved, [])
        self.assertIsNotNone(msg, "session start said nothing about a plane it refuses")
        self.assertIn("nothing was installed", msg)
        self.assertNamesTheConflictAndBothWaysOut(msg)

    def test_a_pin_ahead_of_the_running_version_still_installs_nothing(self):
        """The return to stable the check exists for: this pin is the direction session
        start installs unattended, and on a dev plane that is a PyPI wheel over `main`."""
        self._declare(_AHEAD)
        msg = hooks._autosync_version_lock()
        self.assertEqual(self.moved, [], "session start settled the conflict by installing")
        self.assertIn(_AHEAD, msg)
        self.assertNamesTheConflictAndBothWaysOut(msg)

    def test_a_dev_plane_with_no_pin_is_still_silent(self):
        (self.tmp / "charter.toml").write_text('schema = 1\n[update]\nchannel = "dev"\n')
        config.use(self.tmp)
        self.assertIsNone(hooks._autosync_version_lock())
        self.assertEqual(self.moved, [])


class EverySurfaceSaysTheSameThing(_DevPlaneWithAPin):
    """One state, one sentence, from one function — `update.pin_beside_dev`.

    The substring checks above would pass if each surface kept its own wording of the two
    ways out, and four wordings of one refusal is how the issue began: session start said
    "drop one of the two", `version bump` said "drop the channel and bump again, or pin
    nothing", and `version sync` and `charter version` said nothing about it at all.
    """

    def test_all_four_print_the_conflict_and_both_ways_out_verbatim(self):
        self._declare(_AHEAD)
        for inside in (False, True):
            with self.subTest(running_inside=inside), \
                 mock.patch("charter.channel.running_inside", return_value=inside):
                shared = update.pin_beside_dev()
                # Not vacuous: the remedy inside the sentence is decided at the call, so a
                # surface that cached the sentence would print the wrong next step here.
                self.assertEqual(inside, "pull" in shared[2])
                with mock.patch("charter.update.fetch_and_store", return_value=_AHEAD):
                    surfaces = {
                        "version sync": _run(commands.cmd_version_sync,
                                             SimpleNamespace(cli=True))[1],
                        "charter version": _run(commands.cmd_version, SimpleNamespace())[1],
                        "version bump": _run(commands.cmd_version_bump,
                                             SimpleNamespace(to=None, push=True))[1],
                        "session start": hooks._autosync_version_lock(),
                    }
                for surface, said in surfaces.items():
                    for line in shared:
                        self.assertIn(line, said, f"{surface} does not say it")
        self.assertEqual(self.moved, [])


if __name__ == "__main__":
    import unittest
    unittest.main()
