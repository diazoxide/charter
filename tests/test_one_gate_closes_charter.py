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


def _doomed(**kw):
    """One chat a quit would stop, built straight rather than off disk.

    `tests/test_what_a_quit_says_is_spelled_where_it_is_asserted.py`'s own fixture, for its
    reason: what the gate's doorway is asked here is that it hands `leave.confirm_rows` the
    plan and draws what comes back, not that `leave.plan` reads a frame root correctly —
    which is that file's question and is answered there.
    """
    from charter.frame import leave
    base = dict(chat="beta.1", workspace="beta", persona="", harness="claude-code",
                cwd="/tmp", resume="", server="charter-plane-x", live=True, active=False,
                exit_code=None, closed=False, homeless=False, cwd_gone=False,
                cwd_outside=False, conversation=os.path.abspath(__file__))
    base.update(kw)
    return leave.Doomed(**base)


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

    def test_the_menu_row_and_the_f2_row_are_the_same_act(self):
        """One spelling and one implementation. The gate's first row dispatches into
        exactly the function the `F2` row's `run` calls, so the two cannot come to disagree
        about what *Close charter* does — which is the whole reason `gate.DETACH_TITLE` is
        read by both rather than spelled twice."""
        from charter.frame import builtin_actions
        said: list[str] = []
        tmux = _Tmux(clients=f"{self.A}\t$4\n")
        started: list[list[str]] = []
        with mock.patch.object(tmuxctl, "run", side_effect=tmux.run), \
                mock.patch.object(commands_frame, "_say_on_screen",
                                  side_effect=lambda fid, msg, **kw: said.append(msg)), \
                mock.patch.object(builtin_actions, "_spawn",
                                  side_effect=lambda argv, *, fid: started.append(argv)):
            gate.chose(overlay.Row(id=gate.DETACH_ID, title=gate.DETACH_TITLE),
                       self.FID, client=self.A)
        self.assertEqual(started, [tmuxctl.server_argv("charter-plane-x",
                                                       "detach-client", "-t", self.A)])
        self.assertEqual(said, ["detaching — the harness keeps running"])

    def test_the_row_is_titled_in_the_gates_words(self):
        """One spelling for one act. `F10`'s first row and `F2`'s detach row are the same
        thing, and two sentences about it drift the first time either is edited."""
        from charter.frame import builtin_actions
        reg = builtin_actions.build(self.FID, current_density="normal",
                                    current_chrome="off", client=self.A)
        self.assertEqual(reg.get("frame.detach").title, gate.DETACH_TITLE)


