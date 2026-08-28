"""The socket path a real tmux would hand this machine — asked, never spelled (#601).

`tests/test_frame_launcher.py` carried a socket path with ``tmux-502`` in the middle of
it — **502 is one developer's uid**, baked into a path. On any other machine that path is
either absent or — worse — someone else's socket directory. Five more modules carried the
same string, and `test_no_test_reads_the_operators_shell.py` carried a `501`.

It is the same defect class the whole test-hygiene cluster closed: **the suite reads the
machine instead of the repo.** The shell (#519/#521/#528), the plane (#402/#492), the
filesystem (#529), the clock (#494), the working directory (#537), real credentials and a
real keyboard (#546/#545), forked children and leaked servers (#542/#564) and ``$COLUMNS``
(#544) all went the same way: ask, or state — never assume the developer's answer.

**How tmux itself computes it, because that is what makes this a measurement rather than a
second guess.** ``$TMUX_TMPDIR`` when it is set and non-empty, otherwise ``_PATH_TMP`` —
which is the literal ``/tmp`` in tmux's own source, **not** ``$TMPDIR``. That distinction
is load-bearing on macOS, where ``tempfile.gettempdir()`` answers a per-user
``/var/folders/…`` directory that tmux never puts anything in. Then ``tmux-<uid>``, then
the socket name (``default`` unless ``-L``/``-S`` says otherwise).

**And the result is RESOLVED**, which is the other half of #601 and the reason the
operator's measurement said ``/private/tmp`` rather than ``/tmp``: tmux calls
``realpath()`` on that base before it uses it, and on macOS ``/tmp`` is a symlink to
``/private/tmp``. The same distinction is what made `tools/sweep.py` unusable on macOS with
its default workdir (#572) — ``os.getcwd()`` returns the resolved path while a constructed
prefix does not, so ``startswith`` never matched and the selection map came back empty. A
test that spells one form asserts something false on the platform that hands it the other,
so this hands back the resolved form on every platform and no caller has to know which
spelling it is on.

Nothing here touches the filesystem: `os.path.realpath` resolves what exists and leaves
the rest lexical, so a socket directory that has never been created still gets the right
answer and the suite still starts no tmux server it did not mean to.
"""

from __future__ import annotations

import os

#: tmux's ``_PATH_TMP``, verbatim: a literal in its source, not ``$TMPDIR``.
DEFAULT_TMPDIR = "/tmp"

#: The socket name tmux uses when nobody passes ``-L``.
DEFAULT_SOCKET = "default"


def socket_dir(tmpdir: str | None = None) -> str:
    """``<tmpdir or $TMUX_TMPDIR or /tmp>/tmux-<uid>``, resolved.

    *tmpdir* is for the cases that state one; leaving it None asks the environment, which
    is what a tmux started right now would do.
    """
    base = tmpdir if tmpdir is not None else (os.environ.get("TMUX_TMPDIR") or "")
    return os.path.realpath(os.path.join(base or DEFAULT_TMPDIR, f"tmux-{os.getuid()}"))


def socket_path(name: str = DEFAULT_SOCKET, tmpdir: str | None = None) -> str:
    """The full path of the *name* socket in :func:`socket_dir`."""
    return os.path.join(socket_dir(tmpdir), name)


def tmux_env(socket: str | None = None, *, server_pid: int = 70029,
             session: str = "1") -> str:
    """A ``$TMUX`` value shaped exactly as a real tmux exports it into every pane.

    ``<socket path>,<server pid>,<session id>`` — measured by printing ``$TMUX`` from
    inside a tmux 3.7c pane. The pid and the session id are synthetic on purpose: they are
    charter's *input* here, not something about the machine, and a real pid would make the
    fixture no more true and considerably less stable.
    """
    return f"{socket or socket_path()},{int(server_pid)},{session}"


#: The socket a tmux started on this machine right now would listen on, computed once at
#: import. Every module that used to spell one developer's uid uses this.
OPERATOR_SOCKET = socket_path()

#: The ``$TMUX`` that goes with it.
OPERATOR_TMUX = tmux_env(OPERATOR_SOCKET)
