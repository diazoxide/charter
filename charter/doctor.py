"""Environment preflight checks for a control plane (``charter doctor``).

Read-only: verifies the tools and auth a developer needs *before* they try to
discover or clone, and prints exact remediation steps for anything missing.
Nothing here changes the system.
"""

from __future__ import annotations

import json
import os
import platform
import shutil
import sys
from dataclasses import dataclass
from pathlib import Path

from . import inventory, util
from .forge.gitlab import GitLabForge

OK, WARN, FAIL = "ok", "warn", "fail"

_SYMBOL = {OK: ("32", "✓"), WARN: ("33", "!"), FAIL: ("31", "✗")}


def _color() -> bool:
    return sys.stdout.isatty()


@dataclass
class Result:
    name: str
    status: str
    detail: str = ""
    hint: str = ""

    def render(self) -> str:
        code, glyph = _SYMBOL[self.status]
        if _color():
            glyph = f"\033[{code}m{glyph}\033[0m"
        line = f"  {glyph}  {self.name:<16} {self.detail}".rstrip()
        if self.hint and self.status != OK:
            line += f"\n        → {self.hint}"
        return line


def _first_line(text: str) -> str:
    text = (text or "").strip()
    return text.splitlines()[0] if text else ""


#: Kept in sync with `requires-python` in pyproject.toml — a test pins the two together.
MIN_PYTHON = (3, 11)


def check_python() -> Result:
    ok = sys.version_info >= MIN_PYTHON
    return Result(
        "python3",
        OK if ok else FAIL,
        detail=platform.python_version(),
        hint="" if ok else f"Python {'.'.join(map(str, MIN_PYTHON))}+ is required.",
    )


def check_git() -> Result:
    if not shutil.which("git"):
        return Result("git", FAIL, hint="Install git: xcode-select --install (macOS) or brew install git.")
    return Result("git", OK, detail=_first_line(util.run(["git", "--version"], check=False).stdout))


def check_git_identity() -> Result:
    """A commit needs a resolvable identity. ``commit_push`` (behind reactive memory,
    workspace snapshots, and `charter save`) shells out to git with ``check=False`` and
    swallows a failed commit — it's called from hooks/background paths that must never
    break a turn — so a machine with no ``user.name``/``user.email`` configured would
    silently lose memories/notes/dispatch tallies with nothing said about it. This check
    is that missing visibility, not a new failure mode."""
    name = _first_line(util.run(["git", "config", "--get", "user.name"], check=False).stdout)
    email = _first_line(util.run(["git", "config", "--get", "user.email"], check=False).stdout)
    if name and email:
        return Result("git identity", OK, detail=f"{name} <{email}>")
    missing = ", ".join(k for k, v in (("user.name", name), ("user.email", email)) if not v)
    return Result(
        "git identity",
        FAIL,
        detail=f"not set: {missing}",
        hint='Run: git config --global user.email "you@example.com" && '
             'git config --global user.name "Your Name"  — otherwise a commit (memory, '
             "workspace notes, dispatch tallies) silently never happens.",
    )


#: Per-CLI install hint — used when the control plane declares (or defaults to) a
#: forge whose CLI isn't installed. Keyed by `Forge.cli`, so a new forge kind only
#: needs an entry here, not a new check function.
_INSTALL_HINT = {
    "glab": "brew install glab  (see https://gitlab.com/gitlab-org/cli)",
    "gh": "brew install gh  (see https://cli.github.com/)",
}


def declared_or_default_forges() -> list:
    """The forges THIS control plane actually declares (`[[forge]]` blocks in its own
    ``charter.toml``, re-read fresh against the CURRENT ``config.ROOT`` — same
    discipline as ``commands._instance_load_root``, so a test that redirects
    ``config.ROOT`` after import sees what IT declared, not the real process's stale
    module-level config), de-duplicated by ``(kind, host)``.

    Falls back to a single default :class:`GitLabForge` when none are declared — the
    shape every control plane had before multi-forge support existed, and still what a
    fresh `charter init` (or a legacy single-forge control plane) produces. Before this
    (FINDING I3), `check_forge_cli`/`check_forge_auth` hardcoded `GitLabForge()`
    unconditionally, so a GitHub-only control plane got a `glab` FAIL — a real tool,
    just the wrong one, with a fix (`brew install glab`) that does nothing for a
    control plane that never touches GitLab at all.

    Never raises: a malformed `[[forge]]` block is a config mistake `doctor`'s own
    `check_control_plane_config` already surfaces separately — this just skips it
    rather than taking preflight down.

    Thin wrapper over `forge.registry.declared_or_default` — the same resolution a
    generated persona sub-agent's wording now uses (`commands_persona._render_agent`),
    so `doctor`'s forge checks and a sub-agent's prose can never drift apart on what
    this control plane's forge set actually is."""
    from . import config as _config
    from .forge import registry
    return registry.declared_or_default(_config.ROOT)


