"""Command implementations behind the ``charter`` CLI subcommands."""

from __future__ import annotations

import concurrent.futures
import json
import re
from pathlib import Path

from . import config, doctor, inventory, render, util, workspace, worktree
from . import root as _root
from .forge import ForgeError
from .forge.gitlab import GitLabForge


def _git(args, cwd=None):
    """Run git without raising, so callers can branch on the return code."""
    return util.run(["git", *args], cwd=cwd, check=False)


# Route git auth through THAT REPO'S OWN FORGE's token over HTTPS — no SSH keys, no
# 1Password agent (which is where all the signing/permission pain comes from). One
# credential PER FORGE: `!glab auth git-credential` for GitLab, `!gh auth git-credential`
# for GitHub, each forge's own git credential helper — git appends the get/store/erase
# operation. `_cred_flag` resolves this per call (never a single hardcoded forge), so a
# GitHub-hosted clone — like this control plane itself — authenticates correctly instead
# of silently being handed GitLab's helper.
def _cred_flag(forge) -> list[str]:
    """``-c credential.helper=<forge's>`` — makes ONE git invocation use *that forge's*
    token-holding CLI, regardless of what (if anything) local config already has. Belt
    and braces for the very first ``git clone``, before `gitpolicy.apply` has had a
    chance to write the repo's own local config."""
    return ["-c", f"credential.helper={forge.credential_helper()}"]


def _https_url(r: dict) -> str:
    """HTTPS clone URL for a repo (so cloning uses that repo's own forge's token, never
    SSH) — rewritten via THAT forge's own SSH forms (`registry.for_repo`, from the
    inventory's ``forge`` stamp — see ``charter/forge/registry.py``), not a hardcoded
    gitlab.com prefix, so a GitHub-recorded repo's SSH ``ssh_url`` rewrites correctly too."""
    web = (r.get("web_url") or "").rstrip("/")
    if web.startswith("https://"):
        return web + ".git"
    ssh = r.get("ssh_url") or ""
    from .forge import registry
    https_base, ssh_forms = registry.for_repo(r).insteadof()
    for prefix in ssh_forms:
        if ssh.startswith(prefix):
            return https_base + ssh[len(prefix):]
    return ssh


def _origin_https(root) -> str | None:
    """The control plane's own ``origin`` as an HTTPS URL (rewriting an SSH one via ITS
    forge's own rewrite rule — ``registry.resolve_host`` + ``insteadof()``), or ``None``
    when there's no origin yet, or its host isn't a forge this charter knows about — a
    DEFAULT host (gitlab.com/github.com) or one DECLARED in this control plane's own
    ``charter.toml`` (see ``gitpolicy.forge_for``, which resolves the same way). An
    unrecognised host deliberately returns ``None`` rather than guessing: `commit_push`
    then warns and skips the push instead of silently trying (and failing) against the
    wrong forge."""
    url = _git(["remote", "get-url", "origin"], cwd=root).stdout.strip()
    if not url:
        return None
    from .forge import registry
    forge = registry.resolve_host(url, config.ROOT)
    if forge is None:
        return None
    https_base, ssh_forms = forge.insteadof()
    if url.startswith(https_base):
        return url
    for prefix in ssh_forms:
        if url.startswith(prefix):
            return https_base + url[len(prefix):]
    return None


# --------------------------------------------------------------------------- #
# discover                                                                     #
# --------------------------------------------------------------------------- #
def _build_repo(forge, p: dict, no_probe: bool) -> tuple[dict, bool]:
    """``(record, probe_failed)``. ``probe_failed`` is FINDING I5's fix: it distinguishes
    "the stack probe itself errored" (network hiccup, expired token, a GitHub secondary
    rate limit — likely under `_build_batch`'s concurrency) from "this repo genuinely
    has no recognised root-level stack file". Before this, both looked identical —
    `repo_tree` is the permissive best-effort API, so ANY failure silently became an
    empty file list, and `classify_stack([])` returns "unknown" either way — so a probe
    failure silently rewrote the repo's stack with no way to tell it apart from the
    truth. Uses `repo_tree_strict` (raises on failure) instead of the permissive
    `repo_tree` specifically so this distinction is possible."""
    stack = "unknown"
    probe_failed = False
    if not no_probe:
        try:
            files = forge.repo_tree_strict(p, p.get("default_branch"))
            stack = inventory.classify_stack(files)
        except ForgeError:
            probe_failed = True
    return {
        "name": p["name"],
        "path_with_namespace": p["path_with_namespace"],
        "ssh_url": p["ssh_url"],
        "default_branch": p.get("default_branch") or "main",
        "kind": inventory.classify_kind(p["name"]),
        "stack": stack,
        "description": (p.get("description") or "").strip(),
        "topics": p.get("topics") or [],
        "web_url": p.get("web_url", ""),
        # Which forge this record came from, so a mixed inventory stays unambiguous
        # once records from several forges are merged (see inventory.merge/find).
        "forge": p.get("forge") or forge.kind,
    }, probe_failed


def _build_batch(forge, projects: list[dict], no_probe: bool) -> tuple[list[dict], int]:
    """``(records, probe_failed_count)`` — see :func:`_build_repo`."""
    if no_probe:
        return [_build_repo(forge, p, no_probe)[0] for p in projects], 0
    with concurrent.futures.ThreadPoolExecutor(max_workers=8) as ex:
        results = list(ex.map(lambda p: _build_repo(forge, p, no_probe), projects))
    return [r for r, _ in results], sum(1 for _, failed in results if failed)


def _forges_to_query(cfg: dict):
    """``[(forge, owner, exclude), …]`` for every declared ``[[forge]]`` block, or a
    single back-compat GitLab default (``config.GROUP``/``config.EXCLUDE``) when the
    control plane declares none — the shape every control plane had before multi-forge
    support existed, and still what a fresh `charter init` produces."""
    from . import instance as _instance
    from .forge import registry

    pairs = registry.forges_for(cfg)
    if pairs:
        return [(forge, owner, _instance.exclude_of(cfg, i))
                for i, (forge, owner) in enumerate(pairs)]
    return [(GitLabForge(), config.GROUP, config.EXCLUDE)]


