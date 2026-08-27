"""Static configuration and well-known paths for the control plane.

Every value below is a function of ONE input — the control-plane root — so they are
computed in one place, :func:`derive`, and applied to this module by :func:`use`.

They used to be twenty-five separate module-level statements, and `tests/_isolation.py`
re-implemented all of them line for line to point a test at a temp directory. That
duplication has already failed in production: four constants were missing from the
harness, so the suite wrote fixture data into a contributor's real `.charter/vaults.json`
and orphaned every vault registered on that machine. The guard added in response can only
inspect `Path`-typed names; seven non-`Path` ones of exactly the same shape exist.

With one definition, a new setting is isolated the day it is added, because the harness
calls `use()` rather than knowing what to copy.
"""

from __future__ import annotations

import contextlib
import json
import os
import sys
from pathlib import Path

from . import instance as _instance
from . import legacyenv as _legacyenv
from . import root as _root

#: Fallbacks used when a control plane declares nothing (or none was found).
GROUP_FALLBACK = ""
DEFAULT_WORKSPACE_FALLBACK = "default"

#: The cross-persona namespace: ``personas/_shared/{memory,refs}`` is knowledge
#: every persona can read/write. Not itself a persona (excluded from listings).
#: Not derived — it is the same name in every control plane.
SHARED_PERSONA = "_shared"


def worktrees_root_for(root: "Path", cfg: dict) -> "Path | None":
    """An explicitly relocated worktree root, or ``None`` for the default.

    ``$CHARTER_WORKTREES`` → ``[plane] worktrees`` → ``None``. Relative values resolve
    against ROOT, so ``"../charter.worktrees"`` reads as written.

    ``None`` means the standard layout: ``workspaces/<ws>/.worktrees/``, which is already
    outside every clone — the whole point of that path, and correct whenever a workspace
    holds clones, which is now always. There used to be a second default here, for a plane
    whose clone WAS its own root; that shape is gone.

    Only the worktrees ever move. ``workspaces/<ws>/memory/`` and ``refs/`` stay put — a
    few KB of text that ``charter workspace live`` exists specifically to un-ignore so a
    team can commit them. Relocating those would break sharing to fix a problem they do
    not have.

    Takes *root*/*cfg* rather than reading the module globals so the test harness can
    re-derive it against a temp ROOT the way it already does for GROUP, EXCLUDE and the
    rest. When this read the globals directly it defaulted to the REAL repo's sibling in
    every test — outside the tmp tree — so the suite wrote worktrees into the developer's
    checkout and accumulated them across cases.

    **The two sources are not the same kind of input** (#339). ``$CHARTER_WORKTREES`` is
    set by the person at the machine, on their own machine, and takes anything. ``[plane]
    worktrees`` is committed and shared — a teammate writes it, `git worktree add` then
    creates directories wherever it points, and on 0.47.2 ``"~/../../etc/charter-worktrees"``
    resolved to ``/private/etc/charter-worktrees`` with nothing between the two. A committed
    value is now held to :func:`contain.plane_adjacent`: the plane, or one sibling of it,
    which is exactly the ``"../charter.worktrees"`` shape this docstring documents.

    A refused value falls back to ``None`` — the standard in-plane layout — because this
    runs inside :func:`derive`, on every import, where raising would cost the CLI rather
    than the setting. Falling back silently would be its own defect, so
    ``doctor.check_control_plane_config`` asks :func:`contain.plane_adjacent_refusal` the
    same question and names it.
    """
    from . import contain
    env = os.environ.get("CHARTER_WORKTREES")
    declared = env or _instance.worktrees_of(cfg)
    if not declared:
        return None
    p = Path(declared).expanduser()
    p = p if p.is_absolute() else (root / p)
    if not env and not contain.plane_adjacent(root, p):
        return None
    try:
        return p.resolve()
    except (OSError, RuntimeError):
        return p       # unresolvable (symlink loop, vanished parent) — still usable


