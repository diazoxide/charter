"""The repo inventory: model, classification, and JSON load/save.

``inventory/repos.json`` is the durable source of truth for what exists in the
group. It is tracked in git and stays complete even when zero repos are cloned.

A control plane may span several forges (``[[forge]]`` blocks in ``charter.toml``);
:func:`merge` combines their per-forge repo lists into one inventory, keyed by bare
name, and :func:`find` accepts an optional ``<forge>:`` qualifier to disambiguate a
name two forges both expose. See ``charter/forge/registry.py`` for how a forge block
resolves to a backend.
"""

from __future__ import annotations

import json
import re

from . import config


def classify_kind(name: str) -> str:
    """Coarse role inferred from the repo name (descriptive metadata)."""
    n = name.lower()
    if n.endswith("-workspace"):
        return "workspace"
    if n.endswith("-docs"):
        return "docs"
    if n.endswith("-frontend") or "-ui-" in n or n.endswith("-ui"):
        return "frontend"
    if n.endswith("-service") or n.endswith("-services") or n.endswith("-engine"):
        return "service"
    if n.endswith("-api") or "gateway" in n:
        return "api"
    if n.endswith("-core"):
        return "core"
    return "app"


def classify_stack(files) -> str:
    """Detect the primary build stack from root-level file names."""
    fs = set(files)
    if "nx.json" in fs:
        return "nx"
    if "pom.xml" in fs or ".mvn" in fs:
        return "java-maven"
    if {"build.gradle", "build.gradle.kts", "settings.gradle", "settings.gradle.kts"} & fs:
        return "java-gradle"
    if "go.mod" in fs:
        return "go"
    if "Cargo.toml" in fs:
        return "rust"
    if {"pyproject.toml", "requirements.txt", "Pipfile", "setup.py"} & fs:
        return "python"
    if "pnpm-workspace.yaml" in fs:
        return "node-monorepo"
    if "package.json" in fs:
        return "node"
    if "composer.json" in fs:
        return "php"
    if "Gemfile" in fs:
        return "ruby"
    if {"Chart.yaml", "helmfile.yaml"} & fs:
        return "helm"
    if any(f.endswith(".tf") for f in files):
        return "terraform"
    if "Dockerfile" in fs:
        return "docker"
    return "unknown"


def load() -> dict:
    """Load the inventory document, or an empty skeleton if none exists."""
    if not config.INVENTORY.exists():
        return {"group": config.GROUP, "count": 0, "repos": []}
    return json.loads(config.INVENTORY.read_text())


#: Marks a record charter derived rather than discovered. Its presence is what keeps a
#: synthetic entry out of the file `save` writes, and what lets a listing say "known
#: without discover" instead of implying a forge query found it.
PLANE_SOURCE = "plane"


def _root_default_branch() -> str:
    """The plane root's default branch as best charter can tell, or ``""``."""
    from . import util
    ref = util.run(["git", "-C", str(config.ROOT), "symbolic-ref", "--quiet", "--short",
                    "refs/remotes/origin/HEAD"], check=False)
    head = (ref.stdout or "").strip()
    if ref.returncode == 0 and head:
        return head.split("/", 1)[1] if head.startswith("origin/") else head
    cur = util.run(["git", "-C", str(config.ROOT), "symbolic-ref", "--quiet", "--short",
                    "HEAD"], check=False)
    return (cur.stdout or "").strip() if cur.returncode == 0 else ""


