"""The three things about quit that only a real tmux can answer.

`tests/test_a_quit_records_the_plane_before_it_kills.py` says what a quit writes and in what
order; none of that needs a server. What needs one:

* **the capture is real bytes off a real pane** — that `capture-pane -p -e -N -S -<n>` hands
  back what a harness printed, including its trailing spaces, which `-e` alone trims and
  which are the alignment of anything drawn in columns;
* **the kill really ends the chat**, and killing a session's LAST window really destroys the
  session, which is the property that makes a per-window kill enough;
* **§3.3, which is the only thing in this whole design a single-plane test cannot see.** One
  tmux server serves every plane on the machine, session names carry no plane, and `default`
  is a name every plane has — so a quit's blast radius has to be filtered by *this plane's
  chat directories*. `TwoPlanesOnOneServer` is two plane roots with a workspace name in
  common, and it asserts that quitting one leaves the other's window running.

Per §2.12 **none of this runs in CI** — `.github/workflows/test.yml` installs no tmux, so
every case here skips there and a green gate says nothing about any of them. Hand-verified on
tmux 3.7c and at the 3.2 floor (`tmuxctl.FLOOR`), which is why nothing here carries a version
gate.

**Its own socket, reaped.** `commands_frame.SOCKET` is patched per class: the operator's own
frame runs on the bare `charter` socket with sessions from several projects on it, and a test
that killed windows there would kill their work. `tests/_tmuxreap` collects what a killed run
leaves behind.
"""

from __future__ import annotations

import itertools
import shutil
import subprocess
import time
import unittest
from types import SimpleNamespace
from unittest import mock

from charter import commands_frame, config
from charter.frame import leave, reopen, state, tmuxctl

from tests import _tmuxreap
from tests._isolation import PersonaIso, make_plane

_HAS_TMUX = shutil.which("tmux") is not None

#: A socket per class. Shared inside one class because every case there builds and tears
#: down its own windows; never shared between classes, for
#: `test_a_real_click_on_a_real_tab_bar_switches._SERVERS`' measured reason.
_SERVERS = itertools.count()


