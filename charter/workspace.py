"""Workspaces: isolated, per-task environments of repo clones.

A **workspace** is a named directory under ``workspaces/`` holding one task's clones
(each repo on whatever branch that task needs): ``workspaces/<workspace>/<repo>``.
Different parallel tasks use different workspaces and must never be mixed.

Which workspace a command acts on is resolved by precedence:

1. an explicit ``--workspace`` flag,
2. the ``$CHARTER_WORKSPACE`` env var (set at session launch → hard per-session
   isolation for parallel agents),
3. the **per-Claude-session** pointer (``.charter/sessions/<id>.workspace``),
4. the **per-terminal** pointer (``.charter/terminals/<id>.workspace``) — a terminal
   pane survives closing/reopening Claude, so a pane keeps its own workspace,
5. otherwise ``default``.

``charter workspace use`` writes the per-terminal *and* per-session pointers (never a
shared/global one), so selecting a workspace in one pane never leaks into another.
A SessionStart hook (``workspace _reconcile``) seeds a reopened session's pointer
from its terminal's, so the status line stays in step with commands.

Secrets are deliberately *not* part of this — vaults are cross-workspace.
"""

from __future__ import annotations

import json
import os
import re
import time
from pathlib import Path

from . import config, util

_NAME_RE = re.compile(r"^[A-Za-z0-9][A-Za-z0-9._-]*$")
_SESSION_MAX_AGE = 30 * 86400  # prune per-session pointers older than this
_LEGACY_ROOT = config.ROOT / "repos"  # the pre-rename clone root


def _ensure_layout() -> None:
    """One-time migration: the clone root was renamed ``repos/`` → ``workspaces/``.
    If an old ``repos/`` still exists and the new dir doesn't, move it. Best-effort."""
    try:
        if _LEGACY_ROOT.exists() and not config.WORKSPACES_DIR.exists():
            _LEGACY_ROOT.rename(config.WORKSPACES_DIR)
    except OSError:
        pass


def _session_id(explicit: str | None = None) -> str | None:
    """This session's id, or ``None`` — see :mod:`charter.session`, which owns it."""
    from . import session as _session
    return _session.current(explicit)


def _session_file(sid: str) -> Path:
    return config.SESSIONS_DIR / f"{sid}.workspace"


#: Environment variables that identify ONE PANE — one shell, so at most one Claude
#: session. Safe to key a workspace pointer on.
_PANE_ID_VARS = (
    "TERM_SESSION_ID",   # iTerm2 / Terminal.app — per pane, survives reopen
    "TMUX_PANE",         # tmux pane
    "STY",               # GNU screen
    "SSH_TTY",           # the pty of this ssh session
)

#: Identifies a WINDOW, which holds many tabs and splits. Listed to be explicit that it
#: is deliberately NOT used — see `_terminal_id`.
_WINDOW_ID_VARS = ("WINDOWID",)


def _terminal_id(explicit: str | None = None) -> str | None:
    """A stable id for the current terminal PANE, or ``None`` when there isn't one.

    Unlike the Claude session id this survives closing and reopening Claude in the same
    pane, which is the whole reason a per-terminal pointer exists.

    ``WINDOWID`` is deliberately absent, and used to sit second in this chain. It
    identifies a *window*, and a window holds many tabs and splits — so every Claude
    session in that window received the same "terminal" id, wrote the same pointer, and
    read each other's. Observed exactly that way: two sessions in one window, one runs
    `charter ws use user-reporting`, and the other — which had no pointer of its own and
    so fell through to the terminal one — silently moved with it. `source()` said
    `terminal`; `set_active`'s docstring promised "selecting a workspace in one pane never
    changes another". Parallel workspaces are the feature; sharing one behind the user's
    back is the failure it exists to prevent.

    So the rule is now: key the pointer on something that identifies a pane, or do not
    write one at all. Returning ``None`` costs only the survives-a-restart convenience,
    and only in terminals that cannot tell their panes apart — `resolve` then falls to the
    per-session pointer, which Claude Code always provides, and to `default`. An id that
    is wrong in the sharing direction is worse than no id.
    """
    raw = explicit or next(
        (v for v in (os.environ.get(k) for k in _PANE_ID_VARS) if v), None)
    if not raw:
        try:
            raw = os.ttyname(0)              # controlling tty, if stdin is one — per pane
        except Exception:
            raw = None
    if not raw:
        return None
    tid = re.sub(r"[^A-Za-z0-9._-]", "-", raw.strip())
    return tid or None


