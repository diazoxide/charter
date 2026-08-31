"""What makes the frame read as an application rather than as output.

Six elements, and only one of them is a colour. A named region, a consistent inset, a
row you can see you are on and a status you can read with every escape stripped are the
four that ship on by default, because none of them names a colour charter chose — and
`_CHROME_STYLE`'s rule (*"never a colour charter picked out of the 256 and imposed on a
theme it cannot see"*) is what says they may. The pane surface and the focus indicator
are the two that cannot be said relatively — tmux's `window-style` honours colour ONLY,
`reverse`/`dim`/`bold` are accepted and silently ignored — so they are behind one word.

`tests/test_frame_chrome.py` holds the primitive these are built on.
"""

from __future__ import annotations

import os
import re
import subprocess
import sys
import unittest
from pathlib import Path
from unittest import mock

from charter import instance, statusline, tui
from charter.frame import chrome, gather, slots

from tests._isolation import PersonaIso
from tests.test_frame_tmux_integration import _HAS_TMUX, _TmuxServerFixture


def _plain(out: str) -> list[str]:
    """*out* as the rows a terminal would show, split BEFORE the colour is stripped —
    `tui.strip_ansi` sanitises first, and `sanitize` turns a newline into a space."""
    return [tui.strip_ansi(ln) for ln in out.split("\n")]


class AHeadingHasWeightAndNotARow(PersonaIso, unittest.TestCase):
    """§5.3. `_sidebar_head`'s label becomes bold; nothing else about it moves.

    A heading ROW is the single change with the widest blast radius in the frame —
    fifteen-plus tests assert a panel's exact line count — and it buys nothing weight
    does not. So the assertions here are as much about what did NOT change: same text,
    same width, same number of rows.
    """

    def test_the_label_is_bold(self):
        head = slots._sidebar_head("personas", 6, 40)
        self.assertIn(f"{statusline._BOLD}personas", head)

    def test_the_count_stays_dim_and_the_label_is_not(self):
        """Two facts, not one: a heading whose count shouted as loudly as its name would
        read as two equal things rather than as a named region with a size."""
        head = slots._sidebar_head("personas", 6, 40)
        self.assertIn(f"{statusline._DIM} 6", head)
        self.assertNotIn(f"{statusline._DIM}personas", head)

    def test_the_plain_text_is_exactly_what_it_was(self):
        """`tui.strip_ansi` is what the existing heading tests compare through
        (`tests/test_frame_slots.py:814,1539`), so this is the assertion that says they
        stay green for the right reason rather than by accident."""
        self.assertEqual(tui.strip_ansi(slots._sidebar_head("personas", 6, 40)),
                         f"{statusline._HEAD_PAD}personas 6")

    def test_the_heading_costs_no_columns(self):
        """SGR is zero visible width, and the width arithmetic downstream is measured in
        columns — so weight is free in exactly the way a row is not."""
        self.assertEqual(tui.width(slots._sidebar_head("personas", 6, 40)),
                         tui.width(f"{statusline._HEAD_PAD}personas 6"))

    def test_a_narrow_pane_still_gets_one_line(self):
        for w in (1, 4, 8, 22):
            with self.subTest(width=w):
                head = slots._sidebar_head("personas", 6, w)
                self.assertNotIn("\n", head)
                self.assertLessEqual(tui.width(head), w)

    def test_the_repo_table_is_headed_the_same_way(self):
        """One helper, both sections — asserted rather than assumed, because "the label
        is bold" applied in one renderer and not the other is exactly the drift
        `_sidebar_head` was extracted to stop."""
        self.assertIn(f"{statusline._BOLD}repos", slots._sidebar_head("repos", 2, 40))


