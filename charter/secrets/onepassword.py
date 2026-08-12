"""Native 1Password provider: charter reads **and writes** 1Password items.

The sibling :mod:`reference` provider points *at* a secret someone else created
(``op://vault/item/field``). This one owns the whole lifecycle — ``charter secret set``
creates the item, ``rm`` deletes it — so a credential can be provisioned for a persona
without opening the 1Password UI.

Schema: **one item per vault**, whose custom fields are the secrets.

    charter vault 'devops', key 'AWS_ACCESS_KEY_ID'
        → 1Password item  charter-devops   (or `op-item` from the registry)
          tagged          charter, charter:devops
          field           AWS_ACCESS_KEY_ID, concealed

This replaced one item per key, which could not describe real 1Password vaults: two keys
sharing an item, or a value living in `notesPlain` rather than `password`. Anyone whose
layout it could not express had to keep a separate file of `op://` URIs to work around it.
Custom fields express all of it directly, and the vault becomes one item with one ACL.

The earlier design rejected this for a reason that turned out to be solvable: `op item get
--format json` **conceals** values, so a naive round-trip would write masks back over every
sibling secret. `--reveal` returns the real ones. Two costs remain and are deliberate:

* a write pulls every sibling value through memory. Same trust boundary charter already
  works in, but real — which is why reads use `op read` on the one field and never the
  whole item;
* `op` templates replace rather than merge, so a write is a read-modify-write and two
  agents writing different keys at once can drop one another's field. 1Password keeps item
  history, and `set` verifies the field landed so the failure is loud rather than silent.

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


def _field_name(f: dict) -> str:
    """The key a field is addressable by: its **label**, falling back to its id.

    Label first, and the order is the whole of issue #75. `id` was preferred, and `or`
    only falls through when the left side is falsy — which an id never is on a real item.
    charter-created items hid it completely, because `_write` sets id == label == key; it
    surfaced only on an item charter adopted, where 1Password assigns a random id and
    keeps the human text as the label. Every key then came back as a random string, while
    `doctor` and `vault verify` both reported healthy: resolution genuinely worked, and
    only the names were unusable.

    The fallback matters too — 1Password does not require a label, and a field without one
    stays addressable by id rather than vanishing from the vault.
    """
    return str(f.get("label") or f.get("id") or "")


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
    def op_item(self) -> str:
        """The single 1Password item whose fields are this vault's secrets.

        Defaults to ``charter-<vault>`` so the simple case needs no extra flag, and is
        overridable because the common real case is an item somebody already curates.
        """
        v = (self.config.get("op-item") or self.config.get("op_item") or "").strip()
        return v or f"{_TAG}-{self.name}"

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
        """Read one field via ``op read`` — one call, and only this secret.

        Deliberately not the whole-item round-trip that writes use: reading through the
        item would pull every sibling value into memory to answer a question about one.
        """
        uri = f"op://{self.op_vault}/{self.op_item}/{key}"
        proc = self._run(self._argv("read", "--no-newline", uri))
        if proc.returncode != 0:
            raise SecretNotFound(
                f"no secret '{key}' in vault '{self.name}' "
                f"(field '{key}' of 1Password item '{self.op_item}' in '{self.op_vault}')")
        value = proc.stdout or ""
        return value[:-1] if value.endswith("\n") else value

    def set(self, key: str, value: str) -> None:
        """Create or replace one field, leaving every sibling untouched.

        Read-modify-write, because ``op`` templates replace rather than merge — fetched
        with ``--reveal`` so siblings round-trip as their real values rather than as
        masks, which is the failure that made an earlier design reject this schema.

        Verified afterwards: a concurrent writer can land between the read and the write
        and drop this field, and silently losing a credential somebody believes they
        stored is worse than failing.
        """
        fields = self._fields(reveal=True)
        fields[key] = value
        self._write(fields, creating=fields is not None and not self._item_present())
        if self.get(key) != value:
            raise VaultError(
                f"wrote '{key}' to 1Password item '{self.op_item}' but reading it back "
                f"did not return what was written. Another writer may have replaced the "
                f"item between the read and the write — 1Password keeps item history, so "
                f"check the item's previous versions before retrying.")

    def delete(self, key: str) -> None:
        """Remove one field. The item itself survives — it is the vault, not the secret."""
        fields = self._fields(reveal=True)
        if key not in fields:
            raise SecretNotFound(f"no secret '{key}' in vault '{self.name}'")
        del fields[key]
        self._write(fields, creating=False)

    def keys(self) -> list[str]:
        """The item's field names. Empty when the item does not exist yet."""
        return sorted(self._fields())

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

    def _item_present(self) -> bool:
        proc = self._run(self._argv("item", "get", self.op_item,
                                    "--vault", self.op_vault, "--format", "json"))
        return proc.returncode == 0

    def _fields(self, reveal: bool = False) -> dict:
        """This vault's secrets as ``{field: value}``. ``{}`` if there is no item yet.

        ``reveal`` is **off by default and belongs only to the write path.** With it, `op`
        returns real values — which is what makes the read-modify-write safe, since writing
        back a concealed item would replace every sibling secret with a mask. Without it,
        the field NAMES are still all there, which is all `keys()` and `health()` need.

        That distinction is not cosmetic: `vault list` and `doctor` call `health()`
        routinely, and revealing there would pull every secret in the vault into memory on
        a listing, and could prompt for re-authentication each time.
        """
        argv = ["item", "get", self.op_item, "--vault", self.op_vault, "--format", "json"]
        if reveal:
            argv.append("--reveal")
        proc = self._run(self._argv(*argv))
        if proc.returncode != 0:
            return {}
        try:
            doc = json.loads(proc.stdout or "{}")
        except json.JSONDecodeError as e:
            raise VaultError(f"could not parse 1Password item '{self.op_item}': {e}")
        out: dict = {}
        for f in doc.get("fields") or []:
            name = _field_name(f)
            if not name or "value" not in f:
                continue
            if name in out:
                # 1Password permits two fields with the same label; only the ids
                # disambiguate them. Letting one silently win would make a secret
                # unreachable while `list`, `doctor` and `verify` all reported healthy —
                # invisible loss, which is the one outcome worth failing for.
                raise VaultError(
                    f"1Password item '{self.op_item}' has more than one field labelled "
                    f"'{name}', so charter cannot tell which secret that key means. "
                    f"Rename one in 1Password, then retry.")
            out[name] = f["value"]
        return out

    def _existing_ids(self) -> dict:
        """``{field name: 1Password's id}`` for the item as it stands.

        Adoption must be non-destructive: a field charter did not create carries an id
        1Password generated, and rewriting it to the key name on the next write would
        mutate an identifier the user never chose on an item charter does not own.
        """
        proc = self._run(self._argv("item", "get", self.op_item, "--vault", self.op_vault,
                                    "--format", "json"))
        if proc.returncode != 0:
            return {}
        try:
            doc = json.loads(proc.stdout or "{}")
        except json.JSONDecodeError:
            return {}
        out = {}
        for f in doc.get("fields") or []:
            name = _field_name(f)
            if name and f.get("id"):
                out[name] = str(f["id"])
        return out

    def _write(self, fields: dict, creating: bool) -> None:
        """Replace the item with exactly *fields*. The template travels on stdin.

        Existing ids are carried through so adopting a hand-made item does not renumber
        its fields; only genuinely new ones take the key as their id, which is what makes
        charter-created items round-trip identically.
        """
        ids = {} if creating else self._existing_ids()
        template = json.dumps({
            "title": self.op_item,
            "category": _CATEGORY,
            "tags": [_TAG, self._vault_tag()],
            "fields": [{"id": ids.get(k, k), "label": k, "type": "CONCEALED", "value": v}
                       for k, v in sorted(fields.items())],
        })
        if creating:
            proc = self._run(self._argv("item", "create", "-", "--vault", self.op_vault),
                             stdin=template)
            what = f"creating 1Password item '{self.op_item}'"
        else:
            proc = self._run(self._argv("item", "edit", self.op_item,
                                        "--vault", self.op_vault), stdin=template)
            what = f"updating 1Password item '{self.op_item}'"
        if proc.returncode != 0:
            raise self._fail(what, proc, write=True)

    def _legacy_items(self) -> list[str]:
        """Items left by the one-item-per-key schema — `charter-<vault>-<key>`.

        Their credentials still exist; charter simply stops finding them. Reporting such
        a vault as healthy-and-empty is the failure #55 was about, in a new place.
        """
        prefix = f"{_TAG}-{self.name}-"
        return sorted(t for t in self._list_items() if t.startswith(prefix))

    def health(self) -> tuple[bool, str]:
        """Whether the item is reachable, and how many fields it holds.

        Never reads a value: `vault list` and `doctor` call this routinely, and revealing
        here would hit 1Password on every listing. Reports the count of fields, which
        `op item get` gives without `--reveal`.

        A vault with no item but with leftover one-item-per-key items is NOT healthy: the
        credentials are there and charter can no longer see them, and saying "0 secrets,
        all fine" is exactly the green-line-over-a-broken-subsystem failure of #55.
        """
        if not shutil.which("op"):
            return False, "op CLI not on PATH"
        try:
            n = len(self.keys())
            if n == 0:
                legacy = self._legacy_items()
                if legacy:
                    return False, (
                        f"{len(legacy)} item(s) from the old one-item-per-key layout are "
                        f"not readable as this vault — charter now keeps a vault in a "
                        f"single item ('{self.op_item}'). Their credentials still exist "
                        f"in 1Password; re-register them with `charter secret set`, then "
                        f"delete the old items.")
                return True, f"no secrets yet in item '{self.op_item}'"
        except VaultError as e:
            return False, str(e).split(".")[0]
        return True, f"{n} secret(s) in 1Password item '{self.op_item}'"
