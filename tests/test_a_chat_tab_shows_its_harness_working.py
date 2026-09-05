"""A chat tab spins while that chat's harness is working — #853.

**The whole gap was the falling edge, and everything else was already on disk.** charter
does not own the harness's screen; it owns its HOOKS, and a hook process runs inside the
chat's own pane, whose window the launcher created with ``-e CHARTER_SESSION_ID=<chat id>``
— so a hook has always known exactly which chat it is in. `hooks.userpromptsubmit` already
fired at the start of every turn. `Stop` in `hooks/hooks.json` ran `charter workspace
_autosave` and `hooks._HANDLERS` had **no entry at all**, so nothing charter raised could
ever be lowered.

Four properties, each failing in a different direction:

1. :class:`TheTurnTrackerHoldsOneChatAtATime` — the tracker itself. A mark goes up, comes
   down, decays, and cannot be talked into naming a path outside its own directory.
2. :class:`TheThreeEdgesAreThreeHooks` — the rising edge, the refresh and the falling
   edge, driven through the real handlers, plus the harnesses that get no mark at all.
3. :class:`TheStripDrawsItWithoutMovingACell` — the strip. The spinner takes the mark's
   cell, so a chat starting a turn moves not one column of a map that resolves clicks BY
   column.
4. :class:`OnlyTheChatStripSpins` — the cost property, `slots.ANIMATED`'s one bar over: a
   panel pays for the gate its own renderer draws from and for no other.

**The honest limit, asserted rather than hidden** (:class:`TheLimitIsAToollessThink`): a
long toolless think refreshes nothing, so `inflight.TURN_STALE_SECONDS` cannot both catch
an abandoned turn and cover an arbitrarily long silence. The tracker decays, and the test
that says so is the record of the trade.
"""

from __future__ import annotations

import io
import json
import os
import sys
import time
import unicodedata
import unittest
from contextlib import redirect_stdout
from pathlib import Path
from unittest import mock

from charter import config, hooks, inflight, tui
from charter.frame import chats, gather, panel, slots, state
from tests._isolation import PersonaIso, PlaneIso
from tests.test_frame_chat_switch import _plant


def _run(fn, payload: dict) -> tuple[int, str]:
    """Call a handler with *payload* on stdin; return ``(exit code, stdout)``."""
    old = sys.stdin
    sys.stdin = io.StringIO(json.dumps(payload))
    buf = io.StringIO()
    try:
        with redirect_stdout(buf):
            rc = fn()
    finally:
        sys.stdin = old
    return rc, buf.getvalue().strip()


#: The environment a hook process actually has inside a chat's harness pane. Spelled out
#: rather than taken off `commands_frame._FRAME_IDENTITY`: a test that reads the tuple it
#: is checking agrees with a tuple edited to nothing, and what is being asserted here is
#: that CHARTER'S OWN two variables decide this — which chat, and which harness.
#:
#: `claude-code` is hand-spelled for the same reason. It is `harness.claude_code.NAME`, and
#: a comparison against that constant would pass against a constant edited to `""`, which
#: is exactly the value an absent `$CHARTER_HARNESS` supplies.
CHAT_ENV = {"CHARTER_SESSION_ID": "api.2", "CHARTER_HARNESS": "claude-code"}


def _mark(chat: str) -> Path:
    """Where the tracker puts *chat*'s mark, spelled here rather than asked of the module.

    The path is the interface between a hook process and a panel process, so a test that
    asked `inflight._turn_file` for it would agree with any path that function came to
    answer — including one under a directory `frame/panel.py` is not watching.
    """
    return Path(config.STATE_DIR) / "chat-turns" / chat


