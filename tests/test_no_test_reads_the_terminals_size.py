"""How wide the operator's window is decides nothing here.

`tests/_ttyguard.py` is one guard answering two questions about the same three file
descriptors: whether they are a terminal (#545) and how big it is (#544).
`test_no_test_reads_the_operators_terminal.py` covers the first; this file covers the
second, the way `test_plane_write_guard` and `test_plane_spawn_guard` split `_planeguard`
between them. `test_no_test_reads_the_operators_shell.py` is the other sibling and this
file's model — that one is about what a shell EXPORTS, this one about what a shell cannot
express. Charter asks the size question twice (`tui.term_width`: ``$COLUMNS`` first, then
an ioctl on stdout) and both answers come off the machine rather than out of this
repository.

**Both halves are here because fixing only the first one is the trap #544 walked into.**
Its own suggested fix was "add ``COLUMNS`` to the scrub". Measured at b3dbd54 with
``$COLUMNS`` and ``$LINES`` already unset — that fix, applied — running the three modules
it names on a real pty: ``FAILED (failures=3, errors=1)`` at 40 columns and ``OK`` at 200.
Removing the variable moves the reading to the tty; it does not end it. So the scrub is
pinned here, and so is the ioctl, and the case that matters most
(:meth:`TheTerminalIsNotAFixture.test_the_width_the_suite_sees_is_the_same_in_any_window`)
runs the probe on two real ptys of different widths and demands one answer.

**Every case is a control, in `test_no_test_reads_the_operators_shell`'s own sense.** The
pty case carries its own: it reads the winsize back through `fcntl` and asserts the
terminal really was 40 columns wide, because a probe that quietly got a pipe would agree
with itself for free and prove nothing.
"""

from __future__ import annotations

import fcntl
import json
import os
import pathlib
import pty
import struct
import subprocess
import sys
import tempfile
import termios
import unittest
from unittest import mock

from charter import commands_frame, tui
from tests import _envguard, _ttyguard
from tests._isolation import child_plane_env


class TheNamesAreAskedOfTheModuleThatReadsThem(unittest.TestCase):
    """The scrub half, and the derivation rather than its contents.

    A guard that spelled ``("COLUMNS", "LINES")`` in `tests/_envguard.py` would pass every
    case in this class that merely checked the two names were covered — which is exactly
    how ``$COLUMNS`` got in: `_envguard` covered a list of spellings and called it a
    property. So each case here moves the AUTHORITY and demands the guard move with it.
    """

    def test_every_terminal_size_variable_charter_reads_is_scrubbed(self):
        for name in tui.TERMINAL_SIZE_VARS:
            with self.subTest(variable=name):
                self.assertTrue(
                    _envguard._in_namespace(name),
                    f"${name} is in `tui.TERMINAL_SIZE_VARS`, so charter reads it to "
                    f"decide how wide to draw — and a shell that exports it hands this "
                    f"suite a fixture nobody chose (#544)")

    def test_the_scrub_asks_tui_rather_than_spelling_the_names(self):
        """The #579 move: a name invented in `tui` must be scrubbed on the commit that
        invents it, not on the commit that debugs it. A `_envguard` that carried its own
        copy of the pair passes the case above and fails this one."""
        with mock.patch.object(tui, "TERMINAL_SIZE_VARS",
                               (*tui.TERMINAL_SIZE_VARS, "TERMINAL_ACRES")):
            self.assertIn("TERMINAL_ACRES", _envguard._scrubbed_names())

    def test_the_frame_strips_whatever_tui_names(self):
        """`_frame_env`'s stale list is the same fact, one process out: the environment a
        frame hands its panes must not describe the window `charter` was typed in. It used
        to spell the pair itself, which is the second copy of one answer — and the copy
        that could not be asked from the test harness, because importing `commands_frame`
        resolves a plane."""
        with mock.patch.object(tui, "TERMINAL_SIZE_VARS",
                               (*tui.TERMINAL_SIZE_VARS, "TERMINAL_ACRES")), \
                mock.patch.dict(os.environ, {"TERMINAL_ACRES": "3", "COLUMNS": "200"}):
            env = commands_frame._frame_env("fid-1", None)
        self.assertNotIn("TERMINAL_ACRES", env)
        self.assertNotIn("COLUMNS", env)

    def test_both_halves_of_the_pair_are_stripped_from_a_frames_children(self):
        """``$LINES`` earns its place in the tuple HERE and nowhere else, which is why it
        needs a case of its own: nothing in charter reads it, so removing it from
        `TERMINAL_SIZE_VARS` changes no rendered width and every other case in this file
        stays green — measured, by deleting it. What it does change is what a frame hands
        the processes it starts, and a shell's ``$LINES`` describes the terminal `charter`
        was typed in rather than any pane the frame creates."""
        with mock.patch.dict(os.environ, {"COLUMNS": "200", "LINES": "50"}):
            env = commands_frame._frame_env("fid-1", None)
        for name in ("COLUMNS", "LINES"):
            with self.subTest(variable=name):
                self.assertNotIn(
                    name, env,
                    f"${name} reached a frame's children, describing the terminal charter "
                    f"was typed in rather than the pane the child runs in. Both names are "
                    f"in `tui.TERMINAL_SIZE_VARS` for this, and only one of them for "
                    f"anything else.")

    def test_the_size_variables_are_scrubbed_but_not_refused(self):
        """Tier one only, and `_envguard._loud_names`' own test decides that: loudness is
        worth its cost where "unset" is a CLAIM ABOUT THE WORLD — whose session is this, is
        this a real tmux. "No ``$COLUMNS``" is not such a claim; it is the state a piped
        run is in, and charter's answer to it is a documented default. A refusal here would
        fire inside every one of the several hundred tests that render anything."""
        for name in tui.TERMINAL_SIZE_VARS:
            with self.subTest(variable=name):
                self.assertNotIn(name, _envguard._LOUD)