class TheMenu(PersonaIso, unittest.TestCase):
    """`gate.draw` and the two rows it draws — `frame/tabmenu.py`'s shape, one surface over.

    **What is different from every other palette in the frame is the cursor**, and that is
    the one thing worth a class of its own: this surface's first row can be refused, and
    `palette.aim` answers *the first row that can run*, which here is *stop all chats*. So
    the cursor is pinned by id and the aim rule is deliberately not used.
    """

    FID = "beta.1"
    A = "/dev/ttys003"

    def setUp(self) -> None:
        super().setUp()
        from charter.frame import state
        state.frame_dir(self.FID, create=True)
        state.record_server(self.FID, "charter-plane-x")
        state.record_harness_pane(self.FID, "%3")
        state.record_workspace(self.FID, "beta")

    def test_two_rows_detach_first(self):
        rows = gate.catalogue(self.FID, client=self.A)
        self.assertEqual([r.id for r in rows], ["gate:detach", "gate:stop"])
        self.assertEqual([r.title for r in rows],
                         ["Close charter (keep chats running)",
                          "Close charter and stop all chats…"])

    def test_the_cursor_opens_on_close_charter_when_it_can_run(self):
        surface = gate.Gate(catalogue=gate.catalogue(self.FID, client=self.A),
                            label=gate.LABEL, mouse=True)
        self.assertEqual(surface.selected.id, gate.DETACH_ID)

    def test_the_cursor_never_opens_on_stop_even_when_detach_is_refused(self):
        """**Red with `palette.aim`**, which is the rule every other surface in the frame
        wants: with the first row refused it opens on the first that can run, and here that
        is the row that stops every harness on the plane. Enter on the refused row answers
        with its note instead, which is the whole trade — one keypress that says why,
        rather than one that ends every chat."""
        from charter.frame import builtin_actions
        surface = gate.Gate(catalogue=gate.catalogue(self.FID, client=""),
                            label=gate.LABEL, mouse=True)
        row = surface.selected
        self.assertEqual(row.id, gate.DETACH_ID)
        self.assertTrue(row.refused)
        self.assertEqual(row.note, builtin_actions.NO_PRESSER_TO_DETACH)
        self.assertFalse(gate.chose(row, self.FID, client=""))

    def test_the_refused_row_is_listed_and_not_dropped(self):
        """#512: an option you cannot see is one you cannot ask about — and dropping it
        would also put *stop all chats* under the cursor by being the only row left."""
        rows = gate.catalogue(self.FID, client="")
        self.assertEqual([r.id for r in rows], ["gate:detach", "gate:stop"])

    def test_the_cursor_follows_a_query_the_operator_typed(self):
        """The pin is about where the surface OPENS, not a cursor nailed down: an operator
        who types toward the stop row has asked for it, and `narrow` is what answers. Its
        own case, because a `_refilter` that forced the index whatever the rows were would
        leave the cursor on a row that is no longer on screen."""
        surface = gate.Gate(catalogue=gate.catalogue(self.FID, client=self.A),
                            label=gate.LABEL, mouse=True)
        surface.query = "stop all"
        surface._refilter()
        self.assertEqual(surface.selected.id, gate.STOP_ID)

    def test_stop_all_chats_opens_the_quit_confirmation_listing_every_chat(self):
        """The doorway opens `leave`'s own warning, not a second enumeration of it — so
        `F10 → stop all chats` and `F2 → charter: quit` describe the plane in the same
        words, ended tabs included (decision 12)."""
        from charter.frame import leave
        live = {"beta.1", "beta.2"}
        surface = gate.opens(overlay.Row(id=gate.STOP_ID, title=gate.STOP_TITLE),
                             self.FID, live=live)
        self.assertIsNotNone(surface)
        self.assertEqual(surface.label, leave.QUIT)

    def test_the_confirmation_lists_an_ended_tab_with_what_it_gets_back(self):
        """Built from a plan directly, because what is asserted is that the gate hands
        `leave.confirm_rows` the rows it makes rather than composing its own."""
        from charter.frame import leave
        rows = leave.confirm_rows(
            leave.Plan(chats=(_doomed(chat="beta.1"),
                              _doomed(chat="beta.2"),
                              _doomed(chat="beta.3", exit_code=0, ended=True)),
                       focus="beta"),
            verb=leave.QUIT)
        chat_rows = [r for r in rows if r.refused]
        self.assertEqual(len(chat_rows), 3)
        self.assertTrue(any("comes back ended" in r.note for r in chat_rows))

    def test_the_confirming_row_runs_frame_quit(self):
        from charter.frame import builtin_actions, leave
        go = leave.confirm_rows(leave.Plan(chats=(_doomed(chat="beta.1"),), focus="beta"),
                                verb=leave.QUIT)[0]
        started: list[list[str]] = []
        with mock.patch.object(builtin_actions, "_spawn",
                               side_effect=lambda argv, *, fid: started.append(argv)):
            self.assertTrue(gate.chose(go, self.FID, client=""))
        self.assertEqual(len(started), 1)
        self.assertEqual(started[0][-3:], ["frame-quit", "--chat", self.FID])

    def test_the_menu_is_not_drawn_inside_the_operators_tmux(self):
        """Decision 7. Charter binds no key and draws no button there, so nothing opens
        this surface by a gesture — and a `--gate` typed by hand says where the rows are
        rather than drawing a menu whose first row is refused on every plane."""
        from charter.frame import palette as palette_mod
        from charter.frame import state
        from tests import _tmuxsocket
        state.record_server(self.FID, _tmuxsocket.OPERATOR_SOCKET)
        said, closed = [], []
        with mock.patch.dict(os.environ, {"CHARTER_SESSION_ID": self.FID,
                                          "TMUX_PANE": "%9"}, clear=True), \
                mock.patch.object(palette_mod, "own_the_tty") as drew, \
                mock.patch.object(commands_frame, "_say_on_screen",
                                  side_effect=lambda fid, msg, **kw: said.append(msg)), \
                mock.patch.object(commands_frame, "_close_palette",
                                  side_effect=lambda *a, **kw: closed.append(kw)):
            self.assertEqual(gate.draw(SimpleNamespace(client=self.A, gate=True)), 0)
        drew.assert_not_called()
        self.assertEqual(said, [gate.NOT_HERE])
        self.assertIn("F2's rows carry it", gate.NOT_HERE)
        self.assertEqual(len(closed), 1, "the harness never got its pane back")

    def test_the_menu_is_drawn_on_charters_own_server(self):
        """The control for the case above: without it, a `draw` that refused everywhere
        would pass there and the gate would exist nowhere."""
        from charter.frame import palette as palette_mod
        with mock.patch.dict(os.environ, {"CHARTER_SESSION_ID": self.FID,
                                          "TMUX_PANE": "%9"}, clear=True), \
                mock.patch.object(palette_mod, "own_the_tty",
                                  return_value=None) as drew, \
                mock.patch.object(commands_frame, "_say_on_screen"), \
                mock.patch.object(commands_frame, "_close_palette"):
            self.assertEqual(gate.draw(SimpleNamespace(client=self.A, gate=True)), 0)
        drew.assert_called_once()
        self.assertEqual([r.id for r in drew.call_args[0][0].rows],
                         [gate.DETACH_ID, gate.STOP_ID])

    def test_the_menu_always_returns_zero_and_gives_the_harness_back(self):
        """`cmd_palette`'s rule: a non-zero `run-shell` child is printed INTO THE HARNESS
        PANE and drops it into copy-mode, which is charter drawing in the one rectangle ADR
        0018 says it never draws. And the close is a `finally`, one layer up from
        `Surface.run`'s own."""
        from charter.frame import palette as palette_mod
        closed = []
        with mock.patch.dict(os.environ, {"CHARTER_SESSION_ID": self.FID,
                                          "TMUX_PANE": "%9"}, clear=True), \
                mock.patch.object(palette_mod, "own_the_tty",
                                  side_effect=RuntimeError("boom")), \
                mock.patch.object(commands_frame, "_close_palette",
                                  side_effect=lambda *a, **kw: closed.append(kw)):
            with self.assertRaises(RuntimeError):
                gate.draw(SimpleNamespace(client=self.A, gate=True))
        self.assertEqual(len(closed), 1)

    def test_a_cancel_starts_nothing_and_says_nothing(self):
        """`own_the_tty` answers `None` for Escape, for the hatch and for a pane whose
        writer is gone — the commonest way this surface ends, and the one answer that can
        never become a wedge."""
        from charter.frame import builtin_actions
        from charter.frame import palette as palette_mod
        with mock.patch.dict(os.environ, {"CHARTER_SESSION_ID": self.FID,
                                          "TMUX_PANE": "%9"}, clear=True), \
                mock.patch.object(palette_mod, "own_the_tty", return_value=None), \
                mock.patch.object(commands_frame, "_say_on_screen") as said, \
                mock.patch.object(builtin_actions, "_spawn") as spawn, \
                mock.patch.object(commands_frame, "_close_palette"):
            self.assertEqual(gate.draw(SimpleNamespace(client=self.A, gate=True)), 0)
        said.assert_not_called()
        spawn.assert_not_called()