def _migrate_state_dir(root: Path) -> Path:
    """Resolve the state directory, migrating a legacy ``.edm/`` to ``.charter/`` once.

    - ``$CHARTER_HOME`` set → use it verbatim, no migration (the user chose a path).
    - Neither ``.charter/`` nor ``.edm/`` exists → the new default; nothing to migrate.
    - ``.charter/`` doesn't exist, ``.edm/`` does → ``os.rename`` it to ``.charter/``
      in place (atomic on the same filesystem, preserves permissions including 0600
      vault files) and print a one-line notice to stderr.
    - Both exist → never merge; warn on stderr naming both paths and keep using
      ``.charter/``.
    - The rename fails (e.g. cross-device) → don't crash; print an actionable error
      naming both paths and fall back to the legacy directory so vault access is
      never silently lost.
    """
    override = os.environ.get("CHARTER_HOME")
    if override:
        return Path(override)

    new_dir = root / ".charter"
    legacy_dir = root / ".edm"

    if new_dir.exists():
        if legacy_dir.exists():
            print(f"charter: both {new_dir} and {legacy_dir} exist — using {new_dir} and "
                  f"leaving {legacy_dir} untouched (never auto-merged). Remove the old "
                  "directory once you've confirmed nothing is missing.", file=sys.stderr)
        return new_dir

    if legacy_dir.exists():
        try:
            os.rename(legacy_dir, new_dir)
        except OSError as e:
            print(f"charter: could not migrate {legacy_dir} to {new_dir} ({e}) — "
                  f"continuing to use {legacy_dir}. Move it to {new_dir} manually, or set "
                  "$CHARTER_HOME to choose a location.", file=sys.stderr)
            return legacy_dir
        _repoint_vault_registry(legacy_dir, new_dir)
        print(f"charter: migrated state directory {legacy_dir} -> {new_dir}", file=sys.stderr)
        return new_dir

    return new_dir


def _repoint_vault_registry(legacy_dir: Path, new_dir: Path) -> None:
    """Rewrite absolute vault paths that still point into the moved directory.

    ``vaults.json`` stores each plain-file vault's location as an **absolute**
    path. Renaming the state directory therefore moves the vault files while
    leaving the registry pointing at where they used to be — every vault then
    reports "not created yet" and the credentials look lost, which is the exact
    outcome this migration exists to avoid.

    Only entries under ``legacy_dir`` are touched; a vault deliberately stored
    somewhere else (a shared drive, a per-machine path) is left alone. Any
    failure here is non-fatal and reported: the files themselves are already
    safely moved, and a stale registry is repairable by hand.
    """
    registry = new_dir / "vaults.json"
    if not registry.exists():
        return
    old_prefix = f"{legacy_dir}{os.sep}"
    new_prefix = f"{new_dir}{os.sep}"
    try:
        doc = json.loads(registry.read_text())
        vaults = doc.get("vaults", doc)
        if not isinstance(vaults, dict):
            return
        changed = 0
        for entry in vaults.values():
            if not isinstance(entry, dict):
                continue
            cfg = entry.get("config")
            if not isinstance(cfg, dict):
                continue
            path = cfg.get("file")
            if isinstance(path, str) and path.startswith(old_prefix):
                cfg["file"] = new_prefix + path[len(old_prefix):]
                changed += 1
        if changed:
            # `write_text` then `chmod` was the #437 window one more time: the rewritten
            # registry sat at whatever mode the pre-0.52 file had for the whole of the
            # write, and only then became 0600. `_open_private` settles it on the
            # descriptor first, so there is no such window (#505).
            with _open_private(registry, "w") as f:
                f.write(json.dumps(doc, indent=2) + "\n")
            print(f"charter: repointed {changed} vault path(s) to {new_dir}",
                  file=sys.stderr)
    except (OSError, ValueError) as e:
        print(f"charter: state directory moved, but {registry} could not be "
              f"updated ({e}). Vault files are safe in {new_dir}; fix their "
              "'file' paths there, or re-add them with `charter vault add`.",
              file=sys.stderr)


