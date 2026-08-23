"""ADR 0019 — inside a frame the frame draws, and `charter statusline` does not.

Two properties, and the second is the one that will look like dead code to a future
reader: the command **keeps running**. Claude Code's per-turn payload is the only place
this session's token usage exists (`hooks.py` has no reference to any usage field), so a
suppression that unwired the command would delete the record rather than hide a
duplicate. `RecordsWhileBlank` below is what a "this command prints nothing, remove it"
change has to get past.

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

    def _run(self, *, fid: str | None, tty: bool = False,
             payload: dict | None = None) -> str:
        env = {"CHARTER_SESSION_ID": fid} if fid else {}
        out = _Tty() if tty else io.StringIO()
        with mock.patch.dict(os.environ, env, clear=True), \
             mock.patch("sys.stdin", io.StringIO(json.dumps(payload or _PAYLOAD))), \
             redirect_stdout(out):
            rc = statusline.main([])
        self.assertEqual(rc, 0, "the status line exits 0 whatever it decides to draw")
        return out.getvalue()

    def _a_live_frame(self) -> str:
        """A frame directory on THIS plane whose launcher is this test process."""
        fid = f"demo-{os.getpid()}"
        state.bump(fid)               # creates the directory `is_live` looks for
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
        self.assertIn("charter", self._run(fid=fid))

    def test_an_id_that_names_no_frame_on_this_plane_renders(self):
        """`$CHARTER_SESSION_ID` is not a frame's variable alone — any harness that knows
        its own session sets it (`charter.session.current`), and a UUID's last group can
        be all digits. A live frame has a directory; an id that never named one here is
        not this plane's frame, whatever its digits parse as."""
        self.assertIn("charter", self._run(fid=f"demo-{os.getpid()}"))

    def test_outside_a_frame_nothing_changes_at_all(self):
        self.assertIn("charter", self._run(fid=None))


class PanelFollowsWorkspaceUse(PersonaIso, unittest.TestCase):
    """`charter ws use` inside a frame moves that frame's panels (#411).

    This is the collision #386 had to decide, pinned. The launcher exports the frame id
    under `$CHARTER_SESSION_ID` — the variable `charter.session.current` already owns —
    so the agent's shell, every panel, and every `charter` command typed inside the frame
    agree about which charter session they belong to. `workspace.set_active` writes its
    per-session pointer under that id and `slots._top` reads it back under the same one.
    Nothing else connects the two: the per-TERMINAL pointer is keyed by `$TMUX_PANE`, and
    the harness and each panel are different panes, so it is structurally unable to carry
    a switch from one to the other.
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


if __name__ == "__main__":
    unittest.main()