def check_forge_cli(forge=None) -> Result:
    forge = forge or GitLabForge()
    cli = forge.cli
    if not shutil.which(cli):
        hint = _INSTALL_HINT.get(cli, f"Install {cli}.")
        return Result(cli, FAIL, hint=f"Install {cli}: {hint}.")
    return Result(cli, OK, detail=_first_line(util.run([cli, "--version"], check=False).stdout))


def check_forge_auth(forge=None) -> Result:
    forge = forge or GitLabForge()
    cli = forge.cli
    if not shutil.which(cli):
        return Result(f"{cli} auth", FAIL, hint=f"Install {cli} first, then run: {cli} auth login.")
    try:
        proc = util.run([cli, "auth", "status", "--hostname", forge.host], check=False,
                        timeout=CHECK_TIMEOUT)
    except util.ProcTimeout as e:
        return Result(f"{cli} auth", WARN, detail=f"timed out after {e.seconds:g}s",
                      hint=f"`{cli} auth status` did not answer — the forge may be "
                           f"unreachable, or a credential helper is waiting on input.")
    blob = (proc.stdout or "") + (proc.stderr or "")
    if "Logged in" in blob:
        summary = next(
            (ln.strip() for ln in blob.splitlines() if "Logged in" in ln),
            "authenticated",
        )
        return Result(f"{cli} auth", OK, detail=summary)
    return Result(
        f"{cli} auth",
        FAIL,
        detail=_first_line(blob),
        hint=f"Run: {cli} auth login  (pick {forge.host}; choose TOKEN/HTTPS — charter "
             "never uses SSH for git).",
    )


def check_ssh() -> Result:
    """Golden rule 0: **one credential — per forge** — each repo's OWN forge's token over
    HTTPS (glab for GitLab, gh for GitHub, …). SSH is deliberately NOT used, so this no
    longer probes for a key (that was a contradictory hard requirement). Instead it
    verifies every repo in scope carries ITS forge's token-only git policy
    (`gitpolicy.forge_for` resolves which forge per repo)."""
    from . import config as _config, gitpolicy
    scope = gitpolicy.repos(_config.ROOT, _config.WORKSPACES_DIR)
    drift = {r: gitpolicy.check(r) for r in scope}
    bad = {r: d for r, d in drift.items() if d}
    if not bad:
        return Result("git auth", OK,
                      detail=f"token-only across {len(scope)} repo(s) (each forge's own "
                             f"HTTPS token; no SSH/signing)")
    # `charter git-policy --apply` deliberately no-ops for an UNMANAGED-forge repo (no
    # host to resolve a policy for) — telling a developer to run it for exactly THAT
    # repo is a permanently un-actionable hint. Split the two failure modes so the hint
    # stays honest either way.
    unmanaged = [r for r, d in bad.items() if d == [gitpolicy.UNMANAGED_FORGE]]
    fixable = [r for r in bad if r not in unmanaged]
    names = ", ".join(r.name for r in list(bad)[:3]) + (" …" if len(bad) > 3 else "")
    if unmanaged and not fixable:
        hint = (f"{len(unmanaged)} repo(s) have an unrecognised forge — `charter "
                f"git-policy --apply` deliberately no-ops for these (there's no policy to "
                f"apply for a host it can't identify). Declare the host under [[forge]] in "
                f"charter.toml to bring them under management, then re-run.")
    elif fixable and not unmanaged:
        hint = "Apply the single-credential policy to every clone: charter git-policy --apply"
    else:
        hint = (f"charter git-policy --apply fixes {len(fixable)} drifted repo(s); "
                f"{len(unmanaged)} more have an unrecognised forge and need a [[forge]] "
                f"declaration in charter.toml first — --apply alone won't touch those.")
    return Result(
        "git auth",
        WARN,
        detail=f"{len(bad)}/{len(scope)} repo(s) not token-only: {names}",
        hint=hint,
    )


