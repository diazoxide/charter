"""Suite-wide tripwire: the TERMINAL the suite was launched from is not a fixture either.

`_planeguard` isolates the plane this process resolves and the state directory it writes;
`_envguard` removes the charter variables the operator's shell exports. Both are about what
that shell *says*. This is the fourth fixture, and it is about what the shell *is*: three
open file descriptors, and whether they are a terminal.

**The failure this closes is a HANG, which is worse than a wrong answer.** A wrong answer
is a verdict; a hang is not a pass, not a fail, and not a report::

    $ python3 -m unittest discover -s tests            # stdin a pipe, or CI
    Ran 6636 tests in 442s
    OK
    $ python3 -m unittest discover -s tests            # stdin a terminal
    test_the_frames_own_workspace_is_written_down_on_both_paths
    (tests.test_frame_launcher.ALaunchFillsTheCacheItJustEmptied…)
       ← blocks here, indefinitely

`commands_frame._picker_wanted` gates #518's workspace picker on **two** independent
questions — ``sys.stdin.isatty() and sys.stdout.isatty()`` — by design, because
``charter claude < /dev/null`` in a terminal has one and not the other.
`tests/test_frame_launcher.py` pinned the second of that pair and left the first to the
ambient shell, so what happened next was decided by whichever stdin the runner happened to
have: a pipe answered "no picker" and went green, and a terminal sat on charter's own
prompt waiting for a human who was never going to type (#545). Measured with `input()`
replaced by a recording raise and the suite run under a pty: **122 tests reached that
prompt**, in one module, on a tree whose CI has been green throughout. CI has never seen it
and never can — its stdin is not a terminal — so the environment that reports the suite's
health is precisely the one blind to this.

**And the same fixture goes the other way, quietly.** `test_mcp_approval` records its own
instance in full: `util._USE_COLOR` is ``sys.stderr.isatty()`` evaluated at import, so
running from a terminal put charter's own ANSI codes into a transcript that test derives
its expectations from — and a mutation that dies under a pipe "reported OK under a pty".
A test that stops testing when the run happens to be interactive is worse than one that
hangs, because nothing about it looks wrong.

**Two moves, closing opposite directions — the shape `_envguard` settled.**

1. *The ambient answer is removed*, once, before `charter` is first imported. ``sys.stdin``
   is replaced with a stream that is not a terminal and holds nothing, and ``sys.stdout``
   and ``sys.stderr`` answer :meth:`isatty` ``False``. That is the answer CI gives and the
   one every assertion in this suite was written against. It takes effect for every test at
   once, it covers the reads that happen at import (`util._USE_COLOR` above) where no test
   is running to declare anything, and — because ``input()`` reads ``sys.stdin`` — it means
   a path that does reach a prompt raises ``EOFError`` instead of blocking forever.
2. *A targeted read of* ``sys.stdin.isatty()`` *is REFUSED* while a test runs, unless that
   test said which answer it wanted. Removal alone would only silence the red: every one of
   those 122 tests would still be asserting against a terminal-ness it never chose, and the
   next test that MEANS "there is a human here" would get ``False`` and pass vacuously.

**Only stdin is refused, and the boundary is the same one `_envguard` draws for its loud
set.** Every stream is answered, so no test's result can differ between two machines
either way; loudness is worth its cost only where the answer is a CLAIM ABOUT THE WORLD the
test runs in. ``sys.stdout.isatty()`` decides *formatting and routing* — colour, and whether
`cmd_launch` bypasses the frame — and the suite already treats it as a stated fixture: it is
pinned at thirteen sites in the launcher module alone. ``sys.stdin.isatty()`` decides
whether charter **stops and asks a human**, which is the one question whose wrong answer is
a process that never returns. That is the tier-two criterion, and it selects exactly the
half nobody was pinning.

**How a test declares.** Three ways, and two of them are what tests already do:

* ``mock.patch("sys.stdin.isatty", return_value=False)`` — the pair to the
  ``sys.stdout.isatty`` pin sitting next to it, and what `_launch` and `_launch_inside` in
  `test_frame_launcher.py` now carry. ``True`` for a test that wants the prompt (it must
  then drive `_read` itself, which is the right way round).
* ``mock.patch("sys.stdin", <something>)`` — replacing the stream entirely, the way the
  hook-driving helpers and `test_mcp_approval`'s ``_Tty`` do. The guard's stream is not
  consulted at all, so this declares by construction.
* :func:`no_terminal`, for a case with nothing to patch that simply means "nobody is
  watching this run".

**Scoped to one test and reset at its boundary**, by wrapping `unittest.TestCase.run` — the
same mechanism and the same save/restore as `_envguard`, and for the same reason: a few
tests run an inner `TestCase` inside their own body, and the inner run must not disarm the
outer one.

**Disarmed outside a test, deliberately.** Import-time reads happen before any test could
declare anything, and refusing there would refuse the suite's own boot. Move 1 has already
made those reads deterministic; it is only the *loudness* that waits for a test to run.

**Raised as a `BaseException`**, for the reason `_planeguard.RealPlaneRead` documents:
charter is full of ``except Exception`` fallbacks that would turn this tripwire into a
degraded code path, and `unittest` records a `BaseException` against the test that raised
it, so the failure keeps its name.

**What this cannot see.** A subprocess gets its own file descriptors, and it inherits this
process's — so a child charter really can find a terminal on fd 0. Nothing in the suite
reaches a prompt that way today (`RealPlaneSpawn` already refuses the children that would
resolve a real plane, and the rest are driven with explicit input), and closing it would
mean rewriting fd 0 for every child, which is a bigger change than the hole justifies. It
is written down here rather than left to be discovered.
"""

