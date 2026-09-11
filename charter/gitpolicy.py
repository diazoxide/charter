"""**Golden rule 0: one credential — PER FORGE.** Every git operation in a control plane —
and in every repo clone under it — authenticates with *that repo's own forge's* token over
HTTPS. No SSH keys, no 1Password agent, no commit signing. One credential per forge (not one
credential, full stop — a GitLab clone uses ``glab``'s token, a GitHub clone uses ``gh``'s),
so an agent never stalls on a key prompt, a signer hang, or a missing SSH config, on *any*
forge the control plane spans.

**SCOPE — the control plane and its clones, nothing outside.** Everything here is written
with ``git config --local``, so it applies to the control-plane repo and the clones under
``workspaces/``, and **never** to a developer's global/system git config. A developer's own
preferences (e.g. a global ``commit.gpgsign=true``) keep working everywhere else on their
machine; inside the control plane the local value simply wins, which is exactly what stops
an agent stalling on a signer prompt. Do **not** widen this to ``--global`` / an ``includeIf``
stanza — tests assert the boundary (`test_git_policy.py`).

This module makes that **mechanically true** rather than merely documented, by writing three
things into a repo's *local* git config (never global — we don't touch a developer's machine),
each derived from the ONE forge that repo actually talks to (:func:`forge_for`, resolved from
its ``origin`` remote):

1. ``credential.helper = !<cli> auth git-credential`` — git asks *that forge's* CLI for the
   token (:func:`policy_for`).
2. ``commit.gpgsign=false`` / ``tag.gpgsign=false`` — signing can't hang the agent, on any
   forge (:func:`policy_for`).
3. ``url.<https-base>.insteadOf`` for both of that forge's SSH forms — so even a repo whose
   *remote is an SSH URL* transparently transports over HTTPS+token (:func:`insteadof_for`).
   This is what makes a plain ``git push`` typed by any agent (or any persona sub-agent) work
   without SSH, whichever forge the repo lives on.

With (3) in place the rule holds even for clones the control plane didn't create; the
PreToolUse guard in :mod:`charter.hooks` then blocks the deliberate bypasses (explicit SSH
remotes, ``GIT_SSH_COMMAND``, ``-S``/``--gpg-sign``) — widened the same way, across every
forge host the control plane knows about (see ``hooks._known_forges``).
"""

from __future__ import annotations

from pathlib import Path

from . import config, util
from .forge.gitlab import GitLabForge


def policy_for(forge) -> dict[str, str]:
    """The `key = value` settings a repo on *forge* gets. One credential per forge: the
    forge's own CLI holds the token, and signing is off so no signer prompt can hang an
    agent."""
    return {
        "credential.helper": forge.credential_helper(),
        "commit.gpgsign": "false",
        "tag.gpgsign": "false",
    }


def insteadof_for(forge) -> tuple[str, tuple[str, ...]]:
    """`(https_base, ssh_forms)` so even a repo whose remote is an SSH URL transports
    over HTTPS with a token."""
    return forge.insteadof()


#: Back-compat defaults, kept as *derived* values (never a separately-maintained literal) —
#: GitLab was the only forge before this module went per-forge, and stays the fallback a
#: fresh/originless repo gets (see `forge_for`). Existing callers/tests that read these
#: module-level names keep working unchanged; `check`/`apply` themselves resolve per-repo.
POLICY: dict[str, str] = policy_for(GitLabForge())
HTTPS_BASE, SSH_FORMS = insteadof_for(GitLabForge())
_URL_KEY = f"url.{HTTPS_BASE}.insteadOf"


def _git(args, cwd):
    return util.run(["git", *args], cwd=cwd, check=False)


def is_git_repo(path: Path) -> bool:
    return (Path(path) / ".git").exists()


def _local_config(repo: Path) -> dict[str, list[str]]:
    """The repo's LOCAL config as ``{key: [values]}`` in **one** git call (git parses its own
    format, so this stays correct) — cheap enough to check every clone at preflight."""
    p = _git(["config", "--local", "--list", "-z"], cwd=repo)
    out: dict[str, list[str]] = {}
    if p.returncode != 0:
        return out
    for rec in (p.stdout or "").split("\0"):
        if not rec:
            continue
        key, _, val = rec.partition("\n")
        out.setdefault(key.strip().lower(), []).append(val)
    return out


def _get_all(repo: Path, key: str) -> list[str]:
    return _local_config(repo).get(key.lower(), [])


#: `check` returns exactly this (and only this) when a repo's origin host is neither a
#: default forge host nor one DECLARED in the active control plane's ``charter.toml`` —
#: genuinely unrecognised. Distinguished from ordinary drift (`key != value`) because it
#: can't be fixed by `apply`: there's no forge to apply a policy FOR. Before this existed,
#: an unrecognised host silently fell back to GitLab's policy, so `check` returned `[]`
#: (compliant) for a repo it had never actually verified — a false green that is worse
#: than no check at all, because it still LOOKS like the golden rule is enforced.
UNMANAGED_FORGE = ("forge unknown — origin host is not a default forge host and isn't "
                   "declared in charter.toml; the token-only policy can't be verified or "
                   "applied. Declare it under [[forge]] (host = \"…\") to bring it under "
                   "management.")