def check_control_plane_config() -> Result:
    """``charter.toml`` failed to parse (malformed TOML, or a schema newer than this
    charter understands). ``config`` swallows the exception so the CLI stays usable
    (see ``config.CONFIG_ERROR``); this is where a user would look to find out why.

    A DIFFERENT, narrower failure lives one level down: the file parses fine but one
    ``[[forge]]`` block doesn't (a typo'd ``kind``, a missing field). ``registry.
    known_forges`` already keeps every host that DID resolve (a bad block no longer
    discards its good siblings — see ``charter/forge/registry.py``), but that recovery
    must not go silent: this is where it's surfaced, so a developer actually finds out a
    declared host isn't covered instead of the guard just quietly covering less."""
    from . import config as _config
    from .forge import registry

    if _config.CONFIG_ERROR is not None:
        return Result(
            "charter.toml",
            FAIL,
            detail=_first_line(_config.CONFIG_ERROR),
            hint="Fix or remove charter.toml, then re-run. Falling back to empty "
                 "group/exclude/workspace defaults until it does.",
        )
    _forges, forge_errors = registry.known_forges_report(_config.ROOT)
    if forge_errors:
        shown = "; ".join(forge_errors[:3]) + (" …" if len(forge_errors) > 3 else "")
        return Result(
            "charter.toml",
            WARN,
            detail=f"{len(forge_errors)} [[forge]] block(s) failed to resolve",
            hint=f"{shown} — those hosts are NOT covered by the one-credential guard or "
                 f"git-policy until fixed (other declared/default hosts still are).",
        )
    if not _config.HAS_CONTROL_PLANE:
        # NOT ok. Every check below reports green against a plane that does not exist —
        # `personas: none defined`, `vaults: none configured` — so a session with no
        # personas, no vault and memory written into a scratch directory reads as a
        # healthy one. This row is the only place that can say otherwise, and saying it
        # in the OK column is what made the worktree failure invisible.
        return Result("charter.toml", WARN, detail=f"no control plane found (cwd: {_config.ROOT})",
                      hint="`charter init` here, or cd into a plane, or set $CHARTER_ROOT. "
                           "Every check below is reporting on a plane that does not exist.")
    # Name the plane unconditionally. Nothing else printed WHICH plane is bound, so a
    # stale $CHARTER_ROOT, a nested plane and a rootless cwd all looked identical to a
    # correct setup.
    return Result("charter.toml", OK, detail=f"parsed cleanly ({_config.ROOT})")


def check_control_plane_schema() -> Result:
    """Structural drift, from ``charter.instance.drift``: baseline top-level directories
    (personas/, inventory/, workspaces/) a control plane is expected to have. This is
    the *detect* half of the same stamp/detect/heal pattern ``workspace reinit`` already
    proves for a single workspace's layout — lifted one level up to the whole control
    plane, healed by ``charter reinit`` — surfaced here so a stale control plane is
    visible without running ``reinit`` first."""
    from . import config as _config, instance as _instance

    if not _config.HAS_CONTROL_PLANE:
        return Result("schema", OK, detail="no control plane found")
    found = _instance.drift(_config.ROOT)
    if not found:
        return Result("schema", OK, detail=f"up to date (schema {_instance.SCHEMA})")
    return Result(
        "schema",
        WARN,
        detail=f"{len(found)} issue(s): " + "; ".join(found),
        hint="Run: charter reinit  (creates what's missing; never touches existing content).",
    )


def _git_in(root: Path, *args: str):
    """One read-only git question about ``root``, never raising on a non-zero exit.

    Timed out like every other check: the plane root is normally a small local repo, but
    a plane on a stalled network mount makes `git status` hang, and the SessionStart hook
    has a budget — a check that eats it prints nothing at all (see `CHECK_TIMEOUT`)."""
    return util.run(["git", "-C", str(root), *args], check=False, timeout=CHECK_TIMEOUT)


