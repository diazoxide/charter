"""Command implementations behind the ``charter`` CLI subcommands."""

from __future__ import annotations

import concurrent.futures
import json
import re
from pathlib import Path

from . import config, docsrc, doctor, inventory, render, util, workspace, worktree
# One committer for the control plane, in charter/planegit.py. Re-exported rather
# than moved-and-updated so every existing caller and test keeps working — the point
# of the extraction is that there is ONE implementation, not that callers churn.
from .planegit import (_cred_flag, _git, _origin_https, _spawn_bg_push,  # noqa: F401
                       commit_memory_reactive, commit_push)
from . import root as _root
from .forge import ForgeError
from .forge.gitlab import GitLabForge


def _clone_announcement(r: dict) -> str:
    """``name (branch)``, or just ``name`` when the branch is unknown.

    Read straight off the record as ``r['default_branch']`` until a record turned up
    without one — the plane repo, the first record charter builds by hand rather than from
    a forge query — and the KeyError took down the whole command instead of cloning. Any
    hand-built or older record could do the same; the branch is a nicety, not a
    precondition for cloning.
    """
    branch = r.get("default_branch")
    return f"{r['name']} ({branch})" if branch else r["name"]


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


def cmd_browser_install(args) -> int:
    """Generate Playwright's own driving-surface skill into this plane.

    Charter ships the `browser` skill (vault bridge, per-worker isolation) and none of
    Playwright's pages — see charter/browser.py for why redistributing them is the wrong
    trade. This fetches them from the tool that owns them.
    """
    from . import browser as _browser

    if not config.HAS_CONTROL_PLANE:
        util.err("no control plane found (no charter.toml here or in any parent) — "
                 "`charter browser install` writes into a plane's .claude/skills/.")
        return 1
    if not _browser.npx_available():
        util.err("npx not found — install Node.js to generate the Playwright skill.")
        util.info("The `browser` skill's credential bridge works regardless; only the "
                  "page-driving reference needs this.")
        return 1

    version = getattr(args, "version", None) or _browser.PINNED
    util.info(f"Generating the Playwright driving surface (@playwright/cli@{version})…")
    code, output = _browser.install(config.ROOT, version)
    if code != 0:
        util.err(f"the generator failed (exit {code}).")
        # Handed back verbatim: this is npm's diagnosis (offline, no such version, EACCES),
        # and paraphrasing it would be charter guessing at somebody else's error (ADR 0009).
        print(output.rstrip())
        return 1

    landed = config.ROOT / _browser.SKILL_DIR
    if landed.is_dir():
        util.ok(f"Wrote {_browser.SKILL_DIR} ({len(list(landed.rglob('*.md')))} page(s))")
    else:
        util.warn(f"The generator reported success but {_browser.SKILL_DIR} is not there.")
        return 1
    util.info("It is Playwright's, not charter's — regenerate it with this command "
              "rather than editing it.")
    return 0


def cmd_docs_list(args) -> int:
    """charter's own pages — the ones `docs show` can print.

    Distinct from `cmd_docs`, which writes the *plane's* generated docs. One command
    describes charter, the other describes your repos; they share a noun and nothing else.
    """
    names = docsrc.topics()
    if not names:
        util.warn("No documentation shipped with this charter — reinstall it "
                  "(the wheel is missing charter/_docs).")
        return 1
    util.info(f"charter documentation ({docsrc.source()}):")
    # The topics go to stdout (so `docs list` pipes) while the framing goes to stderr, as
    # everywhere else in charter. Flushed before the trailing hint because stderr is
    # unbuffered and stdout is not: without this the hint overtakes the list it describes
    # the moment the output is a pipe rather than a terminal.
    print("\n".join(f"  {name}" for name in names) + "\n", flush=True)
    util.info("Read one with: charter docs show <topic>")
    return 0


def cmd_docs_show(args) -> int:
    body = docsrc.read(args.topic)
    if body is None:
        names = docsrc.topics()
        # ADR 0009 — classify, don't guess. `docs show persona` is a plausible typo for
        # `personas`, and resolving it would be charter deciding what the caller meant;
        # naming the real topics lets them decide, and costs one line.
        util.err(f"No charter documentation topic named '{args.topic}'.")
        if names:
            util.info("Topics: " + ", ".join(names))
        return 1
    print(body, end="" if body.endswith("\n") else "\n")
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
    # `inventory.repos()`, not the raw document: the plane's own repo is clonable whether
    # or not `discover` has ever run (see `inventory.plane_repo`), so refusing on an empty
    # FILE would turn the one repo a fresh plane definitely has into the one it cannot get.
    if not inventory.repos(doc):
        util.err("Nothing to clone: this plane has no inventory, and its own repo could "
                 "not be derived (no origin, or a forge it does not declare).")
        util.info("  Build one: charter discover")
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

    if len(targets) > 1:
        util.info(f"Cloning {len(targets)} repo(s) into '{ws}', "
                  f"{min(CLONE_WORKERS, len(targets))} at a time …")
    with concurrent.futures.ThreadPoolExecutor(max_workers=CLONE_WORKERS) as ex:
        results = list(ex.map(lambda r: _clone_one(r, wd), targets))

    # Printed HERE, from one thread, in the order the repos were asked for. Eight workers
    # calling util.info/ok/err directly would interleave into something no one can scan
    # for which repo failed — and the failure list is the only part of this output that
    # anybody reads twice.
    failures = 0
    for res in results:
        r, dest = res["repo"], res["dest"]
        if res["status"] == "exists":
            util.info(f"{r['name']}: already cloned in '{ws}'")
        elif res["status"] == "ok":
            # Armed the moment it exists. A clone is where sessions start, and opencode
            # reads its plugin from the session's own directory — so a tree that gets its
            # wiring "later" is a tree whose first session ran unguarded.
            wire_work_tree(dest)
            util.ok(f"{r['name']} → {dest.relative_to(config.ROOT)} "
                    f"({_clone_announcement(r)} via {res['forge'].cli}, HTTPS)")
        else:
            failures += 1
            cli = res["forge"].cli
            util.err(f"{r['name']}: clone failed — no access, network, or {cli} isn't authed "
                     f"(`{cli} auth status`). Skipping.\n" + res["stderr"])
            continue
        _hint_repo_docs(dest, r)
    return 1 if failures else 0