class TheInsetIsOneConstant(PersonaIso, unittest.TestCase):
    """§5.4. Every row's content starts in the same column, and that column is `INSET`.

    The value already existed — `statusline._HEAD_PAD` is two columns and its own comment
    records the two ways a header that computed its own indent shipped broken. What this
    adds is that it is ASKED FOR rather than spelled: a `"  "` typed at a call site is
    the same arithmetic done again, and the second copy is the one that moves.
    """

    def test_the_inset_is_the_status_lines_own_header_pad(self):
        """The agreement `slots.INSET` cannot import at module scope, pinned instead —
        the same trade `panel._DEFAULT_ROWS` makes with `slots._DEFAULT_ROWS`."""
        self.assertEqual(slots.INSET, tui.width(statusline._HEAD_PAD))

    def test_a_marker_is_fitted_to_the_inset_rather_than_padded_by_hand(self):
        self.assertEqual(tui.width(slots._inset("-")), slots.INSET)
        self.assertEqual(tui.width(slots._inset()), slots.INSET)

    def test_a_wide_marker_still_takes_exactly_the_inset(self):
        """`tui.pad` measures in cells. A `marker + " "` would give a two-cell glyph three
        columns and push its row one right of every other — the drift
        `_persona_chip_cells`' own comment says has broken this layout twice."""
        self.assertEqual(tui.width(slots._inset("⚡")), slots.INSET)

    def test_every_row_of_the_sidebar_starts_in_the_same_column(self):
        """The property the constant exists for, asserted end to end through the real
        renderer: a persona name, a todo title and both headings begin in one column."""
        self.make_persona("alice")
        gather.save("f-1", {"gathered_at": 0.0, "workspace": "w", "current_repo": None,
                            "repos": [], "worktrees": [],
                            "todos": [{"title": "ship the sidebar"}], "todo_count": 1})
        with mock.patch("os.get_terminal_size",
                        return_value=os.terminal_size((40, 24))), \
             mock.patch.object(sys.stdout, "fileno", return_value=1, create=True):
            rows = [ln for ln in _plain(slots.render("right", "f-1")) if ln.strip()]
        self.assertGreaterEqual(len(rows), 3, rows)
        for row in rows:
            with self.subTest(row=row):
                self.assertEqual(tui.width(row[:slots.INSET]), slots.INSET)
                self.assertNotEqual(row[slots.INSET], " ",
                                    f"content does not start at the inset: {row!r}")

    def test_the_more_row_is_inset_too(self):
        """It is a sentence about the list rather than a persona, and it still begins
        where the names do — which is what the two literal spaces used to say."""
        cells = [statusline.PersonaChip(f"p{i}", f"▫ p{i}", "") for i in range(9)]
        capped = slots._cap_personas(cells, 4)
        note = tui.strip_ansi(capped[-1].head)
        self.assertTrue(note.startswith(" " * slots.INSET), repr(note))
        self.assertIn("…(+", note)


