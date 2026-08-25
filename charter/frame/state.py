"""What one frame knows about itself, on disk.

Under ``<STATE_DIR>/frame/<frame-id>/`` — per frame, never global, because two frames may
run at once (one per session, named by workspace and pid) and a shared version file would
make each frame's panels redraw for the other's activity.

``config.STATE_DIR`` is read as an attribute at call time, everywhere below, and never
imported as a bare name (``from ..config import STATE_DIR``) — the test harness repoints
it with ``config.use()`` after this module has already been imported, and a name bound at
import time would keep pointing at whatever ``STATE_DIR`` was when Python first loaded
this file, which on a developer's machine is the real plane.

**Minting an id is not resolving one.** :func:`frame_id` sanitises, because it is
producing a name from scratch — the same thing a slug generator does. :func:`frame_dir`
does the opposite: it is handed an id from a caller and must not invent a second identity
for a bad one by rewriting it into a good one. A later caller (``notify.plane_changed``)
reads its id out of ``$CHARTER_SESSION_ID`` rather than minting it here, so the id
``frame_dir`` resolves is not always one this module produced — it goes through
:func:`charter.contain.child`, which refuses a hostile name outright (see #328, #348).

**Nothing here raises, and nothing here mutates on a read.** ``bump()`` runs from
charter's hooks, where an exception costs a session its turn; ``version()`` is polled
several times a second by a panel, where a write-on-read would fight ``reap()`` over
whether a dead frame's directory should exist. A missing frame answers with a sentinel
("0", ``None``, ``[]``) rather than an exception or a side effect.

An id can be shaped correctly and still be unusable: ``contain.child`` bounds shape, not
length, so a multi-thousand-character ``$CHARTER_SESSION_ID`` passes it and then hits
``mkdir``'s own limit (``ENAMETOOLONG``) — reachable in practice, since
``session.py``'s id-safety regex strips characters but never bounds length. The read
paths (``version``, ``exit_code``) were already safe here, because a failing read was
already inside a ``try/except OSError``; ``frame_dir``'s ``mkdir`` and the writes in
``bump``/``record_exit`` needed the same guard to make the claim true rather than
aspirational.
"""

from __future__ import annotations

import json
import os
import re
import shutil
import time
from pathlib import Path

from .. import config, contain

#: Anything outside this becomes an underscore. Only used to MINT an id in
#: :func:`frame_id` — never to rewrite one handed to :func:`frame_dir`, which resolves
#: through :func:`charter.contain.child` instead and refuses rather than rewrites.
_UNSAFE = re.compile(r"[^A-Za-z0-9._-]")


def frame_id(workspace: str, pid: int) -> str:
    """A stable id for one frame: the workspace it is for, and the launcher's pid.

    The same pair the tmux session and socket are named for, so a directory on disk and a
    session in `tmux list-sessions` can always be matched up by eye. *workspace* is a name
    read out of ``workspace.json`` or a directory listing rather than typed by an operator
    (#328), so it is sanitised here rather than trusted — this is the one place in the
    module that mints an identity instead of resolving one handed to it.
    """
    safe = _UNSAFE.sub("_", workspace).strip("._-") or "frame"
    return f"{safe}-{int(pid)}"


def _root() -> Path:
    return Path(config.STATE_DIR) / "frame"


def frame_dir(fid: str, *, create: bool = False) -> Path | None:
    """The directory *fid* owns, or ``None`` when *fid* cannot name one there.

    Resolves through :func:`charter.contain.child` rather than sanitising: *fid* may have
    come from ``$CHARTER_SESSION_ID`` rather than from :func:`frame_id`, and rewriting a
    hostile value into a safe-looking one would silently invent a second identity for it
    instead of surfacing the defect (the exact failure ``contain.child`` documents).

    ``create`` defaults to ``False`` so every READ in this module — ``version``,
    ``exit_code`` — never creates the directory it is only trying to look at. A panel
    polling the version of a frame that ``reap()`` already removed must see it stay gone,
    not have its own read resurrect it; the two write paths (``bump``, ``record_exit``)
    pass ``create=True`` because minting state IS their job.

    **The length guard lives here, not in `frame_id`.** `contain.child` bounds *shape*
    (no traversal, no separators) but not *length* — an id thousands of characters long
    still passes it — and `$CHARTER_SESSION_ID` reaches `bump()` without ever going
    through `frame_id`'s minting, so a cap there would guard some callers and not others.
    `mkdir` is the one place every caller's id necessarily passes through, so it is the
    one place a length cap protects all of them: an oversized-but-otherwise-valid id
    degrades to "no directory" (``ENAMETOOLONG``, caught below) exactly like a hostile
    one does above, rather than raising out of a hook that cannot afford it.
    """
    d = contain.child(_root(), fid)
    if d is None:
        return None
    if create:
        try:
            config.private_mkdir(d)
        except OSError:
            # Shaped correctly (no traversal, no separators) but unusable anyway —
            # ENAMETOOLONG is the case this exists for, but any mkdir failure here
            # (permissions, a full filesystem) gets the same treatment: the caller
            # asked for a directory it cannot have, not for an exception.
            return None
    return d


def bump(fid: str) -> None:
    """Record that the frame changed. A caller on a must-not-crash path (a hook).

    Written to a temp file and moved into place with ``os.replace``. A failed write is
    swallowed (see below), not raised — which is exactly why the atomic replace matters
    more here than it would if a failure were still visible: ``os.replace`` only ever
    touches the target file by fully replacing it, never partially, so a write that
    cannot complete leaves the previous version exactly as a reader last saw it rather
    than corrupting it silently. Does nothing for an *fid* :func:`frame_dir` refuses, and
    nothing for a write that fails after the directory exists (a full filesystem, say) —
    this runs from charter's hooks, where raising costs a session its turn, so every
    failure here is a no-op rather than an exception.
    """
    d = frame_dir(fid, create=True)
    if d is None:
        return
    tmp = d / "version.tmp"
    try:
        tmp.write_text(f"{time.time_ns()}\n")
        os.replace(tmp, d / "version")
    except OSError:
        # The directory existing doesn't guarantee the write does too (a filesystem
        # that fills up between the two calls above, say) — same must-not-raise
        # promise as the mkdir guard in frame_dir, covering the step after it.
        return