def _terminal_file(tid: str) -> Path:
    return config.TERMINALS_DIR / f"{tid}.workspace"


def _read(f: Path) -> str | None:
    try:
        val = f.read_text().strip()
        return val or None
    except Exception:
        return None


def valid_name(name: str) -> bool:
    return bool(name) and name not in (".", "..") and _NAME_RE.match(name) is not None


def is_git_repo(path: Path) -> bool:
    return (path / ".git").exists()


def is_tree(path: Path) -> bool:
    """A working tree of either provenance — a clone (``.git`` is a directory) or a
    linked worktree (``.git`` is a file). :func:`is_clone` deliberately excludes the
    second; this is for callers that only need "is there a checkout here"."""
    return (Path(path) / ".git").exists()


def is_clone(path: Path) -> bool:
    """A real clone, not a worktree. Git itself draws the line: a clone's ``.git`` is a
    DIRECTORY, a linked worktree's ``.git`` is a FILE pointing at the shared gitdir. So a
    worktree can never be miscounted as a cloned repo — no bookkeeping required."""
    return (path / ".git").is_dir()


def workspace_dir(name: str) -> Path:
    return config.WORKSPACES_DIR / name


def from_path(path=None) -> str | None:
    """The workspace whose working tree *path* is inside, or ``None``.

    A workspace's trees live at known places — `workspaces/<ws>/<repo>/…` for a fleet
    clone, `<worktrees-root>/<ws>/<repo>/<piece>/…` for a worktree — so standing in one
    IS the answer to "which workspace am I in", and it is an answer no pointer can
    contradict. You cannot be in two directories at once, which is exactly the property
    the pointers lacked: a session that had never chosen anything inherited another
    session's choice through a shared terminal key.
    """
    from . import worktree
    try:
        here = Path(path or os.getcwd()).resolve()
    except (OSError, RuntimeError):
        return None

    loc = worktree.locate(here)          # (workspace, repo, piece) — handles both roots
    if loc:
        return loc[0]
    try:
        parts = here.relative_to(Path(config.WORKSPACES_DIR).resolve()).parts
    except (ValueError, OSError, RuntimeError):
        return None
    # `workspaces/<ws>` alone is the container, not a tree — only a repo inside it counts.
    return parts[0] if len(parts) >= 2 else None


def resolve(explicit: str | None = None, session_id: str | None = None) -> str:
    """Active workspace by precedence: ``--workspace`` → ``$CHARTER_WORKSPACE`` → **the
    tree you are standing in** → per-session pointer → per-terminal pointer → ``default``.

    The cwd sits above the pointers because it cannot be wrong: a workspace's trees live
    at paths that name the workspace, so being inside one is not a hint about which
    workspace is active, it is the fact. The pointers remain for the case with no tree to
    stand in — a shell at the plane root.
    """
    if explicit:
        return explicit
    env = os.environ.get("CHARTER_WORKSPACE")
    if env:
        return env.strip()
    here = from_path()
    if here:
        return here
    sid = _session_id(session_id)
    if sid:
        val = _read(_session_file(sid))
        if val:
            return val
    tid = _terminal_id()
    if tid:
        val = _read(_terminal_file(tid))
        if val:
            return val
    return config.DEFAULT_WORKSPACE


def source(explicit: str | None = None, session_id: str | None = None) -> str:
    """Human label for where the active workspace came from (for display)."""
    if explicit:
        return "--workspace"
    if os.environ.get("CHARTER_WORKSPACE"):
        return "$CHARTER_WORKSPACE"
    if from_path():
        return "cwd"      # must mirror `resolve`'s order, or the status line explains
                          # the active workspace by naming a source that did not decide it
    sid = _session_id(session_id)
    if sid and _read(_session_file(sid)):
        return "session"
    tid = _terminal_id()
    if tid and _read(_terminal_file(tid)):
        return "terminal"
    return "default"


def _lock_file(sid: str) -> Path:
    return config.SESSIONS_DIR / f"{sid}.lock"


def is_locked(session_id: str | None = None) -> str | None:
    """The workspace this session is **locked** to (i.e. confirmed via ``workspace
    use``/``create --use``), or ``None`` if the session hasn't confirmed one yet.

    A lock is what forbids switching *mid-session*: once set, ``set_active`` refuses
    to move to a different workspace unless ``force=True``. It's keyed by the Claude
    session id, so every new session starts unlocked and gets to choose afresh."""
    sid = _session_id(session_id)
    if not sid:
        return None
    return _read(_lock_file(sid))


