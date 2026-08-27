"""Per-repo forge state (open MR/PR + last CI status) for the status line,
cached + refreshed in the background so a render never blocks on the network.

The status line reads the cache (``read_for``) and, if it's stale, kicks off a
**detached** ``charter gl-refresh`` (``maybe_spawn``) that resolves each clone's
own forge (``state_for_repo``, via ``charter.forge.registry``) and rewrites the
cache — a GitLab clone is queried over ``glab``, a GitHub clone over ``gh``, so a
mixed-forge workspace renders each repo's own notation.
"""

from __future__ import annotations

import json
import os
import subprocess
import time
import urllib.parse
from pathlib import Path

from . import config, util

DISPLAY_TTL = 7200    # show a cached value for up to 2h
REFRESH_TTL = 300     # try to refresh entries older than 5 min
SPAWN_COOLDOWN = 120  # at most one background refresh per this many seconds
#: How long a refresh may claim to be in flight before it is presumed wedged and
#: replaced. Three times `REFRESH_TTL`: a refresh still running after this would land
#: data that was already stale several times over, so waiting on it is worse than
#: starting a fresh one even if it IS alive. This is also what stops a recycled pid from
#: suppressing refreshes indefinitely — the failure it bounds is a slightly stale CI
#: column, never a stuck status line, since `read_for` keeps serving up to `DISPLAY_TTL`.
STUCK_AFTER = 900

_EMPTY = {"change": None, "ci": None, "sigil": ""}


def _change_or_none(v):
    """A change identifier as :meth:`base.Forge.open_change` promises it — ``int`` or
    ``None`` — whatever the forge actually returned.

    ``change`` was the one forge field that reached the rendered status line unchecked
    (#326). ``ci`` is pinned to seven literals by each backend's ``_CI_MAP`` and
    ``sigil`` is a class constant, so both are safe by construction; this was whatever
    the JSON ``number`` (GitHub) or ``iid`` (GitLab) field happened to hold, stored
    verbatim, cached verbatim, and interpolated into a line a terminal interprets. A
    self-hosted or compromised instance, an altered response, or an edit to the cache
    file — which nothing signs — could put an escape sequence there.

    Coerced rather than type-checked, because a forge that serialises its id as ``"42"``
    is answering the question and only its JSON type is off; dropping that would blank a
    real change number over a detail. Anything `int()` refuses, and anything that is not
    a positive identifier on any forge charter speaks to, becomes ``None`` — the same
    thing "no open change" already looks like, which the render path draws as an empty
    cell.

    Never raises: this runs under `state_for_repo`, whose contract is that a status line
    rendering every turn cannot be given a way to fail.
    """
    if v is None or isinstance(v, bool):
        return None
    try:
        n = int(v)
    except (TypeError, ValueError):
        return None
    return n if n > 0 else None


def _cache_file() -> Path:
    return config.STATE_DIR / "cache" / "glstate.json"


def _lock_file() -> Path:
    """The spawn lock: its CONTENT is the pid of an in-flight refresh (empty when none),
    and its MTIME is when that last changed — spawned, or finished.

    Two facts rather than one because #324 needed both and the file carried neither. The
    mtime alone said "a refresh was STARTED 120s ago", which is not a reason to start
    another; what suppresses a second refresh is that the first is still running, and
    what starts the cooldown is that it stopped.
    """
    return config.STATE_DIR / "cache" / "glstate.refreshing"


def _write_lock(pid: int | None) -> None:
    """Record an in-flight refresh (*pid*), or that none is (``None``). Bumps the mtime
    either way. Never raises — every caller is on a path that must not fail."""
    try:
        lock = _lock_file()
        config.private_mkdir(lock.parent)
        config.write_for(lock, str(pid) if pid else "")
    except Exception:
        pass


def _read_lock():
    """``(pid_or_None, age_in_seconds)``, or ``None`` when there is no lock at all.

    Deliberately tolerant of the content: a truncated write, a hand edit or a pid from
    another machine all read as "no pid", which degrades to the plain cooldown rather
    than to an exception on the render path.
    """
    lock = _lock_file()
    try:
        age = max(0.0, time.time() - lock.stat().st_mtime)
    except OSError:
        return None
    try:
        txt = lock.read_text().strip()
    except OSError:
        txt = ""
    return (int(txt) if txt.isdigit() else None), age


def _alive(pid: int | None) -> bool:
    """Whether *pid* is a live process. Signal 0 checks for existence without delivering
    anything; ``PermissionError`` means it exists and belongs to somebody else, which is
    still "in flight" for our purposes."""
    if not pid or pid <= 0:
        return False
    try:
        os.kill(pid, 0)
    except ProcessLookupError:
        return False
    except PermissionError:
        return True
    except Exception:
        return False
    return True


