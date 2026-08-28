"""Vault provider interface, errors, and the output redactor."""

from __future__ import annotations

from abc import ABC, abstractmethod


class VaultError(RuntimeError):
    """Base class for all vault failures."""


class VaultNotConfigured(VaultError):
    """No vault by that name is registered."""


class SecretNotFound(VaultError):
    """The vault has no secret under that key."""


class ProviderUnavailable(VaultError):
    """The provider isn't implemented yet, or its backend/CLI is missing."""


def vault_file_path(configured):
    """Where a vault's ``file`` config actually lands: absolute as given, relative to the
    plane root otherwise.

    A module function rather than only a property because `doctor` has to ask the same
    question — "does this vault's file lie outside the plane?" (#331) — about a vault it
    has no reason to instantiate. `VaultProvider.file_path` already records why two
    implementations of "where is this file" is the wrong number: `plain-file` and
    `reference` had byte-identical `path` properties, and that is how one of them quietly
    keeps resolving the old way. A third copy in `doctor` would be the same mistake with a
    fresh coat of paint.
    """
    from pathlib import Path
    from .. import config as _config

    p = Path(configured).expanduser()
    return p if p.is_absolute() else (Path(_config.ROOT) / p)


#: Every bit that lets an account other than the owner reach a directory. The guard is
#: written as this mask rather than as a comparison against a list of bad modes (0755,
#: 0750, 0775, …) on purpose: a literal set of bad values is a spelling, and the next mode
#: nobody listed walks straight past it. ``mode & _OTHERS`` is the property.
#:
#: **What the mask still cannot see, said out loud rather than left to be discovered.**
#: A POSIX mode is not the whole access-control story on either platform charter runs on.
#: macOS extended ACLs (``chmod +a "user:bob allow read,list"``) and Linux POSIX.1e ACLs
#: (``setfacl -m u:bob:rx``) grant another account full access while ``st_mode`` still
#: reads 0700 — so a directory this function calls private can be readable by someone
#: else, and `loose_dirs` will not report it. That is a real hole and it is not closed
#: here: reading an ACL means `acl_get_file`, which is not in the standard library on
#: either platform, and shelling out to `ls -le`/`getfacl` from a health check that
#: `doctor` runs at every session start is a worse trade than the gap. SECURITY.md's
#: framing applies unchanged — guard rails, not guarantees.
_OTHERS = 0o077


def make_private_dir(p) -> None:
    """Create directory *p*, and **every level of it charter has to create**, at 0700.

    ``Path.mkdir(parents=True, mode=0o700)`` does not do this, which is what the first
    cut of #437 assumed. CPython's ``pathlib`` applies *mode* to the **leaf only** — the
    missing parents are created by a bare recursive ``self.parent.mkdir(parents=True,
    exist_ok=True)``, i.e. at ``0o777 & ~umask``. Measured on a plane where charter
    created every level itself, for a vault configured at ``.charter/vaults/team/prod.json``::

        .charter               0755
        .charter/vaults        0755      <-- lists every vault name, machine-wide
        .charter/vaults/team   0700

    So the directory the fix was *about* — the one holding the vault names — came out
    world-listable on the very path where charter, not the operator, chose the mode. The
    leaf was 0700 and the claim was still false.

    Each missing level is created individually and chmod-ed explicitly. The chmod is not
    redundant with ``os.mkdir``'s *mode*: mkdir's argument is masked by the umask, so a
    process running under ``umask 077`` is fine but one under a permissive-in-the-wrong-
    direction umask is not guaranteed the bits it asked for. The chmod cannot open the
    directory wider than 0700 — it names 0700 outright — so there is no window in which
    the directory is looser than it ends up.

    **A directory that already exists is left exactly as it is.** Not an oversight and not
    laziness: a vault's ``file`` may name any path on this machine (see
    :meth:`VaultProvider.file_path`), so "tighten whatever directory we land in" is how
    charter would come to chmod ``~/`` or a shared team directory unprompted, with nobody
    watching — the #331 defect with a fresh coat of paint. charter tightens what it
    creates and **reports** what it did not; :meth:`PlainFileProvider.health` is where the
    report comes out.

    **One implementation, two names.** The walk lives in :func:`charter.config.private_mkdir`
    now, because the writers that create ``.charter/`` are not only the vault writers —
    the registry, the persona and workspace pointers, the caches and the session markers
    all get there first on some flow, and each of them needs this without importing
    `charter.secrets` (#470). This name stays: it is what the three secrets writers call,
    and what `tests/test_vault_dir_mode.py` measures.
    """
    from .. import config as _config

    _config.private_mkdir(p)


