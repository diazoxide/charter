"""Suite-wide tripwire: no test may REPLACE the process that is running the suite.

`charter` execs in two places, and both are the point of the code they are in:
`commands_frame.bypass` (`os.execvp`, a harness with no frame) and `frame/launcher.attempt`
(`os.execvpe`, the profile's command taking over its pane). A test reaches either with the
exec stood in — `launcher.os.execvpe` patched, or `os.execvp` patched to raise — and every
test that remembered to is fine.

**A test that did not remember is not a failure, it is a missing verdict.** A real exec
from the test process replaces the test runner: no summary, no exit code of its own, and
whatever the exec'd program does next — measured 2026-09-13 on #998's deletion sweep, whose
`if selecting:` mutant (every launch treated as a selector launch) sent
`tests.test_a_profile_launch_is_refused_before_tmux` through `bypass` into a real
`python -P -m charter frame-launch --select`. Locally that printed `No module named charter`
in place of the run; on the sweep it came back neither green nor red — **unresolved**, the
same no-verdict a hang produces. A mutation nobody can measure is a guard nobody knows is
pinned.

**So an exec from the suite's own process fails the test that made it, by name.** The two
primitives every `os.exec*` spelling ends in — `os.execv` and `os.execve`, which
`os._execvpe` looks up at call time — are replaced once, at import of the `tests` package:

* **a program that does not exist, or cannot be executed, still raises what the kernel
  would** (`FileNotFoundError`, `PermissionError`). Nothing is replaced when an exec fails,
  so that path is real behaviour and not a hazard: `bypass`'s own "not installed" sentence
  and exit 127 are tested exactly that way, and `os.execvp` walks `PATH` by trying each
  candidate and catching the first of those;
* **a program that WOULD run raises :class:`ReplacedTheRunner`**, a `BaseException` like
  every tripwire in `tests/_planeguard.py`: not an `OSError`, so no `except OSError` in
  charter and no candidate loop in `os._execvpe` can take it for an ordinary failure to
  start — and not an `Exception` either, so no `except Exception` on the path between the
  exec and the test (`cli.main` has one) can swallow it into a green;
* **a forked child is never refused.** `pty.fork()` followed by `os.execvp("tmux", …)` is how
  a real-terminal case puts a real client on a server, and after the fork the process
  replaced is the child's own, not the suite's. Told apart by pid, recorded at install.

A test that patches `os.execvp`, `os.execvpe` or `launcher.os.execvpe` itself never reaches
these primitives at all, which is the right answer: nothing is exec'd.
"""

from __future__ import annotations

import errno
import os

#: The suite's own process — the one process an exec must never replace.
_RUNNER_PID: int | None = None

_installed = False


class ReplacedTheRunner(BaseException):
    """A test exec'd a real program from the process running the suite.

    `BaseException`, for `_planeguard.RealTmuxReach`'s reason: a tripwire an `except
    Exception` can catch is a tripwire the code under test can turn into a pass.
    """


def _tripwire(name: str, real):
    def exec_(path, *rest):
        if os.getpid() != _RUNNER_PID:
            return real(path, *rest)
        shown = os.fsdecode(path)
        if not os.path.exists(path):
            raise FileNotFoundError(errno.ENOENT, os.strerror(errno.ENOENT), shown)
        if os.path.isdir(path) or not os.access(path, os.X_OK):
            raise PermissionError(errno.EACCES, os.strerror(errno.EACCES), shown)
        raise ReplacedTheRunner(
            f"a test called os.{name}({shown!r}, …) from the process running the suite — "
            f"that replaces the test runner and leaves no verdict at all. Stand the exec "
            f"in (`launcher.os.execvpe`, or `os.execvp` for `commands_frame.bypass`). "
            f"See tests/_execguard.py.")

    exec_.__name__ = name
    return exec_


def install() -> None:
    """Replace the two exec primitives for the suite's own process. Idempotent."""
    global _installed, _RUNNER_PID
    if _installed:
        return
    _installed = True
    _RUNNER_PID = os.getpid()
    os.execv = _tripwire("execv", os.execv)
    os.execve = _tripwire("execve", os.execve)
