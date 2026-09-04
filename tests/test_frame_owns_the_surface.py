"""ADR 0019 — inside a frame the frame draws, and `charter statusline` does not.

Two properties, and the second is the one that will look like dead code to a future
reader: the command **keeps running**. Claude Code's per-turn payload is the only place
this session's token usage exists (`hooks.py` has no reference to any usage field), so a
suppression that unwired the command would delete the record rather than hide a
duplicate. `FrameOwnsTheSurface.test_the_turn_is_still_recorded_while_the_line_is_blank`
is what a "this command prints nothing, remove it" change has to get past.

**#895 unwired it from Claude Code anyway, deliberately, and the operator was told what
it costs.** Charter no longer writes a `statusLine` key, so on a charter-made plane
nothing pipes a per-turn payload in and that record is simply not written — which is why
the frame's `ctx`/`cache` gauge has nothing to read. Every test below still means what it
meant: they drive `statusline.main` with a payload directly, which is exactly what a
hand-wired footer and opencode's `/charter` still do, and the recording path they pin is
untouched. What went with the key is `SuppressionSaysSoOnDemand`, the class that held
`charter doctor`'s frame row to explaining a blank footer — there is no footer for the row
to explain any more, and a note that fires for every framed session about a surface it does
not have is worse than silence. The suppression itself is still pinned, above.

**Every fixture id ends in a pid that is really that pid.** `state.is_live` reads the
number at the end of a frame id and asks whether that process exists, so `something-1`
is `launchd` — permanently alive — and a fixture named that way makes "the frame was
gone, so it rendered" unfailable. Live means `os.getpid()`, this very process; dead means
`_a_dead_pid()`, a child that has exited and been reaped.
"""

from __future__ import annotations

import io
import json
import os
import subprocess
import sys
import unittest
from contextlib import redirect_stdout
from unittest import mock

from charter import statusline, workspace
from charter.frame import slots, state

from tests import _gitguard
from tests._isolation import PersonaIso


def _a_dead_pid() -> int:
    """A real pid that has exited and been reaped — see `tests/test_frame_state.py`,
    which needs this for the same reason and says why a made-up number is a guess about
    the machine rather than a fact about it."""
    p = subprocess.Popen([sys.executable, "-c", "pass"])
    p.wait()
    return p.pid


class _Tty(io.StringIO):
    """A captured stdout that answers `isatty()` the way a terminal does. `io.StringIO`
    says False, which is exactly the piped case, so a test of the tty branch cannot use
    the plain one."""

    def isatty(self) -> bool:
        return True


#: One turn's worth of what Claude Code actually sends: a session id and the token
#: counters `_context_gauge`/`record_usage` read. The numbers are what makes the
#: recording assertions below observable at all — a payload without them records
#: nothing, correctly, and would make every recording test pass for the wrong reason.
_PAYLOAD = {
    "session_id": "cc-session-1",
    "context_window": {"used_percentage": 42,
                       "current_usage": {"cache_read_input_tokens": 90_000,
                                         "cache_creation_input_tokens": 10_000}},
}


