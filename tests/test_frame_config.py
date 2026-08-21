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
        simply does not appear, and `doctor` has the config to report.

        By itself this does not distinguish "filtered" from "ignored": `["top",
        "sideways", "bottom"]` filters to `["top", "bottom"]`, which is byte-identical to
        the default, so a stub that always returns the default would also pass this one.
        `test_a_valid_slots_override_actually_takes_effect` below is what rules that out.
        """
        f = instance.frame_of({"frame": {"slots": ["top", "sideways", "bottom"]}})
        self.assertEqual(f["slots"], ["top", "bottom"])

    def test_a_valid_slots_override_actually_takes_effect(self):
        """Companion to the test above: a *different* valid override must actually be
        honoured, not just filtered. `["left", "right"]` shares no elements with the
        default `["top", "bottom"]`, so a stub that always returns the default — which
        would still pass the "unknown slot is dropped" test above — fails this one."""
        f = instance.frame_of({"frame": {"slots": ["left", "right"]}})
        self.assertEqual(f["slots"], ["left", "right"])

    def test_a_malformed_section_falls_back_instead_of_raising(self):
        """`config` is imported by every command including `charter --version`, so a bad
        value must never crash import.

        `history-limit` (hyphenated — the real charter.toml spelling; an earlier draft of
        this test used the underscore key, which `frame_of`'s TOML-key mapping never looks
        up, so the bad value was silently ignored as an unknown key rather than rejected
        for its type, and the test passed for the wrong reason). This one fails if the
        final `isinstance(value, type(default))` type check is deleted: without it, the
        string `"lots"` would be assigned straight into `history_limit`."""
        f = instance.frame_of({"frame": {"slots": "top", "history-limit": "lots"}})
        self.assertEqual(f["slots"], ["top", "bottom"])
        self.assertEqual(f["history_limit"], 50000)

    def test_a_bool_does_not_satisfy_a_non_bool_default(self):
        """Pins the bool/int subclass guard itself — the one whose own comment predicts a
        future reader will "simplify it back" to a plain `isinstance(value, type(default))`
        check. `bool` is a subclass of `int` in Python, so `isinstance(True, int)` is True;
        without the guard, `history-limit = true` (and `min-cols = true`) would silently
        pass that check and be accepted as a nonsensical int. Deleting the guard clause
        flips `history_limit` and `min_cols` below from their defaults to `True` — that is
        the assertion that actually fails under that mutation.

        `mouse = 1` is included for completeness (a non-bool must not satisfy a bool
        default, per the brief) but does NOT by itself distinguish guard-present from
        guard-absent: `1` is never an instance of `bool`, so `isinstance(1, bool)` is False
        and `mouse` falls back to its default via the plain type check alone, guard or no
        guard. Confirmed empirically before writing this test, by running both versions."""
        f = instance.frame_of({"frame": {
            "mouse": 1,
            "history-limit": True,
            "min-cols": True,
        }})
        self.assertIs(f["mouse"], False)
        self.assertEqual(f["history_limit"], 50000)
        self.assertEqual(f["min_cols"], 100)

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
