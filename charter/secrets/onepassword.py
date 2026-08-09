"""Native 1Password provider: charter reads **and writes** 1Password items.

The sibling :mod:`reference` provider points *at* a secret someone else created
(``op://vault/item/field``). This one owns the whole lifecycle — ``charter secret set``
creates the item, ``rm`` deletes it — so a credential can be provisioned for a persona
without opening the 1Password UI.

Schema: **one item per key**, not one item per vault.

    charter vault 'devops', key 'KUBECONFIG'
        → 1Password item titled  charter-devops-KUBECONFIG
          tagged                 charter, charter:devops
          value in its           password field

One item per *vault* would have been tidier in the 1Password UI and is the obvious
first design — it is also wrong here, for a reason worth recording. Updating one field
of a multi-field item means a read-modify-write through a JSON template, and a template
**replaces** the item; meanwhile ``op item get --format json`` *conceals* values unless
asked otherwise. Round-tripping that would overwrite every sibling secret with a mask.
One item per key removes the interaction entirely: each write touches exactly the
credential it was asked to touch.

**No secret ever reaches argv.** 1Password's own help says it plainly — "Command
arguments get logged in your command history, and can be visible to other processes on
your machine. If you're assigning sensitive values, use a JSON template instead" — so
writes pipe a template on **stdin** (``op item create -``, ``op item edit <item>``).
Deletes and lookups pass only names, which are not secrets.
"""

from __future__ import annotations

import json
import shutil

from .. import util
from .base import SecretNotFound, VaultError, VaultProvider

#: Every item charter creates carries this, so `keys()` can list its own without
#: sweeping a shared 1Password vault the team also uses by hand.
_TAG = "charter"

#: 1Password category whose primary field is a concealed `password`.
_CATEGORY = "PASSWORD"


