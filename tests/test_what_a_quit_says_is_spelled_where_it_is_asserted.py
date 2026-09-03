"""#810 groups C and D: the sentences a quit says, and the values it repeats back.

**Group C's cause, in one line: a round trip cannot pin a name — with a sentence in place
of a file name.** Every assertion the quit/close/reopen suite makes about wording goes
through the module constant on *both* sides:

    self.assertEqual(rows[0].title, leave.NOTHING_OPEN)
    self.assertIn(leave.RESUMES, out)
    self.assertEqual(said.call_args[0][1], commands_frame.NO_TRANSCRIPT)

so rewording the constant moves the test with it and stays green. The remedy is the one
that closed the on-disk names on #796 — spell the words a second time, by hand, at the
surface they reach: **the duplication IS the assertion.** Sixteen constants, and the whole
of what this file adds over the suite it sits beside is that each one is written out here
in the words an operator actually reads.

*Why a sentence is worth pinning at all*, since it is not a compatibility surface: every
one of these is charter answering a question the operator asked, on a path where the thing
they asked for did **not** happen. `docs/frame.md` quotes some of them, `docs/news/`
entries quote more, and the deletion sweep's own charge against this cluster is that a
reword is invisible — not that it is forbidden. This file is where a reword becomes
visible, and what it costs is one line to update deliberately.

---

**A refutation, measured, and it changes what three of these are for.** #810 singles out
`leave.py:397-399` — `OPEN_ID`, `GO_ID`, `CHAT_ID` — as *"palette action ids, a binding
surface a `charter.toml` can name, so a rename silently breaks an operator's keybinding"*,
and calls them the highest-value three in the group. **They are not action ids and no
config can name one.** Measured on this tree:

* `component.usable_id("leave:quit")` is `False` — the alphabet is lower-case letters,
  digits, underscores and at most one dot, and the `:` is deliberate. `frame/leave.py`'s
  own comment says so: *"the whole of why these cannot collide with an action"*.
* `palette.matches` and `palette.exact` both gate id-matching on `usable_id`, so typing
  `leave` in the palette never matches one of these rows by its id.
* `commands_frame._draw_palette` routes them before `ActionRegistry.invoke` and says why:
  *"handing `leave:quit:go` to `ActionRegistry.invoke` would report 'no such action'"*.
* They never leave the process. `palette.own_the_tty` owns the terminal for the whole
  palette session, the ids are minted by `leave.open_rows`/`confirm_rows` and read back by
  `leave.verb_of`/`goes_through`/`is_row` in the same call, and none of them is drawn or
  written into a tmux binding.

The only config-named binding surface in the frame is a component's `key`
(`[[frame.component]]`), which fires `charter frame-toggle <component>` — a component
name, never a row id.

So they are pinned here for a smaller and honest reason: **the three shapes have to stay
tellable apart**, because `verb_of` distinguishes a doorway (`leave:<verb>`) from a
per-chat row (`leave:<verb>:c<n>`) by shape alone, and `usable_id` has to keep refusing
all three or a provider's action could collide with one. The sweep's `retune-string`
operator preserves punctuation — `"leave:{}"` re-tunes to `"mfbwf:{}"`, colon and braces
intact — so the shape assertions below do **not** kill that mutant on their own, and the
hand-spelled literal is what does. Both are here, and the docstrings say which is which.

---

**Group D's cause, in one line: every `harness`, `cwd` and chat id in the suite is already
one line of printable ASCII, so `contain.one_line` and `contain.readable` are the identity
function on every input the tests supply.** These values come off the reopen manifest — a
plain file that outlives the process and can be older than the charter reading it, the same
route the manifest parser was pinned for on #796 — and they reach a **terminal**. #453's
position: a committed value charter prints back is a value that must not be able to own the
line. One hostile manifest covers all of them.
"""

from __future__ import annotations

import unittest
from types import SimpleNamespace
from unittest import mock

from charter import commands_frame, config, contain, util
from charter import workspace as ws_mod
from charter.frame import builtin_actions, component, leave, palette, reopen, state

from tests._isolation import PersonaIso

SERVER = commands_frame.SOCKET

#: One escape sequence and one newline, in a value charter reads off a committed file and
#: prints back. The escape is `ESC [ 2 J` — erase the whole display — because a report line
#: that let it through would not merely look wrong, it would take the operator's screen.
HOSTILE = "claude\x1b[2Jcode\nrm -rf /"


def _doomed(**kw):
    base = dict(chat="alpha.1", workspace="alpha", persona="", harness="claude-code",
                cwd="/tmp", resume="", server=SERVER, live=True, active=False,
                exit_code=None, closed=False, homeless=False, cwd_gone=False,
                cwd_outside=False)
    base.update(kw)
    return leave.Doomed(**base)


