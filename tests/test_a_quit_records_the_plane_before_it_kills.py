"""§4e/§4i: a quit writes the plane down BEFORE it stops anything, in a file `reap` cannot
take.

Three claims, and each one is load-bearing rather than a nicety.

**1. The record survives everything that deletes a chat.** `state.reap` skips anything that
is not a directory, so a plain file in the frame root outlives every reap — and it has to,
because the thing #757 kept (`session.durable`, the id a resume needs) lives INSIDE the
chat's directory and `reap` deletes that whole directory when the launcher pid is dead.
After a restart every launcher pid is dead, so the id #757 kept is gone by the time a reopen
could ask for it. `TheRecordOutlivesTheChatItDescribes` is that fact as a test, driven
through the real `reap` with the real keep-rules rather than by asserting that a file exists.

**2. The record comes first.** `_stop_chats` is patched to assert the manifest is already on
disk when it is called, which is the same rule `trace._trace_secret_use` keeps for the same
reason: a record that depends on the thing it records succeeding is not a record. The
inverse is asserted too — a manifest that could not be written REFUSES the quit rather than
killing a plane nothing would bring back.

**3. Nothing means "was open".** `state.exit_code` answers ``None`` for a chat that never
started, for one the machine took down, and for one `kill-window` ended — §2.17, measured —
so a quit that read "no exit file" as "already over" would silently drop the ordinary case.
The one thing that opts out is `state.was_closed`, which is what `chat: close` writes.

**No tmux here, deliberately.** Everything above reproduces on directories: liveness enters
`leave.plan` as an argument, and the teardown is the one part that needs a server. The real
thing is exercised in `tests/test_quit_and_reopen_on_a_real_tmux.py`.

`PersonaIso` on every case, and `os.environ` is already cleared by it: `state.own_workspace`
and `state.workspace_for` both read `$CHARTER_WORKSPACE` and `$CHARTER_SESSION_ID`, so a
developer running the suite inside a live frame would otherwise be supplying half of every
fixture (#519/#521/#528).
"""

from __future__ import annotations

import json

import unittest
from types import SimpleNamespace
from unittest import mock

from charter import commands_frame, config, inflight
from charter.frame import chats, leave, reopen, state

from tests._isolation import PersonaIso

SERVER = commands_frame.SOCKET


def _plant(fid: str, *, ws: str, harness: str = "claude-code", pane: str = "%1",
           sid: str = "", cwd: str = "", persona: str = "") -> None:
    """Make *fid* look like a chat charter launched — through the production writers.

    Never a hand-written file: `record_workspace` is what `frame_workspace` reads back, so a
    fixture that stopped agreeing with the launcher fails here rather than passing against
    itself. That is `tests/test_frame_chat_switch._plant`'s rule and this is the same one.
    """
    state.frame_dir(fid, create=True)
    state.record_server(fid, SERVER)
    state.record_workspace(fid, ws)
    state.record_harness_pane(fid, pane)
    state.record_identity(fid, {"CHARTER_HARNESS": harness, "CHARTER_WORKSPACE": "",
                                "CHARTER_PERSONA": persona})
    if sid:
        state.record_harness_session(fid, sid)
    if cwd:
        state.record_cwd(fid, cwd)


