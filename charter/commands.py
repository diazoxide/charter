"""Command implementations behind the ``charter`` CLI subcommands."""

from __future__ import annotations

import concurrent.futures
import json
import re
from pathlib import Path

from . import (config, contain, docsrc, doctor, instance, inventory, render, tui, util,
               workspace, worktree)
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
    # Returns NOTHING rather than the value it was handed (#335). The fallthrough used to
    # be unconditional, so anything that matched no known SSH form went to `git clone`
    # verbatim — and `inventory/repos.json` is a tracked file. `ext::sh -c '…'` is a
    # transport that runs a command; `--upload-pack=…` is not a URL at all but an option
    # git reads off the argv; `/etc/passwd` is a path git will happily try to clone.
    # Confirmed on 0.47.2: all three came back unchanged.
    #
    # The only thing stopping the first one today is git's own `protocol.*.allow`, which
    # charter neither sets nor owns — people relax it for `ext` helpers, and a plane that
    # has is unprotected. A function whose entire purpose is producing a URL that uses the
    # right credential should not be able to return something that is not a URL, so this
    # is an allowlist: HTTPS, or an SSH form `insteadof()` actually recognised. Refusing
    # the shape rather than blacklisting `ext::` also means the next transport, and the
    # leading-dash argv case, need no further thought.
    return ""


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
    # Same rule as the vault-config path (#332). A human typed this one, so it is a
    # refusal with a message rather than a traceback — but it is the SAME predicate, since
    # two answers to "is this a version" is how the looser one survives.
    if not _browser.version_ok(version):
        util.err(_browser.NOT_A_VERSION.format(version=version))
        return 1
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

    # The paths this command leaves behind are different in kind, and #278 was filed because
    # saying nothing about any of them left every plane to re-derive the lot, in silence.
    # `.playwright-cli/` is where traces land, and a trace records network requests with
    # their headers and bodies — so a trace of a bridged login holds the credential the
    # bridge exists to protect. That one charter ignores (ADR 0017). The generated pages and
    # `.playwright/cli.config.json` carry no credential, so whether they are committed is a
    # real trade-off the plane owns; charter owes it the costs, not a decision taken while
    # nobody was looking.
    for line in _browser.ensure_output_ignored(config.ROOT):
        util.ok(f"Ignored {line} — traces and snapshots of authenticated runs, not source.")
    util.info(f"{_browser.SKILL_DIR} and {_browser.CONFIG_DIR}/ are left tracked-or-not as "
              f"you choose: commit the pages and a fresh clone needs no npx round trip, at "
              f"the cost of a tree a later `install` rewrites under you. Both answers are "
              f"fine — docs/browser.md states them; charter does not pick for you.")
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
        if res["status"] == "refused":
            # Named, never silently skipped: a refusal here means a committed file in this
            # plane carries something that is not a name, and the person who can fix it is
            # reading this output.
            failures += 1
            util.err(f"{r['name']!r}: not cloned — {res['reason']}.")
            continue
        if res["status"] == "exists":
            util.info(f"{r['name']}: already cloned in '{ws}'")
        elif res["status"] == "ok":
            util.ok(f"{r['name']} → {dest.relative_to(config.ROOT)} "
                    f"({_clone_announcement(r)} via {res['forge'].cli}, HTTPS)")
        else:
            failures += 1
            cli = res["forge"].cli
            util.err(f"{r['name']}: clone failed — no access, network, or {cli} isn't authed "
                     f"(`{cli} auth status`). Skipping.\n" + res["stderr"])
            continue
        # A clone that fetched less than the repo records says so BEFORE the docs hint —
        # an empty submodule directory is the thing that breaks the next command, and the
        # exit code stays 0 because the repo really is cloned (#817).
        report_submodule_drift(dest, r["name"])
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

    **The network half is recorded as in flight (#420).** #387 promised the frame a
    spinner while "a dispatch, clone or `gl-refresh`" runs and only dispatches animated,
    because `inflight.start` had one caller. This is one of the two that were missing. The
    record is per REPO rather than per command, so eight parallel clones read as eight —
    `inflight`'s own `mkstemp` naming exists for exactly that concurrency, and this
    function is already the per-repo, thread-local unit.

    `kind=inflight.CLONE`, which is what keeps the dispatch-overlap nudge out of it: that
    nudge reads names back to an operator as a sentence, and `inflight.still_running`
    answers dispatches unless asked otherwise (see that module's docstring).

    Recorded only around the parts that take time — after the destination and the URL are
    settled, so a refusal never leaves a record, and released in a `finally`, so a `git`
    that raises does not either. A process KILLED mid-clone still leaves one, and that is
    the case `PRESUMED_DEAD_SECONDS`/`PRUNE_SECONDS` already exist for: the frame draws
    such a record statically (`⋯`), never spinning.
    """
    from . import gitpolicy, inflight
    from .forge import registry

    # #325: the destination is a name out of `inventory/repos.json`, a TRACKED file, and
    # this join had nothing between the two. A name with parent components put the clone
    # — and `gitpolicy.apply`'s write to `.git/config` — outside the workspace entirely.
    # `contain.child` rather than `workspace.valid_name`: a forge mints these names, and
    # `org/.github` is a real repo GitHub tells organisations to create.
    dest = contain.child(wd, r.get("name") or "")
    if dest is None:
        return {"repo": r, "dest": wd, "status": "refused",
                "reason": contain.refusal(str(r.get("name") or ""))}
    if dest.exists():
        return {"repo": r, "dest": dest, "status": "exists"}
    forge = registry.for_repo(r)
    # #335: refusing the URL has to mean refusing the clone. Handing "" to `git clone`
    # would be the same defect wearing this fix.
    url = _https_url(r)
    if not url:
        return {"repo": r, "dest": dest, "status": "refused",
                "reason": "its inventory record carries no HTTPS clone URL, and the "
                          "`ssh_url` it does carry is not a form this forge recognises — "
                          "charter will not hand git a string it did not build"}
    inflight.start(dest.name, kind=inflight.CLONE)
    try:
        proc = _git([*_cred_flag(forge), "clone", "--branch", r["default_branch"],
                     "--", url, str(dest)])
        if proc.returncode != 0:
            return {"repo": r, "dest": dest, "status": "failed", "forge": forge,
                    "stderr": (proc.stderr or "").strip()}
        # Golden rule 0: every git op from this clone uses ITS FORGE's token over HTTPS —
        # credential helper + signing off + SSH→HTTPS rewrites (see charter/gitpolicy.py).
        gitpolicy.apply(dest)
        return {"repo": r, "dest": dest, "status": "ok", "forge": forge}
    finally:
        inflight.finish(dest.name, kind=inflight.CLONE)


def cmd_save(args) -> int:
    """Commit + push the CONTROL PLANE's own changes in one step, via ITS OWN FORGE's
    HTTPS token — no SSH keys, no 1Password signing hang. (For a *clone's* changes, work
    in a repo-rooted session; this is only the control plane's orchestration files.)

    **Refused from either kind of tree that is not the plane's own** — a linked worktree of
    it (#806) and a workspace clone that is itself a plane (#809). Both made this run ``git
    -C <somewhere else> add -A`` and commit a tree the caller was not standing in: the
    operator's mid-edit files under the agent's message, and none of the agent's own work.
    They are detected separately (`root.tree_of`, `root.nested_plane_in`) because the
    evidence differs — a worktree's ``.git`` is a file, a clone's is a directory — but both
    guards are in `planegit.commit_push` rather than here, keyed on the add staging files it
    never named: the danger is the unbounded stage, not this command's name.
    """
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
# submodules: reported, never fetched (#817)                                   #
# --------------------------------------------------------------------------- #
#: ``git submodule status`` marks each recorded submodule with ONE leading character:
#: ``-`` nothing checked out, ``+`` checked out at a commit other than the one the
#: superproject records, ``U`` unmerged, and a space for in sync. Only the first two are
#: drift charter reports. ``U`` is a conflict the operator is standing in the middle of,
#: and a second voice on a live merge is noise, not a finding.
_SUBMODULE_ABSENT = "-"
_SUBMODULE_MOVED = "+"

#: The one command charter names, and the only one it names. ``--init`` covers the paths
#: with nothing checked out, the update itself moves the ones left behind, and
#: ``--recursive`` covers a submodule that has submodules of its own — so one line is the
#: remedy for every state below rather than three lines the reader has to choose between.
SUBMODULE_REMEDY = "submodule update --init --recursive"


def submodule_drift(d: Path) -> tuple[list[str], list[str]]:
    """``(nothing checked out, not at the recorded commit)`` — submodule paths in the tree
    at *d*, in the order git lists them.

    ``([], [])`` for a tree with no submodules, for a directory that is not a repository,
    and for a `git` that exited non-zero. This feeds three REPORTS (`clone`, `sync`,
    `status`); none of them is the place to raise, and a workspace holds whatever somebody
    put in it.

    **The non-zero check is not a formality, and it is not satisfied by an empty stdout.**
    `git submodule status` prints the submodules it mapped and THEN fails on one it did
    not: measured, an index carrying a gitlink that `.gitmodules` does not map exits 128
    having already written ``-<sha> mapped`` to stdout. Without the check those lines are
    reported as findings out of a run git itself disowned.

    **The `.gitmodules` stat comes first and is not an optimisation detail.** `status`
    already spends two `git` invocations per row and prints the table as it goes; a third
    on every clone would be paid by every plane, and almost every clone has no submodules
    at all. The stat is also EXACT rather than approximate: `git submodule status` lists
    what `.gitmodules` maps, so with no such file there is nothing it could have said —
    measured, a gitlink with no `.gitmodules` makes it exit 128 (`no submodule mapping
    found in .gitmodules for path 'gl'`) and print nothing, which is the same answer this
    returns.

    Not ``--recursive``. A submodule with nothing checked out has no submodules charter
    can see, so recursion would cost a process per level to re-report the paths already
    named — and the remedy this pairs with is recursive, so nothing is lost by saying it
    once at the top.
    """
    d = Path(d)
    if not (d / ".gitmodules").exists():
        return [], []
    proc = _git(["submodule", "status"], cwd=d)
    if proc.returncode != 0:
        return [], []
    absent: list[str] = []
    moved: list[str] = []
    for line in proc.stdout.splitlines():
        mark, rest = line[:1], line[1:]
        if mark not in (_SUBMODULE_ABSENT, _SUBMODULE_MOVED):
            continue
        # ``<mark><sha> <path>``, and for a CHECKED-OUT one a trailing `` (<describe>)``.
        # The path starts at the FIRST space, because a submodule path may contain more of
        # them — and the describe is taken off **by the mark**, never by looking at what
        # the path ends with. Those two cannot be told apart by inspection: measured, a
        # submodule at ``tools (x)`` reports as ``-<sha> tools (x)`` while absent and
        # ``+<sha> tools (x) (remotes/origin/HEAD)`` while checked out, so a rule that
        # trims whatever resembles a suffix reads the first one's path as ``tools``.
        #
        # `rpartition` rather than a search for the first ``" ("``: the describe is always
        # LAST, which is what makes ``tools (x) (heads/main)`` come back as ``tools (x)``.
        # A checked-out line always carries one — measured on a submodule with no tags,
        # detached, which still prints ``(remotes/origin/HEAD)`` — so this never guesses.
        path = rest.partition(" ")[2]
        if mark == _SUBMODULE_MOVED:
            path = path.rpartition(" (")[0]
        (absent if mark == _SUBMODULE_ABSENT else moved).append(path)
    return absent, moved


def _submodule_remedy(d: Path) -> str:
    """The exact command, rooted at *d* — relative to the plane when it is inside one, so
    it can be pasted from wherever the operator is reading this."""
    try:
        where = Path(d).relative_to(config.ROOT)
    except ValueError:
        where = Path(d)
    return f"git -C {where} {SUBMODULE_REMEDY}"


def report_submodule_drift(d: Path, label: str, branch: str | None = None,
                           lead: str | None = None) -> bool:
    """Say what *d*'s submodules are not, and name the command that fixes it. Returns
    whether anything was said, so a caller can drop its own success line rather than print
    a tick beside this (`_sync_one`).

    ONE warning line and one remedy line, whatever the mix of states — a caller that wants
    to open with its own situation (`sync` has just claimed something about the branch)
    passes *lead*, and the facts follow it in the same sentence. Two commands reporting the
    same finding in two shapes is how the shapes drift apart.

    **charter reports and does not initialise, and the sentence says so.** Two reasons,
    and the second is the one that settles it.

    A submodule URL comes out of `.gitmodules` — a file inside the repo charter has just
    cloned. `_https_url` refuses to hand `git clone` a string charter did not build (#335,
    where `ext::sh -c '…'` is a transport that runs a command); fetching whatever
    `.gitmodules` names, recursively, would put that same string back one layer down where
    that allowlist cannot see it.

    And charter could not do it under its own rule anyway. **`git clone` does not read the
    local config of the repository it is standing in** — system, global and `-c` only. A
    submodule fetch IS a nested `git clone`, so `gitpolicy.apply`'s ``--local``
    `credential.helper` and ``url.<https>.insteadOf`` never reach it. Measured on git
    2.50.1 against a superproject whose submodule needs a config to be fetchable at all::

        LOCAL  protocol.file.allow in the superproject -> submodule init rc = 1
        -c     on the command line                     -> submodule init rc = 0
        GLOBAL protocol.file.allow                     -> submodule init rc = 0
        LOCAL  submodule.<name>.url override           -> submodule init rc = 0

    The last line is the asymmetry: the PARENT resolves the URL from local config, while
    the CHILD consumes the transport config and its config search skips the surrounding
    repository's local file. So golden rule 0 — every git operation over that repo's own
    forge's token — **does not hold for a submodule fetch**, and an auto-init would be
    charter fetching outside its own credential policy, quietly, on the operator's behalf.
    Saying so leaves that call where it belongs.
    """
    absent, moved = submodule_drift(d)
    if not absent and not moved:
        return False
    bits = []
    if absent:
        bits.append(f"{len(absent)} submodule(s) recorded but not initialised "
                    f"({', '.join(absent)}) — nothing is checked out there, so anything "
                    f"that runs from them fails with 'no such file or directory'")
    if moved:
        bits.append(f"{len(moved)} submodule(s) not at the commit "
                    f"{branch or 'this branch'} records ({', '.join(moved)})")
    facts = "; ".join(bits)
    util.warn(f"{label}: {facts}." if lead is None else f"{label}: {lead} — {facts}.")
    util.info(f"  charter does not fetch them: a submodule URL comes out of the cloned "
              f"repo's own .gitmodules and can point anywhere, and charter's token-only "
              f"policy does not reach a submodule fetch. Yours to run: "
              f"{_submodule_remedy(d)}")
    return True


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
        # …and this is the one case where "your work" may be nobody's work. A submodule
        # left behind the commit the branch records IS an unstaged change to the gitlink,
        # so the previous sync's own fast-forward is enough to make this branch fire
        # forever after: the repo silently stops being synced, and the only sentence
        # charter had for it named changes the operator never made. Reported here for the
        # same reason it is reported below — the skip is honest and unexplained, and the
        # explanation is one `git submodule status` away.
        report_submodule_drift(d, label)
        return
    branch = _git(["rev-parse", "--abbrev-ref", "HEAD"], cwd=d).stdout.strip()
    if _git(["fetch", "--prune"], cwd=d).returncode != 0:
        util.err(f"{label}: fetch failed (access or network) — skipping.")
        return
    ff = _git(["merge", "--ff-only", f"origin/{branch}"], cwd=d)
    if ff.returncode != 0:
        util.warn(f"{label}: {branch} won't fast-forward (diverged/local commits) — left as-is.")
        return
    # The tick is not printed over a tree the fast-forward left behind (#817). A
    # fast-forward moves the GITLINK and never touches the submodule's own checkout, so
    # the commonest outcome here is a branch that is up to date beside a submodule that is
    # not — and `up to date on main` was the whole of what charter said about it. Measured
    # on a clone whose upstream moved one submodule pointer and added a second submodule:
    # `✓ ws/repo: up to date on main`, with the first still at the old commit and an empty
    # directory where the second should be.
    #
    # The branch half of that sentence was true, which is exactly why it had to be said
    # differently rather than merely followed by a warning — a tick is what an operator
    # scans for and stops reading at.
    if report_submodule_drift(d, label, branch,
                              lead=f"{branch} is up to date, its submodules are not"):
        return
    util.ok(f"{label}: up to date on {branch}")


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
    # Sized from the values, measured in CELLS (#592). `{:<38} {:<12}` was two of the
    # three shipped instances of #508's constant, and the STACK one is wrong on a value
    # charter itself produces: `node-monorepo` is thirteen characters in a column of
    # twelve, so every monorepo row PUSHED its branch note one column right of every
    # other row's. A repo name is a directory somebody else cloned and a stack comes out
    # of the inventory, so neither width was ever charter's to guess — and `str.format`
    # counts characters where a terminal lays out cells, so a CJK repo name misaligned
    # its row without going anywhere near either constant.
    #
    # **Measured from the two columns that cost nothing to know, and only those.** A repo
    # name is a dictionary key and a stack is an inventory lookup; the third column runs
    # two `git` invocations per clone. Sizing from all three would mean collecting every
    # row before printing any — on a workspace with twenty clones the operator waits for
    # forty git calls before the header appears, where today the table draws as it goes.
    # Nothing is lost by it: the last column has nothing to its right, so it needs no
    # width at all. That is the same rule `_STATS_HEADS` keeps for its own trailing
    # STATUS column.
    rows = sorted(clones.items())
    stacks = {n: inv_by_name.get(n, {}).get("stack", "?") for n, _ in rows}
    nw = tui.column("REPO", [n for n, _ in rows])
    sw = tui.column("STACK", stacks.values())

    def line(repo, stack, note) -> str:
        """Header and data rows through ONE function. They are sibling rows of the same
        table, and two code paths that each believe they agree about the widths is the
        fastest way back to a misaligned report (#508's own finding, one command over)."""
        return f"  {tui.pad(repo, nw)}{tui.pad(stack, sw)}{note}".rstrip()

    print(line("REPO", "STACK", "BRANCH / NOTE"))
    for name, d in rows:
        print(line(name, stacks[name], _clone_note(d)))
    print()


def _clone_note(d: Path) -> str:
    """The NOTE column of `charter status`'s repo table.

    A submodule that was never initialised leaves an EMPTY DIRECTORY and a clean
    `git status --porcelain` — measured — so without this third fact the row reads
    ``main · clean`` over a tree that is missing the scripts every build target calls.
    The whole point of #817 is that nothing said so, and `status` is where an operator
    looks when something already went wrong.

    It costs a `git submodule status` only for a clone that has a `.gitmodules` at all
    (see `submodule_drift`), so the common row is still the two calls it always was."""
    branch = _git(["rev-parse", "--abbrev-ref", "HEAD"], cwd=d).stdout.strip()
    dirty = "dirty" if _git(["status", "--porcelain"], cwd=d).stdout.strip() else "clean"
    note = f"{branch} · {dirty}"
    absent, moved = submodule_drift(d)
    if absent:
        note += f" · {len(absent)} submodule(s) not initialised"
    if moved:
        note += f" · {len(moved)} out of date"
    return note


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
    # In flight while the fetch runs (#420) — the second of the two callers #387's
    # spinner was promised and never got. Recorded HERE, in the process that does the
    # work, never in the `--detach` branch above: that branch returns immediately, and a
    # record taken there would clear before the child it started had begun. `glstate
    # .maybe_spawn` starts that child on the status line's own render path, so an
    # operator sees the frame move while their forge state is being fetched rather than
    # wondering why a column changed by itself.
    #
    # The WORKSPACE is the agent name, not a repo: one refresh covers every tree in it,
    # and naming one of them would be a claim about which. `kind=inflight.REFRESH` keeps
    # it out of the dispatch nudge and the persona chips.
    from . import inflight
    inflight.start(ws, kind=inflight.REFRESH)
    try:
        cache = glstate.refresh(dirs)
    finally:
        inflight.finish(ws, kind=inflight.REFRESH)
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

# This machine's own harness permissions (`charter guard ask|allow --local`). Its committed
# sibling `.claude/settings.json` is deliberately NOT ignored — that one is the team's.
/.claude/settings.local.json

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
        missing += ["/workspaces/*/*", "!/workspaces/.gitkeep"]
    if ".charter/" not in body:
        missing.append("/.charter/")
    if LOCAL_SETTINGS_IGNORE not in existing_lines:
        missing.append(LOCAL_SETTINGS_IGNORE)
    if not missing:
        return False
    # The write itself goes through the one shared appender — `charter browser install`
    # needs the same append-only, idempotent, whole-line behaviour, and a second
    # implementation of it is how a plane ends up with one rule twice. The DETECTION above
    # stays here on purpose: the `.charter/` substring test and the `!/workspaces/.gitkeep`
    # whole-line test each record a specific bug, and neither generalises. Passing only the
    # lines this already decided are missing makes the two equivalent — a line the
    # substring test finds is never handed over, and one it does not find is absent as a
    # whole line too.
    util.append_gitignore(root, missing, "added by `charter init`")
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


def _plugin_dispatches_guard(root: Path) -> str | None:
    """The enabled plugin whose own ``hooks.json`` dispatches `charter hook pretooluse`.

    *root* is the directory whose ``enabledPlugins`` decides it, and it is passed rather
    than inherited because doctor's default is no longer the plane (#851): every row there
    now answers for the directory the SESSION is rooted in, which is where the host reads
    settings from. This caller is not asking about a session — it is about to write
    ``root/.claude/settings.json``, so the enablement that matters is the one recorded in
    that same file. Without the argument, `charter reinit` typed from a subdirectory would
    miss the plugin enabled in the plane's settings and write a second declaration into it
    — the doubling this function exists to prevent, arrived at from a new direction.

    Deliberately NOT "is a charter plugin enabled". `doctor._plugin_declaring_guard`
    records why, and 0.43.1 got it wrong by reading `enabledPlugins` instead: **installed,
    enabled and wired are three different states, and only the third protects anything**
    (#177). A plugin from before the guard existed is enabled and dispatches nothing — and
    skipping the hook for it would leave the plane root unguarded while looking configured.

    Shared with `doctor.check_guard_wired` on purpose. A writer and a checker answering
    "is this wired?" from different evidence is how the guard came to be declared twice.
    """
    from . import doctor

    return doctor._plugin_declaring_guard(root)


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
    # An ENABLED charter plugin already declares this hook, and `doctor.check_guard_wired`
    # counts it as wired for exactly that reason. Writing one here too leaves both live, so
    # `charter hook pretooluse` runs twice for every Bash call — the same doubling Codex
    # got from two installers, arrived at here by one writer and one checker disagreeing
    # about what "wired" means in the same file.
    if _plugin_dispatches_guard(root):
        return "present", p
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
               "Task")

#: A rule already in the host's ``Tool(pattern)`` form.
_TOOL_RULE_RE = re.compile(r"^(?:%s)\(.*\)$" % "|".join(_RULE_TOOLS), re.DOTALL)

#: An MCP rule: a whole server (``mcp__slack``) or one of its tools (``mcp__slack__send``).
#: Claude Code's MCP permission syntax is exactly this — a bare name, never parenthesised
#: and never wildcarded — which is why it needs a shape of its own here.
_MCP_RULE_RE = re.compile(r"^mcp__[A-Za-z0-9_-]+$")


class UnexpressibleRule(ValueError):
    """A pattern that names a rule syntax and then asks for something it cannot say.

    Raised rather than guessed because both guesses fail silently: wrapping produces
    `Bash(mcp__slack__send *)`, which matches a bash command by that literal name, and
    writing it bare produces an MCP rule the host will not match either. A rule charter
    cannot express is the one case where refusing beats writing something.
    """


def _as_rule(pattern: str) -> str:
    """A permission rule for *pattern*, wrapping a bare command in ``Bash(...)``.

    Tests the **shape** of the rule, never the prefix of the string. The prefix test this
    replaces required a parenthesis (`"(" in p`), which excluded the one family that cannot
    carry one: `charter guard ask 'mcp__slack__send'` wrote `Bash(mcp__slack__send)` and
    printed a tick (#365).

    Dropping the parenthesis requirement would have mirrored the bug rather than fixed it.
    `str.startswith` matches raw prefixes, so `Globalprotect --connect` starts with `Glob`
    and `Taskwarrior add x` starts with `Task`; both are ordinary binaries, and writing
    either bare would produce a rule matching nothing in the other direction. The three
    shapes below are what a rule can actually be — `Tool(pattern)`, a bare tool name, a bare
    MCP name — and everything else is a command.
    """
    p = (pattern or "").strip()
    if _TOOL_RULE_RE.fullmatch(p) or p in _RULE_TOOLS or _MCP_RULE_RE.fullmatch(p):
        return p
    if p.startswith("mcp__"):
        raise UnexpressibleRule(
            f"charter cannot write {p!r} as a permission rule. An MCP rule names a server "
            f"(`mcp__slack`) or one of its tools (`mcp__slack__send`) exactly — no "
            f"wildcard and no arguments. Wrapping it as `Bash(...)` would write a rule "
            f"that matches nothing, so charter writes nothing.")
    return f"Bash({p})"


#: The one status in a `guard` result that no harness returns — charter's own word for a
#: file that PASSED the check and then would not take the write. `Harness.apply_ask_rule`
#: owns the other four; this one is produced by `_guard_apply` from an `OSError`, because
#: only the caller knows the write was part of a transaction that had already committed
#: elsewhere. Kept distinct from ``malformed`` on purpose: that one means the content is
#: unparseable, and a file charter could not write is usually perfectly valid.
_UNWRITABLE = "unwritable"


def _say_write_failed(name: str, detail: str) -> None:
    """Report a harness whose file passed the check and then refused the write.

    Deliberately does not name a cause. *detail* carries the OS's own `strerror` beside the
    path, which is the only account of it charter has; guessing between permissions,
    ownership and a full disk would be the ADR 0009 failure of naming a cause charter only
    inferred. The remedy is honest and does not need the cause: this is not charter's file
    to repair, and re-running is what applies the rule once the write can happen.
    """
    util.err(f"{name}: could not write {detail} — left untouched.")
    util.info("  charter asked every harness before writing any of them, so this arrived "
              "AFTER the check. Re-run once that file can be written.")


def _guard_apply(method: str, root: Path, pattern: str,
                 local: bool) -> tuple[list[tuple], bool]:
    """Give *pattern* to every harness that can hold it, or to none of them (#376).

    Returns ``(results, blocked)`` — one ``(harness, status, detail)`` per registered
    harness in registration order, and whether the command wrote nothing because a harness
    refused. Shared by both verbs through *method*, so `ask` and `allow` cannot come to
    disagree about when a command has half-happened.

    **Two phases, because aborting on the first refusal is not enough.** Claude Code is
    first in the registry; by the time opencode refuses, Claude Code's file is written.
    So every harness is asked first with ``dry_run=True`` and nothing is committed unless
    all of them can take it. The check is the write path minus the write rather than a
    validator of its own — `Harness.apply_ask_rule` records why.

    **Only ``malformed`` blocks**, and that is the design rather than an implementation
    detail. `unsupported` is a harness saying it has no such rule to write at all, which
    is permanent: Codex answers it to every pattern, and opencode to every `--local` one,
    so a transaction that stopped there would write nothing anywhere, forever. A rule
    absent because a file is broken is somebody's five-minute fix; a rule absent because a
    harness has nowhere to put it is a standing fact `guard` states rather than resolves.

    **Permanent is not the same as harness-wide, and #374 is why that matters here.**
    opencode now also answers `unsupported` to one PATTERN — a `FLAT_ONLY_PERMISSIONS` key
    with a real glob, `WebFetch(https://x/*)`, which opencode's config genuinely cannot
    say. The line above still holds and the reason is unchanged: re-running never changes
    it, so it is reported and stepped over, and Claude Code takes the rule it CAN express.
    Worth saying because the enumeration used to read as a property of the harness alone,
    and a later reader tempted to key the transaction off "does this harness ever support
    anything" would now be keying it off the wrong question.

    What this does NOT claim: the commit phase writes several files in sequence with no
    rollback primitive, so an IO failure between them still leaves the plane uneven. That
    residue is why `_say_if_uneven` survives — it went from the ordinary outcome to the one
    charter cannot rule out.

    **An IO failure is REPORTED, not raised.** The paragraph above was the design from the
    start, and it was not true of the code: a `.claude/settings.json` or `opencode.json`
    that parses and cannot be written — read-only, wrong owner, full disk — reached
    `write_text` and the `OSError` escaped as a traceback, after the earlier harnesses had
    already been written. The plane was split and charter said nothing at all, which is the
    #369 failure with the message removed rather than moved. The commit call is wrapped
    here, at the one place that knows how far the transaction had got, and the failure
    becomes :data:`_UNWRITABLE` so it flows into the same uneven-landing report as a file
    that changed underneath the command.

    It is a status of charter's own, not a fifth answer asked of harnesses, and it is
    deliberately NOT reported as ``malformed``: the file parses. Telling an operator their
    valid JSON "is not valid" would send them to fix content that is fine, and the issue
    names *malformed OR unwritable* as two triggers, not one.

    The wrap is on the COMMIT phase alone. A check that fails this way is unreachable —
    `dry_run` opens nothing for writing, and every harness already answers ``malformed``
    to an `OSError` while reading — so a branch for it would be a branch no test can drive.
    """
    from .harness import registry

    checked = [(h, *getattr(h, method)(root, pattern, local=local, dry_run=True))
               for h in registry.all()]
    if any(status == "malformed" for _h, status, _detail in checked):
        return checked, True
    committed = []
    for h, status, detail in checked:
        if status == "added":
            try:
                status, detail = getattr(h, method)(root, pattern, local=local)
            except OSError as e:
                status = _UNWRITABLE
                detail = f"{detail} ({e.strerror or e.__class__.__name__})"
        committed.append((h, status, detail))
    return committed, False


def _say_nothing_landed(results: list[tuple], pattern: str) -> int:
    """Report a `guard` command that wrote nowhere, and return its exit code.

    The `✗` used to sit beside a `✓`, and the operator had to work out which harnesses now
    disagreed (#369). It now means what every reader already assumed it meant, and saying
    so out loud is the point: somebody taught by the old behaviour goes and checks the
    other files otherwise, which is exactly the doubt the transaction exists to remove.

    **"Nothing was written" is not "the rule is nowhere", and the difference had to be said
    out loud.** This command aborts on a broken file whether or not an EARLIER command
    already put the rule in place, and the commonest way to meet a broken file is to hit it
    on a re-run — a bad merge, a teammate's machine. In that state the plane really is
    uneven: `.claude/settings.json` holds the rule and `opencode.json` does not. The first
    draft of this reported only what THIS command did, and an operator reading it had no way
    to tell that case from a first attempt that reached nowhere. That was strictly less than
    the per-harness `✓ already asking for …` the old half-writing loop printed — a
    transaction that says less about the plane than the split it replaced. So both halves
    are named: where the rule already stands, and where it would have gone.

    **And a rule that already stands still outranks whatever it outranked (#374).** This
    path replaces the loop, so it also replaces the loop's `_warn_if_outranking` call, and
    dropping it silently was a real regression rather than a theoretical one: with
    `.claude/settings.json` malformed and `opencode.json` already holding `plan_*`,
    `charter guard allow mcp__plan` is blocked, and two of opencode's OWN denies are
    allowed right now by a rule this operator is being told nothing about. Measured against
    a `git archive origin/main` export, same plane, same command — main printed the line.

    It fires on ``present`` ALONE, and not on the same two statuses the loop uses. That
    filter is `_warn_if_outranking`'s "the rule is in force", and here only half of it
    still holds: nothing was written, so an ``added`` harness's rule is NOT in force and
    warning about its consequences would describe a rule that does not exist. Printed
    after the "ALREADY in force" line and well below the malformed one, so the sentence
    that needs acting on stays first — the other half of that filter's reasoning.
    """
    for h, status, detail in results:
        if status == "malformed":
            util.err(f"{h.name}: {detail} is not valid — left untouched.")
            util.info("  Fix it by hand, then re-run. charter never repairs these files.")
    util.warn("  NOTHING was written, under any harness — `charter guard` is all-or-nothing "
              "across the harnesses that can hold a rule, so the plane is exactly as it was "
              "before this command.")
    already = [h.name for h, status, _d in results if status == "present"]
    if already:
        util.info(f"  The rule is ALREADY in force under {', '.join(already)}, from an "
                  f"earlier command — this run neither added it nor took it away. The plane "
                  f"is uneven right now, and fixing the file above is what evens it up.")
    for h, status, _d in results:
        if status == "present":
            _warn_if_outranking(h, pattern, status)
    pending = [h.name for h, status, _d in results if status == "added"]
    if pending:
        util.info(f"  {', '.join(pending)} would have taken the rule and did not. Re-run "
                  f"once the file above parses.")
    return 1


def _say_where_it_cannot_reach(results: list[tuple]) -> None:
    """Name the harnesses with nowhere to put the rule, as a limit and not as a failure.

    All-or-nothing across the harnesses that CAN hold a rule leaves exactly one way for a
    rule to be in force unevenly, and it is the honest one — so the uneven landing still
    has to be visible, just no longer as a defect. Each such harness prints its own reason
    above; what this adds is the word that stops a reader filing it beside the broken case.
    Their remedies are opposites: a malformed file is fixed and the command re-run, and
    re-running does nothing whatever here.

    ``unsupported`` ONLY, and that is load-bearing rather than an accident of which status
    was handy. Every other answer means the harness has somewhere to put the rule: widening
    this to include ``present`` would print "Not in force under claude-code" over a rule
    that is in force under claude-code — on the second run of any command, i.e. constantly.

    The sentence deliberately says nothing about WHY, and #374 is what makes that pay. The
    reason now varies by pattern as well as by harness — Codex has no command patterns at
    all, opencode has no machine-local file, and since #374 opencode also cannot put a URL
    glob under a `FLAT_ONLY_PERMISSIONS` key — and each harness has already printed its own
    account of it on the line above. This adds only the word that files it as a limit
    rather than a break; assembling the reason here would mean restating three of them, and
    getting one wrong the day a fourth appears.
    """
    names = [h.name for h, status, _d in results if status == "unsupported"]
    if not names:
        return
    util.info(f"  Not in force under {', '.join(names)} — nowhere to write it there, which "
              f"is a standing limit of the harness rather than a failed write. Re-running "
              f"will not change that.")


def _say_if_uneven(wrote: bool, refused: bool) -> None:
    """Say so when one command left the rule in force under some harnesses and not others.

    This was the ORDINARY outcome of a malformed config file: the refusal was per-harness,
    so a `✗` beside a `✓` meant one invocation left the plane holding the rule under
    opencode and not under Claude Code, and an operator reading the `✗` as "nothing was
    written" (#369) then re-ran after the fix and made opencode's copy a duplicate write.
    `_guard_apply` closed that: a harness that would refuse is found before anything is
    written, and the command writes nowhere.

    It stays because the closure is not total, and pretending otherwise would be the same
    species of overstatement. The commit phase writes several files one after another with
    no rollback primitive, so a file that changed underneath the command, or an IO error
    between two writes, still splits the plane. That is now the only way here, and it is
    the one charter cannot rule out — which makes this message rarer and more important,
    not obsolete. Shared by both verbs so `ask` and `allow` cannot describe it differently.

    The message names BOTH shapes of late refusal rather than the first one, because both
    reach here and the second one used to reach nowhere: an unwritable file raised out of
    `_guard_apply` before any of this ran. Naming the two is not charter guessing between
    them — the line immediately above says which it was, in its own words ("is not valid"
    against "could not write"). Saying only "changed underneath" would have been the wrong
    cause printed with total confidence, for the case this round added.
    """
    if wrote and refused:
        util.warn("  The rule landed UNEVENLY — it is in force under the harnesses ticked "
                  "above and NOT under the one refused, from this one command. Every "
                  "harness accepted it when charter asked, so the refusal above arrived "
                  "AFTER the check: that file changed underneath this command, or would "
                  "not take the write. Fix what it names and re-run; the harnesses that "
                  "already have it will say so.")


def _refuse_unexpressible(pattern: str) -> str | None:
    """Why *pattern* cannot be written as a rule at all, or ``None``.

    Asked once, BEFORE the harness loop, so a pattern charter cannot express never lands
    under one harness and not another — the split #369 records for the malformed-file case.
    `_as_rule` stays the single authority on what is expressible; this only decides when to
    ask it, so the command and the translator cannot come to different answers.
    """
    try:
        _as_rule(pattern)
    except UnexpressibleRule as e:
        return str(e)
    return None


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


def _load_json_settings(path: Path):
    """`(doc, path)` for a settings file, or `(None, path)` when it is not usable.

    `_load_settings` answers this for the plane's committed file specifically. This is the
    same restraint for any path: a missing file is an empty document, an unparseable one is
    somebody's to fix and charter reports it rather than overwriting it.
    """
    if not path.exists():
        return {}, path
    try:
        doc = json.loads(path.read_text())
    except (OSError, UnicodeDecodeError, json.JSONDecodeError):
        return None, path
    return (doc, path) if isinstance(doc, dict) else (None, path)


def add_permission_rule(root: Path, rule: str, bucket: str, local: bool = False,
                        dry_run: bool = False) -> tuple[str, str]:
    """Append *rule* to ``permissions.<bucket>`` in the plane's `.claude/settings.json`.

    Extracted from `cmd_guard_ask` when a second harness needed the same command: the
    command now asks each registered harness to write its own file in its own syntax,
    and this is Claude Code's half. Refuses a malformed file rather than repairing it,
    and a `permissions` or bucket of the wrong type is somebody's deliberate structure —
    charter reports it and writes nothing HERE.

    "Writes nothing here" is the whole of the promise, and this function is deliberately
    still not where the promise gets any bigger. The refusal remains per-harness — it
    returns ``malformed`` and knows nothing about the other harnesses — but the CALLER no
    longer steps over it: `_guard_apply` asks every harness with `dry_run=True` and writes
    nowhere if any of them answers this way (#376), so a malformed Claude Code file no
    longer leaves the rule in force under opencode. The docstring first claimed the stop
    happened here ("reports it and stops"), then recorded that it did not happen at all
    (#369). It happens one level up, where something can see every harness at once.

    Parameterised over the bucket when `guard allow` arrived: `ask` and `allow` are the
    same job with opposite verbs, and two copies of this function would eventually
    disagree about what "malformed" means — in a file charter only half-owns.

    `local=True` targets `.claude/settings.local.json` instead — gitignored by `charter
    init`, so the rule is one person's, on one machine. Same reasoning one level down: the
    two files differ only in blast radius, so they must not differ in how carefully they
    are parsed or how firmly a malformed one is refused.

    `dry_run=True` returns the answer and touches nothing — including `.gitignore`, which
    `local` would otherwise amend before the settings file is even read. A check with a
    side effect is not a check: the plane has to be exactly as it was when a transaction
    decides not to commit.
    """
    if local and not dry_run:
        _ensure_local_settings_ignored(root)
    settings, path = (_load_json_settings(Path(root) / LOCAL_SETTINGS) if local
                      else _load_settings(root))
    if settings is None:
        return "malformed", str(path)
    perms = settings.setdefault("permissions", {})
    if not isinstance(perms, dict):
        return "malformed", f"{path} (`permissions` is not an object)"
    entries = perms.setdefault(bucket, [])
    if not isinstance(entries, list):
        return "malformed", f"{path} (`permissions.{bucket}` is not a list)"
    if rule in entries:
        return "present", str(path)
    if dry_run:
        return "added", str(path)
    entries.append(rule)
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(json.dumps(settings, indent=2) + "\n")
    return "added", str(path)


#: The plane's MACHINE-LOCAL settings — a rule written here is one person's decision on one
#: machine. `.claude/settings.json`, its committed sibling, carries the same key names and a
#: completely different blast radius.
LOCAL_SETTINGS = Path(".claude") / "settings.local.json"

#: The `.gitignore` line that makes :data:`LOCAL_SETTINGS` actually local. Written by the
#: baseline for a fresh plane AND ensured by `_ensure_local_settings_ignored` at the moment a
#: local rule is written, because a plane created before this existed has neither.
LOCAL_SETTINGS_IGNORE = "/.claude/settings.local.json"


def _ensure_local_settings_ignored(root: Path) -> None:
    """Make `--local`'s promise true in THIS plane before relying on it.

    The flag shipped claiming `charter init` gitignored the file. It did not — charter's own
    repo has that line by hand, and checking there and generalising is how the claim was
    made. In a fresh plane the file was committable, so the one command the flag exists for,
    `charter guard allow --local 'gh pr merge *'`, would have landed in the next commit as
    the team-wide rule it was chosen to avoid.

    Fixing the baseline alone would leave every plane created before today broken, which is
    precisely where somebody reaches for `--local` and is silently let down. So the guarantee
    lives at the point of use as well: additive, idempotent, and never rewriting a line
    anybody else put there.
    """
    p = Path(root) / ".gitignore"
    body = p.read_text() if p.exists() else ""
    if LOCAL_SETTINGS_IGNORE in {ln.strip() for ln in body.splitlines()}:
        return
    util.append_gitignore(root, [LOCAL_SETTINGS_IGNORE],
                          "added by `charter guard --local`")


def add_ask_rule(root: Path, rule: str, local: bool = False,
                 dry_run: bool = False) -> tuple[str, str]:
    """`permissions.ask` — force a prompt for *rule*."""
    return add_permission_rule(root, rule, "ask", local=local, dry_run=dry_run)


def add_allow_rule(root: Path, rule: str, local: bool = False,
                   dry_run: bool = False) -> tuple[str, str]:
    """`permissions.allow` — stop prompting for *rule*."""
    return add_permission_rule(root, rule, "allow", local=local, dry_run=dry_run)


def cmd_guard_allow(args) -> int:
    """Add a stop-prompting rule to `permissions.allow` in the plane's settings.

    The mirror of `cmd_guard_ask`, and deliberately its twin rather than its variation:
    same ADR 0014 reasoning, same harness registry, same "charter is the editor, not the
    author" boundary — the operator named the rule and the command that writes it.

    It exists because charter could only ever make a command prompt MORE. `toolgate` is
    allow-only but keyed on the active persona's declared `tools:`, which is the right
    instrument for "this persona may run `glab`" and the wrong one for "this plane never
    needs to be asked about `git status`". That gap is why routine git felt hook-guarded
    when charter denies only three narrow things (#291).
    """
    pattern = (getattr(args, "pattern", "") or "").strip()
    local = bool(getattr(args, "local", False))
    if not pattern:
        util.err("Nothing to add. Example: charter guard allow 'git status *'")
        return 2
    unexpressible = _refuse_unexpressible(pattern)
    if unexpressible:
        util.err(unexpressible)
        return 2
    from .harness import registry

    root = Path(config.ROOT)
    results, blocked = _guard_apply("apply_allow_rule", root, pattern, local)
    if blocked:
        return _say_nothing_landed(results, pattern)
    rc, wrote = 0, False
    for h, status, detail in results:
        if status == "added":
            util.ok(f"{h.name}: allowing {h.rule_text(pattern)} → {detail}")
            wrote = True
        elif status == "present":
            util.ok(f"{h.name}: already allowing {h.rule_text(pattern)}.")
            wrote = True
        elif status == "malformed":
            util.err(f"{h.name}: {detail} is not valid — left untouched.")
            util.info("  Fix it by hand, then re-run. charter never repairs these files.")
            rc = 1
        elif status == _UNWRITABLE:
            _say_write_failed(h.name, detail)
            rc = 1
        else:
            util.info(f"  {h.name}: {detail} — nothing to relax there.")
        _warn_if_outranking(h, pattern, status)
    if not wrote and rc == 0:
        util.warn("  No harness took the rule.")
    if wrote:
        if local:
            util.info("  Machine-local (gitignored) — this rule is yours, on this machine.")
        else:
            # Deliberately not the reassuring ADR 0014 line `guard ask` prints. An ASK rule
            # narrows what happens without a human, so sharing it is conservative. An ALLOW
            # rule widens it, and sharing extends one person's trust decision to everyone
            # who clones the repo, on machines and under identities they did not choose.
            util.warn("  COMMITTED — this stops the prompt for everyone on this repo, not "
                      "just you. Use `--local` for a rule that is yours alone.")
        # The claim is accurate and stays. What was missing is the next sentence: a reader
        # who has just been told this command cannot reach charter's denials is exactly the
        # reader about to go looking for something that can, and until #370 there was
        # nothing to find and nowhere saying so.
        util.info("  charter's own guards are unaffected: an allow rule relaxes the HOST's "
                  "prompt, never the secret, credential, plane-root or release denials.")
        util.info("  Nothing lifts those — by design. `charter docs show hooks` → *When a "
                  "guard is wrong* for what to do when one of them is wrong about you.")
    _say_where_it_cannot_reach(results)
    _say_if_uneven(wrote, rc == 1)
    return rc


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
    local = bool(getattr(args, "local", False))
    if not pattern:
        util.err("Nothing to add. Example: charter guard ask 'terraform apply *'")
        return 2
    unexpressible = _refuse_unexpressible(pattern)
    if unexpressible:
        util.err(unexpressible)
        return 2
    from .harness import registry

    root = Path(config.ROOT)
    results, blocked = _guard_apply("apply_ask_rule", root, pattern, local)
    if blocked:
        return _say_nothing_landed(results, pattern)
    rc, wrote = 0, False
    for h, status, detail in results:
        if status == "added":
            util.ok(f"{h.name}: asking for {h.rule_text(pattern)} → {detail}")
            wrote = True
        elif status == "present":
            util.ok(f"{h.name}: already asking for {h.rule_text(pattern)}.")
            wrote = True
        elif status == "malformed":
            util.err(f"{h.name}: {detail} is not valid — left untouched.")
            util.info("  Fix it by hand, then re-run. charter never repairs these files.")
            rc = 1
        elif status == _UNWRITABLE:
            _say_write_failed(h.name, detail)
            rc = 1
        else:
            # Not a failure. The harness has no command-pattern permissions, so charter's
            # own hook stays the only thing guarding this command there — which is worth
            # saying, because silence would read as "the rule is in force everywhere".
            util.info(f"  {h.name}: {detail} — charter's own guard still applies.")
        _warn_if_outranking(h, pattern, status)
    if not wrote and rc == 0:
        util.warn("  No harness took the rule.")
    if wrote:
        if local:
            util.info("  Machine-local (gitignored) — this rule is yours, on this machine.")
        else:
            util.info("  These files are committed, so the rule applies to everyone on this "
                      "repo — no sync step, and nothing that can drift (ADR 0014).")
        _warn_if_shadowing(registry.get(registry.CLAUDE_CODE).ask_rule(pattern))
    _say_where_it_cannot_reach(results)
    _say_if_uneven(wrote, rc == 1)
    return rc


#: The buckets `charter guard` writes, in the host's own evaluation order.
#:
#: `deny` is deliberately absent. Charter never writes it, and it answers neither question
#: an operator opens this command with — "what has charter put in my permissions" and "what
#: is currently not prompting me".
_LISTED_BUCKETS = ("ask", "allow")


def _rules_in(doc, bucket: str) -> list[str]:
    """The string rules in ``permissions.<bucket>``, tolerating every other shape.

    A `permissions` block or a bucket of the wrong type is somebody's deliberate structure,
    which `add_permission_rule` refuses to write into. A *reader* has less standing still:
    it reports what it can read and never raises over what it cannot.
    """
    perms = doc.get("permissions") if isinstance(doc, dict) else None
    entries = perms.get(bucket) if isinstance(perms, dict) else None
    return [r for r in entries if isinstance(r, str)] if isinstance(entries, list) else []


def cmd_guard_list(args) -> int:
    """Show the plane's guard rules — read from the host's files, not a charter one.

    Both buckets, from both files. This read `permissions.ask` alone, and bare `charter
    guard` defaults to it, so the command an operator reaches for to answer "what has
    charter put in my permissions" showed the conservative half and hid the widening one
    (#368). An ask rule narrows what happens without a human; an allow rule widens it, which
    is why `cmd_guard_allow` shouts `COMMITTED` when it writes one — and why that half being
    the invisible one was the wrong way round.

    **Grouped by file, because the file a rule lives in IS its blast radius.** A flat list
    with a label would ask the reader to trust the label; a heading makes it structural. The
    machine-local file was invisible here for the same reason the allow bucket was — the
    reader only ever opened one — so `--local` rules had no reader at all.

    Read through the same two loaders `add_permission_rule` writes through, so the reader
    and the writer cannot come to different conclusions about what a file contains. A
    malformed file is named and does not suppress the other: reporting nothing because one
    of two files is unparseable would hide rules that are in force, which is the silent
    direction this command was already failing in.
    """
    root = Path(config.ROOT)
    committed, committed_path = _load_settings(root)
    local, local_path = _load_json_settings(Path(root) / LOCAL_SETTINGS)
    files = ((committed, committed_path, "committed — everyone on this repo"),
             (local, local_path, "machine-local — yours alone, on this machine"))
    rc, shown = 0, False
    for doc, path, blast_radius in files:
        if doc is None:
            util.err(f"{path} is not valid JSON — skipped, so this listing is incomplete.")
            rc = 1
            continue
        rules = [(b, r) for b in _LISTED_BUCKETS for r in _rules_in(doc, b)]
        if not rules:
            continue
        print(f"  {path}  ({blast_radius})")
        for bucket, rule in rules:
            print(f"    {bucket:<6} {rule}")
        shown = True
    if not shown and rc == 0:
        util.info("No guard rules in this plane. "
                  "Add one: charter guard ask 'terraform apply *'")
    return rc


def _warn_if_outranking(h, pattern: str, status: str) -> None:
    """Say at write time when the rule also decides something the HARNESS had decided.

    The sibling of `_warn_if_shadowing`, one layer down: that one is about charter's own
    tool-gate being outranked, this one about the *host's* built-in permissions being
    outranked. ADR 0014 has charter write the host's rules rather than keep its own, so
    charter's entry lands in the same table the host seeded — and where the names
    collide, the operator's sentence quietly decides something they never named.
    `charter guard allow mcp__plan` is the real case: under opencode it turns two
    built-in denies into allows (see `OpenCodeHarness.rule_outranks`).

    Only on `added` and `present`, because those are the two statuses that mean the
    rule is in force. Warning after `unsupported` would describe a consequence of a rule
    that was not written, and after `malformed` it would bury the line that needs acting
    on.

    The sentence comes from the harness. Assembling it here would put one harness's
    resolution order — last-match-wins — into a loop that runs for all of them.
    """
    if status not in ("added", "present"):
        return
    said = h.rule_outranks(pattern)
    if said:
        util.warn(f"  {h.name}: {said}")


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


def _wire_harnesses(root: Path) -> list[tuple[str, str]]:
    """Write every REGISTERED harness's wiring under *root*, IF ABSENT.

    Every harness, not the detected one: `init` typically runs from a plain shell where
    no harness is detectable at all, and wiring only the runtime you happen to be sitting
    in makes the plane you built depend on which terminal you built it from. Writing one
    harness's wiring unconditionally while another waits to be asked is also the lock-in
    ADR 0015 removes.

    Nothing here names a harness. Adding Codex is adding a class to
    ``harness.registry.KINDS`` — this loop, and `init`'s report, cover it that day.

    A pair whose status is ``"unvouched"`` carries a SENTENCE, not a path — see
    :meth:`Harness.wire`. Both callers warn about those instead of listing them, which is
    the difference between "already present" and "this file is where your guard should be
    and charter cannot vouch for it".
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


#: The front door `init` scaffolds. Generic by construction: it names no other persona,
#: because on a fresh plane there are none, and it says so rather than pretending.
#:
#: Every line is a true statement about this persona — the same rule `_TEMPLATE` in
#: commands_persona follows, and for the same reason: whatever is written here is read by
#: an agent as its actual remit, not as advice to whoever edits the file later.
#:
#: The prose does the work the frontmatter cannot. A front door whose charter reads as
#: though doing the work itself were normal would ship the exact failure this design
#: exists to fix, pre-installed, in the file every consumer copies from. So it says: you
#: have nobody to route to yet, here is how to get someone.
_FRONT_DOOR = """---
name: {name}
role: {role}
vault: none
routing: advise
delegate-when: routing work to the right persona, and scoping a request before code is written
---

# {role}

You are the front door of this control plane. Your job is to understand what is actually
being asked, then either do it or hand it to the persona that owns it — not to start
editing the first file that looks relevant.

## Routing

This plane has no other personas yet, so there is nothing to route to. That is the first
thing worth fixing, not a reason to do everything here:

```
charter persona create <name> --role "<Role>" \\
  --delegate-when "<the work that should come to it>"
```

`delegate-when` is what makes a persona findable — it becomes the description whoever is
routing reads. Create one the moment a second kind of work appears in this plane.

Once others exist, `routing: advise` above puts them in front of you on work-shaped
prompts: who exists, what each claims, when each was last dispatched. charter never says
which one owns the request — that call is yours. Route on the *work*, not on the file a
change happens to touch.

Cross-cutting changes stay with you: splitting one coherent change across three personas
costs more in lost context than it saves.

## Scout before you scope

Read the thing before proposing a change to it, and check what this plane already knows —
`charter recall "<keywords>"` searches your memory, the shared namespace and the active
workspace's journal at once.

## What you own

Personas, workspaces, memory and vaults — the shape of this plane. Definitions and memory
are committed and shared; credentials never are.

Record durable facts with `charter persona remember {name} "<fact>"`, and `--shared` for
anything every persona needs.

This file is yours: rename it, rewrite it, or delete it and declare a different front door
with `charter persona default <name>`.
"""


def _ensure_front_door(root: Path, name: str | None) -> tuple[str, str] | None:
    """Scaffold the generated front-door persona and declare it. Returns
    ``(status, label)`` for init's created/present report, or ``None`` when it did nothing.

    Skipped entirely when the plane already has ANY persona: `init` creates only what is
    absent, and a roster is not absent because one particular name is. Skipped too when a
    default is already declared — someone has already answered this question.

    Writes through explicit paths under *root* rather than `config.PERSONAS_DIR`, like
    every other writer in this command: `init` runs before the plane it is creating exists,
    so the derived globals may still point somewhere else entirely.
    """
    if not name:
        return None
    personas = root / "personas"
    if any(personas.glob("*/persona.md")) or any(personas.glob("*.md")):
        return None
    from . import instance as _instance, persona as _persona
    if _instance.default_persona_of(_instance.load(root)):
        return None
    if not _persona.valid_name(name):
        util.warn(f"--front-door {name!r} is not a valid persona name — skipped.")
        return None
    d = personas / name
    d.mkdir(parents=True, exist_ok=True)
    role = f"{name.replace('-', ' ').replace('_', ' ').title()}"
    (d / "persona.md").write_text(_FRONT_DOOR.format(name=name, role=role))
    for sub in ("memory", "refs"):
        (d / sub).mkdir(parents=True, exist_ok=True)
        (d / sub / ".gitkeep").touch()
    _instance.set_default_persona(root, name)
    return ("created", f"personas/{name}/ (front door, declared in charter.toml)")


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
    unvouched: list[str] = []

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
        # "unvouched" is a SENTENCE, not an item — a harness reporting something in its
        # wiring that charter did not write. It used to land in `present`, which is how a
        # shim with every guard cut out of it got listed as "already present" (#433).
        if status == "unvouched":
            unvouched.append(label)
        else:
            (created if status == "created" else present).append(label)

    fd = _ensure_front_door(root, getattr(args, "front_door", None))
    if fd:
        created.append(fd[1])

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
    # After the inventory and before "Next:", so it is the last thing said about what is
    # installed. A harness's wiring charter could not vouch for used to be silent here.
    for why in unvouched:
        util.warn(why)
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

    unvouched: list[str] = []
    for status, label in _wire_harnesses(config.ROOT):
        if status == "created":
            created.append(label)
        elif status == "unvouched":
            unvouched.append(label)

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

    # BEFORE the headline, and never swallowed by it. `doctor`'s hint for a plugin charter
    # cannot vouch for names `charter reinit`, and reinit answered "Up to date — nothing to
    # do" while the file sat there unreadable — a remedy that reports success and changes
    # nothing, which ends the investigation. reinit still does not overwrite it (charter
    # reports, never repairs); it now says so, and says what would.
    for why in unvouched:
        util.warn(why)

    if not created:
        if unvouched:
            util.ok(f"Up to date (schema {_instance.SCHEMA}) — nothing to add. The "
                    f"warning above is not something `reinit` can fix for you.")
            return 0
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
        # The NAME column is stated BEFORE the run, not measured from it: this loop is
        # written to draw each row as its check lands, so there is no completed table to
        # size from — `doctor.name_width` asks the checks what they are called instead of
        # a `:<16` guessing (#600). `Result.render` treats it as a floor, so a name the
        # width did not know about pushes its own row rather than being cut out of the
        # report. (`doctor._checks` is an eager list today, so nothing actually streams
        # yet; sizing from the results would make that unfixable rather than merely
        # unfixed — see `_FIXED_CHECK_NAMES`.)
        name_w = doctor.name_width()
        results = []
        for r in doctor.iter_all():
            results.append(r)
            print(r.render(name_w), flush=True)
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
    # Sized from the values it holds, measured in CELLS (#592). This column was ALREADY
    # sized from the data, which is the half a constant gets wrong — and it still
    # misaligned, because `len` counts characters and a terminal lays out cells. A base
    # label is a persona or workspace directory name, so a CJK one is two cells per glyph
    # and `len` under-pads it by its own length; a combining mark is zero cells and `len`
    # over-pads. Either way the TITLE column lands somewhere on that row it lands on no
    # other. Sizing and padding have to be the same unit or the arithmetic was for
    # nothing, which is why `tui.pad` fills these rather than `{:<n}`.
    dates = [h.date.isoformat() if h.date else "—" for h in results]
    dw = tui.column("", dates)
    lw = tui.column("", [h.label for h in results])
    full = getattr(args, "full", False)
    # The address line hangs under the TITLE, so its indent is the two columns in front of
    # it and not a number that happens to equal them today. It was `14` written out, which
    # is `2 + 10 + 2` — correct only while the date column is exactly ten wide.
    hang = " " * (2 + dw)
    for h, date in zip(results, dates):
        tag = f"  ({h.score})" if h.score else ""
        # The date leads because when listing it IS the sort key — and the column order
        # stays identical under a query (where score orders instead), since a layout that
        # rearranges itself per mode is harder to read than one that never moves.
        print(f"  {tui.pad(date, dw)}{tui.pad(h.label, lw)}{h.title}{tag}".rstrip())
        # The ADDRESS, on every hit. This used to be a closing sentence telling the reader
        # to go and find the file, which is a direction rather than a location — and the
        # slug rules differ per base (the journal timestamps its filenames, persona memory
        # does not), so following it cost an inference and a `Read` per hit.
        print(f"{hang}{_rel_to_root(h.path)}")
        if full:
            snip = _body_snippet(h.path)
            if snip:
                print(f"{hang}{snip}")
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


def _news_line(e, status: str) -> str:
    """One entry, one line, in the shape the reader acts on.

    Deliberately not JSON. The skill is a Claude Code artifact and opencode gets none, so
    this text is the only thing an agent on another harness has — a machine surface would
    become the parsed one, and this the unchecked one.

    Every span an entry owns goes through `contain.one_line`. The slug, the headline and
    the `adopt:` command are all frontmatter or a filename, this is two lines of charter's
    own report with a structure a reader parses by eye, and #502 is the same value crossing
    into the same kind of line one function over. The `·` and the indent are charter's; a
    third of each would be the entry's.
    """
    from . import news

    adopt = contain.one_line(e.adopt)
    # Asked of the CONTAINED value, not the raw one. They agree today; the question this
    # line needs answered is "is there a command to show the reader", and a value that
    # renders as nothing is not one.
    how = f"adopt: charter {adopt}" if adopt else "adopt: manual (see the entry)"
    return (f"  {contain.one_line(e.slug)} · {news.marker(e)}"
            f"{contain.one_line(e.headline)}\n      {how}")


#: Said once, where a probe would otherwise be run against nothing.
#:
#: "Has this plane adopted it?" has no subject outside a control plane. Running the probes
#: anyway would spend a subprocess each to report whatever a plane-less charter happens to
#: exit with — an answer to a question nobody asked, indistinguishable in the output from
#: one that means something.
_NO_PLANE = ("no control plane here, so charter cannot tell which of these this plane has "
             "adopted — run `charter news --pending` from inside one")


def cmd_news(args) -> int:
    """What a version brought, and what this plane has not taken up."""
    from . import news

    planeless = not config.HAS_CONTROL_PLANE

    version = getattr(args, "for_version", None)
    if version:
        # THE release gate. `release.yml`'s pre-publish check runs this and refuses to tag
        # on a non-zero exit, and its `announce` job pipes this same stdout into
        # `gh release create --notes-file`. So an ordering charter cannot honour is caught
        # before a Release exists to be wrong, rather than after — which is the position
        # #486 was filed from, with the notes already published.
        # Both halves, behind one gate. `entry_errors` speaks for files that BECAME
        # entries; `unreadable` speaks for files in the news directory that did not, which
        # is where a miscased `Version:` lands — dropped by `_read` before any per-entry
        # check exists to be asked, and waved through by a release guard that answers from
        # filenames (#503). Asked over the whole directory rather than for this version,
        # because a file with no readable version has no version to be filtered by and the
        # release being cut is when somebody wants to hear about it.
        problems = news.entry_errors(news.for_version(version)) + news.unreadable()
        if problems:
            # "the news for", not "the news entries for": half of what this gate now
            # catches is a file that never became an entry, and a sentence naming only
            # entries would send the reader looking for one that does not exist.
            util.err(f"the news for {version} declares something charter cannot honour:")
            for why in problems:
                util.info(f"  {why}")
            return 1
        body = news.render_body(version)
        if not body:
            util.err(f"no news entry for {version}.")
            return 1
        # The claim nobody was making (#665). Everything above answers "do these entries
        # render?", which was never the failing question — this command exits 0 on a
        # 300,000-character body, because producing the string is exactly what it was asked
        # to do. What `announce` then does with the string is POST it to an API that
        # refuses a body over `news.RELEASE_BODY_MAX` outright rather than trimming it, and
        # `announce` is `needs: publish`: the refusal lands after an upload PyPI will not
        # take back, and the documented `workflow_dispatch` retry cannot reach `announce`
        # again because `publish` is rejected for a version PyPI already has.
        #
        # Asked here rather than in a step of its own in `release.yml`, because `guard` and
        # `announce` run this exact command against this exact tree: one answer, delivered
        # at the cheap end of the workflow — before `test`, `build` and `publish` — with no
        # second copy of the limit in a shell script to drift from this one.
        #
        # `+ 1` is the newline `print` adds below. It is a character in the file `announce`
        # redirects into and a character GitHub counts, and measuring the string rather than
        # the file is the off-by-one that would show up only at the ceiling — which is the
        # one place nobody gets a second try.
        sent = len(body) + 1
        if sent > news.RELEASE_BODY_MAX:
            util.err(
                f"the release notes for {version} come to {sent:,} characters and GitHub "
                f"refuses a release body over {news.RELEASE_BODY_MAX:,} — `gh release "
                f"create` fails with `body is too long`, and in release.yml that happens "
                f"in `announce`, after the PyPI upload.")
            # Said because the reader's obvious next move — "link more of them" — is the
            # one that cannot work here. `render_body` already links every note it can, so
            # reaching this means the HEADLINES alone are over the limit.
            util.info(f"  charter already lists every note for {version} by headline and a "
                      f"link, and the headlines alone do not fit. Shorten them, or split "
                      f"the release.")
            return 1
        print(body)
        return 0

    if getattr(args, "pending", False):
        if planeless:
            util.warn(_NO_PLANE)
            return 0
        shown, unchecked = 0, 0
        for e in news.released():
            status, why = news.probe(e)
            if status == news.PENDING:
                print(_news_line(e, status))
                shown += 1
            elif status == news.UNKNOWN:
                # Said, not swallowed. A probe that could not run is the one case where
                # silence would be read as "nothing to adopt" — the shape ADR 0013 and
                # `doctor`'s not-checked hint both exist to refuse.
                # The slug is the committed filename with its version prefix cut off, and
                # `why` already carries the entry's `check:` contained (`contain.sentence`).
                util.warn(f"{contain.one_line(e.slug)}: {why}")
                unchecked += 1
        if not shown:
            if unchecked:
                # NOT the ✓ line, which claims every probe reported adopted — the one
                # thing an unchecked entry did not say. A green tick printed under the
                # warning it contradicts is how the warning stops being read.
                util.warn(f"nothing pending, but {unchecked} entr"
                          f"{'y' if unchecked == 1 else 'ies'} could not be checked — "
                          f"which is not the same as nothing to adopt.")
            else:
                util.ok("nothing pending — every entry with a probe reports adopted.")
        return 0

    since = (getattr(args, "since", None) or "").strip()
    until = (getattr(args, "until", None) or _installed_version()).strip()
    if not since:
        # No baseline is not a range. Replaying every entry ever written as though it were
        # news would be charter presenting old text as new — so it says what it can and
        # points at the view that IS honest without one.
        util.info("no baseline recorded, so there is no range to report.")
        util.info("  what this plane has not adopted:  charter news --pending")
        return 0
    entries = news.between(since, until)
    if not entries:
        util.ok(f"nothing new between {since} and {until}.")
        return 0
    if planeless:
        util.warn(_NO_PLANE)
    # Warned, not refused. This view is a reader catching up, and withholding the range
    # over a malformed `security:` line would lose them the other nineteen entries to
    # protect them from one being in the wrong place. `--for` is where refusing belongs,
    # because that is the call that becomes a published Release.
    for why in news.entry_errors(entries) + news.unreadable():
        util.warn(why)
    for e in entries:
        status, _ = (news.INFORMATIONAL, "") if planeless else news.probe(e)
        # Version and headline are both frontmatter, and this is a report line whose
        # two-space column a reader parses by eye (#502).
        print(f"{contain.one_line(e.version)}  {news.marker(e)}"
              f"{contain.one_line(e.headline)}")
        # Only an entry with something to DO gets the action line. An informational entry
        # — a patch note, usually — exists to say there is nothing to take up, so printing
        # "adopt: manual" beneath it invents a chore out of the line that denies one.
        if status == news.PENDING:
            print(_news_line(e, status))
    return 0


def cmd_news_stamp(args) -> int:
    """Move every staged entry onto the version about to ship.

    The bump PR's one mechanical step, beside the four files that carry a version number.
    A command rather than a fifth thing to remember, for the reason the release charter
    gives about `hooks.json`: never work from a remembered count.
    """
    from . import news

    version = (getattr(args, "version", "") or "").strip()
    renamed, blocked = news.stamp(version)
    for why in blocked:
        util.err(why)
    if blocked:
        return 1
    for src, dst in renamed:
        # The same pair of committed filenames `news.stamp`'s refusals carry, on the path
        # that always runs. Containing the refusal and not the success would be #502 in
        # miniature: one spelling of the message guarded, its neighbour three lines away
        # not — and this is the one a release prints.
        util.ok(f"{contain.one_line(src.name)} → {contain.one_line(dst.name)}")
    if not renamed:
        util.info(f"no staged entries — nothing to move onto {version}.")

    # Read back what is on disk rather than trusting the run. Nothing staged is a
    # legitimate state (the entry may already name the version), and an entry missing
    # altogether is not — the two are indistinguishable from the rename count alone, and
    # only one of them publishes a version with no notes. Naming it here costs a line;
    # letting the tag find it costs a release (ADR 0013).
    if not news.stamped(version):
        util.err(f"no entry names {version}. Every published version needs one, including "
                 f"a patch — write docs/news/{version}-<slug>.md before tagging, or the "
                 f"release guard refuses to publish.")
        return 1
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
    "Nothing to upgrade" for a git-installed charter and leaves you pinned.

    **The last gate before a requirement specifier** (#333), the same place
    `browser._checked_version` sits before an npm package spec and for the same reason:
    the argv-list discipline stops the value splitting into extra ARGUMENTS, and stops
    nothing about what it means inside the one argument it is. ``charter-cp==0.*``
    resolves to the latest 0.x, so an exact-looking pin would not pin.

    Raises rather than returning a refusal because every caller is a command or
    :func:`sync_to`, both of which have somewhere to put the message — and because a
    version that got this far unchecked is a bug in the caller, not a user input.
    """
    if not instance.version_ok(version):
        raise ValueError(instance.NOT_A_VERSION.format(version=version))
    return ["uv", "tool", "install", f"{_dist()}=={version}", "--force", "--refresh"]


def sync_to(version: str) -> tuple[bool, str]:
    """Install exactly *version*. Returns (ok, detail). Never raises."""
    import shutil
    try:
        cmd = _sync_cmd(version)
    except ValueError as e:
        return False, str(e)
    if not shutil.which("uv"):
        return False, "uv is not on PATH — install it, or run the pip equivalent by hand"
    proc = util.run(cmd, check=False)
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
        # ASK THE HARNESS. This branch used to read `$CLAUDE_PLUGIN_ROOT` and then print
        # `claude plugin update charter@charter` unconditionally — so an opencode user,
        # for whom that variable is always absent, was told to run a command belonging to
        # a harness they are not in. `Harness.upgrade` makes it one question with one
        # answer, and charter moves only the artifacts it authored.
        from . import harness as _harness

        h = _harness.get(_harness.current())
        if h is None:
            util.warn("no harness detected, so charter cannot say how this plane's "
                      "artifact moves — `charter harness list`.")
            util.info("  the machine-global binary instead: charter version sync --cli")
            return 0
        status, detail = h.upgrade(config.ROOT)
        if status == "current":
            util.ok(f"the {h.name} artifact serving this project is already on {detail}.")
        elif status == "moved":
            util.ok(f"moved: {detail}")
        elif status == "manual":
            # Named, not run. The host owns the artifact: its command may be absent, may
            # prompt for a scope, and it mutates the reader's editor install — the same
            # restraint the MCP launcher check keeps.
            util.info(f"this plane's version is its {h.name} artifact's (→ {locked}).")
            util.info(f"  run: {detail}")
        else:
            util.warn(detail)
        util.info(f"  the machine-global binary instead: charter version sync --cli")
        return 0

    installed = _installed_version()
    if not instance.version_ok(locked):
        # Before the "already on it" check, deliberately: a malformed pin is a defect in a
        # committed file whichever version happens to be installed, and reporting "in sync"
        # against a pin nothing could ever install is the silent-wrongness this guards.
        util.err(instance.NOT_A_VERSION.format(version=locked))
        util.info(f"  fix `[charter] version` in {config.ROOT / 'charter.toml'}")
        return 1
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