class TheRowYouAreOnIsTheWholeRow(PersonaIso, unittest.TestCase):
    """§5.5. The active persona's row is inverted to the pane's last column.

    `▸ steward` is a glyph in a list; the whole row inverted is the row you are on. It is
    the one element that needed new painting machinery, because it has to REACH the last
    column — `tui` strips exactly the pad that would take it there — and the one with a
    defect that only appears when you build it (a reverse row cancels itself at the first
    full reset inside it, and charter's rows are full of them).
    """

    def _render(self, *, cols=22, rows=24, fid="f-1") -> str:
        with mock.patch("os.get_terminal_size",
                        return_value=os.terminal_size((cols, rows))), \
             mock.patch.object(sys.stdout, "fileno", return_value=1, create=True):
            return slots.render("right", fid)

    def _rows(self, out: str) -> list[str]:
        return out.split("\n")

    def test_the_active_personas_row_is_painted_to_the_pane_edge(self):
        self.make_persona("alice")
        self.make_persona("bob")
        with mock.patch("charter.persona.resolve_active", return_value="alice"):
            rows = self._rows(self._render())
        painted = [r for r in rows if "\x1b[7m" in r]
        self.assertEqual(len(painted), 1, f"exactly one row is the one you are on: {rows}")
        self.assertIn("alice", tui.strip_ansi(painted[0]))
        self.assertEqual(tui.width(painted[0]), 22)

    def test_no_other_row_is_painted(self):
        """The control for the assertion above: a `persona_section` that highlighted
        every row would satisfy "the active row is painted" perfectly."""
        self.make_persona("alice")
        self.make_persona("bob")
        with mock.patch("charter.persona.resolve_active", return_value="alice"):
            rows = self._rows(self._render())
        for row in rows:
            if "alice" in tui.strip_ansi(row):
                continue
            with self.subTest(row=row):
                self.assertNotIn("\x1b[7m", row)

    def test_a_plane_with_no_active_persona_paints_nothing(self):
        self.make_persona("alice")
        with mock.patch("charter.persona.resolve_active", return_value=None):
            self.assertNotIn("\x1b[7m", self._render())

    def test_the_highlight_is_not_cancelled_by_the_rows_own_resets(self):
        """The whole defect, end to end through the real renderer: the row carries
        `statusline._R` after the name and after each badge, and a naive wrapper would
        leave it highlighted for two words."""
        self.make_persona("alice")
        with mock.patch("charter.persona.resolve_active", return_value="alice"):
            painted = [r for r in self._rows(self._render()) if "\x1b[7m" in r][0]
        # Every full reset inside the row is followed immediately by a re-assertion.
        for m in re.finditer(r"\x1b\[([0-9;]*)m", painted[:-len(tui.RESET)]):
            if chrome.resets_everything(m.group(1)):
                with self.subTest(at=m.start()):
                    self.assertTrue(painted[m.end():].startswith("\x1b[7m"),
                                    f"reverse dies here: {painted!r}")

    def test_the_pane_is_still_exactly_its_two_parts(self):
        """`tests/test_builtin_components.py:272` compares `slots.render("right")` byte
        for byte against its two sections joined — the most brittle assertion in the
        suite for this change. It stays green because both sides reach the highlight
        through `persona_section`; this says so here too, so a future move to `_right`
        is red in the file that argued against it rather than only in that one."""
        self.make_persona("alice")
        with mock.patch("charter.persona.resolve_active", return_value="alice"), \
             mock.patch("os.get_terminal_size",
                        return_value=os.terminal_size((22, 24))), \
             mock.patch.object(sys.stdout, "fileno", return_value=1, create=True):
            whole = slots.render("right", "f-1")
            parts = slots.persona_section(22, 24, terse=False)
        self.assertEqual(whole.split("\n")[:len(parts)], parts)

    def test_the_row_count_is_unchanged(self):
        """No renderer gains or loses a row. The highlight is paint on a row that was
        already there."""
        cells = [statusline.PersonaChip(f"p{i}", f"▫ p{i}", "", active=i == 0)
                 for i in range(9)]
        with mock.patch("charter.statusline._persona_chip_cells", return_value=cells):
            rows = self._rows(self._render(rows=26))
        self.assertEqual(len(rows), 9 + 1)

    def test_a_chip_built_by_hand_is_not_the_active_row(self):
        """`PersonaChip.active` is defaulted, so the fixtures that build chips
        positionally (`tests/test_frame_slots.py`, `tests/test_frame_density.py`) keep
        describing a list nobody is standing in."""
        self.assertFalse(statusline.PersonaChip("p", "▫ p", "").active)
        self.assertFalse(statusline.PersonaChip(None, "  …(+7 more)", "", 7).active)

    def test_the_chips_themselves_know_which_one_is_active(self):
        """Read as data off the chip rather than found in its rendered head — the half
        that makes the highlight independent of `▸` and of magenta."""
        self.make_persona("alice")
        self.make_persona("bob")
        with mock.patch("charter.persona.resolve_active", return_value="bob"):
            cells = statusline._persona_chip_cells()
        self.assertEqual([c.name for c in cells if c.active], ["bob"])