def version(fid: str) -> str:
    """The frame's version, cheap enough to poll several times a second.

    A probe, not a mutation: a frame with no version file yet — or no directory at all,
    because it was never bumped or was just reaped — answers with the sentinel ``"0"``
    rather than creating one by calling :func:`bump`. Doing that on a read would make
    every panel's poll resurrect a directory :func:`reap` had just deleted, and the two
    would fight forever over whether the frame still exists.

    ``except (OSError, ValueError)``, not just ``OSError``: a version file holding bytes
    that are not valid UTF-8 makes ``read_text()`` raise ``UnicodeDecodeError`` — a
    ``ValueError`` subclass, never caught by ``OSError`` alone — and this module's own
    docstring already promises "nothing here raises... a missing frame answers with the
    sentinel rather than an exception." A panel polls this function several times a
    second and has nothing of its own guarding the call (`panel._tick` reads it
    directly); an uncaught decode error here used to reach the run loop uncaught and
    kill the pane it was drawn in — precisely the hole this whole module exists to
    close, just reached through content corruption rather than a missing file.
    ``exit_code`` below already caught this shape (``int()`` raises ``ValueError`` for
    unparseable text, which happens to catch a decode error too); this brings
    ``version`` in line with it rather than leaving the two read paths inconsistent.
    """
    d = frame_dir(fid)
    if d is None:
        return "0"
    try:
        return (d / "version").read_text().strip() or "0"
    except (OSError, ValueError):
        return "0"


def record_exit(fid: str, code: int) -> None:
    """Record the harness's exit code. Same atomic-write shape as :func:`bump`."""
    d = frame_dir(fid, create=True)
    if d is None:
        return
    tmp = d / "exit.tmp"
    try:
        tmp.write_text(f"{int(code)}\n")
        os.replace(tmp, d / "exit")
    except OSError:
        return


def clear_exit(fid: str) -> None:
    """Forget any exit code recorded under *fid*, because a new frame is claiming the id.

    The bill for #383's rule, and the reason it is only a bill and not a defect. A frame
    id is ``<workspace>-<launcher pid>`` and pids are recycled — Linux wraps at
    ``kernel.pid_max``, 32768 by default — so a launcher for the same workspace really
    does land on a pid an earlier launcher already used. Since :func:`reap` keeps a
    directory for as long as the pid in its name is live, and on a launch that pid is
    live BECAUSE IT IS THE LAUNCHER'S OWN, the earlier frame's directory survives to be
    adopted by the new one, ``exit`` file included. `cmd_launch` then reads that stale
    code back as its own and returns it: a harness running perfectly well, detached, is
    reported as having failed with a dead frame's number.

    A launch beginning is the one moment that can be certain about this — whatever is
    recorded under the id was recorded before this frame existed. Only ``exit`` is
    removed: ``version`` is a counter panels poll, and moving it is :func:`bump`'s job.
    Never raises, and never creates, for the same reasons as everything else here.
    """
    d = frame_dir(fid)
    if d is None:
        return
    try:
        (d / "exit").unlink(missing_ok=True)
    except OSError:
        # Same must-not-raise promise the rest of this module makes: a launch is not
        # worth failing over a file that could not be deleted, and the stale-code
        # reading below is the caller's own to notice.
        return


def record_harness_pane(fid: str, pane: str) -> None:
    """Write down which tmux pane this frame runs its harness in.

    One fact, for one question `$CHARTER_SESSION_ID` alone cannot answer: **is this
    process the harness of that frame, or merely a process that inherited its id?** The
    two are not the same, and reading them as the same is how suppression (ADR 0019)
    blanks the wrong status line:

    * Below ``tmuxctl.SESSION_ENV_FLOOR`` charter cannot put the frame id on
      `new-session`, so a SECOND frame's harness on the shared private server inherits the
      FIRST frame's id (#411). Without this file it would look exactly like frame one's
      own harness and go blank, while its panels followed frame one — leaving that
      operator no correct surface at all, where before they at least had a correct status
      line.
    * An operator who exports ``CHARTER_SESSION_ID`` in a shell rc (a per-shell id of
      their own, say) gets a frame directory minted by the first hook that fires
      (`notify.plane_changed` calls `bump` for any id) and a pid that is permanently live,
      because it is their own shell's. A directory plus a live pid is not proof of a
      frame; a pane a launcher actually started one in is.

    `$TMUX_PANE` is what the other side reads, and tmux sets it in every process it starts
    in a pane — measured 2026-08-24 through a real Claude Code `statusLine` command inside
    a tmux pane, which reported ``PANE=[%0]``, so it survives the harness's own spawning
    of the command. Same atomic-write, never-raise shape as :func:`record_server`, and
    for the same reason: a launch is not worth failing over a bookkeeping file. A frame
    whose pane could not be recorded simply never suppresses, which is the safe direction
    — a duplicated status line rather than a missing one.
    """
    d = frame_dir(fid, create=True)
    if d is None:
        return
    tmp = d / "harness.tmp"
    try:
        tmp.write_text(f"{pane}\n")
        os.replace(tmp, d / "harness")
    except OSError:
        return


def harness_pane(fid: str) -> str | None:
    """The pane *fid* runs its harness in, or ``None`` when charter does not know.

    ``None`` for a frame launched by a charter that predates :func:`record_harness_pane`,
    for a directory that is not a frame's at all, and for a file that cannot be read —
    three different reasons, deliberately one answer, because every caller does the same
    safe thing with it (see :func:`is_live`: no proof, no suppression)."""
    d = frame_dir(fid)
    if d is None:
        return None
    try:
        return (d / "harness").read_text().strip() or None
    except (OSError, ValueError):
        return None


