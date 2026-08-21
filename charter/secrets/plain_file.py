"""Plain-file vault provider: a JSON object of key -> secret, mode 0600.

The developer may point a vault at any file they already keep (``--file``), or
let charter manage one under ``.charter/vaults/``. Values may be multi-line (e.g. a
kubeconfig or PEM), which JSON handles cleanly.
"""

from __future__ import annotations

import datetime
import json
import os
import stat
from pathlib import Path

from .base import SecretNotFound, VaultError, VaultProvider


def _short(p: Path) -> str:
    """A path as it should be SHOWN — relative to the plane when it lives inside it.

    `charter vault list` printed the absolute path in its STATUS column, which is noise
    and leaks one developer's local layout into terminal output other people see (issue
    #21's aside). Inside the plane the relative form is both shorter and the same string
    everyone else would see.
    """
    from .. import config as _config
    try:
        return str(Path(p).resolve().relative_to(Path(_config.ROOT).resolve()))
    except (ValueError, OSError):
        return str(p)


class PlainFileProvider(VaultProvider):
    id = "plain-file"
    label = "Plain file (JSON, 0600)"
    available = True

    @property
    def path(self) -> Path:
        return self.file_path      # shared resolution — see VaultProvider.file_path

    def _load(self) -> dict:
        """Read the vault. **Never writes** — see :meth:`_tighten` for why that matters."""
        p = self.path
        if not p.exists():
            return {}
        try:
            data = json.loads(p.read_text() or "{}")
        except json.JSONDecodeError as e:
            raise VaultError(f"vault file {p} is not valid JSON: {e}")
        if not isinstance(data, dict):
            raise VaultError(f"vault file {p} must be a JSON object of key -> secret")
        return data

    def _save(self, data: dict) -> None:
        p = self.path
        p.parent.mkdir(parents=True, exist_ok=True)
        # Create with 0600 from the start so the plaintext is never briefly world-readable.
        fd = os.open(str(p), os.O_WRONLY | os.O_CREAT | os.O_TRUNC, 0o600)
        with os.fdopen(fd, "w") as f:
            json.dump(data, f, indent=2, ensure_ascii=False)
            f.write("\n")
        os.chmod(p, 0o600)

    @staticmethod
    def _tighten(p: Path) -> None:
        """Force a vault file to 0600 if any group/other bit is set (tighten only,
        never loosen). Self-heals a file created outside ``set`` — e.g. hand-authored
        JSON, which inherits the umask default (often 0644) — so plaintext secrets are
        never left readable once charter touches the vault. Best-effort; never raises.

        **Called from the value paths, never from `_load`** (#331). It used to sit in
        `_load`, which put it under `health()` — and `health()` is called by `doctor` from
        the SessionStart hook and by the status line behind a TTL cache. A committed
        `vaults.json` can point a vault at any path on the machine, so charter chmod-ed a
        file outside the control plane, unprompted, with nobody watching, and reported the
        vault green while doing it. A health check that writes is the defect regardless of
        which file it writes to.

        Moving it here keeps the protection where the plaintext actually is: `get` takes
        secret values out of the file, and a vault charter has read the secrets of is one
        charter has to leave at 0600. `set`/`delete` need no call — `_save` recreates the
        file at 0600 with `O_CREAT` and chmods it again.

        The read-only paths — `health`, `keys`, `ages` — now REPORT a loose mode instead
        of silently fixing it. `health()` already had that branch; `_tighten` running
        first is what made it unreachable, because the file was 0600 by the time the mode
        was read.
        """
        try:
            if stat.S_IMODE(p.stat().st_mode) & 0o077:
                os.chmod(p, 0o600)
        except OSError:
            pass

    def get(self, key: str) -> str:
        # Before the read, not after: the point is that the plaintext is not sitting in a
        # group-readable file while charter is handing it out.
        self._tighten(self.path)
        data = self._load()
        if key not in data:
            raise SecretNotFound(f"secret '{key}' not found in vault '{self.name}'")
        return str(data[key])

    def set(self, key: str, value: str) -> None:
        data = self._load()
        data[key] = value
        self._save(data)
        m = self._load_meta()
        m[key] = {"set_at": datetime.date.today().isoformat()}  # for the rotation audit
        self._save_meta(m)

    def delete(self, key: str) -> None:
        data = self._load()
        if key not in data:
            raise SecretNotFound(f"secret '{key}' not found in vault '{self.name}'")
        del data[key]
        self._save(data)
        m = self._load_meta()
        if m.pop(key, None) is not None:
            self._save_meta(m)

    # --- rotation metadata: a 0600 sidecar tracking when each key was last set --- #
    @property
    def _meta_path(self) -> Path:
        p = self.path
        return p.parent / (p.stem + ".meta.json")

    def _load_meta(self) -> dict:
        p = self._meta_path
        if not p.exists():
            return {}
        try:
            d = json.loads(p.read_text() or "{}")
            return d if isinstance(d, dict) else {}
        except Exception:
            return {}

    def _save_meta(self, meta: dict) -> None:
        p = self._meta_path
        p.parent.mkdir(parents=True, exist_ok=True)
        fd = os.open(str(p), os.O_WRONLY | os.O_CREAT | os.O_TRUNC, 0o600)
        with os.fdopen(fd, "w") as f:
            json.dump(meta, f, indent=2)
            f.write("\n")
        os.chmod(p, 0o600)

    def ages(self) -> dict:
        """key -> age in days since last set (None if set before tracking existed)."""
        meta = self._load_meta()
        today = datetime.date.today()
        out: dict[str, int | None] = {}
        for k in self.keys():
            sa = (meta.get(k) or {}).get("set_at")
            try:
                out[k] = (today - datetime.date.fromisoformat(sa)).days if sa else None
            except (ValueError, TypeError):
                out[k] = None
        return out

    def keys(self) -> list[str]:
        return sorted(self._load().keys())

    def health(self) -> tuple[bool, str]:
        if not self.config.get("file"):
            return False, "no 'file' configured"
        pp = self.file_path
        if not pp.exists():
            return True, f"not created yet ({_short(pp)})"
        try:
            count = len(self._load())
        except VaultError as e:
            return False, str(e)
        mode = stat.S_IMODE(pp.stat().st_mode)
        perms = "" if mode == 0o600 else f", perms {oct(mode)[-3:]} (want 600)"
        return True, f"{count} secret(s){perms}"