def unlock(session_id: str | None = None) -> bool:
    """Drop this session's lock (an explicit escape hatch). Returns True if one was
    cleared. Used by ``workspace unlock``; ``set_active(..., force=True)`` re-locks."""
    sid = _session_id(session_id)
    if not sid:
        return False
    f = _lock_file(sid)
    if f.exists():
        f.unlink()
        return True
    return False


def set_active(name: str, session_id: str | None = None, force: bool = False) -> str:
    """Select ``name`` for THIS pane/session only, and **lock** the session to it.

    Writes a **per-terminal** pointer (keyed by a stable terminal id, so the pane
    keeps this workspace across closing/reopening Claude) and a **per-session**
    pointer (so the status line, which only knows the session id, agrees). Writes no
    global default, so selecting a workspace in one pane never changes another.

    **Session lock:** if this session is already locked to a *different* workspace,
    the switch is refused and ``"locked"`` is returned (nothing is written) — unless
    ``force=True``. Confirming a workspace locks the session to it, so the workspace
    can't be swapped out from under a running task. Returns the scope
    (``session`` | ``terminal`` | ``none``) on success, or ``"locked"`` when refused."""
    locked = is_locked(session_id)
    if locked and locked != name and not force:
        return "locked"
    config.STATE_DIR.mkdir(parents=True, exist_ok=True)
    scope = "none"
    tid = _terminal_id()
    if tid:
        config.TERMINALS_DIR.mkdir(parents=True, exist_ok=True)
        _terminal_file(tid).write_text(name + "\n")
        scope = "terminal"
    sid = _session_id(session_id)
    if sid:
        config.SESSIONS_DIR.mkdir(parents=True, exist_ok=True)
        _session_file(sid).write_text(name + "\n")
        _lock_file(sid).write_text(name + "\n")  # confirming = locking for the session
        scope = "session"
    _prune()
    return scope


def reconcile(session_id: str | None = None, terminal_id: str | None = None) -> str | None:
    """On session start: if this Claude session has no pointer yet but its terminal
    pane does, copy the pane's selection into the session pointer — so the status
    line (which only knows the session id) shows the workspace the pane was on before
    Claude was reopened. Returns the seeded workspace, or None."""
    sid = _session_id(session_id)
    if not sid or _read(_session_file(sid)):
        return None
    tid = _terminal_id(terminal_id)
    val = _read(_terminal_file(tid)) if tid else None
    if val:
        config.SESSIONS_DIR.mkdir(parents=True, exist_ok=True)
        _session_file(sid).write_text(val + "\n")
    return val


def _prune() -> None:
    cutoff = time.time() - _SESSION_MAX_AGE
    for d in (config.SESSIONS_DIR, config.TERMINALS_DIR):
        if not d.exists():
            continue
        for pattern in ("*.workspace", "*.lock", "*.configver", "*.memnudge", "*.usage"):
            for f in d.glob(pattern):
                try:
                    if f.stat().st_mtime < cutoff:
                        f.unlink()
                except OSError:
                    pass


def list_workspaces() -> list[str]:
    """Directory names under ``workspaces/`` that are workspaces (not stray clones)."""
    _ensure_layout()
    root = config.WORKSPACES_DIR
    if not root.exists():
        return []
    out = []
    for d in sorted(root.iterdir()):
        if d.is_dir() and not d.name.startswith(".") and not is_clone(d):
            out.append(d.name)
    return out


def clones(name: str) -> list[Path]:
    """The repo clones inside a workspace (``memory/`` and ``refs/`` are not clones —
    they have no ``.git`` — so this naturally excludes them)."""
    _ensure_layout()
    wd = workspace_dir(name)
    if not wd.exists():
        return []
    return sorted(d for d in wd.iterdir() if is_clone(d))