class NoStatusIsCarriedByColourAlone(PersonaIso, unittest.TestCase):
    """§5.6, made a test rather than a habit.

    > A status conveyed by colour alone is a status some operators cannot read. Every
    > status in the frame carries a glyph or a word that says the same thing. Colour is
    > the second channel, never the only one.

    Asked the way `tests/test_frame_slots.py:140`'s `NoPanelDrawsItsOwnChrome` asks its
    own question — as a structural property over every slot in `slots.SLOTS`, with live
    controls proving the check can fail. The property here needs two renders rather than
    one: a status is only a status because it differs from the absence of one, so each
    slot is drawn healthy and drawn wanting attention, and the two must still differ once
    every SGR is gone.
    """

    def _render(self, slot: str, *, cols=200, rows=24, fid="f-1") -> str:
        with mock.patch("os.get_terminal_size",
                        return_value=os.terminal_size((cols, rows))), \
             mock.patch.object(sys.stdout, "fileno", return_value=1, create=True):
            return slots.render(slot, fid)

    def _seed(self, **overrides) -> None:
        data = {"gathered_at": 0.0, "workspace": "w", "current_repo": None,
                "repos": [], "worktrees": []}
        data.update(overrides)
        gather.save("f-1", data)

    def _repo(self, **overrides) -> dict:
        row = {"name": "demo", "branch": "main", "dirty": False, "tracked_dirty": False,
               "ahead": 0, "behind": 0, "ci": None, "change": None, "sigil": "",
               "current": False, "worktree_count": 0}
        row.update(overrides)
        return row

    # -- the probes: one pair per slot, driven through real state ------------------

    def _probe_top(self) -> tuple[str, str]:
        """A workspace pinned by the environment carries a `*`. The pin is the top bar's
        one status and it was already a glyph — this is what keeps it one."""
        quiet = self._render("top")
        with mock.patch.dict(os.environ, {"CHARTER_WORKSPACE": "default"}):
            loud = self._render("top")
        return quiet, loud

    def _probe_bottom(self) -> tuple[str, str]:
        """Open todos. `_bottom`'s count is unconditional, so the number is the word.

        `_todo_count` is the DATA (it counts files on disk), not the rendering — the
        same seam `tests/test_frame_density.py:379` drives this row through."""
        with mock.patch("charter.statusline._todo_count", return_value=0):
            quiet = self._render("bottom")
        with mock.patch("charter.statusline._todo_count", return_value=3):
            loud = self._render("bottom")
        return quiet, loud

    def _probe_repos(self) -> tuple[str, str]:
        """A dirty repo against a clean one — `_needs_attention`'s own first fact."""
        self._seed(repos=[self._repo()])
        quiet = self._render("repos")
        self._seed(repos=[self._repo(dirty=True, sigil="●")])
        return quiet, self._render("repos")

    def _probe_right(self) -> tuple[str, str]:
        """A persona whose charter is a draft: `_health_mark` speaks only when something
        is wrong, and `⚑` is what it says."""
        self.make_persona("alice")
        quiet = self._render("right", cols=40)
        with mock.patch("charter.persona.is_draft", return_value=True):
            loud = self._render("right", cols=40)
        return quiet, loud

    def test_every_slot_is_probed(self):
        """The coverage half. A slot added without a probe is red here rather than
        silently unexamined — which is the difference between a structural test and a
        list of four examples."""
        probed = {n[len("_probe_"):] for n in dir(self) if n.startswith("_probe_")}
        self.assertEqual(probed, set(slots.SLOTS))

    def test_a_status_survives_every_escape_being_stripped(self):
        for slot in sorted(slots.SLOTS):
            with self.subTest(slot=slot):
                quiet, loud = getattr(self, f"_probe_{slot}")()
                self.assertNotEqual(quiet, loud,
                                    f"{slot}'s probe changed nothing at all")
                self.assertNotEqual(
                    _plain(quiet), _plain(loud),
                    f"{slot} says this only in colour — an operator who cannot see "
                    f"the hue cannot read it: {loud!r}")

    def test_the_check_would_catch_a_status_that_was_only_a_colour(self):
        """The live control this class cannot do without: every assertion above is a
        negative, and a broken comparison passes just as happily as a correct frame."""
        ok = f"{statusline._GREEN}build{statusline._R}"
        bad = f"{statusline._RED}build{statusline._R}"
        self.assertNotEqual(ok, bad, "the two fixtures must differ before stripping")
        self.assertEqual(_plain(ok), _plain(bad),
                         "a colour-only difference must vanish when SGR is stripped")

    def test_the_check_passes_a_status_that_carries_a_glyph(self):
        """The other control — a check that called everything colour-only would satisfy
        the one above."""
        self.assertNotEqual(_plain(f"{statusline._GREEN}✎4{statusline._R}"),
                            _plain(f"{statusline._YELLOW}⚑{statusline._R}"))


