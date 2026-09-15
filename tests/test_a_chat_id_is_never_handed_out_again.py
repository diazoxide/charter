"""#1101: a chat id names one chat for the life of the plane, and is never handed out again.

`state.new_chat_id` used to claim the lowest free ordinal, and a reap freed an ordinal with
its directory. Two things keyed by the id live OUTSIDE that directory — the captured
`<id>.transcript` and the quit record's entry — so a new `default.1` was offered an
unrelated chat's scrollback, and closing it deleted the earlier chat's record.

The counter now starts above a per-prefix high-water mark (`.charter/frame/chat-ids.json`)
and above every trace an id still leaves on disk, and the mark is written before the claim.
The `mkdir` still decides who wins.
"""

from __future__ import annotations

import fcntl
import json
import os
import shutil
import subprocess
import threading
import unittest
from pathlib import Path
from types import SimpleNamespace
from unittest import mock

from charter import commands_frame, config
from charter.frame import chats, leave, reopen, state, tmuxctl
from tests._isolation import PersonaIso, wired_as_today


def _mark_file() -> Path:
    return Path(config.STATE_DIR) / "frame" / "chat-ids.json"


def _write_mark(value) -> None:
    root = Path(config.STATE_DIR) / "frame"
    root.mkdir(parents=True, exist_ok=True)
    (root / "chat-ids.json").write_text(json.dumps(value))


class TheCounterOnlyGrows(PersonaIso, unittest.TestCase):

    def test_a_reaped_ordinal_is_not_handed_out_again(self):
        first = state.new_chat_id("beta")
        self.assertEqual(first, "beta.1")
        shutil.rmtree(state.frame_dir(first))
        self.assertEqual(state.new_chat_id("beta"), "beta.2")

    def test_the_mark_is_a_file_in_the_frame_root(self):
        state.new_chat_id("beta")
        self.assertTrue(_mark_file().is_file())
        self.assertEqual(json.loads(_mark_file().read_text()), {"beta": 1})

    def test_one_mark_per_prefix_not_per_workspace_name(self):
        """`a.b` and `a_b` mint the same prefix, so two marks keyed by NAME would each
        hand out `a_b.1`."""
        self.assertEqual(state.new_chat_id("a.b"), "a_b.1")
        shutil.rmtree(state.frame_dir("a_b.1"))
        self.assertEqual(state.new_chat_id("a_b"), "a_b.2")

    def test_a_lost_mark_falls_back_to_the_traces(self):
        state.new_chat_id("beta")
        state.new_chat_id("beta")
        _mark_file().unlink()
        shutil.rmtree(state.frame_dir("beta.1"))
        shutil.rmtree(state.frame_dir("beta.2"))
        (state._root() / "beta.2.transcript").write_text("scrollback\n")
        self.assertEqual(state.new_chat_id("beta"), "beta.3")

    def test_an_unreadable_mark_still_counts_the_traces(self):
        _write_mark({})
        _mark_file().write_text("not json")
        (state._root() / "beta.7").mkdir()
        self.assertEqual(state.new_chat_id("beta"), "beta.8")

    def test_a_mark_that_is_not_a_positive_number_reads_as_nothing(self):
        for value in ({"beta": "9"}, {"beta": -4}, ["beta", 9]):
            with self.subTest(value=value):
                _write_mark(value)
                self.assertEqual(state.highest_ordinal("beta"), 0)

    def test_the_mark_never_goes_down(self):
        _write_mark({"beta": 9, "gamma": 2})
        with state._locked(state._root()):
            state._raise_mark("beta", 3)
        self.assertEqual(json.loads(_mark_file().read_text()), {"beta": 9, "gamma": 2})

    def test_the_mark_is_written_before_the_directory(self):
        real = config.claim_private_dir
        seen: list[tuple[str, int]] = []

        def claim(d):
            n = int(Path(d).name.rpartition(".")[2])
            seen.append((Path(d).name, json.loads(_mark_file().read_text())["beta"]))
            self.assertGreaterEqual(seen[-1][1], n, "the directory was claimed first")
            return real(d)

        with mock.patch.object(state.config, "claim_private_dir", side_effect=claim):
            self.assertEqual(state.new_chat_id("beta"), "beta.1")
        self.assertEqual(seen, [("beta.1", 1)])

    def test_a_mark_that_cannot_be_written_hands_out_nothing(self):
        real = config.replace_for

        def replace(p, data):
            if Path(p).name == "chat-ids.json":
                raise OSError(28, "No space left on device")
            return real(p, data)

        with mock.patch.object(state.config, "replace_for", side_effect=replace):
            self.assertIsNone(state.new_chat_id("beta"))
        self.assertEqual([p.name for p in state._root().iterdir() if p.is_dir()], [])

    def test_the_lock_is_opened_through_config(self):
        log: list[tuple[str, str]] = []
        real_open, real_replace = config.open_for, config.replace_for

        def opened(p, *a, **kw):
            log.append(("open", Path(p).name))
            return real_open(p, *a, **kw)

        def replaced(p, data):
            log.append(("replace", Path(p).name))
            return real_replace(p, data)

        with mock.patch.object(state.config, "open_for", side_effect=opened), \
                mock.patch.object(state.config, "replace_for", side_effect=replaced):
            self.assertEqual(state.new_chat_id("beta"), "beta.1")
        self.assertIn(("open", "chat-ids.lock"), log)
        self.assertLess(log.index(("open", "chat-ids.lock")),
                        log.index(("replace", "chat-ids.json")))

    def test_the_attempt_bound_counts_from_the_start(self):
        """`_CHAT_ORDINAL_MAX` bounds the attempts, not the ordinal: a counter that never
        goes down would otherwise turn it into a lifetime limit per workspace."""
        _write_mark({"beta": 20000})
        self.assertEqual(state.new_chat_id("beta"), "beta.20001")

    def test_no_ordinal_past_the_ceiling_is_handed_out(self):
        """An ordinal of more digits than a strip can sort (`chats._MAX_ORDINAL_DIGITS`) is
        one no trace scan could count, so handing it out would reopen #1101."""
        _write_mark({"beta": 99999})
        self.assertIsNone(state.new_chat_id("beta"))
        self.assertFalse((state._root() / "beta.100000").exists())

    def test_the_ceiling_is_what_a_strip_can_sort(self):
        self.assertEqual(state.ORDINAL_CEILING, 99_999)
        self.assertEqual(chats._MAX_ORDINAL_DIGITS, len(str(state.ORDINAL_CEILING)))

    def test_a_taken_ordinal_is_still_skipped(self):
        (state._root()).mkdir(parents=True, exist_ok=True)
        real = config.claim_private_dir
        calls: list[str] = []

        def claim(d):
            calls.append(Path(d).name)
            if Path(d).name == "beta.1":
                raise FileExistsError(d)
            return real(d)

        with mock.patch.object(state.config, "claim_private_dir", side_effect=claim):
            self.assertEqual(state.new_chat_id("beta"), "beta.2")
        self.assertEqual(calls, ["beta.1", "beta.2"])

    def test_giving_up_is_bounded_by_attempts(self):
        with mock.patch.object(state, "_CHAT_ORDINAL_MAX", 2), \
                mock.patch.object(state.config, "claim_private_dir",
                                  side_effect=FileExistsError("taken")) as claim:
            self.assertIsNone(state.new_chat_id("beta"))
        self.assertEqual(claim.call_count, 2)


