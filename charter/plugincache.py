"""The installed Claude Code plugin versus the marketplace clone it came from.

``claude plugin update charter@charter`` compares **version strings**, and charter's plugin
version moves exactly once per release. The marketplace, meanwhile, is a git clone of
``main`` that Claude Code re-fetches on its own — so between releases the clone advances
and the installed copy does not, both keep saying ``0.51.0``, and the update command
correctly reports there is nothing to do. Measured on one machine at the time this was
written: 45 files differing, ``skills/secrets/SKILL.md`` and ``skills/browser/SKILL.md``
among them.

**The staleness that bites is the skills.** ``hooks/hooks.json`` invokes ``charter hook …``
— the *command*, resolved from ``PATH``, i.e. the ``uv tool`` install — so hook behaviour
follows the CLI and not the plugin's bundled copy of ``charter/*.py``. Skills are different:
they are text the model loads, and a stale one is wrong instructions delivered confidently.

Two callers, one mechanism:

* `doctor.check_plugin_freshness` compares by CONTENT, which is the question a version
  string cannot answer.
* `commands_update` force-refreshes on the dev channel, where a version-keyed update can
  never see the change.

**Everything here talks to `claude plugin … --json`, never to Claude Code's files.**
``~/.claude/plugins/installed_plugins.json``, ``known_marketplaces.json`` and the cache
layout are internals charter does not own; `update.plugin_version_here` already carries
the lesson (``bin/edm`` bet on an internal path and broke silently, #197). ``claude plugin
list --json`` and ``claude plugin marketplace list --json`` are documented CLI surfaces
that answer the same questions, and they cost ~0.2s each.
"""

from __future__ import annotations

import hashlib
import json
import re
import shutil
from pathlib import Path

from . import util

#: How long any one `claude plugin …` call may take. `list` measured at ~0.2s; `install`
#: fetches from the marketplace clone on disk but `marketplace update` reaches the network,
#: so the budget is the network one. Bounded rather than open-ended because `doctor` runs
#: as a SessionStart preflight with a 20s budget for every check it makes.
LIST_TIMEOUT = 15
REFRESH_TIMEOUT = 120

#: The directories a Claude Code plugin actually LOADS, and therefore the only ones whose
#: drift means anything.
#:
#: charter's marketplace entry is ``"source": "./"`` — the whole repository is copied into
#: the cache, ``tests/`` and ``docs/`` and all — so hashing the copy wholesale would report
#: a test-file edit as a stale plugin. That is not a stricter check, it is a noisier one:
#: `doctor`'s own comments keep returning to the point that a row which warns about
#: something benign trains people to scroll past the row, which costs the case that
#: matters. These five are the surface Claude Code reads.
#:
#: ``commands`` and ``agents`` are listed although charter ships neither today. A plugin
#: directory added later is loaded by the host whether or not this tuple was updated, and
#: the cost of naming it now is nothing (a missing entry is skipped); the cost of not
#: naming it is a category of drift this module reports as clean.
PLUGIN_SURFACE = (".claude-plugin", "hooks", "skills", "commands", "agents")

#: What a plugin id may look like before it is allowed near an argv.
#:
#: These values are read out of `claude plugin list --json` — a machine's own output, not
#: a committed file, so this is a narrower hazard than `charter.toml`'s. It is still input.
#: `util.run` takes a list and never a shell string, so the shell is not the exposure; the
#: exposure is an element that begins with ``-`` and is read by `claude` as a FLAG rather
#: than as the plugin to uninstall. This alphabet has no leading dash and no whitespace.
_PLUGIN_ID_RE = re.compile(r"^[A-Za-z0-9][A-Za-z0-9._-]{0,63}@[A-Za-z0-9][A-Za-z0-9._-]{0,63}$")

#: The scopes `claude plugin install|uninstall --scope` accepts. A closed set for the same
#: reason `instance.UPDATE_CHANNELS` is one: the value reaches an argv, and matching it
#: against constants charter wrote means nothing read from anywhere is ever passed through.
_SCOPES = ("user", "project", "local")

#: The marketplace-side half of the same rule.
_MARKETPLACE_RE = re.compile(r"^[A-Za-z0-9][A-Za-z0-9._-]{0,63}$")


