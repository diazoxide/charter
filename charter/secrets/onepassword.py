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

#: Failures of `op` charter recognises, each paired with the provenance that earns its
#: place here. Matching another CLI's English is only defensible because of which way it
#: fails: an unmatched signature costs precision, never correctness — `_fail` falls back
#: to candidates presented as candidates. Adding an entry without a source is how #78
#: happened, so every entry names where its wording was observed.
_DIAGNOSES: tuple[tuple[str, str], ...] = (
    # op 2.34.0, reproduced locally: no account configured on this machine at all.
    ("no accounts configured",
     "`op` has no account configured on this machine. Sign in (`op signin`), or set "
     "this vault's service-account token, then retry."),
    # A real incident: a service-account token that could read the vault but not write
    # to it. 101 is 1Password's own code, carried inside the message — the process still
    # exits 1, which is precisely why the exit status cannot classify this.
    ("you do not have permission",
     "1Password refused this as unauthorised. For a service-account token that usually "
     "means it has no WRITE access to this vault — the same token reads perfectly well, "
     "which is what makes the failure look like a charter bug."),
    # Reported in #322: an agent invoking the path charter invokes saw "Too many requests.
    # Your client has been rate-limited." across ~17 attempts with backoff. `op` still
    # exits 1 — the same code as "no such item" — which is precisely why the exit status
    # cannot classify this, and why it read as four empty vaults. The advice is the part
    # the operator did not get: wait, and do not conclude anything about the contents.
    ("rate-limited",
     "1Password rate-limited this client. Its contents are UNKNOWN — this is not an empty "
     "vault. Wait and retry rather than re-provisioning secrets that are probably there."),
    # Reported in #78 against op 2.34.0: `op` says this when it never parsed the JSON
    # template, which DOES declare a category. The flag it asks for is a red herring,
    # and supplying it is actively destructive, so the hint has to say so.
    ("provide the item category",
     "`op` did not parse the JSON template it was given on stdin — that template does "
     "declare a category, so the flag op asks for is not the real problem. Do NOT add "
     "--category or --title to satisfy it: op then creates the item with every custom "
     "field silently dropped, storing nothing while reporting success (issue #78)."),
)