class EveryTraceCounts(PersonaIso, unittest.TestCase):
    """With no mark at all, the counter starts above every leftover an id still has."""

    def setUp(self) -> None:
        super().setUp()
        state._root().mkdir(parents=True, exist_ok=True)
        Path(config.SESSIONS_DIR).mkdir(parents=True, exist_ok=True)

    def test_a_chat_directory(self):
        (state._root() / "beta.4").mkdir()
        self.assertEqual(state.new_chat_id("beta"), "beta.5")

    def test_a_transcript(self):
        (state._root() / "beta.6.transcript").write_text("x")
        self.assertEqual(state.new_chat_id("beta"), "beta.7")

    def test_the_quit_record(self):
        c = reopen.Chat(chat="beta.9", workspace="beta", persona="", harness="claude-code",
                        cwd="", resume="", transcript="", active=False)
        self.assertTrue(reopen.write([reopen.Frame(workspace="beta", chats=(c,))],
                                     focus="beta"))
        self.assertEqual(state.new_chat_id("beta"), "beta.10")

    def test_a_quit_record_this_charter_cannot_read_still_counts(self):
        """`reopen.read` answers `None` for a version it does not speak, and that file
        still names ids a transcript carries."""
        reopen.path().write_text(json.dumps(
            {"version": 99, "frames": [{"chats": [{"chat": "beta.12"}, "junk", 4,
                                                  {"chat": 40}, {"chat": "gamma.30"}]},
                                       "junk", {"chats": "junk"}]}))
        self.assertIsNone(reopen.read())
        self.assertEqual(state.new_chat_id("beta"), "beta.13")

    def test_a_quit_record_that_is_not_an_object_counts_nothing(self):
        for raw in ("not json", "[1, 2]", '{"frames": "x"}'):
            with self.subTest(raw=raw):
                reopen.path().write_text(raw)
                self.assertEqual(state.highest_ordinal("beta"), 0)

    def test_a_session_marker(self):
        (Path(config.SESSIONS_DIR) / "beta.3.workspace").write_text("beta\n")
        self.assertEqual(state.new_chat_id("beta"), "beta.4")

    def test_a_marker_family_with_dots_of_its_own_counts(self):
        """`hooks`' `.<tool-use-id>.<kind>.ask-pending` family carries dots after the
        ordinal, so the ordinal is the segment right after the prefix, not the last one."""
        (Path(config.SESSIONS_DIR) / "beta.5.toolu_a1.PreToolUse.ask-pending").write_text("x")
        self.assertEqual(state.new_chat_id("beta"), "beta.6")

    def test_the_bare_id_in_the_sessions_directory_is_not_a_marker(self):
        """`_forget_session` sweeps `<id>.<family>` and nothing else, so neither does this."""
        (Path(config.SESSIONS_DIR) / "beta.4").write_text("x")
        self.assertEqual(state.new_chat_id("beta"), "beta.1")

    def test_names_that_are_not_this_prefix_do_not_count(self):
        for name in ("betamax.9", "beta-9", "beta.x", "beta.9x", "alpha.beta.9"):
            (state._root() / name).mkdir()
        (state._root() / "gamma.8.transcript").write_text("x")
        (Path(config.SESSIONS_DIR) / "betamax.7.workspace").write_text("x")
        (Path(config.SESSIONS_DIR) / "beta").write_text("x")
        self.assertEqual(state.new_chat_id("beta"), "beta.1")

    def test_a_name_past_the_digit_bound_is_ignored_not_raised(self):
        (state._root() / "beta.123456").mkdir()
        (state._root() / ("beta." + "9" * 240)).mkdir()
        # Past `int()`'s own 4,300-digit refusal, which only the quit record can carry.
        reopen.path().write_text(json.dumps(
            {"version": 1, "frames": [{"chats": [{"chat": "beta." + "9" * 5000}]}]}))
        (Path(config.SESSIONS_DIR) / "beta.9999999999999999999999.lock").write_text("x")
        (state._root() / "beta.²").mkdir()   # a digit `str.isdigit` admits and `int` refuses
        self.assertEqual(state.new_chat_id("beta"), "beta.1")

    def test_the_largest_trace_wins(self):
        (state._root() / "beta.2").mkdir()
        (state._root() / "beta.11.transcript").write_text("x")
        (Path(config.SESSIONS_DIR) / "beta.5.persona").write_text("x")
        self.assertEqual(state.new_chat_id("beta"), "beta.12")

    def test_chat_turns_is_not_a_trace(self):
        """`inflight._turn_file` is keyed by the id too, but it stands for a turn at most
        ten minutes and marks a spinner — never a record a new chat could be offered."""
        turns = Path(config.STATE_DIR) / "chat-turns"
        turns.mkdir(parents=True, exist_ok=True)
        (turns / "beta.8").write_text("x")
        self.assertEqual(state.new_chat_id("beta"), "beta.1")

    def test_an_unlistable_frame_root_or_sessions_dir_is_no_trace_and_no_raise(self):
        with mock.patch.object(state.os, "scandir", side_effect=OSError(13, "denied")):
            self.assertEqual(state.highest_ordinal("beta"), 0)