def cmd_discover(args) -> int:
    from .forge import registry

    try:
        cfg = _instance_load_root()
    except Exception as e:
        raise SystemExit(str(e))

    try:
        # An unknown `kind` in a `[[forge]]` block (e.g. a typo, or a forge charter
        # doesn't support) is a config mistake, not a crash — same "clear error, not a
        # traceback" discipline as the CollisionError handling below.
        to_query = _forges_to_query(cfg)
    except ValueError as e:
        raise SystemExit(str(e))

    batches: list[list[dict]] = []
    probe_failures = 0
    for forge, owner, exclude in to_query:
        util.info(f"Querying {forge.kind} {forge.owner_noun} `{owner}` …")
        try:
            forge.check_auth()
        except ForgeError as e:
            raise SystemExit(str(e))
        try:
            projects = [p for p in forge.list_repos(owner) if p["name"] not in exclude]
        except ForgeError as e:
            raise SystemExit(str(e))
        util.info(
            f"Found {len(projects)} project(s) on {forge.kind}. "
            + ("Skipping stack probe." if args.no_probe else "Probing repo stacks …")
        )
        # Built (and appended) immediately after a successful list_repos, but nothing
        # is saved until every declared forge has succeeded — see the merge/save step
        # below. A forge failing here raises before inventory.save is ever reached, so
        # a partial multi-forge failure can never wipe (or half-write) the inventory,
        # the same discipline the single-forge `_api_strict` split enforces.
        records, failed = _build_batch(forge, projects, args.no_probe)
        probe_failures += failed
        batches.append(records)

    try:
        merged = inventory.merge(batches)
    except registry.CollisionError as e:
        raise SystemExit(str(e))

    before = {r["name"] for r in inventory.repos()}
    doc = inventory.save(merged)
    after = {r["name"] for r in merged}

    rel = config.INVENTORY.relative_to(config.ROOT)
    util.ok(f"Wrote {doc['count']} repos to {rel}")
    # FINDING I5: still SAVE on a probe failure (stack is best-effort descriptive
    # metadata — unlike `list_repos`, a repo missing from the map entirely is
    # unacceptable, which is why THAT failure aborts instead; see F1). But never let it
    # be silent — a network hiccup, an expired token, or a GitHub secondary rate limit
    # (8 repos probe concurrently, exactly what trips it) must not quietly masquerade as
    # "these repos have no recognisable stack".
    if probe_failures:
        util.warn(
            f"⚠ stack probe FAILED for {probe_failures} repo(s) — their `stack` was "
            'written as "unknown" because the probe itself errored (network/auth/a '
            "GitHub secondary rate limit), not because they lack a recognised build "
            "file. Re-run `charter discover` to re-probe; if it keeps failing, check "
            "`charter doctor` (forge auth) or wait out the rate-limit window."
        )
    added, removed = sorted(after - before), sorted(before - after)
    if added:
        util.info("New in group: " + ", ".join(added))
    if removed:
        util.warn("No longer in group: " + ", ".join(removed))

    if not args.no_docs:
        cmd_docs(args)
    return 0


def _instance_load_root() -> dict:
    """Re-parse ``charter.toml`` against the *current* ``config.ROOT`` rather than
    trusting the module-level ``config._cfg`` cached at import time. Necessary for
    ``cmd_discover`` specifically: it's the one command whose forge list must reflect
    whatever ``config.ROOT`` names right now, including in tests that redirect ROOT to
    an isolated tmp dir (``PersonaIso``) after import already happened — reading the
    stale cached ``_cfg`` would silently see the real process's charter.toml (or lack
    of one) instead of the tmp control plane a test just set up."""
    from . import instance as _instance
    return _instance.load(config.ROOT)


# --------------------------------------------------------------------------- #
# docs                                                                         #
# --------------------------------------------------------------------------- #
def cmd_docs(args) -> int:
    doc = inventory.load()
    if not doc.get("repos"):
        util.warn("Inventory is empty — run `charter discover` first.")
        return 1

    config.DOCS_DIR.mkdir(parents=True, exist_ok=True)
    (config.DOCS_DIR / "topology.md").write_text(render.topology_md(doc) + "\n")
    util.ok(f"Generated docs/topology.md ({len(doc['repos'])} repos)")

    if refresh_readme_personas():
        util.ok("Refreshed the persona roster block in README.md")
    return 0


def refresh_readme_personas() -> bool:
    """Rewrite the generated persona-roster block in README.md. Returns True when the
    file actually changed, so callers (and git) stay quiet on a no-op. Best-effort: a
    README without the markers, or an unreadable one, is left exactly as it is."""
    p = config.ROOT / "README.md"
    try:
        cur = p.read_text()
    except OSError:
        return False
    new = render.splice_personas(cur)
    if new is None or new == cur:
        return False
    try:
        p.write_text(new)
    except OSError:
        return False
    return True


# --------------------------------------------------------------------------- #
# clone                                                                        #
# --------------------------------------------------------------------------- #
def cmd_clone(args) -> int:
    doc = inventory.load()
    if not doc.get("repos"):
        util.err("Inventory is empty — run `charter discover` first.")
        return 1

    targets = _resolve_targets(args, doc)
    if not targets:
        util.err("No matching repos. Give one or more repo names/paths from the inventory "
                 "(see them: `charter status`; refresh from GitLab: `charter discover`).")
        return 1

    ws = workspace.resolve(getattr(args, "workspace", None))
    workspace.banner(ws, getattr(args, "workspace", None))
    try:
        wd = workspace.ensure(ws)
    except ValueError as e:
        util.err(str(e))
        return 1

    from .forge import registry

    failures = 0
    for r in targets:
        dest = wd / r["name"]
        if dest.exists():
            util.info(f"{r['name']}: already cloned in '{ws}'")
            _hint_repo_docs(dest, r)
            continue
        forge = registry.for_repo(r)
        util.info(f"Cloning {r['name']} ({r['default_branch']}) into '{ws}' via {forge.cli} (HTTPS) …")
        proc = _git([*_cred_flag(forge), "clone", "--branch", r["default_branch"], _https_url(r), str(dest)])
        if proc.returncode != 0:
            failures += 1
            util.err(
                f"{r['name']}: clone failed — no access, network, or {forge.cli} isn't authed "
                f"(`{forge.cli} auth status`). Skipping.\n" + (proc.stderr or "").strip()
            )
            continue
        # Golden rule 0: every git op from this clone uses ITS FORGE's token over HTTPS —
        # credential helper + signing off + SSH→HTTPS rewrites (see charter/gitpolicy.py).
        from . import gitpolicy
        gitpolicy.apply(dest)
        util.ok(f"{r['name']} → {dest.relative_to(config.ROOT)}")
        _hint_repo_docs(dest, r)
    return 1 if failures else 0