def _plan(*doomed, focus="alpha"):
    return leave.Plan(chats=tuple(doomed), focus=focus)


class TheFourResumeSentencesAreTheseWords(PersonaIso):
    """§4f's four sentences, at `leave.note`, spelled out.

    `tests/test_a_reopen_says_what_it_cannot_bring_back.py` already asserts that each of
    the four is chosen for the right chat — and it asserts it against the constant, so it
    would stay green through any reword. What each one SAYS is here.
    """

    def test_a_chat_with_an_id_is_promised_its_conversation(self):
        self.assertEqual(leave.note(_doomed(resume="conv-1")), "conversation resumes")

    def test_a_chat_with_no_id_yet_says_so_and_names_no_harness(self):
        self.assertEqual(leave.note(_doomed(harness="claude-code", resume="")),
                         "reopens empty — no session id recorded for this chat yet")

    def test_a_chat_whose_harness_charter_forgot_says_that_instead(self):
        self.assertEqual(leave.note(_doomed(harness="", resume="")),
                         "reopens empty — charter has no record of this chat's harness")

    def test_a_chat_with_no_recorded_workspace_says_it_cannot_be_reopened(self):
        self.assertEqual(leave.note(_doomed(workspace="")),
                         "charter has no record of its workspace — it cannot be reopened")


class TheConfirmationsOwnWordsAreTheseWords(PersonaIso):
    def test_a_plane_with_nothing_open_says_so_on_its_one_row(self):
        rows = leave.confirm_rows(_plan(), verb=leave.QUIT)

        self.assertEqual(len(rows), 1)
        self.assertEqual(rows[0].title,
                         "no chats are open on this plane — nothing to quit")

    def test_the_summary_of_an_empty_plane_is_the_same_sentence(self):
        self.assertEqual(leave.summary(_plan(), verb=leave.QUIT),
                         "no chats are open on this plane — nothing to quit")

    def test_the_two_doorway_rows_say_what_each_one_stops(self):
        quit_row, close_row = leave.open_rows("alpha.1")

        self.assertEqual(quit_row.title,
                         "charter: quit — stop every harness on this plane")
        self.assertEqual(close_row.title,
                         "chat: close — stop this chat and do not bring it back")

    def test_a_close_row_that_cannot_name_its_chat_says_which_command_can(self):
        _quit_row, close_row = leave.open_rows("")

        self.assertTrue(close_row.refused)
        self.assertEqual(close_row.note,
                         "charter cannot tell which chat this palette was opened in, so "
                         "it has no chat to close — `charter frame-close <chat>` names one")


class TheRowIdsAreNotActionIdsAndCannotBecomeOne(PersonaIso):
    """The refutation above, asserted rather than argued, plus the literals.

    Split from the sentences deliberately: these are not prose and nobody reads them, so
    the properties that ARE load-bearing are stated first and the literal second.
    """

    def test_no_leave_row_id_can_be_an_action_id(self):
        """The `:` is the whole mechanism. A provider that shipped an action called
        `leave:quit:go` could take the keypress that stops the plane; `component.usable_id`
        is what makes that unsayable, and it is asked of every id the frame dispatches."""
        for rid in (leave.OPEN_ID.format(leave.QUIT), leave.GO_ID.format(leave.CLOSE),
                    leave.CHAT_ID.format(leave.QUIT, 3)):
            self.assertFalse(component.usable_id(rid), rid)

    def test_the_palette_never_matches_one_of_these_ids_by_typing(self):
        """`palette.matches` gates id-matching on `usable_id`, so `leave` typed in the
        palette reaches these rows through their TITLES or not at all."""
        row = leave.open_rows("alpha.1")[0]
        by_id = palette.matches("leave:quit", row)

        self.assertFalse(by_id)
        self.assertFalse(palette.exact("leave:quit", row))

    def test_a_doorway_and_a_per_chat_row_are_told_apart_by_shape(self):
        """`verb_of` answers for a doorway and `None` for everything else, and that is what
        decides whether Enter opens a confirmation or says a note. It is matched against the
        two ids this module mints rather than by splitting on `:`."""
        doorway, close_doorway = leave.open_rows("alpha.1")
        rows = leave.confirm_rows(_plan(_doomed()), verb=leave.QUIT)
        go, per_chat = rows[0], rows[1]

        self.assertEqual(leave.verb_of(doorway), leave.QUIT)
        self.assertEqual(leave.verb_of(close_doorway), leave.CLOSE)
        self.assertIsNone(leave.verb_of(go))
        self.assertIsNone(leave.verb_of(per_chat))
        self.assertTrue(leave.goes_through(go, leave.QUIT))
        self.assertFalse(leave.goes_through(per_chat, leave.QUIT))
        self.assertTrue(leave.is_row(per_chat))

    def test_the_three_shapes_are_these_three_strings(self):
        """The hand-spelled half, and the only thing that kills a `retune-string` mutant:
        the sweep's re-tuning preserves punctuation, so every shape assertion above holds
        for `mfbwf:{}` too."""
        self.assertEqual(leave.OPEN_ID, "leave:{}")
        self.assertEqual(leave.GO_ID, "leave:{}:go")
        self.assertEqual(leave.CHAT_ID, "leave:{}:c{}")
        self.assertEqual(leave.open_rows("alpha.1")[0].id, "leave:quit")
        rows = leave.confirm_rows(_plan(_doomed()), verb=leave.CLOSE)
        self.assertEqual(rows[0].id, "leave:close:go")
        self.assertEqual(rows[1].id, "leave:close:c0")


