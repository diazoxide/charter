"""`dispatch.tally` stops re-reading every month file ever written — #887.

The tally is on a per-turn repaint path: `persona.by_use` calls it for the persona
switcher's order (#882), `statusline._persona_chip_cells` draws that column, and
`frame/panel.run` holds a process repainting it for as long as the frame lives. Behind it
`_read_all` walked EVERY month file under `personas/_dispatch/` and parsed every line —
a cost monotonic in the age of the plane and in nothing an operator does. Measured on
charter's own plane at its ~225 dispatches a month: 0.37 ms today, 2.10 ms after a year,
4.22 ms after two, 10.71 ms after five — against 0.06 / 0.27 / 0.54 / 1.32 ms once each
closed month has been read once.

`dispatch._rows_of` now memoises each file's parsed rows on ``(path, mtime_ns, size)``.
A month file that is not the current one is closed — `path_for` only ever writes
``<this month>.<host>.jsonl`` — so every month but one is a permanent hit and the reading
is bounded by the current month however old the plane is.

**Every case here is an invalidation, not a hit.** A cache that returns the right answer
is not evidence of anything; deleting it entirely leaves every value assertion passing.
What has to be pinned is that a file which changed after being memoised is read again —
and each case forges exactly one half of the key so that dropping the other half from it
goes red:

* :meth:`~SizeIsInTheKey.test_a_rewrite_inside_one_mtime_tick_is_seen` holds the mtime
  still and changes the size. Without ``size`` in the key this reads a stale tally, and
  mtime granularity is a filesystem's promise rather than a fact: APFS keeps nanoseconds,
  some ext4 configurations report whole seconds, and `record` appends many times inside
  one such tick.
* :meth:`~MtimeIsInTheKey.test_a_rewrite_at_the_same_size_is_seen` holds the size still
  and moves the mtime. Without ``mtime`` in the key that one reads stale too.

Both forge the stamp through `os.utime` and then **assert the forgery took**, so neither
can pass because the filesystem quietly refused to hold the value the case is about.
"""
from __future__ import annotations

import json
import os
import shutil
import tempfile
import unittest
from unittest import mock
from datetime import datetime, timedelta, timezone
from pathlib import Path

from charter import dispatch
from tests._isolation import PersonaIso, point_config_at


class MonthStore(PersonaIso):
    """A plane holding month files, and the two ways to change one behind charter's back."""

    def month(self, *agents: str) -> Path:
        """One month file, written by the real writer — `dispatch.record`."""
        for a in agents:
            self.assertIsNotNone(dispatch.record(a), f"{a}: nothing was recorded")
        files = sorted(dispatch._dir().glob("*.jsonl"))
        self.assertEqual(len(files), 1, "this fixture writes exactly one month file")
        return files[0]

    def rewrite(self, p: Path, old: str, new: str, *, keep_mtime: bool) -> os.stat_result:
        """Replace *old* with *new* throughout *p*, optionally putting the mtime back.

        The stamp is read before the write and re-asserted after it: a filesystem that
        cannot hold the nanoseconds `os.utime` is handed would otherwise turn a case about
        one half of the key into a case about neither half, and it would pass.
        """
        before = p.stat()
        p.write_text(p.read_text().replace(old, new))
        if keep_mtime:
            os.utime(p, ns=(before.st_atime_ns, before.st_mtime_ns))
            self.assertEqual(p.stat().st_mtime_ns, before.st_mtime_ns,
                             "this filesystem would not hold the forged mtime")
        return before


class SizeIsInTheKey(MonthStore):
    """``size``, because a coarse clock hides a rewrite that a length does not."""

    def test_a_rewrite_inside_one_mtime_tick_is_seen(self):
        p = self.month("aaa", "aaa", "aaa")
        self.assertEqual(dispatch.tally()["aaa"], 3)      # memoises this version
        before = self.rewrite(p, '"aaa"', '"bbbb"', keep_mtime=True)
        self.assertNotEqual(p.stat().st_size, before.st_size,
                            "the case needs the size to move and nothing else")
        self.assertEqual(dispatch.tally()["bbbb"], 3,
                         "a file rewritten inside one mtime tick was served from the memo")
        self.assertEqual(dispatch.tally()["aaa"], 0)


class MtimeIsInTheKey(MonthStore):
    """``mtime``, because a rewrite can leave a file exactly as long as it was."""

    def test_a_rewrite_at_the_same_size_is_seen(self):
        p = self.month("aaa", "aaa", "aaa")
        self.assertEqual(dispatch.tally()["aaa"], 3)
        before = p.stat()
        p.write_text(p.read_text().replace('"aaa"', '"bbb"'))
        os.utime(p, ns=(before.st_atime_ns, before.st_mtime_ns + 1_000_000_000))
        self.assertEqual(p.stat().st_size, before.st_size,
                         "the case needs the mtime to move and nothing else")
        self.assertNotEqual(p.stat().st_mtime_ns, before.st_mtime_ns)
        self.assertEqual(dispatch.tally()["bbb"], 3,
                         "a file whose mtime moved was served from the memo")
        self.assertEqual(dispatch.tally()["aaa"], 0)


