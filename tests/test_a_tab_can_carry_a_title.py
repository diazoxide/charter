"""**A tab can carry a name a person chose, and the id keeps doing the linking.**

Until now a chat tab was its bare id: there was no title and no rename anywhere in the frame
(`chats.py`'s own docstring said so, and `tabmenu.py`'s said *no rename anywhere*). Task 3 of
the one-exit-gate plan gives a chat an optional title (decision 11), set from a rename row in
the tab menu and in `F2`, from a title row at `+`, or from the first line of a handoff's
brief.

What this module pins, in the order the rulings state it:

* **The id is never replaced by the title** (ruling 1). Every link, record, kill, reap and
  claim goes on using the id; the title is display only, and a chat with no title behaves
  exactly as it did before this existed. The strip is the one surface where the title stands
  in place of the id — and even there the click map is keyed by the id.
* **A title is text a person wrote, so it is bounded on the way in and contained on the way
  out** (ruling 2, ruling 35). One line, printable, at most `state.TITLE_MAX`; and every
  drawn title goes through `contain.readable`, because `title` is a file a chat can write.
* **Renaming touches no harness** (ruling 3). No `send-keys`, no `/rename`, no respawn. The
  new name reaches Claude Code at its next start or resume, through `--name` again.
* **Codex and opencode take no name at launch** (ruling 4), so their tabs carry the title in
  charter's own surfaces only.
"""

from __future__ import annotations

import os
import unittest
from types import SimpleNamespace
from unittest import mock

from charter import cli, commands_frame, config, contain
from charter.frame import (builtin_actions, chats, leave, overlay, rename, slots, state,
                           tabmenu, tmuxctl)

from tests._isolation import (PersonaIso, approve_every_profile, declare_profiles,
                              wired_as_today)
from tests._ttyguard import no_terminal


#: A server these chats record, standing in for a plane's own. A NAME no test starts a server
#: on: nothing in this module reaches tmux, and `tests._planeguard` refuses the shape if one
#: ever did.
SERVER = "charter-plane-5e77e45e77e4"


def _plant(fid: str, *, ws: str = "beta", profile: str = "claude",
           harness: str = "claude-code") -> None:
    """One chat on the plane, recorded the way a launch records it."""
    state.frame_dir(fid, create=True)
    state.record_workspace(fid, ws)
    state.record_server(fid, SERVER)
    state.record_harness_pane(fid, "%1")
    state.record_profile(fid, profile)
    state.record_identity(fid, {"CHARTER_HARNESS": harness})


