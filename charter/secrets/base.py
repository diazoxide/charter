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