def _mark_done() -> None:
    """Called by the refresh itself when the work is OVER.

    This is what moves the cooldown from the spawn to the completion. `maybe_spawn`
    cannot do it — it returns the instant `Popen` forks, which is precisely the mistake
    #324 records — so the process that knows the work finished is the one that says so.
    """
    _write_lock(None)


def load() -> dict:
    try:
        return json.loads(_cache_file().read_text())
    except Exception:
        return {}


def _save(cache: dict) -> None:
    f = _cache_file()
    try:
        config.private_mkdir(f.parent)
        config.write_for(f, json.dumps(cache))
    except Exception:
        pass


def read_for(dirs, branches: dict) -> dict:
    """{dir: {"change": int|None, "ci": status|None, "sigil": str}} for fresh,
    branch-matching entries.

    Reads defensively: a cache entry written by an OLDER charter (before this forge-
    protocol upgrade) has ``mr`` instead of ``change``, and no ``sigil`` at all — both
    are read with a fallback so a stale on-disk cache still renders after an upgrade
    rather than raising a ``KeyError``. ``sigil`` falls back to ``""`` here; the render
    path is what turns an absent sigil into the display default (``!``).

    ``change`` is re-validated here and not only where it was written: an entry written
    by the charter that had #326 renders for up to ``DISPLAY_TTL`` (two hours) after the
    upgrade that fixed it, and the cache is an ordinary writable JSON file that nothing
    signs. Same reason `charter.contain` asserts at every join rather than trusting the
    identity layer — a check the value can be written past is not a check.
    """
    cache = load()
    now = time.time()
    out = {}
    for d in dirs:
        ent = cache.get(str(d))
        if not ent or ent.get("branch") != branches.get(d):
            continue
        if now - ent.get("ts", 0) > DISPLAY_TTL:
            continue
        out[d] = {"change": _change_or_none(ent.get("change", ent.get("mr"))),
                  "ci": ent.get("ci"),
                  "sigil": ent.get("sigil") or ""}
    return out


def maybe_spawn(dirs, workspace: str | None = None) -> None:
    """Kick off a detached background refresh if the cache is stale. Non-blocking.

    ``workspace`` is passed to the child explicitly (via ``--workspace``), and the reason
    written here for years — "the status line's env is scrubbed" — is **false**. Measured
    2026-08-24 against Claude Code 2.1.241 with a real `statusLine` command: the
    environment arrives intact, `$CHARTER_WORKSPACE` included. (Same false belief appeared
    in :mod:`charter.statusline` and :mod:`charter.session`; all three are corrected
    together rather than one at a time, because a claim in three places is a claim
    somebody will re-derive from whichever copy survives.)

    Passing it explicitly is still right, for a better reason: the status line resolves
    the workspace for the SESSION — from the payload's `session_id` and `cwd`, which are
    not this process's — while the child would resolve it for ITSELF, from its own
    environment and its own directory. A refresh keyed to a different workspace than the
    row it is refreshing is the defect; an environment variable was never what stood
    between them.
    """
    if not config.HAS_CONTROL_PLANE:
        # A third brake, and the one that is about WHERE rather than how often. Outside a
        # plane `config.STATE_DIR` is `<cwd>/.charter`, so a spawn here would scatter
        # charter's caches into whatever directory the render happened to run in — and the
        # child, having no plane handed to it (`util.child_env` has none to hand), would go
        # looking for one of its own. That is how a status line rendered for a throwaway
        # root came to refresh the operator's live plane (#527): from a linked worktree the
        # child's walk redirects to the main tree. No plane, no background refresh.
        return
    now = time.time()
    try:
        state = _read_lock()
        if state is not None:
            pid, age = state
            # Cooled down since the last spawn OR the last completion, whichever the
            # mtime records.
            if age < SPAWN_COOLDOWN:
                return
            # Past the cooldown, but the previous refresh is STILL RUNNING. This is the
            # case #324 is about: the old code had already forgotten the child existed,
            # so a wedged refresh invited a replacement every 120s for as long as the
            # session stayed open, each one holding the forge credential.
            if _alive(pid) and age < STUCK_AFTER:
                return
    except Exception:
        # Suppress rather than spawn on an unreadable lock: the cost of skipping a
        # refresh is a stale column, and the cost of getting this wrong is the pile-up
        # this function exists to prevent.
        return
    cache = load()
    stale = any(
        str(d) not in cache or now - cache[str(d)].get("ts", 0) > REFRESH_TTL
        for d in dirs
    )
    if not stale:
        return
    # Target the installed package via `-m`, not a path inside the control plane —
    # a control plane has no `bin/edm` (that script lived in the old monorepo the
    # engine was extracted from); `-m charter` resolves through the same
    # interpreter/venv that is already running this process. `-P` (util.self_relaunch_argv,
    # #390) is what keeps that resolution honest: this runs on the status line's own
    # render path, so a project directory that happens to have its own `charter/` package
    # (any charter checkout) would otherwise get shadowed on every single render.
    cmd = util.self_relaunch_argv("gl-refresh")
    if workspace:
        cmd += ["--workspace", workspace]
    try:
        proc = subprocess.Popen(
            cmd, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL,
            stdin=subprocess.DEVNULL, start_new_session=True, env=util.child_env(),
        )
    except Exception:
        # Spawn never happened — don't start the cooldown, so a transient failure
        # (rather than a real refresh) doesn't suppress the next render's retry.
        return
    # Record WHICH process is refreshing, not merely that one was started. The child
    # rewrites this to empty when it finishes (`_mark_done`), which is what makes the
    # cooldown run from completion. A `Popen` stand-in that reports no usable pid still
    # arms the cooldown — a spawn that happened must never invite another in 120s.
    try:
        pid = int(proc.pid)
    except Exception:
        pid = None
    _write_lock(pid)