class TheGateIsTheSamePaneAsThePalette(PersonaIso, unittest.TestCase):
    """`--gate` routing: one command, one pane, one sweep, one hatch — five surfaces."""

    FID = "beta.1"

    def setUp(self) -> None:
        super().setUp()
        from charter.frame import state
        state.frame_dir(self.FID, create=True)
        state.record_server(self.FID, "charter-plane-x")
        state.record_harness_pane(self.FID, "%3")

    def test_the_flag_makes_this_pane_the_gate(self):
        with mock.patch.object(gate, "draw", return_value=0) as drew, \
                mock.patch.object(commands_frame, "_draw_palette") as ordinary, \
                mock.patch.object(commands_frame.pane, "claim", return_value=None), \
                mock.patch.object(commands_frame.pane, "release"):
            commands_frame.cmd_palette(
                SimpleNamespace(pane=True, gate=True, tab="", ended=False, client=""))
        drew.assert_called_once()
        ordinary.assert_not_called()

    def test_without_the_flag_it_is_the_ordinary_palette(self):
        with mock.patch.object(gate, "draw") as drew, \
                mock.patch.object(commands_frame, "_draw_palette",
                                  return_value=0) as ordinary, \
                mock.patch.object(commands_frame.pane, "claim", return_value=None), \
                mock.patch.object(commands_frame.pane, "release"):
            commands_frame.cmd_palette(
                SimpleNamespace(pane=True, gate=False, tab="", ended=False, client=""))
        drew.assert_not_called()
        ordinary.assert_called_once()

    def test_the_pane_it_carves_is_told_the_presser_and_the_flag(self):
        """The presser has to survive the trip, because the pane is a SECOND process: the
        key bind's `#{client_name}` was expanded in the `run-shell` child, and the surface
        that acts on it is the one inside the split."""
        argvs = self._opened(SimpleNamespace(client="/dev/ttys003", gate=True, chat=""))
        self.assertIn("/dev/ttys003", argvs)
        self.assertIn(gate.OPTION, argvs)
        self.assertLess(argvs.index("/dev/ttys003"), argvs.index(gate.OPTION))

    def test_an_f2_carries_the_presser_and_no_flag(self):
        argvs = self._opened(SimpleNamespace(client="/dev/ttys003", gate=False, chat=""))
        self.assertIn("/dev/ttys003", argvs)
        self.assertNotIn(gate.OPTION, argvs)

    def test_a_presser_charter_cannot_read_rides_nothing_at_all(self):
        """The empty tuple is load-bearing, for `tabmenu.forward`'s reason: an argv holding
        an empty positional would be a palette whose `client` is `""` by a different route,
        and the `F2` argv of a charter with no presser stays byte-identical to what it was.
        """
        for bad in ("", "x; kill-server", "#{client_name}"):
            with self.subTest(bad=bad):
                argvs = self._opened(SimpleNamespace(client=bad, gate=False, chat=""))
                self.assertNotIn("", argvs)
                self.assertNotIn(bad, argvs[1:] if bad else argvs)

    def _opened(self, args) -> list[str]:
        """The argv `_open_palette` gives the pane it carves."""
        seen: list[list[str]] = []
        import subprocess
        with mock.patch.object(tmuxctl, "version", return_value=(3, 7)), \
                mock.patch.object(commands_frame, "_close_open_overlays"), \
                mock.patch.object(
                    tmuxctl, "run",
                    side_effect=lambda _w, argv, **kw: (
                        seen.append(list(argv))
                        or subprocess.CompletedProcess(argv, 0, "%9\n", ""))), \
                mock.patch.dict(os.environ, {"CHARTER_SESSION_ID": self.FID},
                                clear=True):
            commands_frame._open_palette(args)
        split = next(a for a in seen if "split-window" in a)
        return split[split.index("--") + 1:]