class TheReopenCommandsOwnWordsAreTheseWords(PersonaIso):
    def test_a_plane_with_nothing_recorded_names_the_thing_that_records_one(self):
        """`charter reopen` on a plane nobody has quit. The sentence names QUITTING,
        because "nothing to reopen" alone reads as a defect to an operator who has just
        lost a frame — and it names the reattach route for the case that is not a quit."""
        self.assertEqual(
            commands_frame.NOTHING_RECORDED,
            "charter reopen: nothing recorded to put back. A plane is recorded when you "
            "quit it (`F2 → charter: quit`); a terminal that closed on its own only "
            "detached, so its harnesses are still running — `tmux -L charter attach` "
            "reaches them.")

    def test_it_is_the_sentence_the_command_actually_prints(self):
        with mock.patch.object(util, "err") as said:
            self.assertEqual(commands_frame.cmd_reopen(SimpleNamespace()), 1)

        said.assert_called_once()
        self.assertEqual(said.call_args[0][0], commands_frame.NOTHING_RECORDED)


class TheTranscriptRowsOwnWordsAreTheseWords(PersonaIso):
    """`chat: previous transcript`, both of its refusals.

    **And one finding the issue's own reading did not have.** #810 records
    `commands_frame.NO_TRANSCRIPT` and `builtin_actions.NO_TRANSCRIPT` as *"the same
    sentence in two places, whichever is the copy should be importing the other"*. They are
    no longer the same sentence: the palette row's carries the parenthetical
    ``(`F2 → charter: quit`)`` and the command's does not. Both are spelled out here rather
    than reconciled, because reconciling them is a decision about which surface names the
    route — and `builtin_actions.py` is held by another change as this lands. What this
    file does is make the next reword of either one visible.
    """

    def test_the_palette_row_says_when_a_transcript_appears_and_names_the_route(self):
        self.assertEqual(
            builtin_actions.NO_TRANSCRIPT,
            "no previous transcript for this chat — one is captured when a plane is quit "
            "(`F2 → charter: quit`) and offered on the chat that comes back")

    def test_the_command_says_the_same_thing_without_the_route(self):
        self.assertEqual(
            commands_frame.NO_TRANSCRIPT,
            "no previous transcript for this chat — one is captured when a plane is quit, "
            "and offered on the chat that comes back")

    def test_the_two_have_drifted_and_this_is_where_that_is_visible(self):
        """Not an assertion that they SHOULD differ — an assertion that they do, so that
        making them one sentence is a change somebody makes on purpose."""
        self.assertNotEqual(builtin_actions.NO_TRANSCRIPT, commands_frame.NO_TRANSCRIPT)

    def test_a_missing_pager_gives_the_path_rather_than_a_refusal(self):
        self.assertEqual(
            commands_frame.NO_PAGER,
            "charter cannot find `less` to show it in — the captured text is at {path}")

    def test_the_pager_sentence_is_said_with_the_path_filled_in(self):
        state.frame_dir("alpha.1", create=True)
        config.write_for(reopen.transcript_path("alpha.1"), "what was on screen\n")
        path = reopen.transcript_path("alpha.1")

        with mock.patch.object(commands_frame.shutil, "which", return_value=None), \
                mock.patch.object(commands_frame, "_say_on_screen") as said:
            self.assertEqual(
                commands_frame.cmd_transcript(SimpleNamespace(chat="alpha.1")), 0)

        said.assert_called_once()
        self.assertEqual(said.call_args[0][1],
                         f"charter cannot find `less` to show it in — the captured text "
                         f"is at {path}")