def record_harness_session(fid: str, sid: str) -> bool:
    """Write down the HARNESS's own session id for this frame. ``True`` when the recorded
    value actually changed.

    **The mapping #413 is about, and there is exactly one process that can write it.**
    Claude Code's per-turn token usage is keyed by ITS session id, and a panel never sees
    that id — a panel only ever knows ``$CHARTER_SESSION_ID``, which the frame launcher
    sets to the FRAME's id. The one moment both ids are in the same process is the
    suppressing `statusline.main`: it has the frame id in its environment and Claude
    Code's id in the JSON payload on its stdin. So it writes this, and a panel reads it.

    **In the frame's own directory, and that placement is the answer to "must not leak
    between planes".** `frame_dir` sits under `config.STATE_DIR`, which is per-plane; the
    file goes with the frame and `reap` deletes it with the rest of the directory when
    the launcher's pid dies. Nothing is committed, nothing is shared, and nothing outlives
    the frame it describes.

    Returns whether it CHANGED, because the caller runs on the status line's own render
    path — several times per turn — and uses the answer to decide whether the frame's
    panels have anything new to repaint for. Reading before writing is also what keeps
    this to one `stat`-shaped read on the overwhelmingly common no-op path.

    Same atomic-write, never-raise shape as :func:`record_harness_pane`, and for the same
    reason: a frame whose harness session could not be recorded simply draws no gauge,
    which is the safe direction — no gauge rather than a wrong one.
    """
    sid = (sid or "").strip()
    if not sid or harness_session(fid) == sid:
        return False
    d = frame_dir(fid, create=True)
    if d is None:
        return False
    tmp = d / "session.tmp"
    try:
        tmp.write_text(f"{sid}\n")
        os.replace(tmp, d / "session")
    except OSError:
        return False
    return True


def harness_session(fid: str) -> str | None:
    """The harness's own session id for *fid*, or ``None`` when charter does not know.

    ``None`` for a frame whose harness is not Claude Code (nothing else is handed a
    per-turn usage payload, so nothing else writes here), for a frame launched by a
    charter that predates :func:`record_harness_session`, for a directory that is not a
    frame's, and for a file that cannot be read.

    Four reasons, deliberately one answer, because every caller does the same thing with
    it: **draw no gauge.** `frame/slots.py`'s own rule — a gauge that silently reads zero
    is worse than no gauge — makes "charter does not know" and "charter knows there is
    nothing" the same picture on purpose.
    """
    d = frame_dir(fid)
    if d is None:
        return None
    try:
        return (d / "session").read_text().strip() or None
    except (OSError, ValueError):
        return None


def record_server(fid: str, server: str) -> None:
    """Write down which tmux server this frame's session (or window) lives on.

    Charter runs frames on two servers now: its own private one (``tmux -L charter``,
    where a frame is a SESSION named by frame id) and, when charter is started from
    inside a tmux the operator already has, theirs (``tmux -S <socket>``, where a frame
    is a WINDOW named by frame id). Neither server's liveness list mentions the other's
    frames, so :func:`reap` needs to know which server each directory belongs to before
    it can decide that "not live" means "dead" rather than "not this server's".

    Same must-not-raise, atomic-write shape as :func:`bump` and :func:`record_exit`, and
    for the same reason: this runs on the launch path, where an id ``frame_dir`` refuses
    (or a filesystem that will not take the write) has to degrade to "unknown server"
    rather than take the launch down with it.
    """
    d = frame_dir(fid, create=True)
    if d is None:
        return
    tmp = d / "server.tmp"
    try:
        tmp.write_text(f"{server}\n")
        os.replace(tmp, d / "server")
    except OSError:
        return


def frame_server(fid: str) -> str | None:
    """Which server *fid* was launched on, or ``None`` when charter does not know.

    ``None`` is the migration case and nothing else: every frame this charter starts
    records one (see :func:`record_server`), so a directory without the marker was
    written by a charter that only ever ran frames on its own private server. See
    :func:`reap` for what that means there.
    """
    d = frame_dir(fid)
    if d is None:
        return None
    try:
        return (d / "server").read_text().strip() or None
    except (OSError, ValueError):
        return None


def record_workspace(fid: str, name: str) -> None:
    """Write down which workspace this frame was LAUNCHED for.

    **A panel cannot work this out for itself, and #512 is what that costs.**
    `workspace.resolve`'s rungs are, in order: `--workspace`, `$CHARTER_WORKSPACE`, the
    tree you are standing in, a per-SESSION pointer, a per-TERMINAL pointer, the declared
    default. At LAUNCH — before anything inside the frame has chosen anything — a panel
    process reaches none of the rungs that could speak for the frame:

    * `$CHARTER_WORKSPACE` arrives as the empty string on every launch where the operator
      did not pin one by hand — `commands_frame._frame_identity_env` emits every name in
      `_FRAME_IDENTITY`, present or not, precisely so a frame cannot inherit a *stale*
      pin from whichever launcher started the shared tmux server. Absent is absent.
    * the cwd is the pane's, which is the plane root for anyone who typed `charter claude`
      there — `workspace.from_path` answers `None` for it.
    * the per-session pointer is keyed on `session.current()`, which inside a frame is the
      FRAME id (`$CHARTER_SESSION_ID`), not the harness's own session id; and the
      per-terminal pointer is keyed on `session.terminal()`, which is the panel's OWN tmux
      pane. Both miss.

    So a panel falls all the way to the declared default (`default`, ordinarily) while the
    launcher — an ordinary shell, in the operator's own terminal, one rung up — resolved
    something else entirely. Measured on the plane that reported #512: three terminal
    pointers naming `harness-wrapper` and one naming `user-reporting`, a `default`
    workspace holding no clones at all, and every panel drawing `default`'s empty repo
    list beside a `repos` pane the LAUNCHER had sized for the real workspace's rows.

    The launcher is the one process that knows, so it writes it down — the same argument
    :func:`record_identity` already makes for the rest of a frame's identity, and the same
    atomic-write, never-raise, rewrite-on-every-launch shape as :func:`record_server` (an
    adopted directory's value is another frame's answer, so it is overwritten rather than
    merged).

    This is deliberately NOT `$CHARTER_WORKSPACE`. Exporting the resolved name into the
    frame's environment would fix the same reads and take `charter ws use` away from every
    framed session on the way past — `workspace.resolve` ranks the variable above every
    pointer, `commands_workspace` says so in as many words, and `hooks` skips its
    session-start workspace nudge whenever it is set. A frame recording what it drew is
    not the same thing as a session being pinned.
    """
    d = frame_dir(fid, create=True)
    if d is None:
        return
    tmp = d / "workspace.tmp"
    try:
        tmp.write_text(f"{name}\n")
        os.replace(tmp, d / "workspace")
    except OSError:
        return