def available() -> bool:
    """True when the `claude` CLI is on PATH. False is an ordinary answer, not a fault —
    charter supports opencode and Codex, and a plane on either has no Claude Code plugin
    to be stale."""
    return bool(shutil.which("claude"))


def _claude_json(args: list[str], cwd=None, timeout: float = LIST_TIMEOUT):
    """Run ``claude plugin <args> --json`` and parse it. ``None`` on any failure.

    ``None`` and ``[]`` are different answers and both are load-bearing downstream: "I
    could not look" must never render as "there is nothing installed", which is the
    confidently-wrong output `hooks.dispatched_handlers` documents at length.
    """
    if not available():
        return None
    proc = util.run(["claude", "plugin", *args, "--json"], cwd=cwd, check=False,
                    timeout=timeout)
    if proc.returncode != 0:
        return None
    try:
        return json.loads(proc.stdout or "")
    except (ValueError, TypeError):
        return None


def installed_charter_plugin(prefer_project=None) -> dict | None:
    """The installed charter plugin entry, or ``None`` when there is not exactly one to act
    on.

    *prefer_project* picks between several. charter's plugin is normally installed at
    ``project`` scope, once per project, and every one of those installs points at the SAME
    versioned cache directory — so refreshing any of them repopulates the cache for all of
    them. Which one is chosen therefore does not change the outcome; it changes which
    project's `claude plugin` invocation performs it, and running it against the plane you
    are standing in is the least surprising choice.

    Returns the raw entry (``id``, ``scope``, ``installPath``, ``projectPath``) with the
    two fields that reach an argv already validated — an entry charter would refuse to act
    on is not returned at all, so no caller has to remember to check.
    """
    entries = _claude_json(["list"])
    if not isinstance(entries, list):
        return None
    ours = []
    for e in entries:
        if not isinstance(e, dict):
            continue
        pid = e.get("id")
        if not isinstance(pid, str) or not _PLUGIN_ID_RE.match(pid):
            continue
        if pid.split("@", 1)[0] != "charter":
            continue
        if e.get("scope") not in _SCOPES:
            continue
        ours.append(e)
    if not ours:
        return None
    if prefer_project is not None:
        want = str(Path(prefer_project).resolve())
        for e in ours:
            got = e.get("projectPath")
            if isinstance(got, str) and str(Path(got).resolve()) == want:
                return e
    return ours[0]


def marketplace_clone(name: str) -> Path | None:
    """Where the marketplace named *name* is cloned, or ``None``.

    *name* comes from a plugin id charter has already matched against
    :data:`_PLUGIN_ID_RE`; it is re-checked here because this function is also reachable
    on its own, and a path built from an unchecked name is a path.
    """
    if not isinstance(name, str) or not _MARKETPLACE_RE.match(name):
        return None
    rows = _claude_json(["marketplace", "list"])
    if not isinstance(rows, list):
        return None
    for row in rows:
        if not isinstance(row, dict) or row.get("name") != name:
            continue
        loc = row.get("installLocation")
        if isinstance(loc, str) and loc:
            p = Path(loc)
            return p if p.is_dir() else None
    return None


def content_hash(root) -> str | None:
    """A stable digest of the plugin surface under *root*, or ``None`` if it cannot be read.

    Path and bytes both go into the digest, so a file that is renamed, added or deleted
    changes it as surely as an edited one does. Entries are sorted, so two trees that hold
    the same files hash the same regardless of directory-iteration order. Symlinks are not
    followed and directories are walked with :meth:`Path.rglob`, which does not.
    """
    root = Path(root)
    if not root.is_dir():
        return None
    h = hashlib.sha256()
    try:
        for rel in sorted(_surface_files(root)):
            h.update(rel.encode("utf-8"))
            h.update(b"\0")
            h.update(hashlib.sha256((root / rel).read_bytes()).digest())
    except OSError:
        return None
    return h.hexdigest()


def _surface_files(root: Path) -> list[str]:
    """Every plugin-surface file under *root*, as POSIX relative paths."""
    out = []
    for name in PLUGIN_SURFACE:
        base = root / name
        if not base.is_dir():
            continue
        for p in base.rglob("*"):
            if p.is_file() and not p.is_symlink():
                out.append(p.relative_to(root).as_posix())
    return out