class TwoAllocatorsNeverShareAnId(PersonaIso, unittest.TestCase):

    def test_eight_threads_five_allocations_each(self):
        got: list[str | None] = []
        guard = threading.Lock()
        barrier = threading.Barrier(8)

        def run():
            barrier.wait()
            for _ in range(5):
                fid = state.new_chat_id("beta")
                with guard:
                    got.append(fid)

        threads = [threading.Thread(target=run) for _ in range(8)]
        for t in threads:
            t.start()
        for t in threads:
            t.join()
        self.assertNotIn(None, got)
        self.assertEqual(len(set(got)), 40)
        self.assertEqual(json.loads(_mark_file().read_text()), {"beta": 40})

    def _held(self) -> bool:
        """Whether another open file description is refused the lock right now."""
        fd = os.open(state._root() / "chat-ids.lock", os.O_RDONLY)
        try:
            fcntl.flock(fd, fcntl.LOCK_EX | fcntl.LOCK_NB)
        except BlockingIOError:
            return True
        finally:
            os.close(fd)
        return False

    def test_the_mark_is_raised_while_the_lock_is_held(self):
        held: list[bool] = []
        real_replace = config.replace_for

        def replaced(p, data):
            if Path(p).name == "chat-ids.json":
                held.append(self._held())
            return real_replace(p, data)

        with mock.patch.object(state.config, "replace_for", side_effect=replaced):
            self.assertEqual(state.new_chat_id("beta"), "beta.1")
            self.assertTrue(state.claim_chat_id("beta.7"))
        self.assertEqual(held, [True, True], "the mark was written with the lock free")
        self.assertFalse(self._held(), "the lock outlived the allocation")

    def test_a_lock_that_cannot_be_opened_hands_out_nothing(self):
        with mock.patch.object(state.config, "open_for", side_effect=OSError(13, "denied")):
            self.assertIsNone(state.new_chat_id("beta"))
            self.assertFalse(state.claim_chat_id("beta.7"))
        self.assertEqual([p for p in state._root().iterdir() if p.is_dir()], [])

    def test_a_lock_that_cannot_be_taken_hands_out_nothing(self):
        with mock.patch.object(state.fcntl, "flock", side_effect=OSError(46, "no locks")):
            self.assertIsNone(state.new_chat_id("beta"))
        self.assertEqual([p for p in state._root().iterdir() if p.is_dir()], [])


