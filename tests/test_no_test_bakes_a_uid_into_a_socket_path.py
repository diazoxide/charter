"""No test writes a tmux socket path with a uid in it, and the helper that replaces them
answers what tmux would answer (#601).

`tests/test_frame_launcher.py` carried ``OPERATOR_SOCKET = "/private/tmp/tmux-502/default"``.
**502 is one developer's uid.** Six modules carried that string or a `501` cousin, and on
anybody else's machine each of them names either nothing or somebody else's socket
directory.

It is the last narrow instance of the shape the test-hygiene cluster closed everywhere
else — **the suite reads the machine instead of the repo** — and it is closed the same way
those were: a helper that asks (`tests/_tmuxsocket.py`), plus a tripwire so the next one
fails on the commit that writes it rather than on the machine that cannot run it.

**The two halves are separate on purpose.** The scan below would pass if `_tmuxsocket`
returned nonsense, and `_tmuxsocket`'s own cases would pass with six literals still in the
suite. Neither alone is the property.
"""

from __future__ import annotations

import os
import re
import unittest
from pathlib import Path
from unittest import mock

from tests import _tmuxsocket

#: Every ``*.py`` under `tests/`, which is the whole of what this claims about.
TESTS_DIR = Path(__file__).resolve().parent

#: A tmux socket path with a **literal** uid in it: ``/tmp/tmux-502`` or
#: ``/private/tmp/tmux-502``.
#:
#: Keyed to the PATH shape rather than to ``tmux-<digits>`` anywhere in a line, and the
#: difference is one real case: `test_the_suite_reaps_its_own_tmux_servers` lists
#: ``"tmux-502"`` among socket *names* charter must not claim as its own, where the digits
#: are an arbitrary label rather than an assertion about where a socket lives. Widening
#: this to catch that would be asking a guard to reach into a name it does not own, which
#: is the very call `_tmuxreap` documents making the other way.
BAKED = re.compile(r"/(?:private/)?tmp/tmux-\d")

#: The two files allowed to contain the pattern: the one that defines what a baked path
#: looks like (this one) and nothing else. `_tmuxsocket` builds the path from
#: `os.getuid()` and so does not match at all — which is the point, and is asserted below
#: rather than assumed.
ALLOWED = {Path(__file__).name}


class NoTestSpellsAUid(unittest.TestCase):
    def test_no_module_under_tests_writes_a_socket_path_with_a_uid_in_it(self):
        found = {}
        for f in sorted(TESTS_DIR.rglob("*.py")):
            if f.name in ALLOWED:
                continue
            hits = [(i, ln.strip()) for i, ln in enumerate(f.read_text().splitlines(), 1)
                    if BAKED.search(ln)]
            if hits:
                found[f.name] = hits
        self.assertEqual(
            found, {},
            "a tmux socket path with a literal uid in it is one developer's machine "
            "written into the suite — on anyone else's it names nothing, or somebody "
            "else's socket directory. Use `tests._tmuxsocket.OPERATOR_SOCKET` / "
            "`OPERATOR_TMUX`, which compute it the way tmux does:\n"
            + "\n".join(f"  {name}:{ln}  {text}"
                        for name, hits in found.items() for ln, text in hits))

    def test_the_helper_itself_does_not_spell_one(self):
        """The escape hatch is not an exemption: `_tmuxsocket` clears the same scan the
        modules it replaced now clear, because it builds the path rather than naming it."""
        self.assertIsNone(BAKED.search(Path(_tmuxsocket.__file__).read_text()))

    def test_the_scan_would_catch_the_line_it_was_written_for(self):
        """A tripwire nobody has seen fire is a tripwire nobody trusts. Both spellings the
        suite actually carried, and the one it did not (`/tmp` on Linux)."""
        for line in ('OPERATOR_SOCKET = "/private/tmp/tmux-502/default"',
                     '"TMUX": "/tmp/tmux-501/default,1,0"',
                     'sock = "/tmp/tmux-0/default"'):
            with self.subTest(line=line):
                self.assertTrue(BAKED.search(line))

    def test_the_scan_leaves_a_computed_path_alone(self):
        """The shape every remaining site now has — and the one
        `test_frame_tmux_integration` already had before any of this."""
        for line in ('(Path("/tmp") / f"tmux-{os.getuid()}" / SOCKET)',
                     'os.path.join(os.environ.get("TMUX_TMPDIR") or "/tmp", '
                     'f"tmux-{os.getuid()}")'):
            with self.subTest(line=line):
                self.assertIsNone(BAKED.search(line))