class TheMemoIsUsedAtAll(MonthStore):
    """The one case that goes red when the memo is removed rather than weakened.

    It changes a file's CONTENT while holding both halves of the key still — the one
    change charter is entitled not to notice, because no filesystem produces it — and
    demands the old answer. Every other case here demands the new one.
    """

    def test_an_unchanged_stamp_is_not_read_again(self):
        p = self.month("aaa", "aaa", "aaa")
        self.assertEqual(dispatch.tally()["aaa"], 3)
        before = self.rewrite(p, '"aaa"', '"bbb"', keep_mtime=True)
        self.assertEqual(p.stat().st_size, before.st_size,
                         "the rewrite has to leave the length alone to say anything")
        self.assertEqual(dispatch.tally()["aaa"], 3,
                         "the file was parsed again although its stamp had not moved")
        self.assertEqual(dispatch.tally()["bbb"], 0)


class TheCurrentMonthStaysLive(MonthStore):
    """The invalidation that happens every day, through the writer rather than a forgery.

    A closed month is a permanent hit; the current one is appended to by `record` many
    times a turn, and every one of those has to be visible to the next repaint.
    """

    def test_a_dispatch_recorded_after_a_read_is_counted(self):
        dispatch.record("devops")
        self.assertEqual(dispatch.tally()["devops"], 1)
        dispatch.record("devops")
        self.assertEqual(dispatch.tally()["devops"], 2,
                         "a dispatch appended after a read was hidden by the memo")

    def test_a_closed_month_and_a_live_one_are_both_counted(self):
        old = datetime.now(timezone.utc) - timedelta(days=95)
        dispatch.record("devops", when=old)
        self.assertEqual(dispatch.tally()["devops"], 1)
        dispatch.record("devops")
        self.assertEqual(dispatch.tally()["devops"], 2)
        self.assertEqual(len({f.name for f in dispatch._dir().glob("*.jsonl")}), 2,
                         "the case needs two month files to be saying anything")

    def test_the_other_readers_see_the_same_appends(self):
        """`tally` is the one on the repaint path, but every reader shares `_read_all`,
        so a memo that went stale would go stale for all of them at once."""
        dispatch.record_advice()
        dispatch.record_resume("devops")
        self.assertEqual((dispatch.advice_tally(), dispatch.resume_tally()), (1, 1))
        dispatch.record_advice()
        dispatch.record_resume("devops")
        self.assertEqual((dispatch.advice_tally(), dispatch.resume_tally()), (2, 2))
        self.assertIsNotNone(dispatch.last_seen("devops"))


class TheKeyIsAWholePath(MonthStore):
    """Two planes name their month files identically — the same month, the same host.

    The memo is keyed on the full path for that reason. Keyed on the file's NAME, the
    second plane would be handed the first plane's tally, and every other case here would
    still pass: they only ever look at one plane.
    """

    def test_two_planes_do_not_share_one_months_rows(self):
        dispatch.record("aaa")
        first = sorted(dispatch._dir().glob("*.jsonl"))[0]
        self.assertEqual(dispatch.tally(), {"aaa": 1})

        other = Path(tempfile.mkdtemp(prefix="edm-test-other-"))
        self.addCleanup(shutil.rmtree, other, True)
        point_config_at(self, other)
        d = dispatch._dir()
        d.mkdir(parents=True, exist_ok=True)
        second = d / first.name
        second.write_text(first.read_text().replace('"aaa"', '"bbb"'))
        # The two files are made INDISTINGUISHABLE except by path: same name, same
        # length, same mtime to the nanosecond. Anything less and the stamps differ, the
        # memo misses for a reason that has nothing to do with the key, and a name-keyed
        # memo passes this case while still being wrong — which is how it passed the
        # first time this was written. `cp -p`, `rsync -a` and an unpacked tarball all
        # carry an mtime across, so two clones of one plane really can reach this.
        st = first.stat()
        os.utime(second, ns=(st.st_atime_ns, st.st_mtime_ns))
        self.assertNotEqual(second, first, "the case needs two planes, not one read twice")
        self.assertEqual((second.stat().st_mtime_ns, second.stat().st_size),
                         (st.st_mtime_ns, st.st_size),
                         "the two month files have to be stamped alike to say anything")
        self.assertEqual(dispatch.tally(), {"bbb": 1},
                         "a second plane was answered out of the first plane's memo")