class FrameOwnsTheSurface(PersonaIso, unittest.TestCase):
    """`statusline.main` with a payload on stdin, exactly as Claude Code invokes it.

    `PersonaIso` is load-bearing rather than hygiene here: `state.is_live` looks for the
    frame's directory under `config.STATE_DIR`, so an isolated plane is what keeps this
    module's own answers independent of whether the developer ran the suite from inside
    a real frame — the property that let this check live at the command edge at all.
    """

    #: The pane the fixture frames below record as their harness's, and the one
    #: `_run` claims to be in unless a test deliberately says otherwise.
    _PANE = "%7"

    def _run(self, *, fid: str | None, tty: bool = False, pane: str = _PANE,
             harness: str = "claude-code", payload: dict | None = None) -> str:
        # `clear=True` below empties the environment, and `statusline.render` reaches
        # `inventory.plane_repo`, which runs `git remote get-url origin`. A git child of a
        # cleared environment reads the operator's own `~/.gitconfig` — the one thing this
        # suite must not let it do (#641) — so the redirect is put back explicitly. Ten
        # cases in this module were the only place in 7,984 that stepped outside it, and
        # `tests._planeguard.AmbientGitConfig` is what found them.
        env = dict(_gitguard.environment())
        if fid:
            env["CHARTER_SESSION_ID"] = fid
        if pane:
            env["TMUX_PANE"] = pane
        if harness:
            # Stated, never left to `harness.current()`'s detection fallback: suppression
            # only ever applies to the harness whose surface the panels duplicate, so a
            # test that did not say which harness it was would be asserting about
            # whatever the developer's own terminal happened to look like.
            env["CHARTER_HARNESS"] = harness
        out = _Tty() if tty else io.StringIO()
        with mock.patch.dict(os.environ, env, clear=True), \
             mock.patch("sys.stdin", io.StringIO(json.dumps(payload or _PAYLOAD))), \
             redirect_stdout(out):
            rc = statusline.main([])
        self.assertEqual(rc, 0, "the status line exits 0 whatever it decides to draw")
        return out.getvalue()

    def _a_live_frame(self, *, pane: str = _PANE) -> str:
        """A frame the way `cmd_launch` leaves one: a directory, a recorded server, a
        recorded harness pane, and a launcher pid that is this very test process.

        Written out longhand rather than hidden behind one call, because each of the
        four is separately load-bearing in `state.is_live` and a test that removes one
        (there are three below) has to be able to say which."""
        fid = f"demo-{os.getpid()}"
        state.bump(fid)                          # the directory
        state.record_server(fid, "charter")      # proof a LAUNCHER made it
        state.record_harness_pane(fid, pane)     # which pane is this frame's harness
        return fid

    # -- blank inside a frame ---------------------------------------------------- #

    def test_inside_a_live_frame_the_line_is_blank(self):
        self.assertEqual(self._run(fid=self._a_live_frame()).strip(), "")

    def test_inside_a_live_frame_nothing_is_even_rendered(self):
        """Not merely "the output was empty": a `render` that returned an empty string
        for some unrelated reason would satisfy the test above. The frame path must not
        reach the renderer at all — that is what makes suppression cost nothing rather
        than gather the whole plane and throw it away every ten seconds."""
        with mock.patch.object(statusline, "render") as render:
            self._run(fid=self._a_live_frame())
        render.assert_not_called()

    # -- and still recording ----------------------------------------------------- #

    def test_the_turn_is_still_recorded_while_the_line_is_blank(self):
        """The reason the command is kept wired at all (ADR 0019). Claude Code's payload
        is the only source of these numbers — nothing in `hooks.py` sees them — so a
        change that stopped invoking `charter statusline` inside a frame would delete
        the history, not merely the duplicate line."""
        self._run(fid=self._a_live_frame())
        self.assertEqual(statusline._history("cc-session-1"), [(90_000, 10_000)])

    def test_a_payload_with_no_numbers_records_nothing_rather_than_a_zero(self):
        """The other half, and what stops the test above from passing against a recorder
        that writes on every call: early in a session (and right after `/compact`) the
        payload carries no usage at all, and a recorded zero would be an invented turn."""
        self._run(fid=self._a_live_frame(), payload={"session_id": "cc-session-1"})
        self.assertEqual(statusline._history("cc-session-1"), [])

    # -- and telling the frame whose session this is (#413) ----------------------- #

    def test_the_frame_learns_the_harnesss_own_session_id(self):
        """#413's mapping, and the reason it is written HERE and nowhere else: the usage
        history is keyed by Claude Code's session id, a panel knows only the FRAME's id,
        and this process is the one place both are in scope at the same moment — the
        frame id in its environment, Claude Code's in the payload on its stdin.

        Without it a framed session has no `ctx`/`cache` gauge at all, which 0.52.0's own
        news entry named as the one capability the frame genuinely lost."""
        fid = self._a_live_frame()
        self._run(fid=fid)
        self.assertEqual(state.harness_session(fid), "cc-session-1")

    def test_a_session_that_changed_under_the_frame_is_rewritten(self):
        """One frame outlives several agent sessions — `/clear`, or a resume — and each
        gets a new id. The mapping is rewritten every turn rather than written once, so a
        panel's gauge follows the session actually running rather than the first one the
        frame ever saw."""
        fid = self._a_live_frame()
        self._run(fid=fid)
        self._run(fid=fid, payload={"session_id": "cc-session-2",
                                    "context_window": {"used_percentage": 10}})
        self.assertEqual(state.harness_session(fid), "cc-session-2")

    def test_the_panels_are_woken_when_a_turn_is_recorded(self):
        """A panel repaints on a version bump and on nothing else, and `record_usage`
        bumps nothing — so without this, `top`'s gauge would sit on whatever it last drew
        until some unrelated hook happened to fire, which on a turn that calls no tools is
        never. A gauge showing last hour's number is worse than no gauge, which is the
        rule this whole feature is built around."""
        fid = self._a_live_frame()
        before = state.version(fid)
        self._run(fid=fid)
        self.assertNotEqual(state.version(fid), before)

    def test_a_rerender_of_the_same_turn_wakes_nobody(self):
        """The other half, and the one that keeps this affordable: the status line renders
        several times per turn, and each bump repaints every panel — `slots.ANIMATED`'s
        own note measures one `render("right")` at 4.8ms. So the frame is woken only when
        something it could draw actually changed. Asserted through TWO calls with the
        identical payload, which is exactly what a re-render is."""
        fid = self._a_live_frame()
        self._run(fid=fid)
        after_first = state.version(fid)
        self._run(fid=fid)
        self.assertEqual(state.version(fid), after_first)

    def test_a_new_session_id_alone_is_enough_to_wake_them(self):
        """The mapping changing is its own reason to repaint even when no new turn was
        recorded: the panel is about to read a different session's history, so what it is
        drawing is stale the moment the file changes. The second payload carries no usage
        numbers at all, so the recorded turn cannot be what moved the version."""
        fid = self._a_live_frame()
        self._run(fid=fid)
        after_first = state.version(fid)
        self._run(fid=fid, payload={"session_id": "cc-session-2"})
        self.assertNotEqual(state.version(fid), after_first)

    def test_nothing_is_recorded_for_a_session_line_that_was_not_suppressed(self):
        """The mapping is written on the suppressed branch alone. Outside a frame there is
        no frame to tell, and `a_frame_owns_this_surface` has already settled the question
        — a second, weaker check here is how the two would come to disagree about what
        counts as being inside one."""
        fid = self._a_live_frame()
        self._run(fid=fid, tty=True)
        self.assertIsNone(state.harness_session(fid))

    # -- and drawing everywhere else --------------------------------------------- #

    def test_a_human_asking_at_a_terminal_still_gets_the_line(self):
        """Claude Code pipes this command's stdout; a tty means somebody typed
        `charter statusline` themselves, and a frame elsewhere on the screen is no
        reason to answer them with a blank line."""
        self.assertIn("charter", self._run(fid=self._a_live_frame(), tty=True))

    def test_a_frame_whose_launcher_is_gone_does_not_blank_it_forever(self):
        """A frame directory outlives a crashed launcher until some later launch reaps
        it. Reading the directory alone as "a frame is running" would leave this plane's
        status line blank for every session afterwards, with nothing on screen to say
        why. The pid at the end of the id is what answers it, with no tmux call on a
        path that runs every time the footer repaints."""
        fid = f"demo-{_a_dead_pid()}"
        state.bump(fid)
        # COMPLETE in every other respect, and that is the point: a fixture missing the
        # server marker or the pane would be refused before the pid was ever consulted,
        # and this test would pass with the liveness check deleted outright. Measured —
        # it did, until the marker check was added underneath it.
        state.record_server(fid, "charter")
        state.record_harness_pane(fid, self._PANE)
        self.assertIn("charter", self._run(fid=fid))

    def test_an_id_that_names_no_frame_on_this_plane_renders(self):
        """`$CHARTER_SESSION_ID` is not a frame's variable alone — any harness that knows
        its own session sets it (`charter.session.current`), and a UUID's last group can
        be all digits. A live frame has a directory; an id that never named one here is
        not this plane's frame, whatever its digits parse as."""
        self.assertIn("charter", self._run(fid=f"demo-{os.getpid()}"))

    def test_a_directory_no_launcher_made_is_not_a_frame(self):
        """A directory is not proof of frame-ness, and this is the operator it protects:
        someone who exports `CHARTER_SESSION_ID=proj-$$` in their shell rc. The first
        hook that fires calls `notify.plane_changed` -> `state.bump`, which MINTS the
        directory, and the pid in that id is their own live shell — so directory-plus-pid
        is satisfied, permanently, for a frame that never existed. Only `cmd_launch`
        writes a server marker."""
        fid = f"demo-{os.getpid()}"
        state.bump(fid)                          # exactly what a hook does, and no more
        state.record_harness_pane(fid, self._PANE)
        self.assertIsNone(state.frame_server(fid), "fixture no longer matches a hook")
        self.assertIn("charter", self._run(fid=fid))

    def test_a_process_that_merely_inherited_the_id_is_not_inside_the_frame(self):
        """The regression this guard exists for, and it is one this PR would otherwise
        have CAUSED. Below `tmuxctl.SESSION_ENV_FLOOR` charter cannot put the frame id on
        `new-session`, so a second frame's harness on the shared private server inherits
        the FIRST frame's id (#411) — live, on this plane, launcher running, every other
        condition satisfied. Suppressing there blanks a footer whose panels are already
        following another frame: no correct surface at all, where before this change that
        operator at least had a correct status line.

        The pane is what tells them apart: tmux gives each pane its own `$TMUX_PANE`, and
        only one of them is the pane the launcher recorded."""
        fid = self._a_live_frame()
        self.assertIn("charter", self._run(fid=fid, pane="%99"))

    def test_no_pane_at_all_is_not_inside_the_frame_either(self):
        """`$TMUX_PANE` absent is an ANSWER — "not in any pane" — not a reason to stop
        asking. Read as "unknown, carry on" instead, a process holding an inherited id
        outside tmux entirely would suppress."""
        fid = self._a_live_frame()
        self.assertIn("charter", self._run(fid=fid, pane=""))

    def test_outside_a_frame_nothing_changes_at_all(self):
        self.assertIn("charter", self._run(fid=None))

    def test_a_harness_with_no_status_bar_of_its_own_is_never_suppressed(self):
        """opencode has no footer for a panel to duplicate, so charter wires the plane in
        as an on-demand `/charter` slash command whose body is
        ``!`echo '{}' | charter statusline` `` (`harness/opencode.py`'s `COMMAND`).

        That invocation satisfies every OTHER condition perfectly — its stdout is a pipe
        because it is a shell substitution, its `$CHARTER_SESSION_ID` is the live frame's,
        and its `$TMUX_PANE` IS the recorded harness pane, because opencode is what runs
        there. Suppressing it removed nothing and cost everything: `/charter` exists to put
        plane state into the AGENT'S CONTEXT, which no panel can do — a panel draws to a
        pane the model never reads. Reproduced as a blank line before this rung existed."""
        self.assertIn("charter", self._run(fid=self._a_live_frame(), harness="opencode"))

    def test_a_harness_charter_cannot_identify_is_not_suppressed_either(self):
        """The safe direction, stated as a property rather than left to luck: an unknown
        (or absent) harness answers "not the surface being duplicated", so the worst case
        is the duplicate line this release removes — never a surface that vanished."""
        self.assertIn("charter", self._run(fid=self._a_live_frame(), harness=""))