# --------------------------------------------------------------------------- #
# refresh (runs in the background / on demand — network allowed here)          #
# --------------------------------------------------------------------------- #
def refresh(dirs) -> dict:
    """For each clone's current branch, fetch its open change + last CI status,
    via that clone's own forge (:func:`state_for_repo`)."""
    cache = load()
    now = time.time()
    for d in dirs:
        branch = _branch(d)
        state = state_for_repo(d, branch) if branch and branch != "?" else dict(_EMPTY)
        cache[str(d)] = {"branch": branch, "ts": now, **state}
    _save(cache)
    # The work is over — start the cooldown from HERE, and stop claiming to be in
    # flight (#324). Last, so a refresh that dies partway leaves its pid behind and is
    # recognised as crashed rather than as finished.
    _mark_done()
    return cache


def state_for_repo(d: Path, branch: str) -> dict:
    """``{"change": int|None, "ci": str|None, "sigil": str}`` for a clone's branch.

    Takes the clone DIRECTORY, not a namespace string: the forge is inferred from that
    clone's own ``origin``, which is what makes a mixed-forge workspace work — two
    clones side by side can be hosted on different forges.

    Resolves via ``registry.resolve_host``, which also recognises a self-hosted forge
    this control plane's own ``charter.toml`` declares, not just each kind's default
    host.

    Best-effort by contract: this feeds the status line, which renders on every turn and
    must never raise. No remote, an unknown host, a missing CLI, or an API failure all
    degrade to empty state.
    """
    from .forge import registry
    try:
        url = _remote_url(d)
        path = _remote_path(d)
        if not url or not path:
            return dict(_EMPTY)
        forge = registry.resolve_host(url, config.ROOT)
        if forge is None:
            return dict(_EMPTY)
        return {"change": _change_or_none(forge.open_change(path, branch)),
                "ci": forge.ci_status(path, branch),
                "sigil": forge.change_sigil}
    except Exception:
        return dict(_EMPTY)


def _remote_url(d: Path) -> str | None:
    """The clone's raw `origin` URL, for inferring which forge hosts it."""
    try:
        p = subprocess.run(["git", "-C", str(d), "remote", "get-url", "origin"],
                           stdout=subprocess.PIPE, stderr=subprocess.DEVNULL,
                           text=True, timeout=10)
    except Exception:
        return None
    url = (p.stdout or "").strip()
    return url or None


def _branch(d: Path) -> str:
    """The tree's branch, clone or linked worktree alike.

    Shares :func:`charter.util.branch_of` with the status line deliberately: this value
    is written into the cache entry and the status line compares its own reading against
    it (:func:`read_for` drops any entry whose ``branch`` differs). Two implementations
    of "what branch is this" is exactly the kind of drift that shows up as a CI column
    that silently never renders — as it did for worktrees, which only this side could
    read.
    """
    try:
        return util.branch_of(d)
    except Exception:
        return "?"


def _remote_path(d: Path):
    """path_with_namespace from the clone's origin remote (ssh or https)."""
    try:
        url = subprocess.run(
            ["git", "-C", str(d), "remote", "get-url", "origin"],
            stdout=subprocess.PIPE, stderr=subprocess.DEVNULL, text=True, timeout=3,
        ).stdout.strip()
    except Exception:
        return None
    if not url:
        return None
    if url.endswith(".git"):
        url = url[:-4]
    if "://" in url:
        return urllib.parse.urlparse(url).path.lstrip("/") or None
    if ":" in url:  # scp-like: git@host:group/sub/repo
        return url.split(":", 1)[1] or None
    return None
