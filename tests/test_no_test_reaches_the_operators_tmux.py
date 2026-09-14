"""No test may start a tmux that talks to the operator's own server.

**The incident (2026-09-12).** An unfinished edit to the profile selector's cancel path
deleted a guard, and a test run from inside a live chat sent `kill-window -t ''` to
charter's own server — `-L charter`, the socket the operator's frame runs on — and closed
their session. Measured on tmux 3.7c on an isolated socket: an empty target exits 0 and
kills the ACTIVE window. `_envguard` had scrubbed `$TMUX`, and it did not matter, because
charter addresses its own server by name and a name resolves under `/tmp` whatever the
environment says.

So `tests._planeguard.RealTmuxReach` refuses the SPAWN, judged on the socket the child would
connect to rather than on the words: the legacy `charter` socket and `default` under the
suite's own `$TMUX_TMPDIR`, whatever `$TMUX` named when the suite started, and — since
ruling 46 gave every plane a server of its own — the real plane's `charter-plane-<hex>` and
every other `charter-plane-<12 hex>` in that directory, because another project's frame is
somebody's live work too. Every case here drives it
with a stand-in `tmux` that only writes a marker, so no real server is involved, and a
marker that stays absent is the evidence that nothing ran.
"""

from __future__ import annotations

import os
import pathlib
import shutil
import subprocess
import tempfile
import unittest

from charter import config
from charter.frame import tmuxctl
from tests import _planeguard


def _socket(name: str, tmpdir: str = "/tmp") -> str:
    return os.path.realpath(os.path.join(tmpdir, f"tmux-{os.getuid()}", name))


