"""One gate out of the frame: `F10`, the identity row's button, and the same two `F2`
rows — each detaching only the terminal that asked (#1115, closing #1097).

**What the four routes have in common is the PRESSER, and that is the whole of this
file.** `F10`, the hotkey, a click on the gate button and a click on any other door each
have to answer *which terminal asked*, because *Close charter (keep chats running)* means
detach that one and leave every other client attached. #729 removed `#{client_name}` from
the hotkey's bind when its one consumer went away; this feature is a new consumer, so the
value comes back — on the key binds as a format tmux expands at the keypress, and on the
pointer route as a pane option the click bind writes with `set-option -F`.

**Step 0's G3 is what let this file be written at all.** Measured by hand on tmux 3.7c and
at the 3.2 floor, two clients attached to one session over two ptys, six alternating SGR
presses on a marked panel pane: the panel's `@charter_presser` read back as the CLICKING
client's name every time, never the other client's and never the literal text
`#{client_name}`. Had it not, the ruling was that this task stops rather than falling back
to detaching everybody.

**And G4 is why #1097 is worse than its own title says.** `detach-client -s <chat id>` on
a chat whose window carries a panel does not fail: tmux parses `default.1` as
`session.pane`, pane index 1 exists the moment charter splits a strip off the harness, and
the command resolves to the WORKSPACE's session and detaches **every client attached to
it**. Measured on both versions — `has-session -t solo.1` answers `can't find pane: 1` on a
one-pane window and rc 0 on a two-pane one, and `detach-client -s default.1` left zero of
two clients attached. So the row that promised to leave the harness running took the other
operator's terminal with it, silently, and the fix is not "make the target resolve" but
"never name a session at all".
"""

from __future__ import annotations

import os
import unittest
from unittest import mock

from charter import commands_frame, config, instance
from charter.frame import gate, overlay, tmuxctl
from tests._isolation import PersonaIso


def _text(*, hotkey: str = "F2", mouse: bool = True, toggles=None) -> str:
    return commands_frame.conf_text(hotkey=hotkey, mouse=mouse, history_limit=1,
                                    session="fr-1", toggles=toggles)


def _line(text: str, key: str) -> str:
    """The one `bind -n <key>` line of *text*, or `""`."""
    want = f"bind -n {key} "
    return next((ln for ln in text.split("\n") if ln.startswith(want)), "")


