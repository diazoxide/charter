r"""A harness argument that ends in `;` reaches the harness with its `;`.

**tmux reads a trailing `;` as its own command separator, and says nothing about it.**
Measured on tmux 3.7c and at the 3.2 floor, through all three builders that put a harness
after `--` — `layout.session_argv`, `layout.chat_window_argv` and `layout.respawn_argv` —
with a recorder standing in for the harness: `run the tests;` arrived as `run the tests`,
`run the tests\;` as `run the tests;`, `a;b c;` as `a;b c`, and a lone `;` as no argument
at all. Every start returned 0 with nothing on stderr. So `charter claude "run the tests;"`
started Claude on a prompt one byte shorter than the one typed, and nothing said so.

**Why `tmuxctl.SEPARATOR`'s own note did not catch it.** That note measured an argument
merely CONTAINING a `;` and found it passed through whole, which is still true: the parse
looks only at the END of each argument. The same parse names its own escape — a trailing
`\;` is handed on as a literal `;` — so `tmuxctl.verbatim` puts one backslash before the
last `;` of an argument that ends in one, and touches nothing else. Measured intact on both
versions and all three builders for `;`, `;;`, ` ;`, `\;`, `\\;` and `a;b c;`.

:class:`ARealTmuxHandsTheHarnessEveryByte` is that measurement, kept.
"""

from __future__ import annotations

import json
import os
import shutil
import subprocess
import sys
import time
import unittest
from pathlib import Path

from charter.frame import layout, tmuxctl

from tests import _tmuxreap, _tmuxsocket
from tests._isolation import PersonaIso

_HAS_TMUX = shutil.which("tmux") is not None

#: The tmux this repo builds to measure its own floor against, named the way
#: `test_a_planes_frame_really_reads_that_way` names it. Absent on CI, so the floor rows
#: there are simply not run — the `tmux` on `$PATH` still is.
_FLOOR_BIN = (Path.home() / ".local/share/charter-testing"
              / f"tmux-{tmuxctl.FLOOR[0]}.{tmuxctl.FLOOR[1]}")

#: ``(what the operator typed, what tmux has to be handed for the harness to get it)`` —
#: the measured matrix. Each row changes by exactly one backslash, before its LAST `;`.
_ESCAPED = (
    (";", "\\;"),
    (";;", ";\\;"),
    (" ;", " \\;"),
    ("\\;", "\\\\;"),
    ("\\\\;", "\\\\\\;"),
    ("a;b c;", "a;b c\\;"),
    ("run the tests;", "run the tests\\;"),
)

#: What tmux already hands on whole, which the escape must leave byte-for-byte alone — a
#: `;` anywhere but last, a trailing backslash with no `;`, and the text a shell or tmux's
#: format parser would mangle if anything here were ever joined or expanded.
_UNCHANGED = (
    "", "plain text", "run a ; then b", "a;b", "; leading", ";\n",
    "ends in a backslash \\", "first line\nsecond line\n\nthird",
    "name #{session_name} here", 'say "hi"', "it's here", "run `x` now",
    "value $(echo x) here", "a\tb c", "a\u2028b c",
)


class TheEscapeIsExactlyWhatTmuxUndoes(unittest.TestCase):
    """`tmuxctl.verbatim` over the measured matrix, with no tmux running."""

    def test_an_argument_ending_in_a_semicolon_gets_one_backslash_before_it(self):
        for typed, sent in _ESCAPED:
            with self.subTest(typed=typed):
                self.assertEqual(tmuxctl.verbatim(typed), sent)

    def test_an_argument_that_does_not_end_in_one_is_handed_on_as_it_is(self):
        for typed in _UNCHANGED:
            with self.subTest(typed=typed):
                self.assertEqual(tmuxctl.verbatim(typed), typed)


#: A harness argv with the escape's cases in every position — the program itself, a
#: prompt, an argument it must not touch, and a lone `;` that tmux would otherwise drop.
_TYPED = ["prog;", "run the tests;", "plain", ";"]
_SENT = ["prog\\;", "run the tests\\;", "plain", "\\;"]


