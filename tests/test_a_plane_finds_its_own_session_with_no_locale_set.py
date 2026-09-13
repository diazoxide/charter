"""A plane finds its own tmux session when the shell charter runs in names no locale (#984).

**tmux decides what it may print from the CLIENT's locale, and charter's readers are that
client.** Measured on tmux 3.7c with private sockets: with `$LANG`, `$LC_ALL` and
`$LC_CTYPE` all unset and no `$TMUX`, a literal TAB in a `-F` format came back as `_`. Five
of charter's formats separate their fields with one, so every row read as a single field:
`commands_frame._plane_session` answered ``None`` for a session its own launcher made, and
`charter handoff` from inside that frame refused the plane as "probably another plane's". A
non-ASCII value came back mangled the same way — `@charter_plane` holding `/tmp/plané_x`
read as `/tmp/plan__x` — so a plane whose path is not ASCII would have been refused even
once the fields split. Only the client's environment mattered; the server's did not.

`-u` on every tmux command charter sends (`tmuxctl.server_argv`) restored both, and so did
a UTF-8 `$LANG`, `$LC_ALL` or `$LC_CTYPE` in the client's environment — but a locale forced
that way was copied into the server's global environment and every pane started after it,
and `-u` touches nothing but the client that carries it.

**Red on `main` whatever locale the developer runs the suite in**, because the case removes
all three variables itself rather than relying on a runner that happens not to set them —
including the `LC_CTYPE=C.UTF-8` Python writes into its own environment when it starts in
the C locale on Linux (PEP 538), which would otherwise hand tmux the answer.
"""

from __future__ import annotations

import os
import shutil
import subprocess
import tempfile
import unittest
from pathlib import Path
from unittest import mock

from charter import commands_frame, config
from charter.frame import state, tmuxctl
from tests import _tmuxreap
from tests._isolation import PersonaIso

_HAS_TMUX = shutil.which("tmux") is not None

#: This module's own server — `tests/_tmuxreap.py`'s namespace, so a killed run's server is
#: collected by the next one.
SOCKET = _tmuxreap.name("fix-984")

#: The three variables tmux's locale check reads, in the order `setlocale` consults them.
_LOCALE = ("LC_ALL", "LC_CTYPE", "LANG")

WS = "alpha"
CHAT = "alpha.1"


def _fixture_tmux(*args: str) -> subprocess.CompletedProcess:
    """The FIXTURE's own tmux, for building the server — never for a value asserted on,
    and never a TAB-separated format, so nothing it reads depends on the bug under test."""
    return subprocess.run(["tmux", "-L", SOCKET, *args], capture_output=True, text=True,
                          timeout=15)


@unittest.skipUnless(_HAS_TMUX, "no tmux on this machine")
class ThePlaneThatMadeTheSessionFindsItWithNoLocale(PersonaIso, unittest.TestCase):

    def setUp(self) -> None:
        super().setUp()
        # Removed rather than set: a shell that names no locale is the report, and "the
        # developer's terminal says UTF-8" is exactly what would turn this green on `main`.
        self.enterContext(mock.patch.dict(os.environ))
        for name in (*_LOCALE, "TMUX"):
            os.environ.pop(name, None)
        self.addCleanup(self._teardown_socket)

        # A plane whose path is not ASCII, so the marker `_plane_session` compares against
        # is one tmux mangles for a client it does not believe is UTF-8.
        plane = Path(tempfile.mkdtemp(prefix="plané-"))
        self.addCleanup(shutil.rmtree, plane, True)
        self.addCleanup(config.restore, config.use(plane))
        self.assertIn("é", str(config.STATE_DIR))

        started = _fixture_tmux("new-session", "-d", "-s", WS, "-n", "c1", "sleep 300")
        self.assertEqual(started.returncode, 0, started.stderr)
        self.session = _fixture_tmux("display-message", "-p", "-t", WS,
                                     "#{session_id}").stdout.strip()
        self.pane = _fixture_tmux("display-message", "-p", "-t", WS,
                                  "#{pane_id}").stdout.strip()

        (config.WORKSPACES_DIR / WS).mkdir(parents=True, exist_ok=True)
        state.frame_dir(CHAT, create=True)
        state.record_workspace(CHAT, WS)
        state.record_server(CHAT, SOCKET)
        state.record_harness_pane(CHAT, self.pane)

    def _teardown_socket(self) -> None:
        """Kill the server, then unlink its socket — in that order, for
        `tests/test_frame_tmux_integration.py::_TmuxServerFixture`'s reason."""
        path = _fixture_tmux("display-message", "-p", "#{socket_path}").stdout.strip()
        _fixture_tmux("kill-server")
        if path.startswith("/"):
            try:
                os.unlink(path)
            except OSError:
                pass

    def test_the_plane_that_marked_its_session_resolves_it(self):
        """The marker is written through charter's own writer and read back through charter's
        own reader, both from this locale-less environment — the round trip a launch and a
        later `handoff` make."""
        argv = commands_frame._plane_option_argv(socket=SOCKET, harness_pane=self.pane)
        self.assertIsNotNone(argv)
        wrote = tmuxctl.run("marking the fixture's session", argv, report=False)
        self.assertEqual(wrote.returncode, 0, wrote.stderr)

        found = commands_frame._plane_session(SOCKET, ws=WS)

        answered = tmuxctl.run("reading back what tmux answered",
                               tmuxctl.server_argv(SOCKET, "list-panes", "-a", "-F",
                                                   commands_frame._PANE_SEAT_FORMAT),
                               report=False).stdout
        self.assertEqual(found, (self.session, CHAT),
                         f"tmux answered {answered!r} for this plane's session, "
                         f"marked {str(config.STATE_DIR)!r}")


if __name__ == "__main__":
    unittest.main()