class TheBindsCarryThePresser(PersonaIso, unittest.TestCase):
    """`conf_text`'s three bind lines, as literals.

    Literals rather than pieces reassembled out of the same constants the code assembles
    them from: what Step 0's G5 `source-file`d and `list-keys` read back byte for byte was
    a STRING, and a test that composed it from `gate.GATE_KEY` and `_CHAT_OPTION` would
    agree with whatever those happened to hold.
    """

    def test_the_hotkey_bind_passes_client_name_again(self):
        """#729's removal is undone, in the positional `cli.py` never stopped accepting.

        Red at `c846740`, where the bind carries `--chat` and nothing else — which is a
        palette that cannot say which of two attached terminals pressed the key, and so a
        *Close charter* row that could only detach both or neither.
        """
        self.assertIn(
            "bind -n F2 run-shell '\"$CHARTER_PY\" -m charter frame-palette "
            "\"#{client_name}\" --chat \"#{@charter_chat}\"'",
            _text())

    def test_f10_opens_the_gate_with_the_presser(self):
        self.assertIn(
            "bind -n F10 run-shell '\"$CHARTER_PY\" -m charter frame-palette "
            "\"#{client_name}\" --gate --chat \"#{@charter_chat}\"'",
            _text())

    def test_a_click_on_a_panel_records_its_presser(self):
        """The panel branch writes the presser BEFORE forwarding, and the other branch is
        untouched.

        Both halves asserted, because a change that dropped the `select-pane` from the
        else-branch would restore #634's focus steal while every assertion about the
        presser still passed.
        """
        line = _line(_text(), tmuxctl.CLICK_KEY)
        self.assertEqual(
            line,
            "bind -n MouseDown1Pane if-shell -F -t = '#{@charter_panel}' "
            "'set-option -F -p -t = @charter_presser \"#{client_name}\" ; send-keys -M' "
            "'select-pane -t =; send-keys -M'")
        self.assertEqual(line.split("'")[5], "select-pane -t =; send-keys -M")

    def test_the_presser_is_written_with_dash_f_so_tmux_expands_it(self):
        """`-F` is the difference between the option holding `/dev/ttys003` and holding
        the eighteen characters `#{client_name}` (the spec's ruling, G3's pass condition).

        Its own case rather than a clause of the literal above, because that is the half a
        reader of the bind cannot see is load-bearing — and `gate.CLIENT_RE` refuses the
        unexpanded text, so without `-F` every click would reach the gate as *no presser*
        and the detach row would be listed refused forever.
        """
        branch = _line(_text(), tmuxctl.CLICK_KEY).split("'")[3]
        self.assertTrue(branch.startswith("set-option -F -p -t = "), branch)
        self.assertIn(f'{gate.PRESSER_OPTION} "#{{client_name}}"', branch)
        self.assertTrue(branch.endswith("; send-keys -M"), branch)

    def test_every_bind_is_a_constant_and_the_hatch_stays_last(self):
        """Two different sessions, two different hotkeys and a hostile toggle produce the
        same three lines — charter interpolates nothing of an operator's into a `bind`,
        which is `_HOTKEY_RE`'s own incident read one surface over. And the escape hatch is
        still the last non-empty line, which is `conf_text`'s standing invariant about the
        LIST rather than about whoever last edited it.
        """
        first = _text()
        for hotkey, session, toggles in (("F2", "fr-2", None),
                                         ("M-x", "fr-3", {"repos": "F7"}),
                                         ("F2", "fr-4", {"repos": "F7\nkill-server"})):
            with self.subTest(hotkey=hotkey, session=session):
                text = commands_frame.conf_text(hotkey=hotkey, mouse=True,
                                                history_limit=1, session=session,
                                                toggles=toggles)
                self.assertEqual(_line(text, gate.GATE_KEY),
                                 _line(first, gate.GATE_KEY))
                self.assertEqual(_line(text, tmuxctl.CLICK_KEY),
                                 _line(first, tmuxctl.CLICK_KEY))
                lines = [ln for ln in text.split("\n") if ln]
                self.assertEqual(lines[-1], overlay.hatch_bind())

    def test_the_gate_bind_sits_between_the_palette_and_the_hatch(self):
        """`conf_text`'s two ends, for the reasons it already argues: after the hotkey so a
        component that claimed the key would visibly steal something (which is what keeps
        `instance.component_arrangement`'s refusal pinnable), and before `hatch_bind()` so
        a tmux below `tmuxctl.FLOOR`, which cannot parse `run-shell -C`, has already
        applied this line by the time it drops that one."""
        lines = _text(toggles={"repos": "F7"}).split("\n")
        hotkey = next(i for i, ln in enumerate(lines) if ln.startswith("bind -n F2 "))
        gate_at = next(i for i, ln in enumerate(lines)
                       if ln.startswith(f"bind -n {gate.GATE_KEY} "))
        hatch = next(i for i, ln in enumerate(lines) if ln == overlay.hatch_bind())
        self.assertLess(hotkey, gate_at)
        self.assertLess(gate_at, hatch)

    def test_the_gate_is_bound_whatever_the_mouse_setting_says(self):
        """`[frame] mouse` is off on most planes, and the key is the primary way in
        precisely there. Gating this on the flag would leave the gate reachable only by
        pointer on exactly the planes that have no pointer."""
        for mouse in (True, False):
            with self.subTest(mouse=mouse):
                self.assertTrue(_line(_text(mouse=mouse), gate.GATE_KEY))

    def test_the_operators_tmux_gets_no_bind(self):
        """Decision 7, pinned at the launch path that builds a frame inside the tmux the
        operator already has: charter writes no key there, so `F10` is theirs.

        A pin, and the positive half is the case above: the private-server path DOES emit
        the line, so an implementation that emitted nothing anywhere would fail there
        rather than pass here.
        """
        seen: list[list[str]] = []

        def answer(cmd, **kw):
            seen.append(list(cmd))
            return _completed(cmd, 0, "%7\n")

        from charter.frame import launcher
        # The repo gather is a real detached charter and is not what this asks about.
        self.enterContext(mock.patch.object(commands_frame, "_spawn_gather"))
        with mock.patch("charter.commands_frame.subprocess.run", side_effect=answer), \
                mock.patch.object(commands_frame, "_wait_for_harness",
                                  return_value=130), \
                mock.patch.dict(os.environ, {"CHARTER_HARNESS": "codex"}):
            commands_frame._launch_in_operator_tmux(
                "op", "$1", ws="beta", argv=launcher.argv_select("claude"),
                display=[commands_frame.SELECTOR_DISPLAY], profile="", h=None,
                v=(3, 7), picked=False, selecting=True)
        flat = [" ".join(c) for c in seen]
        self.assertTrue(flat, "the launch ran no tmux command, so this asserts nothing")
        for one in flat:
            self.assertNotIn("bind", one)
            self.assertNotIn(gate.GATE_KEY, one)