class NoScanReadsTheMarkAsAChat(PersonaIso, unittest.TestCase):
    """The mark and its lock are FILES in the frame root, and every scan of that root reads
    a directory as a chat."""

    def setUp(self) -> None:
        super().setUp()
        self.fid = state.new_chat_id("beta")
        self.files = [state._root() / "chat-ids.json", state._root() / "chat-ids.lock"]
        for f in self.files:
            self.assertTrue(f.is_file(), f)

    def test_the_chat_lists_do_not_hold_them(self):
        self.assertEqual(leave.plane_chats(), [self.fid])
        listed = [n for names in chats._by_workspace().values() for n in names]
        self.assertNotIn("chat-ids.json", listed)
        self.assertNotIn("chat-ids.lock", listed)

    def test_a_reap_of_either_server_leaves_them(self):
        state.reap(set(), server=tmuxctl.LEGACY_SOCKET)
        state.reap(set(), server="x")
        for f in self.files:
            self.assertTrue(f.is_file(), f)

    def test_pruning_transcripts_leaves_them(self):
        reopen.prune_transcripts(set())
        for f in self.files:
            self.assertTrue(f.is_file(), f)


class ARecordedIdIsClaimedExactly(PersonaIso, unittest.TestCase):

    def test_claiming_a_recorded_id_makes_exactly_that_directory(self):
        self.assertTrue(state.claim_chat_id("beta.7"))
        d = state._root() / "beta.7"
        self.assertTrue(d.is_dir())
        self.assertEqual((d / "launcher").read_text().strip(), str(os.getpid()))
        self.assertGreaterEqual(json.loads(_mark_file().read_text())["beta"], 7)
        self.assertEqual(state.new_chat_id("beta"), "beta.8")

    def test_a_claim_of_a_taken_id_fails(self):
        (state._root()).mkdir(parents=True, exist_ok=True)
        (state._root() / "beta.7").mkdir()
        self.assertFalse(state.claim_chat_id("beta.7"))

    def test_a_claim_whose_mark_cannot_be_written_makes_nothing(self):
        real = config.replace_for

        def replace(p, data):
            if Path(p).name == "chat-ids.json":
                raise OSError(28, "No space left on device")
            return real(p, data)

        with mock.patch.object(state.config, "replace_for", side_effect=replace):
            self.assertFalse(state.claim_chat_id("beta.7"))
        self.assertFalse((state._root() / "beta.7").exists())

    def test_a_claim_the_filesystem_refuses_is_false_not_a_raise(self):
        with mock.patch.object(state.config, "claim_private_dir",
                               side_effect=OSError(13, "Permission denied")):
            self.assertFalse(state.claim_chat_id("beta.7"))

    def test_a_name_that_is_not_a_chat_id_is_not_claimed(self):
        for name in ("../x", "beta-7", "beta", "beta.7.transcript", "a.b.7", ".7",
                     "-beta.7", "beta.0", "beta.100000", "beta.²", "7", ""):
            with self.subTest(name=name):
                self.assertFalse(state.claim_chat_id(name))
        self.assertFalse(state._root().exists()
                         and any(p.is_dir() for p in state._root().iterdir()))

    def test_the_ordinal_is_read_after_the_last_dot_within_the_bound(self):
        self.assertEqual(state.ordinal_of("beta.7"), 7)
        self.assertEqual(state.ordinal_of("beta.99999"), 99_999)
        # After the LAST dot: a pre-#695 id could carry a dot in its prefix, and its ordinal
        # is still its tail (whether charter may keep such an id is `_mintable`'s question).
        self.assertEqual(state.ordinal_of("a.b.7"), 7)
        for name in ("beta.0", "beta.100000", "beta", "beta.x", "beta.²", "", "7"):
            with self.subTest(name=name):
                self.assertIsNone(state.ordinal_of(name))

    def test_an_ordinary_launch_never_takes_a_recorded_id(self):
        c = reopen.Chat(chat="beta.3", workspace="beta", persona="", harness="claude-code",
                        cwd="", resume="", transcript="", active=False)
        reopen.write([reopen.Frame(workspace="beta", chats=(c,))], focus="beta")
        self.assertEqual(state.new_chat_id("beta"), "beta.4")

    def test_reaping_a_chat_takes_its_directory_and_its_session_markers(self):
        self.assertTrue(state.claim_chat_id("beta.7"))
        Path(config.SESSIONS_DIR).mkdir(parents=True, exist_ok=True)
        (Path(config.SESSIONS_DIR) / "beta.7.workspace").write_text("beta\n")
        (Path(config.SESSIONS_DIR) / "beta.70.workspace").write_text("beta\n")
        state.reap_chat("beta.7")
        self.assertFalse((state._root() / "beta.7").exists())
        self.assertFalse((Path(config.SESSIONS_DIR) / "beta.7.workspace").exists())
        self.assertTrue((Path(config.SESSIONS_DIR) / "beta.70.workspace").exists())

    def test_a_reap_never_aims_at_a_name_that_is_not_a_chat_id(self):
        """A reap is `rmtree`. Handed a name off a hand-edited manifest, it must not reach
        anything the frame root holds that is not a chat's own directory."""
        state._root().mkdir(parents=True, exist_ok=True)
        for name in ("beta", "beta-7", "frame"):
            (state._root() / name).mkdir()
            (state._root() / name / "keep").write_text("x")
        Path(config.SESSIONS_DIR).mkdir(parents=True, exist_ok=True)
        (Path(config.SESSIONS_DIR) / "beta.workspace").write_text("x")
        for name in ("beta", "beta-7", "frame", "..", ""):
            with self.subTest(name=name):
                state.reap_chat(name)
        for name in ("beta", "beta-7", "frame"):
            self.assertTrue((state._root() / name / "keep").is_file(), name)
        self.assertTrue((Path(config.SESSIONS_DIR) / "beta.workspace").is_file())