class TheTitleIsStoredAndContained(PersonaIso, unittest.TestCase):
    """`state.record_title` is the one gate, and `rename.normalized` is the one bound."""

    def setUp(self):
        super().setUp()
        _plant("beta.1")

    def test_a_title_is_recorded_and_read(self):
        """The round trip, and the file it lands in spelled out. The path is named literally
        rather than through `state.frame_dir`, because "where a chat's title lives" is the
        thing a reader of a plane's state directory has to be able to find."""
        self.assertTrue(state.record_title("beta.1", "fix the widget"))
        self.assertEqual(state.title("beta.1"), "fix the widget")
        here = state._root() / "beta.1" / "title"
        self.assertTrue(here.is_file(), f"no title file at {here}")
        self.assertEqual(here.parent.parent.name, "frame")
        self.assertEqual(here.parent.parent.parent.name, ".charter")

    def test_an_unnamed_chat_answers_none_and_draws_its_id(self):
        """The ordinary chat, and the one every surface has to keep behaving for."""
        self.assertIsNone(state.title("beta.1"))
        self.assertEqual(chats.title_of("beta.1"), "")
        self.assertEqual(chats.label_of("beta.1"), "beta.1")
        self.assertEqual(chats.named("beta.1"), "beta.1",
                         "an untitled chat grew a separator with nothing after it")

    def test_empty_removes_it(self):
        """`""` is how a rename takes a title OFF, and the file goes with it: `state.title`
        already reads an empty file as no title, so leaving one behind would be a second
        spelling of the same state."""
        state.record_title("beta.1", "fix the widget")
        self.assertTrue(state.record_title("beta.1", ""))
        self.assertIsNone(state.title("beta.1"))
        self.assertFalse((state._root() / "beta.1" / "title").exists())

    def test_the_same_title_again_changes_nothing(self):
        """The answer `cmd_rename` bumps every strip on the plane off. Without it a rename to
        the name a tab already carries wakes every panel on the plane to redraw the row it is
        already drawing."""
        self.assertTrue(state.record_title("beta.1", "fix it"))
        self.assertFalse(state.record_title("beta.1", "fix it"))
        self.assertFalse(state.record_title("beta.1", "  fix   it  "),
                         "the collapse is asked before the comparison, or it is not one rule")
        self.assertEqual(state.title("beta.1"), "fix it")

    def test_a_newline_becomes_one_line(self):
        """A title cannot hold a line break at all, so it cannot forge a second row of a strip
        or of a menu. Every run of whitespace collapses — including U+2028 and U+0085, which
        are line separators `str.split` knows about and a naive `replace("\\n", " ")` does
        not."""
        for typed in ("a\nb", "a\u2028b", "a\x85b", "a\tb", "a\xa0b"):
            with self.subTest(typed=typed):
                # Taken off between rounds, because `record_title` answers "the record
                # moved" and every one of these collapses to the same title.
                state.record_title("beta.1", "")
                self.assertTrue(state.record_title("beta.1", typed))
                got = state.title("beta.1")
                self.assertEqual(got, "a b")
                self.assertNotIn("\n", got)

    def test_a_control_byte_is_refused(self):
        """**Refused, not escaped**, which is the one place this bound differs from
        `contain.one_line`. That function would write an ESC into the record as the six
        characters ``\\x1b`` and call it contained; an operator who typed a control byte at
        `charter frame-rename` did not ask for a title with a backslash in it."""
        state.record_title("beta.1", "fix it")
        self.assertFalse(state.record_title("beta.1", "a\x1b[2Kb"))
        self.assertEqual(state.title("beta.1"), "fix it",
                         "a refused rename overwrote the title that was there")
        shown, why = rename.normalized("a\x1b[2Kb")
        self.assertIsNone(shown)
        self.assertEqual(why, rename.NOT_PRINTABLE)

    def test_a_character_with_no_glyph_and_no_whitespace_is_refused(self):
        """The half `contain.one_line` could not have caught and the half it could.

        A zero-width joiner is `Cf` — `one_line` escapes it, `str.split` does not touch it,
        and `isprintable()` is False — so the printable bound is what refuses it. A private-use
        codepoint is `Co`, which `one_line` does NOT escape at all: it would have gone into the
        record unchanged. Both are refused here by one rule.
        """
        for typed in ("a\u200db", "a\u200bb", "a\ue000b", "a\udcffb".encode(
                "utf-8", "surrogatepass").decode("utf-8", "surrogatepass")):
            with self.subTest(typed=typed):
                self.assertFalse(state.record_title("beta.1", typed))
                self.assertIsNone(state.title("beta.1"))

    def test_a_title_over_the_limit_is_refused_with_its_length(self):
        """**Bounded and never clipped**, and the refusal counts: a title silently cut is a
        title the operator believes they set, and "too long" with no number is a refusal
        nobody can act on."""
        self.assertEqual(state.TITLE_MAX, 60)
        self.assertTrue(state.record_title("beta.1", "x" * state.TITLE_MAX))
        self.assertFalse(state.record_title("beta.1", "y" * (state.TITLE_MAX + 1)))
        self.assertEqual(state.title("beta.1"), "x" * state.TITLE_MAX)
        shown, why = rename.normalized("y" * 61)
        self.assertIsNone(shown)
        self.assertIn("this one is 61", why)
        self.assertIn("60", why)

    def test_the_length_is_measured_after_the_collapse(self):
        """A title counted before its whitespace was collapsed would be refused for length it
        does not have — sixty characters typed with a stray double space between two of
        them."""
        typed = "  ".join(["x"] * 30)            # 59 characters, 30 of them spaces doubled
        self.assertGreater(len(typed), state.TITLE_MAX)
        self.assertTrue(state.record_title("beta.1", typed))
        self.assertEqual(state.title("beta.1"), " ".join(["x"] * 30))

    def test_a_brief_first_line_is_cut_not_refused(self):
        """A brief is the whole message a model wrote and the operator approved at the harness
        prompt — not a label somebody typed — so its first line is CUT to fit rather than
        refused, and the whole answer including the marker fits `TITLE_MAX`."""
        brief = "\n\n" + ("fix the widget " * 20) + "\nand then some\n"
        got = rename.first_line_title(brief)
        self.assertLessEqual(len(got), state.TITLE_MAX)
        self.assertTrue(got.endswith("..."), got)
        self.assertTrue(state.record_title("beta.1", got),
                        "a brief's own title was refused by the gate it has to pass")
        self.assertEqual(state.title("beta.1"), got)

    def test_a_brief_that_titles_nothing_titles_nothing(self):
        """Every open that is not a handoff carries no brief, and an all-blank one is the same
        answer. `contain.readable` would render it as its own `""` marker, which is right for a
        sentence naming a value and wrong for a tab."""
        for brief in ("", "\n\n  \n"):
            with self.subTest(brief=brief):
                self.assertEqual(rename.first_line_title(brief), "")
        self.assertFalse(state.record_title("beta.1", rename.first_line_title("")))
        self.assertIsNone(state.title("beta.1"))

    def test_a_brief_carrying_a_terminal_escape_leaves_none_in_the_label(self):
        """A brief is text a model wrote (ADR 0021 approves the MESSAGE, not a tab label), so
        the one thing it may not do is repaint the strip it is drawn on."""
        got = rename.first_line_title("\x1b]0;x\x07 do the thing\n")
        self.assertNotIn("\x1b", got)
        self.assertNotIn("\x07", got)
        state.record_title("beta.1", got)
        self.assertNotIn("\x1b", chats.label_of("beta.1"))

    def test_a_drawn_title_is_escaped(self):
        """**The file is one a chat can write** (ruling 35), so the bound at `record_title` is
        not the whole answer: a title hand-written into `.charter/frame/<id>/title` never went
        through it, and what draws is what has to contain."""
        (state._root() / "beta.1" / "title").write_text(
            "ow\x1b[2Kned\n", encoding="utf-8")
        self.assertEqual(state.title("beta.1"), "ow\x1b[2Kned",
                         "the reader repaired the value instead of the drawer escaping it")
        for shown in (chats.title_of("beta.1"), chats.label_of("beta.1"),
                      chats.named("beta.1")):
            self.assertNotIn("\x1b", shown)
        self.assertIn("beta.1", chats.named("beta.1"))

    def test_a_hand_written_title_is_bounded_where_it_is_drawn(self):
        """The bound on the way out, which is not a restatement of the bound on the way in:
        `record_title` bounds what an operator can TYPE, and this bounds what charter draws
        from a file nothing charter wrote. Without it a 5,000-character `title` would reach
        `contain.DISPLAY_LIMIT` — 160 columns of somebody else's words across a tab strip."""
        (state._root() / "beta.1" / "title").write_text("z" * 5000, encoding="utf-8")
        shown = chats.title_of("beta.1")
        self.assertLessEqual(len(shown), state.TITLE_MAX + len("..."))
        self.assertTrue(shown.endswith("..."), shown)

    def test_a_title_is_never_a_path_and_never_a_chat(self):
        """`record_title` writes under a chat's own directory and nowhere else: the id goes
        through `state.frame_dir`, which is `contain.child`, so a name that is a path names no
        directory and writes nothing."""
        for bad in ("../x", "beta.1/../../x", "/etc/x", ""):
            with self.subTest(bad=bad):
                self.assertFalse(state.record_title(bad, "fix it"))
                self.assertIsNone(state.title(bad))

    def test_contain_readable_is_what_a_drawn_title_goes_through(self):
        """Named rather than implied: a value that renders as nothing is a tab with no name on
        it, which `contain.one_line` cannot prevent — U+3164 HANGUL FILLER is `Lo`, is not
        whitespace, survives `strip`, and is `isprintable()`, so it passes the gate."""
        self.assertTrue(state.record_title("beta.1", "\u3164\u3164"))
        self.assertEqual(chats.title_of("beta.1"),
                         contain.readable("\u3164\u3164", state.TITLE_MAX))
        self.assertNotEqual(chats.label_of("beta.1").strip(), "")