class TheTerminalIsNotAFixture(unittest.TestCase):
    """The ioctl half — the one the scrub cannot reach, measured on real ptys."""

    #: Written for whoever has to read a failure here: what the probe reports back.
    _PROBE = (
        "import fcntl, json, os, struct, sys, termios;"
        # BEFORE `import tests`, and deliberately through `fcntl` rather than
        # `os.get_terminal_size`: this is the CONTROL, and it has to read the terminal the
        # way the guard cannot intercept, or it would be agreeing with the guard about
        # what the guard did.
        "raw = struct.unpack('HHHH', fcntl.ioctl("
        "    sys.stdout.fileno(), termios.TIOCGWINSZ, b'\\0' * 8));"
        "tty = sys.stdout.isatty();"
        "import tests;"
        "from charter import tui;"
        "open(sys.argv[1], 'w').write(json.dumps({"
        "    'raw_cols': raw[1], 'raw_rows': raw[0], 'isatty': tty,"
        "    'columns': os.environ.get('COLUMNS'),"
        "    'width': tui.term_width()}))")

    def _seen_on_a_pty(self, cols: int, rows: int = 24, **planted: str) -> dict:
        """What a fresh suite process reports for `tui.term_width` on a *cols*-wide tty.

        A real pty, because that is the whole question: `os.get_terminal_size` raises on a
        pipe, so a probe run down a pipe would report the guarded answer whether or not
        anything was guarded. `pty.openpty` and `subprocess` rather than `pty.fork` and
        `os.execvp` — `NoCharterEscapesThroughTheExecFamily` writes down every exec in this
        tree, and a new one here would be a spawn the plane guard cannot see.

        The child writes to a FILE rather than to its stdout: its stdout is the pty, and
        reading a pty while the child is still writing to it is a deadlock waiting for a
        buffer size.

        Run from a throwaway plane with ``$PYTHONPATH`` carrying the tree, for the reason
        `_planeguard._explain_spawn` names: a child that imports the `tests` package has
        ``$CHARTER_ROOT`` scrubbed out from under it before `charter.config` loads, so its
        cwd is the only thing left deciding whose plane it resolves.
        """
        master, slave = pty.openpty()
        self.addCleanup(os.close, master)
        try:
            fcntl.ioctl(slave, termios.TIOCSWINSZ,
                        struct.pack("HHHH", rows, cols, 0, 0))
            out = pathlib.Path(tempfile.mkdtemp(prefix="charter-termprobe-")) / "seen.json"
            self.addCleanup(lambda: out.parent.exists() and __import__("shutil").rmtree(
                out.parent, ignore_errors=True))
            tree = pathlib.Path(__file__).resolve().parent.parent
            plane, _ = child_plane_env(self)
            done = subprocess.run(
                [sys.executable, "-c", self._PROBE, str(out)],
                env={**os.environ.copy(), **planted, "PYTHONPATH": str(tree)},
                cwd=str(plane), stdout=slave, stderr=subprocess.PIPE, text=True,
                timeout=120)
        finally:
            os.close(slave)
        self.assertEqual(done.returncode, 0, done.stderr)
        return json.loads(out.read_text())

    def test_the_width_the_suite_sees_is_the_same_in_any_window(self):
        """#544's exit test, run rather than described.

        Two suite processes, identical but for the size of the terminal they were handed.
        On the tree that had this defect they answered 40 and 200 and three renders came
        out different; here they answer the same number twice, and the number is
        `term_width`'s own documented default rather than either window.
        """
        narrow = self._seen_on_a_pty(40)
        wide = self._seen_on_a_pty(200)

        # The control, first: a probe that got a pipe instead of a terminal would report
        # the guarded answer for free, and this whole case would be true of a tree with no
        # guard in it at all.
        for name, got, want in (("narrow", narrow, 40), ("wide", wide, 200)):
            with self.subTest(pty=name):
                self.assertTrue(got["isatty"],
                                "the probe's stdout was not a terminal, so this case "
                                "cannot tell a guarded suite from a piped one")
                self.assertEqual(got["raw_cols"], want,
                                 "the pty was not the width this case set, so what it "
                                 "measured is not what it says it measured")
                self.assertIsNone(got["columns"],
                                  "$COLUMNS reached the child, so this case is measuring "
                                  "the scrub rather than the ioctl it is about")

        self.assertEqual(
            narrow["width"], wide["width"],
            f"the suite is reading the terminal: `tui.term_width()` answered "
            f"{narrow['width']} in a 40-column window and {wide['width']} in a "
            f"200-column one, so every render this suite compares against a fixed string "
            f"depends on how wide the operator's terminal happened to be (#544)")
        self.assertEqual(narrow["width"], 80,
                         "not the terminal, and not an invented number either: "
                         "`term_width`'s documented default is what a piped run — CI, and "
                         "every agent-launched run — already gets")

    def test_a_shell_that_exports_columns_changes_nothing_either(self):
        """The scrub's own end-to-end control, and the one that fails on a tree that never
        had the hole would look green without: ``COLUMNS=40`` is PLANTED in the child's
        environment, the way a real shell exports it, and the child still answers 80."""
        got = self._seen_on_a_pty(200, COLUMNS="40", LINES="5")
        self.assertIsNone(got["columns"], "$COLUMNS survived the scrub into a fresh suite")
        self.assertEqual(got["width"], 80)


