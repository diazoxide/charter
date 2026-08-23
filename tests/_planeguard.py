"""Suite-wide tripwire: no test may WRITE into the developer's real ``.charter/``.

`PersonaIso` isolates a test that remembers to derive from it. Nothing made the
*forgetting* visible, so the same defect kept arriving in a new file: the suite wrote
fixture data into a contributor's real ``.charter/vaults.json`` and orphaned every vault
on that machine (#recorded in `_isolation.py`), and later three plain `unittest.TestCase`
classes ran the real `cmd_launch` — `state.reap`'s ``rmtree`` of every frame directory the
tmux server does not report live — against whatever frames the developer had open (#402).
Both were found by hand, months apart, by someone who happened to look.

This makes the class of mistake self-reporting instead. Every call that could CREATE,
DELETE or REPLACE something under the real state directory is wrapped once, at import of
the `tests` package — before any test module is collected, so no test can opt out by
forgetting a base class. A test that reaches the real plane now fails, by name, on the
line that reached it, having changed nothing.

**Only writes.** Reads are untouched: a test may legitimately look at the real plane (that
`render()` survives a real environment, say), and `_isolation.isolate_state_dir` exists for
exactly that shape. Writing is the part no test in this suite has any business doing.

**Only the state directory.** ``.charter/`` is per-developer, gitignored, and holds live
session state — frame directories, the vault registry, session pointers. The rest of the
plane (``personas/``, ``docs/``, ``workspaces/``) is committed content some tests do
legitimately generate into a tmp root, and guarding the real copies of those would trade
this defect for a stream of false alarms.

**Raised as a `BaseException`, deliberately.** charter is full of `except Exception`
fallbacks that turn an unreachable path into a degraded one — `config.derive` catches
everything so a malformed `charter.toml` cannot break import, `root.find_root` swallows
`OSError`. A tripwire those can catch is a tripwire that reports "no vault configured"
instead of failing. `unittest` still records a `BaseException` against the test that raised
it, so the failure keeps its name.

**What this cannot see: a subprocess.** A `tmux run-shell` hook, or any spawned `charter`,
resolves its own plane from its own environment, so isolating this process does nothing for
it — the parent must hand the throwaway plane across the boundary with ``$CHARTER_ROOT`` on
the child's (or the tmux server's) environment, the way `PanelIntegration` and
`MenuIntegration` in `test_frame_tmux_integration.py` do. This guard is silent about that
half by construction, and saying so here is the point: it is not a licence to skip the env.
"""

from __future__ import annotations

import builtins
import io
import os
import shutil

#: The state directory of the plane this test PROCESS resolved at import — before any
#: `setUp` could repoint `charter.config`, so unavoidably the developer's real one.
#:
#: A TUPLE, because one directory has two spellings a caller may use: the path as written
#: and the path with every symlink resolved. A checkout under a symlinked home (or under
#: macOS's ``/tmp`` → ``/private/tmp``) makes those differ, and a guard that knew only the
#: resolved one would wave through every write spelled the other way — silence that reads
#: exactly like safety. Computed once, at install; the per-call check is a string compare.
_REAL: tuple[str, ...] = ()


class RealPlaneWrite(BaseException):
    """A test tried to write into the developer's own control-plane state directory."""


def _explain(op: str, path) -> str:
    return (
        f"REFUSED: {op} {path}\n"
        f"This test is writing into the developer's REAL control-plane state directory "
        f"({_REAL[0]}) — live vaults, session pointers, and the frame state of running "
        f"sessions. Derive the test case from `tests._isolation.PersonaIso`, or, if it "
        f"deliberately exercises the real plane, call "
        f"`tests._isolation.isolate_state_dir(self)` to redirect just this directory. "
        f"A subprocess needs the throwaway plane passed to it as $CHARTER_ROOT.")


