"""Which persona was last seen in a tree, rendered on the row for that tree.

A fleet coordinator reading the status line can see what each piece *declared* and how long
it has been *silent*, but not who is in it. The claim log has carried ``persona`` since ADR
0011 — but that names whoever *created* the piece, and stays true forever after they leave.
The heartbeat, which is the thing that actually decays, carried only ``{ts, session}``.

**An observation, never an assertion.** Charter cannot verify that anyone is working, so
the cell reads like `silent 3d` already does — a name and an age, with the reader drawing
the conclusion. `▸steward now` says "seen just now", not "steward is working here". ADR
0011 has no ``failed`` or ``blocked`` for exactly this reason, and `_piece_state`'s
docstring makes the same point: never a verdict.

**Recorded, not joined.** The persona could have been derived by looking up the claim for
this piece and reading the persona off it — no schema change at all. It would be wrong the
moment a second persona picks up someone else's piece, which is the case the fleet spine
exists for. The heartbeat records who is *there*; the claim records who *was*.

**Clones too, the plane root never.** Working directly in a clone is ordinary and was
invisible. The plane root already carries an alert whose whole message is *don't work
here*; marking who is present would decorate the thing charter is telling you to stop.
"""

from __future__ import annotations

import json
import re
import unittest
from datetime import datetime, timedelta, timezone

from charter import config, pieces, workspace
from tests._isolation import PersonaIso


def _plain(text: str) -> str:
    return re.sub(r"\x1b\[[0-9;]*m", "", text)


class PresenceCase(PersonaIso):
    WS = "w1"

    def at(self, minutes: int) -> datetime:
        return datetime.now(timezone.utc) - timedelta(minutes=minutes)

    def beat(self, persona: str, minutes: int = 0, repo: str = "svc",
             piece: str | None = "p1", session: str = "s") -> None:
        pieces.seen(self.WS, repo, piece, session=session, persona=persona,
                    when=self.at(minutes))


class TestTheHeartbeatCarriesThePersona(PresenceCase):
    def test_it_is_written(self):
        self.beat("steward")
        self.assertEqual(pieces.last_seen(self.WS, "svc", "p1")["persona"], "steward")

    def test_an_older_record_without_one_still_reads(self):
        """Heartbeats written by an earlier charter carry no persona. They are overwritten
        within a turn, but the render in between must not blow up on them."""
        p = pieces.seen_path(self.WS, "svc", "p1")
        p.parent.mkdir(parents=True, exist_ok=True)
        p.write_text(json.dumps({"ts": self.at(2).isoformat(), "session": "s"}))
        self.assertIsNone(pieces.presence(self.WS, "svc", "p1"))

    def test_the_claim_is_not_used_as_the_answer(self):
        """The claim knows a persona, and reading it here would need no new field at all —
        and would report the persona who CREATED the piece as the one standing in it."""
        pieces.record(self.WS, "claimed", "svc", "p1")
        self.assertIsNone(pieces.presence(self.WS, "svc", "p1"))


class TestItReadsAsAnObservation(PresenceCase):
    def test_a_fresh_beat_says_now(self):
        """`0m` is technically right and reads as broken; a bare name is the assertion
        charter is not entitled to make."""
        self.beat("steward", minutes=0)
        self.assertEqual(pieces.presence(self.WS, "svc", "p1")[1], "now")

    def test_an_older_beat_carries_its_age(self):
        self.beat("steward", minutes=7)
        self.assertEqual(pieces.presence(self.WS, "svc", "p1")[1], "7m")

    def test_a_much_older_beat_reads_coarsely(self):
        """Same grammar `pieces.since` already uses — the number is context for a human
        decision, not an input to one charter is making."""
        self.beat("steward", minutes=60 * 5)
        self.assertEqual(pieces.presence(self.WS, "svc", "p1")[1], "5h")

    def test_nothing_seen_reads_as_nothing(self):
        self.assertIsNone(pieces.presence(self.WS, "svc", "p1"))


class TestASecondPersonaIsCounted(PresenceCase):
    def test_the_most_recent_persona_wins_the_cell(self):
        self.beat("forge", minutes=9)
        self.beat("steward", minutes=1)
        self.assertEqual(pieces.presence(self.WS, "svc", "p1")[0], "steward")

    def test_the_other_is_counted_not_dropped(self):
        """One overwritten file per piece keeps the store bounded, so the second persona
        used to vanish without trace. The count is the honest admission that the cell is
        not the whole truth."""
        self.beat("forge", minutes=9)
        self.beat("steward", minutes=1)
        self.assertEqual(pieces.presence(self.WS, "svc", "p1")[2], 1)

    def test_one_persona_counts_no_others(self):
        self.beat("steward")
        self.assertEqual(pieces.presence(self.WS, "svc", "p1")[2], 0)

    def test_the_same_persona_twice_is_still_one(self):
        self.beat("steward", minutes=9)
        self.beat("steward", minutes=1)
        self.assertEqual(pieces.presence(self.WS, "svc", "p1")[2], 0)

    def test_a_persona_who_left_long_ago_is_not_counted(self):
        """Otherwise `+1` accumulates forever and means "someone was here once", which is
        not a fact worth a column."""
        self.beat("forge", minutes=60 * 24)
        self.beat("steward", minutes=1)
        self.assertEqual(pieces.presence(self.WS, "svc", "p1")[2], 0)

    def test_the_map_cannot_grow_without_bound(self):
        for i in range(20):
            self.beat(f"p{i:02d}", minutes=1)
        blob = json.loads(pieces.seen_path(self.WS, "svc", "p1").read_text())
        self.assertLessEqual(len(blob.get("by") or {}), pieces.PRESENCE_KEEP)