THIS_PLANE = "/this/plane"


class ARestoredChatKeepsItsId(PersonaIso, unittest.TestCase):
    """A reopen claims exactly the recorded id. A directory still standing there is asked
    about with ONE listing on the chat's recorded server before anything is reaped, and a
    server that did not answer proves nothing (#1100's rule)."""

    def setUp(self) -> None:
        super().setUp()
        self.rec = reopen.Chat(chat="beta.7", workspace="beta", persona="",
                               harness="claude-code", cwd="", resume="", transcript="",
                               active=False)
        self.enterContext(mock.patch.object(commands_frame, "_this_plane",
                                            return_value=THIS_PLANE))
        self.rows = self.enterContext(mock.patch.object(commands_frame, "_chat_pane_rows",
                                                        return_value=[]))
        self.gone = self.enterContext(mock.patch.object(commands_frame.tmuxctl,
                                                        "nothing_listening",
                                                        return_value=False))
        Path(config.SESSIONS_DIR).mkdir(parents=True, exist_ok=True)

    def _surviving(self, chat="beta.7", server="srv"):
        state.record_server(chat, server)
        state.record_workspace(chat, "beta")
        (Path(config.SESSIONS_DIR) / f"{chat}.workspace").write_text("beta\n")
        return state.frame_dir(chat)

    def test_a_directory_nobody_holds_is_claimed_without_asking_tmux(self):
        self.assertEqual(commands_frame._claim_kept_id(self.rec), ("beta.7", ""))
        self.rows.assert_not_called()

    def test_a_surviving_directory_no_pane_proves_live_is_reaped_and_claimed(self):
        d = self._surviving()
        self.assertEqual(commands_frame._claim_kept_id(self.rec), ("beta.7", ""))
        self.rows.assert_called_once_with("srv")
        self.assertTrue(d.is_dir())
        self.assertFalse((d / "server").exists(), "the directory is the new claim's")
        self.assertEqual((d / "launcher").read_text().strip(), str(os.getpid()))
        self.assertFalse((Path(config.SESSIONS_DIR) / "beta.7.workspace").exists())

    def test_a_directory_with_no_server_record_is_asked_about_on_the_legacy_server(self):
        self._surviving()
        (state.frame_dir("beta.7") / "server").unlink()
        commands_frame._claim_kept_id(self.rec)
        self.rows.assert_called_once_with(tmuxctl.LEGACY_SOCKET)

    def test_a_server_that_timed_out_refuses_and_leaves_the_directory(self):
        d = self._surviving()
        before = {p.name: p.read_bytes() for p in d.iterdir()}
        self.rows.return_value = None
        with mock.patch.object(state, "reap_chat") as reap:
            fid, why = commands_frame._claim_kept_id(self.rec)
        self.assertIsNone(fid)
        self.assertIn(str(d), why)
        self.assertIn("did not answer", why)
        reap.assert_not_called()
        self.assertEqual({p.name: p.read_bytes() for p in d.iterdir()}, before)
        self.gone.assert_called_once_with("srv")

    def test_a_server_that_is_gone_allows_the_reap_for_that_chat_only(self):
        self._surviving("beta.7")
        other = self._surviving("beta.8")
        self.rows.return_value = None
        self.gone.return_value = True
        self.assertEqual(commands_frame._claim_kept_id(self.rec), ("beta.7", ""))
        self.assertTrue((other / "server").is_file())
        self.assertTrue((Path(config.SESSIONS_DIR) / "beta.8.workspace").is_file())

    def test_a_server_that_answers_without_the_chat_allows_the_reap(self):
        self._surviving()
        self.rows.return_value = [["gamma.2", "@1", "1", THIS_PLANE, "%2"]]
        self.assertEqual(commands_frame._claim_kept_id(self.rec), ("beta.7", ""))

    def test_another_planes_pane_is_not_this_chats_life(self):
        self._surviving()
        self.rows.return_value = [["beta.7", "@3", "1", "/other/plane", "%4"]]
        self.assertEqual(commands_frame._claim_kept_id(self.rec), ("beta.7", ""))

    def test_an_unmarked_pane_is_not_this_chats_life(self):
        self._surviving()
        self.rows.return_value = [["beta.7", "@3", "1", "", "%4"]]
        self.assertEqual(commands_frame._claim_kept_id(self.rec), ("beta.7", ""))

    def test_a_live_pane_of_this_plane_refuses_naming_the_directory_and_the_command(self):
        d = self._surviving()
        self.rows.return_value = [["gamma.2", "@1", "1", THIS_PLANE, "%2"],
                                  ["beta.7", "@3", "1", THIS_PLANE, "%4"]]
        with mock.patch.object(state, "reap_chat") as reap:
            fid, why = commands_frame._claim_kept_id(self.rec)
        self.assertIsNone(fid)
        self.assertIn(str(d), why)
        self.assertIn("running", why)
        self.assertIn("kill-window -t @3", why)
        self.assertIn("srv", why)
        reap.assert_not_called()
        self.assertTrue((d / "server").is_file())

    def test_a_live_launcher_claim_refuses_without_asking_tmux(self):
        d = self._surviving()
        (d / "launcher").write_text(f"{os.getpid()}\n")
        fid, why = commands_frame._claim_kept_id(self.rec)
        self.assertIsNone(fid)
        self.assertIn("being started", why)
        self.rows.assert_not_called()

    def test_a_dead_launcher_claim_is_asked_about_like_any_other(self):
        d = self._surviving()
        (d / "launcher").write_text("999999999\n")
        self.assertEqual(commands_frame._claim_kept_id(self.rec), ("beta.7", ""))
        self.rows.assert_called_once()

    def test_a_name_that_is_not_a_chat_id_asks_nothing_and_reaps_nothing(self):
        state._root().mkdir(parents=True, exist_ok=True)
        (state._root() / "beta").mkdir()
        (state._root() / "beta" / "keep").write_text("x")
        for name in ("beta", "..", "beta.x"):
            with self.subTest(name=name):
                fid, why = commands_frame._claim_kept_id(self.rec._replace(chat=name))
                self.assertIsNone(fid)
                self.assertIn("could not claim", why)
        self.rows.assert_not_called()
        self.assertTrue((state._root() / "beta" / "keep").is_file())

    def test_the_directory_a_refusal_names_is_contained(self):
        """The plane's state path is the operator's, and a sentence carrying it goes to a
        terminal (ruling 35): an ESC in it is shown, never obeyed."""
        state._root().mkdir(parents=True, exist_ok=True)
        (state._root() / "beta.7").mkdir()
        hostile = Path("/plane\x1b[2J/.charter/frame/beta.7")
        self.rows.return_value = [["beta.7", "@3", "1", THIS_PLANE, "%4"]]
        with mock.patch.object(state, "frame_dir", return_value=hostile):
            fid, why = commands_frame._claim_kept_id(self.rec)
        self.assertIsNone(fid)
        self.assertNotIn("\x1b", why)
        self.assertIn("\\u001b", why)

    def test_the_server_in_the_command_a_refusal_names_is_contained(self):
        """The server comes out of the chat's `server` record, which a chat can write, and it
        is spelled into a command the operator is told to run."""
        self._surviving(server="srv\x1b[2J")
        self.rows.return_value = [["beta.7", "@3", "1", THIS_PLANE, "%4"]]
        fid, why = commands_frame._claim_kept_id(self.rec)
        self.assertIsNone(fid)
        self.assertIn("kill-window -t @3", why)
        self.assertNotIn("\x1b", why)

    def test_the_taken_sentence_is_these_words(self):
        """Spelled out by hand, and the cases above cannot do it for this one: each of them
        asserts the `what` and the `fix` it passed IN, so the template's own prose — what an
        operator reads between them — was carried by no assertion at all. Measured: a retune
        of this literal left every case green."""
        self.assertEqual(
            commands_frame.KEPT_ID_TAKEN.format(chat="beta.7", dir="/p/beta.7",
                                                what="running", fix="close it"),
            "charter reopen: beta.7 is not reopened — /p/beta.7 belongs to a chat that is "
            "still running, so opening it would give two chats one id. It stays recorded; "
            "close it, then run charter reopen again.")

    def test_the_unclaimed_sentence_is_these_words(self):
        """Spelled out by hand, so a reword is visible (`test_what_a_quit_says_is_spelled_
        where_it_is_asserted`'s rule): it is what an operator reads when a reopen could not
        claim a chat's own directory."""
        self.assertEqual(
            commands_frame.KEPT_ID_UNCLAIMED.format(chat="beta.7", dir="/p/beta.7"),
            "charter reopen: beta.7 is not reopened — charter could not claim /p/beta.7 for "
            "it. It stays recorded; check that the plane's state directory can be written, "
            "then run charter reopen again.")

    def test_a_claim_that_still_fails_after_the_reap_says_so(self):
        self._surviving()
        with mock.patch.object(state, "claim_chat_id", return_value=False):
            fid, why = commands_frame._claim_kept_id(self.rec)
        self.assertIsNone(fid)
        self.assertIn("could not claim", why)

    def test_an_ordinary_launch_never_takes_a_recorded_id(self):
        reopen.write([reopen.Frame(workspace="beta", chats=(self.rec._replace(chat="beta.3"),))],
                     focus="beta")
        self.assertEqual(state.new_chat_id("beta"), "beta.4")


