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

**§2.12 says none of this runs in CI, and that is wrong** — measured by this very module.
`.github/workflows/test.yml` installs no tmux, but `ubuntu-latest` has one, so `_HAS_TMUX` is
true there and every case here RAN: one of them failed on 3.11 and 3.13 and passed on 3.14 in
the same run, which is a race CI found and this machine did not. So the useful statement is
narrower and sharper than the spec's: **CI runs these on whatever tmux the runner image
happens to ship, which is neither of the two versions charter promises.** The floor and the
version charter is developed on are still hand-run — 3.7c and the 3.2 floor
(`tmuxctl.FLOOR`), which is why nothing here carries a version gate — and CI is a third
machine's answer on top of that, worth having precisely because it is a different one.

**Its own socket, reaped.** `tmuxctl.plane_socket` is patched per class: the operator's own
frames run on their plane's `charter-plane-<hex>` socket, or on the bare `charter` one with
sessions from several projects on it, and a test that killed windows there would kill their
work. `tests/_tmuxreap` collects what a killed run
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
        self.enterContext(mock.patch.object(tmuxctl, "plane_socket", return_value=self.socket))
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

    def test_a_window_charter_cannot_decode_is_dropped_rather_than_raised(self):
        """#828 on a real server, and **not through the caller the issue names.**

        The issue's reproduction is `capture-pane` over a harness pane, on the grounds that
        a pane holds arbitrary bytes. Measured on tmux 3.7c — under `LANG=C.UTF-8` and again
        under `LC_ALL=C` — it does not reach charter that way: a pane that prints `\\377` is
        stored in tmux's own screen as U+FFFD, and `capture-pane -p -e -N` hands back valid
        UTF-8. tmux sanitised it first.

        Two real paths do reach charter, and this is the one a quit walks. A tmux USER
        OPTION round-trips its bytes untouched — `set-option -w @charter_chat` with a raw
        `\\377` in it comes back out of ``list-windows -a -F '#{@charter_chat}'`` and out of
        `display-message -p` exactly as it went in, measured on 3.7c — and that listing is
        `_chat_seats`, which `cmd_quit` asks before it kills anything. §3.3 is why this is
        not hypothetical: one tmux server serves every plane on the machine, so charter
        reads windows it did not create, and the harness agent inside a pane can reach the
        same socket. (The other path is tmux's own stderr, which echoes the raw bytes of an
        argument it refuses: `invalid window name: BAD\\377NAME`, straight into
        `report_failure`.)

        What the row does once it decodes is what this class already documents: it fails
        `_FRAME_ID_RE` and is dropped, *rows charter cannot read are dropped rather than
        guessed at* — and the chats charter CAN read are still answered for.
        """
        self._chat("alpha.1", ws="alpha", session="alpha", first=True, says="ONE")
        pane = self._tmux("new-window", "-d", "-t", "alpha", "-P", "-F", "#{pane_id}",
                          "sh", "-c", "exec cat")
        # A lone surrogate in argv is how a raw byte reaches a child: `os.fsencode` writes
        # it back out with `surrogateescape`, so tmux stores 0xFF and not the escape.
        self._tmux("set-option", "-w", "-t", pane, commands_frame._CHAT_OPTION,
                   "al\udcffpha.9")

        seats = commands_frame._chat_seats(self.socket)

        self.assertEqual(sorted(c for c, _w, _a in seats), ["alpha.1"])

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
class TheTranscriptOpensInAWindowOfItsOwn(PersonaIso, unittest.TestCase):
    """§4f's other half, and **the one target only a hand-run could have found.**

    `kill-window -t %N` resolves a pane to its own window — measured, and `cmd_launch`'s
    early-death path already relies on it — so `new-window -t %N` looks like it should too.
    It does not. Measured on tmux 3.7c **and** at the 3.2 floor, on a session called
    `alpha.2` holding its own `$0`/`@0`/`%0`:

        new-window -t %0        rc 1   can't specify pane here
        new-window -t @0        rc 1   create window failed: index 0 in use
        new-window -t alpha.2   rc 1   can't specify pane here      <- #695, again
        new-window -t $0        rc 0

    `-t` here is a target-WINDOW and a window id is read as the index to insert at, which is
    by definition taken; a dotted session name is parsed as `window.pane`. The session ID is
    the one unambiguous spelling. The first version of this shipped `-t <pane>`, was correct
    against nothing, and reported its own failure into the frame's notice row — which is
    exactly the *"ten lines and a tmux semantics claim is not cheap"* the delivery plan warns
    about (#664, #687, #690).
    """

    def setUp(self):
        super().setUp()
        make_plane(self)
        self.socket = _tmuxreap.name(f"transcript-{next(_SERVERS)}")
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

    def _chat_with_a_transcript(self, text="LINE-ONE\n\x1b[31mRED\x1b[0m\nLINE-THREE\n"):
        """A real chat in a session whose NAME has a dot in it, which is the hostile case.

        A workspace may be called `api.2` (`instance.WORKSPACE_NAME_RE` accepts a dot), and
        `state.workspace_prefix` is what keeps the SESSION name out of that alphabet — so the
        session here is `alpha_2` and the recorded workspace is `alpha.2`, exactly as a real
        launch would have it. The point is that nothing on this path ever hands tmux a name.
        """
        pane = self._tmux("new-session", "-d", "-s", "alpha_2", "-P", "-F", "#{pane_id}",
                          "-x", "80", "-y", "24", "sh", "-c", "exec cat")
        self._tmux("set-option", "-w", "-t", pane, commands_frame._CHAT_OPTION, "alpha_2.1")
        state.frame_dir("alpha_2.1", create=True)
        state.record_server("alpha_2.1", self.socket)
        state.record_workspace("alpha_2.1", "alpha.2")
        state.record_harness_pane("alpha_2.1", pane)
        config.write_for(reopen.transcript_path("alpha_2.1"), text)
        return pane

    def _windows(self):
        return dict(line.split("\t", 1) for line in self._tmux(
            "list-windows", "-a", "-F", "#{window_id}\t#{window_name}").splitlines())

    def test_it_opens_a_pager_window_beside_the_chat_and_shows_the_text(self):
        self._chat_with_a_transcript()
        before = self._windows()

        with mock.patch.object(tmuxctl, "plane_socket", return_value=self.socket):
            self.assertEqual(commands_frame.cmd_transcript(
                SimpleNamespace(chat="alpha_2.1")), 0)

        after = self._windows()
        new = [w for w in after if w not in before]
        self.assertEqual(len(new), 1, f"{before} -> {after}")
        self.assertIn("transcript alpha_2.1", after[new[0]])
        # A pager, not a dead pane: `less` is what makes `q` close the window, and a dead
        # pane under `remain-on-exit` is a window that does not close in a frame whose
        # prefix key charter hides (§2.14).
        #
        # **Polled, and CI is what taught this.** `new-window` returns when tmux has created
        # the pane, not when the process in it has exec'd — so `#{pane_current_command}` is
        # still `tmux` for a moment. Asserted immediately it passed on 3.14 and failed on
        # 3.11 and 3.13 in the same run, which is the signature of a race and not of a
        # version difference. Same shape as `_chat_with_a_transcript`'s own wait, for the
        # same reason.
        self.assertEqual(self._settled(new[0]), (commands_frame._PAGER[0], "0"))
        # And the text really reached it, with the escape sequence rendered rather than
        # printed — which is what `-R` on the pager and `-e` on the capture are both for.
        seen = self._shows(new[0], "LINE-ONE")
        self.assertIn("LINE-ONE", seen)
        self.assertIn("RED", seen)
        self.assertNotIn("\x1b[31m", seen)

    #: How long the two polls below will wait. Generous, because what they are waiting for is
    #: another process starting and painting on a shared runner, and the cost of a too-short
    #: wait is a red that says nothing about charter.
    _WAIT = 15.0

    def _settled(self, window):
        """``(command, dead)`` for *window*'s pane, once its process has exec'd.

        **Polled, and CI is what taught this** — twice, in two places. `new-window` returns
        when tmux has created the pane, not when the process in it has exec'd, so
        `#{pane_current_command}` is `tmux` for a moment: asserted immediately it passed on
        3.14 and failed on 3.11 and 3.13 in the same run.

        Returns whatever the last reading was when the deadline runs out, so a genuine
        failure is reported as the value it actually had rather than as a timeout.
        """
        deadline = time.time() + self._WAIT
        seen = ("", "")
        while time.time() < deadline:
            fields = self._tmux("list-panes", "-t", window, "-F",
                                "#{pane_current_command}\t#{pane_dead}").split("\t")
            seen = (fields[0], fields[1] if len(fields) > 1 else "")
            if seen[0] == commands_frame._PAGER[0]:
                return seen
            time.sleep(0.05)
        return seen

    def _shows(self, window, needle):
        """*window*'s pane contents, once *needle* is in them.

        **The second half of the same lesson, and the one that needed a second CI run to
        find.** A pager that has exec'd has not necessarily PAINTED: with `_settled` alone,
        3.11 and 3.13 went green and 3.14 came back with an empty capture. "Has the process
        started" and "has it drawn" are two facts and neither implies the other on a loaded
        runner.

        Returns the last capture either way, so the assertion that follows fails on what was
        actually on screen rather than on a timeout.
        """
        deadline = time.time() + self._WAIT
        seen = ""
        while time.time() < deadline:
            seen = self._tmux("capture-pane", "-p", "-t", window)
            if needle in seen:
                return seen
            time.sleep(0.05)
        return seen

    def test_the_chats_own_window_is_left_exactly_as_it_was(self):
        pane = self._chat_with_a_transcript()

        with mock.patch.object(tmuxctl, "plane_socket", return_value=self.socket):
            commands_frame.cmd_transcript(SimpleNamespace(chat="alpha_2.1"))

        # Nothing is written into the harness's pane — ADR 0018's half that this change
        # leaves untouched — and its own process is still the one that was there. Read after
        # the new window has settled, so this is not asserting about a moment before
        # `new-window` had finished doing anything at all.
        new = [w for w in self._windows() if "transcript" in self._windows()[w]]
        if new:
            self._settled(new[0])
        self.assertEqual(self._tmux("display-message", "-p", "-t", pane,
                                    "#{pane_current_command}:#{pane_dead}"), "cat:0")

    def test_a_chat_with_no_capture_says_so_and_opens_nothing(self):
        self._chat_with_a_transcript()
        reopen.transcript_path("alpha_2.1").unlink()
        before = self._windows()

        with mock.patch.object(tmuxctl, "plane_socket", return_value=self.socket), \
                mock.patch.object(commands_frame, "_say_on_screen") as said:
            self.assertEqual(commands_frame.cmd_transcript(
                SimpleNamespace(chat="alpha_2.1")), 0)

        self.assertEqual(self._windows(), before)
        said.assert_called_once()
        self.assertEqual(said.call_args[0][1], commands_frame.NO_TRANSCRIPT)

    def _windows_now(self) -> dict:
        """:meth:`_windows`, but ``{}`` when the whole server is gone — killing a session's
        last window ends the session and the server, which is a viewer being gone, not a
        fault. `check=False` so the reader does not fail on that ordinary end-state."""
        out = self._tmux("list-windows", "-a", "-F", "#{window_id}\t#{window_name}",
                         check=False)
        return dict(line.split("\t", 1) for line in out.splitlines() if "\t" in line)

    def _sessions_now(self) -> str:
        return self._tmux("list-sessions", "-F", "#{session_name}", check=False)

    def _open_a_viewer(self) -> str:
        """Open a transcript viewer for `alpha_2.1` and return its window id."""
        before = set(self._windows_now())
        with mock.patch.object(tmuxctl, "plane_socket", return_value=self.socket):
            self.assertEqual(commands_frame.cmd_transcript(
                SimpleNamespace(chat="alpha_2.1")), 0)
        new = [w for w in self._windows_now() if w not in before]
        self.assertEqual(len(new), 1, self._windows_now())
        self.assertIn("transcript alpha_2.1", self._windows_now()[new[0]])
        return new[0]

    def test_closing_the_chat_takes_its_transcript_viewer_with_it(self):
        """**The operator's bug (defect 4).** A viewer opens in its own window beside the
        chat; when the chat is CLOSED, the viewer must go too — otherwise it keeps the
        workspace's session alive with nothing charter recognises in it, which a later tab
        then refuses as another plane's. Here the chat and its viewer are the session's two
        windows, so closing the chat empties and removes the session."""
        self._chat_with_a_transcript()
        viewer = self._open_a_viewer()

        with mock.patch.object(tmuxctl, "plane_socket", return_value=self.socket):
            self.assertEqual(commands_frame.cmd_close(
                SimpleNamespace(chat="", chat_id="alpha_2.1")), 0)

        windows = self._windows_now()
        self.assertNotIn(viewer, windows, "the transcript viewer outlived the closed chat")
        self.assertFalse(any("transcript" in n for n in windows.values()), windows)
        self.assertEqual(self._sessions_now(), "",
                         "the closed chat's session was left alive by its viewer")

    def test_quitting_the_chat_takes_its_transcript_viewer_with_it(self):
        """The same, for quit — quit and close share `_stop_chats`, so a viewer that survived
        one would survive the other."""
        self._chat_with_a_transcript()
        viewer = self._open_a_viewer()

        with mock.patch.object(tmuxctl, "plane_socket", return_value=self.socket):
            self.assertEqual(commands_frame.cmd_quit(SimpleNamespace(chat="alpha_2.1")), 0)

        self.assertNotIn(viewer, self._windows_now(),
                         "the transcript viewer outlived the quit chat")

    def test_a_viewer_orphaned_by_a_dead_chat_is_swept_and_its_session_goes(self):
        """**The pane-died path.** A harness that dies on its own is torn down by tmux's own
        `pane-died` hook, which kills the chat's window and cannot reliably reach the sibling
        viewer window — so the viewer is left an orphan. It is swept before the next launch
        reads liveness, and killing its window (the session's last) removes the session, so it
        never reads as a live workspace that blocks a restore or is refused as another plane's."""
        chat_pane = self._chat_with_a_transcript()
        viewer = self._open_a_viewer()
        # The chat's harness pane dies and its window goes — the viewer window is left behind,
        # keeping the `alpha_2` session alive with nothing charter recognises in it.
        self._tmux("kill-window", "-t", chat_pane)
        self.assertIn(viewer, self._windows_now(), "the fixture did not leave an orphan viewer")
        self.assertIn("alpha_2", self._sessions_now())

        commands_frame._sweep_orphan_transcripts(
            self.socket, commands_frame._live_chats(self.socket))

        self.assertNotIn(viewer, self._windows_now(), "the orphaned viewer was not swept")
        self.assertEqual(self._sessions_now(), "",
                         "the orphan viewer's session outlived it")

    def test_a_second_viewer_for_one_chat_replaces_the_first(self):
        """One viewer per chat: a second press closes the first, so the marked window the
        teardown ties to the chat is unambiguous."""
        self._chat_with_a_transcript()
        first = self._open_a_viewer()
        second = self._open_a_viewer()
        self.assertNotEqual(first, second)
        self.assertNotIn(first, self._windows(), "a second viewer left the first standing")
        self.assertEqual([w for w, n in self._windows().items() if "transcript" in n],
                         [second])

    def test_an_unstamped_window_is_not_a_viewer_on_this_planes_own_server_either(self):
        """A window carrying `@charter_transcript` and no viewer stamp is left alone on the
        plane's OWN server too — by a close of its chat and by the orphan sweep — even with
        the session's `@charter_plane` naming this very plane. There is no own-server
        exception: a stamp-less window is not a viewer anywhere."""
        chat_pane = self._chat_with_a_transcript()
        sid = self._tmux("display-message", "-p", "-t", chat_pane, "#{session_id}")
        stray = self._tmux("new-window", "-d", "-t", sid, "-n", "transcript alpha_2.1",
                           "-P", "-F", "#{window_id}", "sh", "-c", "exec cat")
        self._tmux("set-option", "-w", "-t", stray, commands_frame._TRANSCRIPT_OPTION,
                   "alpha_2.1")
        self._tmux("set-option", "-t", sid, commands_frame._PLANE_OPTION,
                   str(config.STATE_DIR))
        with mock.patch.object(tmuxctl, "plane_socket", return_value=self.socket):
            self.assertEqual(commands_frame.cmd_close(
                SimpleNamespace(chat="", chat_id="alpha_2.1")), 0)
            self.assertIn(stray, self._windows_now(),
                          "a close killed a stamp-less window on the plane's own server")
            commands_frame._sweep_orphan_transcripts(
                self.socket, commands_frame._live_chats(self.socket))
        self.assertIn(stray, self._windows_now(),
                      "the sweep killed a stamp-less window on the plane's own server")

    def test_the_sweep_leaves_the_viewer_of_a_chat_that_is_still_live(self):
        """A sweep takes orphans only. The chat here is live — its harness pane is running —
        so its viewer is the window somebody may be reading, and `_sweep_orphan_transcripts`
        must leave it exactly where it is."""
        self._chat_with_a_transcript()
        viewer = self._open_a_viewer()
        with mock.patch.object(tmuxctl, "plane_socket", return_value=self.socket):
            commands_frame._sweep_orphan_transcripts(
                self.socket, commands_frame._live_chats(self.socket))
        self.assertIn(viewer, self._windows_now(),
                      "the sweep killed the viewer of a chat that is still live")

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
        with mock.patch.object(tmuxctl, "plane_socket", return_value=self.socket):
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

        with mock.patch.object(tmuxctl, "plane_socket", return_value=self.socket):
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

        with mock.patch.object(tmuxctl, "plane_socket", return_value=self.socket), \
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