def _spawn_bg_push(root) -> None:
    """Fire a detached background push of HEAD (best-effort) so a slow push never blocks
    the turn — the same mechanism the workspace Stop-hook auto-save uses."""
    import subprocess
    import sys
    try:
        subprocess.Popen([sys.executable, "-m", "charter", "workspace", "_pushbg"],
                         stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL,
                         stdin=subprocess.DEVNULL, start_new_session=True, cwd=str(root))
    except Exception:
        pass


def commit_memory_reactive(paths: list[str], title: str) -> int:
    """**Reactive memory**: how far the just-written memory file(s) travel is declared per
    control plane via ``config.MEMORY_SHARE`` (``charter.instance.SHARE_MODES``), defaulting
    to ``local`` — safe for a control plane a stranger might run, where nothing should reach
    a remote without a human between writing and disclosure:

    - ``local``  — stays on disk only; nothing is committed.
    - ``commit`` — committed locally (scoped + secret-scanned), never pushed.
    - ``push``   — committed locally and pushed in the BACKGROUND — so a memory reaches the
      shared repo the moment it's recorded, without blocking the turn. Best-effort.

    Returns commit_push's rc (0 = committed / nothing to do / posture is local,
    1 = a secret-shaped value was refused)."""
    from . import instance as _instance
    # Re-clamp defensively — see `instance.clamp_share`: `config.MEMORY_SHARE` is always
    # pre-clamped at import time, but this reactive path must not itself rely on that.
    share = _instance.clamp_share(config.MEMORY_SHARE)
    if share == "local":
        return 0
    msg = f"memory: {title}"[:100]
    if share == "commit":
        return commit_push(config.ROOT, ["add", "--", *paths], msg, no_push=True)
    return commit_push(config.ROOT, ["add", "--", *paths], msg, background=True)


def commit_push(root, add_cmd: list, message: str | None,
                sign: bool = False, no_push: bool = False, background: bool = False) -> int:
    """Stage (``add_cmd``) → secret-scan staged memory/refs → commit → push via the
    control plane's OWN FORGE's token (`gitpolicy.forge_for`; rebase-retry on non-ff).
    Shared by `charter save` and `charter workspace save` so the secret-guard + no-SSH
    push path is identical everywhere, on whichever forge the control plane lives on.

    ``add_cmd`` is git's ARGUMENTS, not a command line — `_git` supplies `git` itself.
    One caller passed `["git", "add", …]`, so it ran `git git add`, which fails; the
    staged-nothing check below then took the "Nothing to save" branch and returned 0, and
    the caller printed `✓ committed + pushed`. `charter version bump --push` had therefore
    never committed anything. Asserting the shape here rather than fixing the one caller,
    because the failure was invisible at every layer above it."""
    if add_cmd and add_cmd[0] == "git":
        raise ValueError(f"commit_push(add_cmd=…) takes git's arguments, not a command "
                         f"line — drop the leading 'git' from {add_cmd!r}")
    _git(add_cmd, cwd=root)
    if _git(["diff", "--cached", "--quiet"], cwd=root).returncode == 0:
        util.info("Nothing to save — the control-plane working tree is clean.")
        return 0
    staged = [ln for ln in _git(["diff", "--cached", "--name-only"], cwd=root).stdout.splitlines() if ln.strip()]

    # Secret guard: refuse if a staged memory/ref file looks like it holds a secret.
    from .hooks import _secret_kind
    flagged = []
    for p in staged:
        if "/memory/" in p or "/refs/" in p:
            try:
                kind = _secret_kind((root / p).read_text())
            except OSError:
                kind = None
            if kind:
                flagged.append((p, kind))
    if flagged:
        util.err("Refusing to save — a secret-shaped value in a memory/ref file:")
        for p, k in flagged:
            util.err(f"  {p}  ({k})")
        util.info("Secrets belong in the vault (`charter persona secret set`). Remove it, then retry.")
        return 1

    msg = message or f"charter save: {len(staged)} file(s)"
    # Unsigned by default so the 1Password signer never hangs; --sign to opt in.
    signcfg = [] if sign else ["-c", "commit.gpgsign=false"]
    _git([*signcfg, "commit", "-q", "-m", msg], cwd=root)
    if _git(["diff", "--cached", "--quiet"], cwd=root).returncode != 0:  # signed commit failed
        _git(["commit", "--no-gpg-sign", "-q", "-m", msg], cwd=root)
    short = _git(["rev-parse", "--short", "HEAD"], cwd=root).stdout.strip()
    util.ok(f"Committed {short}: {msg}  ({len(staged)} file(s))")

    if no_push:
        util.info("Skipped push (--no-push).")
        return 0
    branch = _git(["rev-parse", "--abbrev-ref", "HEAD"], cwd=root).stdout.strip()
    https = _origin_https(root)
    if not https:
        util.warn("origin isn't on a forge charter knows (gitlab.com/github.com/…) — "
                  "committed locally; push manually.")
        return 0
    if background:
        _spawn_bg_push(root)
        util.info("→ pushing to the control plane in the background.")
        return 0

    from . import gitpolicy
    forge = gitpolicy.forge_for(root)
    cred = _cred_flag(forge)

    def push():
        return _git([*cred, "push", https, f"HEAD:{branch}"], cwd=root)

    p = push()
    if p.returncode != 0 and any(s in (p.stderr or "") for s in ("fetch first", "non-fast-forward", "rejected")):
        util.info("remote moved — fetching + rebasing, then retrying …")
        _git([*cred, "fetch", https, branch], cwd=root)
        if _git(["rebase", "FETCH_HEAD"], cwd=root).returncode != 0:
            _git(["rebase", "--abort"], cwd=root)
            util.warn("Committed locally, but rebase hit a conflict — resolve manually, then `charter save`.")
            return 0
        p = push()
    if p.returncode == 0:
        _git(["update-ref", f"refs/remotes/origin/{branch}", "HEAD"], cwd=root)  # sync tracking
        util.ok(f"Pushed {branch} via {forge.cli} (HTTPS token — no SSH, no 1Password).")
    else:
        util.warn(f"Committed, but the {forge.cli} push failed:")
        for ln in (p.stderr or p.stdout or "").splitlines()[-4:]:
            util.warn("  " + ln)
        util.info(f"Check `{forge.cli} auth status`.")
    return 0