def private_mkdir(p, parents: bool = True) -> None:
    """Create directory *p*, and **every level of it charter has to create**, at 0700.

    The one way charter makes a directory under its own state directory, and it lives
    here — in `config`, which every state writer already imports — because the writers
    that create ``.charter/`` are spread across the registry, the persona and workspace
    pointers, the caches, the session markers and the frame (#470). A helper any of them
    has to reach into `charter.secrets` for is a helper most of them will not call.

    **The umask must not decide the mode of the plane's state directory.** It did:
    ``mkdir(parents=True, exist_ok=True)`` creates at ``0o777 & ~umask``, so on the
    default ``umask 022`` every level came out 0755 and any account on the machine could
    list ``.charter/`` — the vault registry's own directory, and the one holding
    ``fingerprint.key``. Whichever command got there first decided it, so the mode
    depended on the order somebody happened to run things in.

    ``Path.mkdir(parents=True, mode=0o700)`` is **not** this, which is what the first cut
    of #437 assumed: CPython's ``pathlib`` applies *mode* to the leaf only and creates the
    missing parents with a bare recursive ``mkdir``, i.e. at the umask default again. Each
    missing level is therefore created individually here and chmod-ed explicitly — mkdir's
    *mode* argument is itself masked by the umask, so a process under a permissive umask is
    not guaranteed the bits it asked for. The chmod names 0700 outright and so cannot widen
    anything: there is no window in which the directory is looser than it ends up.

    **A directory that already exists is left exactly as it is.** Not an oversight: charter
    tightens what it creates and *reports* what it did not. A ``.charter/`` that predates
    this, or one made by ``mkdir -p`` under ``umask 022``, keeps its 0755 — tightening a
    directory charter did not create is how it would come to chmod a home directory or a
    shared team directory unprompted (#331), and ``$CHARTER_HOME`` can point the state
    directory anywhere on the machine. `charter vault list` and `charter doctor` both name
    a loose one and print the ``chmod`` to run (#471).

    **The leaf is attempted FIRST, and the parents only after it answers "no such file or
    directory".** That is `pathlib`'s own order, and copying it is not cosmetic: a leaf
    that cannot exist for a reason of its own — a name longer than ``NAME_MAX``, which is
    exactly what a frame id built out of a hostile workspace name can be — fails with
    ``ENAMETOOLONG``, and a walk that created the parents on the way down would leave the
    frame root standing where the caller had just checked it was gone. `frame.state` pins
    that as "does not raise **or create**", and it is the shape of a directory being
    resurrected under somebody who had reaped it.

    *parents* is the same argument `Path.mkdir` takes, for the same reason: a caller that
    must not create the levels above the leaf — counting a respawn against a frame
    directory `reap` may already have deleted — passes ``parents=False`` and gets the
    ``FileNotFoundError`` as its answer.

    :func:`charter.secrets.base.make_private_dir` is this function under the name the vault
    writers already call it by; one implementation, so a fix to the walk cannot land on
    only one of the two.
    """
    p = Path(p)
    try:
        _mkdir_0700(p)
        return
    except FileNotFoundError:
        if not parents:
            raise
    missing = []
    cur = p.parent
    while not cur.exists():
        missing.append(cur)
        if cur.parent == cur:
            break
        cur = cur.parent
    for d in [*reversed(missing), p]:
        _mkdir_0700(d)


def _mkdir_0700(d: "Path") -> None:
    """One level, created at 0700 and chmod-ed to it; an existing directory is untouched.

    The chmod is not redundant with ``os.mkdir``'s *mode*: mkdir's argument is masked by
    the umask, so a process under a permissive-in-the-wrong-direction umask is not
    guaranteed the bits it asked for. It names 0700 outright, so it cannot widen anything
    and there is no window in which the directory is looser than it ends up.

    ``FileExistsError`` on a **directory** is the concurrent case — someone else got there
    between the walk and here, so it is now a directory charter did not create and keeps
    its mode. On a non-directory it is re-raised, which is what ``mkdir(exist_ok=True)``
    does and what callers that write into the path afterwards depend on.
    """
    try:
        d.mkdir(mode=0o700)
    except FileExistsError:
        if not d.is_dir():
            raise
        return
    try:
        os.chmod(d, 0o700)
    except OSError:
        pass


