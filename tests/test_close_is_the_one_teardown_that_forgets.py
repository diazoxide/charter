"""§4d/§4i: `chat: close` is quit's teardown with one target and one file more.

**The one file is the whole difference, and it exists because of a measurement.** §2.17:
`kill-pane`, `kill-window` and `kill-session` write no `exit` file, so `state.exit_code`
answers ``None`` for a chat that was killed exactly as it does for one that never started.
This design deliberately reads that ``None`` as *"we do not know it stopped — bring it
back"*, because that is what "less invasive" means. The cost is stated in the design rather
than hidden: a chat stopped outside the quit path would come back uninvited.

`chat: close` is what pays that cost. It writes `state.record_closed`, and a quit skips
whatever carries it. Without that file, closing a chat and then quitting the plane would
bring the closed chat back — and the operator would have no way to make it stop.

**And it drops the transcript, where quit keeps it.** A capture exists to be offered on the
way back; a closed chat is not coming back, so keeping its capture would leave a file in the
frame root that nothing will ever collect (`reopen.prune_transcripts` keeps whatever the
manifest names, and a closed chat is in no manifest).

**What it cannot do is refuse a busy chat**, and that is asserted here as a limit rather than
left to be discovered: `inflight` records carry no fid, no chat and no workspace (§2.15), so
there is no reading charter could refuse on. The confirmation row says the chat will not come
back, which is the sentence an operator needs before pressing it.
"""

from __future__ import annotations

import unittest
from types import SimpleNamespace
from unittest import mock

from charter import commands_frame, config
from charter.frame import leave, reopen, state

from tests._isolation import PersonaIso

SERVER = commands_frame.SOCKET


def _seats(windows, active):
    """`commands_frame._chat_seats`' answer, built from what a case means to say.

    One listing answers three questions — which chats are live, which window each is in, and
    which was its session's current — so a fake for it is one value rather than three
    patches. Written as a helper rather than spelled per case so a case says
    ``{"alpha.1": "@0"}`` and ``{"alpha.1"}`` and nothing about tuple order.
    """
    return [(chat, window, chat in active) for chat, window in windows.items()]


def _plant(fid: str, *, ws: str, pane: str = "%1") -> None:
    state.frame_dir(fid, create=True)
    state.record_server(fid, SERVER)
    state.record_workspace(fid, ws)
    state.record_harness_pane(fid, pane)
    state.record_identity(fid, {"CHARTER_HARNESS": "claude-code",
                                "CHARTER_WORKSPACE": "", "CHARTER_PERSONA": ""})


class ClosingMarksAndQuittingSkipsTheMark(PersonaIso, unittest.TestCase):
    """The pair, asserted as a pair: neither half means anything alone."""

    def test_the_mark_is_written_and_read_by_the_state_module(self):
        _plant("alpha.1", ws="alpha")
        self.assertFalse(state.was_closed("alpha.1"))

        state.record_closed("alpha.1")

        self.assertTrue(state.was_closed("alpha.1"))

    def test_a_closed_chat_is_not_recorded_by_a_later_quit(self):
        _plant("alpha.1", ws="alpha")
        _plant("alpha.2", ws="alpha")

        with mock.patch.multiple(commands_frame,
                                 _chat_seats=mock.DEFAULT,
                                 _capture_transcript=mock.DEFAULT,
                                 _stop_chats=mock.DEFAULT) as m:
            m["_chat_seats"].return_value = _seats({"alpha.1": "@0", "alpha.2": "@1"}, set())
            m["_capture_transcript"].return_value = False
            m["_stop_chats"].return_value = 1
            self.assertEqual(commands_frame.cmd_close(
                SimpleNamespace(chat="alpha.1", chat_id="alpha.2")), 0)
            self.assertEqual(commands_frame.cmd_quit(
                SimpleNamespace(chat="alpha.1")), 0)

        self.assertEqual([c.chat for c in reopen.read().all_chats()], ["alpha.1"])

    def test_without_the_mark_the_closed_chat_would_come_back(self):
        # The guard, verified by removing it: this is the same plane as the case above with
        # `record_closed` patched out, and the closed chat is then recorded as "was open".
        _plant("alpha.1", ws="alpha")
        _plant("alpha.2", ws="alpha")

        with mock.patch.multiple(commands_frame,
                                 _chat_seats=mock.DEFAULT,
                                 _capture_transcript=mock.DEFAULT,
                                 _stop_chats=mock.DEFAULT) as m, \
                mock.patch.object(state, "record_closed"):
            m["_chat_seats"].return_value = _seats({"alpha.1": "@0", "alpha.2": "@1"}, set())
            m["_capture_transcript"].return_value = False
            m["_stop_chats"].return_value = 1
            commands_frame.cmd_close(SimpleNamespace(chat="alpha.1", chat_id="alpha.2"))
            commands_frame.cmd_quit(SimpleNamespace(chat="alpha.1"))

        self.assertEqual(sorted(c.chat for c in reopen.read().all_chats()),
                         ["alpha.1", "alpha.2"])