class TheStatComesBeforeTheRead(MonthStore):
    """The order of two statements, and it is the difference between late and never.

    `record` opens the current month file ``O_APPEND`` from any process on the machine, so
    a row can land between the stat and the read. Stamped BEFORE, that row is memoised
    under the older stamp and the next call re-reads the file — the row is one repaint
    late. Stamped AFTER, the SAME row is memoised under the newer stamp, so every later
    call is a hit and the row is invisible for the whole life of the process — and a frame
    panel is a process that lives as long as the frame does.

    The race is made deterministic rather than waited for: `Path.read_text` appends the
    row itself, once, at the exact instant the window is open.
    """

    def test_a_row_that_lands_during_the_read_is_not_lost(self):
        p = self.month("aaa")
        real, fired = Path.read_text, []

        def racing(self_, *a, **kw):
            text = real(self_, *a, **kw)
            if self_ == p and not fired:
                fired.append(True)
                with open(p, "a") as fh:
                    fh.write(json.dumps(
                        {"agent": "bbb",
                         "ts": datetime.now(timezone.utc).isoformat(timespec="seconds")},
                        sort_keys=True) + "\n")
            return text

        with mock.patch.object(Path, "read_text", racing):
            self.assertEqual(dispatch.tally()["aaa"], 1)
        self.assertTrue(fired, "the case needs the append to have happened mid-read")
        self.assertEqual(dispatch.tally()["bbb"], 1,
                         "a row appended during the read was memoised away for good")


class ACorruptLineIsSkippedAndNotTheFile(MonthStore):
    """A half-written line costs its own row and no other — and now costs it once.

    The store is append-only from any process (`record` uses ``O_APPEND``), so a machine
    that dies mid-append leaves a truncated line behind forever; the file is also
    committed, so a bad merge can leave anything at all in it. Parsing is now MEMOISED,
    which is why these belong here rather than only beside the writer: whatever the parse
    decides about a line, it decides once and every later repaint inherits.
    """

    def rows(self, *lines: str) -> None:
        p = dispatch.path_for()
        p.parent.mkdir(parents=True, exist_ok=True)
        p.write_text("".join(f"{ln}\n" for ln in lines))

    def test_a_truncated_line_costs_only_itself(self):
        good = json.dumps({"agent": "devops", "ts": "2026-09-01T00:00:00+00:00"},
                          sort_keys=True)
        self.rows(good, '{"agent": "qa", "ts', good)
        self.assertEqual(dispatch.tally(), {"devops": 2})

    def test_a_blank_line_is_not_a_row(self):
        good = json.dumps({"agent": "devops", "ts": "2026-09-01T00:00:00+00:00"},
                          sort_keys=True)
        self.rows(good, "", "   ", good)
        self.assertEqual(dispatch.tally(), {"devops": 2})

    def test_a_line_that_is_valid_json_but_not_an_object_is_not_a_row(self):
        good = json.dumps({"agent": "devops", "ts": "2026-09-01T00:00:00+00:00"},
                          sort_keys=True)
        self.rows(good, '["devops", 3]', "7", '"devops"', good)
        self.assertEqual(dispatch.tally(), {"devops": 2},
                         "a JSON array or scalar was read as a dispatch row")

    def test_an_object_carrying_neither_an_agent_nor_an_event_is_not_a_row(self):
        good = json.dumps({"agent": "devops", "ts": "2026-09-01T00:00:00+00:00"},
                          sort_keys=True)
        self.rows(good, '{"ts": "2026-09-01T00:00:00+00:00"}', "{}", good)
        self.assertEqual(sum(dispatch.tally().values()), 2)
        self.assertEqual(len(dispatch._rows_of(dispatch.path_for())), 2,
                         "a row with neither an agent nor an event was kept")


class AMonthFileThatGoesAway(MonthStore):
    """A memo entry is not a reason to keep counting a file that is no longer there.

    `_read_all` decides WHICH files exist by globbing the store, every time, and asks the
    memo only about files it just found. Iterating the memo instead would resurrect rows
    the directory no longer holds — and a backfill run deletes and rewrites every
    `*.backfill.jsonl` it owns, so this is a path charter actually walks.
    """

    def test_a_deleted_month_stops_being_counted(self):
        old = datetime.now(timezone.utc) - timedelta(days=95)
        dispatch.record("devops", when=old)
        dispatch.record("qa")
        self.assertEqual(dispatch.tally(), {"devops": 1, "qa": 1})
        dispatch.path_for(old).unlink()
        self.assertEqual(dispatch.tally(), {"qa": 1},
                         "a deleted month file was still counted out of the memo")


if __name__ == "__main__":
    unittest.main()