def frame_workspace(fid: str) -> str | None:
    """The workspace *fid* was launched for, or ``None`` when charter does not know.

    ``None`` is the migration case (a frame launched by a charter that predates
    :func:`record_workspace` and still running across the upgrade) and the corrupt one,
    answered the same way and for :func:`identity`'s reason: the caller
    (:func:`workspace_for`) decides what to do instead, which is to fall through to the
    rung below — a local resolve, exactly today's behaviour and no worse than it.

    **Name-checked on the way out**, like every other name charter reads off disk and
    joins onto a path (`workspace.declared_default`, `persona.default_persona`). The value
    is charter's own — written by a launcher, under `config.STATE_DIR` — not a committed
    file a teammate can set, so this is a floor rather than the whole guard; but the read
    ends up in `workspace_dir()`'s join and on a panel's screen, and #442 is what an
    unchecked `../../` in that position already cost once.

    `valid_name` alone, with no `val and` in front of it: `workspace.valid_name("")` is
    already False, so the truthiness test would be a second guard that no mutation can
    turn red — the shape this repo keeps shipping ("a guard passing because a DIFFERENT
    guard caught it"). A truncated write is refused by the name check, on the name check's
    own terms.
    """
    d = frame_dir(fid)
    if d is None:
        return None
    try:
        val = (d / "workspace").read_text().strip()
    except (OSError, ValueError):
        return None
    from .. import workspace as ws_mod
    return val if ws_mod.valid_name(val) else None


def workspace_for(fid: str) -> str:
    """The workspace this frame is DRAWING — what every surface of the frame asks.

    Four rungs, and the order is the whole of it:

    0. **The pin.** ``$CHARTER_WORKSPACE`` outranks everything, because that is what it
       means everywhere else in charter: `workspace.resolve` puts it above every pointer,
       `commands_workspace` warns that `ws use` will not stick while it is set, and `hooks`
       tells an operator to "re-launch with CHARTER_WORKSPACE=<name> set" as the way to aim
       a parallel or unattended agent. A frame is not exempt from it. Skip this rung and a
       framed session that also typed `charter workspace use other` draws `other` while
       every command it runs acts on the pinned name — a panel naming a workspace nothing
       in the session touches, wearing `slots`' `*` that says the environment chose it.
    1. **What was chosen inside this frame.** `charter workspace use <name>` typed at the
       agent writes the per-session pointer under the FRAME's id, because inside a frame
       the frame is the charter session (`docs/frame.md`, ADR 0019) — and "it moves the
       panels too" is a documented promise, not an accident. An explicit choice made while
       the frame runs outranks anything the launch decided.
    2. **What the launcher resolved**, :func:`record_workspace`. The seed: the launch's own
       answer to a question nothing inside the frame can ask (#512).
    3. **Whatever this process resolves for itself**, for a frame launched by a charter
       that predates the record and still running across the upgrade — today's behaviour,
       so this is never worse than what it replaces.

    Rungs 1 and 2 are the only two below the pin that can ever disagree, and it is worth
    saying why the others cannot. `$CHARTER_WORKSPACE` reaches a panel exactly when the
    launcher had it (`commands_frame._frame_identity_env` carries it, empty when absent),
    and the launcher resolves it first — so the record holds the same value; rung 0 is
    therefore invisible on an ordinary pinned launch and only shows itself when something
    inside the frame tried to move off the pin. The cwd rung is the same story: a panel's
    cwd is the launcher's, and the launcher asked `from_path` about it before anything
    else. The per-terminal pointer and the declared default are the two rungs a panel
    reaches that answer for the PANEL rather than for the frame, and those are exactly the
    two the record is here to outrank.

    **Name-checked, and `valid_name` alone with no `env and` in front of it** — the same
    rule and the same reasoning as :func:`frame_workspace`, since this value ends up in
    `workspace_dir()`'s join too, and `valid_name("")` is already False so a truthiness
    test would be a second guard no mutation can turn red. A pin that cannot name a
    workspace does not get drawn: it falls through, and `slots` withholds the `*` because
    the name on screen is then not the one the environment named.

    Asked through `workspace.for_session` rather than by reading `workspace.source()`'s
    label: that function returns a sentence written for a status line, and matching the
    string ``"session"`` would be this repo's own recurring defect — a spelling standing in
    for a property. Rung 0 reads the variable itself for the same reason.
    """
    from .. import workspace as ws_mod
    env = os.environ.get("CHARTER_WORKSPACE", "").strip()
    if ws_mod.valid_name(env):
        return env
    return ws_mod.for_session(fid) or frame_workspace(fid) or ws_mod.resolve()


def record_density(fid: str, level: str) -> None:
    """Write down the density THIS RUNNING FRAME is at, overriding `[frame] density`.

    The whole of "a keypress overrides for the running frame only". charter.toml is
    hand-maintained and committed, and charter's rule is that machine-written config
    belongs somewhere a machine may rewrite whole — this directory is exactly that place
    (`reap` deletes it entire when the frame is gone, which is the property a config file
    a human edits can never have). So the menu writes here, the frame's panels read here,
    and the operator's own file is left alone: relaunch and the configured default is back.

    Same must-not-raise, atomic-write shape as :func:`bump` and :func:`record_server`.
    """
    d = frame_dir(fid, create=True)
    if d is None:
        return
    tmp = d / "density.tmp"
    try:
        tmp.write_text(f"{level}\n")
        os.replace(tmp, d / "density")
    except OSError:
        return