class TheRowsAChatIsNamedOn(PersonaIso, unittest.TestCase):
    """The confirmation rows a quit and a close draw — `leave.title`."""

    def setUp(self):
        super().setUp()
        _plant("beta.1")

    def _doomed(self, **over):
        fields = dict(chat="beta.1", workspace="beta", persona="", harness="claude-code",
                      cwd="", resume="", server=SERVER, live=True, active=False,
                      exit_code=None, closed=False, homeless=False, cwd_gone=False,
                      cwd_outside=False, profile="claude-work")
        fields.update(over)
        return leave.Doomed(**fields)

    def test_the_row_names_the_id_then_the_title_then_the_profile(self):
        """The id FIRST and the title after it (ruling 1): this row is the last thing read
        before a harness is stopped, and the id is what every refusal and `charter
        frame-close` name that chat by."""
        self.assertEqual(leave.title(self._doomed(title="fix it")),
                         "beta.1 · fix it · claude-work")

    def test_an_untitled_chat_reads_exactly_as_it_did(self):
        """The floor. Empty parts drop with their separator, so a chat nobody named and a chat
        from before profiles each read as they did before this field existed."""
        self.assertEqual(leave.title(self._doomed()), "beta.1 · claude-work")
        self.assertEqual(leave.title(self._doomed(profile="")), "beta.1 · claude-code")
        self.assertEqual(leave.title(self._doomed(profile="", harness="")), "beta.1")
        self.assertEqual(leave.title(self._doomed(profile="", harness="", title="fix it")),
                         "beta.1 · fix it")

    def test_the_plan_reads_the_title_contained(self):
        """`leave.plan` is what builds these rows, and it reads the drawn form — the row is the
        one reader of this field."""
        (state._root() / "beta.1" / "title").write_text("ow\x1b[2Kned\n", encoding="utf-8")
        got = leave.plan(live={"beta.1"}, focus="beta")
        self.assertEqual([c.chat for c in got.chats], ["beta.1"])
        self.assertNotIn("\x1b", got.chats[0].title)
        self.assertIn("beta.1", leave.title(got.chats[0]))


class _AWatchedSpawn(PersonaIso):
    """Every case below records what was started instead of starting it.

    `builtin_actions._spawn` is the one door a rename goes out of — a `Popen` argv and no
    tmux — so what is asserted is the argv that would have run. `tmuxctl.run` is watched
    beside it, because *renaming touches no harness* (ruling 3) is a claim about what did NOT
    happen and an unrecorded tmux call is exactly what would falsify it.
    """

    def setUp(self):
        super().setUp()
        self.spawned: list[tuple[list[str], str]] = []
        self.tmux: list[list[str]] = []
        self.enterContext(mock.patch.object(
            builtin_actions, "_spawn",
            side_effect=lambda argv, *, fid: self.spawned.append((list(argv), fid))))
        self.enterContext(mock.patch.object(
            tmuxctl, "run",
            side_effect=lambda why, argv, **kw: self.tmux.append(list(argv)) or _NOTHING))
        self.said: list[tuple[str, str]] = []
        self.enterContext(mock.patch.object(
            commands_frame, "_say_on_screen",
            side_effect=lambda fid, message, **kw: self.said.append((fid, message))))


