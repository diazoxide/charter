"""A launch tmux refuses as too long says so, with tmux's own number.

**tmux takes one command message of at most 16,364 bytes, and refuses past it in one of
two sentences.** Measured on tmux 3.7c and at the 3.2 floor, identically, through
`new-session`, `new-window` and `respawn-pane`, counting the command's arguments from its
name onward with a NUL after each: 16,364 bytes starts; 16,365 to 16,380 is rc 1 and
`failed to send command`; 16,381 and up is rc 1 and `command too long`. `charter claude
"<a pasted spec>"` reaches it.

What the launcher printed for that was `tmuxctl.report_failure`'s generic line — the whole
command, pasted text and all, then tmux's three words — so the one number that explains the
refusal was the one thing not on screen.

**Classified after the fact, never predicted.** Nothing here counts bytes before tmux is
asked: the limit is tmux's to enforce, and a copy of its arithmetic in charter would be a
second answer free to drift from the first. Only once tmux has refused, and only for those
two exact sentences, does a launch say what the refusal was. Any other refusal keeps the
report it always had.
"""

from __future__ import annotations

import os
import shutil
import subprocess
import unittest
from pathlib import Path
from unittest import mock

from charter.frame import launcher, layout, tmuxctl

from tests import _tmuxreap, _tmuxsocket
from tests._isolation import PersonaIso, wired_as_today
from tests.test_frame_launcher import (_FakeOperatorTmux, _FakeTmux, _launch,
                                       _launch_inside)


#: Ruling 10: a profile whose config folder does not carry charter's guard refuses to
#: launch, and that applies to the built-ins every launch test here starts. In-process the
#: suite's `claude` guard makes detection read UNKNOWN, so every one of them would refuse
#: over a fact none of them is about. One fixture for the module, because no test in it is
#: about wiring; `tests/test_a_profile_is_wired_or_refuses.py` is where that is the subject.
_WIRED = None


def setUpModule():
    global _WIRED
    _WIRED = wired_as_today()
    _WIRED.start()


def tearDownModule():
    if _WIRED is not None:
        _WIRED.stop()


_HAS_TMUX = shutil.which("tmux") is not None

#: See `tests/test_a_harness_argument_ending_in_a_semicolon_arrives_whole.py`.
_FLOOR_BIN = (Path.home() / ".local/share/charter-testing"
              / f"tmux-{tmuxctl.FLOOR[0]}.{tmuxctl.FLOOR[1]}")

#: tmux's two sentences for a command past its limit. Spelled by hand rather than read out
#: of `tmuxctl`, so a reworded constant cannot follow itself into a green run.
_TOO_LONG = ("failed to send command", "command too long")

#: The limit as an operator reads it on screen, spelled by hand for the same reason.
_LIMIT_ON_SCREEN = "16,364"

#: What this launch hands the harness: long enough to be the reason, and not ending in `;`,
#: so what tmux was sent is byte-for-byte what was typed.
_PASTED = "fix it: " + "x" * 20000

def _carried(rest: list[str]) -> str:
    """The bytes tmux was really handed for a launch carrying *rest*, spelled as the
    sentence spells them.

    **Charter's own launcher argv, not the profile's command.** Since a chat pane starts as
    `charter frame-launch --profile <name> -- …` (`frame/launcher.argv`), what tmux measured
    against its limit is that list — and a number beside a limit has to be the number the
    limit was applied to (ADR 0009). The command an operator is SHOWN is the profile's own,
    which is a different list and would explain nothing about the refusal.
    """
    return f"{sum(len(os.fsencode(a)) for a in launcher.argv('claude', rest, attended=True)):,}"


#: `_launch`'s harness is the built-in `claude` profile, and this is what starting it costs.
_CARRIED = _carried([_PASTED])


class TmuxsLengthRefusalIsRecognisedByItsExactWords(unittest.TestCase):
    def test_both_of_tmuxs_sentences_are_recognised(self):
        for said in (*_TOO_LONG, *(s + "\n" for s in _TOO_LONG)):
            with self.subTest(stderr=said):
                self.assertTrue(tmuxctl.refused_as_too_long(said))

    def test_nothing_else_tmux_says_is_read_as_one(self):
        """A near miss is a different sentence, and a different sentence is a different
        failure: reading it as this one would put a byte count beside a refusal it does
        not explain."""
        for said in (None, "",
                     f"no server running on {_tmuxsocket.socket_path('charter')}",
                     "no space for a new pane", "create window failed: index 0 in use",
                     "command too long: new-session", "Command too long",
                     "failed to send command to server"):
            with self.subTest(stderr=said):
                self.assertFalse(tmuxctl.refused_as_too_long(said))


class _RefusedStart(_FakeTmux):
    """`_FakeTmux`, except that tmux refuses the command that would start the harness."""

    def __init__(self, *, stderr: str, **kw):
        super().__init__(**kw)
        self.refusal = stderr

    def _one(self, cmd, **kwargs):
        if "new-session" in cmd or "new-window" in cmd:
            self.calls.append(list(cmd))
            return subprocess.CompletedProcess(cmd, 1, stdout="", stderr=self.refusal)
        return super()._one(cmd, **kwargs)