def cmd_save(args) -> int:
    """Commit + push the CONTROL PLANE's own changes in one step, via ITS OWN FORGE's
    HTTPS token — no SSH keys, no 1Password signing hang. (For a *clone's* changes, work
    in a repo-rooted session; this is only the control plane's orchestration files.)"""
    return commit_push(config.ROOT, ["add", "-A"], args.message,
                       sign=getattr(args, "sign", False), no_push=getattr(args, "no_push", False))


def _resolve_targets(args, doc) -> list:
    out = []
    all_repos = inventory.repos(doc)
    for name in args.repos:
        r = inventory.find(all_repos, name)
        if r:
            out.append(r)
        else:
            util.warn(f"Unknown repo (not in inventory): {name} — check the name (`charter status`) "
                      "or `charter discover` if it's new to the group.")
    return out


def _hint_repo_docs(dest: Path, r: dict) -> None:
    for fname in ("CLAUDE.md", "AGENTS.md", "README.md"):
        if (dest / fname).exists():
            util.info(f"  ↳ {r['name']} ships its own {fname} — read it before working there.")
            return


# --------------------------------------------------------------------------- #
# sync                                                                         #
# --------------------------------------------------------------------------- #
def cmd_sync(args) -> int:
    if getattr(args, "all", False):
        names = workspace.list_workspaces()
        targets = [(n, d) for n in names for d in workspace.clones(n)]
        scope = f"all workspaces ({len(names)})"
    else:
        ws = workspace.resolve(getattr(args, "workspace", None))
        workspace.banner(ws, getattr(args, "workspace", None))
        targets = [(ws, d) for d in workspace.clones(ws)]
        scope = f"workspace '{ws}'"
    if not targets:
        util.warn(f"No cloned repos to sync in {scope}.")
        return 0
    for name, d in targets:
        _sync_one(d, name)
    return 0


def _sync_one(d: Path, ws: str) -> None:
    label = f"{ws}/{d.name}"
    if _git(["status", "--porcelain"], cwd=d).stdout.strip():
        util.warn(f"{label}: uncommitted changes — skipping (your work is left untouched).")
        return
    branch = _git(["rev-parse", "--abbrev-ref", "HEAD"], cwd=d).stdout.strip()
    if _git(["fetch", "--prune"], cwd=d).returncode != 0:
        util.err(f"{label}: fetch failed (access or network) — skipping.")
        return
    ff = _git(["merge", "--ff-only", f"origin/{branch}"], cwd=d)
    if ff.returncode == 0:
        util.ok(f"{label}: up to date on {branch}")
    else:
        util.warn(f"{label}: {branch} won't fast-forward (diverged/local commits) — left as-is.")


# --------------------------------------------------------------------------- #
# status                                                                       #
# --------------------------------------------------------------------------- #
def cmd_status(args) -> int:
    doc = inventory.load()
    inv_by_name = {r["name"]: r for r in doc.get("repos", [])}
    all_ws = workspace.list_workspaces()
    explicit = getattr(args, "workspace", None)
    active = workspace.resolve(explicit)

    print(f"{config.GROUP}: {len(inv_by_name)} repos in inventory · "
          f"{len(all_ws)} workspace(s) · active: {active} (via {workspace.source(explicit)})\n")

    if all_ws:
        for n in all_ws:
            mark = "*" if n == active else " "
            print(f"  {mark} {n}  ({len(workspace.clones(n))} cloned)")
        print()

    which = all_ws if getattr(args, "all", False) else [active]
    for ws in which:
        _status_for_workspace(ws, inv_by_name, active)

    legacy = workspace.legacy_flat_clones()
    if legacy:
        util.warn(
            "Legacy clones sit directly under workspaces/ (pre-workspace layout): "
            + ", ".join(d.name for d in legacy)
            + f". Move them into workspaces/{config.DEFAULT_WORKSPACE}/ or re-clone with a workspace."
        )
    print(f"({len(inv_by_name)} repos available to clone — see docs/topology.md)")
    return 0


def _status_for_workspace(ws: str, inv_by_name: dict, active: str) -> None:
    clones = {d.name: d for d in workspace.clones(ws)}
    marker = " (active)" if ws == active else ""
    print(f"— workspace: {ws}{marker} · {len(clones)} cloned —")
    if not clones:
        print(f"  (empty; `charter clone <repo> --workspace {ws}` to populate)\n")
        return
    fmt = "  {:<38} {:<12} {}"
    print(fmt.format("REPO", "STACK", "BRANCH / NOTE"))
    for name, d in sorted(clones.items()):
        stack = inv_by_name.get(name, {}).get("stack", "?")
        print(fmt.format(name, stack, _clone_note(d)))
    print()


def _clone_note(d: Path) -> str:
    branch = _git(["rev-parse", "--abbrev-ref", "HEAD"], cwd=d).stdout.strip()
    dirty = "dirty" if _git(["status", "--porcelain"], cwd=d).stdout.strip() else "clean"
    return f"{branch} · {dirty}"


# --------------------------------------------------------------------------- #
# gl-refresh (populate the status-line's forge state: open change + last CI)  #
# --------------------------------------------------------------------------- #
def cmd_gl_refresh(args) -> int:
    from . import glstate

    ws = workspace.resolve(getattr(args, "workspace", None))
    # `repo_trees`, not `clones`: the status line draws the plane's own root tree as a
    # repo row, and this is what fills that row's CI and change columns. A monorepo plane
    # has NO clones, so the old list was empty and this command returned early — the one
    # shape where every repo it should refresh is the one it skipped.
    dirs = workspace.repo_trees(ws)
    # Worktrees carry their own branch, so they carry their own pipeline and their own
    # open change; the status line gives them full rows when a workspace holds a single
    # repo. Refreshed here so those columns have something to show.
    dirs += [w for d in dirs for w in worktree.dirs_for(ws, d.name)]
    if not dirs:
        util.info(f"No repos in workspace '{ws}'.")
        return 0
    cache = glstate.refresh(dirs)
    util.ok(f"Refreshed forge state for {len(dirs)} tree(s) in '{ws}'.")
    for d in dirs:
        ent = cache.get(str(d), {})
        bits = []
        if ent.get("change"):
            bits.append(f"{ent.get('sigil') or '!'}{ent['change']}")
        if ent.get("ci"):
            bits.append(f"pipeline:{ent['ci']}")
        if bits:
            util.info(f"  {d.name}: {' · '.join(bits)}")
    return 0


