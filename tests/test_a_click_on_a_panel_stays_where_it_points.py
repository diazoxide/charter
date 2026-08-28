"""`[frame] mouse = true` stops taking the keyboard off the harness on every click (#634).

Charter delivers `click` and `scroll` point-to-act: it acts where the pointer is and never
moves focus, because the frame exists to keep the harness the thing you type into. That
held with `[frame] mouse` off and broke with it on — and on is the setting an operator
turns on precisely *because* they want to click panels, so the setting that made the
feature reliable was the setting that made it hostile.

The cause is tmux's, and so is the fix. tmux's default root binding is
``MouseDown1Pane  select-pane -t = \\; send-keys -M``: with its own mouse on it selects the
pane under the pointer *before* forwarding. The wheel never does this. Charter rebinds that
one key inside its own private server, **conditionally on the pane under the pointer being
one charter split off**, so:

* a panel — forward, do not select;
* the harness, the palette's pane, a pane the operator split themselves — tmux's own two
  commands, untouched.

Three files hold the mechanism and this one asks about all three: the pane option
(`commands_frame._PANEL_OPTION`), the write that puts it on every panel
(`_panel_mark_argv`, from `_split_panels`' single funnel), and the bind that reads it
(`conf_text`). `tests/test_frame_input_reaches_a_component.py` is the real-tmux half — it
can say what tmux does with a real click and cannot say whether charter ever wrote the
option; this file is the other way round.

**Every expected string here is a literal.** A case that built its expectation out of
`_PANEL_OPTION` would still pass with the constant mutated to anything at all, which is
the survivor `commands_change.BLOCK_END` produced this week. So the option name, the value
and the whole bind line are spelled out, and a change to any of them is a change to this
file as well.

Measured, and the measurement is what chose the conditional over the two alternatives —
real tmux server, real client on a real pty, ``mouse on``, three panes (a marked panel, the
harness, and one more split by hand standing in for the operator's own), SGR reports
injected as a reporting terminal sends them. **tmux 3.7c and tmux 3.2 —
`tmuxctl.FLOOR`, built from the release tarball — answered identically in all twelve
cells**::

    bind                      click a panel          click back to harness  click own split
    tmux's own default        delivered, MOVED       works                  works
    blanket `send -M`         delivered, unchanged   BROKEN, stays put      BROKEN, stays put
    charter's (conditional)   delivered, unchanged   works                  works

The blanket row is why this is not two words shorter. Dropping the `select-pane` outright
also takes away clicking back to a pane at all — the harness included — which is worse than
the focus steal it fixes.

**The issue's own open question, settled by measurement rather than reasoning: `#{==:}`
DOES parse and evaluate at the 3.2 floor.** On that same 3.2,
``#{==:#{pane_id},#{@charter_harness_pane}}`` evaluated to ``1`` on the harness pane and
``0`` on a panel, and a `bind` line carrying it returned rc 0 from `source-file` and read
back byte for byte through `list-keys`. So the format was available and the SHAPE is what
is refused: marking the *harness* and treating every other pane as un-clickable-to would
take tmux's documented behaviour away from a pane charter has nothing to do with — the one
the operator split themselves. Marking the *panels* leaves that pane exactly as tmux left
it, and leaves no pane id in the binding at all to drift from the one
`overlay.HATCH_OPTION` already holds.
"""

from __future__ import annotations

import unittest

from charter import commands_frame, instance
from charter.frame import overlay, tmuxctl


def _text(*, mouse: bool = True, toggles=None) -> str:
    return commands_frame.conf_text(hotkey="F2", mouse=mouse, history_limit=1,
                                    session="fr-1", toggles=toggles)


def _click_line(text: str) -> str:
    return next(ln for ln in text.split("\n") if ln.startswith("bind -n MouseDown1Pane"))