class TheTurnTrackerHoldsOneChatAtATime(PersonaIso, unittest.TestCase):
    """`inflight`'s second tracker: keyed by CHAT, where the first is keyed by AGENT.

    The one above is `{"agent", "kind", "ts"}` — *"no fid, no chat, no workspace"*
    (`inflight.prune_all`) — which is the right shape for the dispatch-overlap nudge and
    `bottom`'s `⏳ N`, and cannot answer *which chat is busy* at all. That is why this is a
    second tracker rather than a fifth `kind`.
    """

    def setUp(self):
        super().setUp()
        self.assertIn("edm-test-", str(config.STATE_DIR),
                      "precondition: this case writes, and must write into its own tmp")

    def test_a_marked_chat_is_reported_and_an_unmarked_one_is_not(self):
        self.assertEqual([], inflight.working_chats())
        inflight.turn_begin("api.2")
        self.assertEqual(["api.2"], [c for c, _ in inflight.working_chats()])

    def test_the_falling_edge_takes_the_mark_away(self):
        inflight.turn_begin("api.2")
        self.assertTrue(_mark("api.2").is_file(), "precondition: the mark must be up")
        inflight.turn_end("api.2")
        self.assertEqual([], inflight.working_chats())
        self.assertFalse(_mark("api.2").exists())

    def test_a_mark_nobody_ever_lowered_decays_and_is_swept(self):
        """The turn that never gets a `Stop` — Esc mid-turn fires none.

        Both halves: charter stops CLAIMING (the reader drops it) and stops KEEPING (the
        file goes). The second is what makes the expiry an ordinary `turn_stamp` change,
        so a panel learns of it through the number it already watches rather than needing
        a deadline of its own for a chat that no longer exists.
        """
        inflight.turn_begin("api.2")
        p = _mark("api.2")
        old = time.time() - inflight.TURN_STALE_SECONDS - 1
        os.utime(p, (old, old))
        self.assertEqual([], inflight.working_chats())
        self.assertFalse(p.exists(), "an expired mark was left on disk for good")

    def test_a_bump_rescues_a_mark_that_was_about_to_expire(self):
        """The other direction, so the decay above cannot be satisfied by never refreshing.

        This is what makes `TURN_STALE_SECONDS` a bound on a stretch with NO tool call
        rather than a bound on a turn: every `pretooluse*`/`posttooluse*` handler calls it.
        """
        inflight.turn_begin("api.2")
        old = time.time() - inflight.TURN_STALE_SECONDS - 1
        os.utime(_mark("api.2"), (old, old))
        inflight.turn_bump("api.2")
        self.assertEqual(["api.2"], [c for c, _ in inflight.working_chats()])

    def test_a_bump_raises_nothing_by_itself(self):
        """A tool hook fires for a sub-agent's tools as well as the session's own, and it
        fires in whatever turn happens to be running. If a bump could CREATE, a chat would
        start claiming to be working on evidence that a tool ran rather than on evidence
        that a turn began — and there would be no single moment charter could point at."""
        inflight.turn_bump("api.2")
        self.assertEqual([], inflight.working_chats())
        self.assertFalse(_mark("api.2").exists())

    def test_a_refresh_moves_no_stamp_and_the_two_edges_do(self):
        """`turn_stamp` is the panel's whole idle cost, and this is the property that makes
        it affordable: a turn issuing a tool call a second re-reads nothing, because
        `utime` on an existing file does not move its directory's mtime. Only the SET
        changing costs a read — which is `inflight.stamp`'s rule for the other tracker."""
        inflight.turn_begin("api.2")
        up = inflight.turn_stamp()
        inflight.turn_bump("api.2")
        self.assertEqual(up, inflight.turn_stamp(), "a refresh made every panel re-read")
        inflight.turn_end("api.2")
        self.assertNotEqual(up, inflight.turn_stamp(),
                            "the falling edge announced itself to nobody")

    def test_no_directory_at_all_is_an_answer_rather_than_an_error(self):
        self.assertIsNone(inflight.turn_stamp())
        self.assertEqual([], inflight.working_chats())

    def test_a_chat_id_that_could_name_a_path_is_refused_rather_than_repaired(self):
        """``$CHARTER_SESSION_ID`` is an environment variable, so anything in that shell can
        set it — and the value becomes a filename. A name outside the alphabet a chat id
        reaches tmux under is not a chat id, so it gets no mark at all: nothing lands
        outside the directory, and nothing lands inside it under a repaired name either.
        """
        for chat in ("../escaped", "/etc/passwd", "a/b", "with space",
                     "x" * 65):
            with self.subTest(chat=chat):
                self.assertIsNone(inflight._turn_file(chat))
                inflight.turn_begin(chat)
                self.assertEqual([], inflight.working_chats())
        self.assertFalse((Path(config.STATE_DIR) / "escaped").exists())

    def test_a_real_chat_id_is_the_marks_own_name(self):
        """What the refusal above buys: the reader takes the id off the directory entry,
        so there is no mangled form for it to have to invert and no file to open."""
        inflight.turn_begin("api.2")
        self.assertTrue(_mark("api.2").is_file())
        self.assertEqual(["api.2"], [c for c, _ in inflight.working_chats()])

    def test_a_name_that_is_nothing_but_blanks_is_no_name(self):
        """`$CHARTER_SESSION_ID=" "` is a value a shell can supply, and it is not a chat.
        Normalised HERE and nowhere else — this is the seam that turns the value into a
        path, and `hooks._chat_id` deliberately does not strip a second time."""
        for chat in (None, "", "   ", "\n"):
            with self.subTest(chat=repr(chat)):
                self.assertIsNone(inflight._turn_file(chat))
                inflight.turn_begin(chat)
                self.assertEqual([], inflight.working_chats())

    def test_the_blanks_are_taken_off_both_ends(self):
        """Both ends, and asserted as one case rather than as a tidiness note: with the
        normalisation reduced to either half, `  api.2  ` no longer spells itself and the
        mark is refused outright — so the chat that WOULD have been marked shows nothing.
        """
        inflight.turn_begin("  api.2  ")
        self.assertEqual(["api.2"], [c for c, _ in inflight.working_chats()])
        self.assertTrue(_mark("api.2").is_file())

    def test_every_edge_refuses_a_name_that_is_not_one(self):
        """`turn_begin`'s refusal is asserted by what does NOT appear on disk; the other
        two have nothing to leave behind, so what pins theirs is that they do not raise —
        without it `turn_bump` reaches `os.utime(None)`, which is a `TypeError` and not
        the `OSError` its own catch is for."""
        for edge in (inflight.turn_begin, inflight.turn_bump, inflight.turn_end):
            for chat in (None, "", "..", "../escaped"):
                with self.subTest(edge=edge.__name__, chat=repr(chat)):
                    edge(chat)
        self.assertEqual([], inflight.working_chats())

    def test_a_filesystem_that_refuses_costs_a_mark_and_never_a_turn(self):
        """Each edge's `except OSError` on its own syscall. A tracker that cannot be
        written, refreshed or cleared is a strip that shows nothing — which is what the
        strip showed before this existed — and never an exception out of a hook.

        **The `stat` failure is aimed at ONE entry and not at `Path.stat`**, and the first
        cut of this case is why. Failing every `stat` also fails the `d.exists()` that
        opens `working_chats`, and `pathlib.Path.exists` does not swallow an arbitrary
        `OSError`: it asks `_ignore_error`, which reads `errno`, and an `OSError` raised
        with a message and no errno is re-raised. That made the case pass on 3.14 (whose
        `exists` is written differently) and error on 3.11 and 3.12 — a fixture measuring
        the standard library rather than this tracker. What the loop's `continue` is
        actually for is an entry that vanishes between the `readdir` and the `stat`, and
        that is what this now does.
        """
        gone = OSError("the disk has opinions")
        with mock.patch("charter.config.touch_for", side_effect=gone):
            inflight.turn_begin("api.2")
        self.assertEqual([], inflight.working_chats())
        inflight.turn_begin("api.2")
        with mock.patch("charter.inflight.os.utime", side_effect=gone):
            inflight.turn_bump("api.2")
        with mock.patch.object(Path, "unlink", side_effect=gone):
            inflight.turn_end("api.2")
        self.assertEqual(["api.2"], [c for c, _ in inflight.working_chats()],
                         "the unlink was refused, so the mark is still up")
        real_stat = Path.stat

        def vanished(self, *a, **kw):
            if self.name == "api.2":
                raise gone
            return real_stat(self, *a, **kw)

        with mock.patch.object(Path, "stat", vanished):
            self.assertEqual([], inflight.working_chats())

    def test_the_two_names_a_mangling_cannot_make_safe_are_refused(self):
        """``.`` and ``..`` are made entirely of admitted characters, so they survive
        `_safe_name` intact — and `turn_end` on one would unlink a DIRECTORY rather than a
        record. Refused by name, which is the only thing left that can refuse them."""
        for chat in (".", ".."):
            with self.subTest(chat=chat):
                self.assertIsNone(inflight._turn_file(chat))
                inflight.turn_begin(chat)
                self.assertEqual([], inflight.working_chats())

    def test_the_two_trackers_do_not_read_each_others_records(self):
        """The reason this is a second directory and not a fifth `kind`: the same records
        feed the dispatch-overlap nudge, which reads agent names back to an operator as a
        sentence, and a chat id reaching it would produce #420's own wrong-and-confident
        failure through the other axis."""
        inflight.turn_begin("api.2")
        self.assertEqual([], inflight.live(kind=None))
        token = inflight.start("steward")
        self.addCleanup(inflight.finish, "steward", token=token)
        self.assertEqual(["api.2"], [c for c, _ in inflight.working_chats()])