def root_tree() -> Path | None:
    """The codebase an **embedded** plane serves — its own root — or ``None``.

    In the embedded shape (`charter init` run inside the repo you already work in) there
    is nothing to clone: ``workspaces/`` holds no repos and the tree you edit IS the root.
    It belongs to no single workspace — it is in all of them, since there is physically
    one of it — which is what separates it from everything :func:`clones` returns.

    Two conditions, and the shape check is the load-bearing one. A **fleet** plane's root
    is very often a git repo as well (personas carry committed memory; docs/control-
    plane.md ships ``exclude = ["this-control-plane"]`` because the plane's own repo lives
    on the forge) — but there it is the plane, not a repo you work in, and listing it
    beside the workspace's real clones would be wrong. ``ROOT/.git`` cannot tell those
    apart, so :func:`charter.instance.shape_of` is asked instead.

    The ``is_clone`` half still matters: an embedded plane whose ``.git`` has been removed
    has no tree to offer, and a *worktree* of one (``.git`` is a file) is not the trunk.
    """
    try:
        if config.PLANE_SHAPE != "embedded":
            return None
        return config.ROOT if is_clone(config.ROOT) else None
    except OSError:
        return None


def own_tree(ws: str) -> Path | None:
    """The working tree that belongs to *ws* alone, in an **embedded** plane.

    Embedded means the codebase cannot be cloned apart, so a workspace that materialises
    nothing isolates nothing: `ws use alpha` then `ws use beta` changed the memory
    namespace and not one file being edited, while charter promised "never mix
    workspaces". Two agents in two workspaces were editing the same files on the same
    branch.

    So in an embedded plane a workspace **is** a working tree:

    - ``default`` owns the root tree. A solo user's experience stays `charter init` and
      work in your repo — charter starts materialising trees only when you ask for a
      SECOND concurrent thing, so the simplest case is never the strangest one.
    - every other workspace owns the worktree whose piece name equals the workspace name,
      i.e. ``<worktrees-root>/<ws>/<repo>/<ws>``. Reusing the existing piece layout rather
      than inventing a level: `worktree.dirs_for` keeps listing it, and a workspace's own
      tree is simply its first piece.

    ``None`` when the workspace has no tree yet — the state `workspace use` refuses.
    Always ``None`` in a fleet plane, where a workspace holds clones instead.
    """
    if config.PLANE_SHAPE != "embedded":
        return None
    rt = root_tree()
    if rt is None:
        return None
    if ws == config.DEFAULT_WORKSPACE:
        return rt
    from . import worktree
    p = worktree.path_for(ws, rt.name, ws)
    return p if is_tree(p) else None


def repo_trees(ws: str) -> list[Path]:
    """Every repo this workspace works in.

    The one list anything asking "which repos am I on?" should use — the status line's
    rows and `gl-refresh`'s fetch targets both come from here, so a repo can never be
    drawn without its forge state having been fetched, or fetched without being drawn.
    Splitting that decision in two is what left the root tree with a permanently empty
    CI column: it was rendered from one list and refreshed from another.

    Fleet: the plane's own repo (it has a branch like any other) plus this workspace's
    clones. Embedded: this workspace's OWN tree — the root for `default`, its worktree
    otherwise. Returning the root for every embedded workspace is what made them all look
    like the same workspace, because they were.
    """
    if config.PLANE_SHAPE == "embedded":
        # The workspace's OWN tree stands where the shared root used to — every embedded
        # workspace returning the same root is what made them all one workspace. Clones
        # still count: nothing stops `charter clone` in an embedded plane, and a hybrid
        # (the codebase plus a vendored dependency) is a reasonable thing to have.
        own = own_tree(ws)
        return ([own] if own is not None else []) + clones(ws)
    rt = root_tree()
    return ([rt] if rt is not None else []) + clones(ws)


def legacy_flat_clones() -> list[Path]:
    """Git repos sitting directly under ``workspaces/`` (pre-workspace layout)."""
    _ensure_layout()
    root = config.WORKSPACES_DIR
    if not root.exists():
        return []
    return sorted(d for d in root.iterdir() if d.is_dir() and is_clone(d))


def ensure(name: str) -> Path:
    """Create the workspace directory **and its baseline structure**; return its path.

    `scaffold` used to be called only by create/live/restore/fork, so a workspace born via
    `charter clone` or `charter workspace use` got a bare directory — and then the status
    line showed `⚠ reinit` on every turn, phrased as post-upgrade drift, for a workspace
    that had just been created correctly. The README's own quickstart ends that way.

    Scaffolding here rather than at each call site because "the directory exists" and "the
    directory is a workspace" were two different states with nothing keeping them in step;
    `needs_reinit` is meant to detect a plane left behind by an older charter, not one
    charter made a moment ago.
    """
    if not valid_name(name):
        raise ValueError(
            f"invalid workspace name '{name}' "
            "(use letters, digits, '.', '_', '-'; must not start with a dot)"
        )
    _ensure_layout()
    wd = workspace_dir(name)
    wd.mkdir(parents=True, exist_ok=True)
    try:
        scaffold(name)
    except Exception:
        pass          # best-effort: a workspace you can use beats one that failed to exist
    return wd


