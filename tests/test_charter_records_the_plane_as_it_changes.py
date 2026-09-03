"""#845: the plane is recorded as it changes, and put back when you type `charter`.

**The word is RECORD.** `charter save` commits and pushes a tree; `reopen.write` writes a
file. 0.55.0's own news entry says *"quitting charter records the plane"*, so that is the
vocabulary this uses — a refusal that said "save" would be ambiguous about which of the two
did not happen.

Four claims, and each one is a decision that could have gone the other way.

**1. The write is debounced on the bumps that already exist.** `state.bump` is charter's
own "something changed" signal and every writer in the frame already moves it, so there is
no second notion of change to keep in step. Writing on every bump would record planes that
never existed — `cmd_launch` bumps after `new_chat_id` and before the window is made, so a
record taken there names a chat with no window — and a periodic timer would do the same
while also spending work on a plane that did nothing. `Debounce` is that arithmetic on its
own, with no clock and no thread under it.

**2. The frame process is the sole writer, and it has to POLL to be one.** Panels are
separate processes and six of them notice one bump, so "record where the bump was noticed"
is six processes racing one file. A designated panel is worse — it stops recording when
that panel dies, which is when the record matters most. **The cost this pays is real and is
stated rather than hidden: the frame process does not watch bumps today, only panels do
(`panel._watch`), so this adds a watch to it.** There is no cheaper mechanism available: a
bump is a file written by another process and the standard library has no file-change
notification, which is why `panel._watch` polls too.

**3. What it records is what the manifest records today**, and the transcript field is the
one that needs saying: a continuous record does not `capture-pane` (that is one subprocess
per chat per write), so it names the capture a QUIT left on disk and never invents one.

**4. `--fresh` does not participate**, in both directions. Skipping the restore alone would
leave the recorder overwriting the record it declined to act on, on a two-second timer, and
the loss would be invisible until the operator asked for the plane back.

**`PersonaIso` on every case that touches a path, and every case asserts the state
directory it is about to write is a throwaway one.** This issue is about writing plane
state, so the hazard `tests/_planeguard.py` exists for is at its sharpest here.
"""

from __future__ import annotations

import io
import subprocess
import threading
import unittest
from contextlib import redirect_stderr
from types import SimpleNamespace
from unittest import mock

from charter import cli, commands_frame, config, instance
from charter.frame import record, reopen, state

from tests._isolation import PersonaIso


def _completed(rc=0, out=""):
    return subprocess.CompletedProcess(["tmux"], rc, stdout=out, stderr="")


class TheDebounceIsOneWritePerQuietPeriod(unittest.TestCase):
    """`record.Debounce` — the whole of "when it writes", with no clock and no thread.

    Pure arithmetic over (what was seen, when it changed, what was written), so the
    decision can be asserted at exact times rather than by sleeping.
    """

    def test_a_change_is_not_written_until_the_plane_has_been_quiet(self):
        d = record.Debounce(quiet=2.0)
        d.saw(("a", "1"), now=10.0)
        self.assertFalse(d.due(now=11.9))
        self.assertTrue(d.due(now=12.0))

    def test_a_second_change_inside_the_window_moves_the_deadline(self):
        """The half that makes this a debounce rather than a delay. A launch bumps several
        times in a row; the record is taken once, after the last of them."""
        d = record.Debounce(quiet=2.0)
        d.saw(("a", "1"), now=10.0)
        d.saw(("a", "2"), now=11.0)
        self.assertFalse(d.due(now=12.0))
        self.assertTrue(d.due(now=13.0))

    def test_a_plane_that_has_not_changed_is_not_written_again(self):
        d = record.Debounce(quiet=2.0)
        d.saw(("a", "1"), now=10.0)
        self.assertTrue(d.due(now=12.0))
        d.wrote()
        self.assertFalse(d.due(now=99.0))

    def test_the_first_reading_of_a_plane_is_itself_a_change(self):
        """A plane that came back from `charter reopen` and then sat still has had no bump
        since — and `_consume` deleted the manifest that put it there. Without this it
        would be recorded nowhere at all until something happened to move it."""
        d = record.Debounce(quiet=2.0)
        d.saw((("a", "1"),), now=0.0)
        self.assertTrue(d.due(now=2.0))

    def test_a_plane_that_goes_empty_is_a_change_like_any_other(self):
        """The chats went away — that is a fact about the plane, not an absence of one."""
        d = record.Debounce(quiet=2.0)
        d.saw((("a", "1"),), now=0.0)
        d.wrote()
        d.saw((), now=5.0)
        self.assertTrue(d.due(now=7.0))


class TheFingerprintIsTheBumpsThatAlreadyExist(PersonaIso, unittest.TestCase):
    """`record.fingerprint` — every chat on the plane and the version it is at.

    Read through `state.version`, which is what a panel polls, so there is one answer to
    "has this chat changed" rather than two that can drift.
    """

    def setUp(self) -> None:
        super().setUp()
        self.assertIn("edm-test-", str(config.STATE_DIR))

    def test_a_bump_changes_it(self):
        state.frame_dir("alpha.1", create=True)
        state.bump("alpha.1")
        before = record.fingerprint()
        state.bump("alpha.1")
        self.assertNotEqual(record.fingerprint(), before)

    def test_a_new_chat_changes_it(self):
        state.frame_dir("alpha.1", create=True)
        state.bump("alpha.1")
        before = record.fingerprint()
        state.frame_dir("alpha.2", create=True)
        state.bump("alpha.2")
        self.assertNotEqual(record.fingerprint(), before)

    def test_a_chat_that_went_away_changes_it(self):
        state.frame_dir("alpha.1", create=True)
        state.bump("alpha.1")
        state.frame_dir("alpha.2", create=True)
        state.bump("alpha.2")
        before = record.fingerprint()
        state.reap(set(), server="charter")
        self.assertNotEqual(record.fingerprint(), before)

    def test_the_manifest_beside_the_chats_is_not_one_of_them(self):
        """The frame root holds the record itself and the transcripts, and neither is a
        chat — `leave.plane_chats`' own rule, and the reason writing the record cannot
        make the fingerprint change and provoke another write."""
        config.private_mkdir(state._root())
        config.write_for(state._root() / "reopen.json", "{}\n")
        config.write_for(state._root() / "alpha.1.transcript", "hi\n")
        self.assertEqual(record.fingerprint(), ())

    def test_a_plane_with_no_frame_root_at_all_answers_empty(self):
        """A machine that has never launched a frame. A reader on a poll loop may not
        raise for it — `state.version`'s own promise, one directory out."""
        self.assertEqual(record.fingerprint(), ())