def density(fid: str) -> str | None:
    """The density this frame was last set to by hand, or ``None`` for "never set".

    ``None`` is the ordinary case, not a failure: every frame starts at whatever
    `[frame] density` resolved to, and only a menu selection writes a file here. The
    caller falls back to the configured value — see `frame/slots.py`'s `verbosity`.

    The text is NOT validated here. `instance.density_level` is the one gate, and it sits
    at the point of use so that a hand-edited or truncated file degrades to the configured
    level in exactly the same way an unknown level in charter.toml does — one rule, one
    place, rather than a second half-copy of the closed set living in this module.
    """
    d = frame_dir(fid)
    if d is None:
        return None
    try:
        return (d / "density").read_text().strip() or None
    except (OSError, ValueError):
        return None


def clear_shape(fid: str) -> None:
    """Forget the density, the pane map and the harness session recorded under *fid*,
    because a NEW frame is claiming the id.

    The fourth and fifth lines on :func:`clear_exit`'s bill, and the same recycled pid
    underneath them (#383). A frame id is ``<workspace>-<launcher pid>``; :func:`reap`
    keeps a directory while the pid in its name is live, and on a launch it is live
    BECAUSE IT IS THE LAUNCHER'S OWN — so a launcher landing on a pid an earlier launcher
    for the same workspace already used adopts that earlier frame's whole directory.

    Both files inherited that way are actively wrong for the new frame, not merely stale:

    * ``density`` is an override an operator pressed a key for once, in a frame that is
      over. Left behind, a brand-new frame comes up at that level while `[frame] density`
      says otherwise and nothing anywhere explains it — the config silently overridden by
      a keypress from another session, which is the one thing "for the running frame only"
      promises cannot happen.
    * ``panes`` names tmux panes of a frame that no longer exists. `cmd_launch` rewrites
      it as it draws, so this only matters when a launch dies before that — but then the
      map survives pointing at nothing, and the next density change on the next frame to
      claim the id would `kill-pane` ids that mean whatever tmux has since reused.
    * ``session`` names ANOTHER agent session's token usage (#413). This one is the
      sharpest of the three, because the failure is not an empty panel but a confident
      wrong number: the new frame's `top` row would draw the previous session's `ctx 78%`
      as its own, and go on doing it until that session's own harness happened to write a
      new one — which it never will, because it is over. `slots.py`'s rule is that a gauge
      reading zero is worse than no gauge; a gauge reading somebody else's 78% is worse
      than either.

    Never raises, and never creates, like everything else here.
    """
    d = frame_dir(fid)
    if d is None:
        return
    for name in ("density", "panes", "session"):
        try:
            (d / name).unlink(missing_ok=True)
        except OSError:
            continue


def record_identity(fid: str, values: dict[str, str]) -> None:
    """Write down the charter identity this frame was LAUNCHED with.

    **A frame's identity is not readable from the environment of everything that runs
    inside it, and that is the whole reason this file exists (#411, again).** Charter puts
    exactly four variables on a tmux SESSION (`commands_frame._session_id_env_argv` and
    its three siblings), and only one of them is identity: `CHARTER_SESSION_ID`. Every
    other name a frame cares about — `CHARTER_ROOT`, `CHARTER_WORKSPACE`,
    `CHARTER_HARNESS`, `CHARTER_PERSONA` — reaches a `run-shell` child from the SERVER's
    own environment, and charter's private server is SHARED: it belongs to whichever
    launcher happened to start it, possibly days ago, in another plane.

    Measured against tmux 3.7c, two frames on one private socket::

        session one: CHARTER_WORKSPACE=first-ws   CHARTER_HARNESS=claude-code
        session two: CHARTER_WORKSPACE=second-ws  CHARTER_HARNESS=codex
        tmux run-shell -t two 'echo $CHARTER_WORKSPACE $CHARTER_HARNESS $CHARTER_SESSION_ID'
          -> first-ws claude-code two

    So a hotkey pressed on the second frame runs a `run-shell` child holding the SECOND
    frame's id and the FIRST frame's plane. Anything that goes on to state that identity
    to a new pane — which `commands_frame._relayout` does, and which nothing in charter
    did before it — pins another frame's `$CHARTER_ROOT` and `$CHARTER_WORKSPACE` onto
    that pane's argv, where both win outright over every other source
    (`root.find_root`, `workspace.resolve`). The frame's new panels would draw a different
    plane from the ones that survived.

    The launcher is the one process that knows the answer, so it writes it here. Read back
    by :func:`identity`. Same atomic-write, never-raise shape as :func:`record_server` —
    a frame whose identity could not be recorded degrades to "charter does not know",
    which :func:`identity` answers honestly rather than by guessing.
    """
    d = frame_dir(fid, create=True)
    if d is None:
        return
    tmp = d / "identity.tmp"
    try:
        tmp.write_text(json.dumps({k: v for k, v in values.items()
                                   if isinstance(k, str) and isinstance(v, str)}))
        os.replace(tmp, d / "identity")
    except (OSError, TypeError, ValueError):
        return


def identity(fid: str) -> dict[str, str]:
    """The charter identity *fid* was launched with — ``{}`` when charter does not know.

    ``{}`` is the migration case (a frame launched by a charter that predates
    :func:`record_identity`) and the corrupt one, deliberately answered the same way:
    both mean "do not take this frame's identity from here", and the caller
    (`commands_frame._relayout_pane_env`) is what decides what to do instead. Answering
    with a partial guess would be the same defect this file exists to close, one layer up.

    Every value is shape-checked as it is read, like :func:`panes`: this is JSON on disk
    and the values go straight into a tmux ``-e NAME=VALUE`` argv element.
    """
    d = frame_dir(fid)
    if d is None:
        return {}
    try:
        data = json.loads((d / "identity").read_text())
    except (OSError, ValueError):
        return {}
    if not isinstance(data, dict):
        return {}
    return {k: v for k, v in data.items() if isinstance(k, str) and isinstance(v, str)}