def under_state(p) -> bool:
    """Is *p* the state directory, or a path inside it?

    Asked of two spellings of both sides, and answered "yes" if **either** says so: the
    lexical one (``..`` collapsed, no link followed — the only answer available for a path
    that does not exist yet) and the resolved one (``/tmp`` → ``/private/tmp`` — the only
    answer available when the caller resolved first, which on macOS is most of them).
    A disagreement is resolved towards "yes" on purpose. The cost of that error is a
    directory that came out 0700 when 0755 would have done; the cost of the other one is
    the exposure the walk exists to close.
    """
    p, state = Path(p), Path(STATE_DIR)
    for a in {os.path.normpath(os.path.abspath(p)), str(_resolved(p))}:
        for b in {os.path.normpath(os.path.abspath(state)), str(_resolved(state))}:
            if a == b or a.startswith(b.rstrip(os.sep) + os.sep):
                return True
    return False


def _resolved(p: "Path") -> "Path":
    """*p* with links followed, falling back to the lexical form when it cannot be.

    ``(OSError, RuntimeError)`` for the same reason :func:`worktrees_root_for` catches
    both: a symlink loop is `RuntimeError` on some versions and `OSError` on others, and
    this runs on the path of every state write.
    """
    try:
        return p.resolve()
    except (OSError, RuntimeError):
        return Path(os.path.normpath(os.path.abspath(p)))


def mkdir_for(p, parents: bool = True) -> None:
    """Create *p* — **privately when it is charter's own state, ordinarily when it is
    not** — for the writer that is *handed* its directory rather than deriving one.

    `private_mkdir` closes the writers that name a state path themselves, and
    `tests/_statedirscan.py` reads the package to prove none is left. Neither can see a
    path that arrives as a **parameter**: `memstore.write(mem_dir, …)` is handed the
    committed ``personas/<n>/memory`` on one call and the gitignored
    ``PERSONA_STATE_DIR/ephemeral/<session>/<n>`` on the next, and which one it is a
    question only the caller can answer. So the callee asks it at runtime, here (#470).

    That mattered more than "a level below `.charter/` came out loose": on a fresh clone
    ``.charter/`` is gitignored and absent, so ``charter persona remember … --ephemeral``
    is what *creates the state directory itself* — at ``0o777 & ~umask``, i.e. 0755 for
    everyone and 0777 under ``umask 000``. Every later file written straight into
    ``.charter/`` — ``vaults.json``, ``guard-seen.json``, ``fingerprint.key`` — then sat
    in a directory any account on the machine could list.

    **The property is "the umask does not decide the mode of charter's state", not "this
    call site is private".** Routing this one caller would leave the next writer handed a
    state path exactly as exposed; the dispatch is on where the path *is*.

    A path outside the state directory is created with a plain ``mkdir(exist_ok=True)``
    and NOT tightened — committed directories are the operator's to mode, and charter
    tightening one it merely wrote into is the same overreach as chmod-ing a pre-existing
    ``.charter/`` (#331).
    """
    p = Path(p)
    if under_state(p):
        private_mkdir(p, parents=parents)
        return
    p.mkdir(parents=parents, exist_ok=True)


#: The mode a file holding this plane's state is written at: owner read/write, nobody
#: else. A constant rather than a per-writer decision, for the reason `_OTHERS` in
#: `charter.secrets.base` is a mask rather than a list — "which of charter's state files
#: are sensitive" is a question that gets answered wrong once and then stays wrong. It is
#: the mode `docs/secrets.md` already promises for a vault file, applied to the rest of the
#: state directory, and it is the same posture as the 0700 the directories get: the
#: contents of ``.charter/`` are one account's.
STATE_FILE_MODE = 0o600