class TheRecorderIsOneWriterPerProcess(unittest.TestCase):
    """`record.Recorder` — the loop, and the singleton that stops it being two loops.

    No filesystem: the reading and the writing are both injected, because what is under
    test is when the loop calls out and with what, not what the call does.
    """

    def setUp(self) -> None:
        self.addCleanup(record.stop)
        self.wrote: list[str] = []

    def _writer(self, ok: bool = True):
        def write(chat: str) -> bool:
            self.wrote.append(chat)
            return ok
        return write

    def test_a_tick_writes_nothing_until_the_plane_has_been_quiet(self):
        r = record.Recorder(self._writer(), read=lambda: (("a", "1"),), quiet=2.0)
        r.tick(now=0.0)
        self.assertEqual(self.wrote, [])
        r.tick(now=2.0)
        self.assertEqual(self.wrote, [""])

    def test_the_write_is_told_which_chat_the_terminal_is_on(self):
        """The `focus` a reopen attaches to comes from this. It is the launcher's OWN chat
        rather than a session name off tmux: session names are shared by every plane on the
        machine (§3.3), so reading the focus off one would be another plane answering for
        this one."""
        r = record.Recorder(self._writer(), read=lambda: (("a", "1"),), quiet=0.0)
        r.chat = "alpha.2"
        r.tick(now=0.0)
        self.assertEqual(self.wrote, ["alpha.2"])

    def test_a_plane_that_did_not_change_is_written_once(self):
        r = record.Recorder(self._writer(), read=lambda: (("a", "1"),), quiet=0.0)
        for now in (0.0, 1.0, 2.0, 3.0):
            r.tick(now=now)
        self.assertEqual(self.wrote, [""])

    def test_a_write_that_did_not_land_is_not_retried_every_tick(self):
        """A full filesystem must not turn a two-second debounce into a hot loop. The
        reading was acted on; that it failed is `reopen.write`'s answer, not a reason to
        ask it again about the same plane."""
        r = record.Recorder(self._writer(ok=False), read=lambda: (("a", "1"),), quiet=0.0)
        r.tick(now=0.0)
        r.tick(now=1.0)
        self.assertEqual(self.wrote, [""])

    def test_a_reader_that_raises_does_not_take_the_loop_down(self):
        """This runs on a thread inside the process holding the operator's terminal, so an
        exception here is either a silently dead recorder or a traceback drawn over a
        frame. `frame/state.py`'s promise, one module out."""
        def boom():
            raise OSError("gone")
        r = record.Recorder(self._writer(), read=boom, quiet=0.0)
        r.tick(now=0.0)
        self.assertEqual(self.wrote, [])

    def test_a_writer_that_raises_does_not_take_the_loop_down(self):
        def boom(_chat):
            raise OSError("gone")
        r = record.Recorder(boom, read=lambda: (("a", "1"),), quiet=0.0)
        r.tick(now=0.0)

    def test_a_started_recorder_writes_and_a_stopped_one_has_really_stopped(self):
        """The thread is asked, not a flag: a `stop` that set a bit and left the loop
        running would be a recorder still writing into a plane the launcher is reaping."""
        landed = threading.Event()

        def write(_chat: str) -> bool:
            landed.set()
            return True

        r = record.start(write, quiet=0.0, poll=0.001)
        self.assertIsNotNone(r)
        self.assertTrue(landed.wait(timeout=5), "the recorder never wrote")
        record.stop()
        self.assertFalse(r.alive())
        self.assertIsNone(record.running())

    def test_a_second_start_in_one_process_does_not_make_a_second_writer(self):
        """The automatic restore reaches `cmd_reopen` from inside `cmd_launch`, and both
        of them attach — so without this the one process would hold two loops racing one
        manifest, which is the very thing keeping panels out of this buys."""
        first = record.start(self._writer(), quiet=0.0, poll=0.001)
        self.assertIsNotNone(first)
        self.assertIsNone(record.start(self._writer(), quiet=0.0, poll=0.001))
        self.assertIs(record.running(), first)

    def test_a_recorder_that_never_started_stops_without_a_fuss(self):
        """`record.stop` is the `finally` of every caller, so it has to be safe for a watch
        that never began — a launch that refused before it got that far."""
        r = record.Recorder(self._writer())
        r.stop()
        self.assertFalse(r.alive())

    def test_focus_on_moves_the_chat_without_starting_anything(self):
        record.focus_on("alpha.9")
        self.assertIsNone(record.running())
        r = record.start(self._writer(), quiet=0.0, poll=0.001)
        record.focus_on("alpha.9")
        self.assertEqual(r.chat, "alpha.9")


class RecordingAndRestoringAreTwoKeys(unittest.TestCase):
    """`[frame] record` and `[frame] restore`, both shipped on.

    **Two keys and not one, because the two are separable and the asymmetry is real.**
    Recording costs a little I/O and changes nothing an operator sees; restoring changes
    what happens when they type `charter`. Somebody may reasonably want the plane recorded
    — so `charter reopen` has something to act on when they ask for it — without `charter`
    alone bringing back yesterday's six chats. One key makes that position unreachable.

    Asked of `instance.frame_of` rather than of `config.FRAME`, which on a developer's
    machine is the plane the suite is running inside (this suite's own recurring defect).
    """

    def test_both_are_on_for_a_plane_that_says_nothing(self):
        got = instance.frame_of({})
        self.assertIs(got["record"], True)
        self.assertIs(got["restore"], True)

    def test_recording_can_be_turned_off_on_its_own(self):
        got = instance.frame_of({"frame": {"record": False}})
        self.assertIs(got["record"], False)
        self.assertIs(got["restore"], True)

    def test_restoring_can_be_turned_off_on_its_own(self):
        """The position one key cannot express: record it, and let me ask for it back."""
        got = instance.frame_of({"frame": {"restore": False}})
        self.assertIs(got["restore"], False)
        self.assertIs(got["record"], True)

    def test_a_value_that_is_not_a_boolean_is_the_shipped_answer(self):
        """`charter.toml` is committed and arrives from somebody else's machine, so a key
        charter cannot read degrades rather than raising — `frame_of`'s own contract."""
        for value in ("yes", 1, [], {"on": True}):
            with self.subTest(value=value):
                got = instance.frame_of({"frame": {"record": value, "restore": value}})
                self.assertIs(got["record"], True)
                self.assertIs(got["restore"], True)

    def test_the_toml_spelling_is_the_word_itself(self):
        """One word each, so `docs/frame.md`'s hyphen rule (`history-limit`, never
        `history_limit`) does not arise for either of them."""
        self.assertEqual(instance.FRAME_FIELDS["record"][1], "record")
        self.assertEqual(instance.FRAME_FIELDS["restore"][1], "restore")