def _completed(cmd, code, out=""):
    import subprocess
    return subprocess.CompletedProcess(cmd, code, out, "")


class TheKeyIsReserved(PersonaIso, unittest.TestCase):
    """`F10` joins the keys charter has already bound for a frame.

    A `bind -n` is server-wide and tmux has no notion of a conflict — the later line simply
    replaces the earlier — so a component or a `[frame] hotkey` that took this key would
    take the gate away from every frame on the socket with nothing anywhere saying so.
    """

    def test_a_component_may_not_claim_it(self):
        placed, why = instance.component_arrangement(
            {"component": [{"use": "identity", "key": gate.GATE_KEY}]}, hotkey="F2")
        self.assertIsNone(placed)
        self.assertIn("already bound", why)

    def test_the_reason_names_the_close_menu(self):
        """#512's rule: the sentence has to say WHAT took the key, or the operator's next
        move is to guess. Red with the pre-gate wording, which listed the palette, the
        hatch and the mouse keys and would have read as a lie about `F10`."""
        _placed, why = instance.component_arrangement(
            {"component": [{"use": "identity", "key": gate.GATE_KEY}]}, hotkey="F2")
        self.assertIn("close menu", why)

    def test_a_frame_hotkey_equal_to_it_falls_back_to_f2(self):
        """`[frame] hotkey = "F10"` is refused the way an unusable key is, and the shipped
        `F2` binds instead — because a hotkey that took this key would make `F2` and `F10`
        the same surface and leave the gate with no key of its own."""
        out = instance.frame_of({"frame": {"hotkey": gate.GATE_KEY}})
        self.assertEqual(out["hotkey"], "F2")

    def test_a_hotkey_charter_accepts_is_still_taken(self):
        """The control. Without it the case above passes against an `instance` that
        refused every hotkey, which is the whole setting deleted rather than one value."""
        self.assertEqual(instance.frame_of({"frame": {"hotkey": "M-x"}})["hotkey"], "M-x")

    def test_the_shipped_default_is_not_the_gate_key(self):
        """A pin on the two constants staying apart: `config.FRAME['hotkey']` is what
        `conf_text` binds for the palette and `gate.GATE_KEY` is what it binds for the
        gate, and the same value in both would emit one bind twice."""
        self.assertNotEqual(config.FRAME["hotkey"], gate.GATE_KEY)