def _hits(path, dir_fd=None) -> bool:
    """Does *path* land inside the real state directory?

    ``dir_fd`` is answered ``False``: the path is then relative to an open directory this
    cannot resolve, and guessing against the cwd would refuse the wrong calls. The one
    caller that uses it is `shutil.rmtree`'s fd-based recursion, which is refused whole at
    its own front door below — so nothing is lost by declining to guess here.

    On the hot path: this runs on every `open` in the suite, so an already-absolute,
    already-normal path is answered by string compares alone. `os.path.abspath` — a
    `getcwd` syscall per call — is reached only by a relative path or one containing
    ``..``, which is where it is actually needed.
    """
    if not _REAL or dir_fd is not None:
        return False
    try:
        p = os.fspath(path)
    except TypeError:
        return False                      # an int fd, or something not a path at all
    if isinstance(p, bytes):
        p = os.fsdecode(p)
    for real in _REAL:
        if p == real or p.startswith(real + os.sep):
            return True
    if p.startswith(os.sep) and os.pardir not in p:
        return False
    p = os.path.abspath(p)
    return any(p == real or p.startswith(real + os.sep) for real in _REAL)


def _guard_os(name: str, *, arg: int = 0, both: bool = False) -> None:
    """Wrap ``os.<name>``, watching the argument(s) naming the path it would change.

    *arg* because the created path is not always the first one: ``os.symlink(target,
    link)`` and ``os.link(src, dst)`` both CREATE their SECOND argument and only read
    their first. *both* because ``os.rename``/``os.replace`` change two paths at once —
    the source vanishes and the destination is overwritten — so watching either alone
    leaves a way in.
    """
    original = getattr(os, name)

    def guarded(*args, **kw):
        watched = (0, 1) if both else (arg,)
        for i in watched:
            if len(args) > i and _hits(args[i], kw.get("dir_fd")):
                raise RealPlaneWrite(_explain(f"os.{name}", args[i]))
        return original(*args, **kw)

    guarded.__name__ = name
    setattr(os, name, guarded)


def install() -> None:
    """Wrap every write primitive. Idempotent; called once at `tests` package import."""
    global _REAL
    if _REAL:
        return
    from charter import config
    try:
        written = os.path.abspath(str(config.STATE_DIR))
        resolved = os.path.realpath(written)
    except (OSError, ValueError):         # no resolvable plane: nothing to protect
        return
    _REAL = (written,) if written == resolved else (written, resolved)

    # `os.mkdir` covers `Path.mkdir` AND `os.makedirs` (which calls the module global by
    # name, so it goes through this wrapper too) — one wrapper, both spellings.
    # `os.remove`/`os.unlink` are distinct objects, so both are wrapped.
    for name in ("mkdir", "rmdir", "remove", "unlink", "truncate", "chmod"):
        _guard_os(name)
    for name in ("rename", "replace"):
        _guard_os(name, both=True)
    for name in ("symlink", "link"):
        _guard_os(name, arg=1)

    # `shutil.rmtree` is wrapped at its FRONT DOOR rather than left to the primitives
    # above. On every platform that avoids symlink attacks it recurses with
    # `os.unlink(entry.name, dir_fd=topfd)` — a bare NAME plus a directory fd, which no
    # path-based check can resolve. `state.reap` deletes exactly this way, so a guard that
    # only watched the primitives would have watched the one call it exists to catch go
    # past. The path handed to `rmtree` itself is a real path, and refusing there is
    # earlier anyway: nothing is scanned, let alone removed.
    original_rmtree = shutil.rmtree

    def rmtree(path, *args, **kw):
        if _hits(path):
            raise RealPlaneWrite(_explain("shutil.rmtree", path))
        return original_rmtree(path, *args, **kw)

    shutil.rmtree = rmtree

    # `builtins.open` and `io.open` are the same function object but two separate module
    # attributes, looked up independently at call time: `Path.open` (and so `write_text`,
    # `write_bytes`) calls `io.open`, while charter's own code calls the builtin. Missing
    # either one leaves half the writes unwatched.
    original_open = builtins.open

    def open_(file, mode="r", *args, **kw):
        if any(c in mode for c in "wxa+") and _hits(file):
            raise RealPlaneWrite(_explain(f"open(..., {mode!r})", file))
        return original_open(file, mode, *args, **kw)

    builtins.open = open_
    io.open = open_

    # The low-level `os.open`, used by `tempfile` and by anything wanting O_EXCL. Only the
    # flags that can create or shorten a file are refused; O_RDONLY passes.
    original_os_open = os.open
    writing = os.O_WRONLY | os.O_RDWR | os.O_CREAT | os.O_APPEND | os.O_TRUNC

    def os_open(path, flags, *args, **kw):
        if flags & writing and _hits(path, kw.get("dir_fd")):
            raise RealPlaneWrite(_explain("os.open", path))
        return original_os_open(path, flags, *args, **kw)

    os.open = os_open