class AReopenLaunchKeepsItsId(PersonaIso, unittest.TestCase):
    """The real `_launch`, with tmux and every process it would start stood in — the patch
    set `test_a_chat_opens_in_the_background_with_its_first_message` drives it with."""

    def setUp(self) -> None:
        super().setUp()
        wired_as_today(self)
        self.enterContext(mock.patch.dict(os.environ, {}, clear=True))
        self.enterContext(mock.patch.object(commands_frame, "_this_plane",
                                            return_value=THIS_PLANE))
        self.t = self.tmp / "t.jsonl"
        self.t.write_text("{}\n")
        self.argvs: list[list[str]] = []
        self.at_window: dict = {}
        self.said: list[str] = []

    def _launch(self, rec, *, rows=(), resume=True):
        r = commands_frame.Reopening(rec)
        args = SimpleNamespace(harness="claude", profile="claude", rest=[], no_frame=False,
                               workspace="beta", pick=False, reopening=r, resume=resume,
                               size=(120, 40))

        def run(action, argv, **kw):
            self.argvs.append(list(argv))
            if "new-window" in argv or "new-session" in argv:
                self.at_window = dict(link=state.kept_harness_session(rec.chat),
                                      conversation=state.conversation(rec.chat))
            return subprocess.CompletedProcess(
                argv, 0, "%9\n" if ("new-window" in argv or "new-session" in argv) else "", "")

        def write_all(joint, writes, **kw):
            return [subprocess.CompletedProcess(w.argv, 0, "", "") for w in writes]

        with mock.patch("charter.commands_frame.shutil.which", return_value="/nowhere/claude"), \
                mock.patch("sys.stdout.isatty", return_value=True), \
                mock.patch("sys.stdin.isatty", return_value=False), \
                mock.patch.object(commands_frame.tmuxctl, "version", return_value=(3, 7)), \
                mock.patch.object(commands_frame.tmuxctl, "operator_server", return_value=None), \
                mock.patch.object(commands_frame, "_live_sessions", return_value={"beta"}), \
                mock.patch.object(commands_frame, "_live_chats", return_value=set()), \
                mock.patch.object(commands_frame, "_chat_pane_rows", return_value=list(rows)), \
                mock.patch.object(commands_frame.tmuxctl, "run", side_effect=run), \
                mock.patch.object(commands_frame.tmuxctl, "write_all", side_effect=write_all), \
                mock.patch.object(commands_frame.tmuxctl, "interact",
                                  return_value=subprocess.CompletedProcess([], 0)), \
                mock.patch.object(commands_frame, "_query_pane_dead_status", return_value=None), \
                mock.patch.object(commands_frame, "_draw_panels", return_value={}), \
                mock.patch.object(commands_frame, "_arm_panel_respawn"), \
                mock.patch.object(commands_frame, "_spawn_gather"), \
                mock.patch.object(commands_frame, "_chat_being_left", return_value="beta.0"), \
                mock.patch.object(commands_frame, "_drop_panels"), \
                mock.patch.object(commands_frame.util, "err", side_effect=self.said.append), \
                mock.patch.object(commands_frame.state, "reap"):
            rc = commands_frame._launch(args)
        return rc, r

    def _rec(self, **kw):
        base = dict(chat="beta.7", workspace="beta", persona="", harness="claude-code", cwd="",
                    resume="e8962ccc-d263-4996-9881-c774dc586d3f", transcript="",
                    active=False, profile="claude", conversation=str(self.t))
        base.update(kw)
        return reopen.Chat(**base)

    def test_a_reopen_launch_claims_the_recorded_id(self):
        rc, r = self._launch(self._rec())
        self.assertEqual(rc, 0, self.said)
        self.assertEqual(r.fid, "beta.7")
        windows = [a for a in self.argvs if "new-window" in a or "new-session" in a]
        self.assertTrue(windows, self.argvs)
        self.assertTrue(any("beta.7" in w for w in windows[0]), windows[0])

    def test_a_restored_chat_has_its_link_and_conversation_before_tmux(self):
        self._launch(self._rec())
        self.assertEqual(self.at_window, dict(link="e8962ccc-d263-4996-9881-c774dc586d3f",
                                              conversation=str(self.t)))

    def test_the_resume_flag_rides_tmux_and_the_link_does_not(self):
        self._launch(self._rec())
        joined = [w for a in self.argvs for w in a]
        self.assertIn("--resume", joined)
        self.assertFalse(any("e8962ccc" in w for w in joined), self.argvs)

    def test_a_reopen_that_is_not_resuming_asks_for_no_resume(self):
        self._launch(self._rec(), resume=False)
        self.assertNotIn("--resume", [w for a in self.argvs for w in a])

    def test_a_refused_reopen_says_why_and_stays_recorded(self):
        state.record_server("beta.7", "srv")
        rec = self._rec()
        reopen.write([reopen.Frame(workspace="beta", chats=(rec,))], focus="beta")
        rc, r = self._launch(rec, rows=[["beta.7", "@3", "1", THIS_PLANE, "%4"]])
        self.assertEqual(rc, 1)
        self.assertEqual(r.fid, "")
        self.assertTrue(any("kill-window -t @3" in s for s in self.said), self.said)
        self.assertFalse([a for a in self.argvs if "new-window" in a or "new-session" in a])
        commands_frame._consume(reopen.read(), set(), quiet=True)
        self.assertEqual([c.chat for c in reopen.read().all_chats()], ["beta.7"])


