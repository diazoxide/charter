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


#: Why a check that could not RUN is a warning rather than a tick (#171).
#:
#: "not checked" is the absence of information, not evidence of health — and a green glyph
#: over it is read as the latter by anyone scanning the column, which is how the whole class
#: this audit came from works. Two of these sites already carried the comment "a check that
#: silently does nothing is worse than no check" and then returned OK anyway; the principle
#: was right and the status contradicted it.
#:
#: WARN, never FAIL: `cmd_doctor` exits non-zero only on FAIL, and an unreadable tree or a
#: git that timed out is not "you cannot work" — it is "charter cannot tell you either way".
_NOT_CHECKED_HINT = ("This check could not run, so its silence means nothing. Re-run "
                     "`charter doctor` — if it persists, the reason above is the thing to fix.")


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
        # How far the root has drifted behind its upstream. Read from the ALREADY-FETCHED
        # remote ref — never a live query, because this runs from the SessionStart hook and
        # must not reach the network. `@{upstream}` fails cleanly on a root with no
        # tracking branch (a plane `git init`-ed by hand), which is not a fault.
        behind = _git_in(root, "rev-list", "--count", "HEAD..@{upstream}")
        behind_n = int(behind.stdout.strip() or 0) if behind.returncode == 0 else 0
        upstream = _git_in(root, "rev-parse", "--abbrev-ref", "@{upstream}")
        upstream_ref = upstream.stdout.strip() if upstream.returncode == 0 else ""
    except (util.ProcTimeout, OSError) as e:
        return Result(name, WARN, detail=f"not checked ({e})",
                      hint=_NOT_CHECKED_HINT)

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
    # Said in the DETAIL and never as a finding: a root behind its upstream is the normal
    # resting state of a directory nobody works in, so warning about it would put this row
    # permanently in the yellow — the cost this check already refuses to pay for untracked
    # memory files. But `clean on main` is a statement about the working TREE that reads as
    # one about the plane, and a root drifts behind precisely BECAUSE nobody works in it:
    # every change arrives through a workspace clone and a PR, and nothing pulls the root.
    # Observed three times in one session, twice acting on a stale checkout.
    #
    # Appended to BOTH paths. The first version put it only on the clean one, so a root
    # that was dirty *and* behind said nothing about being behind — and those two states
    # share a cause, since the same neglect that leaves memory files uncommitted is what
    # leaves the checkout stale. It stayed silent on the one occasion it mattered: a plane
    # three commits behind, holding the fix for the bug being debugged, reporting only its
    # uncommitted file.
    #
    # "at last fetch" is not hedging. The number is read from a ref that is only as current
    # as the last fetch, and presenting it as a live reading would be the failure ADR 0013
    # names — in the check whose whole job is to report state honestly.
    drift = (f", {behind_n} behind {upstream_ref or 'upstream'} at last fetch"
             if behind_n else "")
    if not findings:
        return Result(name, OK, detail=f"clean on {branch}{drift}")
    # Where the work belongs is said ONCE, after the per-finding actions. Saying it per
    # finding printed the same "move it into a workspace clone" clause twice in a row
    # that fires with both findings at once — which is the common case, since whoever
    # branched in the root is also editing in it.
    return Result(
        name, WARN, detail=", ".join(findings) + drift,
        hint=" ".join(actions) + " Anything that is not control plane belongs in a "
             "workspace clone — charter workspace create <task>, then charter clone "
             "<repo>; the plane root is one working tree every session shares.",
    )



def _settings_files() -> list[Path]:
    """The settings the HOST actually resolves, in the order it reads them.

    One list, used both for a directly-declared hook and for `enabledPlugins`, so the two
    halves of "is it wired" can never disagree about which files are in force.
    """
    from . import config as _config
    return [Path(_config.ROOT) / ".claude" / "settings.json",
            Path(_config.ROOT) / ".claude" / "settings.local.json",
            Path.home() / ".claude" / "settings.json"]


def _enabled_plugin_ids() -> set[str]:
    """Plugin ids the host has ENABLED. Installed is not enabled (#177)."""
    out: set[str] = set()
    for p in _settings_files():
        try:
            doc = json.loads(p.read_text())
        except (OSError, ValueError, UnicodeDecodeError):
            continue
        if not isinstance(doc, dict):
            continue
        for pid, on in (doc.get("enabledPlugins") or {}).items():
            if on:
                out.add(pid)
    return out


def _plugin_declaring_guard() -> str | None:
    """An ENABLED Claude Code plugin whose own ``hooks.json`` dispatches the guard.

    ``CLAUDE_PLUGIN_ROOT`` is set only for the plugin's OWN processes, so a `charter doctor`
    a human runs in a terminal never sees it — even where the plugin is enabled and the
    guard demonstrably fires. Checking the env var alone cries wolf on exactly the planes
    that ARE protected.

    But **installed, enabled and wired are three different states, and only the third
    protects anything** (#177). 0.31.1 accepted the first and printed a tick over a plane
    where `git checkout -b` in the root succeeded: the plugin was installed and *disabled*,
    so nothing dispatched the hook. That is #168's own category error — verifying a proxy
    instead of the fact — committed while fixing #168.

    Deliberately NOT version-checked, against #177's suggestion. A plugin supplies only the
    WIRING; the handler is whatever ``charter`` is on PATH. The reporting plane's plugin was
    0.29.1, from before the guard existed, and its ``hooks.json`` still dispatches
    ``charter hook pretooluse`` — which would have run today's CLI, guard included, had it
    been enabled. Requiring a minimum plugin version would warn on planes that are genuinely
    protected, which is the cry-wolf failure 0.31.1 was itself fixing. A plugin too old to
    wire ``pretooluse`` at all is already excluded by reading its hooks.json rather than its
    version.

    Read from ``installed_plugins.json`` rather than globbed out of the plugin cache: the
    cache keeps every version ever fetched, so a stale copy would answer for a plugin since
    removed. The manifest is what the host actually installed.
    """
    enabled = _enabled_plugin_ids()
    if not enabled:
        return None
    manifest = Path.home() / ".claude" / "plugins" / "installed_plugins.json"
    try:
        doc = json.loads(manifest.read_text())
    except (OSError, ValueError):
        return None
    for pid, entries in (doc.get("plugins") or {}).items():
        if pid not in enabled:
            continue
        for entry in entries or []:
            path = (entry or {}).get("installPath")
            if not path:
                continue
            hooks_json = Path(path) / "hooks" / "hooks.json"
            try:
                if "charter hook pretooluse" in hooks_json.read_text():
                    return pid
            except (OSError, UnicodeDecodeError):
                continue
    return None