class ALaunchOnCharactersOwnServer(PersonaIso, unittest.TestCase):
    """Both ways a launch on charter's own server starts a harness: the workspace's first
    chat (`new-session`) and a chat joining its live session (`new-window`)."""

    SHAPES = {"new-session": {},
              "new-window": {"pre_existing_sessions": frozenset({"demo"})}}

    def _launched(self, fake: _FakeTmux) -> tuple[int, list[str]]:
        said: list[str] = []
        with mock.patch("charter.util.err", side_effect=said.append):
            rc = _launch(fake, rest=[_PASTED])
        return rc, said

    def test_a_length_refusal_says_the_limit_and_what_this_launch_carried(self):
        for verb, shape in self.SHAPES.items():
            for refusal in _TOO_LONG:
                with self.subTest(verb=verb, stderr=refusal):
                    fake = _RefusedStart(stderr=refusal + "\n", **shape)
                    rc, said = self._launched(fake)
                    self.assertTrue(any(verb in c for c in fake.calls),
                                    f"this case never reached `{verb}`")
                    self.assertEqual(rc, 1)
                    # ONE sentence: the generic report pastes the whole command — twenty
                    # thousand bytes of it here — and would bury this one.
                    self.assertEqual(len(said), 1, said)
                    self.assertIn(_LIMIT_ON_SCREEN, said[0])
                    self.assertIn(_CARRIED, said[0])
                    self.assertIn("in a file", said[0])

    def test_arguments_are_counted_in_the_bytes_exec_was_handed_not_in_characters(self):
        """tmux's limit is in bytes, so the count beside it is too. Two ways to get that
        wrong, one row each: `é` and `漢` are one character and two and three bytes, so
        counting characters comes out short (review round 2 on #959 — with ASCII and a
        surrogate alone, `len(a)` passed every case here); and a byte that is not UTF-8
        reaches `sys.argv` as a surrogate escape, on which a strict `str.encode` raises in
        the middle of a failure report, where `os.fsencode` counts the one byte `exec` was
        handed."""
        said: list[str] = []
        with mock.patch("charter.util.err", side_effect=said.append):
            rc = _launch(_RefusedStart(stderr="command too long\n"),
                         rest=["fix é漢 \udcff " + "x" * 20000])
        self.assertEqual(rc, 1)
        self.assertEqual(len(said), 1, said)
        # The launcher argv, whose tail is that argument: `fix `, é as C3 A9, 漢 as
        # E6 BC A2, a space, the single byte 0xff, a space, then the padding.
        self.assertIn(_carried(["fix é漢 \udcff " + "x" * 20000]), said[0])

    def test_any_other_refusal_keeps_the_report_it_always_had(self):
        rc, said = self._launched(_RefusedStart(stderr="no space for a new pane\n"))
        self.assertEqual(rc, 1)
        self.assertTrue(any("starting the frame failed" in m
                            and "no space for a new pane" in m for m in said), said)
        self.assertFalse(any(_LIMIT_ON_SCREEN in m for m in said), said)


class _RefusedRespawn(_FakeOperatorTmux):
    """`_FakeOperatorTmux`, except that tmux refuses the respawn that starts the harness."""

    def __init__(self, *, stderr: str, **kw):
        super().__init__(**kw)
        self.refusal = stderr

    def _one(self, cmd, **kwargs):
        if "respawn-pane" in cmd:
            self.calls.append(list(cmd))
            return subprocess.CompletedProcess(cmd, 1, stdout="", stderr=self.refusal)
        return super()._one(cmd, **kwargs)


class ALaunchInsideATmuxYouAlreadyHad(PersonaIso, unittest.TestCase):
    """The guest path starts its harness with `respawn-pane`, and the same limit holds."""

    def _launched(self, fake: _FakeOperatorTmux) -> tuple[int, list[str]]:
        said: list[str] = []
        with mock.patch("charter.util.err", side_effect=said.append):
            rc = _launch_inside(fake, rest=[_PASTED])
        return rc, said

    def test_a_length_refusal_says_the_limit_and_what_this_launch_carried(self):
        for refusal in _TOO_LONG:
            with self.subTest(stderr=refusal):
                rc, said = self._launched(_RefusedRespawn(stderr=refusal + "\n"))
                self.assertEqual(rc, 1)
                self.assertEqual(len(said), 1, said)
                self.assertIn(_LIMIT_ON_SCREEN, said[0])
                self.assertIn(_CARRIED, said[0])

    def test_the_placeholder_window_is_still_taken_back(self):
        """A refusal for length leaves the same window behind any refusal does, and it
        goes the same way."""
        fake = _RefusedRespawn(stderr="command too long\n")
        self._launched(fake)
        self.assertTrue(any("kill-window" in c for c in fake.calls), fake.calls)

    def test_any_other_refusal_keeps_the_report_it_always_had(self):
        rc, said = self._launched(_RefusedRespawn(stderr="no pane\n"))
        self.assertEqual(rc, 1)
        self.assertTrue(any("starting the harness in it failed" in m and "no pane" in m
                            for m in said), said)
        self.assertFalse(any(_LIMIT_ON_SCREEN in m for m in said), said)