def loose_dirs(leaf, stop) -> list:
    """The directories from *leaf* up to and including *stop* that any account other than
    the owner can reach, as a list of ``(path, mode)``, outermost last.

    Answers the half of :func:`make_private_dir` that charter deliberately does not fix:
    a ``.charter/vaults/`` that predates the fix keeps its 0755, because tightening a
    directory charter did not create is the thing that must not happen unprompted. Naming
    it on a health line is the honest alternative to either silently chmod-ing it or
    claiming in a news entry that it cannot happen.

    *stop* bounds the walk so this reports the plane's own directories and not ``/Users``
    or ``/``, which are 0755 on every machine and are nobody's defect. A *leaf* that is
    not underneath *stop* at all — a vault whose ``file`` points somewhere else entirely,
    which is a supported configuration — yields just *leaf* itself: charter has an opinion
    about the directory it puts a vault in, and none about that directory's ancestors on
    someone else's filesystem layout.

    The mask is ``mode & 0o077``, never a comparison against known-bad modes. 0755 is the
    one everybody thinks of; 0705, 0711, 0730 and 0701 all list or traverse just as well
    and appear on no such list.
    """
    import os
    import stat as _stat
    from pathlib import Path

    leaf = Path(leaf)
    try:
        stop_rp = Path(stop).resolve()
    except OSError:
        stop_rp = None

    chain, cur, seen = [], leaf, set()
    while True:
        chain.append(cur)
        try:
            rp = cur.resolve()
        except OSError:
            break
        if rp == stop_rp or rp in seen or cur.parent == cur:
            break
        seen.add(rp)
        cur = cur.parent
    # Not underneath *stop*: the walk ran to the filesystem root without meeting it, so
    # keep only the directory charter itself chose.
    if stop_rp is None or not any(_same(c, stop_rp) for c in chain):
        chain = chain[:1]

    out = []
    for d in chain:
        try:
            st = os.stat(d)
        except OSError:
            continue
        if _stat.S_ISDIR(st.st_mode) and _stat.S_IMODE(st.st_mode) & _OTHERS:
            out.append((d, _stat.S_IMODE(st.st_mode)))
    return out


#: :func:`loose_dirs` under a name that reads as what it does from inside a class that also
#: has a ``loose_dirs`` method. Same object; `plain_file` imported it under this name for
#: the same reason, and :meth:`VaultProvider.loose_dirs` calls it.
_dirs_up_to = loose_dirs


def _same(p, resolved) -> bool:
    from pathlib import Path

    try:
        return Path(p).resolve() == resolved
    except OSError:
        return False


def short_path(p) -> str:
    """A path as it should be SHOWN — relative to the plane when it lives inside it.

    `charter vault list` printed the absolute path in its STATUS column, which is noise
    and leaks one developer's local layout into terminal output other people see (issue
    #21's aside). Inside the plane the relative form is both shorter and the same string
    everyone else would see.
    """
    from pathlib import Path

    from .. import config as _config
    try:
        return str(Path(p).resolve().relative_to(Path(_config.ROOT).resolve()))
    except (ValueError, OSError):
        return str(p)


def loose_dir_note(loose) -> str:
    """``listed by other accounts: .charter 755 (want 700 — chmod 700)``, or ``""``.

    **One wording, two commands.** `charter vault list` prints it as its STATUS column and
    `charter doctor` puts it on the vaults line (#471); if each rendered its own sentence,
    the day somebody rewords one is the day an operator can no longer grep for the thing
    they were told to look for. It is also the sentence tests assert on, and two of them
    means one gets updated.

    Terse on purpose: it lands in a table row, and *why* charter will not fix the
    directory itself is a paragraph that belongs in `docs/secrets.md`. The row carries what
    is wrong and what to type.
    """
    if not loose:
        return ""
    named = ", ".join(f"{short_path(d)} {oct(m)[-3:]}" for d, m in loose)
    return f"listed by other accounts: {named} (want 700 — chmod 700)"