def record_panes(fid: str, *, panels: dict[str, str]) -> None:
    """Write down which tmux pane draws which SLOT, so the frame can be re-laid-out later.

    A frame's shape is decided once at launch and then only ever re-asserted
    (`commands_frame._resize_hook_argv`). Changing it while it runs — which is what the
    density menu does — needs to know that `%3` is the `left` panel and not the `right`
    one, because a slot being dropped means killing exactly that pane.

    tmux cannot be asked later: `list-panes` reports ids and geometry but nothing that
    says which pane charter MEANT as `left`, and inferring it from position is the same
    "indices move" trap `frame/layout.py`'s module docstring measures.

    **The harness pane is deliberately NOT in here.** :func:`record_harness_pane` already
    owns that one fact — it is what `is_live` asks to tell a frame's own harness from a
    process that merely inherited its id (ADR 0019) — and it is written on both launch
    paths before any pane is split. A second copy in this file would be two records of
    one fact, written at different moments, free to disagree; `commands_frame.cmd_density`
    reads :func:`harness_pane` for it instead.

    JSON, read back through :func:`panes` — same atomic write as everything else here, and
    the same silence on failure: a frame whose pane map could not be written simply cannot
    be re-laid-out, which is a menu entry doing nothing rather than a launch failing.
    """
    d = frame_dir(fid, create=True)
    if d is None:
        return
    tmp = d / "panes.tmp"
    try:
        tmp.write_text(json.dumps(dict(panels)))
        os.replace(tmp, d / "panes")
    except (OSError, TypeError, ValueError):
        return


def panes(fid: str) -> dict[str, str]:
    """``{slot: pane id}`` — empty when charter cannot tell.

    Every value is shape-checked as it is read: this file is JSON on disk, so a truncated
    write, a hand edit, or a charter that wrote a different shape all reach here, and the
    ids come straight back out of it into a tmux argv. Checking that each is a `str` is
    this function's half; `commands_frame._PANE_ID_RE` — which already guards the same ids
    on the way IN from `split-window`'s stdout — is the half that decides a value really
    looks like tmux's own `%N`, and it stays there rather than being copied here, because
    it is the module that builds the argv that must not be handed a bad one.

    An empty answer is the migration case as well as the corrupt one: a frame launched by
    a charter that predates :func:`record_panes` has no file, and its density menu simply
    cannot re-lay it out.
    """
    d = frame_dir(fid)
    if d is None:
        return {}
    try:
        data = json.loads((d / "panes").read_text())
    except (OSError, ValueError):
        return {}
    if not isinstance(data, dict):
        return {}
    return {s: v for s, v in data.items() if isinstance(s, str) and isinstance(v, str)}


def exit_code(fid: str) -> int | None:
    """The recorded exit code, or ``None`` when the frame has not finished (or exist)."""
    d = frame_dir(fid)
    if d is None:
        return None
    try:
        return int((d / "exit").read_text().strip())
    except (OSError, ValueError):
        return None


#: The subdirectory :func:`respawn_attempt` counts in, named once so
#: :func:`clear_respawn` cannot drift away from it — the two are only correct together.
_RESPAWN_DIR = "respawn"


def respawn_attempt(fid: str, slot: str) -> int | None:
    """Claim the next respawn attempt for *slot* in *fid*, or ``None`` when it cannot.

    Read-increment-write of a single integer, one file per slot. tmux has no way to
    count anything, and a panel pane's `pane-died` hook SURVIVES the respawn it triggers
    (verified against real tmux 3.7c — `show-hooks -p` reads the hook back unchanged
    afterwards), so without a count on disk a panel that dies instantly on every start
    respawns in a hot loop forever. This is the only thing bounding it; see
    `commands_frame._RESPAWN_ATTEMPTS`.

    **Never reset, and not by oversight.** A successful respawn does not zero the count,
    so the budget is three deaths across the whole life of one frame rather than three
    in a row. The alternative needs a definition of "the panel came back up and stayed
    up" that nothing here can observe — tmux only reports that a process was started,
    not that it kept running — and guessing at one is how a panel that dies every
    ninety seconds gets respawned forever while still looking healthy at each individual
    check. What keeps that budget attached to a FRAME rather than to a reused id is
    :func:`clear_respawn`, which a launch calls as it claims the id.

    ``None`` — never ``0``, never a silent restart of the count — for every way this can
    fail to record: an *fid* or *slot* :func:`contain.child` refuses, a directory that
    cannot be made, a write that cannot complete. The caller reads it as "give up",
    which is the safe direction: the failure mode of counting wrong here is an unbounded
    respawn loop, and a dead pane still shows tmux's own `Pane is dead (status N)`.

    **``create=False``, deliberately, even though this writes.** A frame that still
    exists always has its directory (`cmd_launch` creates and `bump`s it before any pane
    is split), so refusing to count for a frame whose state is already gone costs a live
    frame nothing. What `create=True` would buy is the power to REMAKE a directory
    :func:`reap` has removed — one orphan per frame, surviving until some later launch
    reaps it, which is exactly the resurrect-what-reap-just-deleted hazard this module's
    own docstring records for `version()`. Every panel pane dies when its frame is torn
    down, so every panel's `pane-died` hook fires on the way out and lands here; since
    #383 :func:`reap` keeps a directory while the launcher pid in its name is live, so on
    that path the directory is usually still present and a count is spent on a frame that
    is already over. Harmless, and deliberately not special-cased: `cmd_respawn` re-asks
    whether the session exists after its backoff, and the directory goes when the next
    launch reaps it or clears it.
    """
    d = frame_dir(fid)
    if d is None:
        return None
    root = d / _RESPAWN_DIR
    try:
        # `parents=False`: counting an attempt must not recreate a frame directory
        # `reap` has already deleted — the hazard this whole function is careful about.
        config.private_mkdir(root, parents=False)
    except OSError:
        return None
    # One file per slot under a directory of their own, rather than a `respawn-<slot>`
    # sibling of `version`/`exit` in the frame directory itself. The prefix version put
    # the slot in the SECOND path component, where every separator-carrying name lands
    # under a `respawn-<x>` directory that never exists — so the write failed and this
    # function answered `None` whether `contain.child` was here or not, making the
    # guard unobservable and, by the only test that could have pinned it, dead. With
    # the slot as the first component under a directory that DOES exist, a name like
    # `../y` would really be written outside this frame's own directory, and refusing
    # it is a behaviour a test can tell apart from a failed write.
    f = contain.child(root, slot)
    if f is None:
        return None
    try:
        previous = int(f.read_text().strip())
    except (OSError, ValueError):
        # No file yet is the ordinary case (the first death of this frame's own panel);
        # unparseable content is treated identically rather than specially, exactly as
        # `version` and `exit_code` above treat theirs — this is a counter, not a
        # record anything else has to agree with.
        previous = 0
    n = previous + 1
    try:
        f.write_text(f"{n}\n")
    except OSError:
        return None
    return n