class ThePaletteActsForThePresserToo(PersonaIso, unittest.TestCase):
    """`F2`'s own pane reads the positional it has been handed since #1115.

    Its own class because the value makes a different trip on this route: the bind expands
    it, the `run-shell` child forwards it onto the split's argv, and the process INSIDE the
    pane is the one that builds the registry.
    """

    FID = "beta.1"

    def setUp(self) -> None:
        super().setUp()
        from charter.frame import state
        state.frame_dir(self.FID, create=True)
        state.record_server(self.FID, "charter-plane-x")
        state.record_harness_pane(self.FID, "%3")
        state.record_identity(self.FID, {"CHARTER_WORKSPACE": "", "CHARTER_PERSONA": ""})

    def _detach_row(self, client):
        from charter.frame import builtin_actions
        from charter.frame import palette as palette_mod
        built = {}
        real = builtin_actions.build

        def spy(fid, **kw):
            built.update(kw)
            return real(fid, **kw)

        with mock.patch.dict(os.environ, {"CHARTER_SESSION_ID": self.FID}, clear=True), \
                mock.patch.object(builtin_actions, "build", side_effect=spy), \
                mock.patch.object(palette_mod, "own_the_tty", return_value=None), \
                mock.patch.object(commands_frame, "_close_palette"):
            commands_frame._draw_palette(SimpleNamespace(client=client, pane=True,
                                                         gate=False, tab="",
                                                         ended=False))
        return built

    def test_the_palette_builds_its_rows_for_the_presser(self):
        self.assertEqual(self._detach_row("/dev/ttys003")["client"], "/dev/ttys003")

    def test_a_palette_with_no_presser_builds_for_none(self):
        """The control, and the ordinary case on a charter whose installed bind predates
        the value: `build` is handed `""`, the detach row is listed refused with its reason,
        and every other row is what it was."""
        self.assertEqual(self._detach_row("")["client"], "")


