"""A run that was killed leaves a tmux server running; the next run is what ends it.

`tests/_tmuxreap.py` carries the measurement that produced this: a clean run of
`test_frame_overlay_escape_hatch` leaves the socket directory smaller than it found it, and
the same run `kill -9`'d two seconds in leaves a live server and its socket file behind.
So the leak is not a missing `addCleanup` — it is the signal that skips every `addCleanup`
there is, and the deletion sweep sends one every time a mutation makes the suite hang.

**Every case here is a control, in `test_no_test_reads_the_operators_shell`'s sense.** The
reap is proved on a REAL tmux server planted on a socket named for a dead pid — killed and
unlinked — and the refusals are proved on real files too: a live pid's socket, a name
outside charter's namespace, and the operator's own ``charter``. A reaper nobody has
watched decline is a reaper nobody knows is safe to run from every suite process at once.
"""

from __future__ import annotations

import ast
import os
import shutil
import socket
import subprocess
import tempfile
import time
import unittest
from pathlib import Path
from unittest import mock

from charter import commands_frame
from tests import _tmuxreap

_HAS_TMUX = shutil.which("tmux") is not None

#: Where this module puts a temp directory it will BIND a socket in.
#:
#: Not `$TMPDIR`. An `AF_UNIX` path is capped at 104 bytes on macOS, and macOS spells
#: `$TMPDIR` `/var/folders/<two>/<28 chars>/T/` — which leaves 40 for a prefix, a random
#: suffix, a `tmux-<uid>` directory and a socket name, and does not fit. `bind` then
#: fails with `OSError: AF_UNIX path too long`, which is a fact about this machine's
#: temp directory and not about anything under test. tmux picks `/tmp` for its own
#: sockets for the same reason.
_SHORT_TMP = "/tmp"


def _really_gone(pid: int) -> bool:
    """Whether *pid* is gone, asked of the kernel directly.

    **Deliberately not `_tmuxreap._alive`, and a mutation is why.** `_alive` is the thing
    under test here; a fixture that used it to decide whether it had a usable pid would
    ask the mutant whether the mutant was working. Measured: with `_alive`'s
    ``except ProcessLookupError`` narrowed to something nothing raises — so every dead pid
    reports as alive — the case below did not fail, it SKIPPED, and a skip is a green.
    """
    try:
        os.kill(pid, 0)
    except ProcessLookupError:
        return True
    except OSError:
        return False
    return False


def _a_pid_that_is_gone(case) -> int:
    """The pid of a process this test started and waited for.

    Not a large constant: ``pid_max`` differs per platform, and a number picked for looking
    "obviously too big" is a number that becomes somebody's shell one day. Confirmed gone
    rather than assumed gone — a waited-for pid is free for reuse the instant it is reaped,
    and this machine runs several suites at once.

    **It FAILS rather than skipping when it cannot find one, and that is not pedantry.**
    The first version called `skipTest`, and a skip is a green: three separate mutations to
    `_really_gone` above — narrowing either `except`, or deleting the branch that returns
    the pid — make every candidate look alive, so the loop runs out and every case that
    depends on this helper reports success without running. A fixture whose failure mode is
    a skip cannot be the thing a guard's tests stand on. Twenty consecutive reuses of pids
    this process itself freed is not an environment charter has to tolerate; it is this
    helper being broken, and it should say so.
    """
    for _ in range(20):
        proc = subprocess.Popen(["/bin/sh", "-c", "exit 0"])
        proc.wait()
        if _really_gone(proc.pid):
            return proc.pid
    raise AssertionError(
        "twenty processes this case started, waited for and reaped all still answer "
        "`os.kill(pid, 0)`. Either this machine reused every one of those pids "
        "immediately — which nothing here has ever seen — or `_really_gone` has stopped "
        "recognising a dead pid, which is the state three mutations to it produce and the "
        "reason this raises instead of skipping.")