# --------------------------------------------------------------------------- #
# init — a control plane from nothing. Without this, `charter` is unusable by  #
# a stranger AND actively misleading: `root.py`'s own error already says "run  #
# `charter init`" (see `root._explain`), and outside a control plane every     #
# other command silently adopts the cwd (`config.ROOT` falls back there — see  #
# `root.find_root_or_cwd`) and would scatter `workspaces/`/`.charter/` into it. #
# Same additive discipline as `cmd_reinit` below (create only what's absent,   #
# never touch what exists, name — never delete/rename — anything blocking a   #
# path), widened to also cover charter.toml itself, `.gitignore`, and the     #
# Claude Code status line. Deliberately does NOT gate on                      #
# `config.HAS_CONTROL_PLANE` — flipping that flag from false to true is the   #
# whole point.                                                                 #
# --------------------------------------------------------------------------- #
def _toml_str(s: str) -> str:
    """A TOML basic-string literal for ``s``. ``tomllib`` (stdlib) is read-only, and
    charter is stdlib-only at runtime — no TOML writer to reach for — so this leans on
    ``json.dumps``: JSON's string escaping (quotes, backslashes, control chars) is a
    faithful subset of TOML's for the plain owner/host names this ever has to render."""
    return json.dumps(s)


def _render_charter_toml(forge_kind: str, owner: str, host: str | None,
                         shape: str = "fleet") -> str:
    from . import instance as _instance

    lines = [f"schema = {_instance.SCHEMA}"]
    # Written only when it is not the default, so a fleet plane's charter.toml stays the
    # same minimal file it has always been and nobody has to learn a key they don't need.
    if shape == "embedded":
        lines += ["",
                  "# charter is installed inside the one codebase it serves. Nothing to",
                  "# clone: the tree you edit is this directory, and a workspace holds",
                  "# worktrees of it. See docs/control-plane.md.",
                  "[plane]", 'shape = "embedded"']
    lines += ["", "[[forge]]", f"kind = {_toml_str(forge_kind)}"]
    if owner:
        lines.append(f"owner = {_toml_str(owner)}")
    if host:
        lines.append(f"host = {_toml_str(host)}")
    # Explicit default, not just an absent key: a fresh control plane must never
    # publish agent-written notes by accident (see `instance.share_of`'s "local" default —
    # this just makes that default visible in the file a stranger will actually open).
    lines += ["", "[memory]", 'share = "local"', ""]
    return "\n".join(lines)


#: Baseline `.gitignore` a fresh control plane needs: private per-task workspaces (the
#: `!/workspaces/.gitkeep` line is the exact anchor `workspace.set_live` looks for to
#: insert its managed live-workspace block later — see charter/workspace.py) and the
#: per-developer secrets home. Mirrors what `charter reinit`'s sibling, a real control
#: plane's own `.gitignore`, already looks like.
_GITIGNORE_BASELINE = """\
# Per-task workspaces (workspaces/<name>/). LOCAL by default = fully private (clones,
# memory, manifest all ignored). Made LIVE via `charter workspace live <name>` un-ignores
# its workspace.json + memory/ in a managed block here (see charter/workspace.py).
/workspaces/*/*
!/workspaces/.gitkeep

# Per-developer secret vaults + registry (plaintext secrets, tokens, file paths).
# NEVER commit this — it holds credentials.
/.charter/

# Python
__pycache__/
*.py[cod]
.venv/

# OS / editor cruft
.DS_Store
"""


def _ensure_gitignore(root: Path) -> bool:
    """Add charter's baseline ignore rules to ``.gitignore`` — creating it fresh if
    absent, or appending only the lines an existing file is missing (existing content is
    never removed, reordered, or rewritten). Returns ``True`` iff the file was created or
    changed.

    The presence check keys off the *exact* lines a missing block would add, not a
    substring test — ``"workspaces/" in body`` used to match any pre-existing rule that
    happened to contain that text (e.g. ``build/workspaces/output/``) and would then
    wrongly skip writing ``!/workspaces/.gitkeep``, the literal anchor line
    ``workspace.set_live()`` searches for to splice in its managed live-workspace block."""
    p = root / ".gitignore"
    if not p.exists():
        p.write_text(_GITIGNORE_BASELINE)
        return True
    body = p.read_text()
    existing_lines = {line.strip() for line in body.splitlines()}
    missing = []
    if "!/workspaces/.gitkeep" not in existing_lines:
        missing.append("/workspaces/*/*\n!/workspaces/.gitkeep\n")
    if ".charter/" not in body:
        missing.append("/.charter/\n")
    if not missing:
        return False
    p.write_text(body.rstrip("\n") + "\n\n# added by `charter init`\n" + "\n".join(missing))
    return True


#: What `init` writes into `.claude/settings.json`'s `statusLine` key.
_STATUSLINE = {"type": "command", "command": "charter statusline", "padding": 0}


def _statusline_snippet() -> str:
    """The exact JSON to hand a user whose `.claude/settings.json` we could not safely
    touch — so they can paste the `statusLine` key in themselves."""
    return json.dumps({"statusLine": _STATUSLINE}, indent=2)


def _json_style(text: str) -> tuple[str | None, tuple[str, str]]:
    """Best-effort ``(indent, separators)`` for ``json.dumps`` that echoes ``text``'s own
    formatting, so adding one key doesn't reformat the whole file. Not a true round-trip
    (stdlib ``json`` can't preserve comments, trailing whitespace, or hand-tweaked
    separator spacing byte-for-byte) — just the two knobs that dominate a re-dump's diff:

    - indent: the leading whitespace of the first indented ``"key"`` line, so a 4-space
      (or tab) file is re-dumped at 4 spaces, not forced to 2. ``None`` (compact, single
      line) if no such line exists.
    - separators: for a compact file, whether ``:``/``,`` already carry a trailing space
      (``{"a": 1}`` vs ``{"a":1}``), inferred from the raw text so a minified file stays
      minified instead of growing spaces it didn't have."""
    m = re.search(r'\n([ \t]+)"', text)
    if m:
        return m.group(1), (",", ": ")
    colon_space = re.search(r'":\s', text) is not None
    comma_space = re.search(r',\s', text) is not None
    return None, (", " if comma_space else ",", ": " if colon_space else ":")


