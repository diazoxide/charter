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


def color_enabled() -> bool:
    """Whether the loggers below will colour anything — i.e. stderr is a terminal.

    Exported because a message BODY sometimes has to make the same choice the glyph
    already makes. `_c` colours the glyph and nothing else, so a caller that interpolates
    a pre-coloured fragment into `warn(...)` emits raw escape codes down a pipe while the
    `!` beside them is correctly plain — half a line honouring the terminal and half not.
    `commands_report._warn_if_stale` is the case: it renders `statusline._dev_chip()`,
    which is unconditionally ANSI because both of its other callers paint a terminal
    surface directly.

    Reads the module flag rather than calling `isatty` again, so the two halves of one
    line cannot disagree — the flag is sampled once at import, and a test that patches it
    must move the glyph and the body together.
    """
    return _USE_COLOR


def info(msg: str) -> None:
    print(_c("36", "•") + " " + msg, file=sys.stderr)


def ok(msg: str) -> None:
    print(_c("32", "✓") + " " + msg, file=sys.stderr)


def warn(msg: str) -> None:
    print(_c("33", "!") + " " + msg, file=sys.stderr)


def err(msg: str) -> None:
    print(_c("31", "✗") + " " + msg, file=sys.stderr)


def short_path(p) -> str:
    """A path an operator can tell apart from another, short enough for one row.

    Planes were identified by ``Path.name``, which collapses exactly when it matters: clone
    charter into its own plane and both directories are called ``charter``, so the alert
    read "memory and vault go to charter, not charter" and told nobody anything (#200). A
    name is not an identifier — it is a coincidence that usually holds.

    ``~`` rather than the full path because these render on one line beside other things,
    and the home prefix is the longest part carrying the least information.
    """
    p = Path(p)
    try:
        return f"~/{p.relative_to(Path.home())}"
    except (ValueError, RuntimeError, OSError):
        return str(p)