@unittest.skipUnless(_HAS_TMUX, "needs a real tmux")
class ARealQuitStopsRealChats(PersonaIso, unittest.TestCase):
    """One plane, two chats in one workspace, and a real teardown."""

    def setUp(self):
        super().setUp()
        make_plane(self)
        self.socket = _tmuxreap.name(f"quit-{next(_SERVERS)}")
        self.enterContext(mock.patch.object(commands_frame, "SOCKET", self.socket))
        self.addCleanup(self._kill_server)

    def _kill_server(self):
        subprocess.run(["tmux", "-L", self.socket, "kill-server"],
                       capture_output=True, text=True)

    def _tmux(self, *argv, check=True):
        out = subprocess.run(["tmux", "-L", self.socket, *argv],
                             capture_output=True, text=True, timeout=20)
        if check:
            self.assertEqual(out.returncode, 0, f"{argv}: {out.stderr}")
        return out.stdout.strip()

    def _chat(self, fid: str, *, ws: str, session: str, first: bool, says: str):
        """One real chat: a real window, the `@charter_chat` option, and the real records.

        The harness stand-in prints *says* and then holds the pane open with `cat`, which is
        what makes the capture a measurement rather than a guess: something is genuinely on
        screen and the pane genuinely still exists when the quit reads it.

        The records go through the production writers, so a fixture that stopped agreeing
        with the launcher fails here rather than passing against itself.
        """
        cmd = f"printf '{says}   \\n'; exec cat"
        if first:
            pane = self._tmux("new-session", "-d", "-s", session, "-P", "-F",
                              "#{pane_id}", "-x", "80", "-y", "24", "sh", "-c", cmd)
        else:
            pane = self._tmux("new-window", "-d", "-t", session, "-P", "-F",
                              "#{pane_id}", "sh", "-c", cmd)
        self._tmux("set-option", "-w", "-t", pane, commands_frame._CHAT_OPTION, fid)
        state.frame_dir(fid, create=True)
        state.record_server(fid, self.socket)
        state.record_workspace(fid, ws)
        state.record_harness_pane(fid, pane)
        state.record_cwd(fid, str(config.ROOT))
        state.record_identity(fid, {"CHARTER_HARNESS": "claude-code",
                                    "CHARTER_WORKSPACE": "", "CHARTER_PERSONA": ""})
        state.record_harness_session(fid, f"conv-{fid}")
        # The pane has to have PRINTED before anything captures it, and a poll is the only
        # honest way to know: `new-window` returns when tmux has created the pane, not when
        # the process in it has run.
        deadline = time.time() + 10
        while time.time() < deadline:
            if says in self._tmux("capture-pane", "-p", "-t", pane):
                return pane
            time.sleep(0.05)
        self.fail(f"the stand-in harness for {fid} never printed")

    def test_the_capture_is_the_real_pane_with_its_trailing_spaces(self):
        pane = self._chat("alpha.1", ws="alpha", session="alpha", first=True,
                          says="HELLO-FROM-THE-PANE")
        dest = reopen.transcript_path("alpha.1")

        self.assertTrue(commands_frame._capture_transcript(self.socket, pane, dest))

        text = dest.read_text()
        self.assertIn("HELLO-FROM-THE-PANE", text)
        self.assertIn("HELLO-FROM-THE-PANE   ", text,
                      "-N is what keeps the trailing spaces `-e` alone trims")

    def test_a_pane_that_is_already_gone_captures_nothing_and_says_so(self):
        pane = self._chat("alpha.1", ws="alpha", session="alpha", first=True, says="X")
        self._tmux("kill-pane", "-t", pane, check=False)
        dest = reopen.transcript_path("alpha.1")

        self.assertFalse(commands_frame._capture_transcript(self.socket, pane, dest))
        self.assertFalse(dest.exists())

    def test_the_window_listing_maps_chats_to_windows_and_names_the_active_one(self):
        self._chat("alpha.1", ws="alpha", session="alpha", first=True, says="ONE")
        self._chat("alpha.2", ws="alpha", session="alpha", first=False, says="TWO")

        seats = commands_frame._chat_seats(self.socket)

        self.assertEqual(sorted(c for c, _w, _a in seats), ["alpha.1", "alpha.2"])
        self.assertTrue(all(w.startswith("@") for _c, w, _a in seats))
        # `new-window -d` did not move the session's current window, so the first chat is
        # still the one on screen — which is what a reopen puts the operator back on. One
        # listing answers all three, which is why there is one call rather than two.
        self.assertEqual({c for c, _w, showing in seats if showing}, {"alpha.1"})

    def test_a_server_that_is_not_running_answers_none_rather_than_empty(self):
        self._kill_server()

        self.assertIsNone(commands_frame._chat_seats(self.socket))

    def test_quit_stops_both_chats_records_both_and_ends_the_session(self):
        self._chat("alpha.1", ws="alpha", session="alpha", first=True, says="ONE")
        self._chat("alpha.2", ws="alpha", session="alpha", first=False, says="TWO")

        self.assertEqual(commands_frame.cmd_quit(SimpleNamespace(chat="alpha.1")), 0)

        # The kill: both windows gone, and with the last one the session — and with the
        # last session the SERVER, which is measured here rather than assumed. It is why
        # `_chat_seats` comes back `None` and not `[]`: there is nothing left to answer,
        # which is the same tri-state `_live_chats` documents and the right one, because
        # "charter could not ask" and "the server is empty" are different facts.
        self.assertIsNone(commands_frame._chat_seats(self.socket))
        self.assertEqual(self._tmux("list-sessions", "-F", "#{session_name}",
                                    check=False), "")
        # The record: both chats, both resumable, both with a captured transcript.
        m = reopen.read()
        self.assertEqual([c.chat for c in m.all_chats()], ["alpha.1", "alpha.2"])
        self.assertEqual(m.focus, "alpha")
        self.assertEqual([c.resume for c in m.all_chats()],
                         ["conv-alpha.1", "conv-alpha.2"])
        for c in m.all_chats():
            self.assertEqual(c.transcript, f"{c.chat}{reopen.TRANSCRIPT_SUFFIX}")
            self.assertIn("ONE" if c.chat == "alpha.1" else "TWO",
                          reopen.transcript_path(c.chat).read_text())
        self.assertTrue([c for c in m.all_chats() if c.active],
                        "the chat that was on screen is marked")

    def test_the_directory_survives_the_quit_and_the_manifest_survives_the_reap(self):
        self._chat("alpha.1", ws="alpha", session="alpha", first=True, says="ONE")
        commands_frame.cmd_quit(SimpleNamespace(chat="alpha.1"))

        # A quit does NOT invert `reap`, which is stage 4 of the delivery order and six
        # edits wide. So the next launch's reap takes the chat's directory exactly as it
        # does today — and the manifest, which is a file, stays.
        state.clear_claim("alpha.1")
        self.assertEqual(state.reap(set(), server=self.socket), ["alpha.1"])
        self.assertIsNotNone(reopen.read())
        self.assertTrue(reopen.transcript_path("alpha.1").is_file())