def _plane_default_branch(root: Path) -> str | None:
    """This repo's default branch, or ``None`` when charter cannot honestly say.

    Asked in order of decreasing authority:

    1. ``refs/remotes/origin/HEAD`` — the *remote's own* answer, recorded by `git clone`.
       It is the only source that is a fact rather than a guess, so it is consulted
       first: a plane whose default is `trunk` can easily still carry a stale local
       `main`, and guessing before asking would warn about the correct branch.
    2. A local ``main`` or ``master``, in that order. Needed because a plane is very often
       `git init`-ed and then given a remote by hand (`charter init` does not clone), and
       that never writes ``origin/HEAD`` — without this fallback the branch half of the
       check would be silent on most real planes.

    ``None`` when neither answers, and the caller must then say nothing about branches.
    Naming a default charter has not discovered would fire a warning at every session of
    a plane whose only sin is calling its branch something else, and a preflight that is
    permanently yellow is one people stop reading (`check_memory_indexes` records the
    same concern for the same reason)."""
    ref = _git_in(root, "symbolic-ref", "--quiet", "--short", "refs/remotes/origin/HEAD")
    remote_head = ref.stdout.strip()
    if ref.returncode == 0 and remote_head:
        # `--short` renders it as `origin/main`, not `main`.
        return remote_head.split("/", 1)[1] if remote_head.startswith("origin/") else remote_head
    for guess in ("main", "master"):
        if _git_in(root, "rev-parse", "--verify", "--quiet",
                   f"refs/heads/{guess}").returncode == 0:
            return guess
    return None


def check_plane_root() -> Result:
    """Is anyone working in the plane root?

    The plane root — the directory holding ``charter.toml`` — holds the control plane:
    personas, inventory, workspaces, config, and nothing anyone is meant to edit or
    switch branches in. Work happens in a workspace's clones. Nothing in the filesystem
    enforces that (ADR 0008), and the failure it invites is invisible in exactly the
    surface a user would check: two sessions both sitting in the root share one working
    tree and one HEAD and thrash each other's branches, while charter reports two
    different workspaces and lists no tree that would hint at why. Observed rather than
    theorised — six branches in one session, and a `git checkout main` in the root that
    silently reverted in-flight work out of the tree.

    This is the replacement for `check_embedded_worktrees`, which went with the embedded
    plane shape (ADR 0007). That check guarded a hazard specific to a shape that no
    longer exists; deleting it without a successor would leave the *new* failure mode
    unwatched in the file built to watch for failure modes.

    WARN, never FAIL. FAIL is doctor's "you cannot work" list — it is what makes
    `charter doctor` exit non-zero, which is what makes the SessionStart wrapper print
    the preflight-failed banner — and a root being worked in is a smell that gets
    expensive later, not a broken plane. ADR 0008 chose signal over refusal on purpose,
    and this is that signal at the moment acting on it is still cheap.

    Never raises: this runs from the SessionStart hook, and is the command you run
    *because* something is wrong. The exceptions caught are narrow (a missing/unusable
    git, a root that cannot be read, git not answering in time) rather than a bare
    ``except``, for the reason `check_memory_indexes` records: a broad catch there once
    swallowed a `NameError` and reported OK, and a check that silently does nothing is
    worse than no check.
    """
    from . import config as _config

    name = "plane root"
    if not _config.HAS_CONTROL_PLANE:
        # No plane, no plane root. `check_control_plane_config` already says so loudly;
        # a second row repeating it is noise.
        return Result(name, OK, detail="no control plane found")

    root = Path(_config.ROOT)
    try:
        top = _git_in(root, "rev-parse", "--show-toplevel")
        toplevel = top.stdout.strip()
        if top.returncode != 0 or not toplevel:
            # `charter init` in a fresh directory does not run `git init` — that is the
            # README's own 60-second path. No history, no branch to be on, no dirt.
            return Result(name, OK, detail="not a git repository")
        # `.resolve()` on both sides or this comparison lies: macOS hands out temp and
        # home paths through symlinks (`/var` → `/private/var`), and git always answers
        # with the physical path.
        if Path(toplevel).resolve() != root.resolve():
            # A `charter.toml` in a subdirectory of some larger repo: that repo is not
            # the plane's. Its branch is whatever that project is working on and its dirt
            # is that project's work in progress, so reporting on it would warn every
            # session about a state that is entirely correct.
            return Result(name, OK, detail=f"not its own repository (inside {toplevel})")

        head = _git_in(root, "symbolic-ref", "--quiet", "--short", "HEAD")
        # Non-zero means detached — asked this way rather than `rev-parse --abbrev-ref`,
        # which answers the literal string "HEAD" and so reads as an ordinary branch name
        # right up until it is compared against the default.
        branch = head.stdout.strip() if head.returncode == 0 else None
        default = _plane_default_branch(root)
        # `--untracked-files=no` deliberately. Memory defaults to `share = "local"` —
        # written to disk and never committed — so every plane a few days old carries
        # untracked files under `personas/*/memory/`. Counting those would put this row
        # permanently in the yellow, which costs the two findings that do matter.
        status = _git_in(root, "status", "--porcelain", "--untracked-files=no")
        dirty = [ln for ln in status.stdout.splitlines() if ln.strip()]
    except (util.ProcTimeout, OSError) as e:
        return Result(name, OK, detail=f"not checked ({e})")

    findings, actions = [], []
    if branch is None:
        findings.append("detached HEAD")
        actions.append(f"Put the root back on a branch: git -C {root} checkout "
                       f"{default or '<your default branch>'}.")
    elif default and branch != default:
        findings.append(f"on {branch}, not {default}")
        actions.append(f"Put the root back: git -C {root} checkout {default}.")
    if dirty:
        findings.append(f"{len(dirty)} uncommitted file(s)")
        actions.append("Commit control-plane content with `charter save`.")
    if not findings:
        return Result(name, OK, detail=f"clean on {branch}")
    # Where the work belongs is said ONCE, after the per-finding actions. Saying it per
    # finding printed the same "move it into a workspace clone" clause twice in a row
    # that fires with both findings at once — which is the common case, since whoever
    # branched in the root is also editing in it.
    return Result(
        name, WARN, detail=", ".join(findings),
        hint=" ".join(actions) + " Anything that is not control plane belongs in a "
             "workspace clone — charter workspace create <task>, then charter clone "
             "<repo>; the plane root is one working tree every session shares.",
    )


