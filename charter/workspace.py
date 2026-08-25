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

from . import config, contain, instance, util

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


def for_session(sid: str) -> str | None:
    """The workspace explicitly chosen FOR *sid*, or ``None`` if nobody chose one.

    The per-session pointer rung of :func:`resolve`, asked about a session that is not
    necessarily this process's — which is the whole reason it is public. Inside a charter
    frame the frame **is** the charter session (`docs/frame.md`, ADR 0019), so
    `charter workspace use <name>` typed at the agent writes this file under the FRAME's
    id, and that is the documented mechanism by which it "moves the panels too".

    A panel needs to tell that choice apart from the frame's launch-time answer
    (`frame/state.py`'s `record_workspace`, #512): the launcher's answer is a SEED, and an
    operator choosing inside the running frame outranks it. Asking this directly is what
    keeps that distinction a property rather than a spelling — the alternative was reading
    :func:`source`'s human-facing label and matching the string ``"session"``, which is a
    sentence written for a status line, not an API.

    Name-checked like every other rung that hands a value to :func:`workspace_dir`'s join.
    The ``val and`` in front of it is a None-guard rather than a second name check —
    :func:`_read` answers ``None`` for a missing or empty file, and `valid_name` takes a
    string.
    """
    val = _read(_session_file(sid)) if sid else None
    return val if val and valid_name(val) else None


def _terminal_id(explicit: str | None = None) -> str | None:
    """This terminal PANE's id, or ``None`` — see :func:`charter.session.terminal`,
    which owns it. Kept as a module-level name because it is the seam tests patch to
    simulate a pane-less shell, and because `persona` asks the same question: two
    copies of the WINDOWID lesson is one copy too many."""
    from . import session as _session
    return _session.terminal(explicit)


def _terminal_file(tid: str) -> Path:
    return config.TERMINALS_DIR / f"{tid}.workspace"


def _read(f: Path) -> str | None:
    try:
        val = f.read_text().strip()
        return val or None
    except Exception:
        return None


def valid_name(name: str) -> bool:
    """Can *name* name a workspace? **The one place that answers this.**

    Delegates to :func:`instance.workspace_name_ok` rather than keeping a second regex
    here, because ``[workspace] default`` is read during ``config``'s bootstrap — before
    this module can be imported — and two copies of the rule are how a reading site and a
    creation site come to disagree (:mod:`charter.contain`).
    """
    return instance.workspace_name_ok(name)


def _unreadable(fn) -> bool:
    """Run a filesystem predicate, treating "I am not allowed to look" as "no".

    ``pathlib`` does NOT count ``EACCES`` among the errors it swallows — only ENOENT,
    ENOTDIR, EBADF and ELOOP — so ``(d / ".git").is_dir()`` *raises* on a directory the
    process cannot enter. It raises on Linux and returns False on macOS, which is how a
    suite green on a laptop goes red on CI.

    Every caller here is asking "is there a checkout at this path", and the honest answer
    for a directory we cannot read is no. Raising instead means one unreadable directory
    anywhere under ``workspaces/`` takes down every caller that scans it — including the
    status line, whose failure mode is a blank footer on every turn.
    """
    try:
        return fn()
    except OSError:
        return False


def is_git_repo(path: Path) -> bool:
    return _unreadable(lambda: (path / ".git").exists())


def is_tree(path: Path) -> bool:
    """A working tree of either provenance — a clone (``.git`` is a directory) or a
    linked worktree (``.git`` is a file). :func:`is_clone` deliberately excludes the
    second; this is for callers that only need "is there a checkout here"."""
    return _unreadable(lambda: (Path(path) / ".git").exists())


def is_clone(path: Path) -> bool:
    """A real clone, not a worktree. Git itself draws the line: a clone's ``.git`` is a
    DIRECTORY, a linked worktree's ``.git`` is a FILE pointing at the shared gitdir. So a
    worktree can never be miscounted as a cloned repo — no bookkeeping required."""
    return _unreadable(lambda: (path / ".git").is_dir())


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