#: Concurrent clones. The same number `_build_batch` uses for its API probes, so there is
#: one concurrency figure to reason about in this file rather than two. Cloning is
#: network-bound, so this is deliberately not derived from CPU count — a 16-core laptop
#: would open 14 connections to one forge for no gain.
CLONE_WORKERS = 8


def _clone_one(r: dict, wd) -> dict:
    """Clone ONE repo and return what happened. **Prints nothing** — `cmd_clone` renders
    every result afterwards, in order, from a single thread.

    Runs in a worker thread: `_git` shells out (releasing the GIL for the network wait,
    which is the whole point), `registry.for_repo` builds a fresh backend per call, and
    `gitpolicy.apply` writes only inside this clone's own `.git/config`. Nothing here is
    shared between repos.
    """
    from . import gitpolicy
    from .forge import registry

    dest = wd / r["name"]
    if dest.exists():
        return {"repo": r, "dest": dest, "status": "exists"}
    forge = registry.for_repo(r)
    proc = _git([*_cred_flag(forge), "clone", "--branch", r["default_branch"],
                 _https_url(r), str(dest)])
    if proc.returncode != 0:
        return {"repo": r, "dest": dest, "status": "failed", "forge": forge,
                "stderr": (proc.stderr or "").strip()}
    # Golden rule 0: every git op from this clone uses ITS FORGE's token over HTTPS —
    # credential helper + signing off + SSH→HTTPS rewrites (see charter/gitpolicy.py).
    gitpolicy.apply(dest)
    return {"repo": r, "dest": dest, "status": "ok", "forge": forge}


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
    # `inventory.repos(doc)`, not the raw document: the plane's own repo is clonable
    # without `discover` (see `inventory.plane_repo`), so counting only what is on disk
    # would report "0 repos available to clone" beside a `charter clone` that works.
    inv_by_name = {r["name"]: r for r in inventory.repos(doc)}
    all_ws = workspace.list_workspaces()
    explicit = getattr(args, "workspace", None)
    active = workspace.resolve(explicit)

    print(f"{config.GROUP}: {len(inv_by_name)} repos in inventory · "
          f"{len(all_ws)} workspace(s) · active: {active} (via {workspace.source(explicit)})")
    # "Where am I" is this command's whole job, and the plane is the outermost part of that
    # answer — every count above is a count *of this plane*, and a nested one reports
    # numbers that look like the outer plane's and are not (#200). Resolution is unchanged
    # (#140); this only says which plane answered.
    nested = util.nested_plane_note()
    if nested:
        print(nested)
    print()

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
    clones = {d.name: d for d in workspace.repo_trees(ws)}
    marker = " (active)" if ws == active else ""
    print(f"— workspace: {ws}{marker} · {len(clones)} repo(s) —")
    if not clones:
        hint = f"`charter clone <repo> --workspace {ws}` to populate"
        print(f"  (empty; {hint})\n")
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
    # `--detach` is checked first: the point is to return BEFORE any of the work
    # below, in a process the harness will not tear down with the turn. This is
    # what a hook's `"async": true` used to buy — asked of the host, and one host
    # skips such entries outright, so charter does it itself.
    if getattr(args, "detach", False):
        util.detach_self(['gl-refresh'])
        return 0

    from . import glstate

    ws = workspace.resolve(getattr(args, "workspace", None))
    # `repo_trees` is what the status line draws, so refreshing the same list is what
    # keeps a repo from being drawn without its forge state having been fetched.
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


def _render_charter_toml(forge_kind: str, owner: str, host: str | None) -> str:
    from . import instance as _instance

    lines = [f"schema = {_instance.SCHEMA}"]
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
#: `refreshInterval` re-runs the status line every N seconds IN ADDITION to the event-driven
#: updates. Set because charter's status line is mostly things that change without a session
#: event: `silent 12m` ages with wall-clock, piece counts move as background workers claim
#: and declare, and the plane-root warning fires on state another agent created. The docs
#: name this case exactly — "the event-driven triggers can go quiet when the main session is
#: idle, for example while a coordinator waits on background subagents".
#:
#: TEN, not the permitted minimum of one. Charter renders silence in MINUTES, so a
#: one-second timer is ~60x finer than the coarsest thing displayed — and a render is ~80ms
#: on a two-clone plane (a `git status` per tree), so it would burn ~8% of a core
#: continuously to refresh a number that changes once a minute. Ten is still six times finer
#: than the granularity, at a tenth of the cost.
_STATUSLINE = {"type": "command", "command": "charter statusline", "padding": 0,
               "refreshInterval": 10}


def _statusline_snippet() -> str:
    """The exact JSON to hand a user whose `.claude/settings.json` we could not safely
    touch — so they can paste the `statusLine` key in themselves."""
    return json.dumps({"statusLine": _STATUSLINE}, indent=2)


#: The ONE hook charter wires itself, and only this one.
#:
#: It is the plane-root branch guard (#157) — the safety feature — and #168 was filed
#: because it ships inert on any plane not running the plugin while `doctor` showed a green
#: tick over it. A safety feature that ships off by default stays off.
#:
#: Only `pretooluse`, deliberately. If the plugin is installed later, everything wired here
#: fires TWICE. For `pretooluse` that is harmless: the same denial computed twice is the
#: same denial. For `sessionstart` it is not — the persona briefing, memory digest and todo
#: list would render twice every session. So the other five stay plugin-only, where the
#: plugin owns their lifecycle, and charter's footprint in a user-owned file stays at one
#: hook rather than six.
_GUARD_HOOK = {
    "matcher": "Bash",
    "hooks": [{"type": "command", "command": "charter hook pretooluse", "timeout": 10}],
}


