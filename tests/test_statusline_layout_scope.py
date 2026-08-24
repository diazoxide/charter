"""The status line is grouped by **scope**: every item sits with the thing it describes.

Before this, one header line carried four different scopes at once — the active
workspace, a count of *all* workspaces, a count of *all* vaults, and two session
gauges — and the session's own news (denials, memories, dispatches) rendered *inside
the repo column*, where it read as repo news. Items that had nothing to do with each
other sat side by side, and items that belonged together were rows apart.

Four zones now, each answering one question:

    ⬢ umbrella-improvements · ws 9                      WHERE I am
    ◫ repos 2/38            │ ◈ personas 13 · vaults 6  what each column holds
    ├─ …repo rows…          │ …persona chips…           WHAT I'm on × WHO I am
    ctx 22% · cache 100% · ⛊1 denied         ⬢ charter    THIS session · the tool

The rule to keep: a count lives next to what it counts.
"""

from __future__ import annotations

import re
import unittest

from charter import config, persona, statusline
from tests._isolation import PersonaIso


def _plain(s: str) -> str:
    return re.sub(r"\x1b\[[0-9;]*m", "", s)


def _raw(payload=None, width=200):
    """Rendered lines exactly as emitted, frame included."""
    import os
    old = os.environ.get("COLUMNS")
    os.environ["COLUMNS"] = str(width)
    try:
        return [_plain(ln) for ln in statusline.render(payload or {}).split("\n")]
    finally:
        if old is None:
            os.environ.pop("COLUMNS", None)
        else:
            os.environ["COLUMNS"] = old


def _lines(payload=None, width=200):
    """Content lines with the frame stripped — these tests are about the zones inside
    the box, not the box."""
    out = []
    for ln in _raw(payload, width):
        if not ln.strip() or set(ln.strip()) <= set("┌─┐└┘├┤"):
            continue                      # top/bottom border, or a zone divider
        if ln.startswith("│ ") and ln.rstrip().endswith("│"):
            ln = ln[2:].rstrip()[:-1].rstrip()
        out.append(ln)
    return out


_USAGE = {"context_window": {"used_percentage": 22,
                             "current_usage": {"cache_read_input_tokens": 100,
                                               "cache_creation_input_tokens": 0}}}