def _frame(ws: str, fid: str, *, resume: str = "") -> "reopen.Frame":
    """One workspace's worth of manifest, spelled once so a case says what it is about."""
    return reopen.Frame(workspace=ws, chats=(reopen.Chat(
        chat=fid, workspace=ws, persona="", harness="claude-code", cwd="",
        resume=resume, transcript="", active=True),))


class TheRecordIsWrittenAtomically(PersonaIso, unittest.TestCase):
    """`reopen.write` — temp plus rename, so a crash mid-write leaves the PREVIOUS record.

    This already mattered when a quit was the only writer. At a hundred times the write
    rate it matters more, and it brings a second writer with it: the frame process is now
    recording while a quit — or a second attached terminal — may also be writing.
    """

    def setUp(self) -> None:
        super().setUp()
        self.assertIn("edm-test-", str(config.STATE_DIR))

    def test_two_writers_do_not_interleave_into_one_temp_file(self):
        """`os.replace` is atomic; the file it renames is not. Two writers sharing one temp
        NAME write into the same bytes — and the first to rename puts the SECOND writer's
        content in place while reporting that its own record landed, which is a manifest
        that describes a plane nobody asked to record."""
        real = config.write_for
        sibling: list[bool] = []

        def racing(path, text, **kw):
            real(path, text, **kw)
            if not sibling:            # the other writer, once, mid-write
                sibling.append(True)
                reopen.write([_frame("beta", "beta.1")], focus="beta")

        with mock.patch.object(config, "write_for", racing):
            landed = reopen.write([_frame("alpha", "alpha.1")], focus="alpha")

        self.assertTrue(landed)
        self.assertEqual(reopen.read().focus, "alpha")

    def test_a_rename_that_fails_leaves_the_previous_record_whole(self):
        reopen.write([_frame("alpha", "alpha.1", resume="conv-1")], focus="alpha")

        with mock.patch("os.replace", side_effect=OSError("no")):
            self.assertFalse(reopen.write([_frame("beta", "beta.1")], focus="beta"))

        self.assertEqual([c.resume for c in reopen.read().all_chats()], ["conv-1"])

    def test_a_temp_file_that_cannot_be_made_is_a_record_that_did_not_land(self):
        """A full filesystem answers here rather than at the write, and the caller does the
        same thing with either: tell the operator the plane was not recorded."""
        reopen.write([_frame("alpha", "alpha.1", resume="conv-1")], focus="alpha")

        with mock.patch("tempfile.mkstemp", side_effect=OSError("no space")):
            self.assertFalse(reopen.write([_frame("beta", "beta.1")], focus="beta"))

        self.assertEqual([c.resume for c in reopen.read().all_chats()], ["conv-1"])

    def test_a_temp_file_that_cannot_be_removed_is_still_only_a_false(self):
        """The filesystem that refused the rename can refuse the tidy-up too, and this
        function's whole contract is that it answers rather than raises."""
        with mock.patch("os.replace", side_effect=OSError("no")), \
                mock.patch("pathlib.Path.unlink", side_effect=OSError("no")):
            self.assertFalse(reopen.write([_frame("alpha", "alpha.1")], focus="alpha"))

    def test_a_rename_that_fails_leaves_no_half_written_file_behind(self):
        """A temp name of this writer's own would otherwise accumulate one file per crash
        in a directory whose only collector (`reopen.prune_transcripts`) touches nothing but
        `*.transcript`."""
        with mock.patch("os.replace", side_effect=OSError("no")):
            reopen.write([_frame("alpha", "alpha.1")], focus="alpha")

        self.assertEqual(sorted(p.name for p in state._root().iterdir()), [])


def _seats(windows, active):
    """`commands_frame._chat_seats`' answer, built from what a case means to say — the
    same helper `tests/test_a_quit_records_the_plane_before_it_kills.py` uses, so the two
    suites cannot come to disagree about the shape of a tmux listing."""
    return [(chat, window, chat in active) for chat, window in windows.items()]


def _plant(fid: str, *, ws: str, harness: str = "claude-code", pane: str = "%1",
           sid: str = "", cwd: str = "") -> None:
    """Make *fid* look like a chat charter launched — through the production writers, so a
    fixture that stopped agreeing with the launcher fails here rather than against itself."""
    state.frame_dir(fid, create=True)
    state.record_server(fid, commands_frame.SOCKET)
    state.record_workspace(fid, ws)
    state.record_harness_pane(fid, pane)
    state.record_identity(fid, {"CHARTER_HARNESS": harness, "CHARTER_WORKSPACE": "",
                                "CHARTER_PERSONA": ""})
    if sid:
        state.record_harness_session(fid, sid)
    if cwd:
        state.record_cwd(fid, cwd)