def check_guard_wired() -> Result:
    """Can `charter hook pretooluse` actually be invoked? (#168)

    The plane-root branch guard (#157) lives in that handler. If nothing is wired to call
    it, no branch move is ever refused — and `check_plugin_skew` printed a green
    ``✓ not running under the Claude Code plugin`` over exactly that state. **The absence
    of a protection rendered as health**, at the moment it mattered most: someone upgrades
    to get the guard, runs `doctor` to confirm the upgrade, sees all green, and reasonably
    believes they are protected.

    Whether the plane runs as a plugin is an implementation detail. Whether the guard fires
    is the fact the operator needs, so this reports the guard and `check_plugin_skew` keeps
    answering its own separate question about version skew.

    Reachable means ANY of: running under the plugin, or `charter hook pretooluse` declared
    in the plane's ``.claude/settings.json``, its ``.claude/settings.local.json``, or the
    user's ``~/.claude/settings.json``. All four, because a plane that IS wired and gets
    warned every session teaches people to ignore the row — the failure
    `check_memory_indexes` already records.

    It asserts that specific handler rather than "some hook exists": a plane wiring only
    `sessionstart` is unprotected while looking configured, which is this issue again one
    level down.
    """
    from . import config as _config

    name = "plane-root guard"
    if not _config.HAS_CONTROL_PLANE:
        return Result(name, OK, detail="no control plane found")

    # `commands._ensure_guard_hook` asks the SAME question through the same function.
    # Reading different evidence is how a writer and a checker disagreed about "wired"
    # and left the guard declared twice — and enabled is not dispatched (#177).
    plugin = ("Claude Code plugin" if os.environ.get("CLAUDE_PLUGIN_ROOT")
              else _plugin_declaring_guard())

    declared = []
    for p in _settings_files():
        try:
            if "charter hook pretooluse" in p.read_text():
                declared.append(p)
        except (OSError, UnicodeDecodeError):
            continue

    if plugin and declared:
        # Not broken — doubled, which is why nobody finds it: two denials for one command
        # read as one stubborn denial. 0.43.1 stopped `init` writing this; planes wired
        # before it still carry the copy, and charter will not delete from a file that is
        # the operator's and git-tracked.
        where = ", ".join(str(p) for p in declared[:2])
        return Result(name, WARN,
                      detail=f"declared twice — the {plugin} dispatches it, and so does "
                             f"{where}",
                      hint=f"charter runs `hook pretooluse` once per declaration, so every "
                           f"Bash call is guarded twice. Remove the charter `hooks` block "
                           f"from {where} and keep the plugin — or disable the plugin and "
                           f"keep the block. charter does not edit that file for you.")
    if plugin:
        detail = ("wired (Claude Code plugin)" if plugin == "Claude Code plugin"
                  else f"wired (enabled plugin {plugin})")
        return Result(name, OK, detail=detail)
    if declared:
        return Result(name, OK, detail=f"wired ({declared[0]})")
    return Result(name, WARN,
                  detail="pretooluse is not wired — branch moves in the plane root are "
                         "NOT refused",
                  hint=("The 0.30.0 guard only fires through `charter hook pretooluse`.  "
                        "→ charter reinit  (wires it into .claude/settings.json), or "
                        "install the Claude Code plugin."))


def check_harness() -> Result:
    """Which harness this is, and every capability it cannot carry (ADR 0015).

    Charter enforces the same invariants on every harness; what differs is what it can
    *offer*. A missing offer looks exactly like a broken install from the outside — which
    is `check_guard_wired`'s lesson one level up — so each ceiling is named here rather
    than left to be discovered.

    Reported as OK even when the list is long. A ceiling is a fact about the harness, not
    a fault in the plane, and a row that warns every session for something the operator
    cannot fix teaches them to ignore the column.
    """
    from .harness import registry as _harness

    name = "harness"
    current = _harness.current()
    if not current:
        return Result(name, OK, detail="not running inside a harness")
    if _harness.get(current) is None:
        # An empty deficit list here would mean "charter knows of no gaps", and charter
        # knows nothing at all about this runtime — the same sentence rendering two
        # opposite facts. Registered harnesses are the ones whose ceilings have been
        # checked against the binary; anything else is unverified by definition.
        return Result(name, WARN,
                      detail=f"{current} — charter has no record of this harness",
                      hint=(f"Charter enforces what it can here, but nothing has verified "
                            f"which surfaces {current} carries. Register it in "
                            f"`charter/harness/registry.py` (KINDS) with its own deficits, "
                            f"or unset $CHARTER_HARNESS if it was set by mistake."))
    live = _harness.get(current)
    stale = live.stale_wiring() if live else ""
    if stale:
        from . import __version__

        return Result(name, WARN,
                      detail=f"{current} — its plugin was written by {stale}, charter is "
                             f"{__version__}",
                      hint=("A plugin an older charter wrote is still a file where a "
                            "working one belongs — 0.40.0's guard never fired. → charter "
                            "reinit (an edited plugin is reported, never overwritten)"))
    gaps = _harness.deficits(current)
    if not gaps:
        return Result(name, OK, detail=current)
    def _line(d):
        # The remedy sits on the ceiling it answers, not in a footnote: a limit stated and
        # left there reads as "nothing can be done", and the operator stops looking.
        fix = f"  → {d.remedy}" if d.remedy else ""
        return f"        ↳ {d.key}: {d.detail}{fix}"

    listed = "\n".join(_line(d) for d in gaps)
    plural = "" if len(gaps) == 1 else "s"
    return Result(name, OK, detail=f"{current} — {len(gaps)} capability ceiling{plural}\n{listed}")