class TheRecordOutlivesTheChatItDescribes(PersonaIso, unittest.TestCase):
    """The manifest is a FILE in the frame root, and that is the whole durability story.

    **Measured while writing this: the property is over-determined in `reap`, and no single
    mutation can take it away.** Three independent things keep a non-directory entry there —
    the `if not d.is_dir()` filter, the `except OSError: continue` keep-rule (a
    `NotADirectoryError` out of `d.iterdir()` IS an `OSError`), and `shutil.rmtree` with
    `ignore_errors=True`, which cannot remove a file even if it were reached. Removing the
    `is_dir()` half alone leaves these tests green. That is recorded rather than papered
    over: the assertions below are about the OUTCOME the design depends on, and the
    last-line case pins it against the hostile shape a chat-named file is.
    """

    def test_reap_never_removes_a_non_directory_from_the_frame_root(self):
        # The property, stated on its own and against the shape that would be a chat if it
        # were a directory. Nothing in `.charter/frame/` that is not a directory is `reap`'s
        # to collect — which is what makes the manifest and the transcripts durable, and
        # which `chats.of_workspace` relies on from the other side.
        config.private_mkdir(state._root())
        for name in (reopen.MANIFEST, "alpha.1.transcript", "alpha.2"):
            config.write_for(state._root() / name, "not a chat\n")

        self.assertEqual(state.reap(set(), server=SERVER), [])

        for name in (reopen.MANIFEST, "alpha.1.transcript", "alpha.2"):
            self.assertTrue((state._root() / name).is_file(), name)

    def test_reap_takes_the_chat_directory_and_leaves_the_manifest(self):
        _plant("alpha.1", ws="alpha", sid="conv-1")
        # The claim marker names THIS process, which `reap` keeps a directory for — so it
        # goes, exactly as it does at the end of a real launch (`state.clear_claim`).
        # Without this the reap below would keep the directory for the right reason and
        # prove nothing about the manifest.
        state.clear_claim("alpha.1")
        self.assertTrue(reopen.write(
            [reopen.Frame(workspace="alpha", chats=(reopen.Chat(
                chat="alpha.1", workspace="alpha", persona="", harness="claude-code",
                cwd=str(config.ROOT), resume="conv-1", transcript="", active=True),))],
            focus="alpha"))
        # And the thing #757 kept is in the directory that is about to go.
        self.assertEqual(state.kept_harness_session("alpha.1"), "conv-1")

        removed = state.reap(set(), server=SERVER)

        self.assertEqual(removed, ["alpha.1"])
        self.assertIsNone(state.kept_harness_session("alpha.1"),
                          "reap took session.durable with the directory — which is why "
                          "the manifest has to hold the id too")
        m = reopen.read()
        self.assertIsNotNone(m)
        self.assertEqual([c.resume for c in m.all_chats()], ["conv-1"])

    def test_a_transcript_is_a_file_in_the_frame_root_and_survives_too(self):
        _plant("alpha.1", ws="alpha")
        state.clear_claim("alpha.1")
        path = reopen.transcript_path("alpha.1")
        config.write_for(path, "what was on screen\n")

        state.reap(set(), server=SERVER)

        self.assertTrue(path.is_file())

    def test_neither_file_is_ever_mistaken_for_a_chat(self):
        _plant("alpha.1", ws="alpha")
        reopen.write([], focus="alpha")
        config.write_for(reopen.transcript_path("alpha.1"), "text\n")

        # Both scans filter on `is_dir()`, and this is the assertion that says so rather
        # than the comment that claims it.
        self.assertEqual(chats.of_workspace("alpha"), ["alpha.1"])
        self.assertEqual(leave.plane_chats(), ["alpha.1"])