class WhatARunningPlaneRecordsIsWhatAQuitRecords(PersonaIso, unittest.TestCase):
    """`commands_frame.record_the_plane_now` — the same manifest, taken without stopping.

    **The same reader as the quit** (`leave.plan`), so "what a chat is" has one answer.
    What differs is the two things a running plane cannot do: it does not `capture-pane`
    (one subprocess per chat per write, at a hundred times the rate), and it does not prune
    — a record taken while everything is still running has no business being the collector
    for files a quit left behind.
    """

    def setUp(self) -> None:
        super().setUp()
        self.assertIn("edm-test-", str(config.STATE_DIR))

    def test_it_records_the_chats_the_plane_has(self):
        _plant("alpha.1", ws="alpha", sid="conv-1", cwd=str(config.ROOT))
        _plant("beta.1", ws="beta", harness="opencode")

        with mock.patch.object(commands_frame, "_chat_seats") as seats:
            seats.return_value = _seats({"alpha.1": "@0", "beta.1": "@1"}, {"beta.1"})
            self.assertTrue(commands_frame.record_the_plane_now("alpha.1"))

        m = reopen.read()
        self.assertEqual([f.workspace for f in m.frames], ["alpha", "beta"])
        first = m.all_chats()[0]
        self.assertEqual(first.chat, "alpha.1")
        self.assertEqual(first.workspace, "alpha")
        self.assertEqual(first.harness, "claude-code")
        self.assertEqual(first.cwd, str(config.ROOT))
        self.assertEqual(first.resume, "conv-1")
        self.assertFalse(first.active)
        self.assertTrue(m.all_chats()[1].active)

    def test_the_focus_is_the_workspace_this_terminal_is_standing_in(self):
        _plant("alpha.1", ws="alpha")
        _plant("beta.1", ws="beta")

        with mock.patch.object(commands_frame, "_chat_seats") as seats:
            seats.return_value = _seats({"alpha.1": "@0", "beta.1": "@1"}, set())
            commands_frame.record_the_plane_now("beta.1")

        self.assertEqual(reopen.read().focus, "beta")

    def test_a_terminal_that_has_not_claimed_a_chat_yet_records_no_focus(self):
        """`""` is a real answer, not a hole: the launcher starts recording before it
        allocates an id, and `_attach_after_reopen` already falls back from a focus it
        cannot place to the chat that was on screen."""
        _plant("alpha.1", ws="alpha")

        with mock.patch.object(commands_frame, "_chat_seats") as seats:
            seats.return_value = _seats({"alpha.1": "@0"}, set())
            self.assertTrue(commands_frame.record_the_plane_now(""))

        self.assertEqual(reopen.read().focus, "")

    def test_it_never_captures_a_pane(self):
        """A `capture-pane` per chat per write is the cost that makes a continuous record
        unaffordable, and a scrollback is the one restore item §4f says is OFFERED rather
        than replayed — so nothing is lost by leaving it to the quit."""
        _plant("alpha.1", ws="alpha")

        with mock.patch.object(commands_frame, "_chat_seats") as seats, \
                mock.patch.object(commands_frame, "_capture_transcript") as cap:
            seats.return_value = _seats({"alpha.1": "@0"}, set())
            commands_frame.record_the_plane_now("alpha.1")

        cap.assert_not_called()

    def test_it_names_the_capture_a_quit_already_left_on_disk(self):
        """Not capturing is not the same as forgetting. A quit that captured and then had
        its manifest overwritten by this would lose the offer — so the field names the file
        that is there, which is the same name the capture would have written."""
        _plant("alpha.1", ws="alpha")
        config.write_for(reopen.transcript_path("alpha.1"), "what was on screen\n")

        with mock.patch.object(commands_frame, "_chat_seats") as seats:
            seats.return_value = _seats({"alpha.1": "@0"}, set())
            commands_frame.record_the_plane_now("alpha.1")

        self.assertEqual(reopen.read().all_chats()[0].transcript, "alpha.1.transcript")

    def test_a_chat_with_no_capture_offers_none(self):
        _plant("alpha.1", ws="alpha")

        with mock.patch.object(commands_frame, "_chat_seats") as seats:
            seats.return_value = _seats({"alpha.1": "@0"}, set())
            commands_frame.record_the_plane_now("alpha.1")

        self.assertEqual(reopen.read().all_chats()[0].transcript, "")

    def test_a_plane_with_nothing_live_is_not_recorded_over(self):
        """The last thing a plane that has just been quit needs is its record replaced by
        an empty one. Nothing live is nothing to record, and the previous record stands."""
        _plant("alpha.1", ws="alpha")
        reopen.write([reopen.Frame(workspace="alpha", chats=(reopen.Chat(
            chat="alpha.1", workspace="alpha", persona="", harness="claude-code",
            cwd="", resume="conv-1", transcript="", active=True),))], focus="alpha")

        with mock.patch.object(commands_frame, "_chat_seats") as seats:
            seats.return_value = []
            self.assertFalse(commands_frame.record_the_plane_now("alpha.1"))

        self.assertEqual([c.resume for c in reopen.read().all_chats()], ["conv-1"])

    def test_a_record_that_did_not_land_says_so(self):
        """The recorder's own loop reads this: a write that failed is not retried on every
        tick, so the answer has to be honest rather than optimistic."""
        _plant("alpha.1", ws="alpha")

        with mock.patch.object(commands_frame, "_chat_seats") as seats, \
                mock.patch.object(reopen, "write", return_value=False):
            seats.return_value = _seats({"alpha.1": "@0"}, set())
            self.assertFalse(commands_frame.record_the_plane_now("alpha.1"))

    def test_it_does_not_collect_a_transcript_no_live_chat_names(self):
        """`reopen.prune_transcripts` is the quit's, and it has to be: a running plane's
        record is taken while chats come and go, and a sweep on that timetable would delete
        the capture of a chat that is between a close and its reopen."""
        _plant("alpha.1", ws="alpha")
        stale = reopen.transcript_path("alpha.9")
        config.write_for(stale, "an older chat's screen\n")

        with mock.patch.object(commands_frame, "_chat_seats") as seats:
            seats.return_value = _seats({"alpha.1": "@0"}, set())
            commands_frame.record_the_plane_now("alpha.1")

        self.assertTrue(stale.is_file())


def _launch_args(**kw):
    """The namespace `cmd_launch` reads, with every field it consults named rather than
    left to a `getattr` default — `_reopen_args`' own rule."""
    base = dict(harness="claude", rest=[], no_frame=False, workspace="alpha", pick=False,
                fresh=False)
    base.update(kw)
    return SimpleNamespace(**base)


