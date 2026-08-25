"""`[frame]` in charter.toml, with defaults that hold when it is absent.

Defaults are the shipped behaviour, so they are asserted rather than assumed: `mouse` is
off because `set -g mouse on` takes over drag-select, and breaking the operator's copy to
enable a feature v1 does not ship is a bad trade.

`slots` is every edge charter draws (#386, narrowed to three by #488's retirement of
`left`), and the reason is the same kind of trade read the other way: inside a frame
`charter statusline` draws nothing (ADR 0019), so an edge the frame does not fill is
information nobody sees at all. The order is asserted too, because it is the split order
and therefore the geometry — see `instance.FRAME_FIELDS`.
"""

from __future__ import annotations

import unittest
from unittest import mock

from charter import instance


class FrameDefaults(unittest.TestCase):
    def test_an_absent_section_yields_the_shipped_defaults(self):
        f = instance.frame_of({})
        self.assertEqual(f["slots"], ["top", "bottom", "right"])
        self.assertIs(f["mouse"], False)
        self.assertEqual(f["hotkey"], "F2")
        self.assertEqual(f["history_limit"], 50000)
        self.assertEqual(f["min_cols"], 100)
        self.assertEqual(f["min_rows"], 20)

    def test_a_section_overrides_only_what_it_names(self):
        f = instance.frame_of({"frame": {"mouse": True, "hotkey": "F5"}})
        self.assertIs(f["mouse"], True)
        self.assertEqual(f["hotkey"], "F5")
        self.assertEqual(f["slots"], ["top", "bottom", "right"])

    def test_an_unknown_slot_is_dropped_rather_than_carried(self):
        """A typo must not reach a tmux argv. Dropping is louder than it looks: the slot
        simply does not appear, and `doctor` has the config to report.

        Until #386 this could not tell "filtered" from "ignored" on its own: the default
        was `["top", "bottom"]`, so `["top", "sideways", "bottom"]` filtered to something
        byte-identical to it and a stub that always returned the default passed too. The
        shipped default carries `right` as well, so the two answers differ and this test
        distinguishes them by itself; `test_a_valid_slots_override_actually_takes_effect`
        below stays, because that is a property worth pinning on purpose rather than by
        the luck of what the default happens to be this release.
        """
        f = instance.frame_of({"frame": {"slots": ["top", "sideways", "bottom"]}})
        self.assertEqual(f["slots"], ["top", "bottom"])

    def test_a_valid_slots_override_actually_takes_effect(self):
        """Companion to the test above: a *different* valid override must actually be
        honoured, not just filtered. Every slot name is in the shipped default, so
        "shares no element with it" is not available to anything; what rules a
        default-returning stub out here is the length."""
        f = instance.frame_of({"frame": {"slots": ["bottom", "right"]}})
        self.assertEqual(f["slots"], ["bottom", "right"])

    def test_the_operators_own_slot_order_is_kept_exactly(self):
        """**Order is geometry, so order is a promise.** `layout.panel_argvs` splits each
        slot off the harness pane in list order, so `["top", "right", "bottom"]` and
        `["top", "bottom", "right"]` are two different frames from the same three names
        — measured on tmux 3.7c at 200x50, a 177-column bottom row inset beside the
        sidebar versus a full-width 200-column one (see `instance.FRAME_FIELDS`).

        Nothing pinned that. A `frame_of` that re-sorted an operator's `slots` into the
        shipped order — silently handing them a frame they did not ask for — passed the
        entire suite, including the test above, because every list it was ever given was
        already in the default's relative order. `["right", "top"]` is the shortest list
        that is not."""
        f = instance.frame_of({"frame": {"slots": ["right", "top"]}})
        self.assertEqual(f["slots"], ["right", "top"])

    def test_a_retired_slot_name_is_dropped_the_way_a_typo_is(self):
        """#488 retired `left`, and a committed `charter.toml` outlives a charter
        upgrade: a plane (or a teammate's checkout) still carrying the four-slot list is
        the ORDINARY case for a release or two, not an edge one. `instance.FRAME_SLOTS`
        is what makes it safe — the dead name is filtered here, so nothing downstream
        splits a pane no renderer will ever draw in. Asserted with the exact list that
        shipped as the default one release ago."""
        f = instance.frame_of(
            {"frame": {"slots": ["top", "bottom", "left", "right"]}})
        self.assertEqual(f["slots"], ["top", "bottom", "right"])

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
        self.assertEqual(f["slots"], ["top", "bottom", "right"])
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