class TheFrameNamesColoursAndNeverIndexes(PersonaIso, unittest.TestCase):
    """§3.2, enforced instead of remembered.

    Only three things may appear in charter's own chrome: `default`, the sixteen ANSI
    names, and the SGR attributes. Not a 256-cube index, not a 24-bit triple, ever — a
    name like `brightblack` is a slot in the operator's own palette while `colour236` is
    a fixed point in the cube that no theme moves.

    The sharp form of the argument is the inverse of the obvious one: **an absolute
    colour is unsafe precisely on the terminals that render it faithfully.** A 16-colour
    client gets charter's `colour236` downsampled to the operator's own black and looks
    fine; a truecolor client with a light theme gets the dark grey verbatim.

    Asked of the PARAMETERS rather than by grepping for `48;5`: the extended-colour form
    is "38 or 48, then a 5 or a 2", and `\\x1b[1;38;5;236m` is the same escape wearing a
    prefix.
    """

    #: SGR parameters that introduce a colour charter did not get from the operator.
    _EXTENDED = ("38", "48")

    def _absolute_colours(self, out: str) -> list[str]:
        found = []
        for m in re.finditer(r"\x1b\[([0-9;]*)m", out):
            params = m.group(1).split(";")
            for i, p in enumerate(params[:-1]):
                if p in self._EXTENDED and params[i + 1] in ("5", "2"):
                    found.append(m.group(0))
        return found

    def _render(self, slot: str) -> str:
        with mock.patch("os.get_terminal_size",
                        return_value=os.terminal_size((200, 24))), \
             mock.patch.object(sys.stdout, "fileno", return_value=1, create=True):
            return slots.render(slot, "f-1")

    def test_no_slot_emits_a_cube_index_or_a_24_bit_triple(self):
        self.make_persona("alice")
        gather.save("f-1", {"gathered_at": 0.0, "workspace": "w", "current_repo": None,
                            "repos": [{"name": "demo", "branch": "main", "dirty": True,
                                       "tracked_dirty": True, "ahead": 2, "behind": 0,
                                       "ci": "failed", "change": None, "sigil": "●",
                                       "current": True, "worktree_count": 0}],
                            "worktrees": [],
                            "todos": [{"title": "ship it"}], "todo_count": 1})
        for slot in sorted(slots.SLOTS):
            with self.subTest(slot=slot):
                self.assertEqual(self._absolute_colours(self._render(slot)), [])

    def test_the_check_recognises_both_forms(self):
        """The control. Every assertion above is a negative; these are the two escapes it
        is supposed to find, plus the prefixed spelling a grep for `\\x1b[48;5;` misses."""
        for esc in ("\x1b[48;5;236m", "\x1b[38;2;30;60;90m", "\x1b[1;38;5;236m"):
            with self.subTest(esc=esc):
                self.assertEqual(self._absolute_colours(esc), [esc])

    def test_the_check_passes_the_names_charter_does_use(self):
        for esc in (statusline._GREEN, statusline._YELLOW, statusline._RED,
                    statusline._DIM, statusline._BOLD, tui.RESET, "\x1b[7m"):
            with self.subTest(esc=esc):
                self.assertEqual(self._absolute_colours(esc), [])

    def test_the_styles_charter_hands_tmux_name_colours_too(self):
        """The same rule on the other side of the boundary: `window-style` takes a
        colour, and the words in `FRAME_CHROME` are palette slots, not cube indices.

        **`FRAME_PANE_BG` is inside this assertion and not an exception to it**, which is
        the decision the per-component key had to make rather than inherit.
        `colour0`-`colour255` was considered for it and refused: sixteen names come out of
        the operator's own palette and a cube index does not, and the argument does not
        get weaker for the key being per pane — it gets stronger, because a `[[frame
        .component]] bg` is committed and read on a machine whose theme the author of the
        file has never seen. Iterated from the table rather than spelled, so a colour added
        there is checked by this line without anybody remembering to come back."""
        for table in (instance.FRAME_CHROME, instance.FRAME_PANE_BG):
            for level, pairs in table.items():
                for name, value in pairs:
                    with self.subTest(level=level, option=name):
                        self.assertNotRegex(value, r"colour\d|#[0-9a-fA-F]{6}")

    def test_every_pane_background_is_one_of_the_sixteen_names_or_default(self):
        """The positive half, because the regex above is a negative and `bg=chartreuse`
        would pass it. Named against `FRAME_PANE_COLOURS` so the two cannot drift."""
        allowed = {"default", *instance.FRAME_PANE_COLOURS,
                   *(f"bright{c}" for c in instance.FRAME_PANE_COLOURS)}
        for word, pairs in instance.FRAME_PANE_BG.items():
            for option, value in pairs:
                with self.subTest(word=word, option=option):
                    self.assertTrue(value.startswith("bg="), value)
                    self.assertIn(value[len("bg="):], allowed)