class TheThreeEdgesAreThreeHooks(PlaneIso, unittest.TestCase):
    """The rising edge, the refresh and the falling edge, through the real handlers.

    `PlaneIso`, because since #852 every handler opens on the plane gate and a case on a
    root with no marker asserts nothing at all.
    """

    def setUp(self):
        super().setUp()
        self.assertIn("edm-test-", str(config.STATE_DIR))
        self.payload = {"cwd": str(config.ROOT), "session_id": "s853"}

    def _hook(self, fn, **extra):
        with mock.patch.dict(os.environ, {**CHAT_ENV, **extra}):
            return _run(fn, dict(self.payload, **{}))

    def test_a_prompt_raises_the_mark(self):
        self._hook(hooks.userpromptsubmit)
        self.assertEqual(["api.2"], [c for c, _ in inflight.working_chats()])

    def test_the_stop_hook_lowers_it(self):
        """The line that did not exist. `hooks._HANDLERS` had eleven entries and none of
        them was `stop`, so `hooks/hooks.json`'s `Stop` ran an autosave and charter's own
        dispatch table never heard the turn end."""
        self._hook(hooks.userpromptsubmit)
        self.assertTrue(inflight.working_chats(), "precondition: something must be up")
        self._hook(hooks.stop)
        self.assertEqual([], inflight.working_chats())

    def test_a_tool_call_mid_turn_refreshes_it(self):
        self._hook(hooks.userpromptsubmit)
        old = time.time() - inflight.TURN_STALE_SECONDS - 1
        os.utime(_mark("api.2"), (old, old))
        self.assertEqual([], inflight.working_chats(),
                         "precondition: the mark must have been about to expire")
        # It was swept by the read above, so put it back the way a turn does.
        self._hook(hooks.userpromptsubmit)
        os.utime(_mark("api.2"), (old, old))
        self._hook(hooks.posttooluse, **{})
        self.assertEqual(["api.2"], [c for c, _ in inflight.working_chats()])

    def test_a_tool_call_outside_a_turn_raises_nothing(self):
        """The bump/begin split, end to end. Without it a `posttooluse` from a sub-agent
        would put a tab back up after its parent's `Stop` had taken it down."""
        self._hook(hooks.posttooluse)
        self.assertEqual([], inflight.working_chats())

    def test_a_harness_charter_will_never_hear_the_end_from_gets_no_mark(self):
        """*"A recency mark would claim now while measuring recently."*

        opencode sets ``$CHARTER_SESSION_ID`` on its tool hooks and has no session-stop
        event; Codex is tool-hooks only. A mark charter can raise and cannot lower is not a
        working light, and the operator cannot tell the two apart by looking. Both names
        are hand-spelled — a test comparing against `harness.opencode.NAME` would agree
        with that constant edited to `claude-code`.
        """
        for harness in ("opencode", "codex"):
            with self.subTest(harness=harness):
                self._hook(hooks.userpromptsubmit, CHARTER_HARNESS=harness)
                self.assertEqual([], inflight.working_chats())

    def test_a_session_that_does_not_say_which_harness_gets_no_mark_either(self):
        """"charter does not know which harness this is" and "charter knows this harness
        reports no stop" reach the same picture on purpose — `state.harness_session`'s
        four-reasons-one-answer rule, one surface over."""
        with mock.patch.dict(os.environ, {"CHARTER_SESSION_ID": "api.2"}):
            os.environ.pop("CHARTER_HARNESS", None)
            _run(hooks.userpromptsubmit, self.payload)
        self.assertEqual([], inflight.working_chats())

    def test_a_session_outside_a_frame_asks_the_tracker_nothing_at_all(self):
        """Most sessions run with no frame at all, so this is the common path and it must
        cost nothing — not merely write nothing. `notify.plane_changed` makes the same
        check for the same reason one function over: *"both checks happen before `gather`
        is even imported, so the common 'no frame' path never touches the gather module at
        all"*.

        **A recording mock and not a raising one**, which is the difference between a pin
        and a test that only looks like one: `hooks._turn_begin` and its two siblings
        swallow every exception on purpose — a readout must never break a turn — so a
        `side_effect=AssertionError` here would be caught by the very code under test and
        the case would pass with the guard deleted.
        """
        for fn in (hooks.userpromptsubmit, hooks.posttooluse, hooks.stop):
            calls = {name: mock.DEFAULT for name in ("turn_begin", "turn_bump", "turn_end")}
            with self.subTest(hook=fn.__name__), \
                 mock.patch.dict(os.environ, {"CHARTER_HARNESS": "claude-code"}), \
                 mock.patch.multiple("charter.inflight", **calls) as patched:
                os.environ.pop("CHARTER_SESSION_ID", None)
                _run(fn, self.payload)
                for m in patched.values():
                    m.assert_not_called()
        self.assertFalse((Path(config.STATE_DIR) / "chat-turns").exists())

    def test_a_tracker_that_raises_does_not_break_the_turn(self):
        """The rule every hook in this module keeps and these three are no exception: a
        readout is not worth a turn. All three edges, because each reaches the tracker
        through its own import and its own syscall, and a catch is only a promise on the
        function that carries it."""
        boom = ValueError("tracker is having a day")
        for fn, target in ((hooks.userpromptsubmit, "turn_begin"),
                           (hooks.posttooluse, "turn_bump"),
                           (hooks.stop, "turn_end")):
            with self.subTest(hook=fn.__name__):
                with mock.patch(f"charter.inflight.{target}", side_effect=boom):
                    rc, out = self._hook(fn)
                self.assertEqual((0, ""), (rc, out))

    def test_the_stop_handler_says_nothing_and_refuses_nothing(self):
        """`Stop` can block, by exit 2 or by ``{"decision": "block"}``. This handler exists
        to STOP claiming something; a claim it could not retract is not worth a turn."""
        rc, out = self._hook(hooks.stop)
        self.assertEqual((0, ""), (rc, out))

    def test_a_subagent_finishing_does_not_take_the_tab_down(self):
        """`Stop` and `SubagentStop` share the autosave in `hooks/hooks.json` and must not
        share this: a dispatched sub-agent finishing does not end the turn that dispatched
        it, and on a fan-out the tab would blink off once per worker.

        Read off the manifest, because that is the artifact that decides it — the CLI
        cannot tell which event invoked it.
        """
        doc = json.loads((Path(__file__).resolve().parents[1]
                          / "hooks" / "hooks.json").read_text())["hooks"]
        cmds = [h["command"] for entry in doc["SubagentStop"] for h in entry["hooks"]]
        self.assertTrue(cmds, "precondition: SubagentStop must still be wired to something")
        self.assertEqual([], [c for c in cmds if "charter hook stop" in c])