class Zones(PersonaIso):
    def setUp(self) -> None:
        super().setUp()
        for n in ("alpha", "beta"):
            self.make_persona(n, role=n.title(), vault=n, **{"delegate-when": f"{n} work"})
        # `vaults N` is shown only when there is at least one — `vaults 0` is noise.
        config.VAULTS_REGISTRY.parent.mkdir(parents=True, exist_ok=True)
        config.VAULTS_REGISTRY.write_text(
            '{"vaults": {"alpha": {"provider": "plain-file", "config": {"file": "/x"}}}}')

    def test_the_top_line_is_identity_only(self):
        """Where am I, and how many other workspaces exist. Nothing else."""
        top = _lines(_USAGE)[0]
        self.assertNotIn("repos", top)
        self.assertNotIn("vaults", top)
        self.assertNotIn("ctx", top)
        self.assertNotIn("⚡", top)

    def test_repos_count_heads_the_repo_column(self):
        head = next(ln for ln in _lines() if "repos" in ln)
        self.assertTrue(head.lstrip().lstrip(statusline._MARK_HEAD).lstrip()
                        .startswith("repos"), head)

    def test_the_left_column_introduces_no_unproven_glyph(self):
        """The left column is width-critical: it is padded to `_LEFT_W` using
        `tui.width`, so a font that draws any character wider than the Unicode tables
        claim makes that row overhang and its right-hand cell start late.

        A `◫` on the repo header did exactly that — the personas header rendered one
        space right of every chip below it. The rule that prevents a repeat: this column
        may only use glyphs that ALREADY appear in it (proven safe by the fact that its
        rows line up with each other). Decoration belongs in the right column, past the
        alignment point.
        """
        allowed = set("├└│╰─↳⑂✓✗●·⚡⛊✎◌⚠▪▸▫")  # tree, markers, and status glyphs
        for ln in _lines(_USAGE):
            sep = ln.find("│", 40)      # the column separator, past the tree glyphs
            if sep < 0:
                continue                # full-width row (identity, alerts, strip):
                                        # nothing to its right, so width can't shear it
            for ch in ln[:sep]:
                if ord(ch) < 128 or ch.isspace():
                    continue
                self.assertIn(ch, allowed,
                              f"unproven glyph {ch!r} (U+{ord(ch):04X}) in the "
                              f"width-critical left column: {ln!r}")

    def test_personas_and_vaults_count_heads_the_persona_column(self):
        head = next(ln for ln in _lines() if "personas" in ln)
        self.assertIn("personas 2", head)
        self.assertIn("vaults", head)

    def test_the_two_column_headers_share_one_row(self):
        ls = _lines()
        self.assertEqual([i for i, ln in enumerate(ls) if "repos" in ln],
                         [i for i, ln in enumerate(ls) if "personas" in ln])

    def test_session_gauges_live_on_the_last_content_line(self):
        ls = [ln for ln in _lines(_USAGE) if ln.strip()]
        self.assertIn("ctx", ls[-1])
        # `cache`, not `⚡`: the bolt belongs to a running dispatch now, and the two
        # gauges read as a pair because both carry a word.
        self.assertIn("cache", ls[-1])
        self.assertNotIn("⚡", ls[-1])

    def test_the_brand_sits_at_the_end_of_the_session_strip(self):
        ls = [ln for ln in _lines(_USAGE) if ln.strip()]
        self.assertIn("charter", ls[-1])
        self.assertIn("ctx", ls[-1], "brand must share the strip, not float on a chip row")

    def test_no_session_news_inside_the_repo_column(self):
        """`⛊ N denied` used to render under the repo tree, as if it were repo news."""
        ls = _lines(_USAGE)
        head = next(i for i, ln in enumerate(ls) if "repos" in ln)
        last = max(i for i, ln in enumerate(ls) if ln.strip())
        for ln in ls[head:last]:
            left = ln.split("│", 1)[0] if "│" in ln[40:] else ln
            for token in ("denied", "recorded", "dispatched", "ctx", "cache", "⚡"):
                self.assertNotIn(token, left, f"session news leaked into the repo column: {ln}")


class DegradesCleanly(PersonaIso):
    def setUp(self) -> None:
        super().setUp()
        self.make_persona("alpha", role="A", vault="a", **{"delegate-when": "x"})

    def test_a_narrow_pane_still_renders_every_zone(self):
        for w in (24, 40, 80, 100, 131, 200):
            with self.subTest(width=w):
                out = _lines(_USAGE, width=w)
                self.assertTrue(any(ln.strip() for ln in out))
                for ln in out:
                    self.assertLessEqual(statusline.tui.width(ln), w)

    def test_the_strip_is_absent_when_there_is_nothing_to_say(self):
        """No usage yet (fresh session / just compacted) and no news — the strip must
        not render an empty row just to hold the brand."""
        ls = [ln for ln in _lines({}) if ln.strip()]
        self.assertFalse(any(ln.startswith("ctx") for ln in ls))

    def test_it_never_raises_on_a_broken_payload(self):
        for bad in ({"context_window": None}, {"context_window": {"used_percentage": "x"}},
                    {"session_id": 12}):
            with self.subTest(payload=bad):
                statusline.render(bad)