class TheNameIsWhatMakesASocketReapable(unittest.TestCase):
    """The rule, and the three shapes it must refuse."""

    def test_what_the_helper_produces_is_what_the_reaper_recognises(self):
        """The derivation, not its contents: a slug nobody has invented yet is reapable on
        the commit that invents it, because the producer and the pattern live together."""
        for slug in ("integration-test", "overlay-hatch", "palette-integ", "a", "x9-y7"):
            with self.subTest(slug=slug):
                made = _tmuxreap.name(slug)
                self.assertTrue(_tmuxreap.owns(made), made)
                self.assertTrue(made.endswith(f"-{os.getpid()}"))

    def test_the_operators_own_frame_socket_is_not_ours(self):
        """`commands_frame.SOCKET` — asked of production, not spelled — is the socket the
        operator's live frame runs on, and three of them were on this machine while #564
        was being measured. It carries no pid, so the rule cannot reach it; this is the
        case that fails if the rule is ever loosened to a bare prefix."""
        self.assertFalse(_tmuxreap.owns(commands_frame.SOCKET))

    def test_a_name_outside_charters_namespace_is_not_ours(self):
        """`probe-menu-80053` and friends were on this machine too, left by hand-run probe
        scripts. They are not charter's to remove — `_envguard` makes the same call about
        an ``EDM_`` prefix, and for the same reason: a guard that reaches sideways into
        names charter does not own is a guard that deletes somebody else's work.

        The last three are the ones a mutation found. ``charter-integration-test-`` — the
        prefix with the pid missing — is what a rule spelt ``(\\d*)`` accepts, and
        `reapable` would then hand ``int("")`` a `ValueError` on its way to deleting it.
        The newline pair is what ``$`` accepts and `fullmatch` does not: ``$`` matches
        before a trailing newline, and a filename may end in one.
        """
        for outside in ("probe-menu-80053", "default", "tmux-502", "charter-sk2",
                        "charter", "charter-", "notcharter-integration-test-1",
                        "charter-integration-test-", "charter-integration-test-1\n",
                        "\ncharter-integration-test-1"):
            with self.subTest(name=outside):
                self.assertFalse(_tmuxreap.owns(outside))


class WhetherAPidIsStillThere(unittest.TestCase):
    """`_alive` decides everything the reaper does, so it is asked directly.

    Reaching it only through `reap()` left two of its lines unpinned, and the covering
    case answered a mutation with a SKIP rather than a failure (see :func:`_really_gone`).
    A predicate this load-bearing gets its own cases.
    """

    def test_a_pid_that_is_gone_reads_as_gone(self):
        self.assertFalse(_tmuxreap._alive(_a_pid_that_is_gone(self)))

    def test_this_process_reads_as_alive(self):
        self.assertTrue(_tmuxreap._alive(os.getpid()))

    def test_pid_zero_is_answered_without_signalling_anything(self):
        """``os.kill(0, …)`` is a PROCESS GROUP operation — it addresses every process in
        this one's group, which is the suite runner and every child it has. Signal 0 makes
        that a permission check rather than a delivery, so the answer would come back
        "alive" either way; the guard is there so the reaper never makes that call at all.
        A mutation deleting it survived every test that went through `reap()`, because a
        socket named ``charter-x-0`` is not a thing that exists on disk to be reaped.
        """
        calls = []
        with mock.patch("os.kill", side_effect=lambda *a: calls.append(a)):
            self.assertTrue(_tmuxreap._alive(0))
            self.assertTrue(_tmuxreap._alive(-1))
        self.assertEqual(calls, [], "the reaper signalled a process group")

    def test_a_pid_that_belongs_to_somebody_else_reads_as_alive(self):
        """The one answer that decides in the SAFE direction, and the one no fixture on
        this machine can produce: `os.kill` raises `PermissionError` for a pid that exists
        and is not ours. The pid exists, so the run that named it may still be going, so
        the reaper must not touch its socket. Patched rather than staged, because staging
        it needs a second user account — which is exactly the "unreachable on the platform
        the sweep ran on" case `tools/sweep.py` names for a `narrow-except` survivor."""
        with mock.patch("os.kill", side_effect=PermissionError(1, "not yours")):
            self.assertTrue(_tmuxreap._alive(4242))

    def test_an_unreadable_answer_also_reads_as_alive(self):
        """Any other `OSError` lands the same way, for the same reason: not knowing is not
        a licence to delete."""
        with mock.patch("os.kill", side_effect=OSError(999, "who knows")):
            self.assertTrue(_tmuxreap._alive(4242))


