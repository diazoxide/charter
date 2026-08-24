"""Where ``charter secret cp`` has put plaintext, so the read guards can cover it.

``secret cp`` exists so a tool that insists on a file (a kubeconfig, a client cert) can
have one without the value passing through the model's context. The value still lands on
disk, at a path the caller chose — and both vault guards only ever matched ``.charter/``.
So the file it writes was outside every guard, and ``secret cp v k /tmp/x && cat /tmp/x``
was a two-command, in-policy read of any vault value. The `--reveal` denial named ``cp``
as the remedy, which made this the *documented* way around itself.

This is the ledger that closes it: every destination ``secret cp`` writes is recorded, and
:func:`charter.hooks._leak_reason` / :func:`charter.hooks.pretooluse_read` refuse to read
anything in it.

Three properties worth stating, because each is a place this could have gone wrong:

* **It holds paths, never values.** Same posture as ``.charter/vaults.json``, the registry:
  knowing that a secret was materialised at a path is not knowing the secret. 0600 anyway —
  a materialisation path can name a customer, a cluster or a role.
* **It is append-only-ish and self-pruning.** An entry whose file no longer exists is
  dropped on the next write, so a long-lived plane does not accumulate a list of every temp
  path it ever used. Dropping on READ would let an attacker clear the ledger by deleting
  and recreating the file, so reads never prune.
* **It is best-effort on the write side and fail-closed on the read side.** Recording is
  wrapped by the caller so a ledger problem never breaks `secret cp`; the guard treats an
  unreadable ledger as empty, which is the same posture the rest of `hooks` takes on a hot
  path — and the guarded `.charter/` paths do not depend on it.

Two known limits, stated rather than papered over:

* charter records where it *put* the file. If something later moves or copies it, the copy
  is not in the ledger. Covering that needs content scanning on every read, which is a
  different and much more expensive guard.
* a search rooted at a DIRECTORY that happens to contain a materialised file is not
  refused. That is the same posture :func:`charter.hooks.pretooluse_read` already takes for
  `.charter/vaults/` — "denying every broad search is untenable, so this checks the path
  the caller actually named" — and a rule that differed between the two would be a rule
  nobody could state.
"""

from __future__ import annotations

import json
import os
import time
from pathlib import Path

from . import config


def ledger_path() -> Path:
    """The ledger file. Resolved per call, because ``config.STATE_DIR`` is redirected by
    tests (and by ``$CHARTER_HOME``) and a module-level constant would freeze the real
    developer's path into the test run."""
    return Path(config.STATE_DIR) / "materialized.json"


def _entries() -> list[dict]:
    try:
        data = json.loads(ledger_path().read_text())
    except (OSError, ValueError):
        return []
    return [e for e in data.get("paths", []) if isinstance(e, dict) and e.get("path")]


def record(dest: str | os.PathLike, vault: str = "", key: str = "") -> None:
    """Record that a secret was materialised at *dest*. Never raises.

    *vault* and *key* are NAMES; no value reaches this file. They are here so an operator
    reading the ledger can tell which credential is sitting on disk and rotate it.
    """
    try:
        path = os.path.realpath(os.path.expanduser(str(dest)))
        kept = [e for e in _entries()
                if e.get("path") != path and os.path.exists(e.get("path", ""))]
        kept.append({"path": path, "vault": vault, "key": key, "at": int(time.time())})
        p = ledger_path()
        p.parent.mkdir(parents=True, exist_ok=True)
        fd = os.open(str(p), os.O_WRONLY | os.O_CREAT | os.O_TRUNC, 0o600)
        with os.fdopen(fd, "w") as f:
            json.dump({"paths": kept}, f, indent=2)
        os.chmod(p, 0o600)
    except Exception:
        return


def paths() -> frozenset[str]:
    """Every recorded destination, as realpaths. Empty when nothing was ever materialised.

    One `os.stat` in the common case: the hot Bash-hook path asks this on every tool call,
    and a plane that has never run `secret cp` has no file to read.
    """
    p = ledger_path()
    try:
        st = p.stat()
    except OSError:
        return frozenset()
    stamp = (st.st_mtime_ns, st.st_size, str(p))
    cached = _CACHE.get("stamp")
    if cached == stamp:
        return _CACHE["paths"]
    known = frozenset(e["path"] for e in _entries())
    _CACHE["stamp"], _CACHE["paths"] = stamp, known
    return known


#: mtime-keyed, so a `secret cp` in one process is visible to the next hook process (each
#: hook is its own process anyway) and re-reading a hot file costs a stat, not a parse.
_CACHE: dict = {"stamp": None, "paths": frozenset()}


def covers(operand: str, cwd: str = "", here: str = "") -> bool:
    """Whether *operand*, as written on a command line, names a materialised secret.

    *cwd* is the session's directory (the hook payload carries it) and *here* is where an
    earlier ``cd`` in the same command left us. A relative path is resolved against both,
    because a guard that only understood absolute paths would be satisfied by `cd /tmp`.
    """
    known = paths()
    if not known or not operand:
        return False
    raw = os.path.expanduser(operand)
    if os.path.isabs(raw):
        return os.path.realpath(raw) in known
    bases = [""]                       # the hook process's own cwd, as a last resort
    if cwd:
        bases.append(cwd)
    if here:
        bases.append(here if os.path.isabs(here) else os.path.join(cwd or "", here))
    return any(os.path.realpath(os.path.join(b, raw)) in known for b in bases)