def check_inventory() -> Result:
    """Can this plane clone anything?

    Asked of `inventory.repos()` rather than the file's own count, because a plane can
    clone its own repo without ever running `discover` — the root's `origin` says what it
    is (`inventory.plane_repo`). `discover` is therefore optional, and a plane that can
    reach the repo it was made for is not missing anything.

    Warning regardless would be the permanently-yellow preflight `check_memory_indexes`
    records the case against — and the nag is expensive in its own right: on a personal
    account `discover` enumerates every repo the owner has, and writes that listing into
    a tracked `inventory/repos.json`. Telling someone to publish sixty repos to silence a
    row about the one they already have is worse advice than saying nothing.
    """
    n = inventory.load().get("count", 0)
    if n:
        return Result("inventory", OK, detail=f"{n} repos mapped")
    if inventory.repos():
        return Result("inventory", OK,
                      detail="not built — this plane's own repo is clonable without it")
    return Result("inventory", WARN,
                  detail="empty, and this plane's own repo could not be derived",
                  hint="Run: charter discover  (builds inventory/repos.json).")


def check_vaults() -> Result:
    # Kept non-fatal: no vaults is a perfectly valid state, and this check runs
    # from the SessionStart hook — it must never block a session.
    from .secrets import base, registry

    try:
        vs = registry.vaults()
    except base.VaultError as e:
        return Result("vaults", WARN, hint=str(e))
    if not vs:
        return Result("vaults", OK, detail="none configured")
    bad, no_identity, timed_out = [], [], []
    for name in vs:
        try:
            prov = registry.provider_for(name)
            # A vault that declares the identity it is read through, whose variable is
            # not set, is broken in a way `health()` cannot see: `op` answers with "no
            # items" or a permission error, so it reads as an empty or misconfigured
            # vault rather than as a missing credential. Separated from `bad` because
            # the fix is `export`, not anything about the vault.
            prov.env_overlay()
            healthy, _ = prov.health()
        except util.ProcTimeout:
            # `op` reaching a desktop app that is waiting on a biometric prompt looks
            # exactly like a hang. Naming it beats inheriting the caller's whole budget.
            timed_out.append(name)
            continue
        except base.VaultError as e:
            if "is unset" in str(e):
                no_identity.append(name)
                continue
            healthy = False
        if not healthy:
            bad.append(name)
    if timed_out:
        return Result("vaults", WARN, detail=f"{len(vs)} configured",
                      hint=f"timed out reading: {', '.join(timed_out)} — the provider CLI "
                           f"did not answer within {CHECK_TIMEOUT:g}s (1Password waiting on "
                           f"a biometric prompt looks exactly like this).")
    if no_identity:
        srcs = []
        for n in no_identity:
            srcs += [f"${s}" for s in (vs[n].get("config", {}).get("env") or {}).values()]
        return Result("vaults", WARN, detail=f"{len(vs)} configured",
                      hint=f"identity variable unset for: {', '.join(no_identity)} — "
                           f"export {', '.join(sorted(set(srcs)))} (charter will not fall "
                           f"back to an ambient token; that would read the vault as "
                           f"someone else)")
    if bad:
        return Result("vaults", WARN, detail=f"{len(vs)} configured",
                      hint="unhealthy: " + ", ".join(bad))
    return Result("vaults", OK, detail=f"{len(vs)} configured, all healthy")