class AClosedChatsLeftoversAreNeverInherited(PersonaIso, unittest.TestCase):
    """#1101's own sequence: a chat with a transcript and a quit record is closed, and a
    new chat opens in the same workspace."""

    def setUp(self) -> None:
        super().setUp()
        self.old = state.new_chat_id("default")
        self.assertEqual(self.old, "default.1")
        state.record_identity(self.old, {"CHARTER_HARNESS": "claude-code"})
        state.record_workspace(self.old, "default")
        reopen.transcript_path(self.old).write_text("the earlier chat's scrollback\n")
        c = reopen.Chat(chat=self.old, workspace="default", persona="",
                        harness="claude-code", cwd="", resume="", transcript="default.1.transcript",
                        active=False)
        self.assertTrue(reopen.write([reopen.Frame(workspace="default", chats=(c,))],
                                     focus="default"))
        state.record_closed(self.old)
        shutil.rmtree(state.frame_dir(self.old))

    def test_the_next_chat_is_not_the_closed_chats_id(self):
        self.assertEqual(state.new_chat_id("default"), "default.2")

    def test_the_new_chat_is_not_offered_the_old_transcript(self):
        from charter.frame import builtin_actions
        new = state.new_chat_id("default")
        self.assertFalse(builtin_actions._has_transcript(new))

    def test_closing_the_new_chat_leaves_the_old_record(self):
        from charter import commands_frame
        new = state.new_chat_id("default")
        commands_frame._forget_transcript(new)
        m = reopen.read()
        self.assertIsNotNone(m)
        self.assertEqual([c.chat for c in m.all_chats()], [self.old])
        self.assertTrue(reopen.transcript_path(self.old).is_file())