def _private_fd(p: "Path", *, append: bool) -> int:
    """A descriptor on *p* with `STATE_FILE_MODE` settled on the **inode**, before any
    content of charter's can reach it.

    ``os.open(..., O_CREAT, 0o600)`` is not this, and that is the whole of #437's lesson
    repeated one surface over: the *mode* argument applies **only when the call creates
    the inode**. For a file that already exists — one written by a charter older than
    this, or restored from a tarball, or made by hand — it is ignored entirely, so the
    old contents and every byte written after them sit at whatever mode the file already
    had. The mode is therefore settled with `os.fchmod` on the descriptor this call
    holds, which is also why it is `fchmod` and not `chmod`: nothing swapped at the path
    in between can be affected, and nothing at the path can be affected instead of the
    inode charter is about to write.

    ``O_TRUNC`` is deliberately NOT in the flags. Truncating first would empty the file
    while it is still at its old mode; the caller truncates after the `fchmod`, so there
    is no window in which new content is readable by an account the finished file is not.

    A `fchmod` that fails is swallowed rather than raised. Filesystems with fixed
    permissions (exFAT, many network mounts) accept the state directory and cannot hold a
    mode, and every caller here is a hook, a status line or a marker file — refusing to
    write ``guard-seen.json`` would take the plane down to protect a mode the filesystem
    was never going to keep. `charter.secrets.plain_file` makes the opposite trade for the
    one file where it is right to (plaintext secrets: it reads the mode back and refuses),
    and `charter.secrets.registry._write` makes this one, for these reasons.
    """
    flags = os.O_WRONLY | os.O_CREAT | (os.O_APPEND if append else 0)
    fd = os.open(str(p), flags, STATE_FILE_MODE)
    try:
        os.fchmod(fd, STATE_FILE_MODE)
    except OSError:
        pass
    return fd


@contextlib.contextmanager
def open_for(p, mode: str = "w", *, encoding: str | None = "utf-8"):
    """Open *p* for writing — **privately when it is charter's own state, ordinarily when
    it is not**. The file half of :func:`mkdir_for`, and the same dispatch (#505).

    #470 answered "the umask must not decide the mode of charter's state" for
    directories. The **files** were never asked: every one of them was written at
    ``0o777 & ~umask``, i.e. 0644 on the default ``umask 022`` and 0666 under ``umask
    000``. That was harmless only as long as the directory above them was 0700 — which is
    exactly the case charter deliberately does **not** guarantee. `private_mkdir` leaves a
    directory it did not create exactly as it is (#331: ``$CHARTER_HOME`` can point the
    state directory at a home or a shared team directory, so "chmod whatever we land in"
    is how charter comes to tighten one unprompted), and `vault list` / `doctor` report
    the loose one rather than fixing it. So on a plane whose ``.charter/`` predates charter
    — or was made by ``mkdir -p`` before 0.52.x — every account on the machine could read
    the trace log, the ephemeral persona store and ``guard-seen.json``, and under ``umask
    000`` could *rewrite* the last of those and decide what charter treats as consented.

    **The property is "this file holds plane state", not "charter made the folder".** So
    the dispatch is on where the path is, at runtime, exactly as `mkdir_for`'s is — not on
    whether charter created the directory, and not on a list of the files somebody
    remembered were sensitive.

    **A file that already exists is tightened; a directory that already exists is not.**
    That looks like two answers and is one: charter tightens what is *its own*, and
    reports what is not. A directory under ``$CHARTER_HOME`` may be somebody's home or a
    team share that charter merely landed in, and has a life of its own — so charter names
    it and prints the ``chmod`` (`base.loose_dir_note`, `doctor`). A file charter is
    putting its own bytes into is charter's whatever its history, and leaving the old mode
    on it is the #437 defect verbatim. `secrets.registry._write` and
    `secrets.plain_file._write_private` have settled the mode on the descriptor of a
    pre-existing file since then; this is the same discipline for the rest of the state
    directory, not a second policy.

    A path **outside** the state directory is opened with a plain :func:`open` and NOT
    tightened, which is what makes this a dispatch rather than a privatiser: `memstore`
    writes a committed ``personas/<n>/memory/`` file through the same call, and those are
    the operator's to mode. `mkdir_for` documents the same half.

    *mode* is a `open` mode string; only the writing ones reach here. ``"a"`` appends
    (``O_APPEND``, no truncation), anything else truncates **after** the mode is settled.
    """
    p = Path(p)
    if not under_state(p):
        with open(p, mode, encoding=(None if "b" in mode else encoding)) as f:
            yield f
        return
    with _open_private(p, mode, encoding=encoding) as f:
        yield f