class WhoRecordsAndWhoDoesNot(unittest.TestCase):
    """`commands_frame._records_the_plane` — the frame process, and nothing else.

    The gate is asked as a predicate rather than discovered inside the launcher, because
    every one of these five is a way of NOT being the operator's terminal, and a guard that
    passes only because a different guard caught the case is one this repository deletes.
    """

    def test_a_launch_that_is_the_terminal_records(self):
        with mock.patch("sys.stdout.isatty", return_value=True):
            self.assertTrue(commands_frame._records_the_plane(_launch_args()))

    def test_a_launch_that_will_never_attach_does_not(self):
        """`_open_workspace` launches from a `frame-switch` running detached with all three
        streams on `/dev/null`, and a reopen's own per-chat launches pass `attach=False`.
        Neither is anybody's terminal, and both would be a second writer."""
        with mock.patch("sys.stdout.isatty", return_value=True):
            self.assertFalse(
                commands_frame._records_the_plane(_launch_args(attach=False)))

    def test_a_probe_does_not(self):
        """`--probe` is read-only and answers before the launcher touches anything —
        `cmd_launch`'s own first line."""
        with mock.patch("sys.stdout.isatty", return_value=True):
            self.assertFalse(commands_frame._records_the_plane(_launch_args(probe=True)))

    def test_a_harness_run_bare_does_not(self):
        """`--no-frame` is an `os.execvp`: there is no frame to record and, a moment later,
        no charter left in this process to record it."""
        with mock.patch("sys.stdout.isatty", return_value=True):
            self.assertFalse(
                commands_frame._records_the_plane(_launch_args(no_frame=True)))

    def test_a_pipeline_does_not(self):
        """`charter 2>&1 | head` is a probe for "is charter installed" and `cmd_launch`
        answers it with `bypass` — an exec. Recording there would write a manifest for a
        plane this process is about to stop being part of."""
        with mock.patch("sys.stdout.isatty", return_value=False):
            self.assertFalse(commands_frame._records_the_plane(_launch_args()))

    def test_a_plane_that_turned_recording_off_does_not(self):
        with mock.patch("sys.stdout.isatty", return_value=True), \
                mock.patch.dict(config.FRAME, {"record": False}):
            self.assertFalse(commands_frame._records_the_plane(_launch_args()))

    def test_fresh_does_not_record_over_the_record_it_skipped(self):
        """The half of `--fresh` that is easy to leave out and impossible to notice
        missing: a run that declines to restore and then records anyway destroys the record
        it declined, two seconds later, with nothing said."""
        with mock.patch("sys.stdout.isatty", return_value=True):
            self.assertFalse(commands_frame._records_the_plane(_launch_args(fresh=True)))


class WhenBareCharterPutsThePlaneBack(unittest.TestCase):
    """`commands_frame._restores_the_plane` — and the one case it must never fire in."""

    def test_a_bare_launch_that_is_the_terminal_restores(self):
        self.assertTrue(commands_frame._restores_the_plane(_launch_args()))

    def test_a_launch_carrying_a_command_does_not(self):
        """`charter claude --resume <id>` and `charter frame -- <cmd>` asked for a
        particular thing. Answering with six other chats is not a restore, it is an
        override."""
        self.assertFalse(
            commands_frame._restores_the_plane(_launch_args(rest=["--resume", "x"])))

    def test_a_launch_that_will_never_attach_does_not(self):
        self.assertFalse(commands_frame._restores_the_plane(_launch_args(attach=False)))

    def test_a_plane_that_turned_restoring_off_does_not(self):
        with mock.patch.dict(config.FRAME, {"restore": False}):
            self.assertFalse(commands_frame._restores_the_plane(_launch_args()))

    def test_fresh_does_not(self):
        self.assertFalse(commands_frame._restores_the_plane(_launch_args(fresh=True)))

    def test_recording_being_off_does_not_stop_a_restore(self):
        """Two keys, and this is the pair that proves they are two: a plane that stopped
        recording still has whatever its last quit wrote, and asking `charter` to put that
        back is a coherent thing to want."""
        with mock.patch.dict(config.FRAME, {"record": False}):
            self.assertTrue(commands_frame._restores_the_plane(_launch_args()))


class TheLauncherTakesTheDecision(PersonaIso, unittest.TestCase):
    """`cmd_launch` reaching `cmd_reopen`, and the live plane it must not reach it from.

    Driven to the decision and stopped immediately after it: `state.new_chat_id` answers
    ``None``, which is `cmd_launch`'s own "could not open a chat" — the first exit past the
    branch that starts nothing and writes nothing.
    """

    def setUp(self) -> None:
        super().setUp()
        self.assertIn("edm-test-", str(config.STATE_DIR))
        reopen.write([_frame("alpha", "alpha.1")], focus="alpha")

    def _launch(self, *, chats_live=(), reopen_rc=0, **kw):
        with mock.patch.multiple(commands_frame,
                                 _live_sessions=mock.DEFAULT,
                                 _live_chats=mock.DEFAULT,
                                 _choose_workspace=mock.DEFAULT,
                                 _workspace_to_focus=mock.DEFAULT,
                                 cmd_reopen=mock.DEFAULT) as m, \
                mock.patch("charter.frame.state.new_chat_id", return_value=None), \
                mock.patch("charter.commands_frame.shutil.which",
                           side_effect=lambda n, *a, **k: f"/usr/bin/{n}"), \
                mock.patch("charter.frame.tmuxctl.version", return_value=(3, 7)), \
                mock.patch("charter.frame.tmuxctl.operator_server", return_value=None), \
                mock.patch("sys.stdout.isatty", return_value=True):
            m["_live_sessions"].return_value = set()
            m["_live_chats"].return_value = set(chats_live)
            m["_choose_workspace"].return_value = ("alpha", None, False)
            m["_workspace_to_focus"].return_value = None
            m["cmd_reopen"].return_value = reopen_rc
            rc = commands_frame.cmd_launch(_launch_args(**kw))
        return rc, m["cmd_reopen"]

    def test_bare_charter_on_a_dead_plane_puts_the_record_back(self):
        rc, reopened = self._launch()
        self.assertEqual(rc, 0)
        reopened.assert_called_once()

    def test_the_restore_is_the_quiet_one(self):
        """An operator who typed `charter reopen` is reading. An operator who typed
        `charter` wanted a terminal, so the same wall of per-chat lines there is noise at
        the worst moment — one line, and the detail on demand."""
        _rc, reopened = self._launch()
        self.assertIs(getattr(reopened.call_args[0][0], "quiet", False), True)

    def test_a_plane_with_chats_still_running_is_never_restored_over(self):
        """The defect this gate exists for. With recording on, the record is CURRENT rather
        than a relic of the last quit — so a second terminal typing `charter` while the
        plane runs would reopen every chat it can already see, and nothing on screen would
        tell the duplicates from the originals."""
        rc, reopened = self._launch(chats_live=("alpha.1",))
        self.assertEqual(rc, 1)
        reopened.assert_not_called()

    def test_a_plane_with_nothing_recorded_opens_a_chat_as_it_always_did(self):
        reopen.forget()
        rc, reopened = self._launch()
        self.assertEqual(rc, 1)
        reopened.assert_not_called()

    def test_a_restore_that_started_nothing_falls_through_to_a_launch(self):
        """Refusing to open anything because the record could not be acted on would leave
        an operator who asked for a terminal without one."""
        rc, reopened = self._launch(reopen_rc=1)
        reopened.assert_called_once()
        self.assertEqual(rc, 1)