class AValueOffTheManifestCannotOwnTheLine(PersonaIso):
    """Group D. One hostile manifest, and every `contain` wrapper on the reopen path.

    The values are the manifest's `harness` and `cwd`. The manifest is a plain file that
    outlives the process and can be older than the charter reading it — the same route
    #796 pinned the parser for — and these two reach a **terminal** rather than a `chdir`,
    which is what made them the acceptable half to defer.

    `contain.readable` escapes by Unicode CATEGORY (`Cc`, `Cf`, `Cs`, `Zl`, `Zp`), so the
    assertions are about the two properties an operator can check — the line stays one
    line, and the escape is SHOWN rather than executed — and never about a codepoint list.
    """

    def setUp(self):
        super().setUp()
        config.private_mkdir(state._root())

    def _warnings(self, chat) -> list[str]:
        said: list[str] = []
        with mock.patch.object(util, "warn", side_effect=said.append), \
                mock.patch.object(commands_frame, "cmd_launch", return_value=1):
            commands_frame._reopen_one(chat)
        return said

    def _chat(self, **kw):
        base = dict(chat="alpha.1", workspace="alpha", persona="", harness="claude-code",
                    cwd=str(config.ROOT), resume="", transcript="", active=True)
        base.update(kw)
        return reopen.Chat(**base)

    def assertContained(self, line: str):
        self.assertNotIn("\n", line, "a report line that can carry a newline is two lines")
        self.assertNotIn("\x1b", line, "the escape reached the terminal unescaped")
        self.assertIn("2J", line, "the escape is SHOWN, so the operator can see what it was")

    def test_a_recorded_harness_charter_cannot_launch_is_shown_not_executed(self):
        said = self._warnings(self._chat(harness=HOSTILE))

        self.assertTrue(said, "the fallback warning is the surface under test")
        self.assertContained(said[0])

    def test_a_hostile_harness_is_still_contained_when_the_plane_has_a_default(self):
        """The SECOND `contain.readable(c.harness)`, on the branch this file's other case
        cannot reach: with a `[harness] default` declared, the reopen falls back to it and
        says so, and that is a different sentence with its own wrapper. Measured — without
        this case, hand-mutating that line leaves the module green."""
        with mock.patch.object(config, "HARNESS", {"default": "claude"}):
            said = self._warnings(self._chat(harness=HOSTILE))

        line = next(s for s in said if "reopening it under" in s)
        self.assertContained(line)

    def test_a_recorded_directory_that_has_gone_is_shown_not_executed(self):
        said = self._warnings(self._chat(cwd=HOSTILE))

        line = next(s for s in said if "directory" in s)
        self.assertContained(line)

    def test_a_directory_charter_cannot_enter_is_shown_not_executed(self):
        """The third wrapper, on the value that reached `os.chdir` and refused.

        **The hostile path is INSIDE the workspace**, and that is #867 rather than a
        detail: a restore only stands in a recorded cwd it has contained
        (`_restore_root`), so the manifest value that can still reach `os.chdir` is one
        under ``workspaces/<ws>/``. A fixture standing outside it would now measure the
        workspace directory charter composed itself, which no manifest can influence and
        which needs no wrapper."""
        where = ws_mod.workspace_dir("alpha") / "clone"
        with mock.patch.object(commands_frame.os, "chdir",
                               side_effect=OSError(13, "refused")), \
                mock.patch.object(commands_frame.os.path, "isdir", return_value=True):
            said = self._warnings(self._chat(cwd=str(where) + HOSTILE))

        line = next(s for s in said if "cannot enter" in s)
        self.assertContained(line)

    def test_a_chat_name_charter_refuses_is_shown_not_executed(self):
        """`charter frame-close <chat>` with a name off the command line — the first
        `contain.one_line(target)`, on the branch that refuses the SHAPE."""
        said: list[str] = []
        with mock.patch.object(util, "err", side_effect=said.append):
            self.assertEqual(
                commands_frame.cmd_close(SimpleNamespace(chat=HOSTILE, chat_id=HOSTILE)), 1)

        self.assertTrue(said)
        self.assertContained(said[0])

    def test_a_chat_name_that_is_shaped_right_and_absurdly_long_is_bounded(self):
        """The second `contain.one_line(target)`, and it is NOT masked by the first.

        `chats.ID_RE` is ``[A-Za-z0-9._-]+`` with no length bound, so a three-hundred
        character id passes the shape check and reaches the "no open chat" refusal intact.
        What the wrapper does there is the OTHER half of `contain`'s job — `DISPLAY_LIMIT`,
        *"a budget a longer input makes longer is not a budget"* — so the case that pins it
        is a long name rather than a hostile one, and a case that reused `HOSTILE` here
        would have been measuring the first branch twice.
        """
        def refusal(n: int) -> str:
            long_id = "a" * n + ".1"
            self.assertTrue(commands_frame.chats.ID_RE.fullmatch(long_id),
                            "or this case never reaches the branch it is about")
            said: list[str] = []
            with mock.patch.object(util, "err", side_effect=said.append), \
                    mock.patch.object(commands_frame, "_plane_servers", return_value=()), \
                    mock.patch.object(commands_frame, "_plane_live",
                                      return_value=(set(), {}, {})):
                self.assertEqual(
                    commands_frame.cmd_close(SimpleNamespace(chat="", chat_id=long_id)), 1)
            self.assertTrue(said)
            return said[0]

        short, long = refusal(400), refusal(4000)

        # The budget property, stated as the thing it is: ten times the input, the same
        # line. An assertion against a fixed number would pass for a wrapper that merely
        # clipped to something generous, and would move every time the sentence is edited.
        self.assertEqual(len(short), len(long))
        self.assertIn("no open chat", short)
        self.assertNotIn("a" * 400, short)


