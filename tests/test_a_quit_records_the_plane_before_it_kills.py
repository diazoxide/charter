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


def _seats(windows, active):
    """`commands_frame._chat_seats`' answer, built from what a case means to say.

    One listing answers three questions — which chats are live, which window each is in, and
    which was its session's current — so a fake for it is one value rather than three
    patches. Written as a helper rather than spelled per case so a case says
    ``{"alpha.1": "@0"}`` and ``{"alpha.1"}`` and nothing about tuple order.
    """
    return [(chat, window, chat in active) for chat, window in windows.items()]


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
                                 _chat_seats=mock.DEFAULT,
                                 _capture_transcript=mock.DEFAULT,
                                 _stop_chats=mock.DEFAULT) as m:
            m["_chat_seats"].return_value = _seats({"alpha.1": "@0", "alpha.9": "@1"}, set())
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

        Patched at `_chat_seats` rather than at `tmuxctl.run`, because
        what these tests are about is the ORDER of charter's own writes — and a fake at the
        subprocess boundary would make every one of them depend on a format string.
        """
        return mock.patch.multiple(
            commands_frame,
            _chat_seats=mock.DEFAULT,
            _capture_transcript=mock.DEFAULT, _stop_chats=mock.DEFAULT)

    def test_the_manifest_is_on_disk_before_anything_is_stopped(self):
        seen = {}

        def _stop(doomed, *, windows):
            seen["manifest"] = reopen.read()
            return len(doomed)

        with self._no_tmux() as m:
            m["_chat_seats"].return_value = _seats({"alpha.1": "@0"}, {"alpha.1"})
            m["_capture_transcript"].return_value = False
            m["_stop_chats"].side_effect = _stop
            self.assertEqual(commands_frame.cmd_quit(self.args), 0)

        self.assertIsNotNone(seen["manifest"], "the kill ran before the record landed")
        self.assertEqual([c.chat for c in seen["manifest"].all_chats()], ["alpha.1"])

    def test_a_record_that_will_not_land_refuses_the_quit_and_kills_nothing(self):
        with self._no_tmux() as m, mock.patch.object(reopen, "write", return_value=False):
            m["_chat_seats"].return_value = _seats({"alpha.1": "@0"}, set())
            m["_capture_transcript"].return_value = False
            self.assertEqual(commands_frame.cmd_quit(self.args), 1)
            m["_stop_chats"].assert_not_called()

    def test_the_focus_is_the_workspace_the_quit_was_pressed_in(self):
        _plant("beta.1", ws="beta")
        with self._no_tmux() as m:
            m["_chat_seats"].return_value = _seats({"alpha.1": "@0", "beta.1": "@1"}, set())
            m["_capture_transcript"].return_value = False
            m["_stop_chats"].return_value = 2
            commands_frame.cmd_quit(SimpleNamespace(chat="beta.1"))

        self.assertEqual(reopen.read().focus, "beta")

    def test_the_chats_are_grouped_by_workspace_one_frame_each(self):
        _plant("alpha.2", ws="alpha")
        _plant("beta.1", ws="beta")
        with self._no_tmux() as m:
            m["_chat_seats"].return_value = _seats({"alpha.1": "@0", "alpha.2": "@1",
                                               "beta.1": "@2"}, set())
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
            m["_chat_seats"].return_value = []
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
                                 _chat_seats=mock.DEFAULT,
                                 _capture_transcript=mock.DEFAULT) as m, \
                mock.patch.object(commands_frame, "_stop_chats", side_effect=_stop), \
                mock.patch.object(inflight, "prune_all", side_effect=_prune):
            m["_chat_seats"].return_value = _seats({"alpha.1": "@0"}, set())
            m["_capture_transcript"].return_value = False
            commands_frame.cmd_quit(SimpleNamespace(chat="alpha.1"))

        self.assertEqual([step for step, _ in order], ["kill", "prune"])
        self.assertEqual(order[0][1], 1,
                         "the record was still there while the kill was attempted")


class TheQuitsOwnSentenceIsSaidByTheLauncher(PersonaIso, unittest.TestCase):
    """The gap found by asking where the quit's own message goes: nowhere.

    `charter: quit` is spawned with its three streams on `/dev/null` — `_spawn`'s measured
    requirement, since the palette's pane is killed the instant a row is invoked — and by the
    time it has finished there is no frame left to draw a notice on. `cmd_launch` is the
    process that hands the operator their shell back, so it is the one that can name
    `charter reopen`.
    """

    def _said(self, fid):
        import io
        from contextlib import redirect_stderr
        buf = io.StringIO()
        with redirect_stderr(buf):
            commands_frame._say_it_was_quit(fid)
        return buf.getvalue()

    def test_a_launcher_whose_chat_was_quit_names_the_command_that_undoes_it(self):
        reopen.write([reopen.Frame(workspace="alpha", chats=(
            reopen.Chat(chat="alpha.1", workspace="alpha", persona="",
                        harness="claude-code", cwd="", resume="conv-1", transcript="",
                        active=True),))], focus="alpha")

        said = self._said("alpha.1")

        self.assertIn("charter reopen", said)
        self.assertIn("1 with a conversation to resume", said)

    def test_a_launcher_whose_chat_merely_ended_says_nothing(self):
        # The gate that makes the sentence true rather than likely: a detach and an ordinary
        # harness exit reach the same lines of `cmd_launch`.
        reopen.write([reopen.Frame(workspace="alpha", chats=(
            reopen.Chat(chat="alpha.2", workspace="alpha", persona="", harness="c",
                        cwd="", resume="", transcript="", active=True),))], focus="alpha")

        self.assertEqual(self._said("alpha.1"), "")

    def test_no_manifest_at_all_says_nothing(self):
        self.assertEqual(self._said("alpha.1"), "")


class TheWarningIsSaidOnStderrAndOnTheScreenTheOperatorHas(PersonaIso, unittest.TestCase):
    """`_warn_about`'s two surfaces, and the `if on:` the sweep found unpinned.

    §4i is explicit that quit *warns and proceeds*, so the record of what was lost has to
    survive the keypress — and once the frame's panes are about to stop existing, stderr is
    the only surface left. The attention row is the OTHER caller's, and it is written only
    when there is a frame to write it to: `charter frame-quit` typed in an ordinary shell has
    no chat, and `state.say("")` would be a notice under a frame id that names nothing.
    """

    def setUp(self):
        super().setUp()
        _plant("alpha.1", ws="alpha", sid="conv-1")
        self.plan = leave.plan(live={"alpha.1"}, focus="alpha")

    def _warn(self, on):
        import io
        from contextlib import redirect_stderr
        buf = io.StringIO()
        with redirect_stderr(buf), \
                mock.patch.object(commands_frame, "_say_on_screen") as said:
            commands_frame._warn_about(self.plan, on=on, verb=leave.QUIT)
        return buf.getvalue(), said

    def test_every_chat_gets_its_own_line_on_stderr(self):
        out, _said = self._warn("alpha.1")

        self.assertIn(leave.summary(self.plan), out)
        self.assertIn("alpha.1", out)
        self.assertIn(leave.RESUMES, out)

    def test_the_summary_also_lands_on_the_frame_that_asked(self):
        _out, said = self._warn("alpha.1")

        said.assert_called_once()
        self.assertEqual(said.call_args[0][0], "alpha.1")
        self.assertEqual(said.call_args[0][1], leave.summary(self.plan))

    def test_a_quit_typed_outside_a_frame_writes_no_notice_anywhere(self):
        # The `if on:` guard. There is no frame to draw on, and a notice written under an
        # empty id is one nothing can ever show.
        out, said = self._warn("")

        said.assert_not_called()
        self.assertIn("alpha.1", out, "and stderr still carries the whole warning")


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


class _Answered:
    """One `tmuxctl.run` answer, so a parser can be driven without a server.

    The three fields `tmuxctl.run` callers actually read. A class rather than a
    `SimpleNamespace` so a field nobody set is an `AttributeError` here rather than a
    silently-`None` somewhere downstream.
    """

    def __init__(self, stdout="", returncode=0):
        self.stdout = stdout
        self.returncode = returncode


def _answers(stdout="", returncode=0):
    """A `tmuxctl.run` stand-in that always gives the same answer, and records the argv."""
    seen = []

    def _run(why, argv, **kw):
        seen.append(argv)
        return _Answered(stdout, returncode)

    return _run, seen


class TheWindowListingRefusesWhatItCannotRead(PersonaIso, unittest.TestCase):
    """`_chat_seats`, driven against fabricated tmux output rather than a server.

    **Every assertion here exists because the deletion sweep asked for it.** The real-tmux
    module drives this function against a real `list-windows`, which only ever produces
    well-formed rows — so the three refusals below were unpinned, and the sweep said so:
    dropping the field-count guard, and dropping either half of the regex pair, all left the
    whole 9,895-test suite green.

    They are #475's boundary: these values come off a tmux option and go on to be a `-t`
    target and a state directory's name. `%1;kill-server` in that position is the shape that
    already cost this project a `kill-server` armed on every window resize.
    """

    def _seats_from(self, stdout):
        run, _seen = _answers(stdout)
        with mock.patch.object(commands_frame.tmuxctl, "run", side_effect=run):
            return commands_frame._chat_seats(SERVER)

    def test_a_well_formed_listing_is_read_whole(self):
        seats = self._seats_from("alpha.1\t@0\t1\nalpha.2\t@3\t0\n")

        self.assertEqual(seats, [("alpha.1", "@0", True), ("alpha.2", "@3", False)])

    def test_a_row_with_the_wrong_number_of_fields_is_dropped(self):
        # A window with no `@charter_chat` prints an empty first field, and a format that
        # ever changed shape would arrive here as a row of the wrong width. Neither is a
        # seat, and neither may be read as one by index.
        seats = self._seats_from("alpha.1\t@0\t1\n"
                                 "only-two\t@1\n"
                                 "a\tb\tc\td\n"
                                 "\n")

        self.assertEqual(seats, [("alpha.1", "@0", True)])

    def test_a_chat_id_outside_the_alphabet_is_dropped(self):
        seats = self._seats_from("alpha.1;kill-server\t@0\t1\nalpha.2\t@1\t0\n")

        self.assertEqual(seats, [("alpha.2", "@1", False)])

    def test_a_window_id_that_is_not_tmuxs_own_shape_is_dropped(self):
        # The window id is what `_stop_chats` aims `kill-window` at. Anything that is not
        # `@<digits>` is a target charter did not get from tmux.
        seats = self._seats_from("alpha.1\t$0\t1\n"
                                 "alpha.2\tnot-a-window\t1\n"
                                 "alpha.3\t@7\t1\n")

        self.assertEqual(seats, [("alpha.3", "@7", True)])

    def test_only_the_exact_active_flag_means_active(self):
        # `#{window_active}` is `0` or `1`. Anything else is not a claim charter may read as
        # "this is the chat the operator was looking at" — that answer decides which tab a
        # reopen puts them back on.
        seats = self._seats_from("alpha.1\t@0\t1\nalpha.2\t@1\t0\nalpha.3\t@2\ttrue\n")

        self.assertEqual([c for c, _w, showing in seats if showing], ["alpha.1"])

    def test_a_server_that_would_not_answer_is_none_and_not_empty(self):
        run, _seen = _answers("", returncode=1)
        with mock.patch.object(commands_frame.tmuxctl, "run", side_effect=run):
            self.assertIsNone(commands_frame._chat_seats(SERVER))

    def test_a_live_server_with_no_chats_is_an_empty_list(self):
        # The opposite fact from the line above, and the whole reason the tri-state exists.
        self.assertEqual(self._seats_from(""), [])

    def test_the_listing_is_one_call_asking_for_all_three_fields(self):
        run, seen = _answers("")
        with mock.patch.object(commands_frame.tmuxctl, "run", side_effect=run):
            commands_frame._chat_seats(SERVER)

        self.assertEqual(len(seen), 1, "two calls for one listing is two answers")
        self.assertIn("list-windows", seen[0])
        self.assertIn(commands_frame._CHAT_WINDOW_FORMAT, seen[0])


class OneServerRefusingMakesTheWholePlaneUnknown(PersonaIso, unittest.TestCase):
    """`_plane_live`'s tri-state, which the sweep found unpinned in both directions.

    With two servers in play, a quit that trusted the half that answered would record only
    one server's chats and kill only those — and then tell the operator it had stopped the
    plane. So ``None`` is the answer when **any** server refuses, and the sweep's mutations
    (collapse the conditional either way) both have to go red.
    """

    def _live(self, answers):
        """Drive `_plane_live` over servers whose answers are stated per server."""
        def _seats(server):
            return answers[server]

        with mock.patch.object(commands_frame, "_chat_seats", side_effect=_seats):
            return commands_frame._plane_live(list(answers))

    def test_every_server_answering_gives_the_union_and_never_none(self):
        live, windows, active = self._live({
            "s1": [("alpha.1", "@0", True)],
            "s2": [("beta.1", "@5", False)]})

        self.assertEqual(live, {"alpha.1", "beta.1"})
        self.assertEqual(windows, {"s1": {"alpha.1": "@0"}, "s2": {"beta.1": "@5"}})
        self.assertEqual(active, {"alpha.1"})

    def test_one_server_refusing_makes_the_liveness_answer_none(self):
        live, windows, _active = self._live({
            "s1": [("alpha.1", "@0", True)],
            "s2": None})

        self.assertIsNone(live, "the half that answered is not the plane")
        self.assertEqual(windows, {"s1": {"alpha.1": "@0"}},
                         "and what did answer is still aimed at")

    def test_every_server_refusing_is_also_none(self):
        live, windows, active = self._live({"s1": None, "s2": None})

        self.assertIsNone(live)
        self.assertEqual((windows, active), ({}, set()))

    def test_only_the_shown_chat_is_reported_active(self):
        # The `showing` filter the sweep dropped: without it every chat would be marked as
        # the one on screen, and a reopen would pick whichever came first.
        _live, _windows, active = self._live({
            "s1": [("alpha.1", "@0", False), ("alpha.2", "@1", True),
                   ("alpha.3", "@2", False)]})

        self.assertEqual(active, {"alpha.2"})


class TheCaptureIsBoundedAndNeverRaises(PersonaIso, unittest.TestCase):
    """`_capture_transcript`'s four refusals, each unpinned until the sweep asked."""

    def setUp(self):
        super().setUp()
        config.private_mkdir(state._root())
        self.dest = reopen.transcript_path("alpha.1")

    def _capture(self, stdout, returncode=0):
        run, seen = _answers(stdout, returncode)
        with mock.patch.object(commands_frame.tmuxctl, "run", side_effect=run):
            return commands_frame._capture_transcript(SERVER, "%1", self.dest), seen

    def test_the_line_bound_is_the_shipped_number_and_it_is_asked_of_tmux(self):
        # Asserted as the LITERAL, not as an f-string over the constant: the sweep retunes
        # the constant, and an assertion that reads the constant follows it and stays green.
        # The bound belongs where the memory is (one 200-column pane at `history_limit =
        # 50000` took a tmux server from 3.7 MB to 130 MB), so what matters is that tmux is
        # asked for a bounded window at all — and which one.
        self.assertEqual(commands_frame._TRANSCRIPT_LINES, 2000)
        _ok, seen = self._capture("something\n")
        self.assertIn("-2000", seen[0])
        self.assertIn("-S", seen[0])

    def test_a_capture_at_the_byte_cap_is_kept_whole(self):
        text = "x" * commands_frame._TRANSCRIPT_BYTES
        ok, _seen = self._capture(text)

        self.assertTrue(ok)
        self.assertEqual(self.dest.read_bytes(), text.encode(),
                         "exactly at the cap loses nothing")

    def test_one_byte_over_the_cap_is_trimmed_from_the_front(self):
        text = "HEAD" + "x" * commands_frame._TRANSCRIPT_BYTES + "TAIL"
        ok, _seen = self._capture(text)

        self.assertTrue(ok)
        got = self.dest.read_text()
        self.assertTrue(got.endswith("TAIL"), "the end is what was on screen")
        self.assertNotIn("HEAD", got)
        self.assertLessEqual(len(got.encode()), commands_frame._TRANSCRIPT_BYTES)

    def test_the_cap_holds_for_multi_byte_text_too(self):
        """The defect the sweep led to, and the reason the comparison is gone.

        The first version measured BYTES and then trimmed CHARACTERS, so a capture made of
        two-byte characters was left at twice the cap — measured at a cap of 16, `"é" * 16`
        is 16 characters and 32 bytes and the old line answered 32. It is byte-exact now.
        """
        ok, _seen = self._capture("é" * commands_frame._TRANSCRIPT_BYTES)

        self.assertTrue(ok)
        self.assertLessEqual(len(self.dest.read_bytes()),
                             commands_frame._TRANSCRIPT_BYTES)

    def test_the_cut_never_writes_half_a_character(self):
        # A byte cut can land inside a character, and the only edge it can land inside is
        # the leading one. That partial character is dropped rather than written as a
        # replacement glyph the pager would show as noise.
        ok, _seen = self._capture("é" * commands_frame._TRANSCRIPT_BYTES)

        self.assertTrue(ok)
        got = self.dest.read_text()    # would raise if the cut split a character
        self.assertNotIn("\ufffd", got)
        self.assertTrue(got.startswith("é"))

    def test_a_pane_that_answered_nothing_writes_no_file(self):
        for empty in ("", "   \n\n"):
            self.dest.unlink(missing_ok=True)
            ok, _seen = self._capture(empty)
            self.assertFalse(ok, repr(empty))
            self.assertFalse(self.dest.exists(), repr(empty))

    def test_a_server_that_would_not_answer_writes_no_file(self):
        ok, _seen = self._capture("text\n", returncode=1)

        self.assertFalse(ok)
        self.assertFalse(self.dest.exists())

    def test_a_frame_root_that_cannot_be_made_is_reported_not_raised(self):
        run, _seen = _answers("text\n")
        with mock.patch.object(commands_frame.tmuxctl, "run", side_effect=run), \
                mock.patch.object(commands_frame.config, "private_mkdir",
                                  side_effect=OSError("no")):
            self.assertFalse(commands_frame._capture_transcript(SERVER, "%1", self.dest))

    def test_a_write_that_cannot_land_is_reported_not_raised(self):
        run, _seen = _answers("text\n")
        with mock.patch.object(commands_frame.tmuxctl, "run", side_effect=run), \
                mock.patch.object(commands_frame.config, "write_for",
                                  side_effect=OSError("full")):
            self.assertFalse(commands_frame._capture_transcript(SERVER, "%1", self.dest))


if __name__ == "__main__":       # pragma: no cover
    unittest.main()