class TheSeamAnswersForTheStrangersRepo(PersonaIso, unittest.TestCase):
    """Outside a plane the tracker writes nothing, touches nothing and deletes nothing.

    `PersonaIso` and NOT `PlaneIso`: this case is about the gate, so its root must not be a
    plane. Outside one ``config.STATE_DIR`` is ``<cwd>/.charter``, so every file below is
    one a repository you cloned can simply contain and commit —
    `test_a_repo_that_is_not_a_plane_gets_no_housekeeping`'s finding, and the reason the
    three seams carry the gate rather than their call sites: `hooks.pretooluse_read` is
    deliberately ungated and calls `_turn_bump`.
    """

    def setUp(self):
        super().setUp()
        self.assertFalse(config.HAS_CONTROL_PLANE,
                         "precondition: this case is vacuous inside a plane")

    def test_a_prompt_creates_no_mark_in_a_repo_charter_does_not_own(self):
        with mock.patch.dict(os.environ, CHAT_ENV):
            hooks._turn_begin()
        self.assertFalse((Path(config.STATE_DIR) / "chat-turns").exists())

    def test_a_committed_mark_is_not_consumed(self):
        """`_turn_end` **unlinks**, so out here it would be charter deleting a file the
        checkout supplied — `posttooluse_bash`'s ask-marker finding, one directory over.
        Driven through `hooks.stop`, which carries no gate of its own precisely so that
        this one is the only thing standing between the handler and the unlink."""
        p = _mark("api.2")
        config.private_mkdir(p.parent)
        p.write_text("api.2")
        self.assertTrue(p.is_file(), "precondition: the mark must exist to be taken")
        with mock.patch.dict(os.environ, CHAT_ENV):
            rc, out = _run(hooks.stop, {"cwd": str(config.ROOT)})
        self.assertEqual((0, ""), (rc, out))
        self.assertTrue(p.is_file(), "charter deleted a file from a repo it does not own")

    def test_a_committed_marks_timestamp_is_left_alone(self):
        """`_turn_bump` writes no bytes, so `assertUntouched`-style content snapshots
        cannot see it — and an mtime charter moved in somebody else's checkout is still a
        write, one that a `make` there would act on. Driven through `pretooluse_read`,
        the one handler with no plane gate at all."""
        p = _mark("api.2")
        config.private_mkdir(p.parent)
        p.write_text("api.2")
        old = time.time() - 5_000
        os.utime(p, (old, old))
        before = p.stat().st_mtime
        with mock.patch.dict(os.environ, CHAT_ENV):
            _run(hooks.pretooluse_read, {"cwd": str(config.ROOT), "tool_name": "Read",
                                         "tool_input": {"file_path": "README.md"}})
        self.assertEqual(before, p.stat().st_mtime,
                         "charter touched a file in a repo it does not own")