def _ensure_statusline(root: Path) -> tuple[str, Path | None]:
    """Write ``.claude/settings.json``'s ``statusLine`` key IF ABSENT. That file is
    user-owned, git-tracked, has no comment syntax, and holds keys charter has no
    business touching (``permissions``, ``enabledPlugins``, ``extraKnownMarketplaces``,
    …) — so this touches *only* the one key it owns, and only when the key isn't already
    there. A malformed existing file is left completely alone: never rewritten, never
    "repaired".

    Returns ``(status, detail)``:
    - ``"created"``, ``None`` — file (or just the key) was written.
    - ``"present"``, ``None`` — a ``statusLine`` already exists; untouched.
    - ``"malformed"``, the settings path — exists but isn't valid JSON; untouched.
    - ``"blocked"``, the ``.claude`` path — that path exists and isn't a directory.
    """
    d = root / ".claude"
    p = d / "settings.json"
    if not p.exists():
        try:
            d.mkdir(parents=True, exist_ok=True)
        except OSError:
            return "blocked", d
        p.write_text(json.dumps({"statusLine": dict(_STATUSLINE)}, indent=2) + "\n")
        return "created", None
    raw = p.read_text()
    try:
        settings = json.loads(raw)
    except (OSError, json.JSONDecodeError):
        return "malformed", p
    if not isinstance(settings, dict):
        return "malformed", p
    if "statusLine" in settings:
        return "present", None
    settings["statusLine"] = dict(_STATUSLINE)
    indent, separators = _json_style(raw)
    rewritten = json.dumps(settings, indent=indent, separators=separators)
    # Match the original's trailing newline (or lack of one) rather than forcing one —
    # same "disturb it as little as practical" rule applied to the one byte outside the
    # JSON grammar itself.
    if raw.endswith("\n"):
        rewritten += "\n"
    p.write_text(rewritten)
    return "created", None


def cmd_init(args) -> int:
    """Scaffold a control plane from nothing — the first-run command a stranger needs
    and the one `root.py`'s own "no control plane found" error already points to.
    Additive-only, same discipline as `cmd_reinit`: create only what's absent, never
    modify an existing value, and when a path charter wants is occupied by something it
    can't safely touch, name the blocker, refuse to delete/rename it, and still create
    everything else that's unblocked. Writes relative to `config.ROOT`, which falls back
    to the cwd when no charter.toml is found yet (`root.find_root_or_cwd`) — that
    fallback is exactly what lets `init` land in the directory the user actually ran it
    from, instead of needing a control plane to already exist to find one."""
    from . import instance as _instance
    from .forge import registry as _registry

    root = config.ROOT
    forge_kind = getattr(args, "forge", None) or "gitlab"
    if forge_kind not in _registry.KINDS:
        util.err(f"unknown --forge {forge_kind!r} — known kinds: "
                 f"{', '.join(sorted(_registry.KINDS))}")
        return 1
    owner = getattr(args, "owner", None) or ""
    host = getattr(args, "host", None)

    # THE moment the fleet/embedded distinction is decidable. `charter init` run inside a
    # directory that is already a git repo means the user chose to install charter into a
    # codebase they work in — the embedded shape — and running it in an empty or non-repo
    # directory means the plane is its own thing. Afterwards nothing can tell: a fleet
    # plane's root is a git repo too (its personas carry committed memory), so `ROOT/.git`
    # stops being evidence the moment init has finished. So decide here, write it down,
    # and never sniff for it again. `--shape` overrides for scripted setups.
    shape = getattr(args, "shape", None)
    if shape and shape not in _instance.SHAPES:
        util.err(f"unknown --shape {shape!r} — known shapes: "
                 f"{', '.join(_instance.SHAPES)}")
        return 1
    detected = (root / ".git").is_dir()
    shape = shape or ("embedded" if detected else "fleet")

    created, present, blocked = [], [], []

    toml_path = root / _root.MARKER
    if toml_path.exists():
        present.append(_root.MARKER)
        shape = _instance.shape_of(_instance.load(root))   # existing file decides
    else:
        toml_path.write_text(_render_charter_toml(forge_kind, owner, host, shape))
        created.append(_root.MARKER)
        if shape == "embedded":
            util.info(f"{root.name} is already a git repo, so this plane is EMBEDDED: "
                      f"charter serves this codebase, there is nothing to clone, and "
                      f"worktrees live outside the tree. Re-run with --shape fleet if "
                      f"you meant a plane that clones other repos.")
        elif not owner:
            util.warn(f"No --owner given — {_root.MARKER}'s [[forge]] block has no "
                      f"owner/group set. Add one before `charter discover`.")

    d_created, d_present, d_blocked = _create_baseline_dirs(root, shape)
    created += d_created
    present += d_present
    blocked += d_blocked

    if _ensure_gitignore(root):
        created.append(".gitignore")
    else:
        present.append(".gitignore")

    sl_status, sl_detail = _ensure_statusline(root)
    if sl_status == "created":
        created.append(".claude/settings.json (statusLine)")
    elif sl_status == "present":
        present.append(".claude/settings.json (statusLine already set)")
    elif sl_status == "malformed":
        util.warn(f"{sl_detail} is not valid JSON — left it completely untouched (charter "
                  f"never rewrites or 'repairs' it). Add the status line yourself:\n"
                  f"{_statusline_snippet()}")
    elif sl_status == "blocked":
        blocked.append((".claude", sl_detail))

    # Same failure shape either way — "you asked for this, it did not happen" — so both
    # exit non-zero: a blocked baseline path (file already prints its own util.err above)
    # and a malformed settings.json (its util.warn already fired where sl_status was
    # decided). Scripted/CI callers must be able to tell from the exit code alone that
    # something requested was skipped, not just from stderr text.
    if blocked or sl_status == "malformed":
        for name, p in blocked:
            util.err(f"{name}/ can't be created — {p} already exists and is not a "
                     f"directory. charter never deletes or renames existing content; "
                     f"move or remove it yourself, then re-run `charter init`.")
        if created:
            util.info(f"  created: {', '.join(created)}")
        if present:
            util.info(f"  already present: {', '.join(present)}")
        return 1

    if created:
        util.ok(f"Initialized control plane (schema {_instance.SCHEMA}) → "
                f"{', '.join(created)}.")
    else:
        util.ok(f"Control plane already fully set up (schema {_instance.SCHEMA}) — "
                f"nothing to do.")
    if present:
        util.info(f"  already present: {', '.join(present)}")
    util.info("Next: `charter doctor` to preflight, then `charter discover` to build the "
              "inventory.")
    return 0