class TheScanRefusesEverythingItCannotBeSureOf(unittest.TestCase):
    """`reapable` walks a directory it does not own, and every branch there decides
    whether something gets DELETED. Each one is asked directly, because reaching them
    through `reap()` needs a machine in a state no fixture can arrange, and a branch only
    a real leak can reach is a branch nothing pins.
    """

    def _own_dir(self) -> Path:
        """A socket directory of this case's own, so nothing here can see the machine's."""
        tmp = Path(tempfile.mkdtemp(prefix="charter-reapscan-", dir=_SHORT_TMP))
        self.addCleanup(shutil.rmtree, tmp, True)
        home = tmp / f"tmux-{os.getuid()}"
        home.mkdir()
        self.enterContext(mock.patch.dict(os.environ, {"TMUX_TMPDIR": str(tmp)}))
        return home

    def test_a_missing_socket_directory_is_not_an_error(self):
        """A machine that has never run tmux has no such directory, and the suite must
        still start. Nothing was ever left there, so there is nothing to reap."""
        tmp = Path(tempfile.mkdtemp(prefix="charter-reapscan-", dir=_SHORT_TMP))
        self.addCleanup(shutil.rmtree, tmp, True)
        with mock.patch.dict(os.environ, {"TMUX_TMPDIR": str(tmp / "never-created")}):
            self.assertEqual(_tmuxreap.reapable(), [])
            self.assertEqual(_tmuxreap.reap(), [])

    def test_a_plain_file_with_a_reapable_name_is_not_reaped(self):
        """`S_ISSOCK`, and it is not decoration: the name rule is about NAMES, and a
        directory somebody keeps notes in can hold a file called anything at all. A reaper
        that deleted by name alone would delete those."""
        home = self._own_dir()
        decoy = home / f"charter-reaper-probe-{_a_pid_that_is_gone(self)}"
        decoy.write_text("not a socket\n")
        self.assertEqual(_tmuxreap.reapable(), [])
        self.assertEqual(_tmuxreap.reap(), [])
        self.assertTrue(decoy.exists(), "the reaper deleted a file that is not a socket")

    def test_an_entry_that_cannot_be_stat_ed_is_skipped_rather_than_reaped(self):
        """The race the scan is walking into: a file listed a moment ago and gone now, or
        one whose directory turns unreadable mid-walk. Not knowing what something is, is
        not a licence to delete it."""
        home = self._own_dir()
        (home / f"charter-reaper-probe-{_a_pid_that_is_gone(self)}").write_text("x")
        with mock.patch("os.stat", side_effect=OSError(5, "I/O error")):
            self.assertEqual(_tmuxreap.reapable(), [])

    def test_a_name_that_does_not_match_at_all_is_passed_over(self):
        """The `continue` before the pid is parsed. Without it the scan reaches
        `matched.group(1)` on `None` and dies on the first unrelated file in a directory
        every tmux on the machine shares."""
        home = self._own_dir()
        (home / "default").write_text("somebody else's")
        (home / "probe-menu-80053").write_text("somebody else's")
        self.assertEqual(_tmuxreap.reapable(), [])


class TheListeningProbe(unittest.TestCase):
    """One `AF_UNIX` connect stands between a socket file and a `kill-server`."""

    def test_a_path_with_nothing_at_it_is_not_listening(self):
        tmp = Path(tempfile.mkdtemp(prefix="charter-reapprobe-", dir=_SHORT_TMP))
        self.addCleanup(shutil.rmtree, tmp, True)
        self.assertFalse(_tmuxreap._listening(tmp / "nothing-here"))

    def test_a_bound_socket_with_nobody_accepting_is_not_listening(self):
        """What a leftover file looks like once its server is gone: bound, on disk, and
        refusing every connection."""
        tmp = Path(tempfile.mkdtemp(prefix="charter-reapprobe-", dir=_SHORT_TMP))
        self.addCleanup(shutil.rmtree, tmp, True)
        sock = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
        self.addCleanup(sock.close)
        sock.bind(str(tmp / "s"))
        self.assertFalse(_tmuxreap._listening(tmp / "s"))

    def test_a_socket_that_accepts_is_listening(self):
        tmp = Path(tempfile.mkdtemp(prefix="charter-reapprobe-", dir=_SHORT_TMP))
        self.addCleanup(shutil.rmtree, tmp, True)
        sock = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
        self.addCleanup(sock.close)
        sock.bind(str(tmp / "s"))
        sock.listen(1)
        self.assertTrue(_tmuxreap._listening(tmp / "s"))

    def test_an_error_that_is_not_a_refusal_is_read_as_maybe(self):
        """The safe direction, and the reason it is written as a set membership rather
        than a bare `return False`: a timeout, an `EACCES`, anything that is not "nobody is
        bound" leaves a `kill-server` on the table, because the thing this module is about
        is a server that outlived the process named in its socket."""
        with mock.patch("socket.socket") as made:
            made.return_value.connect.side_effect = OSError(60, "timed out")
            self.assertTrue(_tmuxreap._listening(Path("/whatever")))


