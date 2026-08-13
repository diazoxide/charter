"""Process execution, coloured logging, and small helpers.

Everything here is stdlib-only so the CLI runs with a bare ``python3``.
"""

from __future__ import annotations

import os
import subprocess
import sys
import urllib.parse
from pathlib import Path
from typing import Sequence

_USE_COLOR = sys.stderr.isatty()


def _c(code: str, text: str) -> str:
    return f"\033[{code}m{text}\033[0m" if _USE_COLOR else text


def info(msg: str) -> None:
    print(_c("36", "•") + " " + msg, file=sys.stderr)


def ok(msg: str) -> None:
    print(_c("32", "✓") + " " + msg, file=sys.stderr)


def warn(msg: str) -> None:
    print(_c("33", "!") + " " + msg, file=sys.stderr)


def err(msg: str) -> None:
    print(_c("31", "✗") + " " + msg, file=sys.stderr)


class ProcTimeout(RuntimeError):
    """A subprocess outlived its ``timeout``.

    Its own class rather than letting `subprocess.TimeoutExpired` escape, because the
    thing a caller wants to say is "this check timed out" — `doctor` renders it as a WARN
    naming the seconds, instead of the traceback `cli.main` would otherwise print (it
    catches only `KeyboardInterrupt`).
    """

    def __init__(self, cmd, seconds: float) -> None:
        self.cmd, self.seconds = list(cmd), seconds
        super().__init__(f"timed out after {seconds:g}s: {' '.join(self.cmd)}")


class ProcError(RuntimeError):
    """A subprocess exited non-zero while ``check=True``."""

    def __init__(self, cmd: Sequence[str], returncode: int, stderr: str) -> None:
        self.cmd = list(cmd)
        self.returncode = returncode
        self.stderr = (stderr or "").strip()
        super().__init__(
            f"command failed ({returncode}): {' '.join(self.cmd)}\n{self.stderr}"
        )


def run(
    cmd: Sequence[str], cwd=None, check: bool = True, capture: bool = True,
    input: str | None = None, env: dict | None = None, timeout: float | None = None,
) -> subprocess.CompletedProcess:
    """Run ``cmd``. Raises :class:`ProcError` on failure when ``check``.

    ``input`` is written to the child's stdin. That is how a secret reaches a CLI
    without ever appearing in argv — where `ps` and the shell's history can see it.

    ``timeout`` bounds the child in seconds, raising :class:`ProcTimeout` — a *charter*
    error, so a caller can render it rather than let `subprocess.TimeoutExpired` reach the
    user as a traceback. Without it, six call sites bypassed this function entirely just
    to pass their own literal, and every un-timeouted path (`gh api`, `glab api`, every
    doctor check) could hang indefinitely: a 1Password session needing re-auth stalled the
    SessionStart preflight for its whole 20s budget.

    ``env`` is an OVERLAY on this process's environment, not a replacement: the child
    still needs PATH, HOME and the rest to find and run the CLI at all. It exists so a
    vault can carry the credential a CLI reads (``OP_SERVICE_ACCOUNT_TOKEN``,
    ``VAULT_TOKEN``) for the duration of one call, without charter ever setting it on
    itself — a mutated `os.environ` would outlive the call and silently apply to the next
    vault, which is the identity confusion the whole feature exists to prevent.
    """
    overlay = dict(env or {})
    if cmd and cmd[0] == "git":
        # git falls back to prompting on the TERMINAL when a credential helper produces
        # nothing — and this function captures stdout/stderr while leaving stdin
        # INHERITED, so that prompt is invisible and the call simply waits, forever.
        #
        # charter's auth design (see `planegit`) says a prompt is never the path: every
        # git operation authenticates with its forge CLI's token over HTTPS. So this
        # restricts nothing charter supports — it makes that intent enforceable, and turns
        # an invisible hang into the "isn't authed (`gh auth status`)" error that already
        # exists. It matters more now that clones run concurrently, where a stuck child
        # is one of eight and its prompt is buffered out of sight.
        #
        # Covers git's own prompts only — not a GUI credential manager, and not an SSH
        # signing agent, which is a separate way for a captured git call to hang.
        overlay.setdefault("GIT_TERMINAL_PROMPT", "0")
    child_env = None
    if overlay:
        child_env = {**os.environ, **{k: v for k, v in overlay.items() if v is not None}}
    try:
        proc = subprocess.run(
            list(cmd),
            cwd=cwd,
            text=True,
            input=input,
            env=child_env,
            timeout=timeout,
            stdout=subprocess.PIPE if capture else None,
            stderr=subprocess.PIPE if capture else None,
        )
    except subprocess.TimeoutExpired:
        raise ProcTimeout(cmd, timeout) from None
    if check and proc.returncode != 0:
        raise ProcError(cmd, proc.returncode, proc.stderr if capture else "")
    return proc


def urlenc(s: str) -> str:
    """URL-encode a path segment (e.g. a group path with slashes)."""
    return urllib.parse.quote(s, safe="")


def git_dir(tree: Path) -> Path | None:
    """The git directory backing *tree*, or ``None`` when *tree* is not a working tree.

    Git stores this two different ways and both are normal. A **clone**'s ``.git`` is a
    directory holding HEAD. A linked **worktree**'s ``.git`` is a FILE containing
    ``gitdir: <path>``, and its HEAD — along with everything else that is per-worktree —
    lives at that path instead. ``workspace.is_clone`` relies on exactly this difference
    to tell the two apart without bookkeeping.

    Readers that handled only the directory form reported the worktree's branch as ``?``,
    which is the entire branch column of a monorepo control plane, where every tree below
    the root is a worktree.
    """
    g = Path(tree) / ".git"
    try:
        if g.is_dir():
            return g
        txt = g.read_text().strip()
    except OSError:
        return None
    if not txt.startswith("gitdir:"):
        return None
    p = Path(txt[len("gitdir:"):].strip())
    # `git worktree add` writes an absolute path, but the file format permits a relative
    # one (and `git worktree repair` can produce it) — resolve it against the tree.
    return p if p.is_absolute() else (Path(tree) / p)


def branch_of(tree: Path) -> str:
    """Current branch of a working tree, read straight from HEAD — **no subprocess**.

    ``?`` when unreadable, a short sha when detached, and the full ref name otherwise
    (branch names legitimately contain slashes, so only ``refs/heads/`` is stripped).

    Filesystem-only on purpose: the status line renders on every turn and calls this once
    per tree, so a `git` fork here would be paid over and over for something two `read`s
    answer exactly.
    """
    gd = git_dir(tree)
    if gd is None:
        return "?"
    try:
        txt = (gd / "HEAD").read_text().strip()
    except OSError:
        return "?"
    if txt.startswith("ref:"):
        return txt.split("/", 2)[-1] or "?"   # refs/heads/<branch> — keeps slashes
    return txt[:7] if txt else "?"            # detached HEAD → short sha