@unittest.skipUnless(_HAS_TMUX and shutil.which(commands_frame._PAGER[0]),
                     "needs a real tmux and a pager")
class TwoPlanesTranscriptViewersOnOneSharedServer(PersonaIso, unittest.TestCase):
    """**#933 for the transcript viewer.** On the legacy `charter` socket (and inside an
    operator's `-S` tmux) two planes both hold `default.1`, and `_live_chats` lists EVERY
    window there. A viewer killed by its chat id alone would let plane A's close, quit, or
    own "previous transcript" kill plane B's viewer window. The WINDOW-scoped `@charter_plane`
    marker vetoes that, exactly as `_chat_seats` vetoes the chat kill.

    Red on `01a27eb`, where `_kill_transcript_windows` matched by `@charter_transcript` alone.
    """

    def setUp(self):
        super().setUp()
        make_plane(self)
        self.socket = _tmuxreap.name(f"shared-viewers-{next(_SERVERS)}")
        # The shared server IS the legacy socket here — a per-plane server holds only one
        # plane's windows, so the collision this is about lives on the shared ones.
        self.enterContext(mock.patch.object(tmuxctl, "LEGACY_SOCKET", self.socket))
        self.addCleanup(lambda: subprocess.run(
            ["tmux", "-L", self.socket, "kill-server"], capture_output=True, text=True))
        self.a_root = self.tmp
        self.b_root = self.tmp.parent / f"{self.tmp.name}-plane-b"
        self.b_root.mkdir()
        (self.b_root / "charter.toml").write_text("schema = 1\n")
        self.addCleanup(shutil.rmtree, self.b_root, True)
        self.ws = config.DEFAULT_WORKSPACE
        self.fid = f"{self.ws}.1"

    def _tmux(self, *a, check=True):
        out = subprocess.run(["tmux", "-L", self.socket, *a], capture_output=True,
                             text=True, timeout=20)
        if check:
            self.assertEqual(out.returncode, 0, f"{a}: {out.stderr}")
        return out.stdout.strip()

    def _windows(self):
        out = self._tmux("list-windows", "-a", "-F",
                         "#{window_id}\t#{@charter_transcript}\t#{@charter_viewer_plane}",
                         check=False)
        return {w.split("\t")[0]: w.split("\t") for w in out.splitlines() if "\t" in w}

    def _plant(self, root, *, first: bool) -> tuple[str, str]:
        """Plane *root*'s `default.1` chat window and its transcript viewer, both on the
        shared server, both marked with that plane's own `@charter_plane`. Returns the chat
        pane and the viewer window id."""
        config.use(root)
        plane = str(config.STATE_DIR)
        if first:
            pane = self._tmux("new-session", "-d", "-s", self.ws, "-P", "-F",
                              "#{pane_id}", "-x", "80", "-y", "24", "sh", "-c", "exec cat")
        else:
            pane = self._tmux("new-window", "-d", "-t", self.ws, "-P", "-F",
                              "#{pane_id}", "sh", "-c", "exec cat")
        self._tmux("set-option", "-w", "-t", pane, commands_frame._CHAT_OPTION, self.fid)
        self._tmux("set-option", "-w", "-t", pane, commands_frame._PLANE_OPTION, plane)
        viewer = self._tmux("new-window", "-d", "-t", self.ws, "-n",
                            f"transcript {self.fid}", "-P", "-F", "#{window_id}",
                            "sh", "-c", "exec cat")
        # The viewer's own plane stamp first, then the chat mark — `cmd_transcript`'s order.
        self._tmux("set-option", "-w", "-t", viewer, commands_frame._VIEWER_PLANE_OPTION,
                   plane)
        self._tmux("set-option", "-w", "-t", viewer, commands_frame._TRANSCRIPT_OPTION,
                   self.fid)
        state.frame_dir(self.fid, create=True)
        state.record_server(self.fid, self.socket)
        state.record_workspace(self.fid, self.ws)
        state.record_harness_pane(self.fid, pane)
        state.record_cwd(self.fid, str(root))
        state.record_identity(self.fid, {"CHARTER_HARNESS": "claude-code",
                                         "CHARTER_WORKSPACE": "", "CHARTER_PERSONA": ""})
        return pane, viewer

    def _viewer_of(self, plane_root) -> str:
        want = str(config.derive(plane_root)["STATE_DIR"])
        return next(w for w, cols in self._windows().items()
                    if cols[1] == self.fid and cols[2] == want)

    def test_plane_As_close_leaves_plane_Bs_viewer(self):
        self._plant(self.a_root, first=True)
        self._plant(self.b_root, first=False)
        b_viewer = self._viewer_of(self.b_root)
        config.use(self.a_root)
        self.assertEqual(commands_frame.cmd_close(
            SimpleNamespace(chat="", chat_id=self.fid)), 0)
        self.assertIn(b_viewer, self._windows(),
                      "plane A's close killed plane B's transcript viewer of the same chat id")

    def test_plane_As_quit_leaves_plane_Bs_viewer(self):
        self._plant(self.a_root, first=True)
        self._plant(self.b_root, first=False)
        b_viewer = self._viewer_of(self.b_root)
        config.use(self.a_root)
        commands_frame.cmd_quit(SimpleNamespace(chat=self.fid))
        self.assertIn(b_viewer, self._windows(),
                      "plane A's quit killed plane B's transcript viewer of the same chat id")

    def test_plane_As_own_transcript_leaves_plane_Bs_viewer(self):
        """`cmd_transcript` kills an existing viewer of the same chat before opening a new
        one — and on a shared server that must be its OWN plane's, not plane B's."""
        a_pane, _av = self._plant(self.a_root, first=True)
        self._plant(self.b_root, first=False)
        b_viewer = self._viewer_of(self.b_root)
        config.use(self.a_root)
        config.write_for(reopen.transcript_path(self.fid), "OLD TEXT\n")
        with mock.patch.object(tmuxctl, "plane_socket", return_value=self.socket):
            self.assertEqual(commands_frame.cmd_transcript(
                SimpleNamespace(chat=self.fid)), 0)
        self.assertIn(b_viewer, self._windows(),
                      "plane A opening its own transcript killed plane B's viewer")

    def test_plane_As_own_stamped_viewer_does_die_on_As_close_and_quit(self):
        """**The positive control.** Without it the three "B survives" cases pass on code
        that kills nothing at all. A's own viewer — stamped with A's plane — goes on A's
        close, and on A's quit, while B's stays both times."""
        for verb in ("close", "quit"):
            with self.subTest(verb=verb):
                self.setUp()
                self._plant(self.a_root, first=True)
                self._plant(self.b_root, first=False)
                a_viewer = self._viewer_of(self.a_root)
                b_viewer = self._viewer_of(self.b_root)
                config.use(self.a_root)
                if verb == "close":
                    self.assertEqual(commands_frame.cmd_close(
                        SimpleNamespace(chat="", chat_id=self.fid)), 0)
                else:
                    commands_frame.cmd_quit(SimpleNamespace(chat=self.fid))
                windows = self._windows()
                self.assertNotIn(a_viewer, windows,
                                 f"plane A's {verb} left its own stamped viewer standing")
                self.assertIn(b_viewer, windows, f"plane A's {verb} killed plane B's viewer")
                self.doCleanups()

    def _unstamped_window_in_As_session(self) -> str:
        """A window carrying `@charter_transcript` for A's chat id and NO viewer stamp, in
        the session A created — whose SESSION-level `@charter_plane` is A's. Through tmux's
        window-then-session fallback `#{@charter_plane}` on this window reads A's plane, which
        is exactly what a viewer must never be recognised by."""
        config.use(self.a_root)
        window = self._tmux("new-window", "-d", "-t", self.ws, "-n",
                            f"transcript {self.fid}", "-P", "-F", "#{window_id}",
                            "sh", "-c", "exec cat")
        self._tmux("set-option", "-w", "-t", window, commands_frame._TRANSCRIPT_OPTION,
                   self.fid)
        # The session marker, as the launch that CREATED the session wrote it.
        self._tmux("set-option", "-t", self.ws, commands_frame._PLANE_OPTION,
                   str(config.STATE_DIR))
        return window

    def test_an_unstamped_window_survives_As_close_quit_and_transcript_on_a_shared_server(self):
        """**Ruling: a window that never got both options is not a viewer.** It is never
        swept and never killed — not even though its session's own `@charter_plane` is A's,
        which a `#{@charter_plane}` read would have handed A as the window's plane."""
        for verb in ("close", "quit", "transcript"):
            with self.subTest(verb=verb):
                self.setUp()
                self._plant(self.a_root, first=True)
                stray = self._unstamped_window_in_As_session()
                config.use(self.a_root)
                if verb == "close":
                    commands_frame.cmd_close(SimpleNamespace(chat="", chat_id=self.fid))
                elif verb == "quit":
                    commands_frame.cmd_quit(SimpleNamespace(chat=self.fid))
                else:
                    config.write_for(reopen.transcript_path(self.fid), "OLD TEXT\n")
                    with mock.patch.object(tmuxctl, "plane_socket", return_value=self.socket):
                        commands_frame.cmd_transcript(SimpleNamespace(chat=self.fid))
                self.assertIn(stray, self._windows(),
                              f"plane A's {verb} killed a window carrying only the chat mark")
                self.doCleanups()


if __name__ == "__main__":       # pragma: no cover
    unittest.main()