class TheReapNeverRaises(unittest.TestCase):
    """This runs at import of the `tests` package, so it is the suite's own boot.

    A machine whose socket directory has turned hostile mid-reap must still be able to run
    the suite: the cost of a skipped reap is the backlog #564 found, and the cost of
    raising here is no suite at all. Asked directly, because neither branch can be reached
    by arranging files on a working machine.
    """

    def _a_stale_file(self) -> Path:
        """A bound `AF_UNIX` socket with nobody accepting — what 497 of the files on this
        machine were. Deliberately no `listen()`: a connect gets `ECONNREFUSED`, which is
        the positive "nobody is bound here" answer, so `reap` goes straight to the unlink
        without spending a `tmux` invocation."""
        tmp = Path(tempfile.mkdtemp(prefix="charter-reapraise-", dir=_SHORT_TMP))
        self.addCleanup(shutil.rmtree, tmp, True)
        home = tmp / f"tmux-{os.getuid()}"
        home.mkdir()
        self.enterContext(mock.patch.dict(os.environ, {"TMUX_TMPDIR": str(tmp)}))
        path = home / f"charter-reaper-probe-{_a_pid_that_is_gone(self)}"
        sock = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
        self.addCleanup(sock.close)
        sock.bind(str(path))
        self.assertFalse(_tmuxreap._listening(path))
        self.assertEqual([p.name for p in _tmuxreap.reapable()], [path.name])
        return path

    def test_a_file_that_cannot_be_unlinked_is_reported_as_not_removed(self):
        """And does not stop the reap, or the suite behind it. Reported honestly rather
        than optimistically: `reap` returns what it actually removed, which is what
        `test_nothing_that_was_reapable_survives_the_reap` measures against."""
        path = self._a_stale_file()
        with mock.patch("pathlib.Path.unlink", side_effect=OSError(13, "read-only")):
            self.assertEqual(_tmuxreap.reap(), [])
        self.assertTrue(path.exists())

    def test_a_stale_file_with_nobody_behind_it_costs_no_tmux_invocation(self):
        """The 497-files case, and the reason `_listening` exists at all: running
        `tmux … kill-server` once per candidate would have been 497 subprocesses on this
        machine's first run."""
        path = self._a_stale_file()
        with mock.patch("subprocess.run") as never:
            self.assertEqual(_tmuxreap.reap(), [path.name])
        never.assert_not_called()
        self.assertFalse(path.exists())


class TheReaperRunsOnceAtStart(unittest.TestCase):
    """`install` is called from `tests/__init__`, and only the first call does anything."""

    def setUp(self) -> None:
        self.addCleanup(setattr, _tmuxreap, "_installed", _tmuxreap._installed)

    def test_a_second_install_reaps_nothing(self):
        """Idempotent, the way `_planeguard.install` and `_envguard.install` are: a child
        process importing the package while a parent already did must not pay for a second
        directory walk, and a test calling it must not silently re-reap."""
        with mock.patch.object(_tmuxreap, "reap") as spy:
            _tmuxreap.install()
        spy.assert_not_called()

    def test_a_machine_with_no_tmux_does_not_even_look(self):
        """Nothing can have started a server, so nothing can be waiting to be reaped, and
        a machine without tmux should not pay a directory scan to find that out."""
        _tmuxreap._installed = False
        with mock.patch.object(_tmuxreap.shutil, "which", return_value=None), \
                mock.patch.object(_tmuxreap, "reap") as spy:
            _tmuxreap.install()
        spy.assert_not_called()

    def test_the_first_install_on_a_machine_with_tmux_reaps(self):
        """The control for the two above: without it they would both be green on an
        `install` that had been gutted entirely."""
        _tmuxreap._installed = False
        with mock.patch.object(_tmuxreap.shutil, "which", return_value="/usr/bin/tmux"), \
                mock.patch.object(_tmuxreap, "reap") as spy:
            _tmuxreap.install()
        spy.assert_called_once_with()