class TheBindCharterWrites(unittest.TestCase):
    """What `conf_text` actually puts in the file `source-file` parses."""

    def test_the_click_bind_is_the_line_that_was_measured(self):
        """The whole line, byte for byte, as a literal — this is the string a real tmux
        was asked to parse on 3.7c and at the floor, and a test that assembled it from
        the same pieces the code assembles it from would agree with any of them."""
        self.assertIn(
            "bind -n MouseDown1Pane if-shell -F -t = '#{@charter_panel}' "
            "'send-keys -M' 'select-pane -t =; send-keys -M'",
            _text())

    def test_a_panel_is_forwarded_to_and_never_selected(self):
        """The true branch is `send-keys -M` ALONE. A `select-pane` that crept back into
        it would restore the exact defect, and every other assertion in this file would
        still pass — the line would still be present, still conditional, still last-wins
        safe."""
        line = _click_line(_text())
        true_branch = line.split("'")[3]
        self.assertEqual(true_branch, "send-keys -M")

    def test_every_other_pane_keeps_tmuxs_own_two_commands(self):
        """The false branch is tmux's default behaviour, restated. This is what keeps
        clicking BACK to the harness — and into a pane the operator split themselves —
        working, and it is the difference between this and `bind -n MouseDown1Pane
        send -M`, which was measured to break both."""
        line = _click_line(_text())
        false_branch = line.split("'")[5]
        self.assertEqual(false_branch, "select-pane -t =; send-keys -M")

    def test_it_is_bound_whatever_the_mouse_setting_says(self):
        """Not gated on *mouse*, and the reason is sharper than symmetry with the wheel: a
        root key table is server-wide and `source-file` can only ADD a binding. Omitting
        the line on a `mouse = false` launch would not unbind what a `mouse = true` frame
        on the same socket already bound, so a gate would buy nothing and make the root
        table depend on launch order."""
        for mouse in (True, False):
            with self.subTest(mouse=mouse):
                self.assertIn("bind -n MouseDown1Pane", _text(mouse=mouse))

    def test_the_click_bind_sits_between_the_palette_and_the_hatch(self):
        """Both ends load-bearing, for the reasons `conf_text` already argues about the
        toggles: after the palette so a colliding key would visibly steal something (which
        is what keeps `component_tables`' refusal pinnable), and before
        `overlay.hatch_bind()` so a tmux below `tmuxctl.FLOOR` — which cannot parse
        `run-shell -C` — has already applied this line by the time it drops that one."""
        lines = _text(toggles={"repos": "F7"}).split("\n")
        palette = next(i for i, ln in enumerate(lines) if "frame-palette" in ln)
        click = next(i for i, ln in enumerate(lines) if "MouseDown1Pane" in ln)
        hatch = next(i for i, ln in enumerate(lines) if ln == overlay.hatch_bind())
        self.assertLess(palette, click)
        self.assertLess(click, hatch)

    def test_the_escape_hatch_is_still_the_last_line(self):
        """`test_component_toggle_keys.py` pins this for the toggles; asked again here
        because this change inserted a line into the same list and the invariant is about
        the LIST, not about who last edited it."""
        for toggles in ({"repos": "F7"}, {}, None):
            with self.subTest(toggles=toggles):
                lines = [ln for ln in _text(toggles=toggles).split("\n") if ln]
                self.assertEqual(lines[-1], overlay.hatch_bind())

    def test_nothing_an_operator_wrote_is_anywhere_in_the_line(self):
        """A `bind` line is format-expanded by tmux, so the containment rule that applies
        to a style applies here: charter's own constants only. The hostile hotkey and the
        hostile toggle are both refused elsewhere; what this asks is that the click bind
        is the same eleven words whatever they were."""
        for hotkey, toggles in (("F2", None),
                                ("M-x", {"repos": "F7"}),
                                ("F2", {"repos": "F7\nkill-server"})):
            with self.subTest(hotkey=hotkey, toggles=toggles):
                text = commands_frame.conf_text(hotkey=hotkey, mouse=True,
                                                history_limit=1, session="fr-1",
                                                toggles=toggles)
                self.assertEqual(
                    _click_line(text),
                    "bind -n MouseDown1Pane if-shell -F -t = '#{@charter_panel}' "
                    "'send-keys -M' 'select-pane -t =; send-keys -M'")

    def test_the_wheel_bind_is_untouched(self):
        """The control. The wheel never moved the keyboard and this change must not give
        it a way to start: a refactor that routed both mouse binds through one builder
        could quietly hand the wheel a `select-pane`."""
        self.assertIn("bind -n WheelUpPane if-shell -F -t = '#{mouse_any_flag}' "
                      "'send-keys -M' 'copy-mode -e; send-keys -M'", _text())


class ThePaneOptionTheBindReads(unittest.TestCase):
    """`_panel_mark_argv` — the write half, which nothing else in the suite covers."""

    def test_it_sets_charters_own_name_to_charters_own_value(self):
        argv = commands_frame._panel_mark_argv(socket="charter-x", pane_id="%11")
        self.assertEqual(argv[-2:], ["@charter_panel", "1"])

    def test_the_write_is_pane_scoped_and_targets_that_pane(self):
        """`-p`, never `-w` and never `-g`. A window write would mark the harness pane
        too, since it shares the window — and a click on the harness would stop selecting
        it. A global write is the "last launched wins" trap `conf_text`'s docstring names,
        reached through an option instead of a session setting."""
        argv = commands_frame._panel_mark_argv(socket="charter-x", pane_id="%11")
        self.assertIn("set-option", argv)
        self.assertIn("-p", argv)
        self.assertNotIn("-w", argv)
        self.assertNotIn("-g", argv)
        self.assertEqual(argv[argv.index("-t") + 1], "%11")

    def test_it_goes_to_the_server_it_was_handed(self):
        """Same rule as every other argv this module builds: charter's private server by
        name, or the operator's own by socket PATH, and whichever one the caller named.

        The path here is a bare absolute one rather than a real tmux socket, because
        `tmuxctl.server_argv` discriminates on the leading `/` alone and
        `tests/test_no_test_bakes_a_uid_into_a_socket_path.py` refuses a `tmux-<uid>` spelt
        into a test file — this case is about the flag, not about where tmux listens."""
        self.assertEqual(
            commands_frame._panel_mark_argv(socket="charter-x", pane_id="%11")[:3],
            ["tmux", "-L", "charter-x"])
        self.assertEqual(
            commands_frame._panel_mark_argv(socket="/nowhere/default",
                                            pane_id="%11")[:3],
            ["tmux", "-S", "/nowhere/default"])

    def test_the_name_the_bind_reads_is_the_name_the_write_sets(self):
        """One constant, two uses — spelled as a literal on both sides here so that a
        rename that updated only one of them fails rather than passing quietly. This is
        the only case in the file allowed to look at the constant at all, and it looks at
        it to compare it against text, not to build one."""
        self.assertEqual(commands_frame._PANEL_OPTION, "@charter_panel")
        self.assertIn("'#{@charter_panel}'", _click_line(_text()))
        self.assertEqual(
            commands_frame._panel_mark_argv(socket="s", pane_id="%1")[-2],
            "@charter_panel")