@contextlib.contextmanager
def _open_private(p, mode: str = "w", *, encoding: str | None = "utf-8"):
    """:func:`open_for`'s private branch, unconditionally — for the one caller that knows
    it is writing charter's own state before ``STATE_DIR`` exists to be compared against.

    `_repoint_vault_registry` runs *during* the migration that produces the state
    directory, so `under_state` has nothing to answer with yet. Reaching past the dispatch
    is right there and wrong everywhere else, which is why it is private and says so.
    """
    append = "a" in mode
    fd = _private_fd(Path(p), append=append)
    try:
        if not append:
            os.ftruncate(fd, 0)     # after the fchmod, never before: see `_private_fd`
        f = os.fdopen(fd, mode, encoding=(None if "b" in mode else encoding))
    except BaseException:
        os.close(fd)
        raise
    with f:                         # the file object owns the descriptor now
        yield f


def write_for(p, data) -> None:
    """`Path.write_text` for a path that may be charter's own state — the whole content,
    written into a file charter has already made private if that is where it belongs.

    This is the ``config.private_write`` #505 asks for, under the name that matches
    :func:`mkdir_for`: it is a **dispatch**, not a privatiser, and calling it
    ``private_write`` would invite the next writer to reach for it on a committed path and
    quietly tighten one. ``_for`` is what this package spells "ask where the path is".
    """
    with open_for(p, "wb" if isinstance(data, (bytes, bytearray)) else "w") as f:
        f.write(data)


def touch_for(p) -> None:
    """`Path.touch` for a path that may be charter's own state.

    A marker file carries its meaning in *existing*, not in its bytes — which is exactly
    why it was easy to miss that ``Path.touch()`` creates at ``0o666 & ~umask`` like every
    other writer. An empty file another account can create, delete or re-time is a marker
    that account gets to set: `hooks._ask_mark_set` and `toolgate.snapshot` both decide
    what a later turn is allowed to do from one.
    """
    p = Path(p)
    if not under_state(p):
        p.touch()
        return
    os.close(_private_fd(p, append=True))
    os.utime(p, None)       # `Path.touch` bumps the mtime of an existing file too