#: Entry count at which a memory index is worth curating. Not a cap and not a
#: truncation point — charter injects a bounded digest, so a long index costs
#: nothing at session start. It is a nudge toward `charter persona optimize`.
_INDEX_LINES_WARN = 150


def check_version_lock() -> Result:
    """`[charter] version` vs what is installed.

    Opt-in: a control plane that pins nothing is a perfectly normal state and
    reports OK, not a nag. When it does pin, drift is a WARN rather than a FAIL —
    this runs from the SessionStart hook, and charter must never make its own
    tooling the reason someone cannot work (being offline is not a defect).
    """
    from . import __version__, config as _config, instance as _instance
    try:
        locked = _instance.locked_version(_instance.load(_config.ROOT))
    except Exception as e:
        return Result("version lock", OK, detail=f"not checked ({e})")
    if not locked:
        return Result("version lock", OK, detail="not pinned")
    if locked == __version__:
        return Result("version lock", OK, detail=f"pinned {locked}, in sync")
    return Result("version lock", WARN,
                  detail=f"pinned {locked}, running {__version__}",
                  hint="Run: charter version sync  (conforms this machine to the lock)")


def check_memory_indexes() -> Result:
    """Every memory base's MEMORY.md must agree with the files beside it.

    A dangling link makes `charter recall` surface a hit nobody can read; an
    unindexed file is a memory the index — and therefore the SessionStart digest
    — never mentions. Neither needs a concurrency bug to happen: MEMORY.md is
    append-heavy and edited by many agents and humans at once, so a merge
    resolved by taking one side drops the other's line while its file survives.
    That is exactly how both showed up in a real control plane.

    WARN, never FAIL: drift is a hygiene problem, and doctor's blockers list means
    "you cannot work" — an out-of-step index does not stop you cloning a repo.
    (An earlier version of this note justified the same choice with "this runs from
    the SessionStart hook, which must never block a session". It does not: `hooks.py`
    never imports this module. Right conclusion, wrong reason.)
    """
    from . import config, memstore, persona, workspace

    bases = []
    try:
        for name in persona.list_personas():
            bases.append((name, persona.memory_dir(name)))
        bases.append((config.SHARED_PERSONA, persona.memory_dir(config.SHARED_PERSONA,
                                                                shared=True)))
        for name in workspace.list_workspaces():
            bases.append((f"ws:{name}", workspace.memory_dir(name)))
    except OSError as e:
        # Only an unreadable/absent tree is tolerated. A broader `except` here once
        # swallowed a NameError and reported OK — a check that silently does
        # nothing is worse than no check.
        return Result("memory indexes", OK, detail=f"not checked ({e})")

    dangling = unindexed = 0
    worst = []
    large = []
    for label, mem_dir in bases:
        if not mem_dir.exists():
            continue
        d = memstore.index_drift(mem_dir)
        if d["dangling"] or d["unindexed"]:
            dangling += len(d["dangling"])
            unindexed += len(d["unindexed"])
            worst.append(f"{label} ({len(d['dangling'])} dangling, "
                         f"{len(d['unindexed'])} unindexed)")
        # Growth signal. An index only ever appends, so a long-lived persona's grows
        # without bound and nothing says so — you have to already suspect you need
        # `persona optimize`. Not truncation: charter injects a bounded digest at
        # SessionStart, so nothing is silently dropped. Just a nudge to curate.
        n = memstore.index_size(mem_dir)
        if n >= _INDEX_LINES_WARN:
            large.append(f"{label} ({n} entries)")
    if not worst and not large:
        return Result("memory indexes", OK, detail=f"{len(bases)} base(s) consistent")
    hint = ", ".join(worst[:4]) + (", …" if len(worst) > 4 else "")
    if unindexed:
        hint += "  → charter persona optimize --all --apply  (links unindexed files)"
    if dangling:
        hint += "  → a dangling link is proposal-only: prune it, or write the memory it names"
    if large:
        if hint:
            hint += "  "
        hint += ("large: " + ", ".join(large[:4]) + (", …" if len(large) > 4 else "")
                 + "  → charter persona optimize <name>  (curate; growth is not a defect)")
    if not worst:
        return Result("memory indexes", WARN,
                      detail=f"{len(large)} large index(es)", hint=hint)
    return Result("memory indexes", WARN,
                  detail=f"{dangling} dangling, {unindexed} unindexed", hint=hint)


