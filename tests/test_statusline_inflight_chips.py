"""In-flight dispatches on the persona chips.

`inflight` has always known which personas are running right now, and the status line
spent that knowledge on one aggregate at the bottom of the screen — `⚡in flight 2 ·
devops, devops` — where the reader had to match a name against a roster ten rows above
to learn anything. The count now lives next to the thing it counts.

Two rules, both borrowed from surfaces already on this line:

* **count only when >1** — `⚡1` is the same non-fact as `todo 0` or `✎0`, which render
  as nothing. Presence is the signal; the number is only interesting once it is a
  number.
* **age always** — the question a human asks a running dispatch is *"has this been
  stuck for four minutes?"*, and only the age answers it. Coarse, in
  `pieces._presence_age`'s vocabulary, so it ages in the same units as the `silent 12m`
  a couple of rows away — and because at a 10s refresh, seconds would be a lie.

The aggregate stays on the session strip (as a bare count) precisely because this
column is croppable: it caps at `_MAX_PERSONA_LINES` and disappears entirely on a
narrow pane.
"""

from __future__ import annotations

import json
import os
import re
import time
import unittest
from pathlib import Path

from charter import config, inflight, statusline
from tests._isolation import PersonaIso


def _plain(s: str) -> str:
    return re.sub(r"\x1b\[[0-9;]*m", "", s)


def _start_aged(agent: str, seconds: float) -> str:
    """Start a dispatch and backdate that ONE record by *seconds*.

    Both the JSON ``ts`` and the file's mtime move: `inflight` prunes on mtime, so a
    record aged only in its payload would be a fixture that could never occur.
    """
    token = inflight.start(agent)
    when = time.time() - seconds
    p = config.STATE_DIR / "dispatch-inflight" / f"{token}.json"
    rec = json.loads(p.read_text())
    rec["ts"] = when
    p.write_text(json.dumps(rec))
    os.utime(p, (when, when))
    return token


class Chips(PersonaIso):
    def setUp(self) -> None:
        super().setUp()
        for n in ("devops", "release"):
            self.make_persona(n, role=n.title(), **{"delegate-when": f"{n} work"})

    def _chip(self, name: str) -> str:
        for c in statusline._persona_chips():
            if re.search(rf"\b{re.escape(name)}\b", _plain(c)):
                return _plain(c)
        raise AssertionError(f"no chip for {name}")

    def test_a_persona_not_in_flight_carries_no_bolt(self):
        self.assertNotIn("⚡", self._chip("devops"))

    def test_one_dispatch_shows_a_bolt_and_an_age_but_no_count(self):
        _start_aged("devops", 4 * 60)
        chip = self._chip("devops")
        self.assertIn("⚡", chip)
        self.assertIn("4m", chip)
        self.assertNotIn("⚡1", chip)

    def test_more_than_one_dispatch_shows_the_count(self):
        _start_aged("devops", 4 * 60)
        _start_aged("devops", 60)
        self.assertIn("⚡2", self._chip("devops"))

    def test_the_age_is_the_oldest_dispatch(self):
        """The newest is the least interesting: what a reader wants to know is how long
        the longest-running one has been out.

        Aged in minutes rather than hours because nothing older than
        `inflight.TTL_SECONDS` can be observed at all — `live_records` prunes it as a
        killed process. That the *most* alarming dispatch is the one that vanishes is
        real and deliberately not fixed here (diazoxide/charter#308): it changes what
        `inflight` means, not what this line draws.
        """
        _start_aged("devops", 60)
        _start_aged("devops", 25 * 60)
        _start_aged("devops", 6 * 60)
        chip = self._chip("devops")
        self.assertIn("⚡3", chip)
        self.assertIn("25m", chip)
        self.assertNotIn("6m", chip)

    def test_only_the_dispatched_persona_is_marked(self):
        _start_aged("devops", 60)
        self.assertIn("⚡", self._chip("devops"))
        self.assertNotIn("⚡", self._chip("release"))

    def test_a_finished_dispatch_takes_its_bolt_with_it(self):
        _start_aged("devops", 60)
        inflight.finish("devops")
        self.assertNotIn("⚡", self._chip("devops"))

    def test_a_dispatch_under_a_minute_says_now(self):
        """`0m` is technically correct and reads as broken — the same call
        `pieces._presence_age` already made, reused rather than re-litigated."""
        inflight.start("devops")
        self.assertIn("now", self._chip("devops"))

    def test_a_broken_inflight_store_never_breaks_the_chips(self):
        orig = inflight.live_records
        inflight.live_records = lambda *a, **k: 1 / 0
        try:
            self.assertTrue(statusline._persona_chips())
        finally:
            inflight.live_records = orig

    def test_the_marker_stays_two_columns_wide(self):
        """The bolt trails the name, so it must not move where a name starts — the
        chips line up with the column header on exactly that promise."""
        _start_aged("devops", 4 * 60)
        for chip in statusline._persona_chips():
            self.assertRegex(_plain(chip), r"^[▸▫] \S")