class WhatAQuitWritesDown(PersonaIso, unittest.TestCase):
    """The four restore items, the resume id, and who is left out."""

    def test_the_four_restore_items_are_all_recorded(self):
        _plant("alpha.1", ws="alpha", sid="conv-1", cwd=str(config.ROOT),
               persona="")
        from charter import persona as persona_mod
        persona_mod.set_active("steward", session_id="alpha.1", terminal_id="")

        p = leave.plan(live={"alpha.1"}, focus="alpha")

        self.assertEqual(len(p.chats), 1)
        c = p.chats[0]
        self.assertEqual(c.workspace, "alpha")
        self.assertEqual(c.persona, "steward")
        self.assertEqual(c.harness, "claude-code")
        self.assertEqual(c.cwd, str(config.ROOT))
        self.assertEqual(c.resume, "conv-1")

    def test_a_chat_with_no_exit_file_is_recorded_because_nothing_means_was_open(self):
        _plant("alpha.1", ws="alpha")
        self.assertIsNone(state.exit_code("alpha.1"),
                          "the premise: kill-window writes no exit (§2.17)")

        p = leave.plan(live={"alpha.1"}, focus="alpha")

        self.assertEqual([c.chat for c in p.chats], ["alpha.1"])

    def test_a_closed_chat_is_the_one_thing_left_out(self):
        _plant("alpha.1", ws="alpha")
        _plant("alpha.2", ws="alpha")
        state.record_closed("alpha.2")

        p = leave.plan(live={"alpha.1", "alpha.2"}, focus="alpha")

        self.assertEqual([c.chat for c in p.chats], ["alpha.1"])

    def test_a_chat_whose_workspace_record_is_gone_is_still_recorded(self):
        # `chats.of_workspace` cannot see this chat at all — it says it is in no
        # workspace — which is exactly why the quit scans the frame root itself.
        state.frame_dir("alpha.9", create=True)
        state.record_server("alpha.9", SERVER)
        state.record_harness_pane("alpha.9", "%9")

        p = leave.plan(live={"alpha.9"}, focus="")

        self.assertEqual([c.chat for c in p.chats], ["alpha.9"])
        self.assertEqual(p.chats[0].workspace, "")

    def test_a_chat_with_no_workspace_is_stopped_and_honestly_not_recorded(self):
        # Recorded would be a lie: `reopen.read` holds a manifest entry's workspace to
        # `valid_name`, so a line written for a chat that says nothing would be discarded by
        # every reader — a record that looks like a promise and is not. The warning names it
        # (`leave.NOT_REOPENED`) and the quit's own line says how many of how many were kept.
        _plant("alpha.1", ws="alpha")
        state.frame_dir("alpha.9", create=True)
        state.record_server("alpha.9", SERVER)
        state.record_harness_pane("alpha.9", "%9")

        with mock.patch.multiple(commands_frame,
                                 _chat_windows=mock.DEFAULT, _active_chats=mock.DEFAULT,
                                 _capture_transcript=mock.DEFAULT,
                                 _stop_chats=mock.DEFAULT) as m:
            m["_chat_windows"].return_value = {"alpha.1": "@0", "alpha.9": "@1"}
            m["_active_chats"].return_value = set()
            m["_capture_transcript"].return_value = False
            m["_stop_chats"].return_value = 2
            self.assertEqual(commands_frame.cmd_quit(
                SimpleNamespace(chat="alpha.1")), 0)
            # Both were aimed at — it is the RECORD that leaves one out, not the teardown.
            self.assertEqual(
                sorted(c.chat for c in m["_stop_chats"].call_args[0][0]),
                ["alpha.1", "alpha.9"])

        self.assertEqual([c.chat for c in reopen.read().all_chats()], ["alpha.1"])

    def test_a_wedged_server_is_not_an_empty_one(self):
        _plant("alpha.1", ws="alpha")

        # `None` is `_live_chats`' third answer: charter could not ask. Every chat is then
        # attempted rather than skipped, because skipping one that IS running would leave a
        # harness alive behind a manifest saying it was stopped.
        p = leave.plan(live=None, focus="alpha")

        self.assertIsNone(p.chats[0].live)
        self.assertEqual([c.chat for c in leave.stopping(p)], ["alpha.1"])

    def test_a_chat_tmux_says_is_gone_is_recorded_but_not_stopped(self):
        _plant("alpha.1", ws="alpha")

        p = leave.plan(live=set(), focus="alpha")

        self.assertEqual([c.chat for c in p.chats], ["alpha.1"])
        self.assertEqual(leave.stopping(p), ())


