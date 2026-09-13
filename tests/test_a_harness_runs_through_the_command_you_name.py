"""A harness profile runs through the command YOU launch it with — `ccs work`, a wrapper, a
binary kept off `$PATH`.

An operator who reaches Claude Code through a wrapper — `ccs work`, which picks the
subscription a chat is billed to; a script that pins a binary off `$PATH` — had two doors,
both wrong. `charter frame -- ccs work` runs the words but forgets the harness: no
`$CHARTER_HARNESS` in the pane, no workspace layer, nothing `charter reopen` can resume. A
shell alias is invisible to charter, which execs a program by name. A profile in
`charter.local.toml` is the third door: the REGISTERED harness, launched through the
operator's own words, with everything the registration carries.

    [harness.claude-ccs]
    kind = "claude"
    command = ["ccs", "work"]

**Where this came from.** PR #968 proposed exactly this door as a per-developer
`.charter/local.toml` naming one command per harness; the harness-profiles work (ADR 0022)
reached the same place with a per-machine file, several profiles per kind, an approval
before a new command runs and a wiring check. What that PR specified and profiles had not
yet pinned is here:

* **the command's words come first and the operator's arguments follow them**, on a bare
  launch and in the pane — so `--resume <id>` on a reopen reaches whatever the wrapper
  forwards it to. Nothing failed if the launcher kept only a command's first word;
* **the program that is checked is the program that runs** — `ccs` missing is refused
  before tmux and named, and `claude` missing does not stop a `ccs` that is there: the
  wrapper is what runs, so the wrapper is what has to be findable;
* **a file that is not UTF-8 is a refusal with a sentence, never a traceback**, and the
  built-ins still load.
"""

from __future__ import annotations

import io
import unittest
from contextlib import redirect_stderr
from unittest import mock

from charter import config, profiles
from charter.frame import launcher
from tests._isolation import approve_every_profile, wired_as_today
from tests.test_a_profile_launch_is_refused_before_tmux import _ALaunchNamesAProfile

#: A wrapper, one word of its own after the program.
WRAPPED = '[harness.claude-ccs]\nkind = "claude"\ncommand = ["ccs", "work"]\n'

#: Ruling 10 refuses an unwired profile before tmux, and no case here is about wiring
#: (`tests/test_a_profile_launch_is_refused_before_tmux.py` records the same choice).
_WIRED = None


def setUpModule():
    global _WIRED
    _WIRED = wired_as_today()
    _WIRED.start()


def tearDownModule():
    if _WIRED is not None:
        _WIRED.stop()


def _only(missing: str):
    """A `shutil.which` that finds every program but *missing*."""
    return lambda name, *a, **kw: None if name == missing else f"/usr/bin/{name}"


class _AWrappedProfile(_ALaunchNamesAProfile):
    def setUp(self) -> None:
        super().setUp()
        self.local.write_text(WRAPPED)
        approve_every_profile(self)


class TheCommandsWordsComeFirstAndYourArgumentsFollow(_AWrappedProfile, unittest.TestCase):

    def test_a_bare_launch_execs_every_word_of_the_command_then_the_operators(self):
        self.assertEqual(self._launch(profile="claude-ccs", no_frame=True,
                                      rest=["-p", "hi"]), 0)
        (program, argv, _env), = self.execs
        self.assertEqual((program, argv), ("ccs", ["ccs", "work", "-p", "hi"]))

    def test_the_pane_execs_every_word_of_the_command_then_the_resume_it_was_handed(self):
        """What a reopen hands the pane is `--resume <id>`; a wrapper that forwards its
        arguments passes it on, and one that dropped `work` would start the wrong account."""
        p = profiles.current().profiles["claude-ccs"]
        with mock.patch.object(launcher.shutil, "which", side_effect=_only("")):
            self.assertEqual(launcher.start(p, ["--resume", "abc"], fid=None,
                                            attended=False), 0)
        (program, argv, _env), = self.execs
        self.assertEqual((program, argv), ("ccs", ["ccs", "work", "--resume", "abc"]))

    def test_the_wrapper_is_still_the_harness_it_wraps(self):
        """The whole point of the door: `charter frame -- ccs work` forgot which harness it
        was, and a profile does not."""
        self._launch(profile="claude-ccs", no_frame=True)
        (_program, _argv, env), = self.execs
        self.assertEqual(env["CHARTER_HARNESS"], "claude-code")
        self.assertEqual(env["CHARTER_HARNESS_PROFILE"], "claude-ccs")


class TheProgramCheckedIsTheProgramThatRuns(_AWrappedProfile, unittest.TestCase):

    def test_a_missing_wrapper_is_refused_before_tmux_and_named(self):
        err = io.StringIO()
        with redirect_stderr(err):
            rc = self._launch(profile="claude-ccs", which=_only("ccs"))
        self.assertEqual(rc, launcher.MISSING_EXIT)
        self.assertIn("ccs", err.getvalue())
        self.assertFalse(self._started(), "refused AFTER tmux — nothing was drawn")
        self.assertEqual(self.execs, [])

    def test_a_missing_harness_binary_does_not_stop_a_wrapper_that_is_there(self):
        self.assertEqual(self._launch(profile="claude-ccs", which=_only("claude")), 0)
        self.assertTrue(self._started(), self.argvs)


class AFileThatIsNotUtf8IsRefusedWithASentence(_ALaunchNamesAProfile, unittest.TestCase):

    def test_the_file_is_refused_and_the_built_ins_still_load(self):
        (config.ROOT / profiles.LOCAL_FILE).write_bytes(
            b'[harness.claude-ccs]\nkind = "claude"\ncommand = ["c\xffcs"]\n')
        read = profiles.current()
        self.assertNotIn("claude-ccs", read.profiles)
        self.assertIn("claude", read.profiles)
        why = [r for r in read.refused if r.source == profiles.LOCAL_FILE and not r.name]
        self.assertEqual(len(why), 1, read.refused)
        self.assertIn("could not be read", why[0].reason)


if __name__ == "__main__":
    unittest.main()