def check_personas() -> Result:
    """Roster config health — `persona lint` across every persona, summarised.

    `lint` could always find a dangling ``extends:``, an inheritance cycle, or a
    charter naming a skill no sub-agent can invoke; nothing ever ran it. It was in no
    hook and in no other command, so it reported drift only to someone who already
    suspected drift. This is the check running by itself, in the preflight a developer
    already runs.

    One line, not a per-persona dump: the detail names what is wrong and the hint
    points at `charter persona lint`, which has room to explain. WARN, never FAIL —
    doctor's blockers list means "you cannot work", and an untidy persona does not stop
    you cloning a repo or reaching the forge.

    Affordable only because :func:`persona._installed_skills` is memoised: the walk it
    performs is ~27ms and `lint` calls it once per persona, which is what made a
    13-persona sweep cost 364ms.
    """
    from . import persona
    try:
        names = persona.list_personas()
    except Exception as e:
        return Result("personas", OK, detail=f"not checked ({e})")
    if not names:
        return Result("personas", OK, detail="none defined")

    errors: dict[str, int] = {}
    warns: dict[str, int] = {}
    drafts: list[str] = []
    for n in names:
        try:
            issues = persona.lint(n)
            if persona.is_draft(n):
                drafts.append(n)
        except Exception:
            # A persona charter is a file humans edit; a malformed one must not take
            # down the command you run *because* something is wrong.
            errors[n] = errors.get(n, 0) + 1
            continue
        for level, _msg in issues:
            (errors if level == "error" else warns)[n] = \
                (errors if level == "error" else warns).get(n, 0) + 1

    if not errors and not warns:
        return Result("personas", OK, detail=f"{len(names)} persona(s), all clean")

    bits = []
    if errors:
        bits.append(f"{len(errors)} with error(s): {', '.join(sorted(errors))}")
    if drafts:
        bits.append(f"{len(drafts)} draft: {', '.join(sorted(drafts))}")
    soft = sorted(set(warns) - set(drafts))
    if soft:
        bits.append(f"{len(soft)} with warning(s): {', '.join(soft)}")
    return Result("personas", WARN, detail=" · ".join(bits),
                  hint="charter persona lint  (per-persona detail and how to fix each)")