@unittest.skipUnless(_HAS_TMUX, "needs a real tmux")
class TwoPlanesOnOneServer(PersonaIso, unittest.TestCase):
    """§3.3, and the one case a single-plane test is blind to.

    Both planes have a workspace called `default` — `config.DEFAULT_WORKSPACE`, a name every
    plane has — so both have a tmux session called `default` on one server... which tmux
    itself will not allow twice. That is the whole hazard: the SECOND plane's launch joins
    the FIRST plane's session (`cmd_launch`'s `if session in live_sessions`), so one session
    holds windows belonging to two different planes, and a quit that targeted the session
    would stop somebody else's work.

    So the assertion is per WINDOW: plane A's quit kills plane A's window and leaves plane
    B's running, in the same session, on the same server.
    """

    def setUp(self):
        super().setUp()
        make_plane(self)
        self.socket = _tmuxreap.name(f"quit-planes-{next(_SERVERS)}")
        self.addCleanup(self._kill_server)
        self.plane_a = config.use(self.tmp)
        # A second plane root beside the first. `config.use` is how a test moves between
        # them, which is the same call `PersonaIso` itself uses — never a hand-set attribute
        # (`make_plane`'s own note: a plane a test claims to have must be one a subprocess
        # could find too).
        self.b_root = self.tmp.parent / f"{self.tmp.name}-plane-b"
        self.b_root.mkdir()
        (self.b_root / "charter.toml").write_text("schema = 1\n")
        self.addCleanup(shutil.rmtree, self.b_root, True)

    def _kill_server(self):
        subprocess.run(["tmux", "-L", self.socket, "kill-server"],
                       capture_output=True, text=True)

    def _tmux(self, *argv, check=True):
        out = subprocess.run(["tmux", "-L", self.socket, *argv],
                             capture_output=True, text=True, timeout=20)
        if check:
            self.assertEqual(out.returncode, 0, f"{argv}: {out.stderr}")
        return out.stdout.strip()

    def _use(self, root):
        config.use(root)

    def _plant_chat(self, fid: str, *, session: str, first: bool) -> str:
        cmd = "exec cat"
        if first:
            pane = self._tmux("new-session", "-d", "-s", session, "-P", "-F",
                              "#{pane_id}", "-x", "80", "-y", "24", "sh", "-c", cmd)
        else:
            pane = self._tmux("new-window", "-d", "-t", session, "-P", "-F",
                              "#{pane_id}", "sh", "-c", cmd)
        self._tmux("set-option", "-w", "-t", pane, commands_frame._CHAT_OPTION, fid)
        state.frame_dir(fid, create=True)
        state.record_server(fid, self.socket)
        state.record_workspace(fid, config.DEFAULT_WORKSPACE)
        state.record_harness_pane(fid, pane)
        state.record_identity(fid, {"CHARTER_HARNESS": "claude-code",
                                    "CHARTER_WORKSPACE": "", "CHARTER_PERSONA": ""})
        return pane

    def test_a_quit_on_one_plane_leaves_the_other_planes_chat_running(self):
        ws = config.DEFAULT_WORKSPACE
        # Plane A's chat creates the session; plane B's joins it, exactly as a real second
        # launch does when both planes have a workspace of the same name.
        self._use(self.tmp)
        a_pane = self._plant_chat(f"{ws}.1", session=ws, first=True)
        self._use(self.b_root)
        b_pane = self._plant_chat(f"{ws}.2", session=ws, first=False)

        # Both windows are in one session, on one server, with two different planes' state.
        with mock.patch.object(commands_frame, "SOCKET", self.socket):
            self._use(self.tmp)
            self.assertEqual(commands_frame.cmd_quit(
                SimpleNamespace(chat=f"{ws}.1")), 0)

            found = {c for c, _w, _a in commands_frame._chat_seats(self.socket) or []}

        self.assertNotIn(f"{ws}.1", found, "plane A's chat was stopped")
        self.assertIn(f"{ws}.2", found, "plane B's chat is still running")
        self.assertEqual(self._tmux("display-message", "-p", "-t", b_pane,
                                    "#{pane_dead}"), "0")
        del a_pane

    def test_the_quit_records_only_its_own_planes_chats(self):
        ws = config.DEFAULT_WORKSPACE
        self._use(self.tmp)
        self._plant_chat(f"{ws}.1", session=ws, first=True)
        self._use(self.b_root)
        self._plant_chat(f"{ws}.2", session=ws, first=False)

        with mock.patch.object(commands_frame, "SOCKET", self.socket):
            self._use(self.tmp)
            commands_frame.cmd_quit(SimpleNamespace(chat=f"{ws}.1"))
            recorded_a = [c.chat for c in reopen.read().all_chats()]
            self._use(self.b_root)
            recorded_b = reopen.read()

        self.assertEqual(recorded_a, [f"{ws}.1"])
        self.assertIsNone(recorded_b, "plane B was never quit and has nothing recorded")

    def test_neither_planes_quit_ever_reaches_for_kill_server(self):
        ws = config.DEFAULT_WORKSPACE
        self._use(self.tmp)
        self._plant_chat(f"{ws}.1", session=ws, first=True)
        seen = []
        real = tmuxctl.run

        def _watch(why, argv, **kw):
            seen.append(argv)
            return real(why, argv, **kw)

        with mock.patch.object(commands_frame, "SOCKET", self.socket), \
                mock.patch.object(commands_frame.tmuxctl, "run", side_effect=_watch):
            commands_frame.cmd_quit(SimpleNamespace(chat=f"{ws}.1"))

        flat = [" ".join(a) for a in seen]
        self.assertFalse([c for c in flat if "kill-server" in c])
        self.assertFalse([c for c in flat if "kill-session" in c])
        self.assertTrue([c for c in flat if "kill-window" in c])

    def test_the_plan_a_plane_builds_holds_only_its_own_chats(self):
        # The same claim one layer down, and without tmux in it at all: `leave.plan` scans
        # THIS plane's frame root, so a chat id live on the shared server but belonging to
        # another plane is simply not there to be stopped.
        ws = config.DEFAULT_WORKSPACE
        self._use(self.tmp)
        state.frame_dir(f"{ws}.1", create=True)
        state.record_workspace(f"{ws}.1", ws)
        self._use(self.b_root)
        state.frame_dir(f"{ws}.2", create=True)
        state.record_workspace(f"{ws}.2", ws)

        self._use(self.tmp)
        p = leave.plan(live={f"{ws}.1", f"{ws}.2"}, focus=ws)

        self.assertEqual([c.chat for c in p.chats], [f"{ws}.1"])


if __name__ == "__main__":       # pragma: no cover
    unittest.main()