class TheButton(PersonaIso, unittest.TestCase):
    """`F10 close` at the right end of the identity row — the pointer's way to the gate.

    **With `[frame] mouse` off, which is the default, it is a label that teaches the key**,
    and that is not a consolation: the complaint #751 answered is that a key name beside a
    noun reads as a button everywhere else an operator has seen one, and the answer there
    was to make it one. This row now says where the exit is whether or not it is clickable.
    """

    FID = "beta.1"

    def setUp(self) -> None:
        super().setUp()
        from charter.frame import slots, state
        state.frame_dir(self.FID, create=True)
        state.record_server(self.FID, "charter-plane-x")
        state.record_workspace(self.FID, "beta")
        self.addCleanup(slots.DOORS.forget)

    def _top(self, *, cols=120, rows=24) -> str:
        return self._top_and_width(cols=cols, rows=rows)[0]

    def _top_and_width(self, *, cols=120, rows=24):
        """The row as drawn, and the canvas width it was drawn for.

        Both, because a door is published at a CANVAS column and the rendered row is
        right-trimmed — so a test that worked the button's column out of the string it can
        see would be measuring the pad rather than the cell.
        """
        import sys
        from charter import tui
        from charter.frame import slots
        with mock.patch("os.get_terminal_size",
                        return_value=os.terminal_size((cols, rows))), \
                mock.patch.object(sys.stdout, "fileno", return_value=1, create=True):
            return (tui.strip_ansi(slots.render("top", self.FID)),
                    slots.content_width("top"))

    def test_the_button_is_the_right_hand_end_of_the_row(self):
        """The last thing on the row, after the version. `rstrip` because the row is
        right-trimmed on its way out — the cell keeps its trailing column, and the pad is
        not something a test gets to assert about."""
        from charter.frame import slots
        row = self._top()
        self.assertTrue(row.rstrip().endswith(slots.GATE_BUTTON), row)

    def test_its_columns_open_the_gate_and_not_the_palette(self):
        """Two column sets rather than one, which is the part of `_Doors`' own docstring
        that stopped holding: it argued against a mapping *"whose values nothing branches
        on"*, and something branches on the value now."""
        from charter import tui
        from charter.frame import slots
        row, w = self._top_and_width()
        for at in range(w - tui.width(f" {slots.GATE_BUTTON} ") + 1, w - 1):
            self.assertTrue(slots.DOORS.opens_gate(at), f"{at} of {w}: {row!r}")
            self.assertFalse(slots.DOORS.opens_palette(at), f"{at} of {w}: {row!r}")

    def test_the_workspace_chip_is_still_the_palettes_and_not_the_gates(self):
        """The reverse, so the two sets cannot pass by being one set under two names."""
        from charter.frame import slots
        self._top()
        self.assertTrue(slots.DOORS.opens_palette(1))
        self.assertFalse(slots.DOORS.opens_gate(1))

    def test_a_starved_row_drops_the_version_before_the_button(self):
        """The ladder's order, and it is the operator's: the version is a fact about the
        install and the button is the way out. A row too narrow for both keeps the way out.
        """
        from charter import __version__
        wide, narrow = self._top(cols=120), self._top(cols=30)
        self.assertIn(__version__, wide)
        self.assertNotIn(__version__, narrow)
        self.assertIn("F10 close", narrow)

    def test_the_button_stays_at_terse(self):
        """`terse` buys back a line by dropping the version — the one field on this row
        that reads the same on every frame on the machine all day. It does not buy it back
        by removing the way out."""
        from charter.frame import state
        from charter import __version__
        state.record_density(self.FID, "minimal")
        row = self._top()
        self.assertNotIn(__version__, row)
        self.assertIn("F10 close", row)

    def test_an_identity_too_wide_for_the_button_keeps_the_identity(self):
        """The bottom rung, and the one that must not publish a door. What the row is FOR
        is where you are and who you are being; a button drawn over that would be the frame
        losing the answer to keep the exit sign."""
        from charter.frame import slots
        row = self._top(cols=12)
        self.assertNotIn("F10 close", row)
        self.assertFalse(any(slots.DOORS.opens_gate(c) for c in range(40)))

    def test_no_button_inside_the_operators_tmux(self):
        """Decision 7 on the pointer half: charter binds no `F10` there, so a button
        advertising it would be telling every operator about a key that does nothing, on
        every repaint — `_bottom`'s rule for the `F2 palette` hint, which is the same
        defect reached through the same server."""
        from charter.frame import slots, state
        from tests import _tmuxsocket
        state.record_server(self.FID, _tmuxsocket.OPERATOR_SOCKET)
        row = self._top()
        self.assertNotIn("F10 close", row)
        self.assertFalse(any(slots.DOORS.opens_gate(c) for c in range(200)))