def clone_of(path=None) -> tuple[str, str] | None:
    """``(workspace, repo)`` when *path* is inside a **clone**, else ``None``.

    The clone counterpart to `worktree.locate`, and deliberately exclusive of it: a path
    inside a worktree answers ``None`` here, so a caller asking both questions can never
    record the same directory twice under two identities.

    ``None`` for the plane root, which is the point — the root already carries an alert
    whose entire message is *work belongs in a workspace clone*, and marking who is present
    there would decorate the thing charter is telling you to stop doing. ``None`` too for
    ``workspaces/<ws>`` itself, which is a container rather than a tree.

    Path arithmetic only, no git and no subprocess: this is reached from a hook that fires
    every turn and from the status line's render path.
    """
    from . import worktree
    try:
        here = Path(path or os.getcwd()).resolve()
    except (OSError, RuntimeError):
        return None
    if worktree.locate(here):
        return None
    try:
        parts = here.relative_to(Path(config.WORKSPACES_DIR).resolve()).parts
    except (ValueError, OSError, RuntimeError):
        return None
    if len(parts) < 2:
        return None
    # A dotted second segment is charter's own furniture (`.worktrees/`), never a repo.
    # `worktree.locate` catches those whose layout it knows; this catches the rest rather
    # than inventing a repo called `.worktrees`.
    if parts[1].startswith("."):
        return None
    return parts[0], parts[1]


#: A committed, deliberately-chosen fallback workspace — `workspaces/.default`.
#:
#: NOT the "last active workspace" pointer #124 rejected, and the distinction is the whole
#: argument. That one is IMPLICIT: written by every `workspace use`, changing under sessions
#: that never asked, which is the failure `_terminal_id` was hardened against ("an id that is
#: wrong in the sharing direction is worse than no id"). This is EXPLICIT: set once by a
#: human, stable, and read only when every other rung has missed. `charter persona default`
#: is exactly this shape and already ships.
DEFAULT_FILE = ".default"


def default_file() -> Path:
    return config.WORKSPACES_DIR / DEFAULT_FILE


def declared_default() -> str | None:
    """The workspace nominated by `charter workspace default`, or ``None``.

    **Gated like every other committed file charter reads a name out of.** This dotfile is
    ordinarily committable (see :func:`set_declared_default`), so the value is a
    teammate's, and `resolve()` hands whatever it returns to `workspace_dir()`, which joins
    it onto ``workspaces/``. Unguarded, ``../../esc`` here made `workspace current`,
    `workspace vision` and `read_manifest` report content from outside the plane (#442).
    `valid_name` is the same rule `persona.default_persona` keeps for its own twin.

    Two checks, not one, because they answer different questions. `file_refusal` is about
    the *path*: this rung is read by `resolve()` on every status-line paint, so a FIFO
    committed at ``workspaces/.default`` would hang the paint rather than cost it a value.
    `valid_name` is about the *name* inside it. Neither stands in for the other.

    Never raises: a hook may cost a session its briefing and never its turn.
    """
    f = default_file()
    if contain.file_refusal(f):
        return None
    try:
        val = f.read_text().strip()
    except OSError:
        return None
    return val if val and valid_name(val) else None


def set_declared_default(name: str) -> None:
    """Nominate *name*. Raises ``ValueError`` for a name :func:`declared_default` would
    refuse to read back — writing a value the reader discards is a setting that silently
    does nothing, which is worse than an error."""
    name = name.strip()
    if not valid_name(name):
        raise ValueError(
            f"invalid workspace name '{name}' "
            "(use letters, digits, '.', '_', '-'; must not start with a dot)"
        )
    # A fixed name directly under `workspaces/`, which the default ignore rule
    # (`/workspaces/*/*`) does not match — so it is an ordinarily committable path, and a
    # link there redirects this write (#349).
    d = contain.writable(default_file())
    d.parent.mkdir(parents=True, exist_ok=True)
    d.write_text(name + "\n")


def clear_declared_default() -> None:
    try:
        default_file().unlink()
    except OSError:
        pass