class NeitherMouseKeyIsAComponentsToTake(unittest.TestCase):
    """#566's hazard, in the mouse table.

    tmux key tables have no notion of a conflict — a later `bind -n` replaces the earlier
    and `list-keys` reads back one line where two were meant. `conf_text` writes both mouse
    binds BEFORE the toggles, so a component claiming either would leave its own key alive
    and charter's mouse handling silently gone: the wheel would stop entering copy-mode,
    and a click on a panel would go back to taking the keyboard off the harness.

    Both names pass `instance._HOTKEY_RE` — they are alphanumerics under twenty characters
    — so this is reachable rather than theoretical, and the guard is what closes it.
    """

    def _resolve(self, key: str) -> dict:
        return instance.frame_of(
            {"frame": {"hotkey": "M-x", "component": [{"use": "identity", "key": key}]}})

    def test_both_names_are_keys_the_alphabet_would_otherwise_allow(self):
        """Without this the two cases below could be passing for the wrong reason — a
        component claiming a key `_HOTKEY_RE` rejects is refused by a different guard
        entirely, and the collision guard would never fire."""
        for key in ("WheelUpPane", "MouseDown1Pane"):
            with self.subTest(key=key):
                self.assertEqual(instance.toggle_key(key), key)

    def test_a_component_may_not_take_the_wheel(self):
        frame = self._resolve("WheelUpPane")
        self.assertEqual(frame["components"], [])
        self.assertEqual(frame["slots"], instance.FRAME_DEFAULTS["slots"])

    def test_a_component_may_not_take_the_click(self):
        frame = self._resolve("MouseDown1Pane")
        self.assertEqual(frame["components"], [])
        self.assertEqual(frame["slots"], instance.FRAME_DEFAULTS["slots"])

    def test_a_key_charter_has_not_taken_is_the_control(self):
        """Without this a `component_tables` that refused every key would pass both cases
        above. `MouseDown2Pane` is one character from a key charter binds and is nobody's."""
        frame = self._resolve("MouseDown2Pane")
        self.assertEqual([p["key"] for p in frame["components"]], ["MouseDown2Pane"])

    def test_a_component_with_no_key_survives_an_arrangement_with_no_hotkey(self):
        """The reserved set holds real keys and nothing else, pinned where it costs
        something.

        `component_tables` builds that set as ``{k for k in (hotkey, HATCH_KEY,
        *MOUSE_KEYS) if k}``, and *hotkey* is ``None`` for every caller resolving an
        arrangement outside a `[frame]` section — this function's own default. The
        collision test is a bare ``key in bound``, so a ``None`` that got into the set
        would match the component that declares **no** toggle key, which is the common
        case, and refuse the whole arrangement: every panel gone because nobody named a
        key.

        The deletion sweep found this exact line unpinned when the guard read ``key is
        not None and key in bound`` — the filter could be deleted with the suite still
        green, because that second condition made it unobservable. This case is what the
        filter now costs to remove.
        """
        got = instance.component_tables({"component": [{"use": "identity"}]})
        self.assertEqual([t["use"] for t in got or []], ["identity"],
                         "a component that declares no key collided with something")

    def test_a_component_with_no_key_still_collides_with_nothing_when_others_do(self):
        """The control beside it: two components, one keyed and one not, both survive —
        so the case above is not passing because keys are ignored altogether."""
        got = instance.component_tables(
            {"component": [{"use": "identity", "key": "F7"}, {"use": "repos"}]},
            hotkey="F2")
        self.assertEqual([t["use"] for t in got or []], ["identity", "repos"])

    def test_the_reserved_names_are_the_ones_actually_bound(self):
        """The drift this reservation exists to prevent, asked directly: every key in
        `tmuxctl.MOUSE_KEYS` must appear as a `bind -n` in `conf_text`'s output, and the
        two spellings are pinned as literals so that renaming the constant alone is red."""
        self.assertEqual(tuple(tmuxctl.MOUSE_KEYS), ("WheelUpPane", "MouseDown1Pane"))
        for key in ("WheelUpPane", "MouseDown1Pane"):
            with self.subTest(key=key):
                self.assertIn(f"bind -n {key} ", _text())


if __name__ == "__main__":  # pragma: no cover
    unittest.main()