# --------------------------------------------------------------------------- #
# workspace memory + refs — a private, per-task journal beside the task's clones #
# (local/gitignored, like the clones; distinct from persona memory, which is the #
# shared, committed knowledge of a *role*).                                       #
# --------------------------------------------------------------------------- #
# --------------------------------------------------------------------------- #
# workspace manifest — the COMMITTED, shareable setup: which repos on which      #
# branches. Clones stay gitignored; this + memory/ are tracked, so a workspace   #
# becomes a reproducible team artifact (`restore` rebuilds it from here).        #
# --------------------------------------------------------------------------- #
# --------------------------------------------------------------------------- #
# live vs local workspaces. LOCAL (default) = fully private: clones, memory, and #
# manifest all gitignored, nothing committed. LIVE (opt-in) = shareable: its      #
# workspace.json + memory/ are un-ignored (via a managed .gitignore block) so the #
# commit/sync/auto-save flow applies. Liveness is recorded in that block, so it's #
# git-visible and travels with the control plane.                                  #
# --------------------------------------------------------------------------- #
_LIVE_BEGIN = "# >>> charter live workspaces (managed by `charter workspace live`) >>>"
_LIVE_END = "# <<< charter live workspaces <<<"


def _gitignore() -> Path:
    return config.ROOT / ".gitignore"


def live_workspaces() -> set[str]:
    """Names of workspaces marked LIVE (their un-ignore lines are in the managed block)."""
    try:
        text = _gitignore().read_text()
    except OSError:
        return set()
    out, inblock = set(), False
    for line in text.splitlines():
        s = line.strip()
        if s == _LIVE_BEGIN:
            inblock = True
        elif s == _LIVE_END:
            inblock = False
        elif inblock:
            m = re.match(r"!/workspaces/([^/]+)/workspace\.json", s)
            if m:
                out.add(m.group(1))
    return out


def is_live(name: str) -> bool:
    return name in live_workspaces()


def _live_block(names) -> str:
    """The managed .gitignore block un-ignoring every LIVE workspace's shareable paths.

    Each path is listed twice — the directory and its contents — because un-ignoring
    ``…/memory`` alone re-includes the directory entry and none of the files inside it.
    ``todos`` needs the same pair: a shared task list is one of the better reasons to make
    a workspace LIVE at all, and without a line here the list would simply never travel,
    which is the quietest way this could fail.
    """
    lines = [_LIVE_BEGIN]
    for n in sorted(names):
        lines += [f"!/workspaces/{n}/workspace.json", f"!/workspaces/{n}/workspace.md",
                  f"!/workspaces/{n}/memory", f"!/workspaces/{n}/memory/**",
                  f"!/workspaces/{n}/todos", f"!/workspaces/{n}/todos/**"]
    lines.append(_LIVE_END)
    return "\n".join(lines)


def _write_live_block(names) -> None:
    """Rewrite the managed block for exactly *names*, creating it if absent."""
    gi = _gitignore()
    text = gi.read_text() if gi.exists() else ""
    block = _live_block(names)
    if _LIVE_BEGIN in text:
        text = re.sub(re.escape(_LIVE_BEGIN) + r".*?" + re.escape(_LIVE_END), block,
                      text, flags=re.DOTALL)
    elif "!/workspaces/.gitkeep\n" in text:
        text = text.replace("!/workspaces/.gitkeep\n", "!/workspaces/.gitkeep\n" + block + "\n", 1)
    else:
        text = text.rstrip("\n") + "\n" + block + "\n"
    gi.write_text(text)


def refresh_live_block() -> None:
    """Regenerate the managed block from the workspaces that are already LIVE.

    Idempotent, and it changes no workspace's liveness — it only brings the block up to
    the paths this version of charter shares. A plane made LIVE before `todos/` existed
    has a block listing four paths per workspace and nothing re-runs `set_live` on its
    own, so without this the list silently never travels for exactly the people who
    adopted the feature earliest.
    """
    _write_live_block(live_workspaces())