def clear_respawn(fid: str) -> None:
    """Forget every respawn count under *fid*, because a NEW frame is claiming the id.

    The third line on :func:`clear_exit`'s bill, and the same recycled pid underneath it
    (#383). A frame id is ``<workspace>-<launcher pid>``; since #383 :func:`reap` keeps a
    directory for as long as the pid in its name is live, and on a launch that pid is
    live BECAUSE IT IS THE LAUNCHER'S OWN — so a launcher landing on a pid an earlier
    launcher for the SAME workspace already used adopts that earlier frame's whole
    directory, its ``respawn/`` counts included.

    Inheriting those is not litter, it is a budget already spent. :func:`respawn_attempt`
    never resets, and every panel's `pane-died` hook fires during the previous frame's
    teardown, so an adopted count is at least one per slot and may already sit at
    `commands_frame._RESPAWN_ATTEMPTS`. The new frame's panels would then die once and
    stay dead without ever being brought back — the precise outcome the count exists to
    ration, handed out for a pid number rather than for anything that happened. Clearing
    as the id is claimed keeps "three deaths" a property of a frame, not of a name.

    Never raises, and never creates: ``rmtree(ignore_errors=True)`` is the same tool
    :func:`reap` uses for the same must-not-fail reason, and the ordinary first launch
    for a workspace has no directory here at all — it must not mint one just to empty it.
    """
    d = frame_dir(fid)
    if d is None:
        return
    shutil.rmtree(d / _RESPAWN_DIR, ignore_errors=True)


def _launcher_pid(name: str) -> int | None:
    """The launcher pid :func:`frame_id` put at the end of *name*, or ``None``.

    The inverse of :func:`frame_id`'s last line and nothing more — a reader, not a
    parser of arbitrary text. ``None`` means "this name carries no pid", which is a real
    answer and not a failure: the frame root can hold debris, a hand-made directory, or a
    name minted by an older charter, and none of those may become undeletable just
    because nothing can be read out of them.

    Requires the separator, so a bare ``12345`` reads as ``None`` rather than as a pid —
    :func:`frame_id` always emits ``<workspace>-<pid>`` with a non-empty workspace (its
    ``or "frame"`` fallback guarantees it), so a name without one did not come from here
    and its digits are not a claim about any process. ``rpartition`` rather than
    ``split``, because a workspace may contain ``-`` itself (``harness-wrapper-4242``).
    """
    head, sep, tail = name.rpartition("-")
    if not sep or not head or not tail.isdigit():
        return None
    pid = int(tail)
    # `0` is "every process in my group" to `kill(2)`, not a process — and `frame_id`
    # can only ever have written a real `os.getpid()` here, which is never 0 or negative.
    return pid if pid > 0 else None


def _launcher_is_alive(pid: int) -> bool:
    """Is the process *pid* names still running? Asks; never signals anything.

    ``os.kill(pid, 0)`` is a QUESTION on POSIX and an ANSWER on Windows, where it maps to
    ``TerminateProcess`` and would kill whatever the name happened to hold (the same trap
    ``news._outer_probe`` documents). Off POSIX the pid is taken at its word — a frame
    directory that outlives its launcher there costs a few hundred bytes, where getting
    it wrong costs somebody else's process. Belt and braces in practice: :func:`reap` is
    only ever called from `commands_frame.cmd_launch`, which has already refused if there
    is no tmux, and there is no tmux off POSIX.

    ``PermissionError`` (EPERM) is an ANSWER too, and the opposite of what it looks like:
    the process exists, it simply is not ours to signal. Reading it as "gone" would make
    every frame launched by another user reapable while it was still running.
    """
    if os.name != "posix":
        return True
    try:
        os.kill(pid, 0)   # signal 0 asks whether it exists; it sends nothing
    except ProcessLookupError:
        return False
    except OverflowError:
        # A number too large for a `pid_t` — `int()` accepted it happily and `os.kill`
        # would raise straight out of a launch. NOT an `OSError`, so it needs naming
        # separately. It cannot name a live process, so the directory stays reapable.
        return False
    except OSError:
        return True       # alive, and not ours to signal
    return True