class OnePasswordProvider(VaultProvider):
    id = "1password"
    label = "1Password (op CLI — charter creates and manages the items)"
    available = True

    #: Indirection so tests never shell out. ``(argv, input=None) -> CompletedProcess``.
    runner = staticmethod(util.run)

    # --- configuration ----------------------------------------------------- #
    @property
    def op_vault(self) -> str:
        """The 1Password vault items live in. Not charter's vault name."""
        v = (self.config.get("op-vault") or self.config.get("op_vault") or "").strip()
        if not v:
            raise VaultError(
                f"vault '{self.name}' has no 'op-vault' configured — which 1Password "
                f"vault should its items live in? Re-register it:\n"
                f"  charter vault add {self.name} --provider 1password --op-vault <NAME>")
        return v

    @property
    def account(self) -> str | None:
        return (self.config.get("account") or "").strip() or None

    def _title(self, key: str) -> str:
        return f"{_TAG}-{self.name}-{key}"

    def _vault_tag(self) -> str:
        return f"{_TAG}:{self.name}"

    def _argv(self, *args: str) -> list[str]:
        """``op`` invocation with the account pin applied, if configured.

        A machine signed into several 1Password accounts resolves an unqualified vault
        name against whichever is default, which is a quiet way to write a credential
        into the wrong company's vault.
        """
        argv = ["op", *args]
        if self.account:
            argv += ["--account", self.account]
        return argv

    def _run(self, argv: list[str], stdin: str | None = None):
        if not shutil.which("op"):
            raise VaultError(
                "the 1Password CLI ('op') is not on PATH. Install it and sign in "
                "(https://developer.1password.com/docs/cli/), then retry.")
        return self.runner(argv, input=stdin, check=False, env=self.env_overlay())

    def _fail(self, what: str, proc, write: bool = False) -> VaultError:
        """Errors never carry the process output.

        ``op``'s stderr can echo the assignment it was given, and on a read path its
        stdout *is* the secret. Report the exit status and what was attempted.

        Write failures name **permissions** first, from experience: against a real
        account the first failure was ``(101) You do not have permission to perform
        this action`` — a service-account token that could read the vault but not write
        to it. The original message suggested checking that `op` was signed in and the
        vault existed; both were true, so it sent the reader looking in the wrong place.
        """
        hint = ("Check that `op` is signed in (`op whoami`), that the vault exists, and "
                "— for a service-account token — that it has WRITE access to this vault; "
                "a read-only token fails here with 1Password error (101)."
                if write else
                "Check that `op` is signed in (`op whoami`) and the vault exists.")
        return VaultError(
            f"{what} failed (op exit {proc.returncode}){self.identity_note()}. {hint} "
            "(op output withheld — it can contain the secret.)")

    # --- CRUD -------------------------------------------------------------- #
    def get(self, key: str) -> str:
        """Read via ``op read``, which returns the bare value on stdout."""
        uri = f"op://{self.op_vault}/{self._title(key)}/password"
        proc = self._run(self._argv("read", "--no-newline", uri))
        if proc.returncode != 0:
            # `op read` cannot distinguish "no such item" from "no access" without
            # parsing localised stderr, so a miss is reported as a miss and the
            # remedy for both is the same: check the vault and your session.
            raise SecretNotFound(
                f"no secret '{key}' in vault '{self.name}' "
                f"(1Password item '{self._title(key)}' in vault '{self.op_vault}')")
        value = proc.stdout or ""
        return value[:-1] if value.endswith("\n") else value

    def set(self, key: str, value: str) -> None:
        """Create the item, or replace its password — the value travels on stdin.

        The template is a whole item because ``op`` templates replace rather than
        merge. That is safe *because* of the one-item-per-key schema: the only field
        charter owns on this item is its password.
        """
        title = self._title(key)
        template = json.dumps({
            "title": title,
            "category": _CATEGORY,
            "tags": [_TAG, self._vault_tag()],
            "fields": [{"id": "password", "type": "CONCEALED",
                        "purpose": "PASSWORD", "label": "password", "value": value}],
        })
        if self._exists(title):
            proc = self._run(self._argv("item", "edit", title,
                                        "--vault", self.op_vault), stdin=template)
            if proc.returncode != 0:
                raise self._fail(f"updating '{key}' in 1Password", proc, write=True)
            return
        proc = self._run(self._argv("item", "create", "-",
                                    "--vault", self.op_vault), stdin=template)
        if proc.returncode != 0:
            raise self._fail(f"creating '{key}' in 1Password", proc, write=True)

    def delete(self, key: str) -> None:
        title = self._title(key)
        if not self._exists(title):
            raise SecretNotFound(f"no secret '{key}' in vault '{self.name}'")
        proc = self._run(self._argv("item", "delete", title, "--vault", self.op_vault))
        if proc.returncode != 0:
            raise self._fail(f"deleting '{key}' from 1Password", proc, write=True)

    def keys(self) -> list[str]:
        """Item titles charter owns in this vault, mapped back to key names.

        Filtered by tag rather than by title prefix: the 1Password vault may well be a
        shared one the team also fills by hand, and charter must not list — far less
        offer to delete — an item it did not create.
        """
        items = self._list_items()
        prefix = f"{_TAG}-{self.name}-"
        return sorted(t[len(prefix):] for t in items if t.startswith(prefix))

    # --- helpers ----------------------------------------------------------- #
    def _list_items(self) -> list[str]:
        proc = self._run(self._argv("item", "list", "--vault", self.op_vault,
                                    "--tags", self._vault_tag(), "--format", "json"))
        if proc.returncode != 0:
            raise self._fail(f"listing vault '{self.name}'", proc)
        try:
            data = json.loads(proc.stdout or "[]")
        except json.JSONDecodeError as e:
            raise VaultError(f"could not parse `op item list` output: {e}")
        return [str(i.get("title", "")) for i in data if isinstance(i, dict)]

    def _exists(self, title: str) -> bool:
        return title in self._list_items()

    def health(self) -> tuple[bool, str]:
        """Count items — never read a value.

        ``vault list`` and ``doctor`` call this routinely, and a read would hit
        1Password every time and could prompt for re-authentication.
        """
        if not shutil.which("op"):
            return False, "op CLI not on PATH"
        try:
            n = len(self.keys())
        except VaultError as e:
            return False, str(e).split(".")[0]
        return True, f"{n} item(s) in 1Password vault '{self.op_vault}'"
