"""`[frame]` in charter.toml, with defaults that hold when it is absent.

Defaults are the shipped behaviour, so they are asserted rather than assumed: `mouse` is
off because `set -g mouse on` takes over drag-select, and breaking the operator's copy to
enable a feature v1 does not ship is a bad trade.
"""

from __future__ import annotations

import unittest

from charter import instance


class FrameDefaults(unittest.TestCase):
    def test_an_absent_section_yields_the_shipped_defaults(self):
        f = instance.frame_of({})
        self.assertEqual(f["slots"], ["top", "bottom"])
        self.assertIs(f["mouse"], False)
        self.assertEqual(f["hotkey"], "F2")
        self.assertEqual(f["history_limit"], 50000)
        self.assertEqual(f["min_cols"], 100)
        self.assertEqual(f["min_rows"], 20)

    def test_a_section_overrides_only_what_it_names(self):
        f = instance.frame_of({"frame": {"mouse": True, "hotkey": "F5"}})
        self.assertIs(f["mouse"], True)
        self.assertEqual(f["hotkey"], "F5")
        self.assertEqual(f["slots"], ["top", "bottom"])

    def test_an_unknown_slot_is_dropped_rather_than_carried(self):
        """A typo must not reach a tmux argv. Dropping is louder than it looks: the slot
        simply does not appear, and `doctor` has the config to report."""
        f = instance.frame_of({"frame": {"slots": ["top", "sideways", "bottom"]}})
        self.assertEqual(f["slots"], ["top", "bottom"])

    def test_a_malformed_section_falls_back_instead_of_raising(self):
        """`config` is imported by every command including `charter --version`, so a bad
        value must never crash import."""
        f = instance.frame_of({"frame": {"slots": "top", "history_limit": "lots"}})
        self.assertEqual(f["slots"], ["top", "bottom"])
        self.assertEqual(f["history_limit"], 50000)

    def test_the_hyphenated_settings_are_actually_read(self):
        """`history-limit`, `min-cols` and `min-rows` are the spellings charter.toml (and
        docs/frame.md) document. A prior draft keyed the merge loop on the underscore
        form, so these three settings matched nothing in a real `[frame]` section and
        silently kept their defaults no matter what an operator wrote — this pins that
        they are read, not merely defaulted."""
        f = instance.frame_of({"frame": {
            "history-limit": 999,
            "min-cols": 42,
            "min-rows": 7,
        }})
        self.assertEqual(f["history_limit"], 999)
        self.assertEqual(f["min_cols"], 42)
        self.assertEqual(f["min_rows"], 7)

    def test_the_underscore_spelling_is_not_a_second_alias(self):
        """Only the hyphenated TOML spelling is honoured. Accepting the underscore form
        too would give one setting two undocumented spellings, with no rule for which
        wins the day they disagree — so `history_limit` in charter.toml is inert and the
        default holds."""
        f = instance.frame_of({"frame": {"history_limit": 999}})
        self.assertEqual(f["history_limit"], 50000)


if __name__ == "__main__":
    unittest.main()