# --------------------------------------------------------------------------- #
# reinit — control-plane schema: the same stamp/detect/heal pattern            #
# `workspace reinit` (charter/workspace.py, commands_workspace.cmd_workspace_  #
# reinit) already proves for a single workspace's layout, lifted one level up  #
# to the whole control plane. The stamp is `schema` in charter.toml            #
# (charter.instance.load enforces it can't be newer than this engine          #
# understands); `charter.instance.drift` is the detect half; this command is   #
# the heal half; `doctor`'s "schema" check is where the drift is visible       #
# without running this first.                                                  #
# --------------------------------------------------------------------------- #
def _create_baseline_dirs(root: Path,
                          shape: str = "fleet") -> tuple[list[str], list[str], list[tuple[str, Path]]]:
    """Create any ``instance.baseline_dirs_for(shape)`` absent under ``root``; never touch a
    directory that's already there. Shared by ``cmd_init`` (a fresh control plane) and
    ``cmd_reinit`` (healing an existing one) so both follow the exact same additive
    discipline instead of two dialects of the same rule.

    Returns ``(created, present, blocked)`` — ``created``/``present`` are ``"name/"``
    labels; ``blocked`` is ``(name, path)`` for a baseline path occupied by a FILE (or
    anything else that isn't a directory). FINDING C1: that case used to fall through
    straight into ``mkdir()``, raising an uncaught ``FileExistsError``. The additive rule
    means we never delete or rename the user's file to make room — surface it and let
    them decide."""
    from . import instance as _instance

    created, present, blocked = [], [], []
    for d in _instance.baseline_dirs_for(shape):
        p = root / d
        if p.is_dir():
            present.append(f"{d}/")
        elif p.exists():
            blocked.append((d, p))
        else:
            p.mkdir(parents=True, exist_ok=True)
            created.append(f"{d}/")
    return created, present, blocked


def cmd_reinit(args) -> int:
    """Bring the control plane's own baseline layout up to date — create any top-level
    directory (personas/, inventory/, workspaces/) a newer charter expects but this
    control plane predates. Idempotent + additive: existing content is never touched.
    Fails clearly outside a control plane rather than scaffolding into whatever
    directory happens to be the cwd."""
    if not config.HAS_CONTROL_PLANE:
        util.err("no control plane found (no charter.toml here or in any parent) — "
                  "`charter reinit` only works inside one.")
        return 1
    from . import instance as _instance

    # Read from the plane on disk, NOT `config.PLANE_SHAPE`. Both are "the shape", but the
    # global is an import-time snapshot of whatever ROOT was then, and `instance.drift` —
    # the detect half this command heals — reads the file every time. When those two
    # disagree, `doctor` reports drift that `reinit` then declines to fix, which is a loop
    # with no way out from the outside. Same source, same answer, always.
    shape = _instance.shape_of(_instance.load(config.ROOT))
    created, present, blocked = _create_baseline_dirs(config.ROOT, shape)

    if blocked:
        for d, p in blocked:
            util.err(f"{d}/ can't be created — {p} already exists and is not a "
                     f"directory. charter never deletes or renames existing content; "
                     f"move or remove it yourself, then re-run `charter reinit`.")
        if created:
            util.info(f"  created: {', '.join(created)}")
        if present:
            util.info(f"  already present: {', '.join(present)}")
        return 1

    if not created:
        util.ok(f"Up to date (schema {_instance.SCHEMA}) — nothing to do.")
        return 0
    util.ok(f"Reinitialized control plane → added {', '.join(created)}.")
    if present:
        util.info(f"  already present: {', '.join(present)}")
    return 0


# --------------------------------------------------------------------------- #
# doctor                                                                       #
# --------------------------------------------------------------------------- #
def cmd_doctor(args) -> int:
    """Preflight the environment. Exit non-zero if any hard requirement fails."""
    results = doctor.run_all()

    if getattr(args, "json", False):
        print(json.dumps(
            [{"name": r.name, "status": r.status, "detail": r.detail, "hint": r.hint}
             for r in results],
            indent=2,
        ))
    else:
        print("charter preflight:\n")
        for r in results:
            print(r.render())
        print()

    failed = [r for r in results if r.status == doctor.FAIL]
    warned = [r for r in results if r.status == doctor.WARN]
    if getattr(args, "json", False):
        return 1 if failed else 0

    if failed:
        print("✗ " + f"{len(failed)} blocker(s): " + ", ".join(r.name for r in failed)
              + ". Fix the → hints above, then re-run `charter doctor`.")
        return 1
    if warned:
        print("! " + f"{len(warned)} optional item(s) pending — see hints above.")
    else:
        print("✓ All set — you can discover and clone repos.")
    return 0


def cmd_recall(args) -> int:
    """The single memory-fetch gate: search (or list) across every relevant memory base —
    the active workspace's journal, the active persona's own memory, and the shared
    namespace (+ ephemeral with --ephemeral) — with each hit labeled by its source."""
    from . import recall as rc
    scopes = list(rc.DEFAULT_SCOPES)
    if getattr(args, "scope", None):
        scopes = [s.strip() for s in args.scope.split(",") if s.strip() in rc.SCOPES]
        if not scopes:
            util.err(f"invalid --scope; choose from {', '.join(rc.SCOPES)}")
            return 1
    if getattr(args, "ephemeral", False) and "ephemeral" not in scopes:
        scopes.append("ephemeral")
    results = rc.recall(query=getattr(args, "query", None), limit=getattr(args, "limit", 8),
                        persona_name=getattr(args, "persona", None),
                        workspace_name=getattr(args, "workspace", None), scopes=scopes)
    if not results:
        q = getattr(args, "query", None)
        util.info(f"No memories {'match ' + repr(q) if q else 'yet'} across {', '.join(scopes)}.")
        return 0
    width = max((len(s) for s, _p, _t, _sc in results), default=8)
    for source, path, title, score in results:
        tag = f"  ({score})" if score else ""
        print(f"  {source:<{width}}  {title}{tag}")
    util.info(f"{len(results)} memory(ies) across {', '.join(scopes)}. "
              f"Read one: find the file under its base (workspaces/… or personas/…).")
    return 0


