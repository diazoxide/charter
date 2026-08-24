"""The masked shape of a secret: a keyed fingerprint and a size band.

`charter secret get` without `--reveal` has to say *something* about the value, because
the questions it exists to answer are real ones — "is this present", "is it the same value
the other vault has", "did the whole token land or half of it". What it printed was::

    audit2/WEAK: present · 11 bytes · sha256:323725e8eff4

which answers a fourth question nobody meant to expose: **"is the value `Summer2024!`?"**
An unsalted, un-keyed SHA-256 prefix is a function of the value and nothing else, so
anyone holding that line — an agent transcript, a pasted terminal, a ticket, a log
shipped off the machine — can run a wordlist against it offline, with no further access
to charter. The byte count prefilters the list and the digest confirms the hit. Decisive
against a human-chosen password; the digest is the whole attack, and 48 bits is plenty to
confirm with (#436).

Two changes, one per half of that line.

**The digest is keyed.** ``hmac(plane_key, value)`` instead of ``sha256(value)``, where
*plane_key* is 32 random bytes generated on first use and kept 0600 under ``.charter/``.
The fingerprint stays stable and comparable *within one control plane*, which is all
anyone ever compares — is this the same value as before, is it the same value that vault
has — and stops being computable by anyone who does not hold the key. Comparability
*across* planes is the loss, and it is the same property as the attack: a digest that
means the same thing on every machine is an oracle on every machine.

**The size is a band, not a count.** ``8–15 bytes`` instead of ``11 bytes``. A band still
distinguishes "empty", "a password", "a token", "a PEM file" — the reason a human reads
it — while giving a wordlist about one bit to filter on instead of ten.

Neither of these makes `secret get` safe to paste. SECURITY.md's framing holds: guard
rails, not guarantees. The claim here is narrow and testable — *the masked line is not a
function of the value alone*, so it cannot be checked against a guess offline. Two things
that claim does **not** cover, said out loud rather than left to be discovered:

- **Whoever holds the key holds the oracle.** The offline check comes back intact for
  anyone who can read ``.charter/fingerprint.key``. That is why `hooks._VAULT_PATH_RE`
  denies it to the harness's file-reading tools alongside ``.charter/vaults/`` — it
  matters most for a 1Password-backed vault: there is no vault file on this machine for
  that guard to refuse, so the key is what it has to cover. A shell running as you still
  reads the key, as it reads everything else you own — this is a guard rail, not a
  boundary.
- **Within one plane it is still an equality oracle.** Someone who can run
  `charter secret set` here can store a guess and compare fingerprints. They can also run
  `charter secret get --reveal --force` and read the value outright, so this grants them
  nothing they did not already have — but it is the reason the key is per-plane and not
  per-vault. Per-vault salting would close it and would also break the one comparison the
  fingerprint exists to serve: *do these two vaults hold the same value*.
"""

from __future__ import annotations

import hmac
import os
import stat
from pathlib import Path

from .base import make_private_dir

#: 256 bits, matching the HMAC's own block security. Read back and length-checked on
#: every use, so a truncated file (a create interrupted midway) is regenerated rather
#: than quietly used as a short key.
KEY_BYTES = 32

KEY_FILE = "fingerprint.key"


def key_path() -> Path:
    """Where the plane's fingerprint key lives — resolved per call, never cached.

    ``config.STATE_DIR`` is rebound by ``tests/_isolation.py`` (and by ``$CHARTER_HOME``),
    so a module-level constant would pin the real ``.charter/`` into every test process.
    """
    from .. import config

    return Path(config.STATE_DIR) / KEY_FILE


def _key() -> bytes | None:
    """The plane's fingerprint key, generating it on first use. ``None`` if it cannot be
    read or created — a read-only ``.charter/``, a plane state directory that does not
    exist and cannot be made.

    ``None`` means callers print **no fingerprint at all**. It must never mean "fall back
    to an unkeyed digest": a fallback that restores the property the key exists to remove
    is the bug wearing the fix's clothes, and it would fire in exactly the constrained
    environments nobody watches.
    """
    p = key_path()
    try:
        existing = p.read_bytes()
    except OSError:
        existing = b""
    if len(existing) == KEY_BYTES:
        return existing

    try:
        make_private_dir(p.parent)
        new = os.urandom(KEY_BYTES)
        # Same descriptor-first discipline as the vault writer (#437): the mode argument
        # to `os.open` is ignored for an inode that already exists, so a leftover
        # world-readable key file would keep its mode. fchmod the descriptor, read the
        # mode back off it, and refuse to write key material into a file that is still
        # readable by other accounts.
        fd = os.open(str(p), os.O_WRONLY | os.O_CREAT, 0o600)
        try:
            try:
                os.fchmod(fd, 0o600)
            except OSError:
                pass
            if stat.S_IMODE(os.fstat(fd).st_mode) & 0o077:
                return None
            os.ftruncate(fd, 0)
            os.write(fd, new)
        finally:
            os.close(fd)
        return new
    except OSError:
        return None


def fingerprint(value: str) -> str | None:
    """A 12-hex-character keyed fingerprint of *value*, or ``None`` when this plane has
    no key and none can be made.

    Keyed, so it is not reproducible from *value* alone. Truncated to 48 bits because the
    only comparison anyone makes with it is against another fingerprint printed by the
    same plane, and a preimage search is off the table without the key regardless of
    length.
    """
    k = _key()
    if k is None:
        return None
    return hmac.new(k, value.encode("utf-8"), "sha256").hexdigest()[:12]


def size_band(value: str) -> str:
    """The size of *value* as a power-of-two band: ``empty``, ``1–15 bytes``,
    ``16–31 bytes``, … ``1024+ bytes``.

    Bands, not counts, because an exact length is the prefilter half of the offline
    check — it cuts a wordlist by an order of magnitude before the digest is consulted at
    all. A band leaves roughly one bit.

    The first band deliberately starts at 1 rather than continuing the doubling downward:
    a "1 byte"/"2–3 bytes" label would pin very short values almost exactly, and nothing a
    reader does with this line needs that resolution. ``empty`` stays its own label
    because a stored empty value is a real, actionable mistake (`secret set` refuses one
    outright for the same reason) and blurring it into "short" would hide it.

    Counted in **UTF-8 bytes**, which is what the word "bytes" claimed and `len()` on a
    `str` did not: the old line called 3 characters of Cyrillic "3 bytes" when the file
    held 6.
    """
    n = len(value.encode("utf-8"))
    if n == 0:
        return "empty"
    if n < 16:
        return "1–15 bytes"
    if n >= 1024:
        return "1024+ bytes"
    lo = 1 << (n.bit_length() - 1)
    return f"{lo}–{(lo << 1) - 1} bytes"


def masked(value: str) -> str:
    """The half-line describing *value* without disclosing it: size band, and the keyed
    fingerprint when this plane has one."""
    fp = fingerprint(value)
    return f"{size_band(value)} · fp:{fp}" if fp else size_band(value)
