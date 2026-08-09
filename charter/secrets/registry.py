"""The vault registry: which vaults exist, their provider + config + persona.

Persisted at ``.charter/vaults.json`` (0600, gitignored). This file may reference
provider config such as file paths or token locations, so it is treated as
sensitive and never committed.
"""

from __future__ import annotations

import json
import os

from .. import config
from .base import VaultError, VaultNotConfigured, VaultProvider
from .onepassword import OnePasswordProvider
from .plain_file import PlainFileProvider
from .reference import ReferenceProvider

#: provider id -> implementation class. Add real providers here as they land.
PROVIDERS: dict[str, type[VaultProvider]] = {
    cls.id: cls
    for cls in (
        PlainFileProvider,
        ReferenceProvider,
        OnePasswordProvider,
    )
}


def load_registry() -> dict:
    if not config.VAULTS_REGISTRY.exists():
        return {"vaults": {}}
    try:
        doc = json.loads(config.VAULTS_REGISTRY.read_text())
    except json.JSONDecodeError as e:
        raise VaultError(f"vault registry {config.VAULTS_REGISTRY} is corrupt: {e}")
    doc.setdefault("vaults", {})
    return doc


def save_registry(doc: dict) -> None:
    config.STATE_DIR.mkdir(parents=True, exist_ok=True)
    fd = os.open(str(config.VAULTS_REGISTRY), os.O_WRONLY | os.O_CREAT | os.O_TRUNC, 0o600)
    with os.fdopen(fd, "w") as f:
        json.dump(doc, f, indent=2, ensure_ascii=False)
        f.write("\n")
    os.chmod(config.VAULTS_REGISTRY, 0o600)


def vaults(doc: dict | None = None) -> dict:
    return (doc or load_registry()).get("vaults", {})


def get_vault_config(name: str, doc: dict | None = None) -> dict:
    vc = vaults(doc).get(name)
    if not vc:
        raise VaultNotConfigured(
            f"no vault named '{name}'. Register one with `charter vault add {name} "
            f"--provider plain-file --file <path>`."
        )
    return vc


def provider_for(name: str, doc: dict | None = None) -> VaultProvider:
    vc = get_vault_config(name, doc)
    pid = vc.get("provider")
    cls = PROVIDERS.get(pid)
    if not cls:
        raise VaultError(f"vault '{name}' uses unknown provider '{pid}'")
    return cls(name, vc.get("config", {}))


def add_vault(name: str, provider: str, cfg: dict, persona: str | None = None,
              force: bool = False) -> None:
    """Register a vault. Refuses to replace an existing registration unless *force*.

    Registering over a name that already exists used to overwrite it in place — different
    provider, no prompt, exit 0 (issue #22). The registration is the ONLY pointer to a
    plain-file vault's secrets, so replacing it does not migrate anything: it strands the
    file on disk with nothing referring to it, and `charter secret get` then reports the
    key as missing rather than as unreachable. Observed during a real migration, where
    three vaults were re-registered onto 1Password and accepted silently.

    That also broke the rule the rest of charter states and follows — additive: never
    delete or rename a user's thing to make room; name the blocker and refuse. `init`,
    `reinit` and `_create_baseline_dirs` all work this way.

    ``force`` is the deliberate override, the same idiom `charter persona create --force`
    already uses. It still does not migrate: moving secrets between providers is its own
    operation and must be typed on purpose, not ride along inside `add`.
    """
    if provider not in PROVIDERS:
        raise VaultError(
            f"unknown provider '{provider}'. Available: {', '.join(sorted(PROVIDERS))}"
        )
    doc = load_registry()
    existing = doc["vaults"].get(name)
    if existing is not None and not force:
        old = existing.get("provider", "?")
        where = (existing.get("config") or {}).get("file")
        detail = f" ({where})" if where else ""
        raise VaultError(
            f"vault '{name}' is already registered with provider '{old}'{detail}. "
            f"charter will not replace it: the registration is the only pointer to that "
            f"vault's secrets, so replacing it strands them with nothing referring to "
            f"them.\n"
            f"  keep both:     charter vault add <other-name> --provider {provider} …\n"
            f"  inspect it:    charter vault list\n"
            f"  replace anyway: re-run with --force (this does NOT migrate secrets)"
        )
    doc["vaults"][name] = {"provider": provider, "persona": persona, "config": cfg}
    save_registry(doc)


def remove_vault(name: str) -> None:
    doc = load_registry()
    if name not in doc["vaults"]:
        raise VaultNotConfigured(f"no vault named '{name}'")
    del doc["vaults"][name]
    save_registry(doc)


def vaults_for_persona(persona: str, doc: dict | None = None) -> list[str]:
    return sorted(n for n, v in vaults(doc).items() if v.get("persona") == persona)