def _hooks_snippet() -> str:
    """The exact JSON for a user whose settings file charter could not safely touch."""
    return json.dumps({"hooks": {"PreToolUse": [_GUARD_HOOK]}}, indent=2)


def _ensure_guard_hook(root: Path) -> tuple[str, Path | None]:
    """Wire `charter hook pretooluse` into ``.claude/settings.json`` IF ABSENT.

    Same contract and the same restraint as :func:`_ensure_statusline`, for the same
    reason: that file is user-owned and git-tracked and holds keys charter has no business
    touching. This adds one entry under one key, only when no charter `pretooluse` hook is
    already declared there, and never rewrites a malformed file.

    Returns ``(status, detail)`` — ``"created"`` / ``"present"`` / ``"malformed"`` /
    ``"blocked"``, matching `_ensure_statusline` so callers report both the same way.
    """
    d = root / ".claude"
    p = d / "settings.json"
    if not p.exists():
        try:
            d.mkdir(parents=True, exist_ok=True)
        except OSError:
            return "blocked", d
        p.write_text(json.dumps({"hooks": {"PreToolUse": [_GUARD_HOOK]}}, indent=2) + "\n")
        return "created", None
    raw = p.read_text()
    try:
        settings = json.loads(raw)
    except (OSError, json.JSONDecodeError):
        return "malformed", p
    if not isinstance(settings, dict):
        return "malformed", p
    hooks_block = settings.get("hooks")
    if hooks_block is not None and not isinstance(hooks_block, dict):
        return "malformed", p
    pre = (hooks_block or {}).get("PreToolUse") or []
    if not isinstance(pre, list):
        return "malformed", p
    if any("charter hook pretooluse" in json.dumps(entry) for entry in pre):
        return "present", None
    settings.setdefault("hooks", {}).setdefault("PreToolUse", []).append(_GUARD_HOOK)
    indent, separators = _json_style(raw)
    rewritten = json.dumps(settings, indent=indent, separators=separators)
    if raw.endswith("\n"):
        rewritten += "\n"
    p.write_text(rewritten)
    return "created", None


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


#: Tools whose rules charter writes verbatim. Anything else is wrapped as ``Bash(...)``.
#:
#: `Bash(...)` is the host's syntax and the operator should not have to know it, but a rule
#: naming another tool must survive untouched — wrapping `Read(./secrets/**)` would produce
#: `Bash(Read(./secrets/**))`, which matches nothing and fails in the silent direction.
_RULE_TOOLS = ("Bash", "Read", "Edit", "Write", "Grep", "Glob", "WebFetch", "NotebookEdit",
               "Task", "mcp__")


def _as_rule(pattern: str) -> str:
    """A permission rule for *pattern*, wrapping a bare command in ``Bash(...)``."""
    p = (pattern or "").strip()
    if p.startswith(_RULE_TOOLS) and ("(" in p or p in ("Bash",)):
        return p
    return f"Bash({p})"


def _settings_path(root: Path) -> Path:
    return Path(root) / ".claude" / "settings.json"


def _load_settings(root: Path) -> tuple[dict | None, Path]:
    """``(settings, path)``; ``None`` when the file exists and is not parseable.

    A missing file reads as ``{}`` — writing the first rule into a plane that has no
    settings yet is ordinary. A *malformed* one reads as ``None`` so callers refuse rather
    than repair: the operator's content is in there, and `_ensure_statusline` already keeps
    that restraint for the same file.
    """
    p = _settings_path(root)
    if not p.exists():
        return {}, p
    try:
        doc = json.loads(p.read_text())
    except (OSError, json.JSONDecodeError, UnicodeDecodeError):
        return None, p
    return (doc if isinstance(doc, dict) else None), p


def add_ask_rule(root: Path, rule: str) -> tuple[str, str]:
    """Append *rule* to `permissions.ask` in the plane's `.claude/settings.json`.

    Extracted from `cmd_guard_ask` when a second harness needed the same command: the
    command now asks each registered harness to write its own file in its own syntax,
    and this is Claude Code's half. Refuses a malformed file rather than repairing it,
    and a `permissions` or `ask` of the wrong type is somebody's deliberate structure —
    charter reports it and stops.
    """
    settings, path = _load_settings(root)
    if settings is None:
        return "malformed", str(path)
    perms = settings.setdefault("permissions", {})
    if not isinstance(perms, dict):
        return "malformed", f"{path} (`permissions` is not an object)"
    ask = perms.setdefault("ask", [])
    if not isinstance(ask, list):
        return "malformed", f"{path} (`permissions.ask` is not a list)"
    if rule in ask:
        return "present", str(path)
    ask.append(rule)
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(json.dumps(settings, indent=2) + "\n")
    return "added", str(path)