class AClickReadsTheRecordedPresser(PersonaIso, unittest.TestCase):
    """`builtins._strip_events` — the pointer route's far end.

    The click bind wrote the clicking client onto the panel pane before forwarding the
    press (Step 0's G3); this is the panel process reading its own pane back and handing it
    to the surface it opens. It never asks tmux for a most-recently-active client: that
    answers *a* client, which on a two-client frame is a coin toss over whose terminal to
    close.
    """

    FID = "beta.1"
    A = "/dev/ttys003"

    def setUp(self) -> None:
        super().setUp()
        from charter.frame import slots, state
        state.frame_dir(self.FID, create=True)
        state.record_server(self.FID, "charter-plane-x")
        self.addCleanup(slots.DOORS.forget)

    def _clicked(self, *, answer, gate_door):
        import subprocess
        from charter.frame import builtin_actions, builtins, slots
        slots.DOORS.publish((5,) if not gate_door else (), gate=(5,) if gate_door else ())
        started: list[list[str]] = []
        with mock.patch.dict(os.environ, {"TMUX_PANE": "%9"}, clear=True), \
                mock.patch.object(
                    tmuxctl, "run",
                    side_effect=lambda _w, argv, **kw: subprocess.CompletedProcess(
                        argv, 0, answer, "")), \
                mock.patch.object(builtin_actions, "_spawn",
                                  side_effect=lambda argv, *, fid: started.append(argv)):
            builtins._strip_events(self.FID)(
                SimpleNamespace(pressed=True, name="left", col=5))
        return started[0] if started else []

    def test_a_press_on_the_button_opens_the_gate_for_that_client(self):
        argv = self._clicked(answer=f"{self.A}\n", gate_door=True)
        self.assertEqual(argv[-3:], ["frame-palette", self.A, gate.OPTION])

    def test_a_press_on_any_other_door_opens_the_palette_for_that_client(self):
        """The same presser, the other surface — which is what makes `F2 → Close charter`
        work after a click on the workspace chip as well as after the key."""
        argv = self._clicked(answer=f"{self.A}\n", gate_door=False)
        self.assertEqual(argv[-2:], ["frame-palette", self.A])

    def test_a_presser_that_cannot_be_read_is_no_presser(self):
        """An empty option — a pane charter never wrote one on, a tmux that would not
        answer — and a value outside the shape both reach the surface as *no presser*,
        where the detach row is listed refused with its reason. Never as a client charter
        aims a detach at."""
        for answer in ("", "\n", "x; kill-server\n", "#{client_name}\n"):
            with self.subTest(answer=answer):
                argv = self._clicked(answer=answer, gate_door=True)
                self.assertEqual(argv[-2:], ["frame-palette", gate.OPTION])

    def test_a_click_that_is_not_on_a_door_opens_nothing(self):
        from charter.frame import builtin_actions, builtins, slots
        slots.DOORS.publish((5,), gate=(6,))
        with mock.patch.dict(os.environ, {"TMUX_PANE": "%9"}, clear=True), \
                mock.patch.object(tmuxctl, "run") as asked, \
                mock.patch.object(builtin_actions, "_spawn") as spawn:
            builtins._strip_events(self.FID)(
                SimpleNamespace(pressed=True, name="left", col=40))
        spawn.assert_not_called()
        # And nothing was asked of tmux either: a read per click on dead space would be a
        # round trip for every stray press the terminal reports.
        asked.assert_not_called()

    def test_a_release_opens_nothing(self):
        """Kept word for word from the handler this grew out of: a drag begun on a pane
        border delivers exactly one release, and a drag that began elsewhere and happened
        to end over this row never pointed here."""
        from charter.frame import builtin_actions, builtins, slots
        slots.DOORS.publish((), gate=(5,))
        with mock.patch.dict(os.environ, {"TMUX_PANE": "%9"}, clear=True), \
                mock.patch.object(tmuxctl, "run"), \
                mock.patch.object(builtin_actions, "_spawn") as spawn:
            builtins._strip_events(self.FID)(
                SimpleNamespace(pressed=False, name="left", col=5))
        spawn.assert_not_called()