class RowsStayTrue(PersonaIso):
    """Both changes make a chip's badges *conditional*, which is the exact shape that has
    broken this layout before: a glyph on some rows and not others moves one row and not
    its neighbour, and a header has no sibling to reveal the drift.

    The frame is the ruler that catches it. A row whose real width disagrees with what
    `tui.width` believes pushes its own `│` out of line, so "every line is the same width"
    is a stronger assertion than it looks.
    """

    def setUp(self) -> None:
        super().setUp()
        from charter import update
        spawn = update.maybe_spawn          # never fork a network child from the suite
        update.maybe_spawn = lambda: None
        self.addCleanup(lambda: setattr(update, "maybe_spawn", spawn))
        # A roster where every conditional badge is both present and absent: `flying`
        # declares a vault this machine has never registered (`◦`) and is in flight;
        # `quiet` declares no vault at all (nothing) and is not.
        self.make_persona("flying", role="F", vault="nowhere", **{"delegate-when": "x"})
        self.make_persona("quiet", role="Q", **{"delegate-when": "x"})
        _start_aged("flying", 4 * 60)

    def _rendered(self, width=200):
        os.environ["COLUMNS"] = str(width)
        self.addCleanup(os.environ.pop, "COLUMNS", None)
        return [_plain(ln) for ln in statusline.render({}).split("\n") if ln.strip()]

    def _content(self, width=200):
        """Rendered rows with the frame peeled off — the frame's own right border is a
        `│` too, and this test is about the divider *between* the columns."""
        out = []
        for ln in self._rendered(width):
            if set(ln.strip()) <= set("┌─┐└┘├┤"):
                continue
            if ln.startswith("│ ") and ln.rstrip().endswith("│"):
                ln = ln[2:].rstrip()[:-1].rstrip()
            out.append(ln)
        return out

    def test_a_chip_with_badges_is_no_wider_than_one_without(self):
        from charter import tui
        for width in (80, 140, 200):
            with self.subTest(columns=width):
                rows = self._rendered(width)
                self.assertGreater(len(rows), 4, rows)   # not vacuous: real rows measured
                widths = {tui.width(ln) for ln in rows}
                self.assertEqual(len(widths), 1, f"rows disagree on width: {sorted(widths)}")

    def test_names_still_start_where_the_header_says_they_do(self):
        """The vault mark disappearing must not move a name: the marker before it is
        exactly two columns and the badges all trail."""
        rows = [ln for ln in self._content() if ln.find("│", 40) > 0]
        self.assertGreater(len(rows), 2, rows)   # header + both chips, or it proves nothing
        starts = set()
        for ln in rows:
            right = ln[ln.find("│", 40) + 1:]
            m = re.search(r"[A-Za-z]", right)
            self.assertIsNotNone(m, right)
            starts.add(m.start())
        self.assertEqual(len(starts), 1,
                         f"right-column text starts at differing columns: {sorted(starts)}")

    def test_the_column_shows_both_signals_at_once(self):
        """Guards the fixture: a width assertion over rows that carry no badges proves
        nothing."""
        col = "\n".join(_plain(c) for c in statusline._persona_chips())
        self.assertIn("◦", col)
        self.assertIn("⚡", col)


class Records(PersonaIso):
    """`live()` answers "who", which is all the aggregate ever needed. The chip needs
    "since when" too, and must not learn to read the state directory itself to get it."""

    def test_records_carry_a_start_time(self):
        _start_aged("devops", 90)
        recs = inflight.live_records()
        self.assertEqual([n for n, _ in recs], ["devops"])
        self.assertAlmostEqual(recs[0][1], time.time() - 90, delta=5)

    def test_records_preserve_duplicates(self):
        _start_aged("devops", 60)
        _start_aged("devops", 30)
        self.assertEqual(len(inflight.live_records()), 2)

    def test_records_prune_the_stale_like_live_does(self):
        _start_aged("devops", inflight.TTL_SECONDS + 60)
        self.assertEqual(inflight.live_records(), [])

    def test_live_still_answers_with_names(self):
        _start_aged("devops", 60)
        _start_aged("release", 60)
        self.assertEqual(inflight.live(), ["devops", "release"])

    def test_a_record_written_without_a_timestamp_falls_back_to_mtime(self):
        """Records written by an older charter carry no ``ts``. A chip that renders `?`
        for one is worse than one that uses the mtime, which is the same instant."""
        token = _start_aged("devops", 120)
        p = Path(config.STATE_DIR) / "dispatch-inflight" / f"{token}.json"
        when = p.stat().st_mtime
        p.write_text(json.dumps({"agent": "devops"}))
        os.utime(p, (when, when))
        recs = inflight.live_records()
        self.assertAlmostEqual(recs[0][1], when, delta=5)


if __name__ == "__main__":
    unittest.main()
