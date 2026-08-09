"""The vault registry: which vaults exist, their provider + config + persona.

**Two files, one view.**

``vaults.json`` at the plane root is the SHARED half — committed, beside ``personas/``
and ``inventory/``. It carries what is identical on every machine: provider, persona,
op-vault, and a ``file`` relative to the plane. That is what makes the wiring travel:
reference vaults hold ``op://`` URIs rather than values and are meant to be committed, but
before this the index that located them hard-coded one developer's home directory, so a
fresh clone found the vault files present and unusable (issue #21).

``.charter/vaults.json`` (0600, gitignored) is the LOCAL half — this developer's own
vaults, plus per-machine overrides of shared ones. It wins on conflict.

The split is per FIELD, not per vault, and it is small: going through them, only
``account`` — which 1Password account this developer is signed into — genuinely differs
between machines.

**Registering is local by default.** ``--share`` publishes. That direction is deliberate
and matches ``[memory].share``, which defaults to ``local`` so "a fresh control plane must
never publish agent-written notes by accident". The same applies here with more force: a
registry names which personas hold credentials and where their files are, which is a map
worth having even without the values, and charter's own repo is public.
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


#: Config keys that never travel — see the module docstring's field-by-field pass.
LOCAL_ONLY_KEYS = ("account",)


def _read(path) -> dict:
    if not path.exists():
        return {"vaults": {}}
    try:
        doc = json.loads(path.read_text())
    except json.JSONDecodeError as e:
        raise VaultError(f"vault registry {path} is corrupt: {e}")
    doc.setdefault("vaults", {})
    return doc


def load_shared() -> dict:
    return _read(config.SHARED_VAULTS)


def load_local() -> dict:
    return _read(config.VAULTS_REGISTRY)


def load_registry() -> dict:
    """The merged view every reader gets: shared as the base, local layered over it.

    Merged per FIELD rather than per vault, so a developer can pin `account` on a vault
    the team declares without having to restate the provider, the file and the persona —
    restating them is how a local copy silently drifts from the shared one.
    """
    merged = {}
    shared, local = load_shared(), load_local()
    for name, entry in shared.get("vaults", {}).items():
        merged[name] = json.loads(json.dumps(entry))          # deep copy, no aliasing
    for name, entry in local.get("vaults", {}).items():
        if name not in merged:
            merged[name] = json.loads(json.dumps(entry))
            continue
        base = merged[name]
        for k, v in entry.items():
            if k == "config":
                base.setdefault("config", {}).update(v or {})
            elif v is not None:
                base[k] = v
    return {"vaults": merged}


def _write(path, doc: dict, mode: int) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    fd = os.open(str(path), os.O_WRONLY | os.O_CREAT | os.O_TRUNC, mode)
    with os.fdopen(fd, "w") as f:
        json.dump(doc, f, indent=2, ensure_ascii=False)
        f.write("\n")
    os.chmod(path, mode)


def save_registry(doc: dict) -> None:
    """Write the LOCAL half. Kept for callers that read-modify-write the merged view.

    0600 as before: this file holds per-developer paths and account pins.
    """
    _write(config.VAULTS_REGISTRY, doc, 0o600)


def save_shared(doc: dict) -> None:
    """Write the SHARED half — committed, so 0644 rather than 0600. It carries no secret
    values by construction: `LOCAL_ONLY_KEYS` never reaches it, and a `file` is a path."""
    _write(config.SHARED_VAULTS, doc, 0o644)


def vaults(doc: dict | None = None) -> dict:
    return (doc or load_registry()).get("vaults", {})


def scope_of(name: str) -> str:
    """``"shared"``, ``"local"``, or ``"both"`` — where *name* is registered.

    `vault list` shows this because "why does my teammate not see this vault" and "why is
    this vault in git" are the two questions a two-file registry invites.
    """
    in_shared = name in load_shared().get("vaults", {})
    in_local = name in load_local().get("vaults", {})
    return "both" if (in_shared and in_local) else ("shared" if in_shared else "local")


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
              force: bool = False, share: bool = False) -> None:
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

    ``share`` writes the committed half instead of the local one. Off by default, matching
    ``[memory].share`` — a registration must never be published by accident, because it
    names which personas hold credentials and where their files are. Local-only keys
    (:data:`LOCAL_ONLY_KEYS`) are split off and kept local even then, so a shared entry
    never carries one developer's 1Password account pin.
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

    cfg = dict(cfg or {})
    local_cfg = {k: cfg.pop(k) for k in LOCAL_ONLY_KEYS if k in cfg}
    entry = {"provider": provider, "persona": persona, "config": cfg}

    if share:
        shared = load_shared()
        shared["vaults"][name] = entry
        save_shared(shared)
        # An account pin still belongs to this machine, so it is layered on top rather
        # than published with the rest of the entry.
        if local_cfg:
            local = load_local()
            local["vaults"].setdefault(name, {}).setdefault("config", {}).update(local_cfg)
            save_registry(local)
    else:
        entry["config"] = {**cfg, **local_cfg}
        local = load_local()
        local["vaults"][name] = entry
        save_registry(local)


def remove_vault(name: str) -> None:
    """Remove *name* from wherever it is registered — both halves if both carry it.

    Removing from one and leaving the other is how a vault comes back from the dead after
    the user watched charter say it was gone.
    """
    shared, local = load_shared(), load_local()
    in_shared, in_local = name in shared["vaults"], name in local["vaults"]
    if not (in_shared or in_local):
        raise VaultNotConfigured(f"no vault named '{name}'")
    if in_shared:
        del shared["vaults"][name]
        save_shared(shared)
    if in_local:
        del local["vaults"][name]
        save_registry(local)


def vaults_for_persona(persona: str, doc: dict | None = None) -> list[str]:
    return sorted(n for n, v in vaults(doc).items() if v.get("persona") == persona)
