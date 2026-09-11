r"""A chat starts in the directory it was launched from, whatever `#` that name carries (#961).

**tmux reads a `-c <start-directory>` as a FORMAT before it looks for the directory**, and
says nothing when the answer is somewhere else. Measured on tmux 3.7c and at the 3.2 floor
through `new-window -c` and `respawn-pane -c` — the two commands `frame/layout.py` hands a
directory to — with every start returning 0 and an empty stderr:

* `x#{session_name}` started in `xbase` when a directory by that name sat beside it, and in
  `$HOME` when none did. `a#S`, `a#,b`, `a#}b`, `a#{b` and `a##b` went to `$HOME` too.
* `y#(touch job-ran)` went to `$HOME`. Sent as ONE command, after which the client
  disconnects, its command ran in 0 of 30 starts per command. With the client held
  connected by a chained `run-shell 'sleep 1'`, it ran in 5 of 5.
* On 3.7c and not on 3.2, a trailing `#` was dropped: `trail#` went to `$HOME`, and a
  directory named `#` started in its parent.
* A run of `#` directly before `[` arrived exactly as it was — `a#[b`, `a##[b`, `a###[b`,
  `a####[b`, `a#[fg=red]b` — and each of those, DOUBLED, went to `$HOME`.

`tmuxctl.start_directory` is the escape those measurements describe. `new-session` — a
workspace's first chat — is handed no directory, and from a working directory named
`x#{session_name}` it started exactly there on both versions, with and without a server
already running.

`-e NAME=VALUE` is not read as a format: through `new-session`, `new-window` and
`respawn-pane` on both versions, values carrying `#{session_name}`, `##`, `#S`, `#[` and
`#(…)` each arrived exactly. So identity values keep #959's escape and no other, and
:meth:`ARealTmuxStartsTheChatInThatDirectory.test_an_identity_value_carrying_a_format_reaches_the_harness_exactly`
keeps that measurement.
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
#: `test_a_harness_argument_ending_in_a_semicolon_arrives_whole` names it. Absent on CI, so
#: the floor rows there are simply not run — the `tmux` on `$PATH` still is.
_FLOOR_BIN = (Path.home() / ".local/share/charter-testing"
              / f"tmux-{tmuxctl.FLOOR[0]}.{tmuxctl.FLOOR[1]}")

#: ``(directory, what tmux has to be handed for the chat to start in it)``.
_ESCAPED = (
    # A format, a shell job, a short alias, and the characters tmux reads after a `#`.
    ("/w/x#{session_name}", "/w/x##{session_name}"),
    ("/w/y#(touch job-ran)", "/w/y##(touch job-ran)"),
    ("/w/a#S", "/w/a##S"),
    ("/w/a#,b", "/w/a##,b"),
    ("/w/a#}b", "/w/a##}b"),
    ("/w/a#{b", "/w/a##{b"),
    ("/w/a#1", "/w/a##1"),
    # A trailing `#`, which 3.7c drops.
    ("/w/trail#", "/w/trail##"),
    ("/w/#", "/w/##"),
    # `##` in a NAME is two literal `#`, so it is doubled like any other pair.
    ("/w/a##b", "/w/a####b"),
    ("/w/a##{session_name}", "/w/a####{session_name}"),
    ("/w/a\\#{b", "/w/a\\##{b"),
    # A `[` keeps only the run directly in front of it.
    ("/w/a#[#{session_name}", "/w/a#[##{session_name}"),
    ("/w/a#[##b", "/w/a#[####b"),
    ("/w/a#[b]#S", "/w/a#[b]##S"),
    # And #957's trailing `;`, with a `#` and without one.
    ("/w/z#{session_name};", "/w/z##{session_name}\\;"),
    ("/w/a#;", "/w/a##\\;"),
    ("/w/a#[;", "/w/a#[\\;"),
    ("/w/dir;", "/w/dir\\;"),
)

#: What tmux already hands on exactly, which the escape must leave byte-for-byte alone.
_UNCHANGED = (
    "", "/", "/w/plain dir", "/w/a;b",
    "/w/a#[b", "/w/a##[b", "/w/a###[b", "/w/a####[b", "/w/a#[fg=red]b",
    "/w/#[x", "/w/a#[b#[c", "/w/a #[ b", "/w/a#[",
)


class TheEscapeIsExactlyWhatTmuxUndoes(unittest.TestCase):
    """`tmuxctl.start_directory` over the measured names, with no tmux running."""

    def test_every_hash_tmux_reads_as_a_format_is_doubled(self):
        for where, sent in _ESCAPED:
            with self.subTest(where=where):
                self.assertEqual(tmuxctl.start_directory(where), sent)

    def test_hashes_already_doubled_in_a_name_are_doubled_again(self):
        """A directory named `a##b` has two `#` in it. tmux reads `##` as one, so it has to
        be handed `####` for the chat to start in `a##b`."""
        self.assertEqual(tmuxctl.start_directory("/w/a##b"), "/w/a####b")

    def test_what_tmux_already_hands_on_whole_is_left_alone(self):
        for where in _UNCHANGED:
            with self.subTest(where=where):
                self.assertEqual(tmuxctl.start_directory(where), where)

    def test_the_two_escapes_give_one_string_in_either_order(self):
        """`verbatim` puts a `\\` before a final `;` and nothing else. The `#` half never
        makes or unmakes a final `;`, and neither `\\` nor `;` is a character that decides
        whether a run of `#` is doubled — so the order cannot change the string. Pinned
        here, so a change to either half that makes the order matter is caught, and on
        tmux 3.7c and 3.2 both orders were measured to start the chat in the same place for
        every name in this module."""
        for where in tuple(w for w, _ in _ESCAPED) + _UNCHANGED:
            with self.subTest(where=where):
                hashes_first = tmuxctl.verbatim(tmuxctl._literal_hashes(where))
                semicolon_first = tmuxctl._literal_hashes(tmuxctl.verbatim(where))
                self.assertEqual(hashes_first, semicolon_first)
                self.assertEqual(tmuxctl.start_directory(where), hashes_first)


class EveryBuilderThatTakesADirectoryEscapesIt(unittest.TestCase):
    """The three builders that pass `-c`, each asked on its own — a builder that forgot the
    escape would otherwise hide behind the two that remembered it."""

    WHERE = "/work/x#{session_name};"
    SENT = "/work/x##{session_name}\\;"

    @staticmethod
    def _after(argv: list[str], flag: str) -> str:
        return argv[argv.index(flag) + 1]

    def test_the_directory_is_handed_to_tmux_escaped(self):
        for name, argv in (
                ("window_argv", layout.window_argv(
                    socket=_tmuxsocket.OPERATOR_SOCKET, session="$1", window="w.1",
                    cwd=self.WHERE)),
                ("chat_window_argv", layout.chat_window_argv(
                    socket="charter", session="w", chat="w.2", cwd=self.WHERE,
                    harness_argv=["prog"])),
                ("respawn_argv", layout.respawn_argv(
                    socket=_tmuxsocket.OPERATOR_SOCKET, harness_pane="%7", env={},
                    cwd=self.WHERE, harness_argv=["prog"]))):
            with self.subTest(builder=name):
                self.assertEqual(self._after(argv, "-c"), self.SENT)

    def test_an_identity_value_is_not_escaped_as_a_format(self):
        """`-e` is not read as a format (module docstring), so a `#` doubled here would
        reach the harness doubled."""
        env = {"CHARTER_ROOT": "/plane/x#{session_name}"}
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
                self.assertEqual(self._after(argv, "-e"),
                                 "CHARTER_ROOT=/plane/x#{session_name}")


@unittest.skipUnless(_HAS_TMUX, "no tmux on this machine")
class ARealTmuxStartsTheChatInThatDirectory(PersonaIso, unittest.TestCase):
    """Where a real tmux started the pane, read back as its `pane_current_path`.

    Every tmux client here runs from `client/` and every server's `$HOME` is `home/`, both
    inside the test's own tree: a fallback lands somewhere the assertion can name, and a
    `#(…)` job that did run could only write in here.
    """

    #: The three ways a directory reaches tmux, each asked on its own below.
    BUILDERS = ("new-window", "respawn-pane", "guest window")

    def setUp(self) -> None:
        super().setUp()
        v = tmuxctl.version()
        if v is None or v < tmuxctl.FLOOR:
            self.skipTest(f"the frame's floor is tmux {tmuxctl.FLOOR[0]}.{tmuxctl.FLOOR[1]};"
                          f" this machine has {v}")
        self.home = self.tmp / "home"
        self.client = self.tmp / "client"
        self.home.mkdir()
        self.client.mkdir()
        self.env = dict(os.environ, HOME=str(self.home))
        self.recorder = self.tmp / "recorder.py"
        self.recorder.write_text(
            "import json, os, sys\n"
            "with open(sys.argv[1] + '.tmp', 'w') as f:\n"
            "    json.dump({'root': os.environ.get('CHARTER_ROOT')}, f)\n"
            "os.replace(sys.argv[1] + '.tmp', sys.argv[1])\n")
        #: ``(binary, socket)`` for every tmux this machine has.
        self.servers: list[tuple[str, str]] = []
        candidates = [("tmux", "hash-cwd")]
        if _FLOOR_BIN.is_file():
            candidates.append((str(_FLOOR_BIN), "hash-cwd-floor"))
        for binary, slug in candidates:
            socket = _tmuxreap.name(slug)
            # Killed whether or not the start below worked: a server that half-started is
            # still a server, and the reaper only sees it once this process is gone.
            self.addCleanup(subprocess.run, [binary, "-L", socket, "kill-server"],
                            capture_output=True, timeout=20)
            started = self._tmux(binary, ["tmux", "-L", socket, "-f", "/dev/null",
                                          "new-session", "-d", "-s", "base", "-x", "80",
                                          "-y", "24", "--", "sleep", "600"])
            self.assertEqual(started.returncode, 0, started.stderr)
            self.servers.append((binary, socket))

    def _tmux(self, binary: str, argv: list[str], *then: str) -> subprocess.CompletedProcess:
        """*argv* as a builder made it, run by *binary* instead of the `tmux` it names, with
        *then* chained after it in the same invocation."""
        return subprocess.run([binary, *argv[1:], *then], capture_output=True, text=True,
                              timeout=20, env=self.env, cwd=str(self.client))

    def _start(self, binary: str, socket: str, builder: str, where: str,
               *then: str) -> tuple[str, str]:
        """Start a pane in *where* through *builder*; answer its id and the program in it."""
        if builder == "new-window":
            started = self._tmux(binary, layout.chat_window_argv(
                socket=socket, session="base", chat="base.2", cwd=where,
                harness_argv=["sleep", "600"]), *then)
            self.assertEqual(started.returncode, 0, started.stderr)
            return started.stdout.split()[0], "sleep"
        if builder == "respawn-pane":
            made = self._tmux(binary, ["tmux", "-L", socket, "new-window", "-d", "-a", "-t",
                                       "base", "-P", "-F", "#{pane_id}", "--",
                                       *layout.PLACEHOLDER])
            self.assertEqual(made.returncode, 0, made.stderr)
            pane = made.stdout.strip()
            started = self._tmux(binary, layout.respawn_argv(
                socket=socket, harness_pane=pane, env={}, cwd=where,
                harness_argv=["sleep", "600"]), *then)
            self.assertEqual(started.returncode, 0, started.stderr)
            return pane, "sleep"
        started = self._tmux(binary, layout.window_argv(
            socket=socket, session="base", window="guest", cwd=where), *then)
        self.assertEqual(started.returncode, 0, started.stderr)
        return started.stdout.split()[1], layout.PLACEHOLDER[0]

    def _path(self, binary: str, socket: str, pane: str, program: str) -> str:
        """*pane*'s `pane_current_path`, read once `pane_current_command` names *program*.

        Waiting on the command rather than on the path is what makes a wrong directory fail
        at once: a wait for the RIGHT path would sit out its deadline on every row that is
        wrong, and a red run is made of exactly those rows. The respawn's placeholder runs a
        different program from the harness it is replaced by, so the reading cannot be the
        placeholder's.
        """
        deadline = time.monotonic() + 10
        while time.monotonic() < deadline:
            running = self._tmux(binary, ["tmux", "-L", socket, "display-message", "-p",
                                          "-t", pane, "#{pane_current_command}"])
            if running.stdout.strip() == program:
                return self._tmux(binary, ["tmux", "-L", socket, "display-message", "-p",
                                           "-t", pane, "#{pane_current_path}"]
                                  ).stdout.rstrip("\n")
            time.sleep(0.02)
        self.fail(f"{program} never ran in {pane}")

    def _starts_in(self, name: str, *, sibling: str | None = None) -> None:
        """Through every builder on every tmux: a pane started in a directory called *name*
        is in exactly that directory. *sibling* is made beside it — the directory tmux's
        expansion of *name* would otherwise reach."""
        for binary, socket in self.servers:
            for builder in self.BUILDERS:
                parent = self.tmp / "dirs" / f"{len(list(self.tmp.glob('dirs/*')))}"
                (parent / name).mkdir(parents=True)
                if sibling:
                    (parent / sibling).mkdir()
                where = os.path.realpath(parent / name)
                with self.subTest(tmux=binary, builder=builder, name=name, sibling=sibling):
                    pane, program = self._start(binary, socket, builder, str(parent / name))
                    self.assertEqual(self._path(binary, socket, pane, program), where)
                    self._tmux(binary, ["tmux", "-L", socket, "kill-window", "-t", pane])

    def test_a_directory_tmux_would_read_as_a_format_is_where_the_chat_starts(self):
        """Every row here started somewhere else before the escape, on both versions."""
        self._starts_in("x#{session_name}", sibling="xbase")
        self._starts_in("x#{session_name}")
        self._starts_in("a#S")
        self._starts_in("a##b")
        self._starts_in("z#{session_name};")

    def test_a_trailing_hash_stays_in_the_name(self):
        """3.7c dropped a trailing `#` — `trail#` went to `$HOME` and `#` to its parent —
        and 3.2 kept it, so on the floor binary these rows pass with or without the
        escape."""
        self._starts_in("trail#")
        self._starts_in("#")

    def test_a_run_of_hashes_before_a_bracket_is_not_doubled(self):
        """A guard on the escape's one exception rather than on the defect: the first three
        names started in the right place before the escape and pass without it. Doubled
        like every other `#`, each went to `$HOME` on both versions. The fourth moved
        before the escape, and only the `#` after the `[` may be doubled for it to land."""
        self._starts_in("a#[b")
        self._starts_in("a##[b")
        self._starts_in("a#[fg=red]b")
        self._starts_in("a#[#{session_name}")

    def _took(self, name: str) -> list[str]:
        """Every file called *name* under this test's tree, removed once found — so one
        builder's job cannot be counted against the next."""
        found = sorted(self.tmp.rglob(name))
        for path in found:
            path.unlink()
        return [str(path.relative_to(self.tmp)) for path in found]

    def test_a_directory_named_like_a_shell_job_starts_the_chat_there_and_runs_nothing(self):
        """**Held connected, or this could not go red.** Measured on 3.7c and 3.2, a
        `#(…)` in `-c` ran its command in 5 of 5 starts whose client a chained
        `run-shell 'sleep 1'` kept connected, and in 0 of 30 sent as one command. So each
        start here is held the same way, and a control — a `#(…)` through `display-message`,
        held identically — first shows that a job CAN write its marker from here."""
        hold = (";", "run-shell", "sleep 1")
        for binary, socket in self.servers:
            with self.subTest(tmux=binary, control="display-message"):
                shown = self._tmux(binary, ["tmux", "-L", socket, "display-message", "-p",
                                            "-t", "base", "control#(touch control-ran)"],
                                   *hold)
                self.assertEqual(shown.returncode, 0, shown.stderr)
                self.assertNotEqual(self._took("control-ran"), [],
                                    "a #(…) job held connected wrote no marker, so the "
                                    "check below could not tell a job from none")
            for builder in self.BUILDERS:
                parent = self.tmp / "jobs" / f"{len(list(self.tmp.glob('jobs/*')))}"
                (parent / "y#(touch job-ran)").mkdir(parents=True)
                where = parent / "y#(touch job-ran)"
                with self.subTest(tmux=binary, builder=builder):
                    pane, program = self._start(binary, socket, builder, str(where), *hold)
                    self.assertEqual(self._took("job-ran"), [],
                                     "the directory's name ran as a shell command")
                    self.assertEqual(self._path(binary, socket, pane, program),
                                     os.path.realpath(where))
                    self._tmux(binary, ["tmux", "-L", socket, "kill-window", "-t", pane])

    def test_an_identity_value_carrying_a_format_reaches_the_harness_exactly(self):
        """The `-e` half of the measurement, kept: it passes before and after the escape,
        and goes red if identity values are ever doubled as though tmux expanded them."""
        root = str(self.tmp / "plane" / "x#{session_name}")
        for n, (binary, socket) in enumerate(self.servers):
            for builder in ("new-session", "new-window", "respawn-pane"):
                record = self.tmp / f"root-{n}-{builder}.json"
                harness = [sys.executable, str(self.recorder), str(record)]
                env = {"CHARTER_ROOT": root}
                with self.subTest(tmux=binary, builder=builder):
                    if builder == "new-session":
                        argv = layout.session_argv(session="r", conf="/dev/null",
                                                   socket=socket, cols=80, rows=24,
                                                   harness_argv=harness, chat="r.1", env=env)
                    elif builder == "new-window":
                        argv = layout.chat_window_argv(socket=socket, session="base",
                                                       chat="base.r", cwd=str(self.tmp),
                                                       harness_argv=harness, env=env)
                    else:
                        pane, _ = self._start(binary, socket, "respawn-pane", str(self.tmp))
                        argv = layout.respawn_argv(socket=socket, harness_pane=pane, env=env,
                                                   cwd=str(self.tmp), harness_argv=harness)
                    started = self._tmux(binary, argv)
                    self.assertEqual(started.returncode, 0, started.stderr)
                    deadline = time.monotonic() + 10
                    while not record.is_file() and time.monotonic() < deadline:
                        time.sleep(0.02)
                    self.assertEqual(json.loads(record.read_text())["root"], root)


if __name__ == "__main__":
    unittest.main()