class TheChromeValueIsAWordAndNeverAStyle(unittest.TestCase):
    """`[frame] chrome` — a closed enum at the config boundary.

    A tmux style value is FORMAT-EXPANDED at draw time. Measured on 3.7c: the option is
    stored verbatim and evaluated when the pane is drawn —

        $ tmux set -p -t %1 window-style 'bg=#{?#{==:1,1},colour196,colour46}'
        $ tmux show -p -t %1 -v window-style
        bg=#{?#{==:1,1},colour196,colour46}          <- stored as written
           wire: b'...\\x1b[48;5;196m\\x1b[2BPANEL'    <- tmux evaluated the conditional

    charter.toml is committed and arrives from someone else's machine, so a free style
    string there would be a committed value reaching a tmux evaluator — `[frame] hotkey`'s
    class exactly, where a newline once ran a second tmux command at launch with no
    keypress. Execution was not achieved through a style on 3.7c (`#(...)` is refused by
    the style parser outright) and that is not the same as it being safe.
    """

    def test_the_three_words_are_the_whole_enum(self):
        self.assertEqual(set(instance.FRAME_CHROME), {"off", "dark", "light"})

    def test_off_is_the_shipped_default(self):
        self.assertEqual(instance.FRAME_DEFAULTS["chrome"], "off")

    def test_off_sets_nothing_at_all(self):
        """Not "sets a default style" — sets nothing. `show -p` must answer `''`, which
        is a claim about the absence of a command rather than about its argument."""
        self.assertEqual(instance.chrome_options("off"), ())

    def test_each_word_names_two_pane_options_one_step_apart(self):
        for level in ("dark", "light"):
            with self.subTest(level=level):
                pairs = dict(instance.chrome_options(level))
                self.assertEqual(set(pairs), {"window-style", "window-active-style"})
                self.assertNotEqual(pairs["window-style"],
                                    pairs["window-active-style"],
                                    "the focused pane must be a shade off the others")

    def test_a_style_string_is_refused_and_leaves_the_frame_off(self):
        """The containment assertion, and it names WHICH refusal fired: the value is
        gone AND the resolved setting is the shipped default rather than some other
        word."""
        hostile = "bg=#{?#{==:1,1},colour196,colour46}"
        f = instance.frame_of({"frame": {"chrome": hostile}})
        self.assertEqual(f["chrome"], "off")
        self.assertIsNone(instance.chrome_level(hostile))
        self.assertEqual(instance.chrome_options(hostile), ())

    def test_a_word_charter_does_not_know_leaves_the_frame_off(self):
        for value in ("auto", "DARK", "dark ", "", "solarized"):
            with self.subTest(value=value):
                self.assertEqual(
                    instance.frame_of({"frame": {"chrome": value}})["chrome"], "off")

    def test_a_value_that_is_not_a_string_does_not_raise(self):
        """`tomllib` can hand this a list or a table, and `instance` is imported by every
        command including `charter --version`. `isinstance` first, for `density_level`'s
        own reason: `value in FRAME_CHROME` raises `TypeError` on an unhashable value."""
        for value in (["dark"], {"chrome": "dark"}, 7, True, None):
            with self.subTest(value=value):
                self.assertEqual(
                    instance.frame_of({"frame": {"chrome": value}})["chrome"], "off")

    def test_the_three_words_are_actually_read(self):
        for value in ("off", "dark", "light"):
            with self.subTest(value=value):
                self.assertEqual(
                    instance.frame_of({"frame": {"chrome": value}})["chrome"], value)

    def test_the_toml_spelling_is_the_bare_word(self):
        """One word, so the hyphen question (`history-limit`, not `history_limit`) does
        not arise — and the underscore form is not a second, undocumented alias."""
        self.assertEqual(instance.FRAME_FIELDS["chrome"][1], "chrome")

    def test_the_defaults_view_and_the_fields_table_agree(self):
        self.assertIn("chrome", instance.FRAME_DEFAULTS)
        self.assertEqual(set(instance.FRAME_DEFAULTS), set(instance.FRAME_FIELDS))


class ThisPlaneOptsIntoTheSurface(unittest.TestCase):
    """charter's own charter.toml asks for the fill; `FRAME_DEFAULTS` does not.

    That division is what `[frame]` already runs on: the plane's own committed file says
    what THIS plane looks like, and the shipped default says what a stranger's plane looks
    like before they have said anything at all. Both halves are asserted, because either
    one alone is the mistake — a default that repaints a stranger's terminal, or a plane
    whose operator asked for a look and did not get it.
    """

    def test_charters_own_plane_declares_a_surface(self):
        import tomllib
        root = Path(__file__).resolve().parent.parent
        cfg = tomllib.loads((root / "charter.toml").read_text())
        self.assertEqual(cfg["frame"]["chrome"], "dark")

    def test_the_shipped_default_is_still_off(self):
        self.assertEqual(instance.FRAME_DEFAULTS["chrome"], "off")