def set_live(name: str, live: bool) -> bool:
    """Mark a workspace LIVE (un-ignore its manifest+memory) or LOCAL (re-ignore).
    Returns True if the liveness changed. Rewrites the managed .gitignore block."""
    names = live_workspaces()
    if (name in names) == live:
        return False
    names = names | {name} if live else names - {name}
    _write_live_block(names)
    return True


def _rename_active_pointers(old: str, new: str) -> None:
    """Repoint any per-session/per-terminal pointer + lock whose value is ``old`` to
    ``new``, so a renamed workspace stays the active/locked one for its session."""
    for d in (config.SESSIONS_DIR, config.TERMINALS_DIR):
        if not d.exists():
            continue
        for f in list(d.glob("*.workspace")) + list(d.glob("*.lock")):
            try:
                if f.read_text().strip() == old:
                    f.write_text(new + "\n")
            except OSError:
                pass


def rename(old: str, new: str) -> None:
    """Rename a workspace: move its directory (clones + memory + refs come along),
    update the manifest ``name``, move its liveness (gitignore block) if live, and
    repoint active pointers/lock. Filesystem-level; the caller commits the tracked
    move for a LIVE workspace. Assumes the caller validated old exists / new is free."""
    was_live = is_live(old)
    workspace_dir(old).rename(workspace_dir(new))
    m = read_manifest(new)
    if m:
        m["name"] = new
        write_manifest(new, m)
    if was_live:
        set_live(old, False)
        set_live(new, True)
    _rename_active_pointers(old, new)


def manifest_path(name: str) -> Path:
    return workspace_dir(name) / "workspace.json"


def read_manifest(name: str) -> dict:
    """The committed manifest ({name, description, repos:[{name,branch}], …}), or {}."""
    try:
        return json.loads(manifest_path(name).read_text())
    except (OSError, ValueError):
        return {}


def write_manifest(name: str, data: dict) -> None:
    ensure(name)
    manifest_path(name).write_text(json.dumps(data, indent=2) + "\n")


def memory_dir(name: str) -> Path:
    return workspace_dir(name) / "memory"


def refs_dir(name: str) -> Path:
    return workspace_dir(name) / "refs"


def notes_file(name: str) -> Path:
    return memory_dir(name) / "notes.md"


_WS_MEM_HEADER = (
    "# {name} — task memory\n\n"
    "One file per memory — a small, programmatically-explorable DB, not a single log to\n"
    "merge-conflict on. Files are timestamp-prefixed, so this index (and the directory) "
    "list chronologically. **Committed + shared** for LIVE workspaces. Write with "
    "`charter workspace remember \"…\"`, search with `charter workspace recall [--query …]`, drop "
    "one with `charter workspace forget <slug>`. Never put secrets here (vault only).\n"
)


def memory_index(name: str) -> Path:
    from . import memstore
    return memstore.index_path(memory_dir(name))


def scaffold_memory(name: str) -> Path:
    """Ensure the memory dir + MEMORY.md index; grandfather a legacy notes.md into the
    index so it stays discoverable. Returns the index path."""
    from . import memstore
    idx = memstore.ensure_index(memory_dir(name), _WS_MEM_HEADER.format(name=name))
    nf = notes_file(name)
    if nf.exists() and "(notes.md)" not in idx.read_text():
        memstore.index_append(idx, "notes.md", "Task memo (legacy)")
    return idx


def scaffold(name: str) -> None:
    """Create a workspace's baseline structure: memory/ (per-file DB + index), refs/, the
    workspace.md charter, and the structure-version marker. Idempotent + additive."""
    scaffold_memory(name)
    refs = refs_dir(name)
    refs.mkdir(parents=True, exist_ok=True)
    rr = refs / "README.md"
    if not rr.exists():
        rr.write_text(f"# {name} — task references\n\nDrop docs, links, and snippets for "
                      f"this task here (local, gitignored).\n")
    scaffold_charter(name)  # workspace.md — the living vision/context/glossary charter
    _structure_marker(name).write_text(str(STRUCTURE_VERSION) + "\n")  # stamp the layout version


def remember(name: str, text: str, title: str | None = None) -> Path:
    """Record one workspace memory as its own timestamp-prefixed file (and index it) — the
    task journal, structured exactly like persona memory. Returns the file path."""
    scaffold_memory(name)
    from . import memstore
    return memstore.write(memory_dir(name), text, title, timestamped=True, index=True)