class TestACloneIsTrackedToo(PresenceCase):
    def test_a_clone_has_its_own_heartbeat(self):
        """`piece=None` is a clone: working in one directly is ordinary and was invisible,
        because `_touch_piece` returned early outside a worktree."""
        self.beat("steward", piece=None)
        self.assertEqual(pieces.presence(self.WS, "svc", None)[0], "steward")

    def test_a_clone_and_its_worktree_do_not_share_a_record(self):
        """A file beside the directory, never inside it — a piece named like the sentinel
        could otherwise collide with the clone's own record."""
        self.beat("steward", piece=None)
        self.beat("forge", piece="p1")
        self.assertEqual(pieces.presence(self.WS, "svc", None)[0], "steward")
        self.assertEqual(pieces.presence(self.WS, "svc", "p1")[0], "forge")

    def test_the_clone_record_is_not_inside_the_piece_directory(self):
        clone = pieces.seen_path(self.WS, "svc", None)
        piece_dir = pieces.seen_path(self.WS, "svc", "p1").parent
        self.assertNotEqual(clone.parent, piece_dir)


class TestWhereTheHookLooks(PersonaIso):
    def paths_for(self, cwd):
        from charter import worktree
        return worktree.locate(cwd), workspace.clone_of(cwd)

    def test_a_clone_is_recognised(self):
        d = config.WORKSPACES_DIR / "w1" / "svc"
        d.mkdir(parents=True, exist_ok=True)
        self.assertEqual(workspace.clone_of(d), ("w1", "svc"))

    def test_a_directory_inside_a_clone_is_recognised(self):
        d = config.WORKSPACES_DIR / "w1" / "svc" / "src" / "deep"
        d.mkdir(parents=True, exist_ok=True)
        self.assertEqual(workspace.clone_of(d), ("w1", "svc"))

    def test_the_plane_root_is_not_a_clone(self):
        """It has its own alert telling you not to work there."""
        self.assertIsNone(workspace.clone_of(config.ROOT))

    def test_the_workspace_directory_itself_is_not_a_clone(self):
        d = config.WORKSPACES_DIR / "w1"
        d.mkdir(parents=True, exist_ok=True)
        self.assertIsNone(workspace.clone_of(d))

    def test_somewhere_outside_the_plane_is_not_a_clone(self):
        self.assertIsNone(workspace.clone_of(self.tmp / "elsewhere"))


class TestItRendersOnTheRow(PresenceCase):
    def test_the_suffix_names_the_persona_and_the_age(self):
        from charter import statusline
        self.beat("steward", minutes=3)
        self.assertEqual(_plain(statusline._presence_text(self.WS, "svc", "p1")),
                         "▸steward 3m")

    def test_the_suffix_carries_the_other_count(self):
        from charter import statusline
        self.beat("forge", minutes=5)
        self.beat("steward", minutes=1)
        self.assertEqual(_plain(statusline._presence_text(self.WS, "svc", "p1")),
                         "▸steward 1m +1")

    def test_nothing_seen_renders_nothing(self):
        from charter import statusline
        self.assertEqual(statusline._presence_text(self.WS, "svc", "p1"), "")

    def test_it_never_raises(self):
        """Every-turn render path: it degrades to an empty cell rather than blanking the
        footer."""
        from charter import statusline
        real = pieces.presence
        pieces.presence = lambda *a, **k: (_ for _ in ()).throw(OSError("nope"))
        self.addCleanup(setattr, pieces, "presence", real)
        self.assertEqual(statusline._presence_text(self.WS, "svc", "p1"), "")

    def test_it_loses_the_column_before_the_branch_does(self):
        """The losing order settled in Q4/Q5: markers and the branch name are true of the
        tree, presence is an extra. A long branch keeps its cell and presence drops out
        rather than squeezing what was already there."""
        from charter import statusline
        self.beat("steward", minutes=3)
        long_branch = "feature/" + ("x" * statusline._BRANCH_W)
        row = _plain(statusline._branch_cell_for(long_branch, "▸steward 3m"))
        self.assertNotIn("steward", row)

    def test_it_keeps_the_column_when_there_is_room(self):
        from charter import statusline
        row = _plain(statusline._branch_cell_for("main", "▸steward 3m"))
        self.assertIn("steward", row)


if __name__ == "__main__":
    unittest.main()