class TheOperatorsServerIsNeverReached(unittest.TestCase):

    def setUp(self):
        self.dir = pathlib.Path(tempfile.mkdtemp(prefix="tmuxguard-"))
        self.addCleanup(shutil.rmtree, self.dir, True)
        self.marker = self.dir / "the-tmux-ran"
        self.tmux = self.dir / "tmux"
        self.tmux.write_text(f"#!/bin/sh\necho ran >> {self.marker}\n")
        self.tmux.chmod(0o755)
        self.charter = _socket("charter")
        self.default = _socket("default")

    def _refused(self, *words: str, env=None) -> _planeguard.RealTmuxReach:
        with self.assertRaises(_planeguard.RealTmuxReach) as caught:
            subprocess.run([str(self.tmux), *words], env=env)
        self.assertFalse(self.marker.exists(), "refused, and yet it ran")
        return caught.exception

    def _allowed(self, *words: str, env=None) -> None:
        # A hand-built environment carries the suite's background-checks switch, or
        # `BackgroundGrandchild` refuses it before this guard's answer can be read (#945).
        # Adding one name moves no socket: `_tmux_socket` reads `$TMUX` and `$TMUX_TMPDIR`.
        if env is not None:
            env = {**env, "CHARTER_NO_BACKGROUND_CHECKS": "1"}
        subprocess.run([str(self.tmux), *words], env=env, check=True)
        self.assertTrue(self.marker.exists(), "allowed, and yet it did not run")

    # --- the guard is there, and reads production -------------------------------------

    def test_the_sockets_are_resolved_when_the_suite_starts(self):
        self.assertIn(_socket("charter", os.environ.get("TMUX_TMPDIR") or "/tmp"),
                      _planeguard._OPERATOR_TMUX)
        self.assertIn(_socket("default", os.environ.get("TMUX_TMPDIR") or "/tmp"),
                      _planeguard._OPERATOR_TMUX)

    def test_the_charter_socket_is_the_legacy_one_frames_still_run_on(self):
        self.assertEqual(_planeguard._CHARTER_SOCKET, tmuxctl.LEGACY_SOCKET)

    def test_the_plane_pattern_is_productions(self):
        self.assertEqual(_planeguard._PLANE_SOCKET.pattern, tmuxctl.PLANE_SOCKET_RE.pattern)

    def test_the_real_planes_own_server_is_one_of_them(self):
        """Computed with production's function against the state directory the suite
        started in — `install` asks before any test isolates it. Recomputed here by pointing
        `config` back at that directory, which is the only way to ask the same question of
        the same input from inside a test."""
        self.assertTrue(_planeguard._REAL, "the guard recorded no real state directory")
        real = _planeguard._REAL[0]
        previous = config.STATE_DIR
        config.STATE_DIR = pathlib.Path(real)
        try:
            name = tmuxctl.plane_socket()
        finally:
            config.STATE_DIR = previous
        self.assertIn(_socket(name, os.environ.get("TMUX_TMPDIR") or "/tmp"),
                      _planeguard._OPERATOR_TMUX)

    def test_the_operators_own_tmux_is_one_of_them_when_the_suite_started_inside_it(self):
        found = _planeguard._operator_tmux(
            {"TMUX": "/somewhere/tmux-1/work,123,0", "TMUX_TMPDIR": "/elsewhere"},
            "charter-plane-0123456789ab")
        self.assertEqual(found, {os.path.realpath("/somewhere/tmux-1/work"),
                                 _socket("charter", "/elsewhere"),
                                 _socket("default", "/elsewhere"),
                                 _socket("charter-plane-0123456789ab", "/elsewhere")})

    def test_without_tmux_set_or_a_plane_there_are_exactly_two(self):
        self.assertEqual(_planeguard._operator_tmux({}), {self.charter, self.default})

    def test_it_is_a_base_exception_so_a_fallback_cannot_eat_it(self):
        """`tmuxctl.run` turns a failed call into a return code, and a tripwire something
        catches reports a server that is not there instead of failing."""
        self.assertTrue(issubclass(_planeguard.RealTmuxReach, BaseException))
        self.assertFalse(issubclass(_planeguard.RealTmuxReach, Exception))

    # --- what reaches it --------------------------------------------------------------

    def test_charters_own_socket_by_name_is_refused_from_a_hand_built_environment(self):
        """The incident's shape: a cleared environment, and a name that resolves under
        `/tmp` whatever `_envguard` scrubbed."""
        self._refused("-L", "charter", "kill-window", "-t", "", env={})

    def test_the_label_attached_to_its_flag_is_refused(self):
        self._refused("-Lcharter", "ls", env={})

    def test_a_bundle_of_flags_ending_in_the_label_is_refused(self):
        self._refused("-2uL", "charter", "ls", env={})

    def test_a_bare_tmux_reaches_the_default_server_and_is_refused(self):
        self._refused("ls", env={})

    def test_a_label_with_no_value_is_the_default_server(self):
        self._refused("-L", env={})

    def test_the_default_server_by_socket_path_is_refused(self):
        self._refused("-S", self.default, "ls", env={})

    def test_the_server_tmux_names_is_refused_when_nothing_else_is_given(self):
        self._refused("ls", env={"TMUX": f"{self.charter},4242,0", "TMUX_TMPDIR": str(self.dir)})

    def test_a_socket_path_wins_over_a_label_in_either_order(self):
        """tmux's own precedence: `-S` decides whatever `-L` says, before or after it."""
        self._refused("-L", "charter-mine", "-S", self.charter, "ls", env={})
        self._refused("-S", self.charter, "-L", "charter-mine", "ls", env={})

    def test_another_planes_own_server_is_refused_by_name(self):
        """Not the plane the suite runs in — any plane's. A test has no business on another
        project's live frame, and its name is not in `_OPERATOR_TMUX` to be matched."""
        self._refused("-L", "charter-plane-fedcba987654", "kill-window", "-t", "", env={})

    def test_another_planes_own_server_is_refused_by_socket_path(self):
        self._refused("-S", _socket("charter-plane-fedcba987654"), "ls", env={})

    def test_the_real_planes_own_server_is_refused(self):
        self._refused("-L", _planeguard._real_plane_socket(), "ls", env={})

    def test_the_suites_own_environment_is_read_when_the_child_is_given_none(self):
        self._refused("-L", "charter", "ls")

    def test_the_refusal_names_the_test_the_command_the_socket_and_the_ways_out(self):
        text = str(self._refused("-L", "charter", "kill-window", "-t", "", env={}))
        self.assertIn("test_the_refusal_names_the_test_the_command_the_socket_and_the_ways_out",
                      text)
        self.assertIn("kill-window", text)
        self.assertIn(self.charter, text)
        self.assertIn("-L charter-", text)
        self.assertIn("$TMUX_TMPDIR", text)
        self.assertIn("tmuxctl.live_pane_by_pid", text)

    # --- what does not ----------------------------------------------------------------

    def test_a_server_of_the_tests_own_is_allowed(self):
        self._allowed("-L", f"charter-tmuxguard-{os.getpid()}", "ls", env={})

    def test_charters_name_under_a_tmpdir_of_the_tests_own_is_allowed(self):
        self._allowed("-L", "charter", "ls", env={"TMUX_TMPDIR": str(self.dir)})

    def test_a_plane_name_under_a_tmpdir_of_the_tests_own_is_allowed(self):
        """What makes a real plane socket name usable in a test at all: its own
        `$TMUX_TMPDIR`, where no operator's frame can be listening."""
        self._allowed("-L", "charter-plane-fedcba987654", "ls",
                      env={"TMUX_TMPDIR": str(self.dir)})

    def test_a_name_that_only_resembles_a_planes_is_allowed(self):
        """Eleven hex digits, or capitals, is not the shape — the refusal is a fullmatch."""
        self._allowed("-L", "charter-plane-fedcba98765", "ls", env={})
        self._allowed("-L", "charter-plane-FEDCBA987654", "ls", env={})

    def test_a_label_overrides_the_server_tmux_names(self):
        self._allowed("-L", "charter-mine", "ls", env={"TMUX": f"{self.charter},4242,0"})

    def test_asking_the_version_connects_to_nothing(self):
        self._allowed("-V", env={})

    def test_words_after_the_command_are_the_commands_not_tmuxs(self):
        self._allowed("-L", "charter-mine", "new-session", "-S", self.charter, env={})

    def test_a_flags_value_that_looks_like_a_flag_is_a_value(self):
        """`-f` takes the next word whatever it looks like — tmux's getopt does the same."""
        self._allowed("-f", f"-S{self.charter}", "-L", "charter-mine", "ls", env={})

    def test_a_value_attached_to_its_flag_ends_the_bundle(self):
        """`-fS…` is a config file named `S…`, not `-f` followed by `-S`."""
        self._allowed(f"-fS{self.charter}", "-L", "charter-mine", "ls", env={})


if __name__ == "__main__":
    unittest.main()