def note(name: str, text: str) -> Path:
    """`note` is the long-standing verb for the same thing — an alias for remember()."""
    return remember(name, text)


def recall(name: str, query: str | None = None, limit: int = 8) -> list[tuple[Path, str, int]]:
    """Search the workspace's memories by keyword, or (no query) list them all
    chronologically. Returns [(path, title, score)]."""
    from . import memstore
    if query:
        return memstore.search([memory_dir(name)], query, limit)
    return [(p, t, 0) for p, t, _tx in memstore.entries(memory_dir(name))]


def forget_memory(name: str, ident: str) -> bool:
    """Delete one workspace memory (by slug or filename) and drop its index line."""
    from . import memstore
    return memstore.forget(memory_dir(name), ident)


def memories(name: str) -> list[Path]:
    from . import memstore
    return memstore.files(memory_dir(name))


def read_notes(name: str) -> str:
    """Legacy memo text (notes.md), '' if none — kept for pre-v2 workspaces."""
    f = notes_file(name)
    return f.read_text() if f.exists() else ""


# --------------------------------------------------------------------------- #
# workspace charter (workspace.md) — the LIVING, human+agent-readable context: #
# the task's Vision (north star), Context & decisions, and Glossary. Seeded at  #
# creation (ideally with a vision the developer describes), kept current as the #
# work evolves, committed for LIVE workspaces, and inherited by a fork — so     #
# anyone can pick up the task with full context. The append-only chronological  #
# "what was done" log stays in memory/notes.md; this is the curated "why/what". #
# --------------------------------------------------------------------------- #

def charter_file(name: str) -> Path:
    return workspace_dir(name) / "workspace.md"


_VISION_PLACEHOLDER = (
    "_Not set yet — describe the goal: what are we building or fixing, and why? "
    'Set it with `charter workspace vision "…"` (or edit this file)._'
)

_CHARTER_TEMPLATE = """# {name}

> **Living charter** for this workspace — its north star and shared context.
> Keep it current as the work evolves (edit this file, or `charter workspace vision "…"`).
> It's committed + shared for LIVE workspaces, and a fork inherits it — so anyone
> can pick up the task with full context. Never put secrets here (vault only).

## Vision

{vision}

## Context & decisions

<!-- Key facts, constraints, and design/architecture decisions found while working —
     the durable "why", not a chronological log. Grow this as you learn. -->

_Nothing yet._

## Glossary

<!-- Task/domain vocabulary so a teammate or a fork isn't lost: `term` — definition. -->

_Nothing yet._

## Log

Chronological "what was done" lives in the task memo — `memory/notes.md`
(append with `charter workspace note "…"`).
"""


def _replace_md_section(text: str, header: str, body: str) -> str:
    """Replace the body under a ``## <header>`` section (down to the next ``## `` or
    EOF), keeping the header line. Appends the section if it's absent."""
    lines = text.splitlines()
    out: list[str] = []
    i, n = 0, len(lines)
    hdr = re.compile(rf"^##\s+{re.escape(header)}\s*$", re.IGNORECASE)
    replaced = False
    while i < n:
        out.append(lines[i])
        if hdr.match(lines[i]):
            i += 1
            while i < n and not lines[i].startswith("## "):
                i += 1
            out += ["", body.strip(), ""]
            replaced = True
            continue
        i += 1
    result = "\n".join(out).rstrip() + "\n"
    if not replaced:
        result += f"\n## {header}\n\n{body.strip()}\n"
    return result


def scaffold_charter(name: str, vision: str | None = None) -> None:
    """Create workspace.md from the template if missing; if a vision is given, set it."""
    cf = charter_file(name)
    if not cf.exists():
        cf.parent.mkdir(parents=True, exist_ok=True)
        cf.write_text(_CHARTER_TEMPLATE.format(
            name=name, vision=(vision.strip() if vision else _VISION_PLACEHOLDER)))
    elif vision:
        set_vision(name, vision)


def set_vision(name: str, text: str) -> None:
    """Set/replace the charter's ## Vision section (creating the charter if needed)."""
    scaffold_charter(name)
    cf = charter_file(name)
    cf.write_text(_replace_md_section(cf.read_text(), "Vision", text))


def read_charter(name: str) -> str:
    cf = charter_file(name)
    return cf.read_text() if cf.exists() else ""


