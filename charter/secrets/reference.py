"""Reference vault provider: entries are **URIs into someone else's secret store**.

For a team that already runs HashiCorp Vault or 1Password, a charter vault would
otherwise be a *third* place a credential lives — and therefore a third place it
can go stale or diverge. This provider keeps charter's plumbing (``secret exec``,
``--dotenv``, redaction, "never in argv") while leaving **storage** where it
already is: the vault file holds a reference like ``op://Eng/deploy/token``, and
the value is fetched from the real store at read time.

What this file stores is therefore *not* a secret. It is still written 0600, for
the same reason a bookmark to a safe is worth not publishing: the reference names
your vault layout.

Supported today::

    op://<vault>/<item>/<field>          → 1Password CLI   (`op read`)
    vault://<path>#<field>               → HashiCorp Vault (`vault kv get`)

Adding a scheme is one entry in :data:`_RESOLVERS`. Each entry maps a URI to an
**argv list** — never a shell string — so a reference can never be command
injection, whatever it contains.
"""

from __future__ import annotations

import json
import os
import shutil
from pathlib import Path
from urllib.parse import urlsplit

from .. import util
from .base import SecretNotFound, VaultError, VaultProvider


def _op_argv(uri: str) -> tuple[list[str], str]:
    """``op://vault/item/field`` → the 1Password CLI reads the whole URI itself."""
    parts = urlsplit(uri)
    if not parts.netloc or len(parts.path.strip("/").split("/")) < 2:
        raise VaultError(f"malformed 1Password reference '{uri}' — "
                         "expected op://<vault>/<item>/<field>")
    return ["op", "read", "--no-newline", uri], "op"


def _vault_argv(uri: str) -> tuple[list[str], str]:
    """``vault://secret/data/app#FIELD`` → ``vault kv get -field=FIELD secret/data/app``."""
    parts = urlsplit(uri)
    path = (parts.netloc + parts.path).strip("/")
    field = parts.fragment
    if not path or not field:
        raise VaultError(f"malformed Vault reference '{uri}' — "
                         "expected vault://<path>#<FIELD>")
    return ["vault", "kv", "get", f"-field={field}", path], "vault"


#: scheme → (argv builder, CLI name). Argv, never a shell string.
_RESOLVERS = {"op": _op_argv, "vault": _vault_argv}


def scheme_of(value: str) -> str | None:
    """The reference scheme of *value*, or None if it isn't a supported reference."""
    scheme = urlsplit(value or "").scheme
    return scheme if scheme in _RESOLVERS else None


