"""Reap the tmux servers and socket files left behind by suite runs that were killed.

**Measured before it was fixed, and the measurement is the whole design.** On the machine
this was written on, at b3dbd54:

* 14 live tmux servers named ``charter-integration-*``, the oldest running for two and a
  half days, each holding a session and a ``cat``/``sleep``/``charter panel`` child;
* 658 files in ``/tmp/tmux-<uid>/``, 497 of them ``charter-overlay-hatch-*`` — every one
  dated inside the previous 48 hours, at roughly 250 a day.

Three explanations were open (#564): a test aborting before its cleanup, a `kill-server`
that does not take, or a run interrupted before `addCleanup` could run at all. They were
told apart by experiment rather than by argument:

* One clean run of `test_frame_overlay_escape_hatch` — 7 tests, all passing — left the
  socket directory one file SMALLER than it found it. Teardown works.
* The same run `kill -9`'d two seconds in left exactly one socket file **and one live tmux
  server** behind: ``tmux -L charter-overlay-hatch-7754 … new-session -d -s s … cat``.

So it is the third, and **it is not a test bug**. Nothing a `tearDown`, an `addCleanup` or
an `atexit` hook can do survives the signal that skips them, and the deletion sweep runs
this suite once per mutation — killing it whenever a mutation makes it hang. 497 files in
two days is what "once per mutation" looks like from the socket directory's side.

**Which is why this runs at START.** Exit-time cleanup by definition cannot clean up after
a run that had no exit; the only moment that can is the beginning of the NEXT one. A
reaper here is also the only kind that shrinks the backlog already on the machine, rather
than merely declining to add to it.

**What it will touch, stated as a rule rather than a list.**

1. The name must be one this suite hands out: :func:`name`'s ``charter-<slug>-<pid>``.
   The operator's own frame runs on ``charter`` — `commands_frame.SOCKET`, no suffix — and
   cannot match. ``probe-menu-80053`` and the other hand-rolled probe sockets on this
   machine cannot match either, and that is deliberate: they are not in charter's
   namespace, and `tests/_envguard.py` makes the same call about ``EDM_`` for the same
   reason — a guard must not reach sideways into names charter does not own.
2. The pid in the name must be **gone**. A concurrent suite run's socket carries a live
   pid and is left alone, which is what makes this safe to run from every process that
   imports the `tests` package, including two at once.
3. A server that is still listening is **killed before its file is unlinked**, never
   after. `TmuxIntegration._teardown_socket` documents why at length and it holds here
   too: unlink-then-kill points `kill-server` at a path with no server on it and leaves
   the real one running.

**Whether a server is listening is asked of the socket, not of tmux.** A
``tmux -L … kill-server`` per candidate would have been 497 subprocesses on this machine's
first run. One `AF_UNIX` connect answers the same question in microseconds: a live server
accepts, a stale file gives ``ECONNREFUSED``. Only the ones that accept cost a tmux
invocation, and in the steady state there are none.
"""

from __future__ import annotations

import errno
import os
import re
import shutil
import socket
import stat
import subprocess
from pathlib import Path

#: ``charter-<slug>-<pid>``: charter's own prefix at the front and the pid of the process
#: that started the server at the back — the two halves that make a name both OURS and
#: datable. Kept next to :func:`name`, which is the only thing in the suite that produces
#: one, so the rule and its producer cannot drift.
#:
#: Matched with `re.fullmatch`, and written WITHOUT ``^``/``$`` because with `fullmatch`
#: they would buy nothing — a mutation deleting either survived every test in this suite,
#: which is the sweep's own definition of a line that should not be there. ``$`` was worse
#: than redundant: it also matches before a trailing newline, so ``"charter-x-1\n"`` —
#: which is a legal filename — was ours under ``match`` and is not under `fullmatch`.
_OURS = re.compile(r"charter-[a-z0-9]+(?:-[a-z0-9]+)*-(\d+)")

#: How long a `kill-server` on a socket we already know is listening gets. Generous, and
#: spent only on the way to giving up: nothing here waits for it to succeed.
_KILL_TIMEOUT = 10.0


def name(slug: str) -> str:
    """The socket name a test module starts its real tmux server on.

    *slug* says which module owns it (``"overlay-hatch"``, ``"integration-test"``), and the
    pid makes it this PROCESS's — so two suite runs at once cannot collide, which is the
    property the ``-<pid>`` suffix was introduced for, and so a later run can tell whether
    the process that made it is still there, which is the property this module needs.

    One producer for the whole suite, rather than four modules each writing
    ``f"charter-…-{os.getpid()}"``. Four spellings of a rule is four chances for the fifth
    to be spelled in a way the reaper does not recognise — and a socket the reaper does not
    recognise is exactly the leak this exists to end.
    """
    return f"charter-{slug}-{os.getpid()}"


def owns(candidate: str) -> bool:
    """Whether *candidate* is a socket name this suite hands out."""
    return _OURS.fullmatch(candidate) is not None