def cmd_guard_ask(args) -> int:
    """Add a force-prompt rule to `permissions.ask` in the plane's `.claude/settings.json`.

    Charter writes the **host's** rule and keeps no list of its own (ADR 0014). Claude Code
    already evaluates `deny → ask → allow`, segments compound commands correctly — the job
    `hooks._segment_argv` does by hand and got wrong once — and the file is committed, so
    every engineer on the repo gets the same list with nothing to install or sync. A
    `charter.toml` list would be a second engine that could not win: *"a matching ask rule
    still prompts even when the hook returned `allow` or `ask`"*.

    `_ensure_statusline`'s docstring says settings.json holds keys "charter has no business
    touching (``permissions``, …)". That still holds for `init`, which writes unasked. This
    is the opposite: the operator named the rule and the command that writes it, so charter
    is the editor rather than the author.
    """
    pattern = (getattr(args, "pattern", "") or "").strip()
    if not pattern:
        util.err("Nothing to add. Example: charter guard ask 'terraform apply *'")
        return 2
    from .harness import registry

    root = Path(config.ROOT)
    rc, wrote = 0, False
    for h in registry.all():
        status, detail = h.apply_ask_rule(root, pattern)
        if status == "added":
            util.ok(f"{h.name}: asking for {h.ask_rule(pattern)} → {detail}")
            wrote = True
        elif status == "present":
            util.ok(f"{h.name}: already asking for {h.ask_rule(pattern)}.")
            wrote = True
        elif status == "malformed":
            util.err(f"{h.name}: {detail} is not valid — left untouched.")
            util.info("  Fix it by hand, then re-run. charter never repairs these files.")
            rc = 1
        else:
            # Not a failure. The harness has no command-pattern permissions, so charter's
            # own hook stays the only thing guarding this command there — which is worth
            # saying, because silence would read as "the rule is in force everywhere".
            util.info(f"  {h.name}: {detail} — charter's own guard still applies.")
    if not wrote and rc == 0:
        util.warn("  No harness took the rule.")
    if wrote:
        util.info("  These files are committed, so the rule applies to everyone on this "
                  "repo — no sync step, and nothing that can drift (ADR 0014).")
        _warn_if_shadowing(registry.get(registry.CLAUDE_CODE).ask_rule(pattern))
    return rc


def cmd_guard_list(args) -> int:
    """Show the plane's force-prompt rules — read from the host's file, not a charter one."""
    root = Path(config.ROOT)
    settings, path = _load_settings(root)
    if settings is None:
        util.err(f"{path} is not valid JSON.")
        return 1
    perms = settings.get("permissions")
    ask = (perms or {}).get("ask") if isinstance(perms, dict) else None
    if not ask:
        util.info("No force-prompt rules in this plane. "
                  "Add one: charter guard ask 'terraform apply *'")
        return 0
    print(f"  {path}")
    for rule in ask:
        print(f"    {rule}")
    return 0


def _warn_if_shadowing(rule: str) -> None:
    """Say at write time if this rule will shadow a persona's declared tool.

    Doctor reports the same overlap; this is the moment the operator can still change their
    mind, and charter knows it then.
    """
    from . import doctor as _doctor
    hit = _doctor.shadowed_tools([rule])
    if hit:
        util.warn(f"this also prompts for {', '.join(sorted(hit))}, which a persona "
                  f"declares — an ask rule outranks charter's tool-gate.")


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
    - ``"updated"``, ``None`` — charter's OWN status line gained a field it now writes
      (currently ``refreshInterval``). Only ever charter's own, only ever a field that was
      absent — a hand-set value is never reverted.
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
    existing = settings.get("statusLine")
    if isinstance(existing, dict):
        # Charter's OWN status line, missing a field charter has since started writing.
        # Adding it is updating a key charter wrote, which is a different act from touching
        # a user's — and without it the field would reach only brand-new planes. A status
        # line running someone else's script is left alone, and so is a value someone set by
        # hand: silently reverting a deliberate choice is what this function exists not to do.
        if (existing.get("command") == _STATUSLINE["command"]
                and "refreshInterval" not in existing):
            existing["refreshInterval"] = _STATUSLINE["refreshInterval"]
            indent, separators = _json_style(raw)
            rewritten = json.dumps(settings, indent=indent, separators=separators)
            if raw.endswith("\n"):
                rewritten += "\n"
            p.write_text(rewritten)
            return "updated", None
        return "present", None
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


def _fold_entries(entries: list[str]) -> list[str]:
    """``a.json (x)``, ``a.json (y)`` → ``a.json (x, y)``, order preserved.

    One line naming every file `init` wrote reached 254 characters once there were three
    harnesses to wire — long enough that the README's demo capture grew 40% wider and its
    text stopped being readable at GitHub's column width. Folding makes the line grow with
    the number of FILES rather than the number of things charter did to each.
    """
    order: list[str] = []
    notes: dict[str, list[str]] = {}
    for e in entries:
        head, _, tail = e.partition(" (")
        if head not in notes:
            order.append(head)
            notes[head] = []
        note = tail.rstrip(")") if tail else ""
        if note and note not in notes[head]:
            notes[head].append(note)
    return [f"{h} ({', '.join(notes[h])})" if notes[h] else h for h in order]


def ensure_env_var(root: Path, key: str, value: str) -> tuple[str, Path | None]:
    """Set ``env[key]`` in ``.claude/settings.json`` IF ABSENT (ADR 0015).

    Claude Code's `env` *"sets environment variables that apply to every session"*, which
    is how a harness with no per-shell hook still names itself to charter. Public rather
    than underscored because a harness class calls it — `settings.json` is this module's
    territory (it already owns the status line, the guard hook and the ask rules in that
    same file), so the plumbing stays here and the harness asks for it.

    Same contract and restraint as :func:`_ensure_statusline`. An `env` that is not an
    object, or a key someone set by hand, is left alone — reverting a deliberate choice
    is what these writers exist not to do.
    """
    settings, p = _load_settings(root)
    if settings is None:
        return "malformed", p
    env = settings.get("env")
    if "env" in settings and not isinstance(env, dict):
        return "present", p
    if not isinstance(env, dict):
        env = {}
    if env.get(key):
        return "present", p
    env[key] = value
    settings["env"] = env
    raw = p.read_text() if p.exists() else ""
    indent, separators = _json_style(raw)
    rewritten = json.dumps(settings, indent=indent, separators=separators)
    if raw.endswith("\n"):
        rewritten += "\n"
    p.parent.mkdir(parents=True, exist_ok=True)
    p.write_text(rewritten)
    return "created", p