class WhatAnAutomaticRestoreSays(PersonaIso, unittest.TestCase):
    """One compact line, detail on demand — and never silence.

    **The two halves of #752 and #823 pulling in opposite directions, settled here.**
    Silence is #752's defect exactly: a frame that quietly draws less than it should, with
    nothing to act on. A wall of per-chat lines is what `charter reopen` prints, and it is
    right there — that command is a deliberate act and the operator is reading. This one
    happens because somebody typed `charter` and wanted a terminal.

    **Each sentence is spelled out by hand**, not compared against the constant that
    produces it: a round trip through the constant moves the test with the reword and stays
    green, which is what `tests/test_what_a_quit_says_is_spelled_where_it_is_asserted.py`
    exists to stop. The duplication IS the assertion.
    """

    def setUp(self) -> None:
        super().setUp()
        self.assertIn("edm-test-", str(config.STATE_DIR))
        self.fail_for: set[str] = set()

    def _launcher(self, args):
        if args.workspace in self.fail_for:
            return 1
        args.reopening.fid = f"{args.workspace}.9"
        return 0

    def _record(self, *chats_):
        by_ws: dict = {}
        for c in chats_:
            by_ws.setdefault(c.workspace, []).append(c)
        reopen.write([reopen.Frame(workspace=w, chats=tuple(cs))
                      for w, cs in by_ws.items()], focus="alpha")

    def _chat(self, **kw):
        base = dict(chat="alpha.1", workspace="alpha", persona="", harness="claude-code",
                    cwd=str(config.ROOT), resume="", transcript="", active=True)
        base.update(kw)
        return reopen.Chat(**base)

    def _reopen(self, **kw):
        said = io.StringIO()
        with mock.patch.object(commands_frame, "cmd_launch", side_effect=self._launcher), \
                mock.patch.object(commands_frame.sys.stdout, "isatty",
                                  return_value=True), \
                mock.patch.object(commands_frame.tmuxctl, "version",
                                  return_value=(3, 7)), \
                mock.patch.object(commands_frame.tmuxctl, "operator_server",
                                  return_value=None), \
                mock.patch.object(commands_frame, "_attach_after_reopen",
                                  return_value=0), \
                redirect_stderr(said):
            rc = commands_frame.cmd_reopen(SimpleNamespace(**kw))
        return rc, said.getvalue()

    def test_a_whole_plane_that_came_back_is_one_line(self):
        self._record(self._chat(chat="alpha.1"), self._chat(chat="alpha.2"))

        rc, out = self._reopen(quiet=True)

        self.assertEqual(rc, 0)
        self.assertIn("charter: restored the 2 chat(s) this plane last recorded", out)
        self.assertEqual(len(out.strip().splitlines()), 1)

    def test_a_plane_that_came_back_short_names_the_rest_on_the_same_line(self):
        """Not silence, and not a wall: the count and the names, on one line."""
        self._record(self._chat(chat="alpha.1", workspace="alpha"),
                     self._chat(chat="beta.1", workspace="beta"))
        self.fail_for = {"beta"}

        rc, out = self._reopen(quiet=True)

        self.assertEqual(rc, 0)
        self.assertIn("charter: restored 1 of the 2 chats this plane last recorded — "
                      "beta.1 did not come back", out)
        self.assertEqual(len(out.strip().splitlines()), 1)

    def test_a_long_list_of_missing_chats_is_still_one_short_line(self):
        """A compact line stays compact on a plane that lost six workspaces."""
        self._record(self._chat(chat="alpha.1", workspace="alpha"),
                     *[self._chat(chat=f"w{i}.1", workspace=f"w{i}") for i in range(5)])
        self.fail_for = {f"w{i}" for i in range(5)}

        _rc, out = self._reopen(quiet=True)

        self.assertIn("w0.1, w1.1, w2.1 and 2 more did not come back", out)

    def test_the_chats_that_did_not_come_back_are_still_recorded(self):
        """`_consume`'s own contract, unchanged: what is left behind is the retry. What the
        line no longer does is POINT at it — this process is recording too, so the record is
        the running plane again a couple of seconds later."""
        self._record(self._chat(chat="alpha.1", workspace="alpha"),
                     self._chat(chat="beta.1", workspace="beta"))
        self.fail_for = {"beta"}

        self._reopen(quiet=True)

        self.assertEqual([c.chat for c in reopen.read().all_chats()], ["beta.1"])

    def test_charter_reopen_itself_still_says_it_chat_by_chat(self):
        """The quiet line is the AUTOMATIC restore's, and `charter reopen` is unchanged: an
        operator who asked for it by name is reading the answer."""
        self._record(self._chat(chat="alpha.1", workspace="alpha"),
                     self._chat(chat="beta.1", workspace="beta"))
        self.fail_for = {"beta"}

        rc, out = self._reopen()

        self.assertEqual(rc, 0)
        self.assertIn("charter: reopened 1 of 2 chats", out)
        self.assertIn("beta.1 did not come back", out)
        self.assertIn("`charter reopen` again to retry just those", out)