def check_guard_seen() -> Result:
    """Has a guard ever actually RUN here, and under which harness?

    A sibling of `check_guard_wired`, not a replacement, for the reason that check is itself
    a sibling of `check_plugin_skew`: *"Whether the plane runs as a plugin is an
    implementation detail. Whether the guard fires is the fact the operator needs"* — two
    facts, two rows, so neither can hide behind the other. Declared and dispatched are just
    as separate: a plane root was switched between branches four times and committed to,
    unguarded, while `check_guard_wired` would have reported a tick throughout. The
    declaration was real. Nothing dispatched it.

    **An age, never a verdict.** A plane worked in from a plain terminal has no dispatch and
    is fine; a plane whose guard last fired weeks ago under a harness you have since stopped
    using is the incident. Charter supplies the date and the harness and lets the reader
    draw that line (ADR 0013).

    Silent on a plane nobody has worked in yet — nothing could have dispatched there, and a
    warning on day one is how a row stops being read.
    """
    from . import guardseen as _seen

    name = "guard seen"
    rec = _seen.last()
    if rec:
        at = _seen_age(rec.get("ts"))
        where = rec.get("harness") or "an unnamed harness"
        return Result(name, OK, detail=f"last ran {at} ago under {where}")
    if not _seen.plane_has_been_used():
        return Result(name, OK, detail="plane not worked in yet")
    return Result(name, WARN,
                  detail="no guard has ever run in this plane",
                  hint="Charter's guards reach a session through its harness, so work done "
                       "outside one is unguarded and says nothing about it. If you work "
                       "here through a harness, it is not wired: -> charter reinit")


def _seen_age(ts) -> str:
    from datetime import datetime, timezone

    from . import pieces as _pieces

    try:
        when = datetime.fromisoformat(ts)
    except (TypeError, ValueError):
        return "?"
    return _pieces.since(when, datetime.now(timezone.utc))


def check_nested_plane() -> Result:
    """Is the plane charter resolved sitting inside ANOTHER plane's ``workspaces/``? (#140)

    `charter.toml` is tracked, so every clone of a plane is a plane, and `charter clone`
    puts clones exactly where an outer plane keeps them. Standing in one, every command
    silently operates on the inner plane — its own vault registry, its own workspace
    pointers, its own `workspaces/`.

    Reported rather than resolved. The ambiguity is genuine: sometimes the inner plane is
    the one you mean, since charter's own dogfooding clones charter into a workspace and
    that clone is a plane you might legitimately manage. Changing `ROOT` resolution is the
    most invasive change available in this codebase and would guess for you; naming it ends
    the silence, which is the actual complaint — the failure is invisible in both
    directions, so nothing tells you the outer plane never saw your command.

    ADR 0013's second rule, applied: a divergence charter can see, charter names.

    WARN, never FAIL: the inner plane works perfectly. It is simply, probably, not the one
    you meant.
    """
    from . import config as _config
    from . import root as _root

    name = "nested plane"
    if not _config.HAS_CONTROL_PLANE:
        return Result(name, OK, detail="no control plane found")
    try:
        outer = _root.enclosing_plane(Path(_config.ROOT))
    except OSError as e:
        return Result(name, WARN, detail=f"not checked ({e})",
                      hint=_NOT_CHECKED_HINT)
    if outer is None:
        # Standing in a nested clone no longer reaches here: `find_root` hops outward
        # through `workspaces/`, so ROOT is already the outer plane and there is nothing to
        # warn about. Say which plane answered anyway — the hop is a correction charter
        # made on the operator's behalf, and ADR 0013's second rule covers charter's own
        # corrections too.
        origin = getattr(_config, "NESTED_ORIGIN", None)
        if origin is not None and origin != _config.ROOT:
            return Result(name, OK,
                          detail=f"standing in {util.short_path(origin)}, acting on "
                                 f"{util.short_path(_config.ROOT)}")
        return Result(name, OK, detail="not nested")
    # Reached only when $CHARTER_ROOT refused the hop, which is the one state where the
    # inner plane really does take the writes.
    return Result(name, WARN,
                  detail=f"pinned inside {util.short_path(outer)}'s workspaces/",
                  hint=("$CHARTER_ROOT points at a plane nested in another one, so vaults "
                        "and workspace pointers go to the inner plane and the outer never "
                        "sees them. Without the override charter would resolve to "
                        f"{util.short_path(outer)}.  → unset CHARTER_ROOT to use it"))