class TheContainmentIsNotDecorative(PersonaIso):
    """The other half of group D: what the wrappers actually do to a hostile value.

    Asked of `contain` directly as well as through the report line, because a case that
    only reads the assembled sentence cannot say whether the containment or the f-string's
    own shape kept the line intact.
    """

    def test_readable_escapes_the_escape_and_the_newline(self):
        out = contain.readable(HOSTILE)

        self.assertNotIn("\x1b", out)
        self.assertNotIn("\n", out)
        self.assertIn("code", out, "the readable part survives")

    def test_one_line_escapes_the_escape_and_the_newline(self):
        out = contain.one_line(HOSTILE)

        self.assertNotIn("\x1b", out)
        self.assertNotIn("\n", out)
        self.assertIn("code", out)


class ACaptureCharterCannotEncodeStillLands(PersonaIso):
    """#810's odd one out: `commands_frame`'s ``text.encode("utf-8", "replace")``.

    The issue files it with the sentences because the sweep's `retune-string` operator is
    what reaches it, but it is not prose — it is the error handler on the encode that turns
    a captured pane into the bytes the byte-exact truncation slices. `str.encode` looks a
    handler up **lazily**, only when an error actually occurs (measured: `"abc".encode(
    "utf-8", "nosuchhandler")` returns `b"abc"`, and the same call on a string holding a
    lone surrogate raises `LookupError`), which is exactly why no test in the suite could
    redden it: every capture the suite supplies is encodable, so the name is never read.

    So the case that pins it hands `_capture_transcript` a capture that cannot be encoded,
    through the seam every other case in `TheCaptureIsBoundedAndNeverRaises` already uses.
    **Stated honestly: today's decode path cannot produce such a string** — and the reason
    moved with #828. It used to be that `tmuxctl.run` decoded STRICTLY, so a pane charter
    could not read raised there rather than arriving here as surrogates; that was the defect
    #828 fixed, because the raise landed in the middle of a quit. It now decodes with
    `tmuxctl.DECODE_ERRORS`, which substitutes U+FFFD — a character that encodes perfectly
    well — so a lone surrogate still cannot reach this line, and the handler is still a
    floor. What it is a floor FOR is what changed: not "the day the read stops raising", but
    "the day the read stops replacing".

    The invariant is the one the class one file over is named for: **the capture never
    raises.** A quit that tracebacks on the way out is the one failure §4e cannot afford,
    because the operator has already asked for the plane to be recorded.
    """

    def setUp(self):
        super().setUp()
        config.private_mkdir(state._root())
        self.dest = reopen.transcript_path("alpha.1")

    def _capture(self, stdout: str) -> bool:
        answer = SimpleNamespace(returncode=0, stdout=stdout, stderr="")
        with mock.patch.object(commands_frame.tmuxctl, "run", return_value=answer):
            return commands_frame._capture_transcript(SERVER, "%1", self.dest)

    def test_a_capture_that_cannot_be_encoded_is_written_rather_than_raised(self):
        text = "before\udcff after\n"
        with self.assertRaises(UnicodeEncodeError):
            text.encode("utf-8")      # or this case is not about what it says it is

        self.assertTrue(self._capture(text))

        got = self.dest.read_text()
        self.assertIn("before", got)
        self.assertIn("after", got)
        self.assertNotIn("\udcff", got)



if __name__ == "__main__":
    unittest.main()