class TheHelperAsksTheMachine(unittest.TestCase):
    def test_the_uid_is_this_process_and_not_a_number(self):
        self.assertIn(f"tmux-{os.getuid()}", _tmuxsocket.socket_path())

    def test_tmux_tmpdir_wins_when_it_is_set(self):
        """tmux reads ``$TMUX_TMPDIR`` first, so a machine that sets it puts its sockets
        somewhere else entirely and a helper that ignored it would be back to guessing."""
        with mock.patch.dict(os.environ, {"TMUX_TMPDIR": "/var/run"}):
            self.assertTrue(_tmuxsocket.socket_path().startswith(
                os.path.realpath("/var/run")), _tmuxsocket.socket_path())

    def test_an_empty_tmux_tmpdir_is_not_a_directory(self):
        """tmux tests ``*tmux_tmpdir != '\\0'``, not merely that the variable exists —
        an exported-but-empty one falls back to ``/tmp`` rather than making the socket
        directory ``/tmux-<uid>`` at the filesystem root."""
        with mock.patch.dict(os.environ, {"TMUX_TMPDIR": ""}):
            self.assertEqual(_tmuxsocket.socket_path(),
                             _tmuxsocket.socket_path(tmpdir="/tmp"))

    def test_the_default_is_slash_tmp_and_not_the_temp_directory_python_would_pick(self):
        """tmux's ``_PATH_TMP`` is the literal ``/tmp``. On macOS `tempfile.gettempdir()`
        answers a per-user ``/var/folders/…`` path that tmux never writes to, so a helper
        built on it would be wrong on exactly the platform this was measured on."""
        import tempfile
        with mock.patch.dict(os.environ, {}, clear=False):
            os.environ.pop("TMUX_TMPDIR", None)
            got = _tmuxsocket.socket_path()
        self.assertEqual(got, _tmuxsocket.socket_path(tmpdir="/tmp"))
        if os.path.realpath(tempfile.gettempdir()) != os.path.realpath("/tmp"):
            self.assertFalse(got.startswith(os.path.realpath(tempfile.gettempdir())), got)

    def test_the_path_is_resolved_so_the_platform_decides_its_spelling(self):
        """The other half of #601, and #572's measurement one directory over: on macOS
        ``/tmp`` is a symlink to ``/private/tmp``, tmux calls ``realpath()`` on its base
        before using it, and a test that spelled one form asserted something false on the
        platform handing it the other.

        Asserted as a fixed point — the path is its own `realpath` — rather than against
        either spelling, so it is one case on both platforms instead of two that disagree.
        """
        got = _tmuxsocket.socket_path()
        self.assertEqual(got, os.path.realpath(got))

    def test_the_resolution_is_what_a_real_tmux_would_have_produced(self):
        """Not merely resolved — resolved to the same place. On macOS this is the
        assertion that says `/private/tmp`; on Linux it says `/tmp`; neither is written
        down here."""
        self.assertEqual(_tmuxsocket.socket_dir(),
                         os.path.realpath(f"/tmp/tmux-{os.getuid()}"))

    def test_nothing_is_created_on_disk_by_asking(self):
        """`realpath` resolves what exists and leaves the rest lexical, so a machine with
        no tmux server running still gets the right answer and the suite still starts no
        server it did not mean to (#542/#564's whole subject)."""
        d = _tmuxsocket.socket_dir(tmpdir="/tmp")
        before = os.path.isdir(d)
        _tmuxsocket.socket_path()
        self.assertEqual(os.path.isdir(d), before)

    def test_the_tmux_value_is_shaped_the_way_tmux_exports_it(self):
        """``<socket path>,<server pid>,<session id>`` — and `tmuxctl` parses it, so the
        shape is charter's input rather than decoration."""
        parts = _tmuxsocket.OPERATOR_TMUX.split(",")
        self.assertEqual(len(parts), 3, _tmuxsocket.OPERATOR_TMUX)
        self.assertEqual(parts[0], _tmuxsocket.OPERATOR_SOCKET)
        self.assertTrue(parts[1].isdigit() and parts[2])

    def test_the_socket_name_is_the_one_tmux_uses_with_no_dash_l(self):
        self.assertTrue(_tmuxsocket.OPERATOR_SOCKET.endswith("/default"))


if __name__ == "__main__":
    unittest.main()