def _ask_rules() -> list | None:
    """`permissions.ask` from the plane's settings — ``None`` when it cannot be read."""
    from . import config as _config
    p = Path(_config.ROOT) / ".claude" / "settings.json"
    if not p.exists():
        return []
    try:
        doc = json.loads(p.read_text())
    except (OSError, ValueError, UnicodeDecodeError):
        return None
    if not isinstance(doc, dict):
        return None
    perms = doc.get("permissions")
    if not isinstance(perms, dict):
        return []
    ask = perms.get("ask")
    return ask if isinstance(ask, list) else []


def _declared_binaries() -> dict[str, set[str]]:
    """``{binary: {persona, …}}`` — every tool any persona declares, and who declares it."""
    from . import persona as _persona
    out: dict[str, set[str]] = {}
    for name in _persona.list_personas():
        try:
            for tool in _persona.effective_tools(name) or ():
                out.setdefault(tool, set()).add(name)
        except Exception:
            continue
    return out


def shadowed_tools(rules) -> set[str]:
    """Declared persona tools that *rules* would force a prompt for.

    Matching is deliberately coarse — the first token inside ``Bash(...)``, plus the blanket
    forms ``Bash`` and ``Bash(*)`` which match everything. Charter is not re-implementing
    the host's matcher; it is answering "would this obviously shadow a tool", and a coarse
    answer that is right about `Bash(kubectl *)` is worth more than a precise one nobody
    maintains.
    """
    declared = _declared_binaries()
    if not declared:
        return set()
    hit: set[str] = set()
    for raw in rules or ():
        if not isinstance(raw, str):
            continue
        rule = raw.strip()
        if rule in ("Bash", "Bash(*)", "*"):
            return set(declared)
        if not rule.startswith("Bash(") or not rule.endswith(")"):
            continue
        inner = rule[len("Bash("):-1].strip()
        head = inner.split()[0] if inner.split() else ""
        head = head.rstrip("*")
        if head and head in declared:
            hit.add(head)
    return hit


def check_ask_rules() -> Result:
    """An `ask` rule that shadows a tool a persona declares.

    ADR 0014 puts pattern-shaped policy in the host's `permissions`, which creates exactly
    one interaction charter must not leave silent: *"a matching ask rule still prompts even
    when the hook returned `allow` or `ask`"*. So `toolgate` can return `allow` for a
    binary the active persona declares and the operator is prompted anyway — the persona's
    tools quietly stop being pre-approved, with nothing naming the cause. That is the
    looks-wired-but-isn't shape this repo has paid for twice (#177, #197).

    It never suggests deleting the rule. The rule is the operator's policy and is probably
    deliberate; charter names the consequence and leaves the choice (ADR 0013).
    """
    name = "ask rules"
    rules = _ask_rules()
    if rules is None:
        return Result(name, WARN, detail="settings.json not readable",
                      hint=_NOT_CHECKED_HINT)
    if not rules:
        return Result(name, OK, detail="none")
    try:
        hit = shadowed_tools(rules)
    except Exception as e:
        return Result(name, WARN, detail=f"not checked ({e})", hint=_NOT_CHECKED_HINT)
    if not hit:
        return Result(name, OK, detail=f"{len(rules)} rule(s), none shadow a persona tool")
    declared = _declared_binaries()
    who = sorted({p for t in hit for p in declared.get(t, ())})
    return Result(name, WARN,
                  detail=f"{', '.join(sorted(hit))} prompt(s) despite being declared by "
                         f"{', '.join(who)}",
                  hint="An ask rule outranks a PreToolUse allow, so charter's persona "
                       "tool-gate cannot pre-approve these. That may be exactly what you "
                       "want — this names it so the prompts are not a mystery.")


#: The skills the plugin ships. A constant rather than a directory listing because the CLI
#: is installed from a wheel that contains no `skills/` — that directory belongs to the
#: plugin artifact. A test asserts this equals the repo's `skills/`, so the two cannot part
#: company without the suite saying so.
SHIPPED_SKILLS = frozenset({"secrets", "working-in-a-clone", "persona", "browser"})


def _is_charter_checkout(root) -> bool:
    """True when *root* is a clone of charter itself.

    Structural, and deliberately so: the marker is charter's own source tree sitting beside
    a `pyproject.toml` that names the distribution. Nothing here depends on how this
    process was installed, which is what the previous test got wrong.
    """
    from pathlib import Path

    root = Path(root)
    if not (root / "charter" / "docsrc.py").is_file():
        return False
    try:
        return "charter-cp" in (root / "pyproject.toml").read_text()
    except OSError:
        return False


def shadowed_knowledge(root) -> dict[str, list[str]]:
    """A plane's own pages and skills that cover something charter already ships.

    Returns {"skills": [...], "docs": [...]} — names only, sorted.

    Empty when the plane *is* charter's own checkout. Its `docs/personas.md` is the very
    page `docs show personas` serves; reporting that as a shadow of itself would make the
    check noise on the one machine most likely to run it.

    That test asks what the ROOT is, not where `docsrc` happened to read from. Keying it on
    `docsrc.source()` looked equivalent and was not: `source()` prefers the packaged copy,
    so it only matched for someone running `python3 -m charter` from the clone. Every
    contributor also has `uv tool install charter-cp` — the README says to — and for them
    the exemption missed and doctor reported all eight of charter's own pages as shadows of
    themselves. Exactly the noise this paragraph promises to prevent.
    """
    from pathlib import Path

    from . import docsrc

    root = Path(root)
    if _is_charter_checkout(root):
        return {"skills": [], "docs": []}

    skills = sorted(
        d.name for d in (root / ".claude" / "skills").glob("*")
        if d.is_dir() and (d / "SKILL.md").is_file() and d.name in SHIPPED_SKILLS
    )
    topics = set(docsrc.topics())
    docs = sorted(
        p.stem for p in (root / "docs").glob("*.md") if p.stem in topics
    )
    return {"skills": skills, "docs": docs}