def nested_plane_note() -> str | None:
    """One sentence when the caller is standing in a plane charter did **not** act on,
    else ``None`` — the ordinary case, which says nothing.

    ``charter.toml`` is tracked, so every clone of a plane is a plane too, and `charter
    clone` puts clones exactly where the upward walk finds them first. `find_root` now
    hops outward through ``workspaces/`` so the plane holding the vault wins, which makes
    this a *notice* rather than a warning: nothing is going astray, but charter is acting
    on a plane other than the directory the operator is standing in, and a correction it
    makes silently is one it cannot be argued with (ADR 0013).

    Both planes are named by path, never by ``Path.name`` — clone charter into its own
    plane and both directories are called ``charter``, which is how the old wording came
    out as "memory and vault go to charter, not charter" (#200).

    It lives here, beside `info`/`err`, so the surfaces that say it cannot drift into two
    wordings of one fact, and it **returns** the sentence rather than printing it because
    the callers want different streams: part of the answer on stdout for `charter status`,
    diagnostic on stderr beside `util.err`.

    Imports are deferred because `util` sits below `config` and `root` in the import order
    and must stay there.
    """
    from . import config
    from . import root as _root
    origin = getattr(config, "NESTED_ORIGIN", None)
    if origin is None:
        return None
    if origin == config.ROOT:
        # The hop was overridden by $CHARTER_ROOT, so charter really is acting on the inner
        # plane and the hazard #140 described is live. A warning, not a notice.
        try:
            outer = _root.enclosing_plane(config.ROOT)
        except OSError:
            return None
        if outer is None:
            return None
        return (f"nested plane: acting on {short_path(config.ROOT)}, inside "
                f"{short_path(outer)}'s workspaces/ — memory and vaults go to the inner "
                f"one. Unset ${_root.ENV_VAR} to use {short_path(outer)}")
    return (f"you are standing in {short_path(origin)}, which is a plane too — "
            f"charter is acting on {short_path(config.ROOT)}")


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
    Without it the child gets ``DEVNULL``: charter's own stdin is never inherited, so a
    CLI that reads it gets EOF instead of blocking on a descriptor nobody will write to
    (#324). See the comment on the ``stdin`` argument below for the audit behind that.

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
        # nothing — and this function captures stdout/stderr, so that prompt is invisible
        # and the call simply waits, forever. (Stdin is `DEVNULL` now too, which stops
        # git READING an answer; this stops it ASKING, so the failure is reported as an
        # auth error rather than as whatever git makes of an empty response.)
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
            # A child charter runs never inherits charter's stdin. This function
            # redirected stdout and stderr and left stdin alone, so a CLI that decided to
            # read it blocked on whatever the parent had — forever, and invisibly, since
            # its output was captured out of sight (#324). `gh api` does exactly that for
            # a field value naming standard input (#323), which is how a status refresh
            # was observed still running after 10s.
            #
            # For EVERY caller, not just the forge path, because "who reads stdin" is not
            # a property of the caller — it is a property of a CLI's argv, which is
            # sometimes built from a forge response. The audit that makes this safe:
            # nothing passes `capture=False`, so every call already captures stdout AND
            # stderr, and a child whose output nobody sees cannot conduct a dialogue with
            # the terminal anyway. Its prompt would be buffered out of sight and the call
            # would simply wait — which is the failure, not a feature. charter's one
            # genuinely interactive path, `secret --exec`, replaces this process with
            # `os.execvpe` and never comes through here.
            #
            # This generalises a rule this function already applied to one CLI:
            # `GIT_TERMINAL_PROMPT=0` above says a prompt is never charter's path. That
            # covered git's own prompts and, as its comment notes, not "a GUI credential
            # manager, [or] an SSH signing agent, which is a separate way for a captured
            # git call to hang". Closing the descriptor closes those too.
            #
            # Conditional because `subprocess.run` opens the pipe itself when `input` is
            # given and rejects being handed both — and that is the correct semantics as
            # well as the required one: `input=` is how a secret reaches a CLI without
            # passing through argv, and it must still arrive.
            stdin=subprocess.DEVNULL if input is None else None,
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


def self_relaunch_argv(*args: str) -> list[str]:
    """The argv that re-launches charter as *this interpreter's own install* — never
    whatever ``charter/`` package happens to sit under the child's cwd (#390).

    ``python -m charter`` prepends the CURRENT WORKING DIRECTORY to ``sys.path`` before
    it even looks for the module to run — ``-m``'s own documented behaviour, not a bug in
    it. Every charter self-relaunch site sets its child's cwd to something outside
    charter's own control (a project directory, a workspace root, wherever an operator's
    pane happened to start), and when THAT directory contains its own ``charter/``
    package — a charter checkout dogfooding itself, the common case for anyone
    developing charter — the child imports that tree instead of the installed one. On a
    tree that predates a command the installed one has, the failure lands as an
    argparse ``invalid choice`` and exit 2, which is how this shipped: ``charter claude``
    came up with both panels dead, and the harness pane survived only because ``claude``
    is a real binary rather than another self-relaunch.

    ``-P`` (3.11+; ``pyproject.toml`` already requires it) is ``-m``'s own switch for
    "don't do that". One helper, not a `[sys.executable, "-m", "charter", ...]` hand-built
    at every call site — the same shape as the frame's own "never join argv" rule: correct
    by construction rather than remembered at five (now seven) separate places. A shell
    TEMPLATE that embeds ``"$CHARTER_PY" -m charter`` (the tmux hotkey bind — see
    ``commands_frame.py``'s own module docstring) cannot use this helper
    directly; those carry ``PYTHONSAFEPATH=1`` instead, the environment-variable form of
    the same switch, alongside ``$CHARTER_PY`` itself.
    """
    return [sys.executable, "-P", "-m", "charter", *args]


def child_env() -> dict:
    """This process's environment plus **the plane this process actually resolved**.

    The environment for a charter that charter spawns. Every self-relaunch site used to
    hand the child a bare ``os.environ.copy()`` and let it work its own plane out by
    walking up from its own cwd — the same defect `glstate.maybe_spawn` already argues
    against for the *workspace*: "the status line resolves the workspace for the SESSION …
    while the child would resolve it for ITSELF, from its own environment and its own
    directory". The plane is that argument one level up, and it is the bigger half: the
    workspace decides which rows get refreshed, the plane decides whose ``.charter/`` gets
    written.

    Measured, from a linked worktree of a charter checkout: a ``gl-refresh`` spawned off
    the status line landed on the MAIN tree's plane, because `root._plane_of` redirects a
    worktree to the tree it was cut from and the child had nothing else to go on. A render
    for one plane refreshed a different one, silently, on every stale render (#527).

    **Only when this process has a plane.** ``$CHARTER_ROOT`` wins outright in
    `root.find_root`, and a value with no ``charter.toml`` at it RAISES rather than falling
    back to a walk — so handing a planeless child an empty-handed pointer would be worse
    than handing it nothing, which is what it gets. Both `maybe_spawn` sites decline to
    fork at all in that state.

    Overwrites rather than deferring to an inherited ``$CHARTER_ROOT``. When this process
    was itself resolved from that variable the two agree; when they disagree, `config.ROOT`
    is the plane whose state this process is actually reading and writing, and the child's
    job is to agree with its parent rather than with the shell.

    Imports are deferred because `util` sits below `config` and `root` in the import order
    and must stay there.
    """
    from . import config
    from . import root as _root

    env = os.environ.copy()
    if config.HAS_CONTROL_PLANE:
        env[_root.ENV_VAR] = str(config.ROOT)
    return env


def detach_self(args: list[str]) -> bool:
    """Re-run ``charter <args>`` in a process that outlives this one. True if it started.

    What a hook's `"async": true` bought, done by charter instead of asked of the host —
    Codex supports no such flag and skips the entry outright, so a manifest that needs one
    silently loses whatever it declared. `start_new_session` is the load-bearing part: a
    hook's process group is torn down when the turn ends, and a refresh killed halfway is
    worse than one that never started.

    Never raises. The caller is a session-start hook, and a plane that cannot spawn a
    background refresh must still open a session.

    The child is handed :func:`child_env`, so ``persona _gc`` collects the plane the hook
    fired for rather than whichever one its own cwd happens to sit under.
    """
    try:
        subprocess.Popen(
            self_relaunch_argv(*args),
            stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL,
            stdin=subprocess.DEVNULL, start_new_session=True, env=child_env(),
        )
    except Exception:
        return False
    return True


def append_gitignore(root, lines, header: str) -> list[str]:
    """Append the ignore *lines* a plane's ``.gitignore`` is missing. Returns the ones
    actually written, so a caller can report what it changed rather than what it asked for
    (ADR 0013).

    **Append-only, and that is a hard requirement rather than politeness.** The file is not
    free-form: ``workspace.set_live()`` splices its managed block at the literal anchor line
    ``!/workspaces/.gitkeep``, so a writer that rewrote or reordered would break live
    workspaces from across the codebase, with a symptom pointing nowhere near here.

    Whole-line matching, never a substring. ``_ensure_gitignore``'s docstring records what
    the substring version cost: ``"workspaces/" in body`` matched a pre-existing
    ``build/workspaces/output/`` and skipped writing the anchor the splice depends on.

    One writer, deliberately. ``charter init`` had the only implementation, inlined, and the
    second command needing to ignore a path it creates would otherwise have grown a rival —
    which is how a plane ends up with one rule twice.
    """
    from pathlib import Path

    p = Path(root) / ".gitignore"
    body = p.read_text() if p.exists() else ""
    present = {ln.strip() for ln in body.splitlines()}
    missing = [ln for ln in lines if ln not in present]
    if not missing:
        return []
    prefix = (body.rstrip("\n") + "\n\n") if body.strip() else ""
    p.write_text(prefix + f"# {header}\n" + "".join(f"{ln}\n" for ln in missing))
    return missing


def git_ignores(root, path) -> bool | None:
    """Whether git would ignore *path* inside *root*. ``None`` when the question does not
    apply — *root* is not a repository, so there is nothing for a credential to be committed
    to.

    Asked of ``git check-ignore`` rather than read out of ``.gitignore``: nested ignore
    files, negations and global excludes all count, and the question is only ever "would git
    take this file", where git is the authority and a hand-rolled parser is a second opinion
    that will eventually differ.
    """
    if run(["git", "-C", str(root), "rev-parse", "--git-dir"], check=False).returncode != 0:
        return None
    return run(["git", "-C", str(root), "check-ignore", "-q", str(path)],
               check=False).returncode == 0