class TheStripDrawsItWithoutMovingACell(PersonaIso, unittest.TestCase):
    """The strip. What is drawn, and — harder — what is NOT moved by drawing it."""

    NAMES = ["api.1", "api.2", "api.3"]

    def setUp(self):
        super().setUp()
        slots.TABS.forget()
        self.addCleanup(slots.TABS.forget)

    def _row(self, busy=(), width=200, here="api.2", now=0.0):
        with mock.patch("charter.frame.slots.time.monotonic", return_value=now):
            rows = slots._bar(list(self.NAMES), here, width, busy=busy)
        return tui.strip_ansi(rows[0]) if rows else ""

    def test_an_idle_strip_is_the_strip_that_shipped(self):
        self.assertEqual("   api.1   api.2   api.3", self._row().rstrip())

    def test_a_working_chat_wears_the_spinner_where_an_idle_one_wears_a_blank(self):
        """Hand-spelled, and the whole row, because what is being asserted is a POSITION:
        the glyph is in the cell `slots._TAB_LEAD` would have left blank, and every
        other character of the row is where it was.

        **`rstrip` and never `strip`**, which #880 is what made load-bearing. These rows
        used to open with the word `chats`, so stripping both ends left the idle mark's
        blank in the middle of the string where a reader could see it; with the heading
        gone, `strip` eats exactly the cell this case is about and an idle row and a
        working one would compare equal. The two leading spaces are `slots.INSET`.
        """
        self.assertEqual("  ✢api.1   api.2   api.3",
                         self._row(busy={"api.1"}, now=0.0).rstrip())
        self.assertEqual("   api.1   api.2  ✶api.3",
                         self._row(busy={"api.3"}, now=slots.SPINNER_PERIOD).rstrip())

    def test_the_chat_you_are_typing_in_shows_its_spinner_too(self):
        """**#903 gave the cell one occupant, and this is what that bought.** It used to
        hold two facts and `*` won: `chrome.block` paints the active tab, but under
        `NO_COLOR` every escape is stripped, so the `*` was the only thing saying where
        you were and the working chat you were standing in had to look idle. "You are
        here" is an edge now (`slots._TAB_LEAD`, `chrome.block`'s underline), so the lead
        answers one question for every tab and the chat you are typing in spins like the
        rest.

        The cost travelled with it and is pinned one class over
        (`tests/test_frame_bars.TheTabYouAreOnIsDrawnAsOne`): with the paint deleted, the
        row no longer says which chat is yours.
        """
        self.assertEqual("   api.1  ✢api.2   api.3",
                         self._row(busy={"api.2"}).rstrip())

    def test_every_tab_on_a_row_shows_the_same_frame(self):
        row = self._row(busy={"api.1", "api.2", "api.3"},
                        now=slots.SPINNER_PERIOD * 2)
        self.assertEqual("  ✻api.1  ✻api.2  ✻api.3", row.rstrip())

    def test_a_chat_starting_a_turn_moves_no_column_of_the_click_map(self):
        """**The property the whole design turns on.** The map resolves a click BY COLUMN,
        so a spinner drawn beside a name instead of in the mark's cell would re-cut the
        strip the moment a sibling started thinking — and the cell the operator was about
        to press would hold another chat's name. Asserted at every width, because the
        rungs that window the list are exactly where a stray cell would show up."""
        for width in range(0, 120):
            with self.subTest(width=width):
                slots.TABS.forget()
                idle = slots._bar(list(self.NAMES), "api.2", width)
                idle_map = dict(slots.TABS._cols)
                slots.TABS.forget()
                with mock.patch("charter.frame.slots.time.monotonic", return_value=0.0):
                    busy = slots._bar(list(self.NAMES), "api.2", width,
                                      busy={"api.1", "api.3"})
                self.assertEqual(idle_map, dict(slots.TABS._cols))
                self.assertEqual([tui.width(r) for r in idle],
                                 [tui.width(r) for r in busy])

    def test_a_click_on_a_spinning_tab_switches_to_that_chat(self):
        """The other half: the mark cell belongs to the tab it marks, and that stays true
        when the mark is a spinner. `_tab_columns` gives a tab the cells of the whole
        field, and there is nothing else a click on that cell could be about."""
        row = self._row(busy={"api.1"})
        col = row.index("✢")
        self.assertEqual("api.1", slots.TABS.switch_to(0, col))

    def test_the_row_count_the_launcher_asks_for_does_not_depend_on_the_clock(self):
        """`bar_rows_wanted` runs in the LAUNCHER and in the `frame-resize` child, where
        nothing knows or should know which chat is thinking. It composes through the same
        ladder, with `busy` left empty — so the pane is sized for the strip that will be
        drawn into it whatever any harness is doing."""
        for n in range(1, 9):
            _plant(f"ws.{n}", workspace="ws")
        inflight.turn_begin("ws.3")
        with mock.patch("charter.frame.slots.working_chats",
                        side_effect=AssertionError("the sizer asked about the clock")):
            for cols in (60, 100, 160):
                with self.subTest(cols=cols):
                    slots.bar_rows_wanted("ws.1", "chats", pane_cols=cols, cap=3)

    def test_the_workspaces_bar_never_spins(self):
        """It is not a strip of chats. A workspace has no harness to be working, so
        `workspaces_bar` asks nothing and passes no `busy` at all."""
        _plant("ws.1", workspace="ws")
        inflight.turn_begin("ws.1")
        with mock.patch("charter.frame.slots.working_chats",
                        side_effect=AssertionError("the workspaces bar asked")):
            slots.workspaces_bar("ws.1", 120)

    def test_an_unreadable_tracker_draws_the_strip_that_shipped(self):
        """A panel that threw out of `render` loses its pane, so the readout degrades to
        stillness rather than to a hole in the frame."""
        with mock.patch("charter.inflight.working_chats", side_effect=OSError("gone")):
            self.assertEqual(frozenset(), slots.working_chats())