class _Nothing:
    """What the watched `tmuxctl.run` answers: rc 0 and no output. `tmuxctl.run`'s own three
    exits all return a `str` for `stdout`, so nothing downstream has to branch."""

    returncode = 0
    stdout = ""
    stderr = ""


_NOTHING = _Nothing()


class RenameFromTheTabMenuAndF2(_AWatchedSpawn, unittest.TestCase):
    """The two doorways, the one input behind them, and the command they spawn."""

    def setUp(self):
        super().setUp()
        for chat in ("beta.1", "beta.2"):
            _plant(chat)

    def test_the_tab_menu_carries_rename_and_close_stays_last(self):
        """The destructive row keeps the bottom of the list: `palette.aim` opens the cursor on
        the first row that can run, and rename can always run — so it is the row Enter lands
        on for a chat with no capture, which used to be close."""
        self.assertEqual([r.id for r in tabmenu.catalogue("beta.1")],
                         [tabmenu.TRANSCRIPT_ID, tabmenu.RENAME_ID, tabmenu.CLOSE_ID])
        state.claim_ended("beta.1")
        self.assertEqual([r.id for r in tabmenu.catalogue("beta.1")][-1],
                         tabmenu.CLOSE_NOW_ID)

    def test_the_palette_carries_rename_before_the_leaving_rows(self):
        """`leave.open_rows` stays last, so `F2 Enter` reaches a destructive row no more
        easily than it did."""
        from charter.frame import builtin_actions as ba
        reg = ba.build("beta.1", current_density="", current_chrome="")
        ids = [r.id for r in commands_frame._palette_catalogue("beta.1", reg, snapshot={})]
        self.assertIn(rename.OPEN_ID, ids)
        self.assertLess(ids.index(rename.OPEN_ID), ids.index(leave.OPEN_ID.format("quit")))
        self.assertEqual(ids[-1], leave.OPEN_ID.format("close"))

    def test_the_rename_row_is_a_doorway(self):
        """A doorway starts nothing — it replaces the surface in the pane the operator is
        already looking at. `chose` is what STARTS work, and it does not recognise the row
        that merely opens."""
        row = rename.open_rows("beta.1")[0]
        opened = rename.opens(row, "beta.1")
        self.assertIsInstance(opened, rename.Rename)
        self.assertEqual(opened.target, "beta.1")
        self.assertFalse(rename.chose(row, "beta.1", fid="beta.1", text="fix it"))
        self.assertEqual(self.spawned, [])

    def test_a_palette_that_cannot_name_its_chat_lists_the_row_with_its_reason(self):
        """#512: an option you cannot see is one you cannot ask about — and a surface over a
        target charter cannot name is an offer it already knows it cannot honour."""
        row = rename.open_rows("")[0]
        self.assertTrue(row.refused)
        self.assertEqual(row.note, rename.NO_CHAT_HERE)
        self.assertIsNone(rename.opens(row, ""))

    def test_the_input_shows_what_was_typed_and_does_not_filter_itself_away(self):
        """`Palette._refilter` narrows the catalogue by the query, which on a one-row surface
        would make the row vanish the moment anything is typed — an input that erases
        itself."""
        surface = rename.Rename(target="beta.1")
        self.assertIn(rename.NOTHING_TYPED, surface.rows[0].title)
        for ch in "fix it":
            surface.handle(overlay.Event(kind=overlay.KEY, name=ch), 24)
        self.assertEqual(len(surface.rows), 1)
        self.assertEqual(surface.rows[0].id, rename.GO_ID)
        self.assertEqual(surface.rows[0].title, rename.TYPED.format(text="fix it"))
        self.assertEqual(surface.typed(), "fix it")

    def test_a_refused_title_stays_in_the_footer_and_keeps_what_was_typed(self):
        """Ruling 6, one surface over: a surface that closed on a refusal would throw away
        sixty characters to tell the operator one of them was wrong."""
        surface = rename.Rename(target="beta.1")
        for ch in "y" * 61:
            surface.handle(overlay.Event(kind=overlay.KEY, name=ch), 24)
        back = rename.again(surface.rows[0], surface)
        self.assertIs(back, surface)
        self.assertIn("this one is 61", surface.footer)
        self.assertEqual(surface.typed(), "y" * 61)
        surface.handle(overlay.Event(kind=overlay.KEY, name="backspace"), 24)
        self.assertEqual(surface.footer, rename.FOOTER,
                         "the reason outlived the keystroke it was about")

    def test_a_title_that_fits_lets_the_enter_through(self):
        """The other half of the pair: `again` answers ``None`` so `own_the_tty`'s loop ends
        and the caller spawns. Without it a title charter accepts would redraw forever."""
        surface = rename.Rename(target="beta.1")
        for ch in "fix it":
            surface.handle(overlay.Event(kind=overlay.KEY, name=ch), 24)
        self.assertIsNone(rename.again(surface.rows[0], surface))

    def test_enter_spawns_frame_rename_with_the_text_on_argv_not_tmux(self):
        """**The one place a person's words go is a child's own `sys.argv`.** No `-e`, no
        `split-window`, no `set-environment`: `layout.CARRIABLE` is unchanged and nothing new
        crosses tmux but flags and closed-alphabet ids."""
        surface = rename.Rename(target="beta.2")
        for ch in "fix it":
            surface.handle(overlay.Event(kind=overlay.KEY, name=ch), 24)
        self.assertTrue(rename.chose(surface.rows[0], surface.target, fid="beta.1",
                                     text=surface.typed()))
        self.assertEqual(len(self.spawned), 1)
        argv, fid = self.spawned[0]
        self.assertEqual(argv[-6:], ["frame-rename", "--chat", "beta.1", "beta.2", "--",
                                     "fix it"])
        self.assertEqual(fid, "beta.1")
        self.assertEqual(self.tmux, [], "a rename reached tmux")

    def test_the_tab_menu_opens_the_same_input_over_the_tab_that_was_clicked(self):
        """The whole difference between the two doorways: `F2` renames the chat it was opened
        IN, and the menu renames the tab the pointer landed on."""
        row = [r for r in tabmenu.catalogue("beta.2") if r.id == tabmenu.RENAME_ID][0]
        opened = tabmenu.opens(row, "beta.2", live=None)
        self.assertIsInstance(opened, rename.Rename)
        self.assertEqual(opened.target, "beta.2")
        self.assertIn("beta.2", row.title)

    def test_the_menu_acts_on_the_typed_text(self):
        """`tabmenu.act` is where a chosen row becomes work, and the text rides in beside it
        because what `own_the_tty` hands back is a ROW."""
        tabmenu.act(overlay.Row(id=rename.GO_ID, title="title: fix it"), "beta.2",
                    fid="beta.1", typed="fix it")
        self.assertEqual(self.spawned[0][0][-6:],
                         ["frame-rename", "--chat", "beta.1", "beta.2", "--", "fix it"])

    def test_the_command_records_bumps_and_says_when_claude_sees_it(self):
        """What `charter frame-rename` does, and the whole of it: one file, one bump, one
        repaint, one sentence."""
        was = state.version("beta.2")
        commands_frame.cmd_rename(SimpleNamespace(chat_id="beta.1", chat="beta.1",
                                                  title=["fix", "it"]))
        self.assertEqual(state.title("beta.1"), "fix it")
        self.assertNotEqual(state.version("beta.2"), was,
                            "the other tabs were never told to repaint")
        self.assertEqual(len(self.said), 1)
        self.assertIn("next time it starts or resumes", self.said[0][1])
        self.assertIn("claude-code", self.said[0][1])

    def test_a_harness_that_takes_no_name_is_promised_nothing(self):
        """Ruling 4. Codex and opencode take no name at launch, so a rename reaches them in
        charter's own surfaces only — and the notice may not imply otherwise."""
        _plant("beta.2", harness="codex")
        commands_frame.cmd_rename(SimpleNamespace(chat_id="beta.2", chat="beta.1",
                                                  title=["fix", "it"]))
        self.assertEqual(self.said[-1][1], rename.RENAMED)
        self.assertFalse(rename.takes_a_name("codex"))
        self.assertFalse(rename.takes_a_name("opencode"))
        self.assertTrue(rename.takes_a_name("claude-code"))

    def test_the_same_title_again_repaints_nothing(self):
        """`state.record_title` answers whether the record moved, and this is what reads it: a
        rename to the name a tab already carries would otherwise wake every panel on the
        plane to redraw the row it is already drawing."""
        commands_frame.cmd_rename(SimpleNamespace(chat_id="beta.1", chat="beta.1",
                                                  title=["fix", "it"]))
        was, was_other = state.version("beta.1"), state.version("beta.2")
        commands_frame.cmd_rename(SimpleNamespace(chat_id="beta.1", chat="beta.1",
                                                  title=["fix", "it"]))
        self.assertEqual(state.version("beta.1"), was)
        self.assertEqual(state.version("beta.2"), was_other)

    def test_a_rename_never_touches_the_harness(self):
        """Ruling 3 and ADR 0018, asserted as the negative it is — and paired with its
        positive, so a mutation that removed the whole rename would not pass both."""
        commands_frame.cmd_rename(SimpleNamespace(chat_id="beta.1", chat="beta.1",
                                                  title=["fix", "it"]))
        self.assertEqual(state.title("beta.1"), "fix it")
        self.assertEqual(self.spawned, [])
        for argv in self.tmux:
            self.assertNotIn("send-keys", argv)
            self.assertNotIn("respawn-pane", argv)
            self.assertNotIn("respawn-window", argv)
            self.assertNotIn("kill-pane", argv)

    def test_a_name_that_is_not_a_chat_is_refused(self):
        """The id is held to `chats.ID_RE` and must name a chat of THIS plane — the last point
        before a name off an argv becomes a state path."""
        for bad in ("../x", "beta.9", "api.1"):
            with self.subTest(bad=bad):
                self.said.clear()
                commands_frame.cmd_rename(SimpleNamespace(chat_id=bad, chat="beta.1",
                                                          title=["fix", "it"]))
                self.assertIn("no chat", self.said[-1][1])
        self.assertIsNone(state.title("beta.1"))

    def test_a_chat_still_at_the_selector_can_be_named(self):
        """`leave.plane_chats` and not `leave.plan`, and this is the difference: a plan drops a
        pane that is still WAITING at the selector, which is exactly the tab a title typed at
        `+` is about."""
        state.record_waiting("beta.2")
        commands_frame.cmd_rename(SimpleNamespace(chat_id="beta.2", chat="beta.1",
                                                  title=["fix", "it"]))
        self.assertEqual(state.title("beta.2"), "fix it")

    def test_a_refused_title_is_said_where_the_keypress_came_from(self):
        """The hand-typed route has no footer to put a reason in, so the one rule is asked
        again where there is somebody to tell."""
        commands_frame.cmd_rename(SimpleNamespace(chat_id="beta.1", chat="beta.1",
                                                  title=["y" * 61]))
        self.assertIn("this one is 61", self.said[-1][1])
        self.assertIsNone(state.title("beta.1"))

    def test_a_title_that_starts_with_a_dash_is_a_title(self):
        """`nargs=REMAINDER` after `--` is what makes a person's words their own vocabulary
        rather than charter's."""
        parser = cli.build_parser()
        args = parser.parse_args(["frame-rename", "beta.1", "--", "--fix", "the", "widget"])
        self.assertEqual(args.chat_id, "beta.1")
        commands_frame.cmd_rename(args)
        self.assertEqual(state.title("beta.1"), "--fix the widget")

    def test_a_hand_typed_title_needs_no_separator(self):
        """**`nargs=REMAINDER` and not `nargs="*"`**, which is where the two come apart: a
        title typed without the `--` still reaches the command rather than being refused as an
        unknown option. A person's words are not charter's vocabulary."""
        args = cli.build_parser().parse_args(
            ["frame-rename", "beta.1", "--fix", "the", "widget"])
        commands_frame.cmd_rename(args)
        self.assertEqual(state.title("beta.1"), "--fix the widget")

    def test_what_the_surface_spawns_parses_back_to_what_was_typed(self):
        """**The whole trip, end to end, through the real parser** — and the one case in this
        module that is about a Python version rather than about charter.

        `nargs=REMAINDER` swallows everything from the first token it reaches, and whether
        argparse hands the `--` separator over with it depends on the shape of the parser:
        measured here, `frame-launch`'s lone `rest` positional KEEPS it and this parser's
        `chat_id` + `title` pair consumes it. `cmd_rename` therefore strips nothing, which is
        right only while that holds — so the argv `rename.chose` really emits is parsed by the
        real parser and run, on every Python CI builds against.
        """
        for typed in ("fix it", "--fix it", "--", "a  b"):
            with self.subTest(typed=typed):
                self.spawned.clear()
                state.record_title("beta.1", "")
                rename.chose(overlay.Row(id=rename.GO_ID, title=""), "beta.1",
                             fid="beta.1", text=typed)
                argv = self.spawned[0][0]
                args = cli.build_parser().parse_args(argv[argv.index("frame-rename"):])
                self.assertEqual(args.chat, "beta.1")
                self.assertEqual(args.chat_id, "beta.1")
                commands_frame.cmd_rename(args)
                shown, _why = rename.normalized(typed)
                self.assertEqual(state.title("beta.1"), shown or None)