from __future__ import annotations

import os
import sys
import unittest


class AmbientTerminalRead(BaseException):
    """A test asked whether stdin is a terminal without saying which answer it wanted."""


#: What the three streams actually answered when the suite started, for anything that
#: genuinely has to know what the operator's terminal looked like. Nothing needs it today;
#: it exists so that such a case has somewhere honest to ask instead of the guard being
#: switched off. `_envguard.scrubbed` is the same courtesy for the same reason.
_AMBIENT: dict[str, bool] = {}

#: The replacement stream. Kept so `install` is idempotent and so a test can recognise it.
_STDIN = None

_installed = False

#: Armed only while a test is running. See the module docstring.
_active = False

#: Whether THIS test has said which answer it wants. Reset — saved and restored — around
#: every `TestCase.run`.
_declared = False


def _explain() -> str:
    from . import _planeguard
    return (
        "REFUSED: read of sys.stdin.isatty()\n"
        f"{_planeguard._current_test()} asked whether stdin is a terminal without saying "
        f"which answer it wants, so the answer would be a reading of the shell the suite "
        f"was launched from. That question decides whether charter STOPS AND ASKS A HUMAN: "
        f"`commands_frame._picker_wanted` opens #518's workspace picker on it, "
        f"`commands_secrets._read_value` falls through to `getpass`, and "
        f"`commands_report._body` waits on a pipe. Under a pipe (CI, an agent's shell) all "
        f"three answer 'nobody is there' and the test goes green; under a terminal — a "
        f"developer, a charter frame — it blocks forever, which is not a pass, not a fail "
        f"and not a report. 122 tests reached that prompt in one module (#545). Three ways "
        f"out, and pick the one that says what this test means:\n"
        f"  - `mock.patch(\"sys.stdin.isatty\", return_value=False)` — the pair to the "
        f"`sys.stdout.isatty` pin that is probably already beside it;\n"
        f"  - `mock.patch(\"sys.stdin\", io.StringIO(\"...\"))` if the test has input to "
        f"give, or a stub whose `isatty` says True if it MEANS to drive the prompt;\n"
        f"  - `tests._ttyguard.no_terminal()` for a case with nothing to patch.")


def _isatty() -> bool:
    if _active and not _declared:
        raise AmbientTerminalRead(_explain())
    return False


def no_terminal() -> None:
    """Declare, for the rest of this test, that nobody is watching the run.

    The answer CI gives. For a case that has no stdin to patch and simply needs charter to
    take the non-interactive branch — the equivalent of `_envguard.unset`.
    """
    global _declared
    _declared = True


def ambient() -> dict[str, bool]:
    """What ``stdin``/``stdout``/``stderr`` answered before the guard replaced them."""
    return dict(_AMBIENT)


def install() -> None:
    """Answer for all three streams, and arm the stdin refusal per test. Idempotent.

    Called at import of the `tests` package, and **first** — above `_envguard`, which is
    what pulls `charter` in. `charter.util` computes ``_USE_COLOR`` from
    ``sys.stderr.isatty()`` at ITS import, so an install one line later would leave the
    whole suite's output decoration decided by the operator's terminal.
    """
    global _installed, _STDIN
    if _installed:
        return
    _installed = True

    for name in ("stdin", "stdout", "stderr"):
        stream = getattr(sys, name, None)
        try:
            _AMBIENT[name] = bool(stream.isatty())
        except (AttributeError, OSError, ValueError):
            # A stream that cannot answer is not a terminal for any purpose here, and a
            # guard that raised while installing would take the suite down at import.
            _AMBIENT[name] = False

    # A REAL file object, not a stub: `fileno()`, `read()`, `buffer` and `close()` all have
    # to keep working, because tests hand stdin to subprocesses and to `input()`. `devnull`
    # is also the only choice that makes a read return "" rather than block — so a path
    # that reaches a prompt despite everything raises `EOFError` instead of hanging.
    _STDIN = open(os.devnull)
    _STDIN.isatty = _isatty
    sys.stdin = _STDIN

    for stream in (sys.stdout, sys.stderr):
        try:
            stream.isatty = lambda: False
        except (AttributeError, TypeError):     # pragma: no cover - not seen on CPython
            pass

    original_run = unittest.TestCase.run

    def run(self, result=None):
        global _active, _declared
        outer_active, outer_declared = _active, _declared
        _active, _declared = True, False
        try:
            return original_run(self, result)
        finally:
            _active, _declared = outer_active, outer_declared

    run.__module__ = __name__
    unittest.TestCase.run = run