def wire_work_tree(tree: Path) -> list[tuple[str, str]]:
    """Arm every registered harness inside one clone or worktree.

    Called the moment a tree comes into existence, because that is where sessions start:
    opencode reads its plugin from the session's own directory and does not walk upwards,
    so a plane-root-only shim is inert in every session that matters (ADR 0008 — the plane
    root is not a work tree). Nothing here names a harness; a harness with no per-tree
    wiring returns nothing and costs one call.
    """
    from .harness import registry

    # Never bring the tree into being. This ran on a clone path before the clone existed
    # and left `a/`, `r0/`… wherever the process happened to be standing — a writer that
    # creates its own target cannot tell a real tree from a typo.
    if not Path(tree).is_dir():
        return []
    out: list[tuple[str, str]] = []
    for h in registry.all():
        out.extend(h.wire_tree(Path(tree)))
    return out


def refresh_work_tree(tree: Path) -> list[tuple[str, str]]:
    """Bring one tree's wiring up to this charter. What `reinit` does, and what `doctor`'s
    "→ charter reinit" hint promises — a hint that does not fix what it points at spends
    the operator's trust and leaves the guard inert."""
    from .harness import registry

    if not Path(tree).is_dir():
        return []
    out: list[tuple[str, str]] = []
    for h in registry.all():
        out.extend(h.refresh_tree(Path(tree)))
    return out


def _backfill_work_trees() -> list[str]:
    """Arm every EXISTING tree. Labels of the ones that were missing it.

    `reinit`'s job: a plane cloned before this shipped — or one whose worktrees were made
    by an older charter — has trees that look wired because `init` reported writing a
    shim, and are not. `doctor` names them; this is the command its hint points at.
    """
    from . import workspace as _ws

    written: list[str] = []
    for tree in _ws.all_trees():
        for status, label in refresh_work_tree(tree):
            if status in ("created", "refreshed"):
                written.append(f"{label} ({status})" if status == "refreshed" else label)
    return written


def _wire_harnesses(root: Path) -> list[tuple[str, str]]:
    """Write every REGISTERED harness's wiring under *root*, IF ABSENT.

    Every harness, not the detected one: `init` typically runs from a plain shell where
    no harness is detectable at all, and wiring only the runtime you happen to be sitting
    in makes the plane you built depend on which terminal you built it from. Writing one
    harness's wiring unconditionally while another waits to be asked is also the lock-in
    ADR 0015 removes.

    Nothing here names a harness. Adding Codex is adding a class to
    ``harness.registry.KINDS`` — this loop, and `init`'s report, cover it that day.
    """
    from .harness import registry

    out: list[tuple[str, str]] = []
    for h in registry.all():
        out.extend(h.wire(root))
    return out


# --------------------------------------------------------------------------- #
# init's one offer: the repo you are standing in                               #
#                                                                              #
# Being inside a git repo used to decide the plane's SHAPE; there is one shape #
# now (docs/adr/0007), so it decides nothing about what init BUILDS. It is     #
# kept for the debt that removal created: charter used to promise a solo user  #
# with one repo could `charter init` and carry on working in that repo,        #
# because the `default` workspace WAS the plane root. It isn't, so init offers #
# the first clone instead — the lesson "work happens in a workspace, not in    #
# the plane root" met once at setup rather than later as "where did my code    #
# go?".                                                                        #
#                                                                              #
# "Offers" is not a prompt, and cannot be: `util.py` carries info/ok/warn/err  #
# and nothing that reads stdin, because charter runs inside hooks and agent    #
# sessions where blocking on stdin hangs the turn. So the offer is a printed   #
# command and running it (`charter init --clone-this-repo`) IS the acceptance  #
# — the same two-step shape `charter report` uses for consent, where the       #
# second command is the consent (docs/adr/0003, charter/commands_report.py).   #
# Nothing is ever cloned that was not asked for by name.                       #
# --------------------------------------------------------------------------- #
def _is_repo_top_level(root: Path) -> bool:
    """True when *root* is the TOP LEVEL of a git working tree.

    Deliberately not "is root somewhere inside a repo". ``rev-parse --show-toplevel``
    walks upwards, and a great many people keep ``$HOME`` under git for their dotfiles —
    so a plane scaffolded at ``~/planes/acme`` would have init offering to clone the home
    directory into it. The offer exists for one person standing in one project, which is
    the equality case; anything looser turns a helpful line into an alarming one.

    Asked of git rather than tested as ``(root / ".git").exists()`` so a linked worktree
    (whose ``.git`` is a file) and a genuinely broken ``.git`` are judged by git's own
    rules. Paths are resolved on both sides because git answers with the real path and the
    plane root may be reached through a symlink — ``/var/…`` vs ``/private/var/…`` on
    macOS is enough to make a true equality read as false.

    A machine with no git at all gets no offer instead of a traceback: `init` is the one
    command a stranger runs BEFORE `doctor` has told them git is missing.
    """
    try:
        proc = _git(["rev-parse", "--show-toplevel"], cwd=root)
        top = (proc.stdout or "").strip()
        if proc.returncode != 0 or not top:
            return False
        return Path(top).resolve() == Path(root).resolve()
    except OSError:
        return False


def _origin_url(root: Path) -> str:
    """*root*'s ``origin`` remote as configured, or ``""`` — never raises."""
    try:
        return _git(["remote", "get-url", "origin"], cwd=root).stdout.strip()
    except OSError:
        return ""


def _first_clone_name(root: Path) -> str:
    """What to call the clone of the repo *root* sits in.

    The basename of its ``origin`` URL when it has one, NOT the directory the plane
    happens to live in: `cmd_clone` names a clone after its inventory record, which
    carries the FORGE's name for the repo. A plane in ``~/work/acme-api-checkout`` whose
    origin is ``…/acme/api.git`` must produce ``workspaces/default/api``, or a later
    `charter clone api` lands a second copy of the same repo beside the first and neither
    one is obviously the real one.

    ``workspace.valid_name`` is reused as a sanity filter on a path segment charter is
    about to create — a URL that ends in nothing name-shaped falls back to the directory
    the user already calls their project."""
    url = _origin_url(root)
    tail = re.split(r"[/:]", url.rstrip("/"))[-1] if url else ""
    if tail.endswith(".git"):
        tail = tail[:-len(".git")]
    return tail if workspace.valid_name(tail) else root.name


