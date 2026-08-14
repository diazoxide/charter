"""Is a newer charter published? — cached, and never on the status line's clock.

The status line renders on every turn, so it must never make a network call. This
mirrors :mod:`charter.glstate`: the renderer only ever *reads* a cache, and kicks
off a detached background refresh when that cache goes stale. A slow or offline
PyPI therefore costs a stale indicator, never a delayed prompt.

The check is deliberately unauthenticated and read-only — one GET of the JSON
metadata endpoint. Nothing is downloaded, installed or executed.
"""

from __future__ import annotations

import json
import os
import subprocess
import sys
import time
from pathlib import Path

from . import config

#: PyPI distribution name. The *command* is `charter`; the *package* is `charter-cp`
#: (PyPI would not allow `charter`), so the metadata lives under the latter.
DIST = "charter-cp"
_URL = f"https://pypi.org/pypi/{DIST}/json"

REFRESH_TTL = 24 * 3600   # re-check at most once a day — releases are not that frequent
SPAWN_COOLDOWN = 3600     # and at most one background attempt per hour, success or not
NET_TIMEOUT = 5           # a detached child, but still: never hang around


#: Said wherever a pin and an install disagree, and nowhere else.
#:
#: The pin is per control plane; the binary is one machine-global install. Two planes
#: pinning different versions cannot both be satisfied, and `version sync` does not fix
#: that — it picks a winner and puts the other plane into drift, which is how a plane that
#: nobody touched went from "in sync" to "drift" because of work done somewhere else.
#: charter is a control plane, not a version manager, so it says this rather than growing a
#: shim to resolve the pinned version per plane.
SHARED_INSTALL_NOTE = (
    "the `charter` binary is ONE machine-global install shared by every control plane on "
    "this machine, so a pin is advisory — syncing here can put another plane into drift "
    "(see: uv tool list)"
)


def _cache_file() -> Path:
    return config.STATE_DIR / "cache" / "update.json"


def _lock_file() -> Path:
    return config.STATE_DIR / "cache" / "update.checking"


def load() -> dict:
    try:
        return json.loads(_cache_file().read_text())
    except (OSError, ValueError):
        return {}


def _parse(v: str) -> tuple:
    """Compare versions numerically per component: 0.10.0 is newer than 0.2.0.

    Anything non-numeric (a dev/rc suffix) sorts low rather than raising — an
    unparseable version must never make the indicator claim an update.
    """
    out = []
    for part in (v or "").split("."):
        digits = "".join(c for c in part if c.isdigit())
        out.append(int(digits) if digits else -1)
    return tuple(out)


def newer_than(current: str) -> str | None:
    """The cached latest version if it is strictly newer than *current*, else None."""
    latest = (load().get("latest") or "").strip()
    if not latest or not current:
        return None
    try:
        return latest if _parse(latest) > _parse(current) else None
    except Exception:
        return None


def latest_display(installed: str) -> str:
    """The `latest` line, honest about what it is.

    `latest` is a reading of PyPI cached for up to :data:`REFRESH_TTL`, not a live answer.
    Usually the distinction does not matter. It matters completely in one case: when the
    INSTALLED version is newer than the cached one, the cache is *provably* out of date —
    you cannot be running something PyPI has not published — and printing the lower number
    beside it produced `installed 0.27.2 / latest 0.26.0`, a contradiction on one screen
    that invites the reader to distrust every other line in the output.

    So that case reports the staleness instead of the number. ADR 0013: do not present as
    checked what was not checked.
    """
    latest = (load().get("latest") or "").strip()
    if not latest:
        return "— (not checked yet)"
    try:
        stale = bool(installed) and _parse(latest) < _parse(installed)
    except Exception:
        stale = False
    if stale:
        return f"— (cached {latest} is stale: it predates the {installed} you are running)"
    return latest


def fetch_and_store() -> str | None:
    """Query PyPI and cache the result. Runs in the detached child, never inline."""
    import urllib.request
    try:
        with urllib.request.urlopen(_URL, timeout=NET_TIMEOUT) as r:
            latest = json.load(r)["info"]["version"]
    except Exception:
        return None
    try:
        p = _cache_file()
        p.parent.mkdir(parents=True, exist_ok=True)
        p.write_text(json.dumps({"latest": latest, "ts": time.time()}))
    except OSError:
        return None
    return latest


def maybe_spawn() -> None:
    """Kick off a detached refresh if the cache is stale. Non-blocking, best-effort.

    Two independent brakes, because this is called from the status line: the cache
    TTL, and a spawn cooldown that also covers *failed* attempts — otherwise an
    offline machine would fork a doomed child on every single render.
    """
    now = time.time()
    lock = _lock_file()
    try:
        if lock.exists() and now - lock.stat().st_mtime < SPAWN_COOLDOWN:
            return
        if now - (load().get("ts") or 0) < REFRESH_TTL:
            return
    except Exception:
        return
    try:
        lock.parent.mkdir(parents=True, exist_ok=True)
        lock.touch()          # touch FIRST: a spawn storm is worse than a missed check
        subprocess.Popen(
            [sys.executable, "-m", "charter", "_version-check"],
            stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL,
            stdin=subprocess.DEVNULL, start_new_session=True, env=os.environ.copy(),
        )
    except Exception:
        return