def forge_for(repo: Path) -> object | None:
    """Which forge governs *repo*'s policy — inferred from its ``origin`` remote.

    Recognises two things: each registered kind's DEFAULT host (``gitlab.com``,
    ``github.com``), and any self-hosted host the ACTIVE control plane DECLARES in its
    own ``charter.toml`` (``registry.resolve_host`` — the same host set the SSH guard
    uses, see ``hooks._known_forges``). This is what makes `check`/`apply` per-forge: a
    GitHub clone gets ``gh``'s credential helper and github.com's SSH→HTTPS rewrite, a
    self-hosted GitLab clone gets ``glab``'s and ITS OWN host's rewrite — never
    gitlab.com's, which would leave its real SSH remote unrewritten.

    Returns ``None`` when *repo* has an origin whose host is genuinely unrecognised —
    not a default, not declared. Callers MUST treat that as unmanaged, never silently
    fall back to another forge's policy (see `UNMANAGED_FORGE`); "do not simply widen
    the fallback" is the whole point — a wrong-but-plausible-looking policy is worse than
    an honest "can't tell".

    A repo with NO origin at all (a fresh ``git init``, nothing pushed yet) is a
    different case — there's no host to be wrong about — and keeps the pre-multi-forge
    default (GitLab), unchanged from before multi-forge support existed."""
    from .forge import registry
    url = _git(["remote", "get-url", "origin"], cwd=repo).stdout.strip()
    if not url:
        return GitLabForge()
    return registry.resolve_host(url, config.ROOT)


def check(repo: Path, cfg: dict[str, list[str]] | None = None) -> list[str]:
    """Policy settings that are MISSING/wrong in ``repo``'s local config (empty = compliant),
    against *that repo's own forge* (see `forge_for`). Pass ``cfg`` to reuse an
    already-read config (avoids a second git call).

    An unrecognised forge (`forge_for` → ``None``) returns ``[UNMANAGED_FORGE]`` —
    never ``[]``. Reporting compliant for a repo whose policy was never actually checked
    is the false-green bug this exists to prevent."""
    forge = forge_for(repo)
    if forge is None:
        return [UNMANAGED_FORGE]
    cfg = _local_config(repo) if cfg is None else cfg
    policy = policy_for(forge)
    https_base, ssh_forms = insteadof_for(forge)
    url_key = f"url.{https_base}.insteadOf"
    drift = []
    for key, want in policy.items():
        got = cfg.get(key.lower(), [])
        if not got or got[-1] != want:
            drift.append(f"{key} != {want}")
    have = set(cfg.get(url_key.lower(), []))
    for ssh in ssh_forms:
        if ssh not in have:
            drift.append(f"{url_key} missing {ssh}")
    return drift


def non_compliant(root: Path, workspaces_dir: Path) -> list[Path]:
    """Every repo in scope (control plane + clones) whose local config isn't token-only."""
    return [r for r in repos(root, workspaces_dir) if check(r)]


def apply(repo: Path) -> list[str]:
    """Write the token-only policy for *repo's own forge* into its **local** config
    (idempotent). Returns the settings changed. Never touches global/system config.

    A no-op (returns ``[]``) when the forge is unrecognised (`forge_for` → ``None``) —
    there's nothing safe to apply for a host we can't identify; guessing would just
    reintroduce the false-green bug one layer down."""
    repo = Path(repo)
    if not is_git_repo(repo):
        return []
    forge = forge_for(repo)
    if forge is None:
        return []
    policy = policy_for(forge)
    https_base, ssh_forms = insteadof_for(forge)
    url_key = f"url.{https_base}.insteadOf"
    changed = []
    for key, want in policy.items():
        got = _get_all(repo, key)
        if not got or got[-1] != want:
            _git(["config", "--local", key, want], cwd=repo)
            changed.append(f"{key}={want}")
    have = set(_get_all(repo, url_key))
    for ssh in ssh_forms:
        if ssh not in have:
            _git(["config", "--local", "--add", url_key, ssh], cwd=repo)
            changed.append(f"rewrite {ssh} → {https_base}")
    return changed


def repos(root: Path, workspaces_dir: Path) -> list[Path]:
    """The control plane itself plus every repo clone under ``workspaces/<ws>/<repo>``."""
    return scan(root, workspaces_dir)[0]


def scan(root: Path, workspaces_dir: Path) -> tuple[list[Path], list[Path]]:
    """:func:`repos`, and beside it every directory under ``workspaces/<ws>/`` whose ``.git``
    charter cannot check.

    #942 final review: `Path.exists` RAISED for one on 3.11–3.13 — a workspace's own `refs/` at
    mode 000 took `charter doctor` down — and answered False on 3.14, leaving what may be a clone
    out of the count without a word. `workspace._exists` tells those apart, and `doctor`'s
    `check_ssh` names the second kind (ADR 0009)."""
    from .workspace import _exists

    out = [Path(root)] if is_git_repo(root) else []
    unseen: list[Path] = []
    if Path(workspaces_dir).exists():
        for ws in sorted(Path(workspaces_dir).iterdir()):
            if not ws.is_dir():
                continue
            for clone in sorted(ws.iterdir()):
                there = _exists(clone / ".git", follow=True)
                if there:
                    out.append(clone)
                elif there is None:
                    unseen.append(clone)
    return out, unseen