class WhereTmuxPutsItsSockets(unittest.TestCase):
    """`socket_dir` follows tmux's own rule, and a test says so rather than a comment."""

    def test_tmux_tmpdir_wins_when_the_environment_names_one(self):
        with mock.patch.dict(os.environ, {"TMUX_TMPDIR": "/somewhere/else"}):
            self.assertEqual(_tmuxreap.socket_dir(),
                             Path("/somewhere/else") / f"tmux-{os.getuid()}")

    def test_without_it_the_directory_is_the_one_under_tmp(self):
        env = dict(os.environ)
        env.pop("TMUX_TMPDIR", None)
        with mock.patch.dict(os.environ, env, clear=True):
            self.assertEqual(_tmuxreap.socket_dir(),
                             Path("/tmp") / f"tmux-{os.getuid()}")

    def test_an_empty_value_is_not_a_directory(self):
        """``TMUX_TMPDIR=`` is what an exported-but-unset variable looks like, and joining
        an empty string would point the reaper at ``tmux-<uid>`` relative to the cwd —
        every suite run's own checkout. tmux itself falls back on empty, which is why this
        is written ``or`` and not ``get(…, "/tmp")``."""
        with mock.patch.dict(os.environ, {"TMUX_TMPDIR": ""}):
            self.assertEqual(_tmuxreap.socket_dir(),
                             Path("/tmp") / f"tmux-{os.getuid()}")