def _message_bytes(argv: list[str], verb: str) -> int:
    """What tmux's client packs for *argv*: each argument from the command's name on, with
    a NUL after each. The global `-L`/`-f` before the name are the client's own and are not
    in it — measured: `new-session` with `-f` in front and `new-window` without it stopped
    at the same count."""
    return sum(len(a.encode()) + 1 for a in argv[argv.index(verb):])


@unittest.skipUnless(_HAS_TMUX, "no tmux on this machine")
class TmuxStillRefusesInTheWordsCharterRecognises(PersonaIso, unittest.TestCase):
    """The limit and both sentences, asked of a real tmux — so a tmux that moves its limit
    or rewords its refusal fails here, rather than every launch quietly quoting a number
    that is no longer true or falling back to the generic report."""

    def setUp(self) -> None:
        super().setUp()
        v = tmuxctl.version()
        if v is None or v < tmuxctl.FLOOR:
            self.skipTest(f"the frame's floor is tmux {tmuxctl.FLOOR[0]}.{tmuxctl.FLOOR[1]};"
                          f" this machine has {v}")
        self.servers: list[tuple[str, str]] = []
        candidates = [("tmux", "too-long")]
        if _FLOOR_BIN.is_file():
            candidates.append((str(_FLOOR_BIN), "too-long-floor"))
        for binary, slug in candidates:
            socket = _tmuxreap.name(slug)
            self.addCleanup(subprocess.run, [binary, "-L", socket, "kill-server"],
                            capture_output=True, timeout=20)
            started = subprocess.run(
                [binary, "-L", socket, "-f", "/dev/null", "new-session", "-d", "-s",
                 "base", "-x", "80", "-y", "24", "--", "sleep", "600"],
                capture_output=True, text=True, timeout=20)
            self.assertEqual(started.returncode, 0, started.stderr)
            self.servers.append((binary, socket))

    def _window(self, socket: str, size: int) -> list[str]:
        """A `new-window` whose command message is exactly *size* bytes."""
        def build(text: str) -> list[str]:
            return layout.chat_window_argv(socket=socket, session="base", chat="base.9",
                                           cwd=str(self.tmp), harness_argv=["true", text])
        argv = build("x" * (size - _message_bytes(build(""), "new-window")))
        self.assertEqual(_message_bytes(argv, "new-window"), size)
        return argv

    @staticmethod
    def _tmux(binary: str, argv: list[str]) -> subprocess.CompletedProcess:
        return subprocess.run([binary, *argv[1:]], capture_output=True, text=True,
                              timeout=20)

    @staticmethod
    def _which(binary: str) -> str:
        """Which tmux answered, by that binary's own `-V` — at the floor it is not the tmux
        on `$PATH` — for a red that is about that tmux rather than about charter."""
        said = subprocess.run([binary, "-V"], capture_output=True, text=True, timeout=20)
        return f"{binary} reports {said.stdout.strip()!r}"

    def test_a_command_exactly_at_the_limit_is_taken(self):
        for binary, socket in self.servers:
            with self.subTest(tmux=binary):
                got = self._tmux(binary, self._window(socket, tmuxctl.MESSAGE_LIMIT))
                self.assertEqual(got.returncode, 0, f"{got.stderr} — {self._which(binary)}")

    def test_one_byte_past_it_is_refused_in_words_charter_recognises(self):
        for binary, socket in self.servers:
            with self.subTest(tmux=binary):
                got = self._tmux(binary, self._window(socket, tmuxctl.MESSAGE_LIMIT + 1))
                self.assertNotEqual(got.returncode, 0, self._which(binary))
                self.assertTrue(tmuxctl.refused_as_too_long(got.stderr),
                                f"{got.stderr!r} — {self._which(binary)}")

    def test_far_past_it_is_refused_in_words_charter_recognises(self):
        for binary, socket in self.servers:
            with self.subTest(tmux=binary):
                got = self._tmux(binary, self._window(socket, 2 * tmuxctl.MESSAGE_LIMIT))
                self.assertNotEqual(got.returncode, 0, self._which(binary))
                self.assertTrue(tmuxctl.refused_as_too_long(got.stderr),
                                f"{got.stderr!r} — {self._which(binary)}")

    def test_the_sentence_changes_at_the_byte_the_docs_name(self):
        """`docs/frame.md` and `tmuxctl._TOO_LONG` both say where tmux's sentence changes —
        16,380 bytes is still `failed to send command`, 16,381 is `command too long` — so
        that byte is asked of a real tmux rather than left standing as a claim."""
        for binary, socket in self.servers:
            for size, sentence in ((16380, "failed to send command"),
                                   (16381, "command too long")):
                with self.subTest(tmux=binary, size=size):
                    got = self._tmux(binary, self._window(socket, size))
                    self.assertEqual(got.stderr.strip(), sentence,
                                     f"at {size:,} bytes — {self._which(binary)}")


if __name__ == "__main__":
    unittest.main()