def socket_dir() -> Path:
    """Where tmux puts a ``-L`` socket, computed the way tmux computes it.

    A COPY of a rule that lives in tmux's source (``$TMUX_TMPDIR`` or ``/tmp``, then
    ``tmux-<uid>/``), which is why nothing here asserts anything about it: there is no
    query command for the directory, only for a running server's own path. Being wrong
    about it costs a reap that finds nothing, never a file removed that should have stayed.
    """
    return Path(os.environ.get("TMUX_TMPDIR") or "/tmp") / f"tmux-{os.getuid()}"


def _alive(pid: int) -> bool:
    """Whether *pid* is a process on this machine.

    ``PermissionError`` counts as alive: the pid exists and belongs to somebody else, which
    is the answer that makes this decline to touch anything.
    """
    if pid <= 0:
        return True                       # not a pid we handed out; leave it alone
    try:
        os.kill(pid, 0)
    except ProcessLookupError:
        return False
    except (PermissionError, OSError):
        return True
    return True


def _listening(path: Path) -> bool:
    """Whether a tmux server may still be accepting on *path*.

    A connect rather than a ``tmux`` invocation, for the cost reason in the module
    docstring — and it is the same act a real client performs, which is why
    `TmuxIntegration._teardown_socket` can measure a server that answers for 0.4 ms after
    ``kill-server`` returned. Connected and closed immediately looks to tmux like a client
    that went away before identifying itself, which it handles as it handles any client
    dying.

    **Only a refusal is taken as "no server".** ``ECONNREFUSED`` and ``ENOENT`` are
    positive answers — nobody is bound, the file is a leftover — and everything else is
    treated as "there might be", because the whole subject here is a server that outlived
    the process named in its socket. Guessing wrong the cautious way costs one ``tmux``
    invocation that prints "no server running"; guessing wrong the other way is the
    resident process #564 counted 14 of.
    """
    sock = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
    try:
        sock.settimeout(0.5)
        sock.connect(str(path))
        return True
    except OSError as exc:
        return exc.errno not in (errno.ECONNREFUSED, errno.ENOENT)
    finally:
        sock.close()


def reapable() -> list[Path]:
    """Every socket file in :func:`socket_dir` this suite left behind and nobody is using.

    Separate from :func:`reap` so a test can ask what would be removed without removing it,
    and so the "assert zero" claim has something to make its claim ABOUT.
    """
    found = []
    try:
        entries = sorted(socket_dir().iterdir())
    except OSError:                       # no socket directory: nothing was ever left
        return found
    for path in entries:
        matched = _OURS.fullmatch(path.name)
        if matched is None:
            continue
        try:
            if not stat.S_ISSOCK(os.stat(path, follow_symlinks=False).st_mode):
                continue                  # not a socket; not ours to remove
        except OSError:
            continue
        if _alive(int(matched.group(1))):
            continue                      # a run still going — possibly this one
        found.append(path)
    return found


def reap() -> list[str]:
    """Kill and unlink everything :func:`reapable` names. Returns the names removed.

    **A file whose server would not die is left where it is**, and that is the whole of
    the ordering argument turned round. Unlinking after a kill that TOOK is #554's fix:
    ``kill-server`` returns 0 with the socket still bound for up to 1.3 ms, and removing
    the file is what closes that window. Unlinking after a kill that did NOT take is #564
    with the evidence destroyed — a resident tmux server holding a session, and no file
    left naming it, so no later run can find it either. Measured, on this file's own first
    draft: 24 servers on this machine with their socket files already gone, unreapable by
    anything short of `ps`.

    So the unlink is gated on the kill's exit status, and a server that refuses to die
    keeps its file and is simply tried again on the next run — which costs one more reap
    and loses nothing, because the file was going to be found again anyway.

    Never raises. This runs at import of the `tests` package, and a machine whose socket
    directory cannot be read must still be able to run the suite — the cost of skipping a
    reap is the backlog this found, and the cost of raising here is no suite at all.
    """
    removed = []
    for path in reapable():
        if _listening(path):
            try:
                killed = subprocess.run(
                    ["tmux", "-L", path.name, "kill-server"],
                    stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL,
                    timeout=_KILL_TIMEOUT, check=False).returncode == 0
            except (OSError, subprocess.SubprocessError):
                killed = False
            if not killed:
                continue                  # still running: keep the file that names it
        try:
            path.unlink()
        except OSError:
            continue
        removed.append(path.name)
    return removed


_installed = False


def install() -> None:
    """Reap once, at import of the `tests` package. Idempotent, and silent.

    Silent because the interesting number is zero and a line saying so on every run is a
    line nobody reads. `test_the_suite_reaps_its_own_tmux_servers` is where the count is
    asserted on, and `reap()` returns it for anyone who wants to look.

    Skipped entirely where tmux is not installed: nothing can have started a server, so
    nothing can be waiting to be reaped, and a machine without tmux should not pay a
    directory scan for it.
    """
    global _installed
    if _installed:
        return
    _installed = True
    if shutil.which("tmux") is None:
        return
    reap()
