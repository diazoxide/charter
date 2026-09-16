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

import unittest

from charter import contain
from charter.frame import chats, leave, rename, state

from tests._isolation import PersonaIso


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
        for typed in ("a\nb", "a b", "a\x85b", "a\tb", "a\xa0b"):
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
        for typed in ("a‍b", "a​b", "ab", "a\udcffb".encode(
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
        self.assertTrue(state.record_title("beta.1", "ㅤㅤ"))
        self.assertEqual(chats.title_of("beta.1"),
                         contain.readable("ㅤㅤ", state.TITLE_MAX))
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