_REPO = Path(__file__).resolve().parents[1]


class TheRecordIsWrittenDown(unittest.TestCase):
    """The decision and its costs, read off the repository root — never through `config`,
    which points at whichever plane ran the suite (CONTRIBUTING, #785)."""

    def _adr(self) -> str:
        found = sorted((_REPO / "docs" / "adr").glob(
            "*-a-chat-id-names-one-chat-and-one-harness-session.md"))
        self.assertEqual(len(found), 1, found)
        return found[0].read_text()

    def test_one_adr_records_the_decision(self):
        self.assertTrue(self._adr().startswith("# "))

    def test_its_costs_are_written_down(self):
        """The half a decision record is usually missing: what the link trusts, what it
        does not follow, and how many ids a prefix can ever hand out."""
        text = self._adr()
        for cost in ("CLAUDE_PID", "/new", "99,999"):
            with self.subTest(cost=cost):
                self.assertIn(cost, text)

    def test_the_vocabulary_says_an_id_is_handed_out_once(self):
        context = (_REPO / "CONTEXT.md").read_text()
        chat = context.split("**Chat**:", 1)[1].split("\n\n", 1)[0]
        self.assertIn("handed out once", chat)
        self.assertIn("harness session", chat)


if __name__ == "__main__":
    unittest.main()
