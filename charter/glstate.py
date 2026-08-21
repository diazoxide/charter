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
import sys
import time
import urllib.parse
from pathlib import Path

from . import config, util

DISPLAY_TTL = 7200    # show a cached value for up to 2h
REFRESH_TTL = 300     # try to refresh entries older than 5 min
SPAWN_COOLDOWN = 120  # at most one background refresh per this many seconds

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
    return config.STATE_DIR / "cache" / "glstate.refreshing"


def load() -> dict:
    try:
        return json.loads(_cache_file().read_text())
    except Exception:
        return {}


def _save(cache: dict) -> None:
    f = _cache_file()
    try:
        f.parent.mkdir(parents=True, exist_ok=True)
        f.write_text(json.dumps(cache))
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

    ``workspace`` is passed to the child explicitly (via ``--workspace``) because
    the status line's env is scrubbed — the child couldn't otherwise resolve the
    session's active workspace on its own.
    """
    now = time.time()
    lock = _lock_file()
    try:
        if lock.exists() and now - lock.stat().st_mtime < SPAWN_COOLDOWN:
            return
    except Exception:
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
    # interpreter/venv that is already running this process.
    cmd = [sys.executable, "-m", "charter", "gl-refresh"]
    if workspace:
        cmd += ["--workspace", workspace]
    try:
        lock.parent.mkdir(parents=True, exist_ok=True)
        subprocess.Popen(
            cmd, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL,
            stdin=subprocess.DEVNULL, start_new_session=True, env=os.environ.copy(),
        )
    except Exception:
        # Spawn never happened — don't start the cooldown, so a transient failure
        # (rather than a real refresh) doesn't suppress the next render's retry.
        return
    try:
        lock.touch()
    except Exception:
        pass


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