def check_shadowed_knowledge() -> Result:
    """A plane keeping its own copy of something charter ships.

    This is the failure that produced the check. A plane carried a `setup` skill telling
    engineers to authenticate over SSH and add an SSH key — months after the rule became
    token-only-over-HTTPS and charter's own guard began denying exactly that. Nine skills
    there were in some stage of the same rot. Every one of them looked wired, and nothing
    compared any of them to the CLI they described.

    Drift runs both ways, which is why this reports rather than resolves: the same plane's
    persona page had grown sections upstream never received. A copy is not automatically
    wrong — it is automatically *unwatched*, and that is the whole finding.

    So, like `check_ask_rules`, it never says delete. A plane may be deliberately overriding
    charter's guidance with something org-specific, and that is the operator's call
    (ADR 0013). Charter names what is being shadowed and what it costs.
    """
    name = "shadowed docs"
    # `_config`, imported here, the way every other check in this module reads the root.
    # A bare `config` was never bound in this scope, so this check raised NameError on
    # every single invocation and the `except` below rendered it as a benign environment
    # warning — a mechanism that looks wired and is not, inside the check written to find
    # exactly that (ADR 0014).
    from . import config as _config
    try:
        hit = shadowed_knowledge(_config.ROOT)
    except Exception as e:  # noqa: BLE001 - a preflight line must never be the thing that fails
        return Result(name, WARN, detail=f"not checked ({e})", hint=_NOT_CHECKED_HINT)

    parts = []
    if hit["skills"]:
        parts.append("skill(s) " + ", ".join(hit["skills"]))
    if hit["docs"]:
        parts.append("docs/" + ", docs/".join(f"{d}.md" for d in hit["docs"]))
    if not parts:
        return Result(name, OK, detail="none — charter's own knowledge is not duplicated here")

    return Result(name, WARN, detail=" · ".join(parts) + " duplicate what charter ships",
                  hint="A local copy wins over charter's and is not compared to it, so it "
                       "drifts in both directions unwatched — behind on a feature it never "
                       "learned about, ahead with sections upstream never received. Read "
                       "the shipped one with `charter docs show <topic>`; keep yours only "
                       "where it says something charter's does not.")