def _first_clone_dest(root: Path) -> Path:
    """Where the first clone of *root*'s repo goes — the FIRST workspace, by name.

    `workspace.resolve()` is deliberately not consulted: it answers "which workspace is
    this session on", and during `init` there is no session to have chosen one. The
    workspace being offered is the one every plane starts with."""
    return config.WORKSPACES_DIR / config.DEFAULT_WORKSPACE / _first_clone_name(root)


def _offer_first_clone(root: Path) -> None:
    """Say it once, in the terms the person is standing in (docs/adr/0008: the plane root
    is not a work tree).

    One `util.info` with embedded newlines rather than three calls, because each call
    prefixes its own `•` bullet — and a bullet in front of the command is a character that
    gets copied along with it. The malformed-settings warning below already prints its
    paste-me JSON this way for the same reason."""
    name = _first_clone_name(root)
    util.info(f"You are standing in the git repo '{name}'. Work happens in a workspace, "
              f"not in the plane root — clone it into the first one:\n"
              f"      charter init --clone-this-repo\n"
              f"  Nothing is cloned unless you run that. It lands in "
              f"workspaces/{config.DEFAULT_WORKSPACE}/{name}/, and declining leaves this "
              f"plane complete.")


def _clone_first_workspace(root: Path) -> int:
    """Accept the offer: clone the repo *root* sits in into the first workspace.

    Cloned from the plane root ON DISK rather than fetched from its origin. This runs
    during setup, before `doctor` has checked that the forge CLI is even authed, and it
    must not be the step that stalls on an SSH passphrase or fails on a token that isn't
    there yet. A local clone also carries commits that were never pushed — for the one
    person standing in their own project, that is the work they were in the middle of.

    Its ``origin`` is then repointed at the SOURCE's origin (rewritten to HTTPS by that
    forge's own rule when charter recognises the host — golden rule 0 is that git talks
    over a token, never SSH). Left pointing at the plane root it would look right and fail
    at the first push: git refuses a push to a non-bare repo's checked-out branch.

    Never touches the plane root's git state — it is read, and only read. The plane root
    is a working tree someone is in the middle of using.
    """
    name = _first_clone_name(root)
    ws = config.DEFAULT_WORKSPACE
    try:
        wd = workspace.ensure(ws)
    except ValueError as e:
        util.err(str(e))
        return 1
    dest = wd / name
    if dest.exists():
        # Additive, exactly like the rest of `init`: re-running is always safe, and never
        # re-clones over work that is already sitting there.
        util.info(f"{name}: already cloned in '{ws}' — left exactly as it is.")
        return 0

    util.info(f"Cloning {name} into workspace '{ws}' …")
    proc = _git(["clone", "--quiet", str(root), str(dest)])
    if proc.returncode != 0:
        util.err(f"could not clone {root} into {dest} — nothing else was changed.\n"
                 + (proc.stderr or "").strip())
        return 1

    upstream = _origin_https(root) or _origin_url(root)
    if upstream:
        _git(["remote", "set-url", "origin", upstream], cwd=dest)
    else:
        util.warn(f"{name} has no origin of its own yet, so this clone's origin is the "
                  f"plane root — give it a real remote before you push.")
    from . import gitpolicy
    gitpolicy.apply(dest)     # golden rule 0, same as `charter clone` does per clone

    rel = dest.relative_to(config.ROOT)
    util.ok(f"{name} → {rel}")
    _hint_repo_docs(dest, {"name": name})
    util.info(f"  work there: cd {rel}   (being in that directory IS this workspace)")
    return 0