class WhereATitleIsShown(PersonaIso, unittest.TestCase):
    """A title is drawn wherever a chat is named to a person, and NOWHERE else.

    The controller's ruling on the spec's first review: the strip (in place of the id), then
    after the id in the tab menu's label, the quit and close rows, the ended selector's resume
    row and drawer, and the chat picker. A workspace tab's selector names profiles, not chats,
    so it shows no chat's title at all.
    """

    def setUp(self):
        super().setUp()
        for chat in ("beta.1", "beta.2"):
            _plant(chat)
        slots.TABS.forget()
        self.addCleanup(slots.TABS.forget)

    def test_the_strip_draws_the_title_instead_of_the_id(self):
        """Decision 11's *shown in: the strip*, and it is the one surface where the title
        stands IN PLACE of the id: a strip is the row where every column is contested, and
        `beta.1 - fix the widget` on fifteen tabs is a strip that cuts to a count and shows
        neither."""
        state.record_title("beta.1", "fix the widget")
        row = slots.chats_bar("beta.1", 200)[0]
        self.assertIn("fix the widget", row)
        self.assertNotIn("beta.1", row)
        self.assertIn("beta.2", row, "the untitled sibling stopped drawing its id")

    def test_a_click_on_the_title_resolves_to_the_chat(self):
        """**The map stays keyed by the id** (ruling 1). A press on the words has to reach the
        chat, never what it is called — which is what makes the title display rather than
        identity, structurally rather than by promise."""
        state.record_title("beta.1", "fix the widget")
        slots.chats_bar("beta.1", 200)
        hit = {slots.TABS.tab_at(0, c) for c in range(200)}
        self.assertIn("beta.1", hit, "a press on the title resolved to no chat")
        self.assertEqual({h for h in hit if h}, {"beta.1", "beta.2"},
                         "a cell resolved to something that is not a chat id")

    def test_the_sizer_measures_the_label_the_renderer_draws(self):
        """The sixth field is not a renderer-only extra: `bar_rows_wanted` runs in the
        LAUNCHER and composes the same ladder, so a strip whose titles it could not see would
        be measured at the width of its ids and drawn at the width of its names."""
        # 60 columns: two bare ids fit on one row, and the two titles below need two. Narrower
        # than that and the ladder has given up on both alike (it draws a count, then
        # nothing), which is the same answer for both and would measure nothing.
        narrow = slots.bar_rows_wanted("beta.1", "chats", pane_cols=60, cap=4)
        self.assertEqual(narrow, 1)
        state.record_title("beta.1", "fix the widget for the third time now")
        state.record_title("beta.2", "and another long one that will not fit")
        self.assertGreater(slots.bar_rows_wanted("beta.1", "chats", pane_cols=60, cap=4),
                           narrow)

    def test_the_workspaces_strip_is_unchanged(self):
        """The pin. A workspace is not a chat and has no title, so this strip composes byte
        for byte what it composed before titles existed."""
        from charter.frame import switch as switch_mod
        names, here, note, close, counts, labels = slots._workspaces_strip("beta.1")
        self.assertEqual(labels, {})
        self.assertEqual(names, switch_mod.workspaces())
        state.record_title("beta.1", "fix the widget")
        row = slots.workspaces_bar("beta.1", 200)[0]
        self.assertNotIn("fix the widget", row)

    def test_the_tab_menus_label_names_the_id_then_the_title(self):
        """The operator right-clicked a tab that may be drawing nothing but the words they
        chose, so the heading has to say which chat that is."""
        state.record_title("beta.1", "fix it")
        self.assertIn("fix it", tabmenu.label("beta.1"))
        self.assertIn("beta.1", tabmenu.label("beta.1"))
        self.assertEqual(tabmenu.label("beta.2"), "chat beta.2")

    def test_the_chat_picker_names_the_id_then_the_title(self):
        """`palette.matches` filters on what is DRAWN and a picker row's id is charter's own
        counter, so a row that dropped the chat id would be a row nobody can reach by typing
        the name charter minted."""
        from charter.frame import choose
        state.record_title("beta.2", "fix it")
        roster = choose.roster(choose.CHAT, "beta.1")
        titles = [r.title for r in roster.rows]
        self.assertIn("beta.1", titles, "an untitled chat stopped drawing its bare id")
        self.assertIn("beta.2 · fix it", titles)
        self.assertEqual(list(roster.names), ["beta.1", "beta.2"],
                         "the picker switched to a title instead of an id")
        row = [r for r in roster.rows if "fix it" in r.title][0]
        self.assertEqual(roster.name_of(row), "beta.2")

    def test_only_a_chat_row_names_a_chats_title(self):
        """**A title belongs to the noun it is a title of**, and the collision is real rather
        than contrived: `persona.valid_name` admits `[a-z0-9._-]`, so a persona may be named
        exactly like a chat id. A picker that asked `state.title` for every noun would draw one
        chat's title on a persona's row — and would pay a file read per row for three nouns
        that have none."""
        from charter.frame import choose
        state.record_title("beta.1", "secret plan")
        self.make_persona("beta.1")
        for noun in (choose.WORKSPACE, choose.PERSONA, choose.CHANGE):
            with self.subTest(noun=noun):
                for row in choose.roster(noun, "beta.1").rows:
                    self.assertNotIn("secret plan", row.title)
        self.assertIn("beta.1", [r.title for r in choose.roster(choose.PERSONA, "beta.1").rows])