def _after_separator(argv: list[str]) -> list[str]:
    return argv[argv.index("--") + 1:]


class EveryBuilderEscapesEveryArgumentAfterTheSeparator(unittest.TestCase):
    """The three builders that hand tmux a harness, each asked on its own — a builder that
    forgot the escape would otherwise hide behind the two that remembered it."""

    def test_the_session_that_starts_a_workspace(self):
        argv = layout.session_argv(session="w", conf="/dev/null", socket="charter",
                                   cols=80, rows=24, harness_argv=list(_TYPED), chat="w.1")
        self.assertEqual(_after_separator(argv), _SENT)

    def test_the_window_that_adds_a_chat(self):
        argv = layout.chat_window_argv(socket="charter", session="w", chat="w.2",
                                       cwd="/tmp", harness_argv=list(_TYPED))
        self.assertEqual(_after_separator(argv), _SENT)

    def test_the_respawn_inside_a_tmux_you_already_had(self):
        argv = layout.respawn_argv(socket=_tmuxsocket.OPERATOR_SOCKET, harness_pane="%7",
                                   env={}, cwd="/tmp", harness_argv=list(_TYPED))
        self.assertEqual(_after_separator(argv), _SENT)


@unittest.skipUnless(_HAS_TMUX, "no tmux on this machine")
class ARealTmuxHandsTheHarnessEveryByte(PersonaIso, unittest.TestCase):
    """What the harness's own process receives, read back from its `sys.argv`.

    The recorder is started as ``[python, recorder, text]`` rather than through a shebang:
    three separate arguments are exec'd directly by tmux (`docs/frame.md`, *What `charter
    frame -- <cmd>` accepts*), so no shell sits between tmux and the bytes being asked
    about, and an interpreter path with a space in it cannot break the case.
    """

    #: The escape's own rows, and the texts that were already whole — so a change that
    #: fixed the first by damaging the second fails here too.
    TEXTS = tuple(typed for typed, _ in _ESCAPED) + (
        "run a ; then b", "first line\nsecond line", "name #{session_name} here",
        "say \"hi\", it's `x` and $(echo x)", "a\tb\u2028c",
    )

    def setUp(self) -> None:
        super().setUp()
        v = tmuxctl.version()
        if v is None or v < tmuxctl.FLOOR:
            self.skipTest(f"the frame's floor is tmux {tmuxctl.FLOOR[0]}.{tmuxctl.FLOOR[1]};"
                          f" this machine has {v}")
        self.recorder = self.tmp / "recorder.py"
        self.recorder.write_text(
            "import json, os, sys\n"
            "p = os.path.join(os.environ['RECORD_DIR'],\n"
            "                 os.environ['TMUX_PANE'].lstrip('%') + '.json')\n"
            "with open(p + '.tmp', 'w') as f:\n"
            "    json.dump(sys.argv[1:], f)\n"
            "os.replace(p + '.tmp', p)\n")
        #: ``(binary, socket, where its panes record)`` for every tmux this machine has.
        #: One records directory per server, because pane ids restart at `%0` on each.
        self.servers: list[tuple[str, str, Path]] = []
        candidates = [("tmux", "trailing-semicolon")]
        if _FLOOR_BIN.is_file():
            candidates.append((str(_FLOOR_BIN), "trailing-semicolon-floor"))
        for binary, slug in candidates:
            socket = _tmuxreap.name(slug)
            records = self.tmp / f"records-{slug}"
            records.mkdir()
            # Killed whether or not the start below worked: a server that half-started is
            # still a server, and the reaper only sees it once this process is gone.
            self.addCleanup(subprocess.run, [binary, "-L", socket, "kill-server"],
                            capture_output=True, timeout=20)
            started = subprocess.run(
                [binary, "-L", socket, "-f", "/dev/null", "new-session", "-d", "-s",
                 "base", "-x", "80", "-y", "24", "--", "sleep", "600"],
                capture_output=True, text=True, timeout=20,
                env=dict(os.environ, RECORD_DIR=str(records)))
            self.assertEqual(started.returncode, 0, started.stderr)
            self.servers.append((binary, socket, records))

    @staticmethod
    def _tmux(binary: str, argv: list[str]) -> subprocess.CompletedProcess:
        """*argv* as a builder made it, run by *binary* instead of the `tmux` it names."""
        return subprocess.run([binary, *argv[1:]], capture_output=True, text=True,
                              timeout=20)

    def _harness(self, text: str) -> list[str]:
        return [sys.executable, str(self.recorder), text]

    def _received(self, binary: str, socket: str, records: Path, pane: str) -> list[str]:
        """What the harness in *pane* was handed, read back off its own `sys.argv`.

        **Failing fast is the half that matters when this goes red.** A harness that could
        not start leaves no record and no pane, and waiting out the deadline on every row
        turned one broken argv into a twelve-minute module — which the deletion sweep could
        only report as a timeout with no verdict, not as the failure it was. So the wait
        ends the moment tmux no longer knows the pane, after one last look for a record the
        harness may have written on its way out.
        """
        record = records / (pane.lstrip("%") + ".json")
        deadline = time.monotonic() + 10
        while time.monotonic() < deadline:
            if record.is_file():
                return json.loads(record.read_text())
            # `list-panes -a`, and never `display-message -t <pane>`: measured on 3.7c and
            # at the 3.2 floor, asked about a pane whose harness failed to exec,
            # `display-message` answers rc 0 with an empty line instead of refusing, so it
            # cannot say "gone" at all — and this wait ran to its deadline on every row.
            panes = subprocess.run([binary, "-L", socket, "list-panes", "-a", "-F",
                                    "#{pane_id}"],
                                   capture_output=True, text=True, timeout=20)
            if pane not in panes.stdout.split():
                if record.is_file():
                    return json.loads(record.read_text())
                self.fail(f"the harness in {pane} ended without recording what it was "
                          f"handed — it never started")
            time.sleep(0.02)
        self.fail(f"the harness in {pane} never started")

    def test_a_workspaces_first_chat_gets_every_byte(self):
        for binary, socket, records in self.servers:
            for i, text in enumerate(self.TEXTS):
                with self.subTest(tmux=binary, text=text):
                    started = self._tmux(binary, layout.session_argv(
                        session=f"s{i}", conf="/dev/null", socket=socket, cols=80,
                        rows=24, harness_argv=self._harness(text), chat=f"s{i}.1"))
                    self.assertEqual(started.returncode, 0, started.stderr)
                    self.assertEqual(self._received(binary, socket, records,
                                                    started.stdout.strip()),
                                     [text])

    def test_a_second_chat_gets_every_byte(self):
        for binary, socket, records in self.servers:
            for i, text in enumerate(self.TEXTS):
                with self.subTest(tmux=binary, text=text):
                    started = self._tmux(binary, layout.chat_window_argv(
                        socket=socket, session="base", chat=f"base.{i + 1}",
                        cwd=str(self.tmp), harness_argv=self._harness(text)))
                    self.assertEqual(started.returncode, 0, started.stderr)
                    self.assertEqual(self._received(binary, socket, records,
                                                    started.stdout.strip()),
                                     [text])

    def test_a_chat_respawned_into_a_placeholder_gets_every_byte(self):
        for binary, socket, records in self.servers:
            for text in self.TEXTS:
                with self.subTest(tmux=binary, text=text):
                    placeholder = self._tmux(binary, [
                        "tmux", "-L", socket, "new-window", "-d", "-a", "-t", "base",
                        "-P", "-F", "#{pane_id}", "--", "sleep", "600"])
                    self.assertEqual(placeholder.returncode, 0, placeholder.stderr)
                    pane = placeholder.stdout.strip()
                    started = self._tmux(binary, layout.respawn_argv(
                        socket=socket, harness_pane=pane, env={}, cwd=str(self.tmp),
                        harness_argv=self._harness(text)))
                    self.assertEqual(started.returncode, 0, started.stderr)
                    self.assertEqual(self._received(binary, socket, records, pane), [text])


if __name__ == "__main__":
    unittest.main()