def check_workspace_clones() -> Result:
    """Clones that are behind their upstream — in EVERY workspace, not just the active one.

    `doctor` reported a healthy machine while a clone in a non-active workspace sat seven
    commits behind `origin/main`. `sync` defaults to the active workspace, which was empty,
    so it reported success having done nothing (#156). Both were locally truthful and
    jointly misleading, because neither was scoped to the thing that was out of date — and
    a stale clone is not inert, it is what a session reads if it happens to work there.

    **Read from remote-tracking refs; never fetched.** This runs from the SessionStart hook
    and must not reach the network, exactly as `check_plane_root` reads the root's own drift.
    The honest consequence, and it belongs in the output rather than only here: the check can
    **under**-report — origin moving is invisible until something fetches — but it can never
    invent staleness. That is the acceptable direction of failure, and the same discipline
    ADR 0009 sets for error text.

    Only *behind*. A clone with no upstream cannot be behind anything, and one that is
    **ahead** has unpushed work — a different condition, with a different remedy, that
    `sync` would not fix. Reporting either here would send the reader to the wrong command
    and put the row permanently yellow on ordinary planes, which costs the findings that
    matter (`check_memory_indexes` records the same concern).

    WARN, never FAIL: a stale clone is a thing to know, not a broken machine.
    """
    from . import config as _config
    from . import workspace as _workspace

    name = "workspace clones"
    if not _config.HAS_CONTROL_PLANE:
        return Result(name, OK, detail="no control plane found")

    behind: list[str] = []
    total = 0
    try:
        for ws in _workspace.list_workspaces():
            for clone in _workspace.clones(ws):
                total += 1
                # `@{upstream}` fails cleanly where there is no tracking branch, which is
                # not a fault — see the docstring.
                r = _git_in(clone, "rev-list", "--count", "HEAD..@{upstream}")
                if r.returncode != 0:
                    continue
                n = int(r.stdout.strip() or 0)
                if n:
                    behind.append(f"{ws}/{clone.name} ({n} behind)")
    except (util.ProcTimeout, OSError, ValueError) as e:
        # Narrow, per `check_memory_indexes`: a broad catch here once swallowed a NameError
        # and reported OK, and a check that silently does nothing is worse than no check.
        return Result(name, WARN, detail=f"not checked ({e})",
                      hint=_NOT_CHECKED_HINT)

    if not behind:
        if not total:
            return Result(name, OK, detail="no clones in any workspace — nothing to check")
        return Result(name, OK, detail=f"{total} clone(s) across all workspaces, none behind")
    return Result(name, WARN,
                  detail=", ".join(behind[:4]) + (", …" if len(behind) > 4 else ""),
                  hint=("→ charter sync --all  (plain `sync` only touches the ACTIVE "
                        "workspace, which is how this stays hidden)  "
                        "Counted from what the last fetch recorded, so it can under-report "
                        "— never a live query, this runs at SessionStart."))


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
                      hint="not reachable: " + ", ".join(bad))
    # Says what was actually checked, and nothing more. This line used to read "all
    # healthy", which is a claim about resolution that nothing here tests: `health()`
    # asks whether the vault is REACHABLE and how many items it holds, and deliberately
    # never resolves — `vault list` and `doctor` call it routinely, and resolving would
    # hit 1Password every time and could prompt for re-auth.
    #
    # Issue #55: "6 configured, all healthy" printed minutes apart from every resolution
    # through those vaults failing. Both were true. A reference can point at an item that
    # no longer exists while the vault holding it is perfectly reachable — and `doctor` is
    # the command you run BECAUSE something is wrong, so a green line about the broken
    # subsystem does not merely fail to help, it steers you away from the cause. It cost
    # the reporter forty minutes mid-incident.
    return Result("vaults", OK, detail=f"{len(vs)} reachable (references not resolved)",
                  hint="Resolve them for real: charter vault verify")


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
        return Result("version lock", WARN, detail=f"not checked ({e})",
                      hint=_NOT_CHECKED_HINT)
    if not locked:
        return Result("version lock", OK, detail="not pinned")

    from . import update
    # The pin is measured against the PLUGIN, because the plugin is the only part of
    # charter that is genuinely per-plane: Claude Code installs it per project, out of a
    # cache holding every version side by side. Measuring it against the machine-global
    # binary is what made the pin unhonourable — no plane can own that binary, so the drift
    # it reported had no fix that did not break another plane (#127).
    plugin = update.plugin_version_here()
    if plugin is not None:
        if plugin == locked:
            return Result("version lock", OK, detail=f"pinned {locked}, plugin in sync")
        return Result("version lock", WARN,
                      detail=f"pinned {locked}, plugin here runs {plugin}",
                      hint=f"Run: {update.PLUGIN_SYNC_CMD}  (this project only — a plugin "
                           f"is installed per project, so no other plane moves)")

    # Not running under the plugin: a `charter` typed in a terminal cannot see which plugin
    # version serves this project. Say that, and compare what CAN be seen — but never call
    # the machine-global CLI "this plane's version", which is the conflation #127 is about.
    if locked == __version__:
        return Result("version lock", OK,
                      detail=f"pinned {locked}, CLI in sync (plugin not visible from here)")
    return Result("version lock", WARN,
                  detail=f"pinned {locked}, CLI is {__version__}",
                  hint=f"{update.SHARED_INSTALL_NOTE}. To move THIS plane only: "
                       f"{update.PLUGIN_SYNC_CMD}")


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
        return Result("memory indexes", WARN, detail=f"not checked ({e})",
                      hint=_NOT_CHECKED_HINT)

    dangling = unindexed = 0
    worst = []
    large = []
    # WHICH KIND of base drifted, not just how much. The hint used to name
    # `charter persona optimize` for every base including `ws:` ones, whose loop never
    # touches a workspace — so it ran cleanly, fixed nothing, and left the drift reading as
    # repaired. A remediation hint that silently no-ops is worse than no hint at all.
    unindexed_kinds: set[str] = set()
    large_kinds: set[str] = set()
    for label, mem_dir in bases:
        if not mem_dir.exists():
            continue
        d = memstore.index_drift(mem_dir)
        if d["dangling"] or d["unindexed"]:
            dangling += len(d["dangling"])
            unindexed += len(d["unindexed"])
            if d["unindexed"]:
                unindexed_kinds.add("workspace" if label.startswith("ws:") else "persona")
            worst.append(f"{label} ({len(d['dangling'])} dangling, "
                         f"{len(d['unindexed'])} unindexed)")
        # Growth signal. An index only ever appends, so a long-lived persona's grows
        # without bound and nothing says so — you have to already suspect you need
        # `persona optimize`. Not truncation: charter injects a bounded digest at
        # SessionStart, so nothing is silently dropped. Just a nudge to curate.
        n = memstore.index_size(mem_dir)
        if n >= _INDEX_LINES_WARN:
            large.append(f"{label} ({n} entries)")
            large_kinds.add("workspace" if label.startswith("ws:") else "persona")
    if not worst and not large:
        return Result("memory indexes", OK, detail=f"{len(bases)} base(s) consistent")
    hint = ", ".join(worst[:4]) + (", …" if len(worst) > 4 else "")
    if unindexed:
        for kind in sorted(unindexed_kinds):
            hint += f"  → charter {kind} optimize --all --apply  (links unindexed files)"
    if dangling:
        hint += "  → a dangling link is proposal-only: prune it, or write the memory it names"
    if large:
        if hint:
            hint += "  "
        hint += ("large: " + ", ".join(large[:4]) + (", …" if len(large) > 4 else "")
                 + "".join(f"  → charter {k} optimize <name>" for k in sorted(large_kinds))
                 + "  (curate; growth is not a defect)")
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
        return Result("personas", WARN, detail=f"not checked ({e})",
                      hint=_NOT_CHECKED_HINT)
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