def _diagnose(stderr: str | None) -> str | None:
    """Recognise a failure of `op`, or admit that we do not.

    Returns a **fixed** string per signature. ``stderr`` is matched against and never
    interpolated into the result, which is what makes "charter errors never carry
    provider output" a property of this function's shape rather than of anyone
    remembering to scrub. There is no path by which op's text can reach a message.
    """
    low = (stderr or "").lower()
    for needle, hint in _DIAGNOSES:
        if needle in low:
            return hint
    return None

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
        """Report what failed, and a cause only where one was actually recognised.

        Errors never carry the process output: ``op``'s stderr can echo the assignment
        it was given, and on a read path its stdout *is* the secret.

        They also never guess. This message used to staple a fixed sentence to every
        write failure — *check that a service-account token has WRITE access; a
        read-only token fails here with (101)*. That was earned once, against a real
        101, and then said unconditionally. In #78 it met a template `op` had refused to
        parse and reported a permissions problem; the reporter went and audited tokens.
        Naming a culprit you have not checked is worse than admitting ignorance, because
        the vague message keeps someone looking and the confident one tells them to stop.
        """
        note = f"{what} failed (op exit {proc.returncode}){self.identity_note()}."
        hint = _diagnose(getattr(proc, "stderr", ""))
        if hint:
            return VaultError(f"{note} {hint}")
        # Nothing matched. The path is the only information left, so it shapes the
        # order of the candidates — and they stay candidates.
        causes = (["the token can read this vault but can not write to it — a "
                   "service-account token fails here while every read succeeds",
                   "another writer changed or removed the item mid-operation"]
                  if write else
                  ["the vault or item does not exist under this identity",
                   "this vault's identity variable is unset, so `op` read it as "
                   "somebody else"]) + \
                 ["`op` is not signed in, or its session has expired"]
        return VaultError(
            f"{note} charter did not recognise this failure. Causes, roughly in order "
            f"of how often they are the real one:\n"
            + "".join(f"    - {c}\n" for c in causes)
            + "  Run the same `op` command yourself to see what it said — charter "
              "withholds op's output because it can contain the secret.")

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

        **One read** (#355). Values, presence and ids are three questions with one answer
        — :meth:`_document` — rather than three ``op item get`` calls that could return
        three different ones. See that method for why the redundancy was worth removing.
        """
        doc = self._document(reveal=True)
        fields = self._fields_of(doc)
        fields[key] = value
        self._write(fields, ids=self._ids_of(doc), creating=doc is None)
        if self.get(key) != value:
            raise VaultError(
                f"wrote '{key}' to 1Password item '{self.op_item}' but reading it back "
                f"did not return what was written. Another writer may have replaced the "
                f"item between the read and the write — 1Password keeps item history, so "
                f"check the item's previous versions before retrying.")

    def delete(self, key: str) -> None:
        """Remove one field. The item itself survives — it is the vault, not the secret.

        One read, for the same reasons as :meth:`set`. ``creating`` is unconditionally
        false here and needs no presence question of its own: reaching the write at all
        means the key was found among this document's fields, so the item exists.
        """
        doc = self._document(reveal=True)
        fields = self._fields_of(doc)
        if key not in fields:
            raise SecretNotFound(f"no secret '{key}' in vault '{self.name}'")
        del fields[key]
        self._write(fields, ids=self._ids_of(doc), creating=False)

    def keys(self) -> list[str]:
        """The item's field names. Empty when the item PROVABLY does not exist yet.

        Raises rather than returning ``[]`` when charter could not read the vault, because
        the caller renders the two differently and cannot tell them apart from a list
        (#322): `cmd_secret_list` prints "has no secrets" and exits 0 for the first, and
        must report and exit non-zero for the second.
        """
        return sorted(self._fields())

    # --- helpers ----------------------------------------------------------- #
    def _list_items(self, tagged: bool = True) -> list[str]:
        """Item titles in the op-vault. Tagged as charter's by default.

        ``tagged=False`` is for the one question the tag cannot answer: is this vault's
        item *there*? An item charter adopted rather than created carries no charter tag —
        that is the whole point of `op-item` — so a tagged listing would report it absent
        and :meth:`_fields` would call an unreadable vault empty for exactly the vaults
        most likely to be somebody's hand-curated item.
        """
        argv = ["item", "list", "--vault", self.op_vault]
        if tagged:
            argv += ["--tags", self._vault_tag()]
        proc = self._run(self._argv(*argv, "--format", "json"))
        if proc.returncode != 0:
            raise self._fail(f"listing vault '{self.name}'", proc)
        try:
            data = json.loads(proc.stdout or "[]")
        except json.JSONDecodeError as e:
            raise VaultError(f"could not parse `op item list` output: {e}")
        return [str(i.get("title", "")) for i in data if isinstance(i, dict)]

    def _exists(self, title: str) -> bool:
        return title in self._list_items()

    def _document(self, reveal: bool = False) -> dict | None:
        """The item as ``op item get`` returns it, or ``None`` when it PROVABLY is not
        there. Raises when charter could not tell — never ``None`` for a failed read.

        **The only `op item get` on any path** (#355). `set()` used to make three, and
        `delete()` two, all fetching this same document: one for the values, one to ask
        whether the item exists, one for the field ids. Traced on 0.50.1::

            0: item get --reveal      <- _fields(reveal=True)
            1: item get               <- _item_present()
            2: item get               <- _existing_ids()

        Call 0 already fetched strictly more than the other two needed. Three costs, all
        real:

        * **three chances to be rate-limited** on the write path, which is where being
          rate-limited is most expensive. #322 and #354 were both reported from rate
          limiting, and the redundancy is what made #354's window exist at all;
        * **the answers could disagree.** The ids came from a different fetch than the
          values they were paired with, so a rename landing between call 0 and call 2 made
          `_existing_ids` key its answer by the NEW label while the values were keyed by
          the old one — charter then found no id for the field and minted a fresh one,
          renumbering a field on an item it does not own. No call failed. Likewise a
          creation landing between call 0 and call 1 turned "proven absent" into "present"
          and flipped `op item create` to `op item edit`, which REPLACES: the template held
          only the key being written and the other writer's secret was gone, with `set()`
          returning success;
        * **one source of truth.** Three code paths answering the same question is the
          shape most of this repo's bugs have turned out to be.

        Two things the split reads did that this one still has to do, because collapsing
        onto the wrong answer would be worse than the redundancy:

        * ``reveal`` stays a parameter and stays **off by default**. It belongs to the
          write path alone: with it `op` returns the real values, which is what makes the
          read-modify-write safe, since writing back a concealed item would replace every
          sibling secret with a mask. `keys()` and `health()` run from `vault list` and
          `doctor` and must not reveal — that would pull every secret in the vault into
          memory on a listing and could prompt for re-authentication each time. The field
          NAMES are all there without it, which is all those two need.
        * ``None`` and ``{"fields": []}`` are **different answers**, and callers must keep
          telling them apart with ``is None``. An item that exists with no fields and an
          item that is not there choose different subcommands, and `_fields`' return value
          cannot express the difference — `{}` means both, which is precisely why
          `_item_present` existed. Getting it wrong creates a second item with a title the
          vault already holds, after which `op item get <title>` is ambiguous and the vault
          is unreadable until a human deletes one by hand.
        """
        argv = ["item", "get", self.op_item, "--vault", self.op_vault, "--format", "json"]
        if reveal:
            argv.append("--reveal")
        proc = self._run(self._argv(*argv))
        if proc.returncode != 0:
            # A vault charter could not READ is not a vault with no secrets (#322).
            #
            # `op item get` exits non-zero for "there is no item yet" and for every way the
            # read can fail — rate limit, expired session, a token that cannot see this
            # vault, the wrong `op-vault` name — and this used to return `{}` for all of
            # them. So a failure arrived everywhere as a benign state: `secret list` said
            # "has no secrets" about a populated vault, `health()` called it fine, and the
            # read-modify-write in `set` piped back an item holding only the key being
            # written, silently dropping every sibling secret. An operator lost an hour to
            # re-provisioning credentials that were already there, and could not verify the
            # result because the same mask was still in place.
            #
            # The two diagnoses demand opposite responses — go and populate it, versus
            # check the identity and treat the contents as unknown — so absence is now
            # PROVEN rather than assumed, and proven by this vault's own identity: if
            # `op item list` can read the vault and the item is not in it, there is no
            # item. If that read fails too, the failure IS the answer and it propagates.
            #
            # Untagged, because an adopted `op-item` carries no charter tag. And no
            # matching of `op`'s English on this path, unlike `_diagnose`, which only
            # sharpens a message: here a missed signature would turn a failure back into
            # an empty vault, which is the bug.
            if self.op_item in self._list_items(tagged=False):
                raise self._fail(f"reading vault '{self.name}'", proc)
            return None
        try:
            doc = json.loads(proc.stdout or "{}")
        except json.JSONDecodeError as e:
            # charter's inability to READ the answer is still a failure, never an absence.
            # #354 had a second parse of these same bytes that swallowed this and returned
            # "no ids" — the same document getting two different answers, which is the
            # class of thing there is now only one parse to have.
            raise VaultError(f"could not parse 1Password item '{self.op_item}': {e}")
        if not isinstance(doc, dict):
            # `None` is this method's sentinel for PROVEN ABSENCE, and `op` exiting 0 is
            # proof of the opposite — so the sentinel has to be unreachable from a
            # successful read, or it is not a sentinel. A body that parses to `null` would
            # otherwise return `None` from here and tell `set` there is no item, on the
            # strength of a read that succeeded: `op item create` against a title the vault
            # already holds, which is the duplicate-item outcome `_item_present` was
            # hardened against in #354. A list or a string would reach `.get` and raise an
            # AttributeError with no vault in the message. This is the one hazard the
            # collapse introduced rather than removed, and it is checked rather than
            # argued away.
            raise VaultError(
                f"could not parse 1Password item '{self.op_item}': `op item get` "
                f"succeeded but did not return a JSON object")
        return doc

    def _fields_of(self, doc: dict | None) -> dict:
        """A document's secrets as ``{field: value}``; ``{}`` for ``None``.

        Pure — no `op` call — so the values and the ids below describe ONE instant.
        ``{}`` here means *this document has no fields*, which is not the same statement
        as *there is no document*; only ``doc is None`` says that, and only the caller
        that still holds ``doc`` can tell.
        """
        out: dict = {}
        for f in (doc or {}).get("fields") or []:
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

    def _fields(self, reveal: bool = False) -> dict:
        """This vault's secrets as ``{field: value}``. ``{}`` if there is no item yet, and
        a :class:`VaultError` if charter could not tell — never ``{}`` for a failed read.

        The read path's shorthand for :meth:`_document` + :meth:`_fields_of`. The write
        path deliberately does NOT use it: it needs the document itself, because presence
        and the field ids are questions this return value cannot answer.
        """
        return self._fields_of(self._document(reveal=reveal))

    @staticmethod
    def _ids_of(doc: dict | None) -> dict:
        """A document's ``{field name: 1Password's id}``; ``{}`` for ``None``.

        Adoption must be non-destructive: a field charter did not create carries an id
        1Password generated, and rewriting it to the key name on the next write would
        mutate an identifier the user never chose on an item charter does not own.

        This used to be `_existing_ids`, a **second** `op item get` (#354, #355). Two
        failures of that fetch used to `return {}` — the #322 swallow arriving as a silent
        MUTATION rather than a false diagnosis, since `{}` does not report anything, it
        tells :meth:`_write` there are no ids to keep. Both are gone with the fetch: there
        is nothing left to fail here, and the ids now come from the same bytes as the
        values, so a rename landing mid-write can no longer key them by a different label.

        Deliberately a different rule from :meth:`_fields_of` over the same document: a
        field with an id but no ``value`` keeps its id, and a field with no id contributes
        none. `_write` only ever looks up names that ARE in the fields, so the extra
        entries are inert — they are kept because narrowing the rule to match would be a
        silent behaviour change dressed as tidying.
        """
        out = {}
        for f in (doc or {}).get("fields") or []:
            name = _field_name(f)
            if name and f.get("id"):
                out[name] = str(f["id"])
        return out

    def _write(self, fields: dict, ids: dict, creating: bool) -> None:
        """Replace the item with exactly *fields*. The template travels on stdin.

        Existing ids are carried through so adopting a hand-made item does not renumber
        its fields; only genuinely new ones take the key as their id, which is what makes
        charter-created items round-trip identically.

        *ids* is passed in rather than fetched here (#355). Fetching meant a second `op
        item get`, and the pairing this template asserts — *this label has this id and this
        value* — is only true if both halves came from one document.
        """
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