class TheRecorderIsToldWhereTheTerminalIsStanding(PersonaIso, unittest.TestCase):
    """`record.focus_on` — the manifest's `focus`, which is the workspace a reopen attaches
    to.

    **The launcher's OWN chat, and not a session name read back off tmux.** Session names
    are shared by every plane on the machine (§3.3: `default` is a name every plane has), so
    reading the focus off `list-clients` would let another plane answer for this one — the
    exact inversion `_plane_session` exists to prevent. What this costs is stated rather than
    hidden: a client that has since been moved to another workspace by a workspace tab
    leaves the focus naming the workspace this terminal launched into, and
    `_attach_after_reopen` falls back from a focus it cannot place to the chat that was on
    screen.
    """

    def setUp(self) -> None:
        super().setUp()
        self.assertIn("edm-test-", str(config.STATE_DIR))
        self.addCleanup(record.stop)

    def test_a_focused_launch_names_the_chat_it_joined(self):
        record.start(lambda _chat: True, quiet=99.0, poll=99.0)
        with mock.patch.object(commands_frame.tmuxctl, "interact",
                               return_value=_completed(0)):
            commands_frame._focus_workspace("$3", "shared.1", ws="shared", picked=False)
        self.assertEqual(record.running().chat, "shared.1")

    def test_a_reopen_names_the_chat_it_put_the_operator_back_on(self):
        record.start(lambda _chat: True, quiet=99.0, poll=99.0)
        back = [commands_frame.Reopening(reopen.Chat(
            chat="alpha.1", workspace="alpha", persona="", harness="claude-code", cwd="",
            resume="", transcript="", active=True))]
        back[0].fid = "alpha.4"
        m = reopen.Manifest(at=0, focus="alpha", frames=())
        with mock.patch.object(commands_frame.tmuxctl, "run",
                               return_value=_completed(0)), \
                mock.patch.object(commands_frame.tmuxctl, "interact",
                                  return_value=_completed(0)):
            commands_frame._attach_after_reopen(m, back)
        self.assertEqual(record.running().chat, "alpha.4")


class ReopenRecordsThePlaneItPutsBack(PersonaIso, unittest.TestCase):
    """`charter reopen` attaches, so it is the frame process for the plane it just built.

    Without this the one case that most needs a record has none: `_consume` deletes the
    record that drove the reopen, so a plane restored and then lost to a crash would have
    nothing behind it at all.
    """

    def setUp(self) -> None:
        super().setUp()
        self.assertIn("edm-test-", str(config.STATE_DIR))
        self.addCleanup(record.stop)

    def test_the_reopen_is_recording_while_it_holds_the_terminal(self):
        reopen.write([_frame("alpha", "alpha.1")], focus="alpha")
        seen: list = []

        def _launcher(args):
            args.reopening.fid = "alpha.9"
            return 0

        with mock.patch.object(commands_frame, "cmd_launch", side_effect=_launcher), \
                mock.patch.object(commands_frame.sys.stdout, "isatty",
                                  return_value=True), \
                mock.patch.object(commands_frame.tmuxctl, "version",
                                  return_value=(3, 7)), \
                mock.patch.object(commands_frame.tmuxctl, "operator_server",
                                  return_value=None), \
                mock.patch.object(commands_frame, "_attach_after_reopen",
                                  side_effect=lambda *_a: seen.append(record.running()) or 0):
            self.assertEqual(commands_frame.cmd_reopen(SimpleNamespace()), 0)

        self.assertIsNotNone(seen[0], "the reopen held the terminal and recorded nothing")

    def test_a_plane_that_turned_recording_off_reopens_without_one(self):
        reopen.write([_frame("alpha", "alpha.1")], focus="alpha")
        seen: list = []

        def _launcher(args):
            args.reopening.fid = "alpha.9"
            return 0

        with mock.patch.dict(config.FRAME, {"record": False}), \
                mock.patch.object(commands_frame, "cmd_launch", side_effect=_launcher), \
                mock.patch.object(commands_frame.sys.stdout, "isatty",
                                  return_value=True), \
                mock.patch.object(commands_frame.tmuxctl, "version",
                                  return_value=(3, 7)), \
                mock.patch.object(commands_frame.tmuxctl, "operator_server",
                                  return_value=None), \
                mock.patch.object(commands_frame, "_attach_after_reopen",
                                  side_effect=lambda *_a: seen.append(record.running()) or 0):
            commands_frame.cmd_reopen(SimpleNamespace())

        self.assertIsNone(seen[0])


class FreshIsARunThatDoesNotParticipate(PersonaIso, unittest.TestCase):
    """`charter --fresh` and `charter <harness> --fresh` — one flag, charter's own.

    **It exists because restore is automatic.** Without it, a plane an operator wanted to
    abandon comes back every time they type `charter`, and the only escape is deleting a
    file they must first know exists. What the flag means is *this run does not
    participate*: no restore, **and no recording over the record it skipped**. That second
    half is not a nicety — the alternative destroys the skipped record on a two-second
    timer, so nothing the operator does makes the loss visible until it is too late.
    """

    def _plane_defaults_to(self, harness: str) -> None:
        (self.tmp / "charter.toml").write_text(
            f'schema = 1\n\n[harness]\ndefault = "{harness}"\n')
        config.use(self.tmp)

    def test_bare_charter_fresh_becomes_the_planes_harness_carrying_the_flag(self):
        self._plane_defaults_to("claude")
        with mock.patch.object(cli.sys.stdout, "isatty", return_value=True):
            argv, rc = cli._bare_launch(["--fresh"])
        self.assertIsNone(rc)
        self.assertEqual(argv, ["claude", "--fresh"])

    def test_a_word_that_is_not_charters_own_flag_is_still_a_subcommand(self):
        """The rewrite widens by exactly one token and no further: anything else in `argv`
        is a command the operator typed, and `_bare_launch` must not touch it."""
        self._plane_defaults_to("claude")
        with mock.patch.object(cli.sys.stdout, "isatty", return_value=True):
            self.assertEqual(cli._bare_launch(["doctor"]), (["doctor"], None))
            self.assertEqual(cli._bare_launch(["--fresh", "doctor"]),
                             (["--fresh", "doctor"], None))

    def test_a_plane_that_names_no_harness_is_still_a_usage_error(self):
        """The rewrite is gated on a declared `[harness] default` exactly as bare `charter`
        is: charter does not guess a harness for somebody who never named one, and adding a
        flag does not make it start guessing."""
        (self.tmp / "charter.toml").write_text("schema = 1\n")
        config.use(self.tmp)
        with mock.patch.object(cli.sys.stdout, "isatty", return_value=True):
            self.assertEqual(cli._bare_launch(["--fresh"]), (["--fresh"], None))

    def test_the_flag_reaches_the_launcher_rather_than_the_harness(self):
        parser = cli.build_parser()
        argv, rest = cli._split_frame_argv(["claude", "--fresh"])
        args = parser.parse_args(argv)
        self.assertIs(args.fresh, True)
        self.assertEqual(rest, [])

    def test_past_the_leading_run_it_is_the_harnesss_own_word(self):
        """`_OWN_FLAGS`' rule, unchanged: once the harness's own argv has started, a token
        that happens to spell one of charter's flags is the harness's."""
        argv, rest = cli._split_frame_argv(["claude", "--resume", "x", "--fresh"])
        self.assertEqual(argv, ["claude"])
        self.assertEqual(rest, ["--resume", "x", "--fresh"])