def derive(root: Path, start: Path | None = None) -> dict:
    """Every setting that follows from *root*, as ``{NAME: value}``.

    The single definition. :func:`use` applies it; the module bootstraps itself with it
    at import; the test harness calls :func:`use` instead of copying any of it.

    *start* is the directory the plane was located FROM, and matters only for
    ``NESTED_ORIGIN`` — "which plane am I standing in" is a fact about a directory, not
    about the root that directory resolved to. ``None`` means the process cwd, which is the
    truth at import; :func:`use` passes *root* instead so the test harness is not reading
    the developer's actual working directory (it was: the suite ran from a worktree inside
    a nested clone and picked up the real one).
    """
    root = Path(root)
    d: dict = {}

    #: The control plane this invocation operates on — located by a ``charter.toml``
    #: marker, NOT by where this package happens to live. That distinction is the whole
    #: point of the engine/instance split: one installed charter serves many planes.
    d["ROOT"] = root

    #: False when no ``charter.toml`` was found. Commands that need a control plane check
    #: this and fail with a clear message; ``--version`` and ``init`` do not.
    d["HAS_CONTROL_PLANE"] = (root / _root.MARKER).is_file()

    #: The nested plane the caller is STANDING IN, when one encloses it — else ``None``.
    #:
    #: `find_root` hops outward through ``workspaces/`` so the plane holding the vault
    #: wins, which means ``enclosing_plane(ROOT)`` is ``None`` by construction whenever the
    #: hop fired. Without recording the origin, charter would silently act on a plane the
    #: operator cannot see it choose. Three states, and each reads differently:
    #:
    #: * ``None`` — no nesting. Say nothing.
    #: * ``!= ROOT`` — the hop fired: standing in a plane, acting on the one above it.
    #: * ``== ROOT`` — ``$CHARTER_ROOT`` pinned this session INSIDE the nested plane, so
    #:   the hop was overridden and the hazard #140 described is live.
    d["NESTED_ORIGIN"] = _root.standing_in_nested_plane(start)

    #: Parsed once. ``config`` is imported by every command (including ``charter
    #: --version``), so a malformed ``charter.toml`` or a too-new schema must never crash
    #: import — ``instance.load`` keeps raising (its own tests pin that); here the failure
    #: is caught and recorded instead of propagated. ``doctor`` surfaces it to the user.
    try:
        cfg = _instance.load(root)
        d["CONFIG_ERROR"] = None
    except Exception as e:            # malformed TOML, schema too new, unreadable file
        cfg, d["CONFIG_ERROR"] = {}, str(e)

    #: The group/org whose repos this control plane tracks — from charter.toml, not baked in.
    d["GROUP"] = _instance.group_of(cfg, GROUP_FALLBACK)

    #: Repos that must never appear in the inventory (typically the control plane itself).
    d["EXCLUDE"] = _instance.exclude_of(cfg)

    #: The always-present workspace used when none is selected — from charter.toml.
    d["DEFAULT_WORKSPACE"] = _instance.default_workspace_of(cfg, DEFAULT_WORKSPACE_FALLBACK)

    #: How far a written memory travels — see charter.instance.SHARE_MODES.
    d["MEMORY_SHARE"] = _instance.share_of(cfg)

    #: How `charter <harness>` composes its frame. Defaults live in
    #: `instance.FRAME_DEFAULTS`; an absent or malformed section yields them whole.
    d["FRAME"] = _instance.frame_of(cfg)

    #: Which charter this plane tracks — see `charter.instance.UPDATE_CHANNELS` and
    #: `charter.channel`. ``{"channel": "stable"}`` unless the plane opts in, and a value
    #: charter does not recognise degrades to exactly that.
    d["UPDATE"] = _instance.update_of(cfg)

    #: Root for worktrees, or ``None`` for the per-workspace ``.worktrees/`` default.
    d["WORKTREES_ROOT"] = worktrees_root_for(root, cfg)

    #: The SHARED half of the vault registry — committed, beside personas/ and inventory/.
    #: Holds what is identical on every machine: provider, persona, op-vault, and a `file`
    #: relative to the plane. Never a secret, and never the per-developer `account` (that
    #: stays in `.charter/vaults.json`, which keeps overriding this one).
    #:
    #: A separate file rather than a section of charter.toml because `vault add` WRITES it:
    #: charter.toml is hand-maintained, tomllib cannot write TOML, and
    #: `instance.set_locked_version` already has to edit that file as raw text to keep the
    #: comments people put in it. Machine-written config belongs somewhere a machine may
    #: rewrite whole.
    d["SHARED_VAULTS"] = root / "vaults.json"

    #: Per-task workspaces live here: ``workspaces/<workspace>/<repo>`` (on-demand repo
    #: clones) plus the workspace's own ``memory/`` and ``refs/``. Gitignored — a workspace
    #: is a private, per-developer, per-task environment. (Renamed from the old ``repos/``;
    #: ``workspace._ensure_layout`` migrates an existing ``repos/``.)
    d["WORKSPACES_DIR"] = root / "workspaces"

    #: The durable source of truth: every repo in the group + metadata.
    d["INVENTORY"] = root / "inventory" / "repos.json"

    #: Generated documentation.
    d["DOCS_DIR"] = root / "docs"

    #: Per-developer secrets home — vault registry + plain-file vaults. Gitignored, never
    #: committed. Override with ``$CHARTER_HOME`` (e.g. to share across clones).
    state = _migrate_state_dir(root)
    d["STATE_DIR"] = state

    #: Registry of configured vaults (name -> provider + config + persona).
    d["VAULTS_REGISTRY"] = state / "vaults.json"

    #: Default on-disk location for plain-file vaults created without an explicit path.
    #: Note: secrets are intentionally **cross-workspace** (global to the developer), so
    #: vaults live under STATE_DIR, not inside any workspace.
    d["VAULTS_DIR"] = state / "vaults"

    #: Legacy shared active-workspace pointer. No longer read by ``resolve`` (it caused one
    #: task's selection to leak into every other session); kept only so old files don't
    #: error. Selection now lives per-terminal + per-session.
    d["ACTIVE_WORKSPACE_FILE"] = state / "active-workspace"

    #: Per-Claude-session active-workspace pointers, keyed by session id, so parallel
    #: sessions in one clone can each select a different workspace.
    d["SESSIONS_DIR"] = state / "sessions"

    #: Per-terminal active-workspace pointers, keyed by a stable terminal id. Unlike the
    #: Claude session id, a terminal pane survives closing and reopening Claude, so a pane
    #: keeps its own workspace across restarts — without leaking into other panes.
    d["TERMINALS_DIR"] = state / "terminals"

    #: Persona definitions — **committed** and shared with the team (unlike vaults). A
    #: persona is a directory ``personas/<name>/`` holding ``persona.md`` (the definition),
    #: ``memory/`` and ``refs/`` (persistent, committed knowledge). The legacy flat
    #: ``personas/<name>.md`` layout still resolves (see ``persona.py``).
    d["PERSONAS_DIR"] = root / "personas"

    #: Per-developer persona runtime state — **ephemeral** memory (session-scoped scratch,
    #: auto-pruned) and the local activity log. Gitignored (under STATE_DIR), never
    #: committed; the counterpart to the committed ``personas/*/memory``.
    d["PERSONA_STATE_DIR"] = state / "persona-state"

    #: Local pointer to the active persona (set by ``charter persona use``). Overridden by
    #: ``$CHARTER_PERSONA`` and by a command's ``--persona``. Gitignored (in STATE_DIR).
    d["ACTIVE_PERSONA_FILE"] = state / "active-persona"

    #: Drafted upstream **reports** — charter's own bugs and gaps, awaiting the Reporter's
    #: approval before anything is published (see charter/report.py, docs/adr/0003).
    #: Under STATE_DIR because it is per-developer and gitignored: a draft may quote an
    #: exception message that has not been redacted yet, so it must never be committable.
    d["REPORTS_DIR"] = state / "reports"

    return d


