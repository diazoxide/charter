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
from types import SimpleNamespace
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


class _Tmux:
    """A tmux that answers by command shape and refuses anything it was not told about.

    **It raises rather than answering a default**, which is this branch's own lesson: a
    stub that answers the same thing forever turns a deleted guard into a test that hangs
    or passes for the wrong reason. Every unexpected command is a failure with the argv in
    it, so a route that grew a fourth round trip says so.
    """

    def __init__(self, *, place="$4\t@9", clients="", plane=None):
        self.place, self.clients, self.plane = place, clients, plane
        self.calls: list[list[str]] = []

    def run(self, _what, argv, **_kw):
        import subprocess
        self.calls.append(list(argv))
        if "list-clients" in argv:
            return subprocess.CompletedProcess(argv, 0, self.clients, "")
        if "display-message" in argv:
            fmt = argv[-1]
            if "session_id" in fmt and "window_id" in fmt:
                return subprocess.CompletedProcess(argv, 0, self.place, "")
            if fmt == "#{@charter_plane}":
                plane = str(config.STATE_DIR) if self.plane is None else self.plane
                return subprocess.CompletedProcess(argv, 0, plane, "")
        raise AssertionError(f"no answer scripted for {argv}")


class CloseCharterDetachesOnlyThePresser(PersonaIso, unittest.TestCase):
    """*Close charter (keep chats running)* — #1097, and decision 2.

    **Every case here is about an argv that is NOT produced**, because the failure this
    replaces was silent in both directions: on a one-pane chat `detach-client -s default.1`
    resolved to nothing and the palette said *detaching — the harness keeps running*, and
    on the ordinary chat with a strip it resolved to the workspace's session and took every
    attached client with it. So the assertions are *no spawn, and this sentence*, with the
    one positive case first so they cannot all pass against a function that spawns nothing.
    """

    FID = "beta.1"
    A = "/dev/ttys003"

    def setUp(self) -> None:
        super().setUp()
        from charter.frame import state
        state.frame_dir(self.FID, create=True)
        state.record_server(self.FID, "charter-plane-x")
        state.record_harness_pane(self.FID, "%3")

    def _detach(self, tmux, client=A):
        from charter.frame import builtin_actions
        started: list[list[str]] = []
        with mock.patch.object(tmuxctl, "run", side_effect=tmux.run), \
                mock.patch.object(builtin_actions, "_spawn",
                                  side_effect=lambda argv, *, fid: started.append(argv)):
            said = builtin_actions._detach(self.FID, client)
        return started, said

    def test_a_proven_presser_is_detached_alone(self):
        """One listing proves it: the presser's row names the session the chat's HARNESS
        PANE is in, and that session answers this plane. Then, and only then,
        `detach-client -t <client>`."""
        started, said = self._detach(_Tmux(clients=f"{self.A}\t$4\n/dev/ttys004\t$7\n"))
        self.assertEqual(started, [tmuxctl.server_argv("charter-plane-x",
                                                       "detach-client", "-t", self.A)])
        self.assertIn("detaching", said)

    def test_no_presser_detaches_nothing(self):
        """The route charter cannot attribute. An older bind carries no client, and a panel
        whose `@charter_presser` could not be read hands on `""` — neither may fall back to
        every client of the session, which is the whole of decision 2."""
        from charter.frame import builtin_actions
        started, said = self._detach(_Tmux(), client="")
        self.assertEqual(started, [])
        self.assertEqual(said, builtin_actions.NO_PRESSER_TO_DETACH)

    def test_a_client_name_outside_its_shape_is_not_used(self):
        """The value arrives from a tmux format on a server shared with every frame on the
        machine and is about to be a `-t` target. `#{client_name}` unexpanded — what the
        option holds if the click bind ever loses its `-F` — is refused by the same line."""
        from charter.frame import builtin_actions
        for bad in ("x; kill-server", "#{client_name}", "/dev/ttys003 ; kill-server",
                    "../../etc/passwd"):
            with self.subTest(bad=bad):
                started, said = self._detach(_Tmux(), client=bad)
                self.assertEqual(started, [])
                self.assertEqual(said, builtin_actions.NO_PRESSER_TO_DETACH)

    def test_a_presser_attached_to_another_session_is_not_detached(self):
        """The listing is the proof, and it is a proof about WHICH session. A client
        attached to another session of the same server is somebody else's terminal."""
        from charter.frame import builtin_actions
        started, said = self._detach(_Tmux(clients=f"{self.A}\t$7\n"))
        self.assertEqual(started, [])
        self.assertEqual(said, builtin_actions.NOT_ATTACHED_HERE)

    def test_a_presser_the_listing_does_not_name_is_not_detached(self):
        """A server that did not answer proves nothing (#1100): an empty listing is not
        `no clients`, it is `no reading`, and both refuse here."""
        from charter.frame import builtin_actions
        started, said = self._detach(_Tmux(clients=""))
        self.assertEqual(started, [])
        self.assertEqual(said, builtin_actions.NOT_ATTACHED_HERE)

    def test_a_session_of_another_plane_is_not_detached(self):
        """The plane marker, which is the guard #933 put on every other write charter aims
        at a pane it found. Two planes can each have a `beta.1` on one server."""
        from charter.frame import builtin_actions
        started, said = self._detach(_Tmux(clients=f"{self.A}\t$4\n", plane="/other"))
        self.assertEqual(started, [])
        self.assertEqual(said, builtin_actions.NOT_ATTACHED_HERE)

    def test_an_unmarked_session_is_not_detached(self):
        """A session an older charter created carries no marker, and an unmarked pane is
        not proven — never adopted."""
        from charter.frame import builtin_actions
        started, said = self._detach(_Tmux(clients=f"{self.A}\t$4\n", plane=""))
        self.assertEqual(started, [])
        self.assertEqual(said, builtin_actions.NOT_ATTACHED_HERE)

    def test_a_chat_whose_pane_tmux_cannot_place_is_not_detached(self):
        """`_pane_place` answers `None` for a record charter has lost or for a target tmux
        would not resolve — and an unresolvable `display-message -p -t` answers rc 0 with
        empty stdout, so the record alone can never be the target."""
        from charter.frame import builtin_actions
        started, said = self._detach(_Tmux(place=""))
        self.assertEqual(started, [])
        self.assertEqual(said, builtin_actions.NOT_ATTACHED_HERE)

    def test_no_route_ever_detaches_a_whole_session(self):
        """Across every case above: no argv charter builds here carries `-s`, and none
        carries a chat id. Red at `c846740`, where the only argv was
        `detach-client -s beta.1` — which on a chat carrying a strip detaches every client
        of the workspace (Step 0's G4)."""
        cases = [(_Tmux(clients=f"{self.A}\t$4\n"), self.A),
                 (_Tmux(), ""),
                 (_Tmux(clients=f"{self.A}\t$7\n"), self.A),
                 (_Tmux(clients=f"{self.A}\t$4\n", plane="/other"), self.A),
                 (_Tmux(place=""), self.A)]
        for tmux, client in cases:
            started, _said = self._detach(tmux, client=client)
            for argv in started + tmux.calls:
                self.assertNotIn("-s", argv)
                self.assertNotIn(self.FID, argv)

    def test_the_f2_detach_row_detaches_the_hotkeys_presser(self):
        """`F2 → Close charter (keep chats running)` is the same act by the same code, with
        the presser the hotkey's bind carried — which is what closes #1097 on the route the
        issue was filed against."""
        from charter.frame import builtin_actions
        started: list[list[str]] = []
        reg = builtin_actions.build(self.FID, current_density="normal",
                                    current_chrome="off", client=self.A)
        tmux = _Tmux(clients=f"{self.A}\t$4\n")
        with mock.patch.object(tmuxctl, "run", side_effect=tmux.run), \
                mock.patch.object(builtin_actions, "_spawn",
                                  side_effect=lambda argv, *, fid: started.append(argv)):
            reg.get("frame.detach").run(SimpleNamespace(fid=self.FID))
        self.assertEqual(started, [tmuxctl.server_argv("charter-plane-x",
                                                       "detach-client", "-t", self.A)])

    def test_the_f2_detach_row_is_refused_without_a_presser(self):
        """#512: the row stays, listed with the reason, because an option you cannot see is
        one you cannot ask about — and the reason names the gesture that works."""
        from charter.frame import builtin_actions
        reg = builtin_actions.build(self.FID, current_density="normal",
                                    current_chrome="off", client="")
        offer = {o.id: o for o in reg.offers(fid=self.FID, snapshot={})}["frame.detach"]
        self.assertFalse(offer.available)
        self.assertEqual(offer.reason, builtin_actions.NO_PRESSER_TO_DETACH)

    def test_the_row_is_available_once_there_is_a_presser(self):
        """The control for the case above: without it, a `build` that refused the row
        unconditionally would pass there and take *Close charter* away everywhere."""
        from charter.frame import builtin_actions
        reg = builtin_actions.build(self.FID, current_density="normal",
                                    current_chrome="off", client=self.A)
        offer = {o.id: o for o in reg.offers(fid=self.FID, snapshot={})}["frame.detach"]
        self.assertTrue(offer.available)

    def test_the_operators_tmux_row_is_refused_with_its_reason(self):
        """Decision 7 at the `F2` row: inside the operator's own tmux a frame is a WINDOW,
        their prefix key is what detaches, and charter says so rather than offering a row
        that would detach a client charter has no business detaching.

        Asserted against the presser being present, so the operator-tmux sentence wins over
        the no-presser one rather than the two racing on whichever is checked first.
        """
        from charter.frame import builtin_actions
        from charter.frame import state
        from tests import _tmuxsocket
        state.record_server(self.FID, _tmuxsocket.OPERATOR_SOCKET)
        reg = builtin_actions.build(self.FID, current_density="normal",
                                    current_chrome="off", client=self.A)
        offer = {o.id: o for o in reg.offers(fid=self.FID, snapshot={})}["frame.detach"]
        self.assertFalse(offer.available)
        self.assertIn("your own prefix key", offer.reason)

    def test_the_row_is_titled_in_the_gates_words(self):
        """One spelling for one act. `F10`'s first row and `F2`'s detach row are the same
        thing, and two sentences about it drift the first time either is edited."""
        from charter.frame import builtin_actions
        reg = builtin_actions.build(self.FID, current_density="normal",
                                    current_chrome="off", client=self.A)
        self.assertEqual(reg.get("frame.detach").title, gate.DETACH_TITLE)