def resolve(explicit: str | None = None, session_id: str | None = None,
            cwd=None) -> str:
    """Active workspace by precedence: ``--workspace`` → ``$CHARTER_WORKSPACE`` → **the
    tree you are standing in** → per-session pointer → per-terminal pointer → ``default``.

    The cwd sits above the pointers because it cannot be wrong: a workspace's trees live
    at paths that name the workspace, so being inside one is not a hint about which
    workspace is active, it is the fact. The pointers remain for the case with no tree to
    stand in — a shell at the plane root.

    ``cwd`` names *whose* directory that rung should read, and exists because the process
    asking is not always the session being described. A status line is the case: Claude
    Code runs the hook and passes the session's directory in the payload, so a renderer
    reading ``os.getcwd()`` would answer for the hook. Callers that ARE the session — every
    CLI command — leave it unset and get the process cwd, which is the same fact.
    """
    if explicit:
        return explicit
    env = os.environ.get("CHARTER_WORKSPACE")
    if env:
        return env.strip()
    here = from_path(cwd)
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
    # Below both pointers, above the built-in. What this replaces is not a considered
    # answer — it is a literal `default` workspace nobody chose either — so slotting a
    # nominated one here does not make workspaces less per-task; it makes the FALLBACK
    # something a human picked, and lets `default` go back to meaning "nobody ever chose"
    # (#193, unparking #124 on its own stated trigger: a terminal in common use that
    # supplies no pane id).
    declared = declared_default()
    if declared:
        return declared
    return config.DEFAULT_WORKSPACE


def source(explicit: str | None = None, session_id: str | None = None,
           cwd=None) -> str:
    """Human label for where the active workspace came from (for display).

    Takes ``cwd`` for the same reason :func:`resolve` does, and must be called with the
    same one: a header that named the workspace from the session's directory and the
    *reason* from the process's would explain the answer by naming a rung that did not
    decide it.
    """
    if explicit:
        return "--workspace"
    if os.environ.get("CHARTER_WORKSPACE"):
        return "$CHARTER_WORKSPACE"
    if from_path(cwd):
        return "cwd"      # must mirror `resolve`'s order, or the status line explains
                          # the active workspace by naming a source that did not decide it
    sid = _session_id(session_id)
    if sid and _read(_session_file(sid)):
        return "session"
    tid = _terminal_id()
    if tid and _read(_terminal_file(tid)):
        return "terminal"
    if declared_default():
        return "declared default"
    # Say WHY nothing answered, not just that nothing did. The operator's complaint was
    # "why are you in default workspace again?" — a surface asserting an answer with no
    # reason, twice, where reconstructing it meant reading `resolve`. ADR 0013's second
    # rule, aimed at the line people read every turn.
    if not _terminal_id():
        return "default (no pane id — nothing persists between sessions)"
    return "default (nothing selected)"


def _trace(event: str, session_id: str | None, **fields) -> None:
    """Record one workspace-selection event, best-effort.

    `persona.set_active` has recorded `persona-use` since the trace existed; the function
    that writes both workspace pointers AND the session lock recorded nothing, so "who
    moved my workspace" could only ever be answered from the pointer files themselves —
    which say what they hold and never who wrote it, when, or why. #254 is what that costs:
    two investigations, two confident wrong conclusions, settled in the end by a harness
    transcript charter cannot rely on existing.

    Swallows everything. Observability must never break the thing it observes, and a
    selection that failed because its own audit line failed would be the worst possible
    trade.
    """
    try:
        from . import session as _session, trace as _trace_mod
        _trace_mod.record(event, session=_session.current(session_id), **fields)
    except Exception:
        pass


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
        _trace("workspace-refused", session_id, workspace=name, locked_to=locked)
        return "locked"
    config.private_mkdir(config.STATE_DIR)
    tid = _terminal_id()
    if tid:
        config.private_mkdir(config.TERMINALS_DIR)
        _terminal_file(tid).write_text(name + "\n")
    sid = _session_id(session_id)
    if sid:
        config.private_mkdir(config.SESSIONS_DIR)
        _session_file(sid).write_text(name + "\n")
        _lock_file(sid).write_text(name + "\n")  # confirming = locking for the session
    _prune()
    _trace("workspace-use", session_id, workspace=name,
           scope=("terminal" if tid else "session" if sid else "none"),
           forced=True if force and locked and locked != name else None)
    # The scope is the REACH of what was written, so it names the longest-lived pointer
    # that actually landed — and the terminal one outlives the session one. These used to
    # be assigned in sequence, so the session branch overwrote the terminal branch and
    # every caller was told `session`. `_scope_note` reads persistence out of this value,
    # so a pane that HAD kept its workspace across restarts was told it had not, and a
    # shell with no pane id at all was told it had, which is the direction that costs
    # someone their selection with nothing having said so.
    return "terminal" if tid else "session" if sid else "none"


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
        config.private_mkdir(config.SESSIONS_DIR)
        _session_file(sid).write_text(val + "\n")
        # The one pointer write nobody typed. If any write is ever going to look as though
        # it came from nowhere, it is this one — so it says where it came from.
        _trace("workspace-seeded", session_id, workspace=val, **{"from": "terminal"})
    return val