#: Every name :func:`derive` produces. The harness snapshots exactly these, so a setting
#: added to `derive` is isolated in tests without anyone remembering to list it anywhere.
DERIVED = tuple(derive(Path(".")).keys())


def use(root) -> dict:
    """Point this module at *root*, returning the values it replaced.

    The seam the test harness needs: `PersonaIso` calls this instead of re-implementing
    the derivation, and restores by handing the snapshot to :func:`restore`.
    """
    previous = {k: globals().get(k) for k in DERIVED}
    # `start=root`, never the process cwd: a test that redirected ROOT into a tmp dir must
    # not have `NESTED_ORIGIN` answered from wherever the suite happens to be running.
    globals().update(derive(root, start=root))
    return previous


def restore(previous: dict) -> None:
    """Put back what :func:`use` returned."""
    globals().update(previous)


# Bootstrap: locate the plane the same way every command does, and derive from it.
#
# The `edm`-era env var names and this warning live in `charter.legacyenv` rather than
# here, and the move is the point: importing THIS module RESOLVES A PLANE, so a caller that
# only wants to know charter's former namespace had to pay for one. `tests/_envguard` is
# that caller and cannot pay — it strips charter's variables from the suite's environment
# *before* charter is first imported — so it did not ask, and those three names were the
# only ones that reached the suite (#540). See `legacyenv`'s docstring.
#
# Warned ONCE, here. There used to be two calls, one beside the definition and this one, so
# every charter invocation with a legacy variable exported printed each banner twice.
_legacyenv.warn()
globals().update(derive(_root.find_root_or_cwd()))