def differing(installed, clone, limit: int = 3) -> list[str]:
    """Up to *limit* surface paths that differ between the two trees, for the report line.

    The hash says *whether*; this says *which*, and a doctor row that names
    ``skills/secrets/SKILL.md`` is the difference between a number a reader distrusts and
    a fact they can go and look at.
    """
    installed, clone = Path(installed), Path(clone)
    try:
        left, right = set(_surface_files(installed)), set(_surface_files(clone))
    except OSError:
        return []
    out = []
    for rel in sorted(left | right):
        if len(out) >= limit:
            break
        if rel not in left or rel not in right:
            out.append(rel)
            continue
        try:
            if (installed / rel).read_bytes() != (clone / rel).read_bytes():
                out.append(rel)
        except OSError:
            out.append(rel)
    return out


def refresh_argvs(plugin_id: str, scope: str) -> list[list[str]] | None:
    """The three commands that force the installed plugin back into step, in order.

    ``None`` if either input fails its check, so a caller cannot run a partial sequence
    against a value charter would not have built an argv from.

    The order is the mechanism, verified end to end rather than reasoned about:

    1. ``marketplace update`` — advance the clone. Skip it and the reinstall faithfully
       re-copies the same stale content.
    2. ``uninstall`` — the install step is a no-op against an already-installed plugin
       (measured: *"Plugin is already installed"*, cache untouched), so this is what makes
       step 3 do any work at all.
    3. ``install`` — which re-copies the marketplace clone over the versioned cache
       directory, sentinel edit and all.

    ``-y`` on both mutating steps because charter runs them non-interactively; `claude`
    requires it when stdout is not a TTY and would otherwise refuse rather than prompt.
    """
    if not isinstance(plugin_id, str) or not _PLUGIN_ID_RE.match(plugin_id):
        return None
    if scope not in _SCOPES:
        return None
    marketplace = plugin_id.split("@", 1)[1]
    return [
        ["claude", "plugin", "marketplace", "update", marketplace],
        ["claude", "plugin", "uninstall", plugin_id, "--scope", scope, "-y"],
        ["claude", "plugin", "install", plugin_id, "--scope", scope, "-y"],
    ]


def force_refresh(prefer_project=None) -> tuple[bool, str]:
    """Re-copy the marketplace clone over the installed plugin. ``(ok, detail)``.

    ``(False, …)`` covers every way this can decline, and each of them is a real state a
    charter user is in rather than a defensive branch: no `claude` on PATH (opencode or
    Codex), no charter plugin installed (CLI-only, which `docs/install.md` supports), and
    a `claude plugin` call that failed.

    **This is not a rollback-capable operation.** The uninstall step is what makes the
    install step do anything, so a failure between them leaves the plugin uninstalled for
    that scope — recoverable with the one command named in the returned detail, and named
    there for exactly that reason.
    """
    if not available():
        return False, "the `claude` CLI is not on PATH, so there is no plugin to refresh"
    entry = installed_charter_plugin(prefer_project)
    if entry is None:
        return False, "no charter plugin is installed here (`claude plugin list`)"
    argvs = refresh_argvs(entry["id"], entry["scope"])
    # Unreachable through `installed_charter_plugin`, which already refuses an entry it
    # would not act on — and kept anyway, because the alternative is `refresh_argvs`
    # returning `None` into a `for argv in None`. Two checks of one rule is the cheap
    # failure; one check that a refactor moves is the expensive one.
    if argvs is None:
        return False, "the installed plugin's id or scope is not one charter will act on"
    cwd = entry.get("projectPath") if entry.get("scope") == "project" else None
    if cwd is not None and not Path(cwd).is_dir():
        cwd = None
    for argv in argvs:
        proc = util.run(argv, cwd=cwd, check=False, timeout=REFRESH_TIMEOUT)
        if proc.returncode != 0:
            why = (proc.stderr or proc.stdout or "").strip().splitlines()
            detail = why[-1][:200] if why else f"exit {proc.returncode}"
            return False, f"`{' '.join(argv)}` failed: {detail}"
    return True, f"{entry['id']} reinstalled from the marketplace clone"