def read_vision(name: str) -> str:
    """The ## Vision section body, or "" if unset/placeholder."""
    m = re.search(r"^##\s+Vision\s*$(.*?)(?=^##\s|\Z)",
                  read_charter(name), re.MULTILINE | re.DOTALL)
    body = (m.group(1).strip() if m else "")
    return "" if body.startswith("_Not set yet") else body


# --------------------------------------------------------------------------- #
# workspace STRUCTURE VERSION — the durable upgrade anchor. A workspace created  #
# by an older version of charter can lack files a newer one expects (workspace.md, #
# refs/, …). We stamp a tiny local marker (.charter-structure) with the layout   #
# version scaffold() produces; a workspace whose marker is missing/older, or that #
# is missing a baseline file, is "stale" and flagged (status line) until          #
# `charter workspace reinit` heals it. To ship a new structural element in future:    #
# create it in scaffold() and bump STRUCTURE_VERSION — every old workspace is then #
# auto-detected and one command upgrades it. The marker is local (regenerated by   #
# scaffold/restore), never committed.                                              #
# --------------------------------------------------------------------------- #

STRUCTURE_VERSION = 2  # v2: memory is a per-file DB (MEMORY.md index), not a lone notes.md
_STRUCTURE_MARKER = ".charter-structure"
_LEGACY_STRUCTURE_MARKER = ".edm-structure"   # pre-rename; migrated in place on read


def _structure_marker(name: str) -> Path:
    """The marker path, migrating a pre-rename ``.edm-structure`` the first time.

    Renaming the marker without moving it would silently reset every existing
    workspace to v0: the new name isn't there, so a fully up-to-date workspace
    reads as stale and gets flagged for reinit. Harmless (reinit is additive and
    idempotent) but wrong, noisy, and on a LIVE workspace it manufactures a
    commit. Rename rather than re-stamp, so a genuinely older marker keeps its
    own version instead of being claimed as current.
    """
    d = workspace_dir(name)
    new = d / _STRUCTURE_MARKER
    legacy = d / _LEGACY_STRUCTURE_MARKER
    if not new.exists() and legacy.exists():
        try:
            legacy.rename(new)
        except OSError:
            return legacy          # unreadable/cross-device: still read the old one
    elif new.exists() and legacy.exists():
        legacy.unlink(missing_ok=True)   # both present: the new one already won
    return new


def _required_components(name: str) -> dict[str, Path]:
    """Baseline files every workspace should have — all created idempotently by
    scaffold(). Add to this (and bump STRUCTURE_VERSION) when the layout grows."""
    return {
        "workspace.md": charter_file(name),
        "memory/MEMORY.md": memory_index(name),
        "refs/README.md": refs_dir(name) / "README.md",
    }


def structure_version(name: str) -> int:
    """The layout version stamped in the workspace's marker (0 if missing/unreadable)."""
    try:
        return int(_structure_marker(name).read_text().strip())
    except (OSError, ValueError):
        return 0


def structure_status(name: str) -> dict:
    """{'ok', 'missing': [rel…], 'version', 'target'} — is the workspace's on-disk
    layout current? ``ok`` iff no baseline file is missing AND the marker is up to date."""
    missing = sorted(rel for rel, p in _required_components(name).items() if not p.exists())
    ver = structure_version(name)
    return {"ok": (not missing) and ver >= STRUCTURE_VERSION,
            "missing": missing, "version": ver, "target": STRUCTURE_VERSION}


def needs_reinit(name: str) -> bool:
    """True if an existing workspace's structure is stale (missing files or old marker)."""
    return workspace_dir(name).exists() and not structure_status(name)["ok"]


def reinit(name: str) -> dict:
    """Idempotently bring a workspace up to the current structure — create any missing
    baseline files and stamp the version marker. Additive: never destroys existing
    content. Returns the pre-reinit status (what was missing / the old version)."""
    before = structure_status(name)
    scaffold(name)  # creates memory/refs/workspace.md if missing + stamps the marker
    # Structure is not only what lives inside the workspace directory: which of its paths
    # are SHARED is part of the layout too, and that lives in the managed .gitignore block.
    # A plane made LIVE before `todos/` existed lists four paths per workspace and nothing
    # re-runs `set_live` unprompted — so the upgrade command is where it gets repaired.
    refresh_live_block()
    return before


def banner(active: str, explicit: str | None = None) -> None:
    """Print which workspace a command is acting on — surfaced everywhere so an
    agent always knows its boundary."""
    util.info(f"workspace: {active}  (via {source(explicit)})")