class BrandFits(PersonaIso):
    """The brand must be present-and-correct or absent — never truncated.

    A real session rendered `⬢ charter 0.10…`: `_with_brand` fits-or-drops and never
    truncates, so the crop came from outside — the pane gave one column less than
    `COLUMNS` promised. The fit check now keeps a margin, so an off-by-one in the
    reserve (or in a terminal's idea of how wide `⬢` is) drops the brand instead of
    shearing it.
    """

    def test_the_brand_is_never_partially_rendered(self):
        for w in range(24, 220):
            with self.subTest(width=w):
                out = _plain(statusline._with_brand("x" * 10, w))
                if "charter" in out:
                    self.assertNotIn("…", out.split("charter")[1])

    def test_it_keeps_a_margin_beyond_the_declared_width(self):
        """Exactly-fits must NOT render: that is the case a one-column shortfall eats."""
        brand_w = statusline.tui.width(_plain(statusline._brand()))
        body = "x" * 10
        exact = 10 + statusline._BRAND_GAP + brand_w
        self.assertNotIn("charter", _plain(statusline._with_brand(body, exact)))
        self.assertIn("charter", _plain(statusline._with_brand(body, exact + statusline._BRAND_MARGIN)))

    def test_it_still_never_exceeds_the_width(self):
        for w in (24, 40, 80, 120, 200):
            for used in (0, 5, 20):
                with self.subTest(width=w, used=used):
                    out = statusline._with_brand("x" * used, w)
                    for line in out.split("\n"):
                        self.assertLessEqual(statusline.tui.width(_plain(line)), w)