def plane_repo() -> dict | None:
    """The **plane repo** — the forge repo the plane root is a checkout of — or None.

    Not the **root tree**, which is the same code seen from the other end: the local
    directory nobody works in (docs/adr/0008). This is the remote you clone *from*.

    It exists because removing the embedded plane shape took away how a solo user reached
    their own code. The plane root used to BE the working tree; now work happens in a
    workspace clone, and the only route charter offered to one was an inventory built by
    enumerating a whole forge owner — on a personal account, dozens of repos queried and
    written to a tracked file to surface the one that mattered. The root's own ``origin``
    already knows the answer, so ``discover`` becomes optional rather than a prerequisite.

    ``None`` whenever charter cannot answer honestly, and each case is a deliberate
    silence rather than a fallback:

    * the root is not a git repo — ``charter init`` in an empty directory, the README's
      own 60-second path;
    * it has no ``origin`` — a repo with no remote cannot be cloned *from* in any useful
      sense, and a workspace clone of it would have no upstream to push to;
    * ``origin``'s host matches no declared forge — guessing the first one would build a
      plausible, wrong clone URL and fail later with a confusing error, where silence
      fails here with an honest one.

    Carries no ``stack`` or ``kind``: those come from probing a repo's file list, and
    faking them would put a wrong answer in a column that renders a missing one as ``?``.
    """
    try:
        from . import util
        from .forge import registry

        p = util.run(["git", "-C", str(config.ROOT), "remote", "get-url", "origin"],
                     check=False)
        origin = (p.stdout or "").strip()
        if p.returncode != 0 or not origin:
            return None

        forge = registry.resolve_host(origin, config.ROOT)
        if forge is None:
            return None

        # `<owner>/<name>`, from either URL form. Trailing `.git` and any `scheme://host`
        # or `git@host:` prefix removed; what is left is the path the forge names it by.
        path = re.sub(r"^[a-z+]+://[^/]+/", "", origin)
        path = re.sub(r"^[^@]+@[^:]+:", "", path)
        path = path.rstrip("/").removesuffix(".git")
        if not path or "/" not in path:
            return None

        return {
            "name": path.rsplit("/", 1)[1],
            # Knowable, unlike stack — and `cmd_clone` announces it. `origin/HEAD` is the
            # remote's own answer where it exists; a plane `git init`-ed and given a
            # remote by hand never has one, so the checked-out branch stands in.
            "default_branch": _root_default_branch(),
            "path_with_namespace": path,
            "forge": forge.kind,
            # One of these is what `commands._https_url` needs. Recording the origin as
            # given lets that function apply THIS forge's own ssh→https rewrite, rather
            # than this module second-guessing a rule that already exists.
            # WITHOUT the `.git` suffix: a forge's `web_url` is its HTML page, and
            # `commands._https_url` appends `.git` itself. Passing the origin through
            # verbatim produced `…/charter.git.git`, which clones nothing.
            "web_url": origin.removesuffix(".git") if origin.startswith("https://") else "",
            "ssh_url": "" if origin.startswith("https://") else origin,
            "description": "",
            "topics": [],
            "source": PLANE_SOURCE,
        }
    except Exception:
        # Never raises: this sits under `repos()`, which the status line, `clone` and
        # `doctor` all call. A plane whose git is broken should lose this convenience,
        # not every listing that depends on it.
        return None


def repos(doc: dict | None = None) -> list:
    """Every repo this plane can clone — the inventory, plus the plane repo.

    The plane repo is contributed here rather than in `clone` so that `clone`, `status`
    and the status line cannot disagree about what is available. Splitting that decision
    is what once left the root tree rendered from one list and refreshed from another.

    A discovered record always wins a duplicate: it carries ``stack``, ``kind``,
    ``description`` and an ``id`` that a derived one cannot know. Matched on
    ``path_with_namespace``, never the bare name — two repos called ``api`` under
    different owners are different repos, which is the distinction that field exists for.

    An explicit ``exclude`` still wins. It is something the user wrote down about their
    own plane, and overriding a written instruction to be helpful is worse than the
    inconvenience it saves.
    """
    listed = (doc or load()).get("repos", [])
    own = plane_repo()
    if own is None:
        return listed
    if own["name"] in config.EXCLUDE or own["path_with_namespace"] in config.EXCLUDE:
        return listed
    if any(r.get("path_with_namespace") == own["path_with_namespace"] for r in listed):
        return listed
    return [own, *listed]