class TheIoctlAnswersWhatAPipeAnswers(unittest.TestCase):
    """In-process: what `_ttyguard` replaced, and that its documented exit works."""

    def test_os_get_terminal_size_is_the_guarded_one(self):
        """Structural, the way `_envguard`'s own case is: `charter.tui`,
        `charter.frame.slots`, `charter.frame.palette` and `charter.commands_frame` all
        look this up on `os` at call time, so replacing the attribute is what covers every
        reader — `shutil.get_terminal_size`, and so `argparse`'s help formatter, included."""
        self.assertEqual(getattr(os.get_terminal_size, "__module__", None),
                         "tests._ttyguard")

    def test_asking_the_tty_raises_the_error_a_pipe_raises(self):
        with self.assertRaises(OSError):
            os.get_terminal_size()
        with self.assertRaises(OSError):
            os.get_terminal_size(sys.stdout.fileno())

    def test_the_message_says_where_it_came_from_and_how_to_state_a_size(self):
        """A tripwire nobody can find their way out of is a tripwire somebody deletes."""
        with self.assertRaises(OSError) as raised:
            os.get_terminal_size()
        said = str(raised.exception)
        self.assertIn("_ttyguard", said)
        self.assertIn("#544", said)
        self.assertIn("os.get_terminal_size", said)

    def test_a_test_that_wants_a_size_states_one_and_gets_it(self):
        """The escape is the idiom this suite already writes fifty-odd times, unchanged:
        `mock.patch` replaces the same module attribute the guard installed."""
        with mock.patch("os.get_terminal_size",
                        return_value=os.terminal_size((22, 3))):
            self.assertEqual(tui.term_width(), 22)
        with self.assertRaises(OSError):        # and it is put back afterwards
            os.get_terminal_size()

    def test_term_width_falls_to_its_documented_default(self):
        _envguard.unset("COLUMNS")
        self.assertEqual(tui.term_width(), 80)
        self.assertEqual(tui.term_width(default=132), 132)

    def test_the_real_measurement_is_kept_for_anything_that_needs_it(self):
        """Neutralised, not destroyed — `_envguard.scrubbed()`'s courtesy, for the same
        reason. Asserted as a shape rather than a value: under a pipe the real call raises,
        which is correct there and must stay green."""
        self.assertIsNot(_ttyguard.real_get_terminal_size, os.get_terminal_size)
        self.assertTrue(callable(_ttyguard.real_get_terminal_size))

    def test_installing_twice_does_not_stack(self):
        """`tests/__init__` calls this once, but a child process importing the package
        while a parent already did is the shape `_planeguard.install` guards the same way.
        Load-bearing here in a way it is not elsewhere: a second install would capture the
        REFUSING function as `real_get_terminal_size` and there would be no way back to a
        real measurement at all."""
        guarded = os.get_terminal_size
        real = _ttyguard.real_get_terminal_size
        _ttyguard.install()
        self.assertIs(os.get_terminal_size, guarded)
        self.assertIs(_ttyguard.real_get_terminal_size, real)


if __name__ == "__main__":       # pragma: no cover
    unittest.main()