class Framed(PersonaIso):
    """The box, and the alignment guarantee it makes visible.

    Everything up to a row's last alignment point is ASCII — the frame, the tree, the
    column divider, and the chip bullets — because `tui.width` only knows what the
    Unicode tables claim and a terminal that disagrees shifts that row alone. Decoration
    is fine after nothing on the row still has to line up.
    """

    def setUp(self) -> None:
        super().setUp()
        for n in ("alpha", "beta", "gamma"):
            self.make_persona(n, role=n.title(), vault=n, **{"delegate-when": f"{n} work"})

    def test_it_has_a_top_and_bottom_rule(self):
        rows = [ln for ln in _raw(_USAGE) if ln.strip()]
        self.assertTrue(set(rows[0]) <= set("┌─┐"), rows[0])
        self.assertTrue(set(rows[-1]) <= set("└─┘"), rows[-1])

    def test_every_content_row_is_bounded_on_both_sides(self):
        rows = [ln for ln in _raw(_USAGE) if ln.strip()][1:-1]
        for ln in rows:
            if set(ln.strip()) <= set("├─┤"):
                # A zone divider is bounded too — it joins the side borders with tees
                # instead of carrying content between them.
                self.assertTrue(ln.startswith("├") and ln.endswith("┤"), ln)
                continue
            self.assertTrue(ln.startswith("│"), ln)
            self.assertTrue(ln.endswith("│"), ln)

    def test_every_row_is_exactly_the_same_width(self):
        """The point of the right edge: a row that renders wider than counted pushes
        its own `|` out, so drift is visible instead of mysterious."""
        widths = {statusline.tui.width(ln) for ln in _raw(_USAGE) if ln.strip()}
        self.assertEqual(len(widths), 1, f"ragged frame: {sorted(widths)}")

    def test_the_frame_never_exceeds_the_pane(self):
        for w in (40, 80, 131, 160, 200, 240):
            with self.subTest(width=w):
                for ln in _raw(_USAGE, width=w):
                    self.assertLessEqual(statusline.tui.width(ln), w)

    def test_a_pane_too_narrow_to_frame_still_renders(self):
        out = _raw(_USAGE, width=24)
        self.assertTrue(any(ln.strip() for ln in out))

    def test_the_divider_sits_in_the_same_column_on_every_row(self):
        """The divider and frame may be box-drawing precisely because every row carries
        exactly one of each, so a terminal that draws them wide shifts every row
        identically and the columns stay true. (The left column's tree pipe is also
        `│`, which is why this checks position rather than counting occurrences.)"""
        cols = {ln.find("│", 40) for ln in _lines(_USAGE) if ln.find("│", 40) > 0}
        self.assertEqual(len(cols), 1, f"divider wanders between columns: {sorted(cols)}")

    def test_headers_indent_with_spaces_and_never_a_glyph(self):
        """The one rule that survived every redesign, because breaking it caused the
        original defect twice: a header's text position may not depend on how a font
        draws anything. A `◈` doing the indenting rendered wide and pushed the title a
        column right; removing it took the indent away and left the title two columns
        left. Spaces only, before the first letter."""
        for ln in _lines(_USAGE):
            sep = ln.find("│", 40)
            if sep < 0 or "personas" not in ln:
                continue
            for cell in (ln[:sep], ln[sep + 1:]):
                lead = cell[:len(cell) - len(cell.lstrip())]
                self.assertTrue(all(ch == " " for ch in lead),
                                f"header indents with something other than spaces: {cell!r}")

    def test_every_persona_name_starts_in_the_same_column_as_the_header(self):
        """The defect that survived three fixes: `personas` sat at column 98 while every
        chip name sat at 100, because a `◈` had been doing the indenting and removing it
        took the indent with it. Headers pad with SPACES now, so this cannot recur."""
        rows = [ln for ln in _lines(_USAGE) if ln.find("│", 40) > 0]
        starts = set()
        for ln in rows:
            right = ln[ln.find("│", 40) + 1:]
            m = re.search(r"[A-Za-z]", right)
            self.assertIsNotNone(m, right)
            starts.add(m.start())
        self.assertEqual(len(starts), 1,
                         f"right-column text starts at differing columns: {sorted(starts)}")


    def test_every_marker_is_east_asian_neutral(self):
        """The guarantee behind the whole layout, and the thing that actually broke.

        A column header's marker and a chip's bullet decide where the text after them
        begins. If they can disagree about their own width, the header's title drifts
        from the names below it — which is what `◈` (header) against `◆`/`○` (chips)
        did: all three are East-Asian **Ambiguous**, and a font drew one of them two
        cells wide.

        Neutral is the property that removes the argument: the tables give it exactly
        one cell everywhere. Assert the property, not the specific characters, so a
        future redesign can pick different glyphs and still be safe.
        """
        import unicodedata as ud
        for name, ch in (("header", statusline._MARK_HEAD),
                         ("active", statusline._MARK_ACTIVE),
                         ("idle", statusline._MARK_IDLE)):
            with self.subTest(marker=name):
                self.assertIn(ud.east_asian_width(ch), ("N", "Na"),
                              f"{name} marker {ch!r} (U+{ord(ch):04X}) is East-Asian "
                              f"{ud.east_asian_width(ch)} — a font may draw it two cells "
                              f"and shift the text after it")

    def test_the_header_marker_is_the_same_width_as_a_chip_bullet(self):
        """Header and chips must occupy the same prefix, or their text disagrees."""
        w = statusline.tui.width
        self.assertEqual(w(statusline._MARK_HEAD), w(statusline._MARK_ACTIVE))
        self.assertEqual(w(statusline._MARK_HEAD), w(statusline._MARK_IDLE))

    def test_the_two_column_headers_share_one_row(self):
        ls = _lines()
        self.assertEqual([i for i, ln in enumerate(ls) if "repos" in ln],
                         [i for i, ln in enumerate(ls) if "personas" in ln])

    def test_session_gauges_live_on_the_last_content_line(self):
        ls = [ln for ln in _lines(_USAGE) if ln.strip()]
        self.assertIn("ctx", ls[-1])
        # `cache`, not `⚡`: the bolt belongs to a running dispatch now, and the two
        # gauges read as a pair because both carry a word.
        self.assertIn("cache", ls[-1])
        self.assertNotIn("⚡", ls[-1])

    def test_the_brand_sits_at_the_end_of_the_session_strip(self):
        ls = [ln for ln in _lines(_USAGE) if ln.strip()]
        self.assertIn("charter", ls[-1])
        self.assertIn("ctx", ls[-1], "brand must share the strip, not float on a chip row")

    def test_no_session_news_inside_the_repo_column(self):
        """`⛊ N denied` used to render under the repo tree, as if it were repo news."""
        ls = _lines(_USAGE)
        head = next(i for i, ln in enumerate(ls) if "repos" in ln)
        last = max(i for i, ln in enumerate(ls) if ln.strip())
        for ln in ls[head:last]:
            left = ln.split("│", 1)[0] if "│" in ln[40:] else ln
            for token in ("denied", "recorded", "dispatched", "ctx", "cache", "⚡"):
                self.assertNotIn(token, left, f"session news leaked into the repo column: {ln}")


