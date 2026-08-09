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
    proc = util.run([cli, "auth", "status", "--hostname", forge.host], check=False)
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
    return Result("charter.toml", OK, detail="parsed cleanly" if _config.HAS_CONTROL_PLANE
                  else "no control plane found")


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


def check_inventory() -> Result:
    from . import config as _config
    n = inventory.load().get("count", 0)
    if n:
        return Result("inventory", OK, detail=f"{n} repos mapped")
    # An embedded plane clones nothing, so it has no clone targets to enumerate and
    # `discover` is not a thing it will ever run. Nagging for it forever is how a
    # permanently-yellow preflight teaches people to stop reading preflight.
    if _config.PLANE_SHAPE == "embedded":
        return Result("inventory", OK, detail="not used by an embedded plane")
    return Result("inventory", WARN, hint="Run: charter discover  (builds inventory/repos.json).")


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
    bad = []
    for name in vs:
        try:
            healthy, _ = registry.provider_for(name).health()
        except base.VaultError:
            healthy = False
        if not healthy:
            bad.append(name)
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


def check_embedded_worktrees() -> Result:
    """An **embedded** plane must not keep worktrees inside its own codebase.

    Worktrees default to ``workspaces/<ws>/.worktrees/`` — outside every clone, which is
    what stops nx/jest/maven recursing into them. In an embedded plane the clone IS the
    plane root, so that same path puts a full second copy of the source inside the tree
    every root-level glob walks. Nothing warns you: ``.gitignore`` hides it from git, and
    from git alone. Measured in charter's own checkout: 214 test files discoverable from
    the root, 142 of them duplicates of the other 72.

    `config.WORKTREES_ROOT` moves new worktrees out. Existing ones are NOT moved
    automatically — relocating a worktree means rewriting git's own ``gitdir`` pointer,
    and ``git worktree move`` is the command that does it correctly. So this reports them
    and hands over that command rather than guessing.

    Silent on a fleet plane, where the default path was never a problem.
    """
    from . import config as _config, workspace as _ws
    if _config.PLANE_SHAPE != "embedded":
        return Result("worktrees", OK, detail="fleet plane — worktrees stay per-workspace")

    stray: list[Path] = []
    try:
        for d in sorted(_config.WORKSPACES_DIR.glob(f"*/{_worktree_dir_name()}/*/*")):
            if d.is_dir():
                stray.append(d)
    except OSError:
        pass

    dest = _config.WORKTREES_ROOT
    if not stray:
        where = f" → {dest}" if dest else ""
        return Result("worktrees", OK, detail=f"outside the codebase{where}")

    names = ", ".join(d.name for d in stray[:3]) + (" …" if len(stray) > 3 else "")
    return Result(
        "worktrees",
        WARN,
        detail=f"{len(stray)} inside the codebase ({names}) — every root-level glob "
               f"(pytest/jest/nx/tsc, IDE indexers) sees the source twice over",
        hint=("Move each one with git, which rewrites its gitdir pointer: "
              f"git worktree move <old> {dest}/<workspace>/<repo>/<piece>"),
    )


def _worktree_dir_name() -> str:
    from . import worktree
    return worktree.DIR_NAME


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
        return Result("plugin", WARN,
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


def run_all() -> list[Result]:
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
                check_inventory(), check_vaults(), check_version_lock(),
                check_memory_indexes(), check_personas(), check_embedded_worktrees(),
                check_plugin_skew()]
    return results