class TheSurfaceIsSetOnPanelPanesAndNoOther(unittest.TestCase):
    """`commands_frame._surface_argvs` — the argv, before any tmux runs it."""

    def test_off_issues_no_commands(self):
        from charter import commands_frame
        self.assertEqual(
            commands_frame._surface_argvs(socket="s", pane_id="%3", chrome="off"), [])

    def test_a_surface_is_pane_scoped(self):
        """`-p -t %N`, never `-g` and never `-w`: a window option would reach the harness
        pane, which is the one thing ADR 0018 says charter may not colour."""
        from charter import commands_frame
        argvs = commands_frame._surface_argvs(socket="s", pane_id="%3", chrome="dark")
        self.assertEqual(len(argvs), 2)
        for argv in argvs:
            with self.subTest(argv=argv):
                self.assertIn("-p", argv)
                self.assertEqual(argv[argv.index("-t") + 1], "%3")
                self.assertNotIn("-g", argv)
                self.assertNotIn("-w", argv)

    def test_no_color_refuses_the_surface_too(self):
        """The half that is easy to miss: the fill is tmux's paint, so gating only the
        panels' own SGR would leave an operator who asked for no colour looking at a
        coloured frame — charter having asked somebody else to paint it. Asserted with
        the empty value, which is the spelling a `== "1"` reading gets wrong."""
        from charter import commands_frame
        for value in ("", "0", "1"):
            with self.subTest(value=value), \
                 mock.patch.dict(os.environ, {"NO_COLOR": value}, clear=True):
                self.assertEqual(
                    commands_frame._surface_argvs(socket="s", pane_id="%3",
                                                  chrome="dark"), [])

    def test_without_no_color_the_surface_is_issued(self):
        """The control: `test_no_color_refuses_the_surface_too` is a negative, and a
        `_surface_argvs` that always answered `[]` would pass it."""
        from charter import commands_frame
        with mock.patch.dict(os.environ, {}, clear=True):
            self.assertEqual(
                len(commands_frame._surface_argvs(socket="s", pane_id="%3",
                                                  chrome="dark")), 2)

    def test_no_color_is_read_the_same_way_on_both_paths(self):
        """One reading of the variable, not two. `panel._write` and this are different
        processes on different paths asking the same question, and #547 is what two
        spellings of one question cost."""
        from charter import commands_frame
        self.assertIs(commands_frame.chrome_mod.no_colour, chrome.no_colour)

    def test_the_value_that_reaches_tmux_is_charters_own_constant(self):
        """The mutation this refuses: letting the config value through as a style. The
        argv carries a word out of `FRAME_CHROME`, and a value charter did not recognise
        produces no argv at all."""
        from charter import commands_frame
        hostile = "bg=#{?#{==:1,1},colour196,colour46}"
        self.assertEqual(
            commands_frame._surface_argvs(socket="s", pane_id="%3", chrome=hostile), [])
        argvs = commands_frame._surface_argvs(socket="s", pane_id="%3", chrome="dark")
        values = [a[-1] for a in argvs]
        # Against the ASSEMBLER: #737 pairs a foreground with charter's own two surfaces,
        # and `surface_options` is the one place the background and that foreground meet.
        self.assertEqual(set(values),
                         {v for _n, v in instance.surface_options(
                             None, "dark", instance.SHIPPED_LOOK)})
        for v in values:
            self.assertNotIn("#", v)


