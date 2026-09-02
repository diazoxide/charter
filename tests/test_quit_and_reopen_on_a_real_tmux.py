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

        with mock.patch.object(commands_frame, "SOCKET", self.socket):
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

        with mock.patch.object(commands_frame, "SOCKET", self.socket):
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

        with mock.patch.object(commands_frame, "SOCKET", self.socket), \
                mock.patch.object(commands_frame, "_say_on_screen") as said:
            self.assertEqual(commands_frame.cmd_transcript(
                SimpleNamespace(chat="alpha_2.1")), 0)

        self.assertEqual(self._windows(), before)
        said.assert_called_once()
        self.assertEqual(said.call_args[0][1], commands_frame.NO_TRANSCRIPT)

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