def cmd_git_policy(args) -> int:
    """**Golden rule 0: one credential — PER FORGE.** Report (or `--apply`) the token-only
    git policy on the control plane and every clone, resolved per repo from ITS OWN forge
    (`gitpolicy.forge_for`): that forge's own credential helper over HTTPS, signing off, and
    SSH→HTTPS URL rewrites so even an SSH remote transports over the token. Local config only —
    a developer's global git config is never touched."""
    from . import gitpolicy
    targets = gitpolicy.repos(config.ROOT, config.WORKSPACES_DIR)
    if not targets:
        util.info("No git repos found (control plane + workspace clones).")
        return 0
    apply = getattr(args, "apply", False)
    drifted = fixed = unmanaged = 0
    for repo in targets:
        try:
            rel = repo.relative_to(config.ROOT)
        except ValueError:
            rel = repo
        name = str(rel) if str(rel) != "." else "control plane"
        drift = gitpolicy.check(repo)
        if not drift:
            continue
        if drift == [gitpolicy.UNMANAGED_FORGE]:
            # Honest "can't tell" — never silently reported green, and never guessed at
            # via `apply` either (see gitpolicy.forge_for / UNMANAGED_FORGE).
            unmanaged += 1
            util.warn(f"{name}: {gitpolicy.UNMANAGED_FORGE}")
            continue
        drifted += 1
        if apply:
            changes = gitpolicy.apply(repo)
            fixed += 1
            util.ok(f"{name}: applied {len(changes)} setting(s) — token-only")
        else:
            util.warn(f"{name}: {len(drift)} setting(s) not token-only")
            for d in drift[:4]:
                util.info(f"    {d}")
    if not drifted and not unmanaged:
        util.ok(f"All {len(targets)} repo(s) are token-only (each forge's own HTTPS token, "
                f"no SSH, no signing).")
    else:
        if apply:
            util.ok(f"Applied the single-credential policy to {fixed} of {len(targets)} repo(s).")
        elif drifted:
            util.info(f"{drifted} of {len(targets)} repo(s) drifted — fix: charter git-policy --apply")
        if unmanaged:
            util.warn(f"{unmanaged} repo(s) have an unrecognised forge — not covered by "
                      f"any policy. Declare the host in charter.toml's [[forge]] to bring "
                      f"{'it' if unmanaged == 1 else 'them'} under management.")
    return 0


def cmd_version_check(args) -> int:
    """Internal: refresh the cached "is a newer charter published?" answer.

    Spawned detached by the status line; prints nothing and always exits 0 — a
    failed check must be indistinguishable from a successful one to any caller.
    """
    from . import update
    update.fetch_and_store()
    return 0


# --------------------------------------------------------------------------- #
# version lock — `[charter] version` in charter.toml                          #
# --------------------------------------------------------------------------- #
def _installed_version() -> str:
    from . import __version__
    return __version__


def _dist() -> str:
    from . import update
    return update.DIST


def _sync_cmd(version: str) -> list[str]:
    """The install that actually works. NOT `uv tool upgrade` — it reports
    "Nothing to upgrade" for a git-installed charter and leaves you pinned."""
    return ["uv", "tool", "install", f"{_dist()}=={version}", "--force", "--refresh"]


def sync_to(version: str) -> tuple[bool, str]:
    """Install exactly *version*. Returns (ok, detail). Never raises."""
    import shutil
    if not shutil.which("uv"):
        return False, "uv is not on PATH — install it, or run the pip equivalent by hand"
    proc = util.run(_sync_cmd(version), check=False)
    if proc.returncode != 0:
        why = (proc.stderr or proc.stdout or "").strip().splitlines()
        return False, (why[-1][:200] if why else f"exit {proc.returncode}")
    return True, version


def cmd_version(args) -> int:
    """Show the lock, what is installed, and what to run next."""
    from . import instance as _instance, update
    cfg = _instance.load(config.ROOT)
    locked = _instance.locked_version(cfg)
    installed = _installed_version()
    latest = (update.load().get("latest") or "").strip() or None

    print(f"  installed  {installed}")
    print(f"  locked     {locked or '— (this control plane pins no version)'}")
    print(f"  latest     {latest or '— (not checked yet)'}")
    print()
    if locked and locked != installed:
        util.warn(f"drift: this control plane pins {locked}, you are running {installed}.")
        util.info(f"  conform this machine:  charter version sync")
        return 1
    if latest and update.newer_than(installed):
        util.info(f"A newer charter is published ({latest}).")
        util.info(f"  update, commit and push the lock:  charter version bump --push")
        return 0
    util.ok("up to date." if not locked else f"in sync with the lock ({locked}).")
    return 0


def cmd_version_sync(args) -> int:
    """Conform this machine to the lock — including downgrading, which is the point."""
    from . import instance as _instance
    locked = _instance.locked_version(_instance.load(config.ROOT))
    if not locked:
        util.info("This control plane pins no version — nothing to sync. "
                  "Pin one with: charter version bump --push")
        return 0
    installed = _installed_version()
    if locked == installed:
        util.ok(f"already on the locked version ({locked}).")
        return 0
    util.info(f"syncing {installed} → {locked} …")
    ok, detail = sync_to(locked)
    if not ok:
        util.err(f"could not install {locked}: {detail}")
        util.info(f"  run by hand: {' '.join(_sync_cmd(locked))}")
        return 1
    util.ok(f"installed charter {locked}.")
    util.info("  The command you just ran is still the old build; the next "
              "`charter …` call uses the new one.")
    return 0


def cmd_version_bump(args) -> int:
    """Install → verify → write the lock → commit (+push). Team-affecting, so in that order."""
    from . import instance as _instance, update
    target = (getattr(args, "to", None) or "").strip()
    if not target:
        update.fetch_and_store()
        target = (update.load().get("latest") or "").strip()
        if not target:
            util.err("could not determine the latest version (offline?). "
                     "Pass one explicitly: charter version bump --to X.Y.Z")
            return 1
    if target != _installed_version():
        util.info(f"installing {target} to verify it before pinning the team to it …")
        ok, detail = sync_to(target)
        if not ok:
            util.err(f"refusing to pin {target}: it did not install ({detail}).")
            return 1
    if not _instance.set_locked_version(config.ROOT, target):
        util.err(f"could not write the lock into {config.ROOT / 'charter.toml'}")
        return 1
    util.ok(f"pinned this control plane to charter {target}.")
    rel = "charter.toml"
    if getattr(args, "push", False):
        rc = commit_push(config.ROOT, ["add", "--", rel], f"charter: pin to {target}")
        if rc != 0:
            util.warn("lock written, but commit/push failed — commit charter.toml yourself.")
            return rc
        util.ok("committed + pushed — teammates conform on their next session.")
    else:
        util.info(f"  commit it to share: git add {rel} && git commit -m 'charter: pin to {target}'")
    return 0