@unittest.skipUnless(_HAS_TMUX, "tmux is not installed on this machine")
class TheHarnessPaneIsNeverStyled(_TmuxServerFixture, PersonaIso):
    """ADR 0018's boundary, read back out of tmux rather than intended in charter.

    A real server, a real split, charter's real `_surface_argvs` — and then `show -p` on
    both panes. The harness pane must answer `''`: not "a style charter considers
    harmless", not "the same style", nothing at all.
    """

    SOCKET_NAME = f"charter-chrome-surface-{os.getpid()}"

    def _panes(self) -> tuple[str, str]:
        r = self._srv("new-session", "-d", "-s", "h", "-x", "80", "-y", "24",
                      "-P", "-F", "#{pane_id}", "--", "sleep", "600")
        self.assertEqual(r.returncode, 0, r.stderr)
        harness = r.stdout.strip()
        p = self._srv("split-window", "-t", harness, "-P", "-F", "#{pane_id}",
                      "--", "sleep", "600")
        self.assertEqual(p.returncode, 0, p.stderr)
        return harness, p.stdout.strip()

    def _style(self, pane: str, option: str = "window-style") -> str:
        return self._srv("show", "-p", "-t", pane, "-v", option).stdout.strip()

    def _apply(self, pane: str, chrome: str) -> None:
        from charter import commands_frame
        for argv in commands_frame._surface_argvs(
                socket=self.SOCKET_NAME, pane_id=pane, chrome=chrome):
            r = subprocess.run(argv, capture_output=True, text=True, timeout=10)
            self.assertEqual(r.returncode, 0, f"{argv}: {r.stderr}")

    def test_a_surfaced_panel_leaves_the_harness_pane_bare(self):
        harness, panel = self._panes()
        self._apply(panel, "dark")
        self.assertEqual(self._style(panel), dict(instance.surface_options(
            None, "dark", instance.SHIPPED_LOOK))["window-style"])
        self.assertEqual(self._style(panel, "window-active-style"),
                         dict(instance.surface_options(
                             None, "dark", instance.SHIPPED_LOOK))["window-active-style"])
        self.assertEqual(self._style(harness), "",
                         "charter styled the pane the operator's harness runs in")
        self.assertEqual(self._style(harness, "window-active-style"), "")

    def test_with_the_surface_off_every_pane_reads_back_empty(self):
        """The other direction, and the default's own exit criterion: `off` is not a
        style charter chose to be invisible, it is no option at all."""
        harness, panel = self._panes()
        self._apply(panel, "off")
        for pane in (harness, panel):
            with self.subTest(pane=pane):
                self.assertEqual(self._style(pane), "")
                self.assertEqual(self._style(pane, "window-active-style"), "")

    def test_tmux_accepts_the_style_charter_actually_sends(self):
        """The claim that would otherwise be untested: `bg=brightwhite` is a style this
        tmux parses. A refused `set-option` is silent in production (reported, not
        fatal), so a typo would ship as a frame that simply never coloured."""
        _harness, panel = self._panes()
        for level in ("dark", "light"):
            with self.subTest(level=level):
                self._apply(panel, level)
                # `fg=white,bg=black` and `fg=black,bg=white` — this test is the one that
                # says a real tmux parses what charter sends, so it has to read the whole
                # style rather than the background half of it (#737).
                self.assertEqual(dict(instance.surface_options(
                    None, level, instance.SHIPPED_LOOK))["window-style"],
                    self._style(panel))

    def test_the_surface_belongs_to_the_pane_and_not_to_the_process_in_it(self):
        """`commands_frame`'s `pane-died` hook respawns a dead panel INTO THE SAME PANE,
        and a renderer-side fill would have to be re-established by whatever came back.
        These are pane options, so the surface is a property of the rectangle. Measured
        rather than reasoned, because `docs/frame.md` says it to an operator."""
        harness, panel = self._panes()
        self._srv("set", "-g", "remain-on-exit", "on")
        self._apply(panel, "dark")
        r = self._srv("respawn-pane", "-k", "-t", panel, "--", "sleep", "600")
        self.assertEqual(r.returncode, 0, r.stderr)
        self.assertEqual(self._style(panel), dict(instance.surface_options(
            None, "dark", instance.SHIPPED_LOOK))["window-style"])
        self.assertEqual(self._style(harness), "")

    def test_the_surface_survives_the_window_being_resized(self):
        """The other half of the same claim, and the one #553 makes worth asserting: a
        fill charter painted would have to be redrawn at the new width and could be one
        cell wrong. An option has no width to get wrong."""
        _harness, panel = self._panes()
        self._apply(panel, "dark")
        r = self._srv("resize-window", "-t", "h", "-x", "120", "-y", "40")
        self.assertEqual(r.returncode, 0, r.stderr)
        self.assertEqual(self._style(panel), dict(instance.surface_options(
            None, "dark", instance.SHIPPED_LOOK))["window-style"])

    def test_a_format_string_would_have_been_stored_verbatim(self):
        """The measurement behind the enum, run here rather than quoted: tmux keeps a
        style value as written and expands it at draw time, so the boundary that matters
        is the one in `instance.frame_of` — not anything tmux does on the way in."""
        _harness, panel = self._panes()
        r = self._srv("set-option", "-p", "-t", panel, "window-style",
                      "bg=#{?#{==:1,1},colour196,colour46}")
        self.assertEqual(r.returncode, 0, r.stderr)
        self.assertEqual(self._style(panel), "bg=#{?#{==:1,1},colour196,colour46}")


if __name__ == "__main__":
    unittest.main()