class CloseDropsWhatQuitKeeps(PersonaIso, unittest.TestCase):
    """The transcript, and the manifest entry if a quit had already written one."""

    def setUp(self):
        super().setUp()
        _plant("alpha.1", ws="alpha")
        _plant("alpha.2", ws="alpha")
        config.private_mkdir(state._root())

    def _close(self, target):
        with mock.patch.multiple(commands_frame,
                                 _chat_seats=mock.DEFAULT,
                                 _stop_chats=mock.DEFAULT) as m:
            m["_chat_seats"].return_value = _seats({"alpha.1": "@0", "alpha.2": "@1"}, set())
            m["_stop_chats"].return_value = 1
            return commands_frame.cmd_close(
                SimpleNamespace(chat="alpha.1", chat_id=target))

    def test_the_closed_chats_transcript_is_dropped(self):
        config.write_for(reopen.transcript_path("alpha.2"), "text\n")

        self.assertEqual(self._close("alpha.2"), 0)

        self.assertFalse(reopen.transcript_path("alpha.2").exists())

    def test_a_siblings_transcript_is_left_alone(self):
        reopen.write([reopen.Frame(workspace="alpha", chats=(
            reopen.Chat(chat="alpha.1", workspace="alpha", persona="",
                        harness="claude-code", cwd="", resume="",
                        transcript="alpha.1.transcript", active=True),))], focus="alpha")
        config.write_for(reopen.transcript_path("alpha.1"), "keep me\n")
        config.write_for(reopen.transcript_path("alpha.2"), "drop me\n")

        self.assertEqual(self._close("alpha.2"), 0)

        self.assertEqual(reopen.transcript_path("alpha.1").read_text(), "keep me\n")
        self.assertFalse(reopen.transcript_path("alpha.2").exists())

    def test_closing_a_chat_a_quit_recorded_takes_it_out_of_the_manifest(self):
        reopen.write([reopen.Frame(workspace="alpha", chats=(
            reopen.Chat(chat="alpha.1", workspace="alpha", persona="", harness="c",
                        cwd="", resume="", transcript="", active=True),
            reopen.Chat(chat="alpha.2", workspace="alpha", persona="", harness="c",
                        cwd="", resume="", transcript="", active=False)))],
            focus="alpha")

        self.assertEqual(self._close("alpha.2"), 0)

        self.assertEqual([c.chat for c in reopen.read().all_chats()], ["alpha.1"])

    def test_a_frame_left_with_no_chats_leaves_no_empty_frame_behind(self):
        reopen.write([reopen.Frame(workspace="alpha", chats=(
            reopen.Chat(chat="alpha.2", workspace="alpha", persona="", harness="c",
                        cwd="", resume="", transcript="", active=True),))], focus="alpha")

        self.assertEqual(self._close("alpha.2"), 0)

        self.assertEqual(reopen.read().frames, ())