def merge(batches: list[list[dict]]) -> list[dict]:
    """Combine per-forge repo lists into one inventory.

    A repo is exposed under a BARE NAME (the final path segment) so every existing
    command, doc and habit keeps working — but IDENTITY, for deciding whether two
    sightings are "the same repo" or a genuine collision, is ``(forge, path_with_
    namespace)`` — never the bare name alone (the pre-fix key) and never "bare name +
    a forge-only equality check" (the pre-fix collision test).

    Two records with the SAME identity (the exact same project, on the exact same
    forge) are genuinely the same repo — the second sighting is a dedupe (e.g. an
    overlapping ``include_subgroups=true`` sweep re-listing a project), not an error.

    Two records that share a bare name but have DIFFERENT identities are a genuine
    collision and are REFUSED rather than resolved by guessing — the on-disk workspace
    path is derived from the bare name, so silently picking one would let two different
    repos clone over each other. This is reachable in normal use: GitLab
    ``include_subgroups=true`` means ``acme/team-a/api`` and ``acme/team-b/api`` both
    have bare name ``api`` (same forge, different namespace); two ``[[forge]]`` blocks
    of the same kind (two GitHub orgs) hit the identical shape; and two different
    forges sharing a bare name (the original cross-forge case) still collides too.
    """
    from .forge.registry import CollisionError
    by_identity: dict[tuple, dict] = {}
    owner_of_name: dict[str, tuple] = {}
    for batch in batches:
        for r in batch:
            name = r["name"]
            identity = (r.get("forge"), r["path_with_namespace"])
            prev_identity = owner_of_name.get(name)
            if prev_identity is not None and prev_identity != identity:
                prev = by_identity[prev_identity]
                if prev.get("forge") == r.get("forge"):
                    # Same forge, different namespace — a forge-qualifier (`forge:name`)
                    # can't disambiguate two repos that are already on the same forge, so
                    # the only actionable fix is excluding one in charter.toml.
                    raise CollisionError(
                        f"{r.get('forge')} exposes two different repos both named "
                        f"{name!r}: {prev['path_with_namespace']!r} and "
                        f"{r['path_with_namespace']!r}. There's no bare-name qualifier "
                        f"that can tell two same-forge repos apart — exclude one via "
                        f"that `[[forge]]` block's `exclude` in charter.toml.")
                raise CollisionError(
                    f"both {prev.get('forge')} ({prev['path_with_namespace']!r}) and "
                    f"{r.get('forge')} ({r['path_with_namespace']!r}) expose a repo "
                    f"named {name!r}. Qualify it — e.g. `{r.get('forge')}:{name}` — "
                    f"or exclude one in charter.toml.")
            owner_of_name[name] = identity
            by_identity[identity] = r
    return sorted(by_identity.values(), key=lambda r: r["name"])


def find(repos: list, name_or_path: str):
    """Look a repo up by short name, full ``path_with_namespace``, or a
    ``<forge>:<name>``-qualified name for disambiguating a cross-forge collision.

    Takes the caller's repo list explicitly (rather than the whole inventory doc) so it
    composes with :func:`merge` — both operate on plain ``list[dict]``.

    The known-kinds check is a deferred import of ``registry.KINDS`` rather than a
    duplicated literal: nothing under ``charter.forge`` imports back into ``inventory``
    or ``config`` (they only reach ``charter.util`` and stdlib), so there is no import
    cycle to dodge — a plain (if deferred, to keep this module cheap to import before
    any forge backend is needed) import is all that's required.
    """
    from .forge import registry
    kind, sep, bare = (name_or_path or "").partition(":")
    if sep and kind in registry.KINDS:
        for r in repos:
            if r.get("forge") == kind and (r["name"] == bare
                                           or r["path_with_namespace"] == bare):
                return r
        return None
    for r in repos:
        if r["name"] == name_or_path or r["path_with_namespace"] == name_or_path:
            return r
    return None


def save(repo_list: list) -> dict:
    """Write the inventory, sorted stably so diffs reflect real changes only.

    Deliberately carries no generated-at timestamp: this file is tracked, and a
    volatile timestamp would churn git history on every discover.
    """
    # A derived record must never land here. This file's own note tells the next reader
    # to regenerate it with `discover` and not to hand-edit it, so a synthetic entry
    # written into it becomes a permanent fake that no `discover` removes. Today no
    # caller does `save(repos())` — the strip makes that safe by design rather than by
    # nobody having tried it yet.
    repo_list = [r for r in repo_list if r.get("source") != PLANE_SOURCE]
    repo_list = sorted(repo_list, key=lambda r: r["name"])
    doc = {
        "group": config.GROUP,
        "count": len(repo_list),
        "note": (
            f"Source of truth for repos in the {config.GROUP} group. "
            "Regenerate with `charter discover`; do not hand-edit."
        ),
        "repos": repo_list,
    }
    config.INVENTORY.parent.mkdir(parents=True, exist_ok=True)
    config.INVENTORY.write_text(json.dumps(doc, indent=2, ensure_ascii=False) + "\n")
    return doc