def _prune() -> None:
    """Drop every per-session pointer past the cutoff — the DIRECTORY, not a list of names.

    This enumerated five suffixes (`*.workspace`, `*.lock`, `*.configver`, `*.memnudge`,
    `*.usage`) and read as an exhaustive list of the marker family while being nothing of
    the kind. Three families were missing by the time anyone looked: `*.ask-pending`,
    `*.route-pending` (`hooks`) and `*.persona` (`persona`, in both directories). The
    allowlist drifted three times, and a reader adding a fourth marker type had nothing
    telling them this list needed editing.

    The list is gone rather than three names longer, because a fourth drift is otherwise
    just a matter of time — a family added tomorrow is now covered the day it is written.
    Both directories are charter's own state and hold nothing but per-session and
    per-terminal pointers, so there is no member for which keeping it past the cutoff is
    the right answer. Files only: a directory in here is not a pointer, and unlinking is
    not the tool for one.

    Pruning `*.ask-pending` was checked against its readers rather than assumed safe — a
    declined ask deliberately leaves its marker behind, and that asymmetry is what makes
    "asked N, approved M" countable (#290). Nothing globs these suffixes: every reader
    (`_ask_mark_take`, `_route_mark_take`, `_route_mark_clear`) addresses one file by exact
    ids, and the tally itself lives in the trace store, which this does not touch.
    """
    cutoff = time.time() - _SESSION_MAX_AGE
    for d in (config.SESSIONS_DIR, config.TERMINALS_DIR):
        if not d.exists():
            continue
        for f in d.iterdir():
            try:
                if f.is_file() and f.stat().st_mtime < cutoff:
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


def repo_trees(ws: str) -> list[Path]:
    """Every repo this workspace works in — its clones, and nothing else.

    The one list anything asking "which repos am I on?" should use — the status line's
    rows and `gl-refresh`'s fetch targets both come from here, so a repo can never be
    drawn without its forge state having been fetched, or fetched without being drawn.
    Splitting that decision in two is what left a tree with a permanently empty CI column:
    it was rendered from one list and refreshed from another.

    The **plane root is deliberately not here**, and its absence is a decision rather than
    an oversight. The root is a git repo — personas carry committed memory, and a plane's
    own repo usually lives on a forge — but it is the plane, not a repo you work in.
    Listing it beside a workspace's clones would invite exactly the thing charter is trying
    to stop: two sessions editing one working tree while reporting two workspaces.

    Note what that does NOT buy: not listing the root does not prevent anyone working in
    it. That gap is why the status line and `doctor` warn about a dirty or off-branch root
    (docs/adr/0008).
    """
    return clones(ws)


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
    """The committed manifest ({name, description, repos:[{name,branch}], …}), or {}.

    Gated like :func:`read_charter`, and for the same reason: `workspace.json` is committed,
    and this is where `restore` and `fork` learn which repos to clone."""
    p = manifest_path(name)
    if contain.dir_refusal(p.parent) or contain.file_refusal(p):
        return {}
    try:
        return json.loads(p.read_text())
    except (OSError, ValueError):
        return {}


def write_manifest(name: str, data: dict) -> None:
    ensure(name)
    contain.writable(manifest_path(name)).write_text(json.dumps(data, indent=2) + "\n")


def merge_repo_rows(manifest_rows, disk_rows) -> tuple[list[dict], list[str]]:
    """Union of what a workspace RECORDED and what it actually HAS.

    charter has two answers to "which repos are in this workspace" and they are both
    right about different questions. `status` and `workspace list` scan the directory:
    always current, never portable. The manifest is a **snapshot**: portable and
    committed, but written only by `charter workspace snapshot`, which deliberately
    refuses while a repo has unpushed work so a recorded branch can actually be restored.

    Nothing reconciled them, and `fork` read the manifest alone. So a workspace with nine
    clones and no snapshot reported nine repos everywhere a human looked and inherited
    zero — issue #81, observed live with the whole point of forking a task environment
    silently defeated.

    Taking the union rather than picking a winner is what keeps both cases working: a
    workspace nobody snapshotted still forks its clones, and a teammate who has just
    cloned the plane — with no repos on disk at all — still inherits from the snapshot.

    Where both know a repo, the **manifest's** branch wins: it was recorded under
    `snapshot`'s guarantee that the branch was pushed, where the disk only knows whatever
    happens to be checked out now.

    Returns ``(rows, disk_only)`` — rows sorted by name, and the names no snapshot
    recorded, which the caller reports rather than silently passing off as snapshotted.
    """
    by_name: dict[str, dict] = {}
    for r in disk_rows or []:
        n = str(r.get("name") or "").strip()
        if n:
            by_name[n] = {"name": n, "branch": r.get("branch") or "HEAD"}
    disk_only = set(by_name)
    for r in manifest_rows or []:
        n = str(r.get("name") or "").strip()
        if not n:
            continue
        disk_only.discard(n)
        by_name[n] = {"name": n,
                      "branch": r.get("branch") or by_name.get(n, {}).get("branch") or "HEAD"}
    return [by_name[n] for n in sorted(by_name)], sorted(disk_only)


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