def _first_clone_step(root: Path, accepted: bool) -> int:
    """The one thing being inside a git repo changes about `init`: what it OFFERS."""
    here = _is_repo_top_level(root)
    if not accepted:
        # An offer already taken is noise. `init` is idempotent and gets re-run — after
        # adding a forge, after an upgrade — and repeating a suggestion the user has
        # already acted on is how people learn to skip init's output, which is also where
        # its actual errors are.
        if here and not _first_clone_dest(root).exists():
            _offer_first_clone(root)
        return 0
    if not here:
        util.err(f"--clone-this-repo: there is no repo here to clone. {root} is not the "
                 f"top level of a git working tree, and the flag clones the repo you are "
                 f"standing in. The control plane itself was still created.")
        return 1
    return _clone_first_workspace(root)


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

    # Being inside a git repo used to decide the plane's SHAPE here, and there is now only
    # one shape — so `init` produces the same plane wherever it runs. What it detects is
    # still worth knowing, but it changes only what init OFFERS (a first clone of the repo
    # you are standing in — see `_first_clone_step`, called last), never what it builds.
    created, present, blocked = [], [], []

    toml_path = root / _root.MARKER
    if toml_path.exists():
        present.append(_root.MARKER)
    else:
        toml_path.write_text(_render_charter_toml(forge_kind, owner, host))
        created.append(_root.MARKER)
        if not owner:
            util.warn(f"No --owner given — {_root.MARKER}'s [[forge]] block has no "
                      f"owner/group set. Add one before `charter discover`.")

    d_created, d_present, d_blocked = _create_baseline_dirs(root)
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
    elif sl_status == "updated":
        created.append(".claude/settings.json (statusLine refreshInterval)")
    elif sl_status == "present":
        present.append(".claude/settings.json (statusLine already set)")
    elif sl_status == "malformed":
        util.warn(f"{sl_detail} is not valid JSON — left it completely untouched (charter "
                  f"never rewrites or 'repairs' it). Add the status line yourself:\n"
                  f"{_statusline_snippet()}")
    elif sl_status == "blocked":
        blocked.append((".claude", sl_detail))

    # The plane-root branch guard (#157). Wired here because #168: it shipped inert on any
    # plane not running the plugin, with `doctor` showing a green tick over it — and a
    # safety feature that ships off by default stays off.
    for status, label in _wire_harnesses(root):
        (created if status == "created" else present).append(label)

    gh_status, gh_detail = _ensure_guard_hook(root)
    if gh_status == "created":
        created.append(".claude/settings.json (plane-root guard)")
    elif gh_status == "present":
        present.append(".claude/settings.json (plane-root guard already wired)")
    elif gh_status == "malformed":
        util.warn(f"{gh_detail} is not valid JSON — left it completely untouched. Wire the "
                  f"plane-root guard yourself:\n{_hooks_snippet()}")
    elif gh_status == "blocked":
        blocked.append((".claude", gh_detail))

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
        # Folding the settings entries together (0.41.0) took this from 254 columns to
        # 194. It still does not wrap, so it is still unreadable in an 80-column terminal,
        # and because the README's demo is a real capture it still drives that image to
        # 1518px. The fold was right; the shape was the rest of it. A COUNT on the
        # headline, the names underneath: what a reader needs first is "it worked, here is
        # how much", and the inventory of paths is detail that belongs under a headline.
        entries = _fold_entries(created)
        util.ok(f"Initialized control plane (schema {_instance.SCHEMA}) — "
                f"{len(entries)} item(s) written.")
        for item in entries:
            util.info(f"  + {item}")
    else:
        util.ok(f"Control plane already fully set up (schema {_instance.SCHEMA}) — "
                f"nothing to do.")
    if present:
        util.info(f"  already present: {', '.join(present)}")
    util.info("Next: `charter doctor` to preflight, then `charter discover` to build the "
              "inventory.")
    # Last, so the most specific thing init can say about THIS directory is the line the
    # user's eye lands on. Its rc becomes init's: a clone that was asked for and did not
    # happen is the same failure shape as a blocked baseline path — "you requested this,
    # it did not happen" has to be visible from the exit code alone.
    return _first_clone_step(root, bool(getattr(args, "clone_this_repo", False)))


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
def _create_baseline_dirs(root: Path) -> tuple[list[str], list[str], list[tuple[str, Path]]]:
    """Create any ``instance.BASELINE_DIRS`` absent under ``root``; never touch a
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
    for d in _instance.BASELINE_DIRS:
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

    created, present, blocked = _create_baseline_dirs(config.ROOT)

    # The plane-root guard, on the SAME terms as init. `doctor`'s hint for an unwired plane
    # names `charter reinit`, so reinit has to be the command that actually wires it (#168).
    # The status line, on the same terms as the guard hook below: charter's own key gains a
    # field charter has since started writing (`refreshInterval`), and nothing else is
    # touched. Without this the field would reach only brand-new planes.
    sl_status, sl_detail = _ensure_statusline(config.ROOT)
    if sl_status == "created":
        created.append(".claude/settings.json (statusLine)")
    elif sl_status == "updated":
        created.append(".claude/settings.json (statusLine refreshInterval)")
    elif sl_status == "malformed":
        util.warn(f"{sl_detail} is not valid JSON — left it completely untouched. Add the "
                  f"status line yourself:\n{_statusline_snippet()}")

    for status, label in _wire_harnesses(config.ROOT):
        if status == "created":
            created.append(label)
    created.extend(_backfill_work_trees())

    gh_status, gh_detail = _ensure_guard_hook(config.ROOT)
    if gh_status == "created":
        created.append(".claude/settings.json (plane-root guard)")
    elif gh_status == "present":
        present.append(".claude/settings.json (plane-root guard already wired)")
    elif gh_status == "malformed":
        util.warn(f"{gh_detail} is not valid JSON — left it completely untouched. Wire the "
                  f"plane-root guard yourself:\n{_hooks_snippet()}")

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
    if getattr(args, "json", False):
        results = doctor.run_all()
        print(json.dumps(
            [{"name": r.name, "status": r.status, "detail": r.detail, "hint": r.hint}
             for r in results],
            indent=2,
        ))
    else:
        # Printed as each check lands, not collected first. A preflight killed by its
        # SessionStart hook timeout used to emit nothing at all — not even the checks that
        # had already passed — so a hang looked identical to a crash. Now the last line
        # printed names where it stopped.
        print("charter preflight:\n")
        results = []
        for r in doctor.iter_all():
            results.append(r)
            print(r.render(), flush=True)
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


def _rel_to_root(path) -> str:
    """Plane-relative where possible. An absolute path out of a temp dir or someone else's
    home is noise in a transcript, and every other charter surface prints relative."""
    try:
        return str(Path(path).relative_to(config.ROOT))
    except (ValueError, TypeError):
        return str(path)


def _body_snippet(path, width: int = 90) -> str:
    """The first real line of a memory's body — enough to tell whether this is the hit you
    wanted, without a `Read`. Frontmatter, headings and the `_Avoid_`-style italic lines are
    skipped because none of them distinguish one memory from another.

    Unreadable is not an error: `recall` runs constantly and must degrade to less, never to
    a failure.
    """
    try:
        lines = Path(path).read_text().splitlines()
    except OSError:
        return ""
    in_front = False
    for i, raw in enumerate(lines):
        ln = raw.strip()
        if i == 0 and ln == "---":
            in_front = True
            continue
        if in_front:
            in_front = ln != "---"
            continue
        if ln and not ln.startswith("#") and not ln.startswith("_"):
            return ln[:width] + ("…" if len(ln) > width else "")
    return ""


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
    since = None
    if getattr(args, "since", None):
        try:
            since = rc.parse_since(args.since)
        except ValueError as e:
            util.err(str(e))
            return 2
    limit = getattr(args, "limit", 8)
    all_ws = getattr(args, "all_workspaces", False)
    # Ask for one more than we will show, so "8 of ?" can say whether anything was cut
    # without a second pass over every base.
    got = rc.recall(query=getattr(args, "query", None), limit=(limit + 1) if limit else 0,
                    persona_name=getattr(args, "persona", None),
                    workspace_name=getattr(args, "workspace", None), scopes=scopes,
                    since=since, all_workspaces=all_ws)
    truncated = bool(limit) and len(got.hits) > limit
    results = got.hits[:limit] if limit else got.hits
    if not results:
        q = getattr(args, "query", None)
        # "No memories match X" and "none of X was searchable" are different answers, and
        # printing the first when it is really the second reports an empty corpus. A query
        # of only stopwords or single characters searches for nothing at all.
        if q:
            from . import memstore as _ms
            if not _ms._terms(q):
                dropped = ", ".join(repr(t) for t in _ms.dropped_terms(q)) or repr(q)
                util.info(f"Nothing searchable in {q!r} — {dropped} "
                          f"{'is' if len(_ms.dropped_terms(q)) == 1 else 'are'} too short "
                          f"or too common to rank. Try a distinctive word.")
                return 0
        where = "every workspace" if all_ws else ", ".join(scopes)
        when = f" recorded since {since}" if since else ""
        util.info(f"No memories {'match ' + repr(q) if q else 'yet'} across {where}{when}.")
        if got.undated:
            util.info(f"{got.undated} undated memory(ies) skipped — no recorded date to compare.")
        return 0
    width = max((len(h.label) for h in results), default=8)
    full = getattr(args, "full", False)
    for h in results:
        tag = f"  ({h.score})" if h.score else ""
        # The date leads because when listing it IS the sort key — and the column order
        # stays identical under a query (where score orders instead), since a layout that
        # rearranges itself per mode is harder to read than one that never moves.
        print(f"  {h.date.isoformat() if h.date else '—':<10}  {h.label:<{width}}  {h.title}{tag}")
        # The ADDRESS, on every hit. This used to be a closing sentence telling the reader
        # to go and find the file, which is a direction rather than a location — and the
        # slug rules differ per base (the journal timestamps its filenames, persona memory
        # does not), so following it cost an inference and a `Read` per hit.
        print(f"              {_rel_to_root(h.path)}")
        if full:
            snip = _body_snippet(h.path)
            if snip:
                print(f"              {snip}")
    where = "every workspace" if all_ws else ", ".join(scopes)
    util.info(f"{len(results)} memory(ies) across {where}."
              + ("" if full else "  Pass --full for a line of each body."))
    if truncated:
        util.info(f"Showing {len(results)} — pass --limit 0 for all.")
    if got.undated:
        util.info(f"{got.undated} undated memory(ies) skipped by --since — no recorded date.")
    if got.undated_refs:
        # Said separately from the memory count on purpose: an undated MEMORY lost a stamp it
        # was meant to carry, while a refs document never had one. Blaming a runbook for a
        # missing date would read as corruption rather than as the filter not applying.
        util.info(f"{got.undated_refs} ref doc(s) not searched — `--since` filters by "
                  f"recorded date and refs carry none. Drop --since to include them.")
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
    print(f"  latest     {update.latest_display(installed)}")
    print()
    if locked and locked != installed:
        util.warn(f"drift: this control plane pins {locked}, you are running {installed}.")
        util.info(f"  {update.SHARED_INSTALL_NOTE}")
        util.info(f"  conform this machine:  charter version sync")
        return 1
    if latest and update.newer_than(installed):
        util.info(f"A newer charter is published ({latest}).")
        util.info(f"  update, commit and push the lock:  charter version bump --push")
        return 0
    util.ok("up to date." if not locked else f"in sync with the lock ({locked}).")
    return 0


def cmd_version_sync(args) -> int:
    """Move THIS plane to its lock. ``--cli`` conforms the machine-global binary instead.

    The default flipped in the change that unparked #127. `version sync` used to conform a
    binary every plane on the machine shares, which is not a fix but a choice of victim:
    "two planes with different pins thrash the same binary back and forth, each `sync`
    breaking the other". The plugin is installed per project out of a cache holding every
    version at once, so moving it honours this plane's pin and touches nothing else.

    ``--cli`` keeps the old behaviour rather than deleting it: a machine running charter
    with no plugin at all still needs a way to conform the binary, and removing the escape
    hatch would be its own defect.
    """
    from . import instance as _instance, update as _update
    locked = _instance.locked_version(_instance.load(config.ROOT))
    if not locked:
        util.info("This control plane pins no version — nothing to sync. "
                  "Pin one with: charter version bump --push")
        return 0

    if not getattr(args, "cli", False):
        plugin = _update.plugin_version_here()
        if plugin == locked:
            util.ok(f"the plugin serving this project is already on the lock ({locked}).")
            return 0
        where = f"{plugin} → {locked}" if plugin else f"→ {locked}"
        util.info(f"this plane's version is the plugin's ({where}).")
        # Named, not run. `claude` may be absent, may prompt for a scope, and the command
        # mutates the reader's editor install — charter says exactly what to run and lets
        # them run it, the same restraint the MCP launcher check keeps.
        util.info(f"  run: {_update.PLUGIN_SYNC_CMD}")
        util.info(f"  the machine-global binary instead: charter version sync --cli")
        return 0

    installed = _installed_version()
    if locked == installed:
        util.ok(f"already on the locked version ({locked}).")
        return 0
    # Said before the install, not after: this conforms a binary every plane on the
    # machine shares, so the next plane the reader opens may have just gone into drift.
    util.warn(_update.SHARED_INSTALL_NOTE)
    util.info(f"syncing the machine-global binary {installed} → {locked} …")
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
        # `charter save`, not raw git. The pin is written into the PLANE ROOT, and on a plane
        # whose repo requires pull requests the root is the one tree #157 forbids branching —
        # so "commit it yourself" stranded the change and sent the operator to make the same
        # edit twice (#167). `save` now knows how to land it either way.
        util.info(f"  share it: charter save 'charter: pin to {target}'")
        util.info(f"  (on a plane whose repo requires pull requests, that pushes a branch "
                  f"and hands you the URL to open one — {rel} lives in the plane root, which "
                  f"is not a tree to branch by hand.)")
    return 0
