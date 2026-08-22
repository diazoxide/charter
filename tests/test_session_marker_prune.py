"""`_prune` sweeps the session directories, not an allowlist of suffixes (#366).

`_prune` keeps `SESSIONS_DIR` and `TERMINALS_DIR` bounded by unlinking per-session pointers
past `_SESSION_MAX_AGE`. It enumerated the families by glob — `*.workspace`, `*.lock`,
`*.configver`, `*.memnudge`, `*.usage` — and read as an exhaustive list of the family while
being nothing of the kind.

**Three families were missing, not two.** `*.ask-pending` (`hooks._ask_mark`) and
`*.route-pending` (`hooks._route_mark`) were the two reported; `*.persona`
(`persona._pointer_files`, in BOTH directories) is a third the report did not find. The
allowlist has drifted three times, and a reader adding a fourth marker type has nothing
telling them this list needs editing — which is the shape that repeats.

So the sweep is now the directory, not a list of names: any file in those two directories
past the cutoff goes. A marker family added tomorrow is covered the day it is written rather
than the day somebody remembers this function. Both directories are charter's own state and
hold nothing but per-session and per-terminal pointers, so there is no member for which
"keep past the cutoff" is the right answer.

**Nothing countable is lost.** `*.ask-pending` is deliberately left behind by a declined ask
— that asymmetry is what makes "asked N, approved M" countable (#290) — so pruning it had to
be checked against its readers rather than assumed safe. Nothing anywhere globs these
suffixes: `_ask_mark_take`, `_route_mark_take` and `_route_mark_clear` all address one file
by an exact session id and tool-use id. The tally itself lives in the trace store, which
`_prune` does not touch. What ages out at thirty days is an inode whose session ended a
month ago.
"""

from __future__ import annotations

import os
import time
import unittest
from pathlib import Path

from charter import config, workspace
from tests._isolation import PersonaIso


def _age(p: Path, days: float) -> Path:
    """Backdate *p* so `_prune`'s cutoff sees it as *days* old."""
    when = time.time() - days * 86400
    os.utime(p, (when, when))
    return p


class PruneCase(PersonaIso):
    def setUp(self) -> None:
        super().setUp()
        config.SESSIONS_DIR.mkdir(parents=True, exist_ok=True)
        config.TERMINALS_DIR.mkdir(parents=True, exist_ok=True)

    def marker(self, name: str, days: float, terminals: bool = False) -> Path:
        d = config.TERMINALS_DIR if terminals else config.SESSIONS_DIR
        p = d / name
        p.write_text("")
        return _age(p, days)


class TestTheFamiliesThatWereMissing(PruneCase):
    def test_a_stale_ask_pending_marker_is_pruned(self):
        m = self.marker("sid.toolu_x.ask-pending", days=40)
        workspace._prune()
        self.assertFalse(m.exists())

    def test_a_stale_route_pending_marker_is_pruned(self):
        m = self.marker("sid.route-pending", days=40)
        workspace._prune()
        self.assertFalse(m.exists())

    def test_a_stale_session_persona_pointer_is_pruned(self):
        """The third family, which the report did not find."""
        m = self.marker("sid.persona", days=40)
        workspace._prune()
        self.assertFalse(m.exists())

    def test_a_stale_terminal_persona_pointer_is_pruned(self):
        m = self.marker("tid.persona", days=40, terminals=True)
        workspace._prune()
        self.assertFalse(m.exists())


class TestTheFloorDoesNotEatLiveState(PruneCase):
    """Thirty days is the floor under the growth, not a cap on a live session."""

    def test_a_fresh_ask_pending_marker_survives(self):
        m = self.marker("sid.toolu_x.ask-pending", days=1)
        workspace._prune()
        self.assertTrue(m.exists())

    def test_a_fresh_route_pending_marker_survives(self):
        m = self.marker("sid.route-pending", days=3)
        workspace._prune()
        self.assertTrue(m.exists())

    def test_a_fresh_persona_pointer_survives(self):
        m = self.marker("sid.persona", days=29)
        workspace._prune()
        self.assertTrue(m.exists())


class TestAFamilyNobodyHasWrittenYet(PruneCase):
    """The reason the allowlist went rather than growing by three names.

    A marker family added tomorrow is covered the day it is written. This test fails on
    any return to an allowlist, which is the point of writing it.
    """

    def test_an_unknown_marker_suffix_is_pruned_when_stale(self):
        m = self.marker("sid.some-marker-invented-later", days=40)
        workspace._prune()
        self.assertFalse(m.exists())

    def test_an_unknown_marker_suffix_survives_while_fresh(self):
        m = self.marker("sid.some-marker-invented-later", days=1)
        workspace._prune()
        self.assertTrue(m.exists())


class TestTheFamiliesThatAlreadyWorked(PruneCase):
    """Regression: the five the allowlist did cover still go."""

    def test_stale_pointers_of_every_listed_family_are_pruned(self):
        names = ["sid.workspace", "sid.lock", "sid.configver", "sid.memnudge", "sid.usage"]
        made = [self.marker(n, days=40) for n in names]
        workspace._prune()
        for m in made:
            with self.subTest(marker=m.name):
                self.assertFalse(m.exists())

    def test_a_fresh_workspace_pointer_survives(self):
        m = self.marker("sid.workspace", days=1)
        workspace._prune()
        self.assertTrue(m.exists())


class TestItOnlyRemovesFiles(PruneCase):
    def test_a_stale_subdirectory_is_left_alone(self):
        """Sweeping the directory means meeting whatever is in it. A directory is not a
        per-session pointer, and unlinking is not the tool for one."""
        d = config.SESSIONS_DIR / "somedir"
        d.mkdir()
        _age(d, 40)
        workspace._prune()
        self.assertTrue(d.exists())

    def test_it_survives_a_missing_directory(self):
        import shutil

        shutil.rmtree(config.SESSIONS_DIR)
        workspace._prune()  # must not raise


if __name__ == "__main__":
    unittest.main()