class WhatCloseRefusesAndWhatItDoesNot(PersonaIso, unittest.TestCase):
    """A name it cannot use, a chat it cannot find, and a busy chat it cannot judge."""

    def test_a_name_that_cannot_be_a_chat_is_refused_before_any_scan(self):
        with mock.patch.object(commands_frame, "_plane_live") as live:
            rc = commands_frame.cmd_close(
                SimpleNamespace(chat="alpha.1", chat_id="alpha.1;kill-server"))
        self.assertEqual(rc, 1)
        live.assert_not_called()

    def test_an_unknown_chat_is_refused_rather_than_marked(self):
        _plant("alpha.1", ws="alpha")

        with mock.patch.multiple(commands_frame, _chat_seats=mock.DEFAULT) as m:
            m["_chat_seats"].return_value = _seats({"alpha.1": "@0"}, set())
            rc = commands_frame.cmd_close(
                SimpleNamespace(chat="alpha.1", chat_id="beta.4"))

        self.assertEqual(rc, 1)
        self.assertFalse(state.was_closed("beta.4"))

    def test_a_chat_that_has_already_stopped_is_still_marked(self):
        # The case the mark is most needed for: its harness ended on its own, so a quit
        # would otherwise record it as open and bring it back.
        _plant("alpha.1", ws="alpha")
        state.record_exit("alpha.1", 0)

        with mock.patch.multiple(commands_frame, _chat_seats=mock.DEFAULT,
                                 _stop_chats=mock.DEFAULT) as m:
            m["_chat_seats"].return_value = []
            rc = commands_frame.cmd_close(
                SimpleNamespace(chat="alpha.1", chat_id="alpha.1"))
            m["_stop_chats"].assert_not_called()

        self.assertEqual(rc, 0)
        self.assertTrue(state.was_closed("alpha.1"))

    def test_the_bare_command_closes_the_chat_it_was_pressed_in(self):
        _plant("alpha.1", ws="alpha")

        with mock.patch.multiple(commands_frame, _chat_seats=mock.DEFAULT,
                                 _stop_chats=mock.DEFAULT) as m:
            m["_chat_seats"].return_value = _seats({"alpha.1": "@0"}, set())
            m["_stop_chats"].return_value = 1
            self.assertEqual(commands_frame.cmd_close(
                SimpleNamespace(chat="alpha.1", chat_id="")), 0)

        self.assertTrue(state.was_closed("alpha.1"))

    def test_the_plan_it_warns_from_is_the_one_quit_uses_narrowed_to_one_chat(self):
        _plant("alpha.1", ws="alpha")
        _plant("alpha.2", ws="alpha")

        p = leave.plan(live={"alpha.1", "alpha.2"}, focus="alpha", only="alpha.2")

        self.assertEqual([c.chat for c in p.chats], ["alpha.2"])


class TheKillIsAimedAtAWindowTmuxJustNamed(PersonaIso, unittest.TestCase):
    """§3.3: a session NAME in another plane is another plane's session."""

    def test_the_target_is_the_window_id_from_the_listing(self):
        _plant("alpha.1", ws="alpha", pane="%1")
        seen = []

        class _Out:
            returncode = 0
            stdout = ""

        def _run(why, argv, **kw):
            seen.append(argv)
            return _Out()

        doomed = leave.stopping(leave.plan(live={"alpha.1"}, focus="alpha"))
        with mock.patch.object(commands_frame.tmuxctl, "run", side_effect=_run):
            self.assertEqual(
                commands_frame._stop_chats(doomed, windows={SERVER: {"alpha.1": "@3"}}), 1)

        self.assertEqual(len(seen), 1)
        self.assertIn("kill-window", seen[0])
        self.assertIn("@3", seen[0])
        self.assertNotIn("alpha", seen[0], "a session name would be another plane's")
        self.assertNotIn("kill-server", seen[0])
        self.assertNotIn("kill-session", seen[0])

    def test_a_chat_with_no_window_in_the_listing_is_not_aimed_at(self):
        _plant("alpha.1", ws="alpha")
        doomed = leave.stopping(leave.plan(live=None, focus="alpha"))

        with mock.patch.object(commands_frame.tmuxctl, "run") as run:
            self.assertEqual(commands_frame._stop_chats(doomed, windows={}), 0)
            run.assert_not_called()


if __name__ == "__main__":       # pragma: no cover
    unittest.main()