@unittest.skipUnless(_HAS_TMUX, "tmux is not installed on this machine")
class TheReaperEndsWhatAKilledRunLeftRunning(unittest.TestCase):
    """A real server on a real socket, killed and unlinked — or deliberately not."""

    def _plant(self, pid: int) -> tuple[str, Path]:
        """Start a real tmux server on a socket named for *pid*, and return both."""
        name = f"charter-reaper-probe-{pid}"
        path = _tmuxreap.socket_dir() / name
        started = subprocess.run(
            ["tmux", "-L", name, "-f", "/dev/null", "new-session", "-d", "-s", "s",
             "-x", "80", "-y", "24", "cat"],
            capture_output=True, text=True, timeout=30)
        self.assertEqual(started.returncode, 0, started.stderr)
        self.addCleanup(self._make_sure_it_is_gone, name, path)
        self.assertTrue(path.exists(), f"tmux did not put a socket at {path}")
        return name, path

    @staticmethod
    def _make_sure_it_is_gone(name: str, path: Path) -> None:
        """Belt and braces for the cases that expect the reaper NOT to act — this module
        must not become the thing it is about."""
        subprocess.run(["tmux", "-L", name, "kill-server"], stdout=subprocess.DEVNULL,
                       stderr=subprocess.DEVNULL, timeout=30, check=False)
        try:
            path.unlink()
        except OSError:
            pass

    def _server_pid(self, name: str) -> int:
        """The pid of the tmux server on *name*, asked of tmux.

        `#{pid}` is the server's own process id — the thing that outlives the run and holds
        the session, and therefore the thing "was it killed" has to be asked about.
        """
        said = subprocess.run(["tmux", "-L", name, "display-message", "-p", "#{pid}"],
                              capture_output=True, text=True, timeout=30)
        self.assertEqual(said.returncode, 0, said.stderr)
        return int(said.stdout.strip())

    def test_a_dead_runs_server_is_killed_and_its_socket_removed(self):
        """The whole issue, run rather than described: `kill-server` FIRST, unlink second,
        and both — a `kill-server` that returns 0 leaves the file (#554), and an unlink
        without a kill leaves a resident tmux holding a session for two days (#564).

        **The server's death is asserted against its PROCESS, and it has to be.** The first
        version of this asked the socket instead — "is anything still accepting on that
        path" — which a reaper that only unlinks satisfies for free, because a path with no
        file gives `ECONNREFUSED` exactly like a path with no server. Measured: with the
        `kill-server` deleted outright, this case stayed green. That is #564's own failure
        mode reproduced inside the test written to prevent it, so the claim now names a pid
        and watches it go.
        """
        pid = _a_pid_that_is_gone(self)
        name, path = self._plant(pid)
        self.assertTrue(_tmuxreap._listening(path), "the planted server never came up")
        server = self._server_pid(name)

        removed = _tmuxreap.reap()

        self.assertIn(name, removed)
        self.assertFalse(path.exists(), f"{path} survived the reap")
        deadline = time.monotonic() + 10.0
        while time.monotonic() < deadline and _tmuxreap._alive(server):
            time.sleep(0.05)
        self.assertFalse(
            _tmuxreap._alive(server),
            f"the socket file is gone and tmux {server} is still running — an unlink "
            f"without a kill is what leaves the resident processes #564 counted 14 of, "
            f"and it hides them by removing the only file that named them")

    def test_a_socket_whose_name_merely_STARTS_like_ours_is_left_alone(self):
        """`reapable` scans a directory, and a directory scan is where "ours" has to mean
        the WHOLE name. ``charter-reaper-probe-<dead pid>-something-else`` begins with a
        name this suite could have handed out and is not one; a rule asked with `re.match`
        says yes to it and deletes somebody else's socket. Measured as a survivor —
        `owns()` had moved to `fullmatch` and the scan had not, and every other case here
        stayed green because none of them put such a file on disk.

        A real `AF_UNIX` socket, bound here rather than started by tmux, because
        `reapable` refuses anything that is not a socket and a plain file would pass this
        for the wrong reason.
        """
        pid = _a_pid_that_is_gone(self)
        path = _tmuxreap.socket_dir() / f"charter-reaper-probe-{pid}-and-then-some"
        sock = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
        self.addCleanup(sock.close)
        self.addCleanup(lambda: path.unlink(missing_ok=True))
        sock.bind(str(path))

        self.assertNotIn(path, _tmuxreap.reapable())
        self.assertNotIn(path.name, _tmuxreap.reap())
        self.assertTrue(path.exists(), f"{path} is not a name this suite hands out, and "
                                       f"the reaper deleted it anyway")

    def test_a_kill_that_did_not_take_leaves_the_file_that_names_the_server(self):
        """The ordering argument turned round, and this module's own first draft got it
        wrong. Unlinking after a kill that TOOK is #554's fix — `kill-server` returns 0
        with the socket still bound for a measured 0.4 ms, and removing the file is what
        closes that window. Unlinking after a kill that did NOT take is #564 with the
        evidence destroyed: a resident tmux server holding a session, and nothing left on
        disk naming it, so no later run can find it either.

        Measured rather than reasoned. While this module was being written, 24 servers
        accumulated on this machine exactly that way — alive, with their socket files
        already gone, unreapable by anything short of `ps`. A server that refuses to die
        keeps its file and is tried again on the next run, which costs one more reap and
        loses nothing.
        """
        name, path = self._plant(_a_pid_that_is_gone(self))
        for failing in (mock.Mock(side_effect=OSError(2, "no tmux")),
                        mock.Mock(side_effect=subprocess.TimeoutExpired("tmux", 10)),
                        mock.Mock(return_value=subprocess.CompletedProcess([], 1))):
            with self.subTest(kill=failing._mock_side_effect or "exit 1"):
                with mock.patch("subprocess.run", failing):
                    self.assertEqual(_tmuxreap.reap(), [])
                self.assertTrue(
                    path.exists(),
                    "the file naming a server that is still running was removed, which "
                    "leaves the server and takes away the only way to find it")
        self.assertTrue(_tmuxreap._listening(path), "the planted server died on its own")

    def test_a_live_runs_socket_is_left_alone(self):
        """What makes this safe to run from every process that imports the `tests` package,
        two suite runs at once included. Planted under THIS process's pid, which is as
        alive as a pid gets."""
        name, path = self._plant(os.getpid())

        removed = _tmuxreap.reap()

        self.assertNotIn(name, removed)
        self.assertTrue(path.exists(), f"{path} was reaped out from under a live run")
        self.assertTrue(_tmuxreap._listening(path), "a live run's server was killed")

    def test_nothing_that_was_reapable_survives_the_reap(self):
        """The zero, and it is stated against what was there BEFORE rather than against
        the directory afterwards. Several suites run on this machine at once and one of
        them can be killed while this case is between two calls; a socket that arrived
        after the reap is not something the reap failed to remove, and asserting an empty
        directory would make this case fail for the very event it exists to describe."""
        before = {p.name for p in _tmuxreap.reapable()}
        _tmuxreap.reap()
        survived = before & {p.name for p in _tmuxreap.reapable()}
        self.assertEqual(survived, set(),
                         f"{sorted(survived)} was reapable before the reap and still is")

    def test_reaping_twice_removes_nothing_the_second_time(self):
        pid = _a_pid_that_is_gone(self)
        name, _ = self._plant(pid)
        self.assertIn(name, _tmuxreap.reap())
        self.assertNotIn(name, _tmuxreap.reap())