def check_vault_registry_divergence() -> Result:
    """The two halves of the vault registry, disagreeing about the same vault.

    `registry.load_registry` layers local over shared **per field**, so where both halves
    define one, the local value is what every read resolves through — silently, and while
    `vault list` reports the scope as `both` and says nothing more. The failure it produces
    is an empty vault rather than an error: `secret get` reports the key missing,
    `vault verify` finds no references, and `check_vaults` stays green because reachability
    is not resolution.

    FAIL rather than WARN, per ADR 0013: `cmd_doctor` exits non-zero only on FAIL, and that
    exit code is the only thing that makes the SessionStart wrapper print. A divergence
    worth naming is worth failing for.

    :data:`LOCAL_ONLY_KEYS` are excluded by construction — an account pin layered over a
    shared entry is the design working, not a fault.
    """
    from .secrets import base, registry

    try:
        shared = registry.load_shared().get("vaults", {})
        local = registry.load_local().get("vaults", {})
    except base.VaultError as e:
        return Result("vault registry", WARN, hint=str(e))

    clashes = []
    for name in sorted(set(shared) & set(local)):
        s, l = shared[name] or {}, local[name] or {}
        for key in ("provider", "persona"):
            sv, lv = s.get(key), l.get(key)
            if sv is not None and lv is not None and sv != lv:
                clashes.append(f"{name}.{key}: shared {sv!r}, local {lv!r}")
        sc, lc = s.get("config") or {}, l.get("config") or {}
        for key in sorted(set(sc) & set(lc)):
            if key in registry.LOCAL_ONLY_KEYS or sc[key] == lc[key]:
                continue
            clashes.append(f"{name}.{key}: shared {sc[key]!r}, local {lc[key]!r}")

    if not clashes:
        if not shared and not local:
            # Claiming "the halves agree" when neither half holds anything is an agreement
            # nothing was compared to reach — the exact wording `check_plugin_skew` already
            # warns about in this file ("it must not claim agreement it hasn't checked").
            return Result("vault registry", OK,
                          detail="no vaults registered — nothing to compare")
        return Result("vault registry", OK,
                      detail=f"shared and local halves agree ({len(shared) + len(local)} "
                             f"entr{'y' if len(shared) + len(local) == 1 else 'ies'})")
    shown = "; ".join(clashes[:3])
    if len(clashes) > 3:
        shown += f" (+{len(clashes) - 3} more)"
    return Result("vault registry", FAIL, detail=shown,
                  hint="the local half shadows the shared one field by field, so these "
                       "resolve through the local value. Re-publish to clear it: "
                       "charter vault add <name> --provider <p> … --share --force")


def _plugin_ids(root: Path) -> tuple[str, str]:
    """``(plugin, marketplace)`` from the manifests the installed plugin carries.

    Both files sit in the directory ``CLAUDE_PLUGIN_ROOT`` already names, so the id is
    exact without parsing Claude Code's cache path — a layout charter does not own and
    must not depend on. Either being absent falls back to a placeholder: the id is a
    convenience, while naming the two *steps* is the part that was missing.
    """
    def name(filename: str, fallback: str) -> str:
        try:
            doc = json.loads((root / ".claude-plugin" / filename).read_text())
            return (doc.get("name") or "").strip() or fallback
        except (OSError, ValueError, AttributeError):
            return fallback

    return name("plugin.json", "<plugin>"), name("marketplace.json", "<marketplace>")


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
    # The advice goes in `detail`, not `hint`: `Result.render` drops the hint entirely
    # when the status is OK, so guidance written there would be invisible while looking
    # shipped — which is the failure ADR 0013 is about, and it would be an unusually poor
    # place to commit it.
    #
    # Both steps, in order, because "upgrade it" did not upgrade anything. The marketplace
    # is a git clone advertising whatever it last fetched, so without refreshing it first
    # `plugin update` finds the installed version already current and correctly does
    # nothing; and `update` defaults to `user` scope while the plugin is usually installed
    # per project, which fails outright rather than silently. Observed together: a plugin
    # two minor versions behind, with `doctor` run repeatedly throughout.
    plugin_name, marketplace = _plugin_ids(Path(root))
    return Result("plugin", OK,
                  detail=f"v{plugin_version} (CLI v{hooks.MIN_PLUGIN_VERSION}) — older "
                         f"plugin, supported. Upgrade: `claude plugin marketplace update "
                         f"{marketplace}` (skip it and the next is a no-op), then `claude "
                         f"plugin update {plugin_name}@{marketplace} --scope "
                         f"<project|user, see: claude plugin list>`")


#: Launcher basenames charter itself removed, so it can name the replacement instead of
#: merely reporting the absence. `bin/edm` and its `bin/charter` forwarding shim went with
#: the rename; a `charter` on PATH is what took over from both.
_REMOVED_SHIMS = ("edm", "charter")


def _claude_json() -> Path:
    """Claude Code's user-level config — where `mcpServers` registrations live, both the
    user-scoped ones and the per-project ones under `projects`."""
    return Path.home() / ".claude.json"


def _vault_in_args(args) -> str | None:
    """The vault a ``charter secret exec <vault> …`` invocation opens, or ``None``.

    Read off the args rather than assumed, because it is the difference between advice that
    works and advice that half-works: a launcher charter merely *starts* has no plane to
    resolve, while one that opens a vault must find the plane holding it. Advice that
    applies to every entry is read as boilerplate and then not read at all.
    """
    if not isinstance(args, list):
        return None
    flat = [a for a in args if isinstance(a, str)]
    for i in range(len(flat) - 2):
        if flat[i] == "secret" and flat[i + 1] == "exec" and not flat[i + 2].startswith("-"):
            return flat[i + 2]
    return None


def _registered_launchers(doc: dict) -> list[tuple[str, str, list]]:
    """``(server name, command)`` for every **stdio** MCP server the host has registered.

    Both scopes, because checking only the top level would miss most real registrations —
    project-scoped servers are the common case. Every container is type-checked on the way
    down: `~/.claude.json` is a large harness-owned file (124KB of caches and counters on the
    machine that reported #197) whose shape charter does not control, and one odd value
    must not take the whole preflight down.
    """
    scopes = [doc.get("mcpServers")]
    projects = doc.get("projects")
    if isinstance(projects, dict):
        for body in projects.values():
            if isinstance(body, dict):
                scopes.append(body.get("mcpServers"))
    out: list[tuple[str, str, list]] = []
    for servers in scopes:
        if not isinstance(servers, dict):
            continue
        for name, entry in servers.items():
            # An SSE/HTTP server has a `url` and no `command`. Absence of a launcher is
            # not a missing launcher.
            if isinstance(entry, dict) and isinstance(entry.get("command"), str):
                out.append((str(name), entry["command"], entry.get("args") or []))
    return out