def check_plugin_skew() -> Result:
    """`charter` ships as two artifacts — the CLI (pip/uv) and the Claude Code plugin
    (``.claude-plugin/plugin.json`` + ``hooks/hooks.json``) — with two version numbers.
    ``hooks.skew_message`` is the loud guard a running hook speaks through; this is the
    same check surfaced in `doctor`, for a developer who just wants to ask directly.

    Only meaningful inside a Claude Code session with the plugin installed: Claude Code
    sets ``CLAUDE_PLUGIN_ROOT`` for the plugin's own processes (including a `charter
    doctor` a hook or the agent runs), pointing at the installed plugin's own directory.
    A bare `charter doctor` from a plain terminal (no plugin, pip/uv install only) has
    nothing to compare against — that's a normal, fully-supported way to run charter, so
    this stays OK rather than warning about a plugin that was never installed."""
    from . import hooks

    root = os.environ.get("CLAUDE_PLUGIN_ROOT")
    if not root:
        return Result("plugin", OK, detail="not running under the Claude Code plugin")
    manifest = Path(root) / ".claude-plugin" / "plugin.json"
    try:
        plugin_version = json.loads(manifest.read_text()).get("version")
    except (OSError, ValueError):
        return Result("plugin", WARN, detail="plugin manifest unreadable",
                      hint=f"expected a readable plugin.json at {manifest}")
    msg = hooks.skew_message(plugin_version)
    if msg:
        # FAIL, not WARN. A plugin NEWER than the CLI can dispatch `charter hook <name>`
        # for a handler this CLI does not have, so the hook simply does not run — the
        # guard looks installed and is not. `cmd_doctor` exits 1 only on FAIL, and that
        # exit code is what makes the SessionStart wrapper (`out=$(charter doctor) ||
        # printf …`) print anything at all; at WARN the message reached nobody through
        # either surface.
        return Result("plugin", FAIL,
                      detail=f"v{plugin_version} (CLI v{hooks.MIN_PLUGIN_VERSION})", hint=msg)
    # `skew_message` is one-directional — silent for an equal OR older plugin — so "matches"
    # was printed for a plugin many versions behind. It stays OK (an older plugin wires
    # fewer hooks, which is benign, unlike a newer one dispatching handlers this CLI lacks)
    # but it must not claim agreement it hasn't checked. A charter that reported "v0.1.0
    # matches the installed CLI" against a v0.13.1 CLI is how the drift stayed invisible.
    if plugin_version == hooks.MIN_PLUGIN_VERSION:
        return Result("plugin", OK, detail=f"v{plugin_version} matches the installed CLI")
    return Result("plugin", OK,
                  detail=f"v{plugin_version} (CLI v{hooks.MIN_PLUGIN_VERSION}) — older "
                         f"plugin, supported; upgrade it for the newest hooks")


#: Seconds a single preflight check may take before it is reported as timed out rather
#: than waited on. `gh api`, `glab api` and `op` all reach the network or a desktop app; a
#: 1Password session needing re-auth stalled the whole SessionStart preflight for its 20s
#: budget and then printed NOTHING, because results were collected before any were shown.
CHECK_TIMEOUT = 5.0


def iter_all():
    """Yield each :class:`Result` as it completes.

    A generator rather than a list because `cmd_doctor` collected everything before
    printing a single line: a preflight killed by its hook timeout emitted no diagnosis at
    all, not even the checks that had already passed. Streaming turns a mystery stall into
    "got as far as `vaults`, then stopped" — which names the culprit without charter
    having to guess at it.
    """
    for r in _checks():
        yield r


def _checks():
    """Order: cheap/local checks first, network checks last. The forge cli/auth pair is
    NOT fixed (it used to be exactly one hardcoded GitLab pair) — it's one pair PER
    FORGE this control plane actually declares (`declared_or_default_forges`), so a
    GitHub-only control plane sees `gh`/`gh auth`, never a `glab` FAIL with no real fix
    (FINDING I3)."""
    results = [check_python(), check_git(), check_git_identity()]
    for forge in declared_or_default_forges():
        results.append(check_forge_cli(forge))
        results.append(check_forge_auth(forge))
    results += [check_ssh(), check_control_plane_config(), check_control_plane_schema(),
                check_plane_root(),
                check_inventory(), check_vaults(), check_version_lock(),
                check_memory_indexes(), check_personas(),
                check_plugin_skew()]
    return results


def run_all() -> list[Result]:
    """Every check, collected. Kept for callers that want them all at once (`--json`,
    tests); `iter_all` is what an interactive preflight should use."""
    return list(iter_all())