class ATitleAtThePlus(PersonaIso, unittest.TestCase):
    """The selector's own title row — the `+` and a workspace tab, and nowhere else."""

    def setUp(self):
        super().setUp()
        declare_profiles(self)
        approve_every_profile(self)
        wired_as_today(self)
        _plant("beta.1")

    def _rows(self, **kw):
        from charter.frame import selector
        have = selector.read(config.ROOT)
        return selector.rows(have, cwd=config.ROOT, **kw)

    def test_the_title_row_is_last_and_only_when_titling(self):
        """Last, because `palette.aim` opens the cursor on the first row that can run: a row
        at the top that starts no harness would spend the Enter of an operator who opened a
        chat to start one."""
        from charter.frame import selector
        self.assertNotIn(selector.TITLE_ID, [r.id for r in self._rows()])
        listed = self._rows(titling=True)
        self.assertEqual(listed[-1].id, selector.TITLE_ID)
        self.assertIn(selector.TITLE_NONE, listed[-1].title)

    def test_the_cursor_never_opens_on_it(self):
        """The half `palette.aim` decides. With any profile that can run, the cursor is on
        that — and a list where nothing can start still opens on this row rather than on
        nothing, which is `aim`'s own answer and is the right one: Enter always does
        something."""
        from charter.frame import palette, selector
        listed = self._rows(titling=True)
        self.assertNotEqual(listed[palette.aim(listed)].id, selector.TITLE_ID)

    def test_a_list_where_nothing_can_start_still_says_so(self):
        """The footer's summary counts only rows that can START a harness. The title row is
        never refused, so without excluding it `NOTHING_TO_PICK` would stop being reachable
        the day a chat could be named."""
        from charter.frame import overlay, selector
        refused = (overlay.Row(id="profile:x", title="x", note="nope", refused=True),
                   overlay.Row(id=selector.TITLE_ID, title="title: (none)"))
        self.assertIn(selector.NOTHING_TO_PICK, selector._footer(refused, None))

    def test_the_plus_and_a_workspace_tab_ask_for_it_and_an_ended_tab_does_not(self):
        """The call sites, which is where "where is a title offered" actually lives."""
        from charter.frame import launcher
        self.assertIn("--title-row", launcher.argv_select("claude", titling=True))
        self.assertNotIn("--title-row", launcher.argv_select("claude"))
        self.assertNotIn("--title-row", launcher.argv_select("claude", ended=True))
        self.assertNotIn("--title-row",
                         launcher.argv_select("claude", ended=True, fresh=True))

    def test_the_launch_hands_the_flag_to_the_selectors_argv(self):
        """The wire between the two call sites above and the pane: `cmd_launch` is where
        `titling` becomes `--title-row`, and a launch that read the field and dropped it would
        leave both of them green and the row absent.

        `no_frame` refuses this launch one line LATER than the argv is built, so nothing here
        reaches tmux, a terminal or a harness.
        """
        from charter.frame import launcher
        seen: list[dict] = []

        def _argv_select(start, **kw):
            seen.append(kw)
            return ["x"]

        def _launch(titling):
            with mock.patch.object(launcher, "argv_select", side_effect=_argv_select):
                commands_frame.cmd_launch(SimpleNamespace(
                    harness="frame", profile=None, select=True, start="", rest=[],
                    no_frame=True, workspace="beta", pick=False, attach=False,
                    size=None, ended=False, **({"titling": True} if titling else {})))

        _launch(True)
        _launch(False)
        self.assertEqual([kw["titling"] for kw in seen], [True, False])

    def test_the_title_is_recorded_in_the_pane_before_any_profile_starts(self):
        """**Written by the process standing in the chat's own proven pane**, before the
        `exec`: a title recorded after it would be recorded by nobody, because the launcher IS
        the harness by then.

        `selector.pick` is stood in for — this case is about what `_select_in_pane` does with
        a `Titled`, not about a tty — and the `execvpe` fake is where the recording is
        measured, which is the moment the pane stops being charter's.
        """
        from charter.frame import launcher, selector
        seen: list[str | None] = []
        answers = [selector.Titled("  fix   the widget "), selector.Choice("claude-work")]

        def _pick(**kw):
            seen.append(kw.get("titled"))
            return answers.pop(0)

        # Nobody is watching this run: `_refused_in_pane` asks whether stdin is a terminal
        # before it waits for Enter, and an ambient answer would make the case block on a
        # developer's machine and pass on CI (`tests/_ttyguard.py`).
        no_terminal()
        at_exec: list[str | None] = []

        def _execvpe(cmd, argv, env):
            at_exec.append(state.title("beta.1"))
            raise OSError(2, "no")

        with mock.patch.dict(os.environ, {"CHARTER_SESSION_ID": "beta.1"}, clear=False), \
                mock.patch.object(launcher, "framed_chat", return_value="beta.1"), \
                mock.patch.object(selector, "pick", side_effect=_pick), \
                mock.patch.object(launcher.pane, "claim", return_value=None), \
                mock.patch.object(launcher.pane, "release"), \
                mock.patch("os.execvpe", side_effect=_execvpe):
            launcher._select_in_pane(
                SimpleNamespace(start="", ended=False, fresh=False, title_row=True),
                "beta.1")

        self.assertEqual(at_exec, ["fix the widget"])
        self.assertEqual(seen, ["", "fix the widget"],
                         "the row did not draw back the title that landed")

    def test_a_chat_nobody_named_starts_exactly_as_it_did(self):
        """The floor: a selector with no title row behaves as it did before this existed."""
        from charter.frame import launcher, selector
        no_terminal()
        with mock.patch.dict(os.environ, {"CHARTER_SESSION_ID": "beta.1"}, clear=False), \
                mock.patch.object(launcher, "framed_chat", return_value="beta.1"), \
                mock.patch.object(selector, "pick",
                                  return_value=selector.Choice("claude-work")) as pick, \
                mock.patch.object(launcher.pane, "claim", return_value=None), \
                mock.patch.object(launcher.pane, "release"), \
                mock.patch("os.execvpe", side_effect=OSError(2, "no")):
            launcher._select_in_pane(
                SimpleNamespace(start="", ended=False, fresh=False), "beta.1")
        self.assertFalse(pick.call_args.kwargs["titling"])
        self.assertIsNone(state.title("beta.1"))
