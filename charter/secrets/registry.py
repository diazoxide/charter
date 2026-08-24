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
from pathlib import Path

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


def usable_vaults(half: dict) -> dict:
    """The entries in one half of the registry that are vault entries at all.

    A vault entry that is not an object is not a vault entry. `vaults.json` is COMMITTED
    and hand-editable, and `provider_for` reached straight for `entry.get("provider")` —
    so a string there raised `AttributeError` out of `doctor.check_vaults`, which runs
    from the SessionStart hook and catches only `VaultError` (#347).

    **The one place that decides what a half contains**, so a reader that *counts* entries
    and a reader that *resolves* them cannot disagree about which ones exist. They did:
    `load_registry` dropped the malformed entry while `doctor.check_vault_registry_
    divergence` read the halves raw — which is both why it counted an entry the line above
    it had just called ignored, and why it was handed the string it crashed on (#363).
    Defending here rather than in each reader is the same single-source rule that put the
    drop in `load_registry` in the first place; the mistake was having two implementations
    of "what is in this file", not where the first one lived.

    Nothing is hidden by the drop: `doctor` names what was dropped (see
    :func:`malformed_shared`).
    """
    return {n: e for n, e in (half or {}).get("vaults", {}).items() if isinstance(e, dict)}


def load_registry() -> dict:
    """The merged view every reader gets: shared as the base, local layered over it.

    Merged per FIELD rather than per vault, so a developer can pin `account` on a vault
    the team declares without having to restate the provider, the file and the persona —
    restating them is how a local copy silently drifts from the shared one.
    """
    merged = {}
    shared, local = load_shared(), load_local()
    for name, entry in usable_vaults(shared).items():
        merged[name] = json.loads(json.dumps(entry))          # deep copy, no aliasing
    for name, entry in usable_vaults(local).items():
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


def malformed_shared() -> list[str]:
    """Names in the COMMITTED half whose entry is not an object, and was therefore
    dropped from the merged view. Never raises — a corrupt file reads as none.

    The complement of :func:`usable_vaults` rather than its own `isinstance` test, so the
    names `doctor` reports as ignored are exactly the ones the merged view left out.
    """
    try:
        half = load_shared()
        kept = usable_vaults(half)
        return sorted(n for n in half.get("vaults", {}) if n not in kept)
    except VaultError:
        return []


def shared_files_outside_plane() -> list[str]:
    """Vaults whose ``file`` is decided by the COMMITTED half and lands outside the plane.

    Absolute is legal and stays legal: `VaultProvider.file_path` blesses it for "a vault
    deliberately kept outside the plane" (#21), and `commands_secrets` actively tells the
    operator to "point --file outside the plane" as the remedy for a plaintext vault git
    would otherwise commit. Refusing it here would break the configuration charter's own
    error message recommends.

    What was missing is that the committed half can decide it and nothing said so (#331) —
    so this NAMES it and refuses nothing. The local half is excluded on purpose: that is
    where a human typed the path, and nothing committed chose it.
    """
    from .base import vault_file_path

    try:
        shared = load_shared().get("vaults", {})
        local = load_local().get("vaults", {})
    except VaultError:
        return []
    out = []
    for name, entry in sorted(shared.items()):
        if not isinstance(entry, dict):
            continue
        # A local override of `file` means this machine's path was chosen locally, so the
        # committed value is not what resolves and there is nothing to report.
        if ((local.get(name) or {}).get("config") or {}).get("file"):
            continue
        configured = (entry.get("config") or {}).get("file")
        if not configured:
            continue
        try:
            p = vault_file_path(configured).resolve()
            p.relative_to(Path(config.ROOT).resolve())
        except (ValueError, OSError):
            out.append(name)
    return out


def _write(path, doc: dict, mode: int) -> None:
    """Write *doc* to *path* at *mode*, with the mode in force before the content is.

    The mode argument to `os.open` is ignored for an inode that already exists, so this
    used to write the local registry into whatever mode the file already had and chmod it
    afterwards — the same mistake `PlainFileProvider._write_private` documents at length
    (#437). Settled on the descriptor, before the truncate, for the same reasons.

    Unlike the vault writer this does not refuse a mode it could not set: *mode* is 0644
    for the SHARED half, which is committed and meant to be world-readable, so "still has
    group bits" is not an error condition here. This file carries provider config, paths
    and environment variable NAMES — never a value — which is what makes the weaker
    posture the right one rather than an oversight.
    """
    path.parent.mkdir(parents=True, exist_ok=True)
    fd = os.open(str(path), os.O_WRONLY | os.O_CREAT, mode)
    try:
        try:
            os.fchmod(fd, mode)
        except OSError:
            pass
        os.ftruncate(fd, 0)
        with os.fdopen(fd, "w") as f:
            fd = -1
            json.dump(doc, f, indent=2, ensure_ascii=False)
            f.write("\n")
    finally:
        if fd >= 0:
            os.close(fd)


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
        #
        # The local half is also REDUCED to those keys, which is the other half of the
        # same idea. `load_registry` layers local over shared per field, so a registration
        # that already existed locally shadows the one just published — every field of it,
        # guaranteed, not as a race. Publishing without clearing it therefore did nothing
        # at all: the entry landed in the committed half while every read still resolved
        # through the stale local copy, and `vault add` printed the old item name back
        # under a success line because it read the merged view (ADR 0013).
        local = load_local()
        previous = local["vaults"].get(name)
        keep = {k: v for k, v in ((previous or {}).get("config") or {}).items()
                if k in LOCAL_ONLY_KEYS}
        keep.update(local_cfg)
        if keep:
            local["vaults"][name] = {"config": keep}
            save_registry(local)
        elif previous is not None:
            # Nothing local-only to preserve, so the entry goes rather than lingering as
            # an empty shadow waiting to shadow the next change.
            del local["vaults"][name]
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