def is_live(fid: str, *, pane: str | None = None) -> bool:
    """Is *fid* a frame of THIS plane that is running, and is *pane* its harness?

    The question ADR 0019 asks before it draws nothing at all, so every way of being wrong
    here costs somebody a surface. Four things are checked and each one is the only guard
    against a failure that really happens:

    * **A server marker a LAUNCHER wrote** (:func:`frame_server`). This is also the whole
      of "is there a frame directory here at all", and deliberately not a second check:
      the marker cannot exist without the directory, so a separate ``is_dir()`` would be a
      guard no mutation could turn red — the shape this suite has been bitten by before.
      What it rules out, in one read: an id that names nothing here (``$CHARTER_SESSION_ID``
      is not a frame's variable alone — every harness that knows its own session sets it,
      and Claude Code's UUID can end in an all-digit group that parses as a pid); an id
      :func:`frame_dir` refuses outright; and a directory no launcher made — :func:`bump`
      creates one on demand and `notify.plane_changed` calls it from seven hook sites for
      whatever id is in the environment, so an operator who exports ``CHARTER_SESSION_ID``
      in a shell rc gets a directory minted by their first tool call, carrying their own
      shell's permanently-live pid. Only `cmd_launch` records a server. (A frame from a
      charter old enough to predate the marker answers ``None`` too, and gets a duplicated
      status line — which is exactly what it had.) It also keeps the SUITE honest: every
      test that touches plane state repoints ``config.STATE_DIR``, so suppression cannot
      switch itself on because of whatever frame the developer's terminal is sitting in.
    * **A launcher pid still running.** The id ends in it (:func:`frame_id`), so
      ``os.kill(pid, 0)`` answers with a syscall and no tmux subprocess on a path that
      runs every time Claude Code repaints its footer. Without it, a directory left by a
      crashed launcher would blank that plane's status line forever.
    * **The harness pane, when the caller offers one** (:func:`harness_pane`). The
      previous three establish that a live frame exists somewhere; only this one
      establishes that the process asking is *inside* it. `is_live` is called by a
      status line that would otherwise vanish, and a process can hold a frame id it
      merely inherited — see :func:`record_harness_pane` for the two ways that happens
      and what each one costs.

    *pane* is optional because :func:`reap`'s question is the frame's existence, not any
    process's membership of it. ``None`` means "do not ask", not "assume yes".

    Every unknown answers ``False``, which for the status line means "render" — a
    duplicated line is recoverable in a way a line that vanished for an invisible reason
    is not.
    """
    if frame_server(fid) is None:
        return False
    pid = _launcher_pid(fid)
    if pid is None or not _launcher_is_alive(pid):
        return False
    if pane is not None and harness_pane(fid) != pane:
        return False
    return True


def reap(live: set[str], *, server: str) -> list[str]:
    """Remove state for frames of *server* that are gone. Returns what was removed.

    Never by age: a frame open for two days is exactly a working frame, and an age
    heuristic would delete precisely that one. *live* names what that server still
    reports — sessions on charter's own private one, windows on an operator's — so the
    only frames removed are ones nothing is watching any more.

    **Scoped to one server, because "not live" is only an answer the frame's OWN server
    can give.** A frame launched inside the operator's tmux is a window on their socket
    and appears in no `tmux -L charter list-sessions` output at all; reaping on that
    list alone deletes a running frame's version file (its panels stop noticing the
    agent) and its recorded exit code (its launcher reads back `None` and reports the
    wrong status) while the frame is still on screen. *server* is matched against
    :func:`frame_server`, which the launcher records when it creates the directory.

    A directory with NO recorded server matches every one, and that is the migration
    case rather than a loophole: only a charter that predates :func:`record_server`
    leaves one, every such frame was on the private server, and refusing to reap them
    would trade one release's transient wrongness for a permanent leak.

    **And never on a live session alone either, because a frame outlives its session
    (#383).** Between a harness exiting and its launcher reading :func:`exit_code`, the
    session is already gone from `tmux list-sessions` while the launcher is still
    sitting in `cmd_launch` with the answer one line away. A `reap()` from a SIBLING
    frame's launch — and one runs at every launch — landing in that window deleted the
    `exit` file before it was ever read: `exit_code` answered ``None``, `cmd_launch`
    turned that into a returned ``0``, and a harness that had genuinely failed was
    reported as a success to whatever `&&` chain or CI step invoked charter. Ordering
    `cmd_launch`'s reap before its own `frame_dir(create=True)` narrows that window; it
    cannot close it, because by then the sibling's session is genuinely absent from
    *live*. Server scoping does not close it either — a sibling on the SAME server is
    exactly the case that bites — so the two guards are independent and both are asked.

    So a second question is asked, and :func:`frame_id` is what makes it answerable
    without inventing any new state: the id ENDS in the launcher's own pid. A directory
    whose launcher is still a live process is one somebody may still come back to, and
    is left alone — no age, no timestamps, no extra file, nothing to keep in sync.

    Both ways of being wrong point the same way, deliberately. A pid the OS has recycled
    onto an unrelated process reads as "alive" and costs one directory's cleanup, deferred
    until that process ends and some later launch reaps it — bounded, silent, and measured
    in bytes. Reading a live launcher as dead costs a real exit code, which is the defect
    above. A launcher also no longer reaps its OWN directory on the way out (its pid is,
    necessarily, still alive); the next launch takes it, which is what already happens for
    every frame that ended while nothing else was starting.

    Recycling has one sharper form that is NOT paid for in bytes, and it is paid for in
    :func:`clear_exit` rather than here: a pid recycled onto a launcher for the SAME
    workspace mints the same id, so the kept directory is not merely litter, it is the id
    the new frame is about to adopt — stale ``exit`` file included. Deciding that here
    would mean guessing which of two frames a directory belongs to; the launcher knows,
    because it is the one claiming the id, so it clears the file as it starts.
    """
    root = _root()
    if not root.is_dir():
        return []
    removed = []
    for d in sorted(root.iterdir()):
        if not d.is_dir() or d.name in live:
            continue
        # Two independent reasons to keep a directory, and BOTH must be absent before
        # anything is deleted. They answer different questions and neither implies the
        # other: the frame may belong to the OTHER tmux server (#381), or its launcher
        # may still be running on this one (#383).
        owner = frame_server(d.name)
        if owner is not None and owner != server:
            continue
        pid = _launcher_pid(d.name)
        if pid is not None and _launcher_is_alive(pid):
            continue
        shutil.rmtree(d, ignore_errors=True)
        removed.append(d.name)
    return removed