class ReferenceProvider(VaultProvider):
    id = "reference"
    label = "Reference (op://, vault:// — resolved at read time)"
    available = True

    #: Indirection so tests never shell out. Signature matches ``util.run``.
    runner = staticmethod(util.run)

    @property
    def path(self) -> Path:
        return self.file_path      # shared resolution — see VaultProvider.file_path

    def _load(self) -> dict:
        p = self.path
        if not p.exists():
            return {}
        try:
            data = json.loads(p.read_text() or "{}")
        except json.JSONDecodeError as e:
            raise VaultError(f"vault file {p} is not valid JSON: {e}")
        if not isinstance(data, dict):
            raise VaultError(f"vault file {p} must be a JSON object of key -> reference")
        return data

    def _save(self, data: dict) -> None:
        p = self.path
        p.parent.mkdir(parents=True, exist_ok=True)
        p.write_text(json.dumps(data, indent=2, sort_keys=True) + "\n")
        os.chmod(p, 0o600)

    # --- CRUD ------------------------------------------------------------- #
    def set(self, key: str, value: str) -> None:
        """Store a *reference*. Rejects anything that isn't a supported URI.

        Refusing a bare value here is the whole point: silently accepting one
        would turn a reference vault into a plaintext vault without saying so,
        which is exactly the divergence this provider exists to prevent.
        """
        value = (value or "").strip()
        if not scheme_of(value):
            raise VaultError(
                f"'{key}': a reference vault stores a URI, not a value — expected one of "
                f"{', '.join(s + '://' for s in sorted(_RESOLVERS))}.\n"
                f"  A reference vault POINTS AT items somebody else owns, which is why it "
                f"declines to create one. To have charter own the item — creating it and "
                f"storing the value, with the value on stdin and never in argv:\n"
                f"      charter vault add <name> --provider 1password --op-vault <VAULT>\n"
                f"      charter secret set <name> {key} --from-file <path>\n"
                f"  To keep the value on this machine instead: --provider plain-file.\n"
                f"  To register an item you created elsewhere: pass its URI here.")
        _RESOLVERS[scheme_of(value)](value)  # validate shape now, not at read time
        data = self._load()
        data[key] = value
        self._save(data)

    def reference_for(self, key: str) -> str:
        """The stored URI for *key* — safe to print; it is not the secret."""
        data = self._load()
        if key not in data:
            raise SecretNotFound(f"no secret '{key}' in vault '{self.name}'")
        return data[key]

    def get(self, key: str) -> str:
        """Resolve the reference through its CLI and return the value.

        Never raises with the resolved value in the message: a resolver's stderr
        can echo what it fetched, so failures report the reference and the CLI's
        exit status only.
        """
        uri = self.reference_for(key)
        scheme = scheme_of(uri)
        if not scheme:
            raise VaultError(f"'{key}' in vault '{self.name}' is not a supported reference: "
                             f"'{uri}'")
        argv, cli = _RESOLVERS[scheme](uri)
        if not shutil.which(cli):
            raise VaultError(f"'{key}' needs the '{cli}' CLI to resolve {uri} — it is not "
                             f"on PATH. Install it and authenticate, then retry.")
        proc = self.runner(argv, check=False, env=self.env_overlay())
        if proc.returncode != 0:
            raise VaultError(
                f"resolving '{key}' via {cli} failed (exit {proc.returncode}) for {uri}"
                f"{self.identity_note()}.\n"
                f"  Causes, roughly in order of how often they are the real one:\n"
                f"    - the item or field behind the reference was renamed, moved or "
                f"deleted (the vault stays healthy — `charter vault verify` tests this)\n"
                f"    - you are not authenticated to {cli}\n"
                f"    - the identity variable for this vault is unset, so {cli} read the "
                f"vault as somebody else\n"
                f"    - charter version drift: an older build may not map this vault's "
                f"identity into {cli}'s environment (`charter version` shows the pin; "
                f"`charter version sync` conforms this machine)\n"
                "  (Resolver output withheld — it can contain the secret.)")
        value = proc.stdout or ""
        # `op read --no-newline` already omits it; `vault kv get -field=` appends one.
        return value[:-1] if value.endswith("\n") else value

    def delete(self, key: str) -> None:
        data = self._load()
        if key not in data:
            raise SecretNotFound(f"no secret '{key}' in vault '{self.name}'")
        del data[key]
        self._save(data)

    def keys(self) -> list[str]:
        return sorted(self._load())

    def health(self) -> tuple[bool, str]:
        """Report reference count and resolver availability — never resolve anything.

        ``vault list`` and ``doctor`` call this routinely; resolving here would hit
        1Password/Vault on every listing and could prompt for re-auth.
        """
        try:
            data = self._load()
        except VaultError as e:
            return False, str(e)
        if not data:
            return True, "no references yet"
        needed = sorted({s for s in (scheme_of(v) for v in data.values()) if s})
        missing = [s for s in needed if not shutil.which(_RESOLVERS[s](
            next(v for v in data.values() if scheme_of(v) == s))[1])]
        n = len(data)
        if missing:
            clis = ", ".join(sorted({_RESOLVERS[s](
                next(v for v in data.values() if scheme_of(v) == s))[1] for s in missing}))
            return False, f"{n} reference(s), but not on PATH: {clis}"
        return True, f"{n} reference(s) via {', '.join(needed)}"