def _launcher_missing(command: str) -> bool:
    """True only when the path is **known** absent.

    An unreadable parent directory makes the answer unknown rather than negative, and
    `pathlib` reports that differently per platform — it raises EACCES on Linux and returns
    False on macOS, which has taken this repo's CI red twice. An ambiguous answer must
    never become a FAIL, so uncertainty resolves to "not missing".
    """
    try:
        return not Path(command).exists()
    except OSError:
        return False


def check_mcp_launchers() -> Result:
    """An MCP server whose launcher does not exist can never start — and says nothing.

    This is #197. The `edm`->`charter` rename removed `bin/edm` and its forwarding shim,
    and every MCP server registered as ``bin/edm secret exec <vault> … --exec -- <server>``
    began failing with ENOENT. Claude Code does not surface that: the tools are simply
    absent. A persona lost its whole toolset mid-investigation and rerouted through
    cloudflared + Kibana before anyone worked out why. Nothing in charter read
    `~/.claude.json` before this, so the breakage was invisible everywhere it happened.

    **Absolute paths only.** A bare `npx` resolves against the PATH of whoever launches the
    server, which is not the PATH charter runs under; calling that missing would be charter
    asserting something about an environment it cannot see — the guess ADR 0009 forbids,
    and how `doctor` cried wolf in 0.31.1 and #177. An absolute path that does not exist is
    a fact, and it is the fact that classifies every instance of this failure, not just the
    one that was reported.

    **FAIL, not WARN**, for the reason `check_plugin_skew` chose it: `cmd_doctor` exits
    non-zero only on FAIL, and that exit code is what makes the SessionStart wrapper print
    anything at all. A WARN would reproduce the defect it diagnoses — a real breakage that
    reached nobody.
    """
    path = _claude_json()
    try:
        doc = json.loads(path.read_text())
    except FileNotFoundError:
        # A machine that never registered an MCP server is healthy, not unknown.
        return Result("mcp", OK, detail="no MCP servers registered")
    except (OSError, ValueError, UnicodeDecodeError) as exc:
        return Result("mcp", WARN, detail=f"~/.claude.json unreadable ({_first_line(str(exc))})",
                      hint=_NOT_CHECKED_HINT)
    if not isinstance(doc, dict):
        return Result("mcp", WARN, detail="~/.claude.json is not an object",
                      hint=_NOT_CHECKED_HINT)

    registered = _registered_launchers(doc)
    checkable = [(n, c, a) for n, c, a in registered if os.path.isabs(c)]
    broken = [(n, c, a) for n, c, a in checkable if _launcher_missing(c)]
    if not broken:
        # "0 launcher(s) resolve" was shown for a plane whose only server had just been
        # repaired onto a bare command. Zero were CHECKED and none were broken; printing
        # the first as though it were a count of what resolves reads as "nothing is
        # registered", which is a different claim about a different fact.
        if not registered:
            return Result("mcp", OK, detail="no MCP servers registered")
        if not checkable:
            # Every registered launcher is a bare command, resolved against the PATH of
            # whoever starts it — which charter cannot see, and so does not claim to have
            # checked. Saying how many exist keeps that honest without inventing a verdict.
            return Result("mcp", OK,
                          detail=f"{len(registered)} server(s), none on an absolute path")
        return Result("mcp", OK, detail=f"{len(checkable)} launcher(s) resolve")

    hints = []
    for name, command, args in broken:
        if Path(command).name in _REMOVED_SHIMS:
            # Charter removed this one, so it knows what replaced it. Naming the
            # replacement turns a diagnosis into a one-line fix.
            fix = (f"{name}: {command} is gone (the edm→charter rename removed that shim) "
                   f"— set this server's command to `charter`, leaving its args unchanged")
            vault = _vault_in_args(args)
            if vault:
                # The half of the advice that was missing. The old command was an absolute
                # path into a particular umbrella; a bare `charter` resolves its plane from
                # the LAUNCHING directory, and an `mcpServers` entry at the top level of
                # `~/.claude.json` is user-scope — it launches for every project, most of
                # which are not that plane. The server then starts, fails to find the vault,
                # and the tools are missing again, silently, exactly as before.
                fix += (f". It opens vault `{vault}`, so also set CHARTER_ROOT in this "
                        f"server's `env` to the plane holding that vault — a user-scope "
                        f"server has no directory to resolve one from")
            hints.append(fix)
        else:
            # No claim about WHY it went missing: charter did not remove it and does not
            # know. It reports the fact and the two ways out.
            hints.append(f"{name}: {command} does not exist, so the server cannot start — "
                         f"repoint its command or remove the registration")
    return Result("mcp", FAIL,
                  detail=f"{len(broken)} of {len(checkable)} launcher(s) missing: "
                         + ", ".join(n for n, _, _ in broken),
                  hint="; ".join(hints))


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
                check_plane_root(), check_harness(), check_guard_wired(), check_guard_seen(), check_nested_plane(),
                check_workspace_clones(),
                check_inventory(), check_vaults(),
                check_vault_registry_divergence(), check_version_lock(),
                check_memory_indexes(), check_personas(), check_ask_rules(),
                check_shadowed_knowledge(),
                check_mcp_launchers(), check_plugin_skew()]
    return results


def run_all() -> list[Result]:
    """Every check, collected. Kept for callers that want them all at once (`--json`,
    tests); `iter_all` is what an interactive preflight should use."""
    return list(iter_all())