class TheOrderIsRecordThenKill(PersonaIso, unittest.TestCase):
    """§4e's ordering, asserted from inside the teardown rather than around it."""

    def setUp(self):
        super().setUp()
        _plant("alpha.1", ws="alpha", sid="conv-1", cwd=str(config.ROOT))
        self.args = SimpleNamespace(chat="alpha.1")

    def _no_tmux(self):
        """Answer every tmux question the way a server with this one chat on it would.

        Patched at `_chat_windows`/`_active_chats` rather than at `tmuxctl.run`, because
        what these tests are about is the ORDER of charter's own writes — and a fake at the
        subprocess boundary would make every one of them depend on a format string.
        """
        return mock.patch.multiple(
            commands_frame,
            _chat_windows=mock.DEFAULT, _active_chats=mock.DEFAULT,
            _capture_transcript=mock.DEFAULT, _stop_chats=mock.DEFAULT)

    def test_the_manifest_is_on_disk_before_anything_is_stopped(self):
        seen = {}

        def _stop(doomed, *, windows):
            seen["manifest"] = reopen.read()
            return len(doomed)

        with self._no_tmux() as m:
            m["_chat_windows"].return_value = {"alpha.1": "@0"}
            m["_active_chats"].return_value = {"alpha.1"}
            m["_capture_transcript"].return_value = False
            m["_stop_chats"].side_effect = _stop
            self.assertEqual(commands_frame.cmd_quit(self.args), 0)

        self.assertIsNotNone(seen["manifest"], "the kill ran before the record landed")
        self.assertEqual([c.chat for c in seen["manifest"].all_chats()], ["alpha.1"])

    def test_a_record_that_will_not_land_refuses_the_quit_and_kills_nothing(self):
        with self._no_tmux() as m, mock.patch.object(reopen, "write", return_value=False):
            m["_chat_windows"].return_value = {"alpha.1": "@0"}
            m["_active_chats"].return_value = set()
            m["_capture_transcript"].return_value = False
            self.assertEqual(commands_frame.cmd_quit(self.args), 1)
            m["_stop_chats"].assert_not_called()

    def test_the_focus_is_the_workspace_the_quit_was_pressed_in(self):
        _plant("beta.1", ws="beta")
        with self._no_tmux() as m:
            m["_chat_windows"].return_value = {"alpha.1": "@0", "beta.1": "@1"}
            m["_active_chats"].return_value = set()
            m["_capture_transcript"].return_value = False
            m["_stop_chats"].return_value = 2
            commands_frame.cmd_quit(SimpleNamespace(chat="beta.1"))

        self.assertEqual(reopen.read().focus, "beta")

    def test_the_chats_are_grouped_by_workspace_one_frame_each(self):
        _plant("alpha.2", ws="alpha")
        _plant("beta.1", ws="beta")
        with self._no_tmux() as m:
            m["_chat_windows"].return_value = {"alpha.1": "@0", "alpha.2": "@1",
                                               "beta.1": "@2"}
            m["_active_chats"].return_value = set()
            m["_capture_transcript"].return_value = False
            m["_stop_chats"].return_value = 3
            commands_frame.cmd_quit(self.args)

        m = reopen.read()
        self.assertEqual([(f.workspace, [c.chat for c in f.chats]) for f in m.frames],
                         [("alpha", ["alpha.1", "alpha.2"]), ("beta", ["beta.1"])])

    def test_nothing_open_is_not_a_quit(self):
        state.reap({"nothing"}, server=SERVER)
        for d in state._root().iterdir():
            if d.is_dir():
                import shutil
                shutil.rmtree(d)
        with self._no_tmux() as m:
            m["_chat_windows"].return_value = {}
            m["_active_chats"].return_value = set()
            m["_capture_transcript"].return_value = False
            self.assertEqual(commands_frame.cmd_quit(self.args), 0)
            m["_stop_chats"].assert_not_called()
        self.assertIsNone(reopen.read())


class TheTrackerIsPrunedAfterTheKill(PersonaIso, unittest.TestCase):
    """§2.15: records clear only on `finish()`, so a quit strands every one of them."""

    def test_prune_all_removes_every_record_and_says_how_many(self):
        self.assertIsNotNone(inflight.start("steward"))
        self.assertIsNotNone(inflight.start("forge", kind=inflight.CLONE))
        self.assertEqual(len(inflight.live(kind=None)), 2)

        self.assertEqual(inflight.prune_all(), 2)

        self.assertEqual(inflight.live(kind=None), [])
        self.assertEqual(inflight.prune_all(), 0, "and it is idempotent")

    def test_the_prune_runs_after_the_kill_not_before(self):
        _plant("alpha.1", ws="alpha")
        inflight.start("steward")
        order = []

        def _stop(doomed, *, windows):
            order.append(("kill", len(inflight.live(kind=None))))
            return 1

        def _prune():
            order.append(("prune", None))
            return 1

        with mock.patch.multiple(commands_frame,
                                 _chat_windows=mock.DEFAULT, _active_chats=mock.DEFAULT,
                                 _capture_transcript=mock.DEFAULT) as m, \
                mock.patch.object(commands_frame, "_stop_chats", side_effect=_stop), \
                mock.patch.object(inflight, "prune_all", side_effect=_prune):
            m["_chat_windows"].return_value = {"alpha.1": "@0"}
            m["_active_chats"].return_value = set()
            m["_capture_transcript"].return_value = False
            commands_frame.cmd_quit(SimpleNamespace(chat="alpha.1"))

        self.assertEqual([step for step, _ in order], ["kill", "prune"])
        self.assertEqual(order[0][1], 1,
                         "the record was still there while the kill was attempted")


