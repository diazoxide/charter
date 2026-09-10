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


class DataArgumentsEndingInASemicolonAreEscapedToo(unittest.TestCase):
    """The same tmux parse reads EVERY argument, not only the harness's (review round 1 on
    #959). A directory or an identity value ending in `;` does not vanish quietly: it ends
    the command early and the flag after it is read as a command of its own. Measured on
    3.7c and 3.2 — `unknown command: -P` from `new-window` and the guest `window_argv`,
    `unknown command: -e` from `respawn-pane`, `unknown command: --` for an `-e` value — so
    a chat opened from a directory named that way failed with a sentence that never named
    the directory."""

    WHERE = "/work/dir;"

    @staticmethod
    def _after(argv: list[str], flag: str) -> str:
        return argv[argv.index(flag) + 1]

    def test_a_directory_ending_in_a_semicolon_is_escaped(self):
        for name, argv in (
                ("window_argv", layout.window_argv(
                    socket="charter", session="$1", window="w.1", cwd=self.WHERE)),
                ("chat_window_argv", layout.chat_window_argv(
                    socket="charter", session="w", chat="w.2", cwd=self.WHERE,
                    harness_argv=["prog"])),
                ("respawn_argv", layout.respawn_argv(
                    socket=_tmuxsocket.OPERATOR_SOCKET, harness_pane="%7", env={},
                    cwd=self.WHERE, harness_argv=["prog"]))):
            with self.subTest(builder=name):
                self.assertEqual(self._after(argv, "-c"), "/work/dir\\;")

    def test_an_identity_value_ending_in_a_semicolon_is_escaped(self):
        env = {"CHARTER_ROOT": "/plane;"}
        for name, argv in (
                ("session_argv", layout.session_argv(
                    session="w", conf="/dev/null", socket="charter", cols=80, rows=24,
                    harness_argv=["prog"], chat="w.1", env=env)),
                ("chat_window_argv", layout.chat_window_argv(
                    socket="charter", session="w", chat="w.2", cwd="/work",
                    harness_argv=["prog"], env=env)),
                ("respawn_argv", layout.respawn_argv(
                    socket=_tmuxsocket.OPERATOR_SOCKET, harness_pane="%7", env=env,
                    cwd="/work", harness_argv=["prog"]))):
            with self.subTest(builder=name):
                self.assertEqual(self._after(argv, "-e"), "CHARTER_ROOT=/plane\\;")


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
            "    json.dump({'argv': sys.argv[1:], 'cwd': os.getcwd(),\n"
            "               'root': os.environ.get('CHARTER_ROOT')}, f)\n"
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
        """What the harness in *pane* was handed, read back off its own `sys.argv`."""
        return self._record(binary, socket, records, pane)["argv"]

    def _record(self, binary: str, socket: str, records: Path, pane: str) -> dict:
        """What the harness in *pane* recorded: its `sys.argv`, the directory it started in,
        and the `$CHARTER_ROOT` it was handed.

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

    #: The three ways charter starts a harness, each asked on its own below.
    BUILDERS = ("new-session", "new-window", "respawn-pane")

    def _placeholder(self, binary: str, socket: str) -> str:
        """A window running `sleep` for a respawn to replace — the guest path's own order."""
        made = self._tmux(binary, ["tmux", "-L", socket, "new-window", "-d", "-a", "-t",
                                   "base", "-P", "-F", "#{pane_id}", "--", "sleep", "600"])
        self.assertEqual(made.returncode, 0, made.stderr)
        return made.stdout.strip()

    def _start(self, binary: str, socket: str, builder: str, *, tag: str,
               harness_argv: list[str], cwd: str | None = None,
               env: dict[str, str] | None = None) -> str:
        """Start *harness_argv* through *builder* on this server; answer its pane."""
        cwd = cwd or str(self.tmp)
        pane = None
        if builder == "new-session":
            argv = layout.session_argv(session=tag, conf="/dev/null", socket=socket,
                                       cols=80, rows=24, harness_argv=harness_argv,
                                       chat=f"{tag}.1", env=env)
        elif builder == "new-window":
            argv = layout.chat_window_argv(socket=socket, session="base",
                                           chat=f"base.{tag}", cwd=cwd,
                                           harness_argv=harness_argv, env=env)
        else:
            pane = self._placeholder(binary, socket)
            argv = layout.respawn_argv(socket=socket, harness_pane=pane, env=env or {},
                                       cwd=cwd, harness_argv=harness_argv)
        started = self._tmux(binary, argv)
        self.assertEqual(started.returncode, 0, started.stderr)
        return pane or started.stdout.strip()

    def test_arguments_after_one_ending_in_a_semicolon_never_run_as_tmux_commands(self):
        """**The dangerous shape the escape closes.** Before it, the arguments after a
        harness argument ending in `;` were a tmux command of their own, run on charter's
        server: measured on 3.7c and 3.2, `["a;", "set-option", "-g", "@injected", "yes"]`
        set `@injected`, the harness was handed `["a"]`, and the start returned 0 with an
        empty stderr. Only an operator's own typed arguments reach a harness argv today,
        which makes it a latent surface rather than a hole — and that is exactly why it is
        pinned against a real server and not only at the builder."""
        for binary, socket, records in self.servers:
            for builder in self.BUILDERS:
                option = f"@injected{builder.replace('-', '')}"
                tail = ["a;", "set-option", "-g", option, "yes"]
                with self.subTest(tmux=binary, builder=builder):
                    pane = self._start(binary, socket, builder, tag=f"inj{builder[4:7]}",
                                       harness_argv=self._harness_argv(tail))
                    self.assertEqual(self._received(binary, socket, records, pane), tail)
                    options = self._tmux(binary, ["tmux", "-L", socket, "show-options",
                                                  "-g"]).stdout
                    self.assertNotIn(option, options,
                                     "an argument of the harness ran as a tmux command")

    def _harness_argv(self, arguments: list[str]) -> list[str]:
        return [sys.executable, str(self.recorder), *arguments]

    def test_a_chat_started_in_a_directory_ending_in_a_semicolon_starts_in_it(self):
        work = self.tmp / "work;"
        work.mkdir()
        where = os.path.realpath(work)
        for binary, socket, records in self.servers:
            for builder in ("new-window", "respawn-pane"):
                with self.subTest(tmux=binary, builder=builder):
                    pane = self._start(binary, socket, builder, tag="cwd", cwd=str(work),
                                       harness_argv=self._harness_argv(["x y"]))
                    self.assertEqual(
                        self._record(binary, socket, records, pane)["cwd"], where)
            with self.subTest(tmux=binary, builder="the guest window_argv"):
                started = self._tmux(binary, layout.window_argv(
                    socket=socket, session="base", window="guest", cwd=str(work)))
                self.assertEqual(started.returncode, 0, started.stderr)
                pane = started.stdout.split()[1]
                deadline, path = time.monotonic() + 5, ""
                while time.monotonic() < deadline and path != where:
                    path = self._tmux(binary, ["tmux", "-L", socket, "display-message",
                                               "-p", "-t", pane,
                                               "#{pane_current_path}"]).stdout.strip()
                    time.sleep(0.05)
                self.assertEqual(path, where)

    def test_a_plane_root_ending_in_a_semicolon_reaches_the_harness_exactly(self):
        root = str(self.tmp / "plane;")
        for binary, socket, records in self.servers:
            for builder in self.BUILDERS:
                with self.subTest(tmux=binary, builder=builder):
                    pane = self._start(binary, socket, builder, tag=f"root{builder[4:7]}",
                                       env={"CHARTER_ROOT": root},
                                       harness_argv=self._harness_argv(["x y"]))
                    self.assertEqual(
                        self._record(binary, socket, records, pane)["root"], root)


if __name__ == "__main__":
    unittest.main()