class EverySocketTheSuiteStartsGoesThroughTheHelper(unittest.TestCase):
    """The pin that stops the next module leaking a socket nobody reaps.

    Four modules each spelled ``f"charter-…-{os.getpid()}"`` before this. Four spellings of
    one rule is four chances for the fifth to be written a way the reaper does not
    recognise — and the reaper is the only thing that cleans up after a killed run.
    """

    def test_the_modules_that_start_a_server_name_it_through_the_helper(self):
        """Asked of the module attribute, so a module that reverts to an f-string producing
        the same text still passes this one — and fails the case below."""
        from tests import (test_frame_overlay_escape_hatch, test_frame_palette_integration,
                           test_frame_tmux_integration)
        for module, attr in ((test_frame_tmux_integration, "SOCKET"),
                             (test_frame_tmux_integration, "OP_SOCKET"),
                             (test_frame_overlay_escape_hatch, "SOCKET"),
                             (test_frame_palette_integration, "SOCKET")):
            with self.subTest(module=module.__name__, attribute=attr):
                self.assertTrue(_tmuxreap.owns(getattr(module, attr)))

    @staticmethod
    def _hand_built(source: str) -> list[str]:
        """``NAME`` for every module-level ``*SOCKET`` bound to an f-string in *source*.

        Parsed, not grepped: a `mock.patch("...SOCKET")` in a docstring is a string. An
        f-string is what "built by hand" looks like — `f"charter-overlay-hatch-{os.getpid()}"`
        is how all four of the leaking modules spelled it — and `SOCKET_PATH` is exempt
        because that is a PATH derived from a name, and the name is what this is about.

        A function of its own so that :meth:`test_it_recognises_a_hand_built_name` can hand
        it one and watch it answer. Without that control, a reader that quietly stopped
        recognising anything would report "no offenders" for the best of reasons — measured:
        the `drop-if` mutation that deletes the collecting branch survived the version of
        this case that only ever ran the reader over a clean tree.
        """
        found = []
        for node in ast.parse(source).body:
            if not isinstance(node, ast.Assign):
                continue
            for target in node.targets:
                if (isinstance(target, ast.Name) and target.id.endswith("SOCKET")
                        and isinstance(node.value, ast.JoinedStr)):
                    found.append(target.id)
        return found

    def test_it_recognises_a_hand_built_name(self):
        """The control, and the one shape it must see: exactly what the four modules that
        leaked used to say."""
        self.assertEqual(
            self._hand_built('import os\nSOCKET = f"charter-overlay-hatch-{os.getpid()}"\n'),
            ["SOCKET"])
        self.assertEqual(
            self._hand_built('OP_SOCKET = f"charter-integration-operator-{PID}"\n'),
            ["OP_SOCKET"])

    def test_it_passes_over_the_shapes_that_are_not_one(self):
        """The other half of the control: a reader that answered "offender" to everything
        would satisfy the case above and fail every module in the tree."""
        for benign in ('from tests import _tmuxreap\nSOCKET = _tmuxreap.name("x")\n',
                       'SOCKET = "charter"\n',
                       'SOCKET_PATH = f"/tmp/tmux-{UID}/{SOCKET}"\n',
                       'def f():\n    SOCKET = f"charter-x-{1}"\n'):
            with self.subTest(source=benign):
                self.assertEqual(self._hand_built(benign), [])

    def test_no_module_builds_a_socket_name_by_hand(self):
        """The census itself, over every test module in this directory."""
        tree = Path(__file__).resolve().parent
        offenders = []
        for path in sorted(tree.glob("test_*.py")):
            # Not wrapped in a try/except: a module under `tests/` that will not parse is
            # a defect, and skipping it would leave whatever socket it names unreapable
            # while this case carried on looking healthy.
            offenders += [f"{path.name}:{name}"
                          for name in self._hand_built(path.read_text(encoding="utf-8"))]
        self.assertEqual(
            offenders, [],
            f"{offenders} builds a tmux socket name by hand. Use "
            f"`tests._tmuxreap.name(\"<slug>\")`: the reaper recognises what that "
            f"function produces, and a socket it does not recognise is one nobody cleans "
            f"up after the next killed run (#564).")


if __name__ == "__main__":       # pragma: no cover
    unittest.main()