class TheManifestRefusesWhatItCannotRead(PersonaIso, unittest.TestCase):
    """Every value in it came off disk and is going onto an argv, a `chdir` or a flag."""

    def test_a_version_this_charter_does_not_speak_is_nothing_to_reopen(self):
        config.private_mkdir(state._root())
        config.write_for(reopen.path(), json.dumps({"version": 99, "frames": []}) + "\n")

        self.assertIsNone(reopen.read())

    def test_a_file_that_is_not_json_is_nothing_to_reopen(self):
        config.private_mkdir(state._root())
        config.write_for(reopen.path(), "{not json\n")

        self.assertIsNone(reopen.read())

    def test_a_chat_whose_name_could_not_be_a_chat_is_dropped_from_the_frame(self):
        config.private_mkdir(state._root())
        config.write_for(reopen.path(), json.dumps({
            "version": reopen.VERSION, "at": 1, "focus": "alpha",
            "frames": [{"workspace": "alpha", "chats": [
                {"chat": "alpha.1;kill-server", "workspace": "alpha"},
                {"chat": "alpha.2", "workspace": "alpha"}]}]}) + "\n")

        m = reopen.read()

        self.assertEqual([c.chat for c in m.all_chats()], ["alpha.2"])

    def test_a_chat_whose_workspace_could_not_be_a_workspace_is_dropped(self):
        config.private_mkdir(state._root())
        config.write_for(reopen.path(), json.dumps({
            "version": reopen.VERSION, "at": 1, "focus": "",
            "frames": [{"workspace": "../etc", "chats": [
                {"chat": "alpha.1", "workspace": "../etc"}]}]}) + "\n")

        self.assertEqual(reopen.read().frames, ())

    def test_a_missing_field_is_that_field_empty_and_not_a_crash(self):
        config.private_mkdir(state._root())
        config.write_for(reopen.path(), json.dumps({
            "version": reopen.VERSION, "at": 1, "focus": "alpha",
            "frames": [{"workspace": "alpha",
                        "chats": [{"chat": "alpha.1", "workspace": "alpha"}]}]}) + "\n")

        c = reopen.read().all_chats()[0]

        self.assertEqual((c.resume, c.cwd, c.persona, c.harness, c.transcript, c.active),
                         ("", "", "", "", "", False))


class TranscriptsAreBounded(PersonaIso, unittest.TestCase):
    """A file in the frame root has no collector but the thing that writes it."""

    def test_a_transcript_no_manifest_names_is_pruned(self):
        config.private_mkdir(state._root())
        for fid in ("alpha.1", "alpha.2"):
            config.write_for(reopen.transcript_path(fid), "text\n")

        reopen.prune_transcripts({"alpha.1"})

        self.assertTrue(reopen.transcript_path("alpha.1").is_file())
        self.assertFalse(reopen.transcript_path("alpha.2").exists())

    def test_the_prune_touches_nothing_but_transcripts(self):
        _plant("alpha.1", ws="alpha")
        reopen.write([], focus="")
        config.write_for(reopen.transcript_path("alpha.2"), "text\n")

        reopen.prune_transcripts(set())

        self.assertTrue(state.frame_dir("alpha.1").is_dir())
        self.assertTrue(reopen.path().is_file())

    def test_a_capture_is_bounded_in_lines_at_tmux_and_in_bytes_here(self):
        # The line bound is a tmux argument, so it is asserted on the argv rather than on
        # the answer; the byte bound is applied to what came back, keeping the END.
        seen = {}

        class _Out:
            returncode = 0
            stdout = "x" * (commands_frame._TRANSCRIPT_BYTES + 10) + "TAIL"

        def _run(why, argv, **kw):
            seen["argv"] = argv
            return _Out()

        config.private_mkdir(state._root())
        dest = reopen.transcript_path("alpha.1")
        with mock.patch.object(commands_frame.tmuxctl, "run", side_effect=_run):
            self.assertTrue(commands_frame._capture_transcript(SERVER, "%1", dest))

        self.assertIn(f"-{commands_frame._TRANSCRIPT_LINES}", seen["argv"])
        self.assertIn("-N", seen["argv"])
        text = dest.read_text()
        self.assertTrue(text.endswith("TAIL"))
        self.assertLessEqual(len(text.encode()), commands_frame._TRANSCRIPT_BYTES)


if __name__ == "__main__":       # pragma: no cover
    unittest.main()
