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

from .base import (SecretNotFound, VaultError, VaultProvider, loose_dir_note,
                   make_private_dir, mode_note, short_path as _short)


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

    def _write_private(self, p: Path, payload: dict) -> None:
        """Write *payload* as JSON into *p*, with *p* provably 0600 before a byte of it
        lands — or write nothing at all.

        `os.open(..., O_CREAT|O_TRUNC, 0o600)` does NOT do this, which is what the old
        version of this method claimed (#437). The mode argument applies **only when the
        call creates the inode**; for a file that already exists it is ignored entirely,
        so a vault someone hand-authored at 0644 stayed 0644 for the whole of `json.dump`
        and was chmod-ed to 0600 only afterwards. Measured: pre-existing 0644, mode while
        the plaintext was on disk 0644, mode after `set` 0600, same inode throughout.

        So the order is inverted, and the mode is settled on the **descriptor**, not the
        path:

        1. open ``O_WRONLY|O_CREAT`` *without* ``O_TRUNC`` — the file still holds only its
           previous contents, which are no more exposed than they already were;
        2. ``fchmod`` that descriptor to 0600 — the inode we hold, so nothing swapped at
           the path in between is affected and nothing at the path can be affected instead;
        3. ``fstat`` the same descriptor and **read the mode back**. A chmod that returned
           successfully is not evidence the bits changed: a mount with fixed permissions
           (exFAT, many SMB shares) accepts it and reports the old mode;
        4. only then ``ftruncate`` and write.

        If step 3 still shows a group- or other-accessible mode, this raises before the
        truncate, so the previous contents survive and the plaintext never reaches a file
        charter could not make private. Refusing is the only outcome that keeps the
        sentence above true — warning and writing anyway leaves the value world-readable
        with a warning scrolled off the top of somebody's log.
        """
        make_private_dir(p.parent)
        fd = os.open(str(p), os.O_WRONLY | os.O_CREAT, 0o600)
        try:
            try:
                os.fchmod(fd, 0o600)
            except OSError:
                pass                      # report the mode we actually have, below
            mode = stat.S_IMODE(os.fstat(fd).st_mode)
            if mode & 0o077:
                raise VaultError(
                    f"refusing to write {_short(p)}: it is mode {oct(mode)[-3:]} and "
                    f"charter could not make it 0600, so the plaintext would be readable "
                    f"by other accounts on this machine. Nothing was written.\n"
                    f"  Filesystems with fixed permissions (exFAT, many network mounts) "
                    f"cannot hold a plain-file vault — point the vault at a path on a "
                    f"filesystem that keeps modes, or use a provider that does not store "
                    f"plaintext.")
            os.ftruncate(fd, 0)
            with os.fdopen(fd, "w") as f:
                fd = -1                   # fdopen owns it now; closing twice is a bug
                json.dump(payload, f, indent=2, ensure_ascii=False)
                f.write("\n")
        finally:
            if fd >= 0:
                os.close(fd)

    def _save(self, data: dict) -> None:
        self._write_private(self.path, data)

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
        charter has to leave at 0600. `set`/`delete` need no call for a different reason
        than the one this used to give: not because `O_CREAT` "recreates the file at 0600"
        — it does not, the mode argument is ignored for an inode that already exists
        (#437) — but because :meth:`_write_private` settles the mode on the descriptor and
        reads it back before it writes anything.

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
        # Same writer as the vault itself. The sidecar holds key NAMES and dates, not
        # values, but it sits in the same directory and had the same in-place bug.
        self._write_private(self._meta_path, meta)

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
        note = loose_dir_note(self.loose_dirs())
        if not pp.exists():
            # The note belongs here too: what another account can list is the DIRECTORY,
            # and it is just as listable before this vault's file exists as after. Leaving
            # it off this branch would also make `vault list` and `doctor` disagree about
            # a vault that has been added and not yet written to.
            return True, f"not created yet ({_short(pp)})" + (f", {note}" if note else "")
        try:
            count = len(self._load())
        except VaultError as e:
            return False, str(e)
        # `base.mode_note`, not a second `oct(...)[-3:]` here. `reference` had no sentence
        # about its file's mode at all, and the fix for that (#491) is one wording for both
        # providers rather than a copy that goes stale in the same way `loose_dirs` did.
        return True, ", ".join(
            [f"{count} secret(s)"] + [s for s in (mode_note(pp), note) if s])

    # `loose_dirs` is INHERITED — the implementation lives on `base.VaultProvider` and is
    # keyed on `file_path`, so `reference` answers it too (#491). The eight lines that used
    # to sit here reported the directory for the one provider that had them; the argument
    # for why they are not copied per provider is `VaultProvider.loose_dirs`'s, and it is
    # the same argument `VaultProvider.file_path` makes about the two byte-identical `path`
    # properties it replaced.
