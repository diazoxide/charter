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
from charter.frame import reopen, state

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


class _Answered:
    """One `tmuxctl.run` answer. `stdout` is a pane id, because that is what the launcher
    reads back from the call that starts the session."""

    def __init__(self, stdout="%0", returncode=0):
        self.stdout = stdout
        self.returncode = returncode


class TheLauncherSTailIsWhereAReopenDiffers(PersonaIso, unittest.TestCase):
    """`cmd_launch` driven all the way to its return, on both paths.

    **Written because the deletion sweep asked for it.** Three branches in that tail are the
    whole of what `Reopening` changes, and every one of them was unpinned: open-or-focus,
    the `attach`, and the sentence `cmd_launch` says on the operator's behalf when their
    plane was quit. `TheReopenPathSuppressesFourThingsInTheLauncher` asserts the PREDICATE
    those branch on and says in its own docstring that the tail "cannot be reached without a
    real tmux session and a real `attach`". The sweep disagreed, and it was right: the tail
    is reachable with every tmux call answered and `attach` stubbed, which is what this does.

    Nothing here starts a server. `tmuxctl.run` answers `%0` to everything and
    `tmuxctl.interact` is the attach — so what is under test is charter's own branching,
    which is exactly what the mutations were about.
    """

    def _launch(self, *, reopening=None, live_workspace=False, rest=(), fresh=False,
                still_live=True):
        """Run `cmd_launch` to its return. Answers what it asked, and records the calls."""
        calls = {"attached": False, "focused": False, "said": []}
        asked: list[int] = []

        def _interact(argv, **kw):
            calls["attached"] = True
            return _Answered(stdout="", returncode=0)

        def _focus(socket, *, ws):
            calls["focused"] = True
            return None            # never actually focus; the call is the observation

        args = SimpleNamespace(harness="claude", rest=list(rest), no_frame=False,
                               workspace="alpha", pick=False, fresh=fresh)
        if reopening is not None:
            args.reopening = reopening
        sessions = {"alpha"} if live_workspace else set()

        def _live_chats(_socket):
            """What a real server says: the chats that exist, asked when it is asked.

            A fixed empty set here is not a cheaper fake, it is a different plane — the
            launcher reaps on this answer twice, and the closing reap would collect the very
            chat it had just built (`state.clear_claim` has already run by then, so nothing
            else is holding it). That is correct behaviour against a server with nothing on
            it, and it is not the server this test means.

            *still_live* is what tells the two asks apart, and a quit is why it exists: the
            launcher asks once on the way IN and once on the way OUT, and a plane that was
            quit while this client was attached answers the second one with nothing. Without
            it, no case here can reach the launcher's own "this chat is over" tail.
            """
            asked.append(1)
            if not still_live and len(asked) > 1:
                return set()
            try:
                return {d.name for d in state._root().iterdir() if d.is_dir()}
            except OSError:
                return set()

        with mock.patch.object(commands_frame.tmuxctl, "version", return_value=(3, 7)), \
                mock.patch.object(commands_frame.tmuxctl, "operator_server",
                                  return_value=None), \
                mock.patch.object(commands_frame.shutil, "which",
                                  return_value="/bin/true"), \
                mock.patch.object(commands_frame.sys.stdout, "isatty",
                                  return_value=True), \
                mock.patch("sys.stdin.isatty", return_value=False), \
                mock.patch.object(commands_frame, "_live_sessions",
                                  return_value=sessions), \
                mock.patch.object(commands_frame, "_live_chats",
                                  side_effect=_live_chats), \
                mock.patch.object(commands_frame, "_spawn_gather"), \
                mock.patch.object(commands_frame, "_workspace_to_focus",
                                  side_effect=_focus), \
                mock.patch.object(commands_frame, "_draw_panels", return_value={}), \
                mock.patch.object(commands_frame, "_arm_panel_respawn"), \
                mock.patch.object(commands_frame, "_query_pane_dead_status",
                                  return_value=None), \
                mock.patch.object(commands_frame, "_chat_being_left", return_value=""), \
                mock.patch.object(commands_frame, "_drop_panels"), \
                mock.patch.object(commands_frame.tmuxctl, "run",
                                  return_value=_Answered()), \
                mock.patch.object(commands_frame.tmuxctl, "interact",
                                  side_effect=_interact):
            calls["rc"] = commands_frame.cmd_launch(args)
        return calls

    def _reopening(self, chat="alpha.9", workspace="alpha"):
        rec = reopen.Chat(chat=chat, workspace=workspace, persona="", harness="claude-code",
                          cwd="", resume="", transcript="", active=False)
        return commands_frame.Reopening(rec)

    def test_an_ordinary_launch_attaches_and_asks_about_open_or_focus(self):
        calls = self._launch()

        self.assertTrue(calls["attached"], "a launch IS the operator's terminal")
        self.assertTrue(calls["focused"], "§4k is asked on every launch with no argv")

    def test_a_reopen_never_attaches(self):
        # The sweep's `if _wants_attach(args):` mutation. Without it a reopen blocks on its
        # first chat and never builds the second.
        calls = self._launch(reopening=self._reopening())

        self.assertFalse(calls["attached"])
        self.assertEqual(calls["rc"], 0)

    def test_a_reopen_never_takes_the_open_or_focus_branch(self):
        # The sweep's `not rest and _reopening(args) is None` mutation. With a live
        # workspace and no argv, an ordinary launch focuses; a reopen must still open a
        # chat, or every chat after the first of a workspace is swallowed.
        ordinary = self._launch(live_workspace=True)
        self.assertTrue(ordinary["focused"])

        reopened = self._launch(reopening=self._reopening(), live_workspace=True)

        self.assertFalse(reopened["focused"])

    def test_a_reopen_hands_the_new_chat_id_back_to_its_driver(self):
        r = self._reopening()

        self._launch(reopening=r)

        self.assertTrue(r.fid, "the driver needs the id to attach and to report")
        self.assertEqual(state.chat_cwd(r.fid), os.getcwd())

    def test_an_ordinary_detach_says_how_to_get_back_in(self):
        import io
        from contextlib import redirect_stderr
        buf = io.StringIO()

        with redirect_stderr(buf):
            self._launch()

        said = buf.getvalue()
        self.assertIn("detached", said)
        self.assertIn("attach -t alpha", said,
                      "the reattach line names the WORKSPACE, because that is the session")

    def test_a_reopens_own_launch_says_nothing_about_detaching(self):
        # It was never the operator's terminal, so "detached — the harness is still running"
        # would be describing something that did not happen — once per chat, in front of
        # `cmd_reopen`'s own summary. The sweep found this branch unpinned because both
        # halves of it return 0 and only the sentence differs.
        import io
        from contextlib import redirect_stderr
        buf = io.StringIO()

        with redirect_stderr(buf):
            self._launch(reopening=self._reopening())

        self.assertNotIn("detached", buf.getvalue())

    def test_a_launch_whose_plane_was_quit_names_the_command_that_undoes_it(self):
        # `if _wants_attach(args): _say_the_plane_is_recorded(fid, over=…)`. The chat this launch is about has
        # to BE in the manifest, which for an ordinary launch means the ordinal it is handed
        # is one a quit recorded — the recycled-ordinal case, which is the common one.
        #
        # **`fresh=True`, and #845 is why the premise had to be said out loud.** Bare
        # `charter` on a plane with nothing live now RESTORES the record instead of opening
        # a chat, so a launch that reaches this tail on a recorded plane is exactly a launch
        # that opted out — which is what `--fresh` is. Without the flag this case tested the
        # restore, and the notice it is about never ran.
        #
        # **`still_live=False` for the other half of the same change.** The record now names
        # running chats too, so the notice is gated on this chat being over — which is what
        # a quit makes true and an ordinary detach does not.
        import io
        from contextlib import redirect_stderr
        reopen.write([reopen.Frame(workspace="alpha", chats=(
            reopen.Chat(chat="alpha.1", workspace="alpha", persona="",
                        harness="claude-code", cwd="", resume="conv-1", transcript="",
                        active=True),))], focus="alpha")
        buf = io.StringIO()

        with redirect_stderr(buf):
            self._launch(fresh=True, still_live=False)

        self.assertIn("charter reopen", buf.getvalue())

    def test_a_reopens_own_launch_stays_silent_about_the_quit_it_is_undoing(self):
        # The other half, and the reason that call is gated: a reopen usually gets the same
        # ordinals back, so every one of its launches is named in the manifest it is acting
        # on and would announce the quit it is in the middle of reversing.
        import io
        from contextlib import redirect_stderr
        reopen.write([reopen.Frame(workspace="alpha", chats=(
            reopen.Chat(chat="alpha.1", workspace="alpha", persona="",
                        harness="claude-code", cwd="", resume="conv-1", transcript="",
                        active=True),))], focus="alpha")
        buf = io.StringIO()

        with redirect_stderr(buf):
            self._launch(reopening=self._reopening())

        self.assertNotIn("charter reopen", buf.getvalue())


if __name__ == "__main__":       # pragma: no cover
    unittest.main()
