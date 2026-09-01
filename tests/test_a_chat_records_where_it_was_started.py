"""§4e: `cwd` is the fourth restore item and it had nowhere to live — now it has a file.

`os.getcwd()` was read on both launch paths, handed to tmux, and dropped. It could not join
`state.record_identity`, and that is a security property rather than a preference: every
value in that record goes onto a tmux ``-e NAME=VALUE`` argv, which
`commands_frame._frame_identity_env` measures as world-readable in `/proc/<pid>/cmdline` —
138 argv elements and 7,696 bytes on a real environment, two live service-account tokens
among them. That list is a PROMISE about what reaches an argv, and a sixth name added for a
convenience is how a promise like that stops being checkable.

**Two files here, and the second is the one that keeps `chat: close` meaningful.**
`state.record_closed` is asserted in `tests/test_close_is_the_one_teardown_that_forgets.py`;
what this module adds is that neither file is in `clear_shape`'s list. That list argues about
four *readings* — a density somebody pressed, a highlight somebody clicked, a pane map, a
gauge — and every one of them is wrong for the next frame. A cwd is where the conversation
was, which is the same kind of fact as `workspace`, and #757 already had to state the same
distinction for `session.durable`.
"""

from __future__ import annotations

import os
import unittest
from types import SimpleNamespace
from unittest import mock

from charter import commands_frame, config
from charter.frame import state

from tests._isolation import PersonaIso


class TheDirectoryIsRecordedAndReadBack(PersonaIso, unittest.TestCase):

    def test_it_round_trips(self):
        state.frame_dir("alpha.1", create=True)

        state.record_cwd("alpha.1", "/some/where")

        self.assertEqual(state.chat_cwd("alpha.1"), "/some/where")

    def test_a_chat_that_never_recorded_one_answers_none(self):
        state.frame_dir("alpha.1", create=True)

        self.assertIsNone(state.chat_cwd("alpha.1"))

    def test_a_relative_path_is_refused_rather_than_resolved_later(self):
        # It would otherwise be resolved against whatever directory `charter reopen`
        # happened to be typed in, which is the "silently somewhere else" a restore must
        # never do — and it is indistinguishable on disk from a truncated write.
        state.frame_dir("alpha.1", create=True)

        state.record_cwd("alpha.1", "some/where")

        self.assertIsNone(state.chat_cwd("alpha.1"))

    def test_a_directory_that_no_longer_exists_is_still_returned(self):
        # Existence is a question about the machine at reopen time, not about the record,
        # and the caller is the one with an operator to tell.
        state.frame_dir("alpha.1", create=True)

        state.record_cwd("alpha.1", "/definitely/not/here")

        self.assertEqual(state.chat_cwd("alpha.1"), "/definitely/not/here")

    def test_a_frame_id_that_cannot_name_a_directory_answers_none(self):
        self.assertIsNone(state.chat_cwd("../escape"))
        # And writing is refused the same way, rather than landing somewhere else.
        state.record_cwd("../escape", "/tmp")
        self.assertIsNone(state.chat_cwd("../escape"))


class NeitherFileIsPartOfAFramesShape(PersonaIso, unittest.TestCase):
    """`clear_shape` argues about readings; these two are durable per-chat facts."""

    def test_clear_shape_keeps_the_directory_and_the_closed_mark(self):
        state.frame_dir("alpha.1", create=True)
        state.record_cwd("alpha.1", "/some/where")
        state.record_closed("alpha.1")
        state.record_density("alpha.1", "minimal")

        state.clear_shape("alpha.1")

        self.assertIsNone(state.density("alpha.1"), "the shape did go")
        self.assertEqual(state.chat_cwd("alpha.1"), "/some/where")
        self.assertTrue(state.was_closed("alpha.1"))


class BothLaunchPathsRecordIt(PersonaIso, unittest.TestCase):
    """Recorded where `os.getcwd()` was already read, on both paths, or it is half a fact.

    Driven through `cmd_launch` far enough to reach the record and no further: the launcher
    is stopped at its first tmux call, which is after every one of its own writes. That is
    the same shape `tests/test_frame_launcher.py` uses to assert what a launch records
    without starting a server.
    """

    def _run_launch(self, harness_absent=False):
        calls = {}

        class _Out:
            returncode = 1
            stdout = ""

        def _run(why, argv, **kw):
            calls.setdefault("first", why)
            return _Out()

        args = SimpleNamespace(harness="claude", rest=[], no_frame=False,
                               workspace="alpha", pick=False)
        with mock.patch.object(commands_frame.tmuxctl, "version", return_value=(3, 7)), \
                mock.patch.object(commands_frame.tmuxctl, "operator_server",
                                  return_value=None), \
                mock.patch.object(commands_frame.shutil, "which", return_value="/bin/true"), \
                mock.patch.object(commands_frame.sys.stdout, "isatty",
                                  return_value=True), \
                mock.patch("sys.stdin.isatty", return_value=False), \
                mock.patch.object(commands_frame, "_live_sessions", return_value=set()), \
                mock.patch.object(commands_frame, "_live_chats", return_value=set()), \
                mock.patch.object(commands_frame, "_spawn_gather"), \
                mock.patch.object(commands_frame.tmuxctl, "run", side_effect=_run):
            commands_frame.cmd_launch(args)
        return calls

    def test_the_private_server_path_records_the_directory_it_launched_in(self):
        self._run_launch()

        recorded = [state.chat_cwd(d.name) for d in state._root().iterdir()
                    if d.is_dir()]
        self.assertEqual(recorded, [os.getcwd()])

    def test_the_operators_tmux_path_records_it_too(self):
        class _Out:
            returncode = 1
            stdout = ""

        args = SimpleNamespace(harness="claude", rest=[], no_frame=False,
                               workspace="alpha", pick=False)
        with mock.patch.object(commands_frame.tmuxctl, "version", return_value=(3, 7)), \
                mock.patch.object(commands_frame.tmuxctl, "operator_server",
                                  return_value=("/tmp/sock", "s0")), \
                mock.patch.object(commands_frame.shutil, "which",
                                  return_value="/bin/true"), \
                mock.patch.object(commands_frame.sys.stdout, "isatty",
                                  return_value=True), \
                mock.patch("sys.stdin.isatty", return_value=False), \
                mock.patch.object(commands_frame, "_live_windows", return_value=set()), \
                mock.patch.object(commands_frame, "_live_chats", return_value=set()), \
                mock.patch.object(commands_frame, "_spawn_gather"), \
                mock.patch.object(commands_frame.tmuxctl, "run",
                                  return_value=_Out()):
            commands_frame.cmd_launch(args)

        recorded = [state.chat_cwd(d.name) for d in state._root().iterdir()
                    if d.is_dir()]
        self.assertEqual(recorded, [os.getcwd()])


if __name__ == "__main__":       # pragma: no cover
    unittest.main()