class HotkeyIsNotAFreeString(unittest.TestCase):
    """`hotkey` is the one `[frame]` value that reaches a PARSER, and it was the one
    with no check beyond `isinstance(value, str)`.

    `commands_frame.conf_text` interpolates it into tmux config text that `source-file`
    parses and runs. Verified against a real tmux 3.7c, through the exact text
    `conf_text` produces: `hotkey = "F2\\nrun-shell 'touch /tmp/PWNED'"` makes
    `source-file` return **0**, silently, and the canary file appears **at launch, with
    no keypress at all** — the newline ends the `bind` line and starts a second command.
    `charter.toml` is committed and shared; it arrives from someone else's machine,
    which is exactly the input class the containment rule exists for.

    Checked at the config boundary rather than inside the frame, which is the point:
    `slots` is set-filtered here, `mouse` is a bool here, the three numbers are
    int-checked here — this was the fifth input arriving through a fifth door.
    """

    def test_a_newline_bearing_hotkey_degrades_to_the_default(self):
        """The exploit itself, as an assertion. Fails if `_HOTKEY_RE` is deleted or
        loosened to accept `\\n` — nothing else in `frame_of` would stop it."""
        f = instance.frame_of({"frame": {"hotkey": "F2\nrun-shell 'touch /tmp/PWNED'"}})
        self.assertEqual(f["hotkey"], "F2")

    def test_every_shape_that_could_break_out_of_the_bind_line_is_refused(self):
        """One case per escape route out of `bind -n {hotkey} run-shell '…'`: end the
        line, end the command, open a quote, start a comment or a tmux format, reach the
        shell, or split the argument. Each must fall back to `F2` — an `assertEqual`
        per shape rather than one loop assertion, so a regression names which shape."""
        for hostile in ["F2\nkill-server",
                        "F2; kill-server",
                        "F2 run-shell 'touch /tmp/x'",
                        "F2'",
                        'F2"',
                        "F2#{client_name}",
                        "F2$(touch /tmp/x)",
                        "F2\\",
                        "F2\tkill-server",
                        "{}"]:
            with self.subTest(hotkey=hostile):
                f = instance.frame_of({"frame": {"hotkey": hostile}})
                self.assertEqual(f["hotkey"], "F2",
                                 f"{hostile!r} was accepted into a tmux config line")

    def test_the_key_names_an_operator_actually_types_are_still_accepted(self):
        """The other half, and the one that stops the fix being "reject everything":
        a guard that only ever returned the default would pass every test above and
        silently take `[frame] hotkey` away from everyone."""
        for good in ["F2", "F12", "M-m", "C-b", "S-Left", "C-M-x", "Escape", "BSpace",
                     "PPage", "Up", "a", "7", "/", "C-/"]:
            with self.subTest(hotkey=good):
                self.assertEqual(instance.frame_of({"frame": {"hotkey": good}})["hotkey"],
                                 good)

    def test_a_refused_hotkey_degrades_silently_and_no_surface_names_it(self):
        """A CHARACTERIZATION test: this pins a known gap, not a desired behaviour.

        `_HOTKEY_RE`'s own comment used to claim a refused key "costs the operator their
        preferred hotkey and a line in `charter frame-probe`". It never did — measured
        with the newline payload in charter.toml, `frame-probe` prints a clean green tick
        and `doctor`'s frame row is green, while the hotkey silently becomes `F2`. That
        false claim is the same class this branch removed from `frame/menu.py` and
        `frame/tmuxctl.py`, so the truth is asserted here rather than only written down.

        It is deliberately left this way for now: NO refused `[frame]` value is reported
        anywhere — a dropped `slots` entry and a rejected `history-limit` are exactly as
        quiet — so the fix is one surface for the whole section, filed as a follow-up. If
        you are implementing that follow-up, this test SHOULD fail; update it, do not
        route around it."""
        from charter import commands_frame, config, doctor
        hostile = "F2\nrun-shell 'touch /tmp/PWNED'"
        resolved = instance.frame_of({"frame": {"hotkey": hostile}})
        self.assertEqual(resolved["hotkey"], "F2")
        with mock.patch("charter.frame.tmuxctl.version", return_value=(3, 7)), \
             mock.patch.dict(config.FRAME, resolved):
            code, level, line = commands_frame.frame_ready()
            row = doctor.check_frame()
        self.assertEqual((code, level), (0, "ok"),
                         "the probe does not currently know a hotkey was refused")
        self.assertNotIn("hotkey", line)
        self.assertEqual(row.status, doctor.OK)
        self.assertNotIn("hotkey", row.hint or "")

    def test_a_non_string_hotkey_still_degrades_the_way_it_always_did(self):
        """The type check the regex sits behind must survive it: `_HOTKEY_RE.fullmatch`
        raises `TypeError` on a non-`str`, and `frame_of` is imported by every command
        including `charter --version`."""
        for bad in [42, True, None, ["F2"], {"key": "F2"}]:
            with self.subTest(hotkey=bad):
                self.assertEqual(instance.frame_of({"frame": {"hotkey": bad}})["hotkey"],
                                 "F2")


if __name__ == "__main__":
    unittest.main()