def mode_note(p) -> str:
    """``perms 644 (want 600)`` for a vault file that is not 0600, or ``""``.

    The FILE half of :func:`loose_dir_note`, extracted here for the same reason that one
    exists: `plain-file` rendered this sentence inline inside `health` and `reference`
    rendered nothing at all, so a vault file another account can read said so on one
    provider and stayed silent on the other (#491). Both providers write 0600, and both
    hold something worth not publishing — a reference file is not a secret, but it names
    every item and field this plane reaches (see :mod:`charter.secrets.reference`).

    Reports, never repairs, which is the posture the read-only paths already take: `health`,
    `keys` and `ages` NAME a loose mode rather than quietly chmod-ing it, because a health
    check that writes is the defect whatever it writes to — `doctor` calls this from the
    SessionStart hook, and a vault's ``file`` may name any path on this machine (#331,
    :meth:`PlainFileProvider._tighten`).

    Never raises. A health line that can throw is a `doctor` that cannot run, and the empty
    string is the honest answer for a mode charter could not read: the file's own existence
    is reported by the branch above every caller's, so silence here is never the only thing
    said about a file. The catch is `Exception` and not `BaseException`, deliberately —
    `tests/_planeguard.py` signals a test reaching the real ``.charter/`` with a
    `BaseException` precisely so a fallback like this cannot turn it into a quiet "".
    """
    import os
    import stat as _stat

    try:
        mode = _stat.S_IMODE(os.stat(p).st_mode)
    except Exception:
        return ""
    return "" if mode == 0o600 else f"perms {oct(mode)[-3:]} (want 600)"