class TheGlyphsAreCheckedBeforeTheyGoOnARow(unittest.TestCase):
    """`slots.TAB_SPINNER`, held to the row's own rules rather than to a list of characters.

    The request was Claude Code's spinner, `· ✢ ✶ ✳ ✽ ✻`. `tui.width` answers **1** for all
    six of them and that is not the whole question: it reads the East-Asian tables, and an
    *Ambiguous* character is one a terminal may draw two cells wide while those tables say
    one. That is what `slots._BAR_RULE` is ASCII to avoid and what
    `statusline._persona_chips` records breaking this project's layout twice — and on a
    strip whose click map is per COLUMN, one glyph a cell wider than it was measured shifts
    every tab right of it.

    So two of the six are refused outright (`·` U+00B7 and `✽` U+273D are Ambiguous) and a
    third is refused for a reason the tables cannot state (`✳` U+2733 carries an emoji
    presentation variant, `✳️`, so a terminal with an emoji fallback font may draw it wide).
    The properties are asserted, never the characters, so a future redesign can pick
    different glyphs and still be safe.
    """

    def test_every_frame_is_exactly_one_cell(self):
        for ch in slots.TAB_SPINNER:
            with self.subTest(glyph=ch):
                self.assertEqual(1, tui.width(ch))

    def test_no_frame_is_east_asian_ambiguous(self):
        for ch in slots.TAB_SPINNER:
            with self.subTest(glyph=ch):
                eaw = unicodedata.east_asian_width(ch)
                self.assertIn(eaw, ("N", "Na"),
                              f"{ch!r} (U+{ord(ch):04X}) is East-Asian {eaw} — a terminal "
                              f"may draw it two cells and shift every tab right of it, "
                              f"and a click then lands on the neighbouring chat")

    def test_no_frame_is_a_character_a_chat_id_may_contain(self):
        """The mark sits flush against the name, so a glyph out of `chats.ID_RE`'s alphabet
        would draw `Oapi.3` and put a character the operator reads as part of a name where
        the name begins. It is why the ASCII pulse this reached for first — `.oOo` — is not
        what shipped: every one of its frames is a legal chat-id character."""
        for ch in slots.TAB_SPINNER:
            with self.subTest(glyph=ch):
                self.assertIsNone(chats.ID_RE.fullmatch(ch))

    def test_the_two_ambiguous_glyphs_that_were_asked_for_are_not_on_the_row(self):
        """The refusal, hand-spelled by codepoint. A test asserting only the property above
        would go green against a `TAB_SPINNER` that had quietly regained `·` on a Python
        whose Unicode tables reclassified it."""
        for cp in (0x00B7, 0x273D, 0x2733):
            with self.subTest(codepoint=f"U+{cp:04X}"):
                self.assertNotIn(chr(cp), slots.TAB_SPINNER)

    def test_the_frame_is_read_off_the_clock_a_caller_names(self):
        """The *now* seam `spinner_frame` documents — *"for tests, which need a specific
        frame rather than whichever one the clock happened to be on"* — asked directly
        rather than through a patched `time.monotonic`, so the parameter is a thing the
        suite exercises and not a branch nothing reaches.

        The instants are named as multiples of `SPINNER_PERIOD` plus a nudge, because
        `int(t / p)` on an exact multiple is a float-division boundary — `0.6 / 0.2` is
        2.9999999999999996 — and a frame table is not the place to assert a rounding rule.
        """
        eighth = slots.SPINNER_PERIOD / 8
        for i, want in enumerate(slots.TAB_SPINNER):
            with self.subTest(frame=i):
                at = slots.SPINNER_PERIOD * i + eighth
                self.assertEqual(want, slots.tab_spinner_frame(at))
                self.assertEqual(want, slots.tab_spinner_frame(
                    at + slots.SPINNER_PERIOD * len(slots.TAB_SPINNER)),
                    "the sequence must repeat rather than run out")

    def test_the_mark_and_the_spinner_are_the_same_width(self):
        """The load-bearing equality: the spinner takes the lead's cell, so the two must
        measure alike or the strip is one cell wider exactly while a chat is working."""
        for ch in slots.TAB_SPINNER:
            with self.subTest(glyph=ch):
                self.assertEqual(tui.width(slots._TAB_LEAD), tui.width(ch))


