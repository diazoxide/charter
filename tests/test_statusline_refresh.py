"""The status line refreshes on a timer, not only on events.

Claude Code re-runs the status line on session events — a prompt, a tool call, a directory
change. Those triggers go quiet in exactly the situation charter built the fleet spine for:
a coordinator waiting on background workers, where nothing in the main session happens for
minutes at a time. The docs name that case directly:

    The event-driven triggers can go quiet when the main session is idle, for example while
    a coordinator waits on background subagents. To keep time-based or externally-sourced
    segments current during idle periods, set `refreshInterval`.

Charter's status line is full of exactly that: `silent 12m` ages with wall-clock, piece
counts change as background workers claim and declare, and the plane-root warning fires on
state another agent creates. All of it froze while the session idled.

**Ten seconds, not the one-second minimum.** Charter renders silence in MINUTES, so a
one-second timer is ~60× finer than the coarsest thing it displays, at ~8% of a core
continuously (a render is ~80ms on a two-clone plane, and it runs a `git status` per tree).
Ten is still six times finer than the granularity and costs about a tenth of that.
"""

from __future__ import annotations

import json
import unittest
from pathlib import Path

from charter import commands
from tests._isolation import PersonaIso


class RefreshCase(PersonaIso):
    def settings_path(self) -> Path:
        return Path(self.tmp) / ".claude" / "settings.json"

    def write(self, body: dict) -> None:
        p = self.settings_path()
        p.parent.mkdir(parents=True, exist_ok=True)
        p.write_text(json.dumps(body, indent=2))

    def read(self) -> dict:
        return json.loads(self.settings_path().read_text())


class TestAFreshPlaneGetsIt(RefreshCase):
    def test_the_written_statusline_carries_a_refresh_interval(self):
        commands._ensure_statusline(Path(self.tmp))
        self.assertEqual(self.read()["statusLine"]["refreshInterval"], 10)

    def test_the_command_and_padding_are_unchanged(self):
        commands._ensure_statusline(Path(self.tmp))
        sl = self.read()["statusLine"]
        self.assertEqual(sl["command"], "charter statusline")
        self.assertEqual(sl["type"], "command")


class TestAnExistingPlaneIsUpgraded(RefreshCase):
    def test_charters_own_statusline_gains_the_field(self):
        """Without this the feature reaches only brand-new planes — including not this one.
        Updating a key charter itself wrote is a different act from touching a user's."""
        self.write({"statusLine": {"type": "command", "command": "charter statusline",
                                   "padding": 0}})
        status, _ = commands._ensure_statusline(Path(self.tmp))
        self.assertEqual(status, "updated")
        self.assertEqual(self.read()["statusLine"]["refreshInterval"], 10)

    def test_a_deliberate_value_is_never_overwritten(self):
        """Someone who set 3 meant 3. Silently reverting a hand-made choice is the exact
        behaviour `_ensure_statusline` was written to avoid."""
        self.write({"statusLine": {"type": "command", "command": "charter statusline",
                                   "refreshInterval": 3}})
        status, _ = commands._ensure_statusline(Path(self.tmp))
        self.assertEqual(status, "present")
        self.assertEqual(self.read()["statusLine"]["refreshInterval"], 3)

    def test_someone_elses_statusline_is_left_alone(self):
        """The whole justification is that charter is updating ITS OWN key. A status line
        running someone else's script is not charter's to change."""
        self.write({"statusLine": {"type": "command", "command": "~/bin/my-statusline.sh"}})
        status, _ = commands._ensure_statusline(Path(self.tmp))
        self.assertEqual(status, "present")
        self.assertNotIn("refreshInterval", self.read()["statusLine"])

    def test_other_keys_survive_the_upgrade(self):
        self.write({"permissions": {"allow": ["Bash(ls:*)"]},
                    "statusLine": {"type": "command", "command": "charter statusline"}})
        commands._ensure_statusline(Path(self.tmp))
        body = self.read()
        self.assertEqual(body["permissions"], {"allow": ["Bash(ls:*)"]})

    def test_a_malformed_file_is_still_left_completely_alone(self):
        p = self.settings_path()
        p.parent.mkdir(parents=True, exist_ok=True)
        p.write_text("{ not json")
        status, _ = commands._ensure_statusline(Path(self.tmp))
        self.assertEqual(status, "malformed")
        self.assertEqual(p.read_text(), "{ not json")


class TestTheIntervalIsJustified(unittest.TestCase):
    def test_it_is_not_the_aggressive_minimum(self):
        """A one-second timer is ~60× finer than the minute granularity charter displays,
        for ~8% of a core continuously. The number should track what is shown, not what is
        allowed."""
        self.assertGreaterEqual(commands._STATUSLINE["refreshInterval"], 5)

    def test_it_is_fine_enough_to_beat_the_display_granularity(self):
        """Silence renders in minutes; a timer coarser than that would notice a stalled
        worker late, which is the case this exists for."""
        self.assertLessEqual(commands._STATUSLINE["refreshInterval"], 30)


if __name__ == "__main__":
    unittest.main()