class VaultProvider(ABC):
    """One configured vault, backed by a concrete provider.

    Subclasses set :attr:`id`/:attr:`label`/:attr:`available` and implement the
    four CRUD methods. Instances are created by the registry with the vault's
    name and its provider-specific ``config`` dict.
    """

    #: Provider id stored in the registry, e.g. ``"plain-file"``.
    id: str = ""
    #: Human-readable label for listings.
    label: str = ""
    #: Whether this provider is implemented and usable today.
    available: bool = True

    def __init__(self, name: str, config: dict | None = None) -> None:
        self.name = name
        self.config = config or {}

    def env_overlay(self) -> dict:
        """Environment this vault's CLI is invoked with — ``{}`` when it declares none.

        A vault may bind the identity it is read through, as a mapping from the variable
        the CLI reads to the variable this machine carries it in::

            "env": {"OP_SERVICE_ACCOUNT_TOKEN": "OP_ACME_DEVOPS_SERVICE_ACCOUNT_TOKEN"}

        Only NAMES are stored — never a value — so the registry stays inert if it leaks
        and the binding is reviewable in git. Least-privilege setups issue one service
        account per scope, and without this the mapping lives in every caller's shell:
        `OP_SERVICE_ACCOUNT_TOKEN="$OP_ACME_DEVOPS_…" charter secret exec devops -- …`,
        which is the property the vault abstraction otherwise removes.

        A declared source that is unset or empty **raises**. Falling back to whatever
        ambient token exists would read the vault under an identity the plane did not
        declare, and 1Password answers that with "no items" or a permission error — the
        wrong-identity failure this exists to eliminate, not a variant of it. Vaults that
        declare nothing are untouched, so single-account setups see no change.

        Both providers call this rather than each resolving `config["env"]`, so a second
        implementation cannot quietly keep reading the ambient environment.
        """
        import os

        mapping = self.config.get("env") or {}
        if not mapping:
            return {}
        out = {}
        for target, source in mapping.items():
            val = os.environ.get(source)
            if not val:
                raise VaultError(
                    f"vault '{self.name}' is read through ${source}, which is unset. "
                    f"charter will not fall back to an ambient ${target}: that would read "
                    f"this vault under an identity it does not declare, and the failure "
                    f"would look like a missing secret rather than a wrong credential.\n"
                    f"  export {source}=… , or drop the binding: "
                    f"charter vault add {self.name} --provider {self.id} --force"
                )
            out[target] = val
        return out

    def identity_note(self) -> str:
        """One clause naming where this vault's credential comes from, or ``""``.

        Appended to read failures so a permission error points at the identity in play
        instead of reading as an empty vault.
        """
        mapping = self.config.get("env") or {}
        srcs = ", ".join(f"${s}" for s in mapping.values())
        return f" (identity from {srcs})" if srcs else ""

    @property
    def file_path(self):
        """Absolute path of this vault's ``file``, resolving a RELATIVE one against the
        control-plane root. Raises when the provider needs a file and none is configured.

        A relative value is how the registry becomes portable (issue #21). Reference
        vaults hold ``op://`` URIs rather than values, so a team commits them and expects
        a fresh clone to inherit the wiring — but the registry that finds them hard-coded
        one developer's home directory, so the committed vault files were invisible on any
        other machine until someone re-registered them by hand.

        Absolute stays valid and untouched, for a vault deliberately kept outside the
        plane. Resolution lives here rather than in each provider because `plain-file` and
        `reference` had byte-identical `path` properties, and two implementations of "where
        is this file" is how one of them quietly keeps resolving the old way.

        **Say the consequence out loud, because it was only ever implied (#331).**
        `vaults.json` at the plane root is the COMMITTED half of the registry, and `file`
        is not a local-only key — so a file in git can name any path on this machine as a
        vault, with no containment check and no confirmation. That is working as designed
        and it stays that way: `commands_secrets` tells the operator to "point --file
        outside the plane" as the remedy for a plain-file vault git would otherwise commit,
        so a containment rule here would refuse the configuration charter itself
        recommends. What bounds it instead:

        * `get()`/`set()` on such a vault need an operator to run a `charter secret`
          command naming a vault they did not register. Nothing unattended reads it.
        * **Nothing unattended WRITES it, either** — `PlainFileProvider.health()` used to
          chmod this path from the SessionStart hook and no longer does; see
          `PlainFileProvider._tighten`.
        * `doctor` names a shared-half vault whose file lands outside the plane, on its
          green line, so the configuration is visible rather than merely legal.
        """
        p = self.config.get("file")
        if not p:
            raise VaultError(f"vault '{self.name}' has no 'file' configured")
        return vault_file_path(p)

    @abstractmethod
    def get(self, key: str) -> str:
        """Return the secret value, or raise :class:`SecretNotFound`."""

    @abstractmethod
    def set(self, key: str, value: str) -> None:
        """Create or overwrite a secret."""

    @abstractmethod
    def delete(self, key: str) -> None:
        """Remove a secret, or raise :class:`SecretNotFound`."""

    @abstractmethod
    def keys(self) -> list[str]:
        """Return the secret names (never the values)."""

    def health(self) -> tuple[bool, str]:
        """``(ok, detail)`` used by ``charter doctor`` and ``charter vault list``.

        Must never include a secret value in ``detail``.
        """
        return True, "ok"

    def loose_dirs(self) -> list:
        """``(path, mode)`` for each directory this vault is reached through that another
        account on the machine can reach. Empty for a provider that keeps nothing on disk.

        **The structured answer, so two commands cannot disagree about it.** The note used
        to exist only inside the string :meth:`health` returns, which is why `doctor` — who
        keeps the boolean and drops the string — could not report it however much its docs
        said it did (#471). Substring-matching `health`'s prose from `doctor` would have
        "fixed" it in the way this codebase keeps being bitten by: the report would go
        silent the day somebody rewords the sentence, and nothing would fail. Both surfaces
        now ask this, and :func:`loose_dir_note` renders it for both.

        A provider that stores its data somewhere else entirely (1Password, Vault) has no
        directory of charter's to report, so the honest answer is an empty list rather than
        a guess about a backend charter does not own.

        **Which providers those are is decided by** :attr:`file_path`, **not by a second
        list somebody keeps in step.** This used to live on `PlainFileProvider` alone, and
        the base default was a bare ``return []`` — honest for 1Password and *false* for
        `reference`, which writes a file under ``.charter/vaults/`` through the same private
        walk and reported nothing about the directory holding it, on `vault list` and on
        `doctor` alike (#491). A reference file is not a secret; the DIRECTORY still lists
        every vault name on the plane, which is the exposure this report exists for.

        Two copies of the same eight lines was the wrong number for the same reason
        :attr:`file_path` gives about the two byte-identical ``path`` properties it
        replaced: the second copy is how one provider quietly keeps answering the old way.
        So the question is asked of the one place that already knows where a vault's file
        is. A provider with no ``file`` in its config — 1Password today, anything backed by
        a real service tomorrow — makes :attr:`file_path` raise, and the answer is the
        empty list it always was, without a new attribute for a new provider to forget.

        Never raises: a health line that can throw is a `doctor` that cannot run. The catch
        is `Exception`, not `BaseException`, deliberately — `tests/_planeguard.py` signals a
        test reaching the real ``.charter/`` with a `BaseException` precisely so that a
        fallback like this one cannot turn it into a quiet empty answer.
        """
        from .. import config as _config

        try:
            return _dirs_up_to(self.file_path.parent, _config.STATE_DIR)
        except Exception:
            return []


def redact(text: str, secrets: list[str]) -> str:
    """Mask the values this call resolved, where they appear literally in ``text``.

    A defence-in-depth net, not a boundary, and the two words are the whole point: this
    sees only the values it was handed and only their exact bytes. A wrapped command that
    *transforms* one — ``base64``, ``rev``, a JSON re-encode — hands back something this
    cannot recognise, and ``--exec``/``--stream`` capture nothing, so nothing arrives here
    to mask (#444). What it does cover is the accidental echo: a ``curl -v`` printing an
    ``Authorization`` header back into captured output.

    Longest values first, so a value that contains a shorter one is masked whole.
    """
    for s in sorted((s for s in secrets if s), key=len, reverse=True):
        text = text.replace(s, "***")
    return text