class OnlyTheChatStripSpins(PersonaIso, unittest.TestCase):
    """`slots.BAR_ANIMATED` must name exactly the renderers a working chat moves.

    `slots.ANIMATED`'s cost property, one gate over, and it is a cost property rather than a
    tidiness one for the same reason: `panel._watch` runs one process per slot, so an
    unscoped "is a chat working" would repaint every panel at `panel.TICK` for the whole
    length of every turn — `right` costs 4 816µs a render to redraw byte-identical output.

    A second SET rather than a `chats` added to `ANIMATED`, because the two name different
    GATES: that one ticks on plane-wide in-flight dispatches and this frame's notice dwell,
    neither of which has anything to do with whether a sibling chat's harness is thinking.
    """

    def setUp(self):
        super().setUp()
        self.fid = "w.1"
        _plant(self.fid, workspace="w")
        _plant("w.9", workspace="w")
        inflight.turn_begin("w.9")
        gather.save(self.fid, {"gathered_at": 0.0, "workspace": "w",
                               "current_repo": None, "repos": [], "worktrees": []})

    def _render_at(self, slot, now):
        """Draw *slot* the way the panel process holding it would.

        Two paths, because there are two: `slots.render` is charter's four panes, and the
        two strips are components whose renderers are reached by name (`builtins._chats`
        calls `slots.chats_bar` and `_workspaces` calls `slots.workspaces_bar`). Asking
        `slots.render` for `chats` answers `unknown slot`, which is a string that does not
        change with the clock — and would make this whole property vacuous for exactly the
        slot it is about.
        """
        bars = {"chats": slots.chats_bar, "workspaces": slots.workspaces_bar}
        with mock.patch("charter.frame.slots.time.monotonic", return_value=now), \
             mock.patch.object(sys.stdout, "fileno", return_value=1, create=True), \
             mock.patch("os.get_terminal_size",
                        return_value=os.terminal_size((120, 40))):
            if slot in bars:
                return "\n".join(bars[slot](self.fid, 120))
            return slots.render(slot, self.fid)

    def test_exactly_the_slots_in_BAR_ANIMATED_move_while_a_chat_works(self):
        moving = set()
        for slot in sorted(set(slots.SLOTS) | set(slots.BARS)):
            with self.subTest(slot=slot):
                first = self._render_at(slot, 0.0)
                later = self._render_at(slot, slots.SPINNER_PERIOD)
                if first != later:
                    moving.add(slot)
        self.assertEqual(moving, set(slots.BAR_ANIMATED),
                         f"`slots.BAR_ANIMATED` says {sorted(slots.BAR_ANIMATED)} but the "
                         f"renderers a working chat actually moves are {sorted(moving)}")

    def test_a_slot_that_draws_no_tab_strip_never_asks_whether_a_chat_is_working(self):
        """The short-circuit, which is what makes a `top`/`bottom`/`right`/`workspaces`
        panel cost exactly what it cost before this feature existed — not merely repaint
        less."""
        for slot in sorted((set(slots.SLOTS) | set(slots.BARS)) - set(slots.BAR_ANIMATED)):
            with self.subTest(slot=slot):
                with mock.patch("charter.frame.panel._working",
                                side_effect=AssertionError("asked, and must not")), \
                     mock.patch("charter.frame.panel._paint"):
                    panel._watch(slot, self.fid, once=True)

    def test_the_chat_strip_does_ask(self):
        """The other direction, so the test above cannot be satisfied by never asking."""
        with mock.patch("charter.frame.panel._working", return_value=1) as asked, \
             mock.patch("charter.frame.panel._paint"):
            panel._watch("chats", self.fid, once=True)
        asked.assert_called_once()

    def test_every_name_in_BAR_ANIMATED_is_a_strip_that_exists(self):
        """The set is hand-maintained and holds NAMES, and the name is a string two other
        things spell independently: `slots.BARS`, and the launcher's own
        `charter panel chats --session <fid>`. A strip renamed in one and not here leaves a
        gate that can never fire — silently, because a gate that never fires looks exactly
        like a chat that is never working."""
        self.assertLessEqual(set(slots.BAR_ANIMATED), set(slots.BARS))
        self.assertTrue(slots.BAR_ANIMATED)

    def test_the_chat_strip_does_not_ride_the_dispatch_trackers_gate(self):
        """The two gates are separate all the way down. A dispatch running says nothing
        about whether any chat's harness is thinking, and a strip ticking for half an hour
        with every tab idle is the unscoped repaint `ANIMATED` exists to prevent."""
        with mock.patch("charter.frame.panel._running",
                        side_effect=AssertionError("the chat strip asked about dispatches")), \
             mock.patch("charter.frame.panel._paint"):
            panel._watch("chats", self.fid, once=True)


