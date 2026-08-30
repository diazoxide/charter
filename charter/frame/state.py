"""What one frame knows about itself, on disk.

Under ``<STATE_DIR>/frame/<frame-id>/`` — per frame, never global, because several frames
may run at once (a chat per tmux WINDOW, several windows to a workspace's session) and a
shared version file would make each frame's panels redraw for the other's activity.

**Two id shapes live here, and which one an id is decides how its liveness is read.** A
chat's id is ``{workspace}.{n}``, allocated by :func:`new_chat_id`, and its liveness comes
from the tmux window it is drawn in. A frame launched by a charter old enough to predate
that is ``{workspace}-{launcher pid}`` (:func:`frame_id`) and keeps the pid rule.
:func:`_launcher_pid` is the discriminator — it parses the second and not the first — so
nothing was migrated and no frame carries a version field.

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
#:
#: **The `.` is not in it, and that is #695 rather than tidiness.** A workspace may be
#: called `api.2` — `instance.WORKSPACE_NAME_RE` accepts a dot and this does not narrow
#: what an operator may call anything — but the name this function MINTS goes on to be a
#: tmux SESSION (`commands_frame.cmd_launch`), and a session name is not a string tmux
#: reads as a string. Measured:
#:
#: * tmux 3.7c keeps the dot and then splits on it in every `-t`: `new-window -t api.2`
#:   answers ``can't specify pane here`` rc 1, so a dotted workspace could never open its
#:   SECOND chat; `set-environment -t api.2` answers rc **0** and writes on session
#:   `api`, which is another workspace's frame told it is this one.
#: * tmux 3.2 — `tmuxctl.FLOOR` — does not even keep it: ``new-session -s api.2`` creates
#:   a session actually named ``api_2``. So charter asked for one name and got another,
#:   and every later `-t` missed the session it had just made.
#:
#: A trailing `:` disambiguates the target on 3.7c and does nothing on 3.2, so the fix
#: cannot live at the target. It lives here, where charter chooses the identifier: the
#: workspace keeps its own name, and the thing derived from it is spelled in an alphabet
#: tmux reads back the way it was written — which is what 3.2 was going to do anyway.
_UNSAFE = re.compile(r"[^A-Za-z0-9_-]")


def workspace_prefix(workspace: str) -> str:
    """*workspace*, reduced to the alphabet an id may be spelled in.

    Shared by :func:`frame_id` and :func:`new_chat_id` so the two cannot drift: a name
    read out of ``workspace.json`` or a directory listing rather than typed by an
    operator (#328) is sanitised rather than trusted, and both minting paths have to
    sanitise it identically or a chat's id would stop starting with the same characters
    the frame ids beside it do.

    ``or "frame"`` is what makes the result non-empty, which :func:`_launcher_pid`
    depends on: a name with no head before its separator is not a pid claim.

    ``strip``, both ends, and the trailing end is the one that matters here: a workspace
    called ``api.`` would otherwise mint the chat id ``api..1``, whose directory
    :func:`frame_dir` still resolves but which no operator can say out loud and which
    reads as an ordinal on a workspace called ``api.``. It is also what makes the result
    of this function unable to be ``.`` or ``..`` — the two names
    :func:`charter.contain.segment_ok` refuses — so a name built from it is a path
    segment by construction.
    """
    return _UNSAFE.sub("_", workspace).strip("._-") or "frame"


def frame_id(workspace: str, pid: int) -> str:
    """The OLD shape of a frame id: the workspace it is for, and the launcher's pid.

    The same pair the tmux session and socket were named for, so a directory on disk and
    a session in `tmux list-sessions` could always be matched up by eye.

    **Nothing mints one of these any more.** A frame is a chat now
    (:func:`new_chat_id`), and its liveness comes from the tmux window it is drawn in
    rather than from a pid in its name. This stays because the shape is still READ:
    :func:`_launcher_pid` is its exact inverse, and a frame launched by an older charter
    — running across the upgrade, or left on disk by one that was — keeps being reported
    live and reaped by the pid rule with no migration. It is the spelling those frames
    are in, so it is spelled once, here.
    """
    return f"{workspace_prefix(workspace)}-{int(pid)}"


#: The separator between a chat's workspace and its ordinal, and it is a `.` rather than
#: a `-` for one measured reason: `_launcher_pid` reads a `-<digits>` tail as a launcher
#: pid, so `myws-2` answers `2` — and pid 2 is alive on every Unix, so every dead chat
#: would look live forever and `reap` would stop bounding `.charter/frame/` at all.
#: Measured on this tree: ``_launcher_pid("myws-2") -> 2``, ``_launcher_pid("myws.2") ->
#: None``. That ``None`` is not a gap, it is the **version discriminator** — an id this
#: module can parse a pid out of is an old `frame_id` frame and keeps the pid rule; one
#: it cannot is a chat and takes liveness from tmux's own window list.
_CHAT_SEP = "."

#: How far :func:`new_chat_id` will count before it gives up on a workspace.
#:
#: Not a policy about how many chats an operator may have — `[frame] max_chats` is that,
#: and it is a different question asked somewhere else. This bounds the LOOP: allocation
#: walks upwards from 1 claiming directories, and without a ceiling a plane whose frame
#: root somehow refuses every name would spin instead of answering. Ten thousand is far
#: past any real plane (the scan costs one `mkdir` per taken ordinal, and a plane holds
#: tens of frame directories, not thousands) and small enough that giving up is a
#: refusal a caller can report rather than a hang nobody can see.
_CHAT_ORDINAL_MAX = 10_000

#: The file :func:`new_chat_id` writes its own pid into as it claims a directory, and the
#: third keep-rule :func:`reap` reads it back through (#685).
#:
#: A plain name beside `server` and `workspace`, not a dotfile: everything in a frame's
#: directory is charter's own bookkeeping and nothing here hides from `ls`. It is read by
#: `reap` alone — no bar, no panel and no palette asks who claimed a chat — which is why
#: the reader below is private and there is no `record_launcher` in this module's public
#: surface for a second caller to reach for.
_CLAIM_FILE = "launcher"


def _root() -> Path:
    return Path(config.STATE_DIR) / "frame"


def new_chat_id(workspace: str) -> str | None:
    """Allocate the next chat id for *workspace* — ``{workspace}.{n}`` — or ``None``.

    **Allocated, not computed, and the ``mkdir`` IS the allocation.** The alternative the
    operator's own sketch reached for was ``{workspace}-{some-hash}``, and both halves of
    it fail. A hash of the only inputs available at creation is a hash of a counter
    wearing a disguise — (workspace, harness) is not unique by construction, two Claude
    chats in one workspace hash the same — and a truncated hash COLLIDES SILENTLY into a
    shared ``.charter/frame/<fid>/``, where one chat's ``session``, ``panes`` and
    ``version`` overwrite the other's and nothing reports it. A `-{ordinal}` tail fails
    differently and worse: see :data:`_CHAT_SEP` for the measurement.

    A counter cannot collide because it is claimed rather than computed.
    :func:`config.claim_private_dir` is `config.private_mkdir` with its idempotence
    removed **on purpose** — `private_mkdir` swallows ``FileExistsError`` on a directory
    (#331), which is exactly right for "make sure this exists" and exactly wrong for "is
    this name mine?". Here ``FileExistsError`` is the claim failing, and the answer to it
    is ``n+1``. Two allocators racing the same workspace cannot both win, because
    ``mkdir`` is one syscall and the kernel picks.

    **Winning the `mkdir` is only half of it, and the other half is :func:`_record_claim`**
    (#685). A claim has to survive until it is a chat, and for hundreds of milliseconds
    after this returns it is a directory with no window, no `server` marker and — unlike
    the `{workspace}-{pid}` ids this replaced — no pid in its name, which is to say a
    directory every one of :func:`reap`'s rules said was dead. A sibling launcher runs a
    reap on the way in to its own claim, so the loser of the race deleted the winner's
    directory and then claimed the same ordinal: the silent collision the paragraph above
    says the design makes impossible, arriving one layer down. The pid goes in the
    directory as its first byte, and `reap` reads it.

    **A scan is not a substitute.** Reading the directory and taking ``max + 1`` gives
    two racers the same answer, and the loser silently adopts the winner's frame
    directory — the collision this whole design exists to make impossible, reintroduced
    by the cheaper-looking spelling.

    ``None`` for every way this can fail to allocate: a frame root that cannot be made, a
    name :func:`contain.child` refuses, an id so long ``mkdir`` answers ``ENAMETOOLONG``,
    a filesystem that will not take the directory, or a workspace already holding
    :data:`_CHAT_ORDINAL_MAX` chats. One answer for all of them, because the caller does
    the same thing with each: report that it could not open a chat, rather than launch
    one whose state has nowhere to live.

    The id is a NAME and is never parsed for meaning. Renaming a workspace leaves its
    live chats spelling the old one and changes nothing — `frame_workspace` reads the
    workspace out of the frame's own ``workspace`` file, which can be repointed, and the
    bars show that rather than the prefix of the id. The one visible cost is cosmetic and
    deliberately not fixed: after a rename, chat 1 of the renamed workspace may be
    ``newname.1`` beside a sibling still called ``oldname.2``. Rewriting ids to tidy that
    would break every ``$CHARTER_SESSION_ID`` already exported into a live process.
    """
    prefix = workspace_prefix(workspace)
    root = _root()
    try:
        config.private_mkdir(root)
    except OSError:
        return None
    for n in range(1, _CHAT_ORDINAL_MAX + 1):
        # A plain join, and the containment is asserted rather than branched on — the
        # deletion sweep is why. `contain.child` here could only ever refuse a name
        # `workspace_prefix` cannot produce: the alphabet holds no separator and no NUL,
        # the head is non-empty (`or "frame"`), the strip rules out `.` and `..`, and the
        # tail is a decimal integer. So `if d is None: return None` was a branch no test
        # could reach and no mutation could redden — the shape `record_identity`'s own
        # unreachable `isinstance` filter was deleted for. What still refuses a bad name
        # is `frame_dir`, which every later reader of this id goes through, and which
        # resolves through `contain.child` on the way back.
        d = root / f"{prefix}{_CHAT_SEP}{n}"
        try:
            config.claim_private_dir(d)
        except FileExistsError:
            continue          # taken — by a sibling chat, by a racer, or by debris
        except OSError:
            # ENAMETOOLONG for an id no `mkdir` will take, a full filesystem, a
            # permission the plane does not have. None of those gets better at `n+1`.
            return None
        # And the claim is SAID, immediately, in the directory the `mkdir` just made
        # (#685). Winning the `mkdir` is only half of "cannot both win": the other half
        # is that the winner's directory survives long enough to become a chat, and
        # between this line and the `new-window` hundreds of milliseconds later the
        # claim passed all three of `reap`'s keep-rules — no window yet, no `server`
        # marker yet, and a chat id carries no launcher pid for the third to abstain in
        # favour of. A sibling launcher's reap (every launch runs one) deleted it, and
        # both launchers then held `api.1`: one `exit` file, one `panes` map, one pane
        # record, and a switch aimed at whichever wrote last. Reproduced.
        _record_claim(d)
        return d.name
    return None


def _record_claim(d: Path) -> None:
    """Write this process's pid into the directory :func:`new_chat_id` has just claimed.

    **What the old `{workspace}-{pid}` id shape said in its NAME, said in a file.** That
    shape got `reap`'s launcher rule for free — the pid was in the name, so the `mkdir`
    that made the directory also made it un-reapable — and Stage 5a spent it deliberately:
    `_launcher_pid`'s `-`-vs-`.` answer is the version discriminator, and widening it to
    read `api.1` as pid 1 would hand every chat ``launchd`` and keep every dead one
    forever. Nothing replaced the guard it removed. This does, in the one place that can:
    the allocator, on the path that has already made the directory.

    **The first byte written into a claimed directory, and that ordering is the guard
    rather than a tidiness.** `reap` reads this file to decide, so between the `mkdir` and
    this write there is a directory it can see and no pid it can read — which is why
    :func:`reap` keeps an EMPTY frame directory outright. Those two rules meet exactly:
    the directory is empty until this lands, and marked from the moment it does. Any later
    writer (`record_server`, `record_workspace`, `bump`) would leave a window in between
    where the directory holds something and says nothing, which is the window this closes.

    Never raises, like every other writer here: a claim that could not be marked is a
    directory a sibling's reap may delete out from under this launch — the defect above —
    but a launch is not worth failing over a file, and the empty-directory rule still
    covers the instant this was called at. Written with `config.write_for` at 0700's
    dispatch for `record_server`'s reason: this is charter's own state.
    """
    try:
        config.write_for(d / _CLAIM_FILE, f"{os.getpid()}\n")
    except OSError:
        return


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
    than corrupting it silently.

    **The temp file goes through `config.write_for`, and that is what settles the mode of
    the destination too.** ``os.replace`` carries the SOURCE's mode onto the target, so
    while this was `Path.write_text` it wrote a 0644 temp file and then moved 0644 onto
    ``version`` — with nothing in the directory ever looking wrong. Ten writers in this
    module share that shape (#582). ``os.replace`` itself needs no dispatch: a rename
    cannot cross filesystems, so an atomic write into ``.charter/`` is written beside its
    destination and is a state path by construction.

    Does nothing for an *fid* :func:`frame_dir` refuses, and
    nothing for a write that fails after the directory exists (a full filesystem, say) —
    this runs from charter's hooks, where raising costs a session its turn, so every
    failure here is a no-op rather than an exception.
    """
    d = frame_dir(fid, create=True)
    if d is None:
        return
    tmp = d / "version.tmp"
    try:
        config.write_for(tmp, f"{time.time_ns()}\n")
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


#: How long a SUCCESSFUL outcome stays on the attention row. The same four seconds
#: `_say_on_screen` used to ask `display-message -d 4000` for, and for the same stated
#: reason ("long enough to read a sentence") — but now it is four seconds of a row, not
#: four seconds of a frozen client, so the number costs what it was always assumed to
#: cost. A success is also the case that needs the row least: the frame ITSELF is the
#: confirmation, and the top row already reads `⬢ gamma` by the time this is drawn.
NOTICE_SECONDS = 4.0

#: How long a REFUSAL stays there. Longer, because a refusal is the one outcome with no
#: other surface — nothing moved, so no panel repaints into the answer — and because it
#: carries the fix in its own text (`no workspace 'x' — have: a, b, c`), which is more to
#: read than `workspace → gamma`. Bounded rather than sticky: the row's next-highest
#: field is an `_alerts()` entry, an actionable problem the operator also needs, and a
#: refusal that never expired would sit on top of one indefinitely.
REFUSAL_SECONDS = 10.0


def say(fid: str, message: str, *, seconds: float = NOTICE_SECONDS) -> None:
    """Put one line on this frame's attention row for *seconds*, then let it expire.

    **The frame's own surface, not the tmux client's message line** (#729). What this
    replaces was `display-message -d 4000`, and it was replaced for two measured reasons
    rather than one:

    * A tmux client does not redraw its PANES while a message is up. Measured on tmux
      3.7c and at the 3.2 floor alike, with an outer terminal mirroring an inner session:
      the pane's own content changed at 0.02s and the operator's screen did not catch up
      until 4.03s. The freeze is exactly the `-d` value — `-d 200` freezes for 0.20s,
      `-d 750` for 0.74s — so the cost was the duration, spent entirely on hiding the
      repaint the message was announcing.
    * `display-message -t <pane>` does not choose the SCREEN. `-t` is the target for
      FORMAT evaluation; the client is `-c`, and with no `-c` tmux picks its own current
      client. Measured on both versions, two sessions on one server with a terminal on
      each: a message aimed at a pane of session `sa` was drawn on the terminal attached
      to `sb` and not on `sa`'s at all. On a socket with eleven frames on it — an
      ordinary control plane — a refusal about one frame was being drawn across another
      operator's, which is why this is per-frame state read by that frame's own panel and
      not a message aimed at a server.

    Best effort, never raises: the callers are switch outcomes, and one that cannot write
    its notice must still return the exit status it owes tmux. Same atomic
    `write_for`-then-`os.replace` shape as :func:`bump`, so a reader never sees half a
    line, and a failed write leaves the previous notice exactly as it was.

    The expiry is stored, not the duration, so a reader needs only the clock and never
    has to know when the write happened. `time.time()` rather than `time.monotonic()`:
    the writer and the reader are different processes, and a monotonic clock is only
    comparable within one.

    *message* goes through `contain.one_line` for the same reason every other caller of
    it does, and for one more that is specific to this file: the notice is stored as an
    expiry line followed by the text, so a newline in *message* would write a value that
    reads back truncated at it. Callers in `switch.py` and `frame/actions.py` already
    contain their own interpolated names; this contains the assembled line, which is a
    different claim — the file format's, not the message's.
    """
    d = frame_dir(fid, create=True)
    if d is None:
        return
    # Stripped BEFORE the emptiness check, not after. `contain.one_line` turns a
    # character with no glyph into a visible escape, so a message that is only
    # whitespace stays truthy and would write a notice that draws nothing — and then
    # hold the row's top priority over an `_alerts()` entry, and keep `panel._watch`
    # repainting for the whole dwell, to say it. `notice()` strips on the way out too;
    # this is what stops the file being written in the first place.
    text = contain.one_line(message).strip()
    if not text:
        return
    tmp = d / "notice.tmp"
    try:
        config.write_for(tmp, f"{time.time() + float(seconds)}\n{text}\n")
        os.replace(tmp, d / "notice")
    except (OSError, ValueError, OverflowError):
        return


def notice(fid: str) -> str:
    """This frame's unexpired notice, or ``""``. A pure read, several times a second.

    Answers ``""`` for every degenerate case — no frame, no file, an unparseable expiry,
    bytes that are not UTF-8, an expiry already passed — because the caller is
    `slots._bottom`, drawing the one row `docs/frame.md` promises is never dropped. A row
    that raised would take that promise down with it.

    **The file is not deleted on expiry, and that is deliberate.** This is a read, and
    this module's own contract is that reads do not mutate: a panel polling five times a
    second must not be the thing that unlinks state, or two panels would race over it and
    a reap would fight a poll. An expired notice is a few dozen stale bytes in the
    frame's own directory, which `reap()` removes with everything else.
    """
    d = frame_dir(fid)
    if d is None:
        return ""
    try:
        raw = (d / "notice").read_text()
    except (OSError, ValueError):
        return ""
    expiry, _, text = raw.partition("\n")
    try:
        # Spelled as "the clock is still BEFORE the expiry" rather than as its negation,
        # because the two are not the same for one value a file can hold. `float("nan")`
        # parses, and every comparison against a NaN is False — so `now >= expiry` reads
        # False and the notice becomes PERMANENT, which is the one outcome this whole
        # cluster is about (#727). Asking for the live case makes NaN answer "not live",
        # which is the direction every other degenerate case here already falls.
        live = time.time() < float(expiry)
    except ValueError:
        return ""
    return text.strip() if live else ""


def notice_expiry(fid: str) -> float:
    """When this frame's notice stops being drawn, as a `time.time()` value — ``0.0``
    when there is nothing pending.

    Split from :func:`notice` for `panel._watch`, which needs the DEADLINE rather than
    the text: the panel has to repaint once when a notice expires, and the only thing
    that changes at that instant is the clock. Nothing bumps the version, nothing
    resizes, and no event arrives — the identical falling edge #727 records for the
    in-flight spinner, which is why one loop change answers both.

    Answers ``0.0``, not ``None``, so a caller can compare it against `time.time()`
    without a branch: an absent notice is one whose deadline has already passed.
    """
    d = frame_dir(fid)
    if d is None:
        return 0.0
    try:
        raw = (d / "notice").read_text()
    except (OSError, ValueError):
        return 0.0
    try:
        return float(raw.partition("\n")[0])
    except ValueError:
        return 0.0


def record_exit(fid: str, code: int) -> None:
    """Record the harness's exit code. Same atomic-write shape as :func:`bump`."""
    d = frame_dir(fid, create=True)
    if d is None:
        return
    tmp = d / "exit.tmp"
    try:
        config.write_for(tmp, f"{int(code)}\n")
        os.replace(tmp, d / "exit")
    except OSError:
        return


def clear_exit(fid: str) -> None:
    """Forget any exit code recorded under *fid*, because a new frame is claiming the id.

    The bill for #383's rule, and the reason it is only a bill and not a defect. A frame
    id WAS ``<workspace>-<launcher pid>`` and pids are recycled — Linux wraps at
    ``kernel.pid_max``, 32768 by default — so a launcher for the same workspace really
    does land on a pid an earlier launcher already used. Since :func:`reap` keeps a
    directory for as long as the pid in its name is live, and on a launch that pid is
    live BECAUSE IT IS THE LAUNCHER'S OWN, the earlier frame's directory survives to be
    adopted by the new one, ``exit`` file included. `cmd_launch` then reads that stale
    code back as its own and returns it: a harness running perfectly well, detached, is
    reported as having failed with a dead frame's number.

    A launch beginning is the one moment that can be certain about this — whatever is
    recorded under the id was recorded before this frame existed.

    **A CHAT's id cannot be adopted that way at all**, because :func:`new_chat_id` claims
    its ordinal with a ``mkdir`` that fails when the name is taken, so a launch never
    lands on an occupied directory. On the launch path this is now belt and braces; the
    case it is still for is reopening a COLD chat, which relaunches into that chat's own
    existing directory. Only ``exit`` is
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


def clear_claim(fid: str) -> None:
    """Give up the claim :func:`new_chat_id` made on *fid*'s directory (#685).

    **The marker is held for the LAUNCH, not for the life of the process**, and this is
    the half that makes those two different. `_record_claim` says "a launcher is still
    working on this directory"; a launcher that has read its harness's exit code and is
    about to return is not, and its own last `reap` — the one whose whole job is to remove
    the frame it just finished — would otherwise be refused by a marker naming a process
    that is still, necessarily, alive.

    Called at the END of a launch and nowhere else, which is why the window this protects
    stays protected: the #383 hazard is a SIBLING's reap landing between a harness dying
    and its own launcher reading `exit`, and that launcher has not reached here yet.

    Nothing is lost when it is never called — a launcher killed outright leaves the marker
    behind and its pid dies with it, so the next reap removes the directory on the pid rule
    exactly as it does for an old `{workspace}-{pid}` frame. Never raises and never
    creates, like :func:`clear_exit` beside it.
    """
    d = frame_dir(fid)
    if d is None:
        return
    try:
        (d / _CLAIM_FILE).unlink(missing_ok=True)
    except OSError:
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
        config.write_for(tmp, f"{pane}\n")
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
        config.write_for(tmp, f"{sid}\n")
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
        config.write_for(tmp, f"{server}\n")
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
        config.write_for(tmp, f"{name}\n")
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
    a human edits can never have). So the palette writes here, the frame's panels read here,
    and the operator's own file is left alone: relaunch and the configured default is back.

    Same must-not-raise, atomic-write shape as :func:`bump` and :func:`record_server`.
    """
    d = frame_dir(fid, create=True)
    if d is None:
        return
    tmp = d / "density.tmp"
    try:
        config.write_for(tmp, f"{level}\n")
        os.replace(tmp, d / "density")
    except OSError:
        return


def density(fid: str) -> str | None:
    """The density this frame was last set to by hand, or ``None`` for "never set".

    ``None`` is the ordinary case, not a failure: every frame starts at whatever
    `[frame] density` resolved to, and only a palette row writes a file here. The
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


def record_chrome(fid: str, level: str) -> None:
    """Write down the pane surface THIS RUNNING FRAME is on, overriding `[frame] chrome`.

    :func:`record_density`'s twin, for :func:`density`'s reason and no other: charter.toml
    is hand-maintained and committed, this directory is machine-written and `reap` deletes
    it whole when the frame ends. A palette row that edited an operator's own file to
    change what one running frame looks like would be a keypress with a commit in it.

    **The surface is the one element a keypress can change without moving a pane**, which
    is why this exists at all rather than riding on `record_density`. `window-style` is a
    pane option and tmux repaints from it on the spot — measured on an attached client,
    both tmux 3.7c and tmux 3.2: ``set-option -p window-style bg=black`` on a pane that
    was already drawn put ``\x1b[40m`` on the client's wire with no `refresh-client` and
    no re-layout. So `commands_frame.cmd_chrome` records here and sets an option, and
    nothing splits.

    Same must-not-raise, atomic-write shape as :func:`record_density`.
    """
    d = frame_dir(fid, create=True)
    if d is None:
        return
    tmp = d / "chrome.tmp"
    try:
        config.write_for(tmp, f"{level}\n")
        os.replace(tmp, d / "chrome")
    except OSError:
        return


def chrome(fid: str) -> str | None:
    """The surface this frame was last set to by hand, or ``None`` for "never set".

    ``None`` is the ordinary case: a frame starts at whatever `[frame] chrome` resolved to
    — `off` unless the plane's own file says otherwise — and only a palette row writes
    here. The caller falls back to the configured value; `commands_frame._current_chrome`
    is the one place that does.

    **The text is NOT validated here**, for :func:`density`'s reason exactly:
    `instance.chrome_level` is the one gate on that closed set and it sits at the point of
    use, so a truncated or hand-edited file degrades to the configured value in the same
    way an unknown word in charter.toml does. A second half-copy of the enum in this
    module is how the two come to disagree — and here the stakes are higher than a
    density's, because the value on the other side of that gate is on its way to a tmux
    style: `instance.chrome_options` maps a WORD to constants charter holds, so a word
    nobody recognises yields no tmux command at all.
    """
    d = frame_dir(fid)
    if d is None:
        return None
    try:
        return (d / "chrome").read_text().strip() or None
    except (OSError, ValueError):
        return None


def record_change(fid: str, slug: str) -> None:
    """Write down which cross-repo change THIS RUNNING FRAME is looking at.

    :func:`record_density`'s twin, for its reason exactly: this is a keypress's answer for
    one running frame, and `reap` deletes the directory whole when the frame ends. There
    is no committed setting behind it and there deliberately is not one — which change you
    are in the middle of is a fact about a session, not an arrangement somebody would
    commit, and `[frame]` gaining a key for it would be a config value that is stale by
    lunchtime.

    Same must-not-raise, atomic-write shape as :func:`record_density`.
    """
    d = frame_dir(fid, create=True)
    if d is None:
        return
    tmp = d / "change.tmp"
    try:
        config.write_for(tmp, f"{slug}\n")
        os.replace(tmp, d / "change")
    except OSError:
        return


def frame_change(fid: str) -> str | None:
    """The change this frame is looking at, or ``None`` for "none chosen".

    ``None`` is the ordinary case and is not a failure: a frame comes up looking at no
    change in particular, and the `changes` panel then lists them all — which is the
    honest answer to *"what am I in the middle of"* when nobody has said.

    **The text is NOT validated here**, for :func:`density`'s reason: `instance` owns the
    one rule for what a change may be called (`change_name_ok`, asked by
    `change.path_for`), and it sits at the point of use so a hand-edited or truncated file
    degrades exactly the way a slug charter cannot resolve does. A second half-copy of
    that rule in this module is how the two come to disagree, and this value is on its way
    to a `changes/<slug>.json` join.
    """
    d = frame_dir(fid)
    if d is None:
        return None
    try:
        return (d / "change").read_text().strip() or None
    except (OSError, ValueError):
        return None


def record_selection(fid: str, name: str) -> None:
    """Write down which repo row THIS RUNNING FRAME has selected.

    :func:`record_change`'s twin, and it is here for the reason that module cannot be:
    **the pane that takes the click and the pane that shows the detail are two
    processes.** A click lands in the `repos` panel, which owns that rectangle and nothing
    else; the attention row is `bottom`'s pane, a separate `charter panel` with its own
    tty and its own loop. There is no shared memory between them and deliberately none —
    `panel.py`'s "liveness is a poll, not a push" is the whole shape of this frame — so a
    selection one panel makes and another draws is a file, and the repaint that follows is
    the `state.bump` every other cross-panel fact already travels on.

    No committed setting behind it, exactly as :func:`record_change` has none, and for the
    same reason said one noun over: which row you last pointed at is a fact about the
    minute you are in, and a `[frame]` key for it would be config that is stale before the
    frame is.

    **The value is a repo NAME and it is never drawn as itself.** `slots._table_lines`
    compares it against the ``name`` of each row it is already drawing and highlights the
    one that matches; a name matching nothing highlights nothing. That is what makes a
    hand-edited or truncated file cost a highlight rather than a line — the same
    degrade-to-nothing :func:`density` gets by leaving validation at the point of use, with
    the sharper edge that here there is no point of use to validate at: the only thing
    charter does with this string is an equality test against names it read out of its own
    gather. What `slots._selected_detail` puts on the attention row is composed from the
    matched GATHER ROW, not from this file.

    Same must-not-raise, atomic-write shape as :func:`record_density`.
    """
    d = frame_dir(fid, create=True)
    if d is None:
        return
    tmp = d / "selection.tmp"
    try:
        config.write_for(tmp, f"{name}\n")
        os.replace(tmp, d / "selection")
    except OSError:
        return


def selection(fid: str) -> str | None:
    """The repo row this frame has selected, or ``None`` for "nothing selected".

    ``None`` is the ordinary case and is not a failure: a frame comes up with no row
    selected, the table draws no highlight and the attention row spends its columns on the
    fields it always had. That is the state every plane with `[frame] mouse` off stays in
    unless the operator moves the selection from the palette, and it is the state
    `frame/component.py` asks a pointer affordance to degrade to.

    Read on every `bottom` repaint, which is the one place this costs anything, so it is
    one `read_text` of a file that usually is not there. See `slots._bottom` for the bill.
    """
    d = frame_dir(fid)
    if d is None:
        return None
    try:
        return (d / "selection").read_text().strip() or None
    except (OSError, ValueError):
        return None


def record_hidden(fid: str, names) -> None:
    """Write down which components THIS RUNNING FRAME is not drawing.

    The mechanism a toggle key and a density level are both expressed in
    (`commands_frame.cmd_toggle`, `commands_frame.cmd_density`): visibility is per
    component, and a level is a name for one set of it.

    Same place and same argument as :func:`record_density` — charter.toml is
    hand-maintained and committed, this directory is machine-written and `reap` deletes it
    whole when the frame ends, so a keypress lands here and the operator's own file is
    left alone. Relaunch and the arrangement they configured is back.

    One name per line, newline-terminated, so that "nothing is hidden" is an EMPTY FILE
    and "nothing has been recorded" is NO FILE. Those are different answers — the first is
    an operator who toggled the last panel back on, the second is a frame nobody has
    touched, whose hidden set is whatever ``visible = false`` its config declared — and a
    caller cannot tell them apart from any value that flattens both to "empty", which is
    why this is a file per frame rather than a line in one. Same must-not-raise,
    atomic-write shape as :func:`record_density`.
    """
    d = frame_dir(fid, create=True)
    if d is None:
        return
    tmp = d / "hidden.tmp"
    try:
        config.write_for(tmp, "".join(f"{n}\n" for n in names))
        os.replace(tmp, d / "hidden")
    except OSError:
        return


def hidden(fid: str) -> tuple[str, ...] | None:
    """The components this frame has been told not to draw, or ``None`` for "never told".

    ``None`` is the ordinary case and is not a failure: a frame comes up drawing the
    arrangement its config describes, and only a keypress writes here. The caller falls
    back to the ``visible = false`` names in that config — see
    `commands_frame._hidden_now`.

    **The text is NOT validated here, and unlike :func:`density` that is not a deferral —
    there is nothing for a validator to protect.** A name in this set is only ever asked
    ``is this name in it?`` about a name that is already in the frame's own arrangement
    (`instance.frame_arrangement`), so a line that is not one of those names removes
    nothing, reaches no tmux command and is not carried anywhere. A guard here would be a
    guard with no consequence, which this repo's own sweep exists to find and delete.

    Blank lines are dropped, which is what makes an empty recorded set read back as an
    empty tuple rather than as one component named ``""``.
    """
    d = frame_dir(fid)
    if d is None:
        return None
    try:
        text = (d / "hidden").read_text()
    except (OSError, ValueError):
        return None
    return tuple(line for line in text.split("\n") if line)


def clear_shape(fid: str) -> None:
    """Forget the shape, the pane map and the harness session recorded under *fid*,
    because a NEW frame is claiming the id.

    "The shape" is two files: the ``density`` a keypress chose and the ``hidden`` set a
    component's own toggle key wrote (:func:`record_hidden`). Both are one operator's
    decision about one frame, and that frame is over.

    The fourth and fifth lines on :func:`clear_exit`'s bill, and the same recycled pid
    underneath them (#383) — and the same narrowing: an ALLOCATED chat id cannot be a
    previous frame's, so what this is for now is a cold chat being reopened into its own
    directory. A frame id WAS ``<workspace>-<launcher pid>``; :func:`reap`
    keeps a directory while the pid in its name is live, and on a launch it is live
    BECAUSE IT IS THE LAUNCHER'S OWN — so a launcher landing on a pid an earlier launcher
    for the same workspace already used adopts that earlier frame's whole directory.

    Both files inherited that way are actively wrong for the new frame, not merely stale:

    * ``density`` is an override an operator pressed a key for once, in a frame that is
      over. Left behind, a brand-new frame comes up at that level while `[frame] density`
      says otherwise and nothing anywhere explains it — the config silently overridden by
      a keypress from another session, which is the one thing "for the running frame only"
      promises cannot happen.
    * ``hidden`` is the same keypress said one component at a time (:func:`record_hidden`)
      and inherits for the same reason, one level sharper: a level at least names a frame
      charter ships, while a stale hidden set can leave a brand-new frame missing exactly
      the panel a previous operator dismissed, with `[frame] slots` naming it and nothing
      on screen to say why. The file's whole contract is that "never recorded" and
      "recorded empty" are different answers, so a file inherited from a dead frame is
      read as a live decision this frame's operator never made.
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

    * ``selection`` is the same keypress or the same click said about a ROW
      (:func:`record_selection`), and it inherits with the mildest of these consequences
      and the same shape: a brand-new frame comes up with one repo highlighted and its
      detail on the attention row, because somebody pointed at it in a session that is
      over. Nothing on screen explains it and nothing the new operator does explains it
      either — the highlight is a claim about an action they did not take.

    Never raises, and never creates, like everything else here.
    """
    d = frame_dir(fid)
    if d is None:
        return
    for name in ("density", "hidden", "panes", "session", "selection"):
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

    **The ``isinstance(k, str) and isinstance(v, str)`` filter that used to sit inside the
    `json.dumps` is gone**, reported by the deletion sweep and unreachable: the only thing
    that reaches *values* is `commands_frame._frame_identity_env`, which is
    ``{name: env.get(name, "") for name in _FRAME_IDENTITY}`` — every key a string from a
    module constant, every value a string or ``""``. A filter no caller can exercise is
    not a defence; it is a line that makes the next reader believe there is one, and
    `test_the_frames_identity_can_only_be_strings` pins the contract that makes it
    unreachable. What a non-string would do if one ever arrived is unchanged in kind and
    stated rather than filtered: `json.dumps` raises `TypeError`, the clause below catches
    it, and the frame degrades to "charter does not know" — this function's declared
    posture for every other failure it can have.
    """
    d = frame_dir(fid, create=True)
    if d is None:
        return
    tmp = d / "identity.tmp"
    try:
        config.write_for(tmp, json.dumps(dict(values)))
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
    density change does — needs to know that `%3` is the `left` panel and not the `right`
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
    be re-laid-out, which is a palette row doing nothing rather than a launch failing.
    """
    d = frame_dir(fid, create=True)
    if d is None:
        return
    tmp = d / "panes.tmp"
    try:
        config.write_for(tmp, json.dumps(dict(panels)))
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
    a charter that predates :func:`record_panes` has no file, and its density rows simply
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
        config.write_for(f, f"{n}\n")
    except OSError:
        return None
    return n


def clear_respawn(fid: str) -> None:
    """Forget every respawn count under *fid*, because a NEW frame is claiming the id.

    The third line on :func:`clear_exit`'s bill, and the same recycled pid underneath it
    (#383), narrowed the same way an allocated id narrows the other two. A frame id WAS
    ``<workspace>-<launcher pid>``; since #383 :func:`reap` keeps a
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

    **The separator is also the version discriminator, and that is a load-bearing use of
    this function rather than a happy accident.** ``-`` is `frame_id`'s and ``.`` is
    `new_chat_id`'s, so ``myws-2`` answers ``2`` and ``myws.2`` answers ``None`` — which
    is what lets `is_live` and `reap` apply the pid rule to an old frame and the window
    rule to a chat with no flag, no migration and no extra field on disk. Widening this
    to accept ``.`` would hand every chat a launcher pid that is really its ordinal:
    ``api.1`` would read as pid 1, ``launchd``/``init``, alive on every Unix, and `reap`
    would keep every dead chat forever.
    """
    head, sep, tail = name.rpartition("-")
    if not sep or not head or not tail.isdigit():
        return None
    pid = int(tail)
    # `0` is "every process in my group" to `kill(2)`, not a process — and `frame_id`
    # can only ever have written a real `os.getpid()` here, which is never 0 or negative.
    return pid if pid > 0 else None


def _claiming_pid(d: Path) -> int | None:
    """The pid :func:`_record_claim` wrote into *d*, or ``None``.

    :func:`_launcher_pid`'s counterpart, asked of the FILE rather than of the name, and
    the same answer means the same thing in both: "this directory carries no claim about
    any process". ``None`` is the ordinary answer for every frame directory an older
    charter left behind and for every old-shape `{workspace}-{pid}` frame, both of which
    go on being decided by the rules that already decided them.

    A `Path`, not a frame id, because its one caller is :func:`reap` — which is walking
    `os.scandir`'s own entries and has the directory in hand. Resolving a name back into a
    path here would be `frame_dir`'s containment asked a second time about a value that
    never left this module.

    Held to the same shape :func:`_launcher_pid` holds its own to: digits, and greater
    than zero, because ``0`` is "every process in my group" to ``kill(2)`` rather than a
    process. Anything else on disk — a truncated write, a hand-edited file, a directory
    where the file should be — is no claim at all, which is the safe direction: a chat
    whose marker cannot be read is reaped like one that never had one, exactly as it is
    today.
    """
    try:
        val = (d / _CLAIM_FILE).read_text().strip()
    except (OSError, ValueError):
        return None
    if not val.isdigit():
        return None
    pid = int(val)
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

    **A CHAT has no launcher pid, and the pane record is what answers instead.** An id
    :func:`_launcher_pid` cannot parse is a `new_chat_id` chat (see :data:`_CHAT_SEP`),
    and there is no process in its name to signal. The honest evidence available here —
    with no tmux subprocess, on a path Claude Code re-runs every time it repaints its
    footer — is :func:`record_harness_pane`'s: a process running in the pane a LAUNCHER
    wrote down is a process inside a window that still exists, which is what "this chat
    is live" means. So for a chat the pane is not an extra check on top of liveness, it
    IS the liveness, and a caller that offers none gets ``False`` rather than a claim
    nothing here can support. Both callers (`statusline`, `doctor`) already pass
    ``$TMUX_PANE``.

    Every unknown answers ``False``, which for the status line means "render" — a
    duplicated line is recoverable in a way a line that vanished for an invisible reason
    is not.
    """
    if frame_server(fid) is None:
        return False
    pane_matches = pane is not None and harness_pane(fid) == pane
    pid = _launcher_pid(fid)
    if pid is None:
        return pane_matches
    if not _launcher_is_alive(pid):
        return False
    if pane is not None and not pane_matches:
        return False
    return True


def reap(live: set[str], *, server: str) -> list[str]:
    """Remove state for frames of *server* that are gone. Returns what was removed.

    Never by age: a frame open for two days is exactly a working frame, and an age
    heuristic would delete precisely that one. *live* names what that server still
    reports — on charter's own private server that is now its sessions AND the chat ids
    its windows carry, on an operator's it is their windows — so the only frames removed
    are ones nothing is watching any more.

    **A chat is bounded by this list alone, and that is the whole of why the id has a dot
    in it.** A `new_chat_id` id carries no launcher pid, so the second rule below
    (:func:`_launcher_pid`) abstains for it and *live* decides on its own. Had the
    ordinal been spelled `-{n}`, `myws-1` would read as pid 1 — ``launchd``/``init``,
    alive on every Unix — and every dead chat would be kept forever: this function is the
    only thing bounding ``.charter/frame/``, so that is not litter, it is an unbounded
    directory. `commands_frame._live_chats` is what puts the chat ids in *live*, read
    from the ``@charter_chat`` WINDOW OPTION rather than from ``#{window_name}``, because
    a window name is not an identity (measured: with ``allow-rename on`` a pane's own
    output renamed a `-n`-named window to ``PWNED`` on tmux 3.7c and on 3.2, while the
    option was untouched — and a chat whose name has been taken from it is a chat this
    function would delete the state of while it was still running).

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

    **A CHAT id carries no pid, so that question is asked of a file instead** (#685,
    :func:`_record_claim`). Stage 5a's ids are `{workspace}.{n}`, and the paragraph above
    is the whole reason: reading `api.1` as pid 1 would hand every chat ``launchd`` and
    keep every dead one forever. What Stage 5a did not do was replace the guard that
    removed. Without it a directory `new_chat_id` had just claimed passed every rule here
    — no window yet (`@charter_chat` is set hundreds of milliseconds later, after
    `_spawn_gather`, the tmux.conf write and `new-window`), no `server` marker yet
    (`record_server` runs after the claim), and no pid in the name by design — so a
    SIBLING launcher's reap deleted it and both launchers went on holding `api.1`. Two
    live chats, one `exit` file, one `panes` map, one pane record. Reproduced.

    The claim marker restores the old shape's guard exactly, including its two failure
    directions and the price of each: a recycled pid keeps one directory until that
    process ends, and a launcher misread as dead costs a real exit code. It also restores
    for chats the #383 protection the paragraph above describes — a chat whose harness has
    exited is now kept until its launcher has read the code, where before Stage 5b it was
    reapable the instant its window went away.

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
        # Four independent reasons to keep a directory, and ALL must be absent before
        # anything is deleted. They answer different questions and none implies another:
        # the frame may belong to the OTHER tmux server (#381), its launcher may still be
        # running on this one (#383), or the directory may be a claim that is not a frame
        # yet (#685) — either because its marker has not landed (it holds nothing at all)
        # or because it has (a live pid is in it).
        owner = frame_server(d.name)
        if owner is not None and owner != server:
            continue
        try:
            claimed = next(d.iterdir(), None) is None
        except OSError:
            # A directory this process cannot list is one it has no reading of, and
            # `rmtree` would not have emptied it either. Kept, on the same side every
            # unreadable answer in this module falls: no proof, no deletion.
            continue
        if claimed:
            # **Nothing in it yet, so its claimer is between two syscalls.**
            # `new_chat_id` creates the directory and `_record_claim` writes the pid as
            # the very first byte into it, so an EMPTY frame directory is a claim caught
            # in the microseconds between the two — the only window the marker itself
            # cannot cover, and the reason the marker has to be the first write rather
            # than merely an early one. Every frame that has ever run holds `server`,
            # `workspace` and `version` at minimum, so nothing this keeps is a frame.
            #
            # It costs an ordinal rather than bytes, and that is the honest price: a
            # directory left empty by an `rmtree` that half-failed is now kept, and
            # `new_chat_id` skips its name for good. One name out of
            # `_CHAT_ORDINAL_MAX`, against a collision that hands two live chats one
            # `exit` file.
            continue
        pid = _claiming_pid(d)
        if pid is None:
            # The claim's own record first and the NAME second, never both at once: a
            # chat's launcher is in the file (#685) and an old `{workspace}-{pid}` frame's
            # is in its id, and no directory carries the two. `is None` rather than
            # falsiness, and that is the difference between a guard and an accident: pid
            # `0` is a value this reads off disk, `kill(2)` takes it as "every process in
            # my group" and would answer alive, and a `0` that fell through to the name
            # would be a corrupt marker being saved by a rule that is about something
            # else. `_claiming_pid` refuses it on its own terms.
            pid = _launcher_pid(d.name)
        if pid is not None and _launcher_is_alive(pid):
            continue
        shutil.rmtree(d, ignore_errors=True)
        removed.append(d.name)
    return removed