class OverflowKeepsTheReposWithSomethingToSay(unittest.TestCase):
    """The cap was a bare positional slice, so with the list in directory order the rows
    went to whatever sorted first. Observed with 18 clones: thirteen rows of clean
    `aaa-svc-NN` on main, while `…(+5 more)` swallowed every dirty repo, the only off-main
    branch and both failing pipelines — the table was showing the repos with nothing to
    say. The repo you were standing in could be hidden too, so the bold you-are-here
    marker attached to nothing on screen."""

    def setUp(self):
        from pathlib import Path
        self.dirs = [Path(f"/w/aaa-{i:02d}") for i in range(1, 16)] + [
            Path("/w/zzz-dirty"), Path("/w/zzz-ahead"), Path("/w/mmm-ci")]
        self.states = {d: {"dirty": False, "ahead": 0, "behind": 0} for d in self.dirs}
        self.gl = {d: {} for d in self.dirs}
        self.states[Path("/w/zzz-dirty")] = {"dirty": True, "ahead": 0, "behind": 0}
        self.states[Path("/w/zzz-ahead")] = {"dirty": False, "ahead": 2, "behind": 0}
        self.gl[Path("/w/mmm-ci")] = {"ci": "failed", "change": 412, "sigil": "#"}

    def _pick(self, cur=None):
        budget = statusline._MAX_REPO_LINES - 1
        return [d.name for d in
                statusline._pick_rows(self.dirs, budget, cur, self.states, self.gl)]

    def test_a_dirty_repo_survives_the_cap(self):
        self.assertIn("zzz-dirty", self._pick())

    def test_an_unpushed_repo_survives_the_cap(self):
        self.assertIn("zzz-ahead", self._pick())

    def test_a_failing_pipeline_survives_the_cap(self):
        self.assertIn("mmm-ci", self._pick())

    def test_the_repo_you_are_standing_in_always_survives(self):
        """Otherwise the bold you-are-here marker attaches to nothing on screen."""
        self.assertIn("aaa-09", self._pick(cur="aaa-09"))

    def test_display_order_is_not_the_ranking(self):
        """Ranking decides WHICH rows; it must not decide where they sit. A row that
        moves as its state changes stops being a place you can look — you would re-read
        the table every turn to find the repo you were just in."""
        names = self._pick()
        order = [d.name for d in self.dirs]
        self.assertEqual(names, sorted(names, key=order.index))

    def test_nothing_is_dropped_when_it_fits(self):
        from pathlib import Path
        few = [Path("/w/a"), Path("/w/b")]
        st = {d: {} for d in few}
        self.assertEqual(statusline._pick_rows(few, 13, None, st, {}),
                         few)

    def test_a_repo_keeps_its_colour_when_a_neighbour_enters_the_cap(self):
        from pathlib import Path
        """Palette was indexed by position among the SHOWN rows, so a repo changed colour
        whenever a neighbour entered or left — which, with ranked selection, happens the
        moment anything goes dirty."""
        rows_a = statusline._repo_rows(self.dirs, "ws", None, self.states,
                                       {d: "main" for d in self.dirs}, self.gl)
        self.states[Path("/w/aaa-14")] = {"dirty": True, "ahead": 0, "behind": 0}
        rows_b = statusline._repo_rows(self.dirs, "ws", None, self.states,
                                       {d: "main" for d in self.dirs}, self.gl)

        def colour_of(rows, name):
            for r in rows:
                line = r.render(statusline._LEFT_W)[0]
                if re.search(rf"\b{name}\b", _plain(line)):
                    m = re.search(r"\x1b\[(\d+)m" + r"[^\x1b]*" + name, line)
                    return m.group(1) if m else None
            return None

        self.assertEqual(colour_of(rows_a, "aaa-02"), colour_of(rows_b, "aaa-02"))


if __name__ == "__main__":
    unittest.main()