class IdleCostsOneStat(PersonaIso, unittest.TestCase):
    """`panel._working` — how a chat strip learns a sibling is working without paying for
    it. `panel._running`'s property one tracker over, and the reason this can be on by
    default: the expensive answer is behind a single `stat` of the tracker's directory."""

    def setUp(self):
        super().setUp()
        self.cache = panel._new_working_cache()

    def test_an_idle_plane_reads_nothing_at_all(self):
        with mock.patch("charter.inflight.working_chats",
                        side_effect=AssertionError("read on an idle plane")):
            self.assertEqual(0, panel._working(self.cache))

    def test_the_records_are_read_once_and_then_cached(self):
        inflight.turn_begin("api.2")
        self.assertEqual(1, panel._working(self.cache))
        with mock.patch("charter.inflight.working_chats",
                        side_effect=AssertionError("re-read with nothing changed")):
            self.assertEqual(1, panel._working(self.cache))

    def test_a_mark_going_up_or_down_is_read_again(self):
        inflight.turn_begin("api.2")
        self.assertEqual(1, panel._working(self.cache))
        inflight.turn_end("api.2")
        self.assertEqual(0, panel._working(self.cache))

    def test_the_expiry_is_re_read_although_no_file_moved(self):
        """The half a directory mtime cannot carry: a mark crosses
        `inflight.TURN_STALE_SECONDS` with nothing on disk changing, so the panel holds the
        earliest such deadline beside its cached answer — `panel._running`'s presumed-dead
        recheck, one tracker over.

        The CLOCK is moved rather than the file, deliberately: an mtime pushed backwards
        is not a state production can reach (a bump only ever moves one forward), and a
        test that reached it would be asserting against a fixture rather than against the
        thing that actually happens, which is time passing while nothing is written.
        """
        inflight.turn_begin("api.2")
        self.assertEqual(1, panel._working(self.cache))
        later = time.time() + inflight.TURN_STALE_SECONDS + 1
        with mock.patch("time.time", return_value=later):
            self.assertEqual(0, panel._working(self.cache))

    def test_a_tracker_that_cannot_be_read_is_stillness(self):
        with mock.patch("charter.inflight.turn_stamp", side_effect=OSError("gone")):
            self.assertEqual(0, panel._working(self.cache))

    def test_a_failed_read_clears_the_cached_answer_rather_than_leaving_it(self):
        """The other half of the failure path, and the half a `return 0` alone does not
        give: the STAMP is deliberately left describing the last directory state charter
        understood, so the next call short-circuits on it — and would hand back the count
        from before the failure if the failure had not written 0 over it. Stillness is the
        safe direction here (`_running` carries the argument), and it has to survive the
        cache as well as the one call."""
        inflight.turn_begin("api.2")
        self.assertEqual(1, panel._working(self.cache))
        with mock.patch("charter.inflight.turn_stamp", side_effect=OSError("gone")):
            self.assertEqual(0, panel._working(self.cache))
        self.assertEqual(0, panel._working(self.cache),
                         "the cache handed back the count from before the failure")


class TheLimitIsAToollessThink(PersonaIso, unittest.TestCase):
    """The cost of this signal, written down as a test rather than as a paragraph.

    A long **toolless** think refreshes nothing — no `pretooluse`, no `posttooluse` — so a
    TTL short enough to catch a turn that was abandoned with Esc also blinks off during
    deep thinking. The two cannot both be had from a hook channel that reports tool calls
    and prompts. `inflight.TURN_STALE_SECONDS` is where the trade is set, and the direction
    it errs in is *not claiming*.
    """

    def test_a_turn_that_thinks_for_longer_than_the_ttl_blinks_off(self):
        inflight.turn_begin("api.2")
        old = time.time() - inflight.TURN_STALE_SECONDS - 1
        os.utime(_mark("api.2"), (old, old))
        self.assertEqual([], inflight.working_chats(),
                         "the honest limit changed — say so in the news entry")

    def test_the_ttl_is_generous_against_the_cadence_it_measures(self):
        """A bound on a stretch with NO tool call, not on a turn. Hand-spelled: a test
        comparing `TURN_STALE_SECONDS` against an expression built from itself agrees with
        any value it comes to take, which is exactly how a number quietly reaches zero."""
        self.assertEqual(600, inflight.TURN_STALE_SECONDS)
        self.assertLess(inflight.TURN_STALE_SECONDS, inflight.PRESUMED_DEAD_SECONDS,
                        "a chat charter cannot see the end of must stop claiming sooner "
                        "than a dispatch it is deliberately still drawing")


if __name__ == "__main__":  # pragma: no cover
    unittest.main()