class TheLauncherStopsAnnouncingAQuitThatDidNotHappen(PersonaIso, unittest.TestCase):
    """`_say_the_plane_is_recorded` — the neighbour this feature makes false if left alone.

    The line `cmd_launch` prints on the way out was gated on one thing: the record naming
    the chat this launch was about. While a quit was the only writer that reading WAS a
    quit, by construction. Now the frame process records the plane as it changes, so it is
    true of every chat that ever ran — the sentence had to stop claiming a quit, and it had
    to stop firing for a chat that is still running.

    **Spelled out by hand**, not compared against the constant that produces it.
    """

    def setUp(self) -> None:
        super().setUp()
        self.assertIn("edm-test-", str(config.STATE_DIR))
        reopen.write([_frame("alpha", "alpha.1", resume="conv-1")], focus="alpha")

    def _said(self, fid: str, **kw) -> str:
        buf = io.StringIO()
        with redirect_stderr(buf):
            commands_frame._say_the_plane_is_recorded(fid, **kw)
        return buf.getvalue()

    def test_it_says_the_plane_is_recorded_rather_than_that_it_was_quit(self):
        said = self._said("alpha.1", over=True)
        self.assertIn("charter: this plane is recorded — 1 chat(s) recorded, 1 with a "
                      "conversation to resume.\n  put it back with: charter reopen", said)
        self.assertNotIn("was quit", said)

    def test_a_chat_that_is_still_running_is_told_nothing(self):
        """The launcher's own next line is "detached — the harness is still running", and
        an offer to put the plane back directly above it describes a different plane."""
        self.assertEqual(self._said("alpha.1", over=False), "")

    def test_a_chat_the_record_does_not_name_is_still_told_nothing(self):
        self.assertEqual(self._said("beta.9", over=True), "")


class ALaunchRecordsForAsLongAsItHoldsTheTerminal(PersonaIso, unittest.TestCase):
    """`cmd_launch`'s wrapper — the watch starts with the launch and is gone with it.

    A whole launch with tmux faked, because the two facts under test are at opposite ends of
    it: the watch has to be running before anything is built, and the chat it names is one
    only the allocation knows.
    """

    def setUp(self) -> None:
        super().setUp()
        self.assertIn("edm-test-", str(config.STATE_DIR))
        self.addCleanup(record.stop)
        self.seen: list = []

    def _launch(self):
        def _live_chats(_socket):
            try:
                return {d.name for d in state._root().iterdir() if d.is_dir()}
            except OSError:
                return set()

        def _spawn(fid, ws):
            # The one place inside the launch that runs after the allocation and before
            # tmux, so it is where "which chat is the watch on" can be read.
            self.seen.append((record.running(), fid))

        args = SimpleNamespace(harness="claude", rest=[], no_frame=False,
                               workspace="alpha", pick=False, fresh=False)
        with mock.patch.object(commands_frame.tmuxctl, "version", return_value=(3, 7)), \
                mock.patch.object(commands_frame.tmuxctl, "operator_server",
                                  return_value=None), \
                mock.patch.object(commands_frame.shutil, "which", return_value="/bin/true"), \
                mock.patch.object(commands_frame.sys.stdout, "isatty", return_value=True), \
                mock.patch("sys.stdin.isatty", return_value=False), \
                mock.patch.object(commands_frame, "_live_sessions", return_value=set()), \
                mock.patch.object(commands_frame, "_live_chats", side_effect=_live_chats), \
                mock.patch.object(commands_frame, "_spawn_gather", side_effect=_spawn), \
                mock.patch.object(commands_frame, "_workspace_to_focus",
                                  return_value=None), \
                mock.patch.object(commands_frame, "_draw_panels", return_value={}), \
                mock.patch.object(commands_frame, "_arm_panel_respawn"), \
                mock.patch.object(commands_frame, "_query_pane_dead_status",
                                  return_value=None), \
                mock.patch.object(commands_frame, "_chat_being_left", return_value=""), \
                mock.patch.object(commands_frame, "_drop_panels"), \
                mock.patch.object(commands_frame.tmuxctl, "run",
                                  return_value=_completed(0)), \
                mock.patch.object(commands_frame.tmuxctl, "interact",
                                  return_value=_completed(0)):
            return commands_frame.cmd_launch(args)

    def test_the_launch_is_recording_before_it_builds_anything(self):
        self._launch()
        self.assertTrue(self.seen)
        self.assertIsNotNone(self.seen[0][0], "the launch built a frame and recorded none")

    def test_the_record_names_the_chat_this_launch_opened(self):
        self._launch()
        watch, fid = self.seen[0]
        self.assertEqual(watch.chat, fid)

    def test_the_watch_is_gone_when_the_launch_is(self):
        """It has to be: the last thing a launch does is `state.reap`, which removes the
        directories the watch reads."""
        self._launch()
        self.assertIsNone(record.running())
        self.assertFalse(self.seen[0][0].alive())
