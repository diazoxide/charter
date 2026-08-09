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
        """
        from pathlib import Path
        from .. import config as _config

        p = self.config.get("file")
        if not p:
            raise VaultError(f"vault '{self.name}' has no 'file' configured")
        p = Path(p).expanduser()
        return p if p.is_absolute() else (_config.ROOT / p)

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
    """Replace every occurrence of each secret value in ``text`` with ``***``.

    A defence-in-depth net: even if a wrapped command echoes a secret, it is
    scrubbed before the output is handed back to the caller (and the model).
    Longest values first, so a value that contains a shorter one is masked whole.
    """
    for s in sorted((s for s in secrets if s), key=len, reverse=True):
        text = text.replace(s, "***")
    return text