class PanelFollowsWorkspaceUseOnAFrameWithNoRecord(PersonaIso, unittest.TestCase):
    """`charter ws use` inside a frame that recorded nothing moves that frame's panels
    (#411), and **only** such a frame since #791.

    This is the collision #386 had to decide, pinned. The launcher exports the frame id
    under `$CHARTER_SESSION_ID` — the variable `charter.session.current` already owns —
    so the agent's shell, every panel, and every `charter` command typed inside the frame
    agree about which charter session they belong to. `workspace.set_active` writes its
    per-session pointer under that id and `slots._top` reads it back under the same one.
    Nothing else connects the two: the per-TERMINAL pointer is keyed by `$TMUX_PANE`, and
    the harness and each panel are different panes, so it is structurally unable to carry
    a switch from one to the other.

    **Which rung of `state.workspace_for` carries it has changed, and the class was
    renamed rather than left to read as a promise it no longer makes.** The pointer used to
    be rung 1, inside `state.own_workspace`, and #791 took it out: a chat's workspace is
    its identity (§4j) and `own_workspace` is also what decides membership of a workspace,
    so a typed command moving that rung re-homed the chat (#733, #788 verbatim). What is
    left is rung 2 — a local `workspace.resolve()` for a frame that recorded no workspace
    of its own, which is the migration case `docs/frame.md` names. The fixture here is
    exactly that frame: a `{workspace}-{pid}` id with no directory under `.charter/frame/`,
    which is what #411 was reported on.

    The chat case — a frame WITH a launch record, which is every frame charter mints today
    — is the opposite assertion and lives with the surface it is about:
    `tests/test_frame_slots.EveryPanelDrawsTheFramesOwnWorkspace`. The last case below
    pins the boundary between the two so neither file can move without the other noticing.
    """

    def _top(self, fid: str) -> str:
        with mock.patch.dict(os.environ, {"CHARTER_SESSION_ID": fid}, clear=True):
            return slots._top(fid)

    def test_the_top_panel_follows_a_switch_made_inside_the_frame(self):
        """Switched TWICE, and both are asserted. One switch alone passes against a
        panel that read the workspace once at launch and never again, as long as the
        launch happened to be in the workspace the test switched to; a second switch
        to a different name is what makes the panel prove it is still reading."""
        fid = f"demo-{os.getpid()}"
        workspace.ensure("alpha")
        workspace.ensure("beta")
        with mock.patch.dict(os.environ, {"CHARTER_SESSION_ID": fid}, clear=True):
            workspace.set_active("alpha")
        self.assertIn("alpha", self._top(fid))
        with mock.patch.dict(os.environ, {"CHARTER_SESSION_ID": fid}, clear=True):
            workspace.set_active("beta", force=True)
        self.assertIn("beta", self._top(fid))

    def test_a_switch_made_under_another_id_does_not_move_this_frame(self):
        """The other direction, and the shape of #411 itself: writer and reader agreeing
        is the whole mechanism, so a pointer written under a DIFFERENT session id must
        leave this frame's panel exactly where it was. Without this, a `resolve()` that
        ignored the session pointer entirely — and answered from some global — would
        still pass the test above."""
        fid = f"demo-{os.getpid()}"
        workspace.ensure("alpha")
        workspace.ensure("beta")
        with mock.patch.dict(os.environ, {"CHARTER_SESSION_ID": fid}, clear=True):
            workspace.set_active("alpha")
        with mock.patch.dict(os.environ,
                             {"CHARTER_SESSION_ID": "someone-elses-frame-1234"},
                             clear=True):
            workspace.set_active("beta")
        self.assertIn("alpha", self._top(fid))
        self.assertNotIn("beta", self._top(fid))

    def test_the_moment_the_frame_records_a_workspace_the_pointer_stops_reaching_it(self):
        """**The boundary #791 drew, asserted from the side that used to cross it.**

        The two cases above pass through rung 2 of `state.workspace_for` — a local
        `workspace.resolve()`, reached only because this frame recorded nothing. Write the
        launch record and rung 1 answers instead, and rung 1 is `state.own_workspace`,
        which no longer reads a pointer at all. So the same switch that moved the panel a
        line ago moves nothing: the panel draws what the launch recorded.

        Without this, both cases above read as "panels follow `ws use`" — which is what
        their class was called and what `docs/frame.md` promised — on a plane where that is
        true for the migration case alone."""
        fid = f"demo-{os.getpid()}"
        workspace.ensure("alpha")
        workspace.ensure("beta")
        state.record_workspace(fid, "alpha")
        with mock.patch.dict(os.environ, {"CHARTER_SESSION_ID": fid}, clear=True):
            workspace.set_active("beta")
            self.assertEqual(workspace.for_session(fid), "beta",
                             "the pointer this case is about was not written")
        self.assertIn("alpha", self._top(fid))
        self.assertNotIn("beta", self._top(fid))


if __name__ == "__main__":
    unittest.main()