def forget_memory(name: str, ident: str):
    """Delete one workspace memory (by slug or filename) and drop its index line.

    Returns the removed path (falsy when nothing matched) so the caller can stage the
    deletion — see `memstore.forget`."""
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
    # `read_charter` below already refuses a `workspace.md` that resolves out of the
    # plane; this is the same file, written. A guard on one side of one name is how the
    # write half of #336 stayed open after the read half closed (#349).
    cf = contain.writable(charter_file(name))
    if not cf.exists():
        cf.parent.mkdir(parents=True, exist_ok=True)
        cf.write_text(_CHARTER_TEMPLATE.format(
            name=name, vision=(vision.strip() if vision else _VISION_PLACEHOLDER)))
    elif vision:
        set_vision(name, vision)


def set_vision(name: str, text: str) -> None:
    """Set/replace the charter's ## Vision section (creating the charter if needed)."""
    scaffold_charter(name)
    cf = contain.writable(charter_file(name))
    cf.write_text(_replace_md_section(cf.read_text(), "Vision", text))


def read_charter(name: str) -> str:
    cf = charter_file(name)
    # `workspace.md` is committed, and the SessionStart digest reads one per workspace on
    # this plane for its vision line — so an entry that blocks costs every session its
    # briefing, and one that never ends costs more than that (#336).
    #
    # BOTH questions, which is `file_refusal`'s own stated precondition ("a path that is
    # not a link cannot have moved relative to the directory it was listed from, which the
    # caller checked once with `dir_refusal`") and what `persona.definition_refusal` has
    # always asked. Asking only the file half left the variant the file half structurally
    # cannot see: when the DIRECTORY is the link, `workspace.md` inside it is an ordinary
    # regular file with nothing to object to. Measured on 0.51.0 — a committed
    # `workspaces/evil -> ../../esc` with `[workspace] default = "evil"`, a legal workspace
    # name, printed the outside charter through `workspace vision` (#442). Containing the
    # NAME does not contain this; the name was never the wrong part.
    #
    # The fast path in `within_data` is one `lstat` here, because `workspaces/` is itself a
    # data root — so the SessionStart digest pays a syscall per workspace, not a resolve.
    if contain.dir_refusal(cf.parent) or contain.file_refusal(cf):
        return ""
    return cf.read_text() if cf.exists() else ""


def last_active(name: str) -> float | None:
    """When this workspace was last *worked* — a unix timestamp, or ``None``.

    One function, because two questions that sound different are the same one: "when did
    someone last write here" (memory, todos, the manifest, a piece record) and "when did a
    session last select it" (a pointer file naming it). A workspace can be chosen and then
    only read from, and one that reported nothing in that case would look abandoned on the
    exact day somebody was in it.

    Derived at read time from mtimes, never cached: ADR 0011's rule is that the record holds
    only what git cannot know, and "when was this touched" is something the filesystem
    already answers. Best-effort — an unreadable workspace is undated, not an exception.
    """
    d = config.WORKSPACES_DIR / name
    best: float | None = None

    def bump(p: Path) -> None:
        nonlocal best
        try:
            m = p.stat().st_mtime
        except OSError:
            return
        if best is None or m > best:
            best = m

    for f in (d / "workspace.md", d / "workspace.json"):
        if f.exists():
            bump(f)
    for sub in ("memory", "todos", "pieces", "refs"):
        try:
            for f in (d / sub).iterdir():
                bump(f)
        except OSError:
            continue
    try:
        for f in config.SESSIONS_DIR.glob("*.workspace"):
            if _read(f) == name:
                bump(f)
    except OSError:
        pass
    return best


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