class TheF2RowsAreTheGate(PersonaIso, unittest.TestCase):
    """`F2` carries the same two rows, in the same words — and in the same places.

    **The point is that there is one gate, not two surfaces that resemble each other.**
    Inside the operator's own tmux the `F2` rows are the ONLY way to them (decision 7), so
    they cannot be a lesser copy; and on charter's own server an operator who learned
    *Close charter* from `F10` has to find the same sentence where they already look.
    """

    FID = "beta.1"

    def setUp(self) -> None:
        super().setUp()
        from charter.frame import state
        state.frame_dir(self.FID, create=True)
        state.record_server(self.FID, "charter-plane-x")
        state.record_harness_pane(self.FID, "%3")
        state.record_identity(self.FID, {"CHARTER_WORKSPACE": "", "CHARTER_PERSONA": ""})

    def test_the_stop_row_is_the_gates_words(self):
        from charter.frame import leave
        self.assertEqual(leave.open_rows(self.FID)[0].title,
                         "Close charter and stop all chats…")
        self.assertEqual(leave.OPEN_QUIT, gate.STOP_TITLE)

    def test_close_is_still_the_last_row(self):
        """`leave.open_rows`' ordering guard, unmoved: the destructive rows are at the
        bottom of the palette so that one is never one `F2 Enter` away, and *chat: close* is
        the last of them. Changing the stop row's WORDS must not change its place."""
        from charter.frame import builtin_actions, choose, leave
        rows = leave.open_rows(self.FID)
        self.assertEqual(rows[-1].id,
                         leave.OPEN_ID.format(leave.CLOSE))
        reg = builtin_actions.build(self.FID, current_density="normal",
                                    current_chrome="off", client="/dev/ttys7")
        catalogue = commands_frame._palette_catalogue(self.FID, reg, snapshot={})
        self.assertEqual(catalogue[-1].id, rows[-1].id)
        self.assertEqual(catalogue[-2].id, rows[0].id)
        self.assertEqual(catalogue[0].id, choose.open_rows(self.FID)[0].id)

    def test_the_detach_row_is_still_the_first_action(self):
        """Its id and its position are unchanged — what moved is the sentence on it. An
        operator's muscle memory for `F2 Enter` reaches the same harmless rows it did."""
        from charter.frame import builtin_actions
        reg = builtin_actions.build(self.FID, current_density="normal",
                                    current_chrome="off", client="/dev/ttys7")
        ids = [o.id for o in reg.offers(fid=self.FID, snapshot={})]
        self.assertEqual(ids[0], "frame.detach")

    def test_the_palette_never_opens_on_the_stop_row_inside_the_operators_tmux(self):
        """Where the detach row is refused, the cursor must still not land on the row that
        stops the plane — which on `F2` is the ORDER doing the work rather than a pin, since
        every picker row above it can run. Asserted on the operator's socket, which is the
        one place charter itself refuses the detach row."""
        from charter.frame import builtin_actions, leave, palette
        from charter.frame import state
        from tests import _tmuxsocket
        state.record_server(self.FID, _tmuxsocket.OPERATOR_SOCKET)
        reg = builtin_actions.build(self.FID, current_density="normal",
                                    current_chrome="off", client="/dev/ttys7")
        catalogue = commands_frame._palette_catalogue(self.FID, reg, snapshot={})
        aimed = catalogue[palette.aim(catalogue)]
        self.assertNotEqual(aimed.id, leave.OPEN_ID.format(leave.QUIT))
        self.assertNotEqual(aimed.id, leave.OPEN_ID.format(leave.CLOSE))
        self.assertFalse(aimed.refused)
