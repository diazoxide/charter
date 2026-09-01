"""Which chats a workspace holds, which one you are in, and which may be switched to.

**The reading half of Phase 5's switch. Nothing here touches tmux and nothing here
starts anything** — `frame/switch.py`'s own rule, kept for its own reason: this is what
a panel process asks on every repaint and what the palette asks when it opens, and a
module that shelled out to tmux to answer would be a round trip on both paths.
`commands_frame.cmd_chat` is the tmux half.

**The directory IS the list** (spec §3.5). There is no per-workspace index of chats,
because an index is a second source of truth for a fact `.charter/frame/` already holds
and is free to disagree with it — the #411 shape. So :func:`of_workspace` scans, and
what it scans is bounded by `state.reap`, which is the only thing bounding that
directory at all.

**Two things tell a chat from an old frame, and only one of them is a name.** A chat's
id is `{workspace}.{n}` and an old frame's is `{workspace}-{pid}`, so
`state._launcher_pid` answers ``None`` for the first and an integer for the second —
that ``None`` is Stage 5a's version discriminator and it is asked here rather than
re-derived, because a second reading of "is this a chat" is a second answer. The other
is `state.frame_workspace`, which is a FILE and can be repointed: a renamed workspace's
chats keep ids spelling the old name and still belong to the new one (spec §3.2), so
membership is never read off the id's prefix.

**A directory name is not an identity either.** Every id here comes off `os.scandir`,
which means it is whatever is on disk — and it goes on to a palette row, to a
`charter frame-chat <id>` argv and, one hop later, to a tmux target. :data:`ID_RE` is
the alphabet a frame id travels to tmux under (`commands_frame._FRAME_ID_RE`, the same
expression asked at the same boundary), and it is applied HERE, where the name enters
charter's own vocabulary, rather than at each of the three places it leaves it. The
state directory is 0700 and `SECURITY.md:43-46` is honest about what that is worth; this
is a guard against a mistake, and the mistake it is against is a name reaching a row or
an argv unexamined.

**What is drawn is contained by whatever draws it, and never here.** `overlay.Surface`
runs `contain.one_line` over every title and note before `tui.width` sees them (#472),
and the bars run it before their own width arithmetic. A second containment on the way
out of this module would be a line no test could go red without — `frame/choose.py`
records the same finding about `builtin_actions._register_names`, whose masked
`contain.one_line` stayed green over its own deletion.

**There is no `label` file, and its absence is a decision rather than an omission.**
Spec §3.5 asks for one per chat, an open-alphabet display name. Nothing in this stage
writes one: renaming a chat is not a task here, and a reader whose writer does not exist
is a line no test can turn red — the shape this repository deletes rather than
documents (`state.new_chat_id`'s own unreachable `None` went the same way in Stage 5a).
What a chat is CALLED today is its id and the harness recorded beside it, and both of
those are real values off disk that the containment tests are measured against. The file
arrives with the stage that writes one.
"""

from __future__ import annotations

import os
import re
from typing import NamedTuple

from .. import contain
from . import state, tmuxctl
from .switch import Outcome, _some

#: The alphabet a chat id may be spelled in — `commands_frame._FRAME_ID_RE`'s, and the
#: same expression on purpose. That is the alphabet `state.workspace_prefix` mints in and
#: the one a frame id has to survive to reach a tmux argv, so asking it here means the
#: three exits from this module (a palette row, a `frame-chat` argv, a `select-window`
#: target) are asking one question once rather than three times differently.
#:
#: `fullmatch`, never `search`: a directory called `api.1;kill-server` contains a name
#: this pattern matches and is not one.
ID_RE = re.compile(r"[A-Za-z0-9._-]+")


class Chat(NamedTuple):
    """One chat of a workspace, as every surface that draws one reads it.

    Carried as a record rather than assembled per surface, so the bar, the picker and
    the refusal cannot come to disagree about what a chat is called or which one is
    active — the same reason `choose.Roster` carries its rows and its names together.
    """

    #: The chat id, held to :data:`ID_RE`. Charter's own currency: it is the frame
    #: directory's name, `$CHARTER_SESSION_ID` inside the pane, and `@charter_chat` on
    #: the window.
    id: str
    #: The harness recorded for it (`CHARTER_HARNESS`, `harness.base.name`), or ``""``
    #: for a chat whose identity file charter could not read. Display text with an open
    #: alphabet — it is a harness's own display name — so it is contained where it is
    #: drawn and never here.
    harness: str
    #: Whether this is the chat asking.
    active: bool


def is_chat(fid: str) -> bool:
    """Whether *fid* names a chat rather than an old `{workspace}-{pid}` frame.

    `state._launcher_pid` and nothing else, for the module docstring's reason: Stage 5a
    made that ``None`` the version discriminator, and a second rule for the same question
    (say, "does it contain a dot") would answer differently for a workspace called
    `v1.2`. The id is still held to :data:`ID_RE` here, because this is where a name off
    `os.scandir` enters charter's vocabulary.
    """
    return bool(ID_RE.fullmatch(fid)) and state._launcher_pid(fid) is None


def of_workspace(workspace: str) -> list[str]:
    """Every chat that says it is in *workspace*, in ordinal order.

    **Membership is `state.own_workspace`, which is `state.workspace_for`'s middle** — and
    the difference between those two functions is the whole of #733. `workspace_for` is
    what a frame DRAWS, and its outer rungs answer for the process ASKING:
    ``$CHARTER_WORKSPACE`` above and a local `workspace.resolve()` below. Asked here they
    would be catastrophic rather than merely wrong: rung 0 returns the same value for every
    id on the plane, so one pinned palette process would sweep every chat on the machine
    into its own pin and out of its own workspace; rung 3 would hand every record-less chat
    to whatever the asker had chosen. **So membership cannot come from `workspace_for`, and
    the fix for a chat missing from a roster is never to call it here.**

    What was wrong was the DEPTH, not the direction. This read `state.frame_workspace`
    alone — the launch record — while `roster` keyed on every rung above it, so a chat
    could be DRAWING `alpha` and excluded from `alpha`'s roster at the same time, and the
    chats it had left could never see it again however it was repaired.
    `state.own_workspace` is the chat-owned rungs — the recorded pin, then the record — in
    the ladder's own order, so this and `roster` now ask one question at one depth.

    **The per-session pointer is NOT one of those rungs, and #791 is why.** It was, for
    one release. `charter workspace use <name>` typed at the agent writes it under the
    CHAT's id (inside a frame `session.current()` is the chat — ADR 0019), so while it was
    a rung that command re-homed the chat: measured, `charter workspace use gamma` inside
    `alpha.1` took it out of `alpha`'s roster and put `gamma`'s chats on its bar, where
    `check` approved rows `cmd_chat` then refuses. That is #733 and #788 through a door
    neither issue named, and §4j settles it — a chat's workspace is its identity, and a
    pointer written by whoever last typed a command is a property. Both rungs left are
    written by the LAUNCH, so nothing after a launch moves a chat between workspaces.

    **Ordinal order, and it is not `created`'s.** Spec §3.5 asks for a `created` stamp
    per chat "so tab order and the ordinal allocator do not depend on directory mtime".
    Neither does: the allocator counts upwards from 1 claiming directories
    (`state.new_chat_id`) and never reads a timestamp, and sorting by the ordinal gives
    allocation order for every chat that has not outlived a reap. Where the two differ —
    an ordinal freed by `state.reap` and handed out again — ordinal order is the one a
    tab bar wants: `api.1` stays leftmost, where an operator learned to look for it,
    instead of jumping to the end because it happens to be the newest. So there is no
    `created` file and nothing to keep in step with the directory.

    A name that is not an ordinal sorts by its text, after every name that is. That is
    the migration and corruption case rather than a shape charter mints, and it is
    ordered rather than dropped for `frame_workspace`'s reason: a chat charter cannot
    read the ordinal of is still a chat, and leaving it out of the list would leave it
    out of the picker while its window is on screen.

    The scan is `os.scandir` over ~30 entries and it is asked when a palette opens and
    when a bar repaints, not on a tick — `frame/panel.py` polls `state.version`, which is
    one `stat`, and only renders when it moved. `state.reap` is what bounds the entries.

    **What the ladder costs, measured rather than assumed**: one entry reads two small
    files where it read one — the identity record and the workspace record — so 30 chats is
    60 reads per call instead of 30 (it was 90 while the per-session pointer was a rung,
    and #791 gave that one back). That is paid on a repaint and never on a tick, and it is
    the price of the two halves of one question being asked at one depth. Deliberately NOT
    cached: a cache here is a second source of truth for a fact `.charter/frame/` already
    holds and free to disagree with it, which is the #411 shape this module's own opening
    paragraph refuses.
    """
    try:
        # **`is_dir()` — kept, and honestly no longer measurable from out here.** A
        # deletion sweep once removed it on the grounds that it could not change the
        # answer: a frame's workspace was a FILE inside its directory, so
        # `frame_workspace` already answered ``None`` for a loose file whatever the filter
        # did, and a guard that passes only because a different guard caught it is the
        # shape this repository deletes. #733 brought it back with a measurement, because
        # membership had grown a rung that did NOT live in the frame's directory — the
        # per-session pointer under `SESSIONS_DIR/<id>.workspace`, which knows nothing
        # about whether the frame root holds a directory or a stray byte of a half-written
        # temp file. With that rung planted, a loose FILE called `api.2` joined the roster
        # and then refused when pressed, having no harness pane to aim at.
        #
        # **#791 removed that rung, so the measurement is gone again**: every rung of
        # `state.own_workspace` now reads a file inside the frame's own directory, and a
        # loose file answers ``None`` through the read whatever this filter does — checked,
        # nothing in the suite goes red without it. It stays because it is the correct
        # predicate rather than a second guard for the same fact (a chat IS a directory;
        # this is the scan deciding what it is looking at, not a membership rule), and
        # because it is free: `os.DirEntry.is_dir` is answered from `readdir`'s own
        # `d_type` on Linux and macOS, so on the ~30 entries this scans it is ordinarily
        # not a `stat` at all. A reader deleting it should know it costs nothing and
        # protects the next out-of-directory rung, not that a test is watching it.
        names = [e.name for e in os.scandir(state._root()) if e.is_dir()]
    except OSError:
        # No frame root at all is the ordinary answer on a plane that has never launched
        # one, and an unreadable one is the same answer for the caller: no chats to
        # offer. Never a raise — a picker that could not scan refuses with its own
        # sentence one layer up.
        return []
    return sorted((n for n in names
                   if is_chat(n) and state.own_workspace(n) == workspace),
                  key=_order)


#: How many digits an ordinal `state.new_chat_id` can mint has, derived from its own
#: ceiling rather than written down twice.
#:
#: **It is a bound on `int()`, not on taste.** CPython refuses to convert a string of more
#: than 4,300 digits to an integer — `int("9" * 5000)` raises `ValueError`, not
#: `OverflowError` — and a name off `os.scandir` is whatever is on disk. Without this, a
#: directory called `api.<5000 nines>` under `.charter/frame/` would raise out of
#: `sorted`, in a panel's render path, and take the bar down with it. The state directory
#: is 0700 and `SECURITY.md:43-46` is honest about what that is worth; this is a guard
#: against a mistake, and the mistake is a crash where a sort was wanted.
#:
#: Bounded HERE and not in :func:`is_chat`, deliberately: that function is Stage 5a's
#: version discriminator, and teaching it about ordinal size would make `api.100000` —
#: perfectly parseable, merely above the allocator's ceiling — stop being a chat and
#: start being reaped by the wrong rule. A name this cannot read the ordinal of is still
#: a chat; it just sorts with the others it cannot read.
_MAX_ORDINAL_DIGITS = len(str(state._CHAT_ORDINAL_MAX))


def _order(fid: str) -> tuple[int, int, str]:
    """Sort key for :func:`of_workspace` — ordinal first, unparsable names last.

    Split out so the ordering is a thing a test can hand a list of names to without a
    plane underneath it, and so the two halves of "unparsable sorts last" are one
    expression rather than a branch in the loop above.

    **`sep` and not `head`.** `rpartition` answers `("", "", name)` for a name with no
    separator in it, so the empty separator already refuses `api5` — a truthiness test on
    the head would be a second guard for the case the first one caught, and the only name
    it could tell apart is `.5`, whose ordinal is as good an answer as sorting it last.
    That is the shape `state.frame_workspace` names for its own `valid_name`.
    """
    _head, sep, tail = fid.rpartition(state._CHAT_SEP)
    if sep and tail.isdigit() and len(tail) <= _MAX_ORDINAL_DIGITS:
        return (0, int(tail), fid)
    return (1, 0, fid)


def harness_of(fid: str) -> str:
    """The harness name recorded for chat *fid*, or ``""``.

    `state.identity`, which is what the LAUNCH put on tmux's `-e` — never
    `os.environ["CHARTER_HARNESS"]`. This is asked by a palette and by a panel, both of
    which are children of a tmux server shared between every frame on the machine, so
    this process's own variable may be another chat's (`state.record_identity` measures
    exactly that).

    ``""`` for a chat charter has no identity record for, which is the migration case and
    an ordinary one — a bar draws the id alone rather than inventing a harness.
    """
    return state.identity(fid).get("CHARTER_HARNESS", "").strip()


def pane_of(chat: str) -> str | None:
    """The tmux pane charter records for *chat*, held to tmux's own shape — or ``None``.

    **One read of that record, asked by both the check and the switch**, and the reason it
    is a function rather than a line in each is what the sweep found: `cmd_chat` re-read
    it with an `or ""` fallback, so a record that changed between the two reads — a reap,
    a relaunch — became `select-window -t ""`. An empty tmux target is not nothing; it
    resolves to the CURRENT window, so charter would have reported a switch that did not
    happen and then torn the panels down around it.

    `tmuxctl.PANE_ID_RE` and not merely "non-empty", for #475's reason and at #475's
    boundary: this value comes off disk and is about to be a `-t` target, and
    `%1;kill-server` in that file is the shape that already cost this project a
    `kill-server` armed on every window resize.
    """
    pane = state.harness_pane(chat) or ""
    return pane if tmuxctl.PANE_ID_RE.fullmatch(pane) else None


def roster(fid: str) -> list[Chat]:
    """Every chat in *fid*'s workspace, with *fid* marked.

    **Keyed on the FRAME's workspace, not this process's** (#512) — `state.workspace_for`
    is the one rule every frame surface asks, and resolving locally would list another
    plane's chats on this frame's screen.

    **Two questions, one ladder** (#733). This one is "which workspace is *fid* DRAWING",
    which is `workspace_for` and includes the rungs that answer for *fid*'s own process —
    the pin it was launched under, and, for a frame with no records at all, a local
    resolve. :func:`of_workspace` then asks "which chats say they are in that one", which
    is `state.own_workspace` and can only be the records. Those are genuinely different
    questions and they are asked at the same depth: `own_workspace` IS the middle of
    `workspace_for`, so a chat's own answer moving moves both. It used to be a rung
    shallower, and a chat could be drawing `alpha` while `alpha`'s roster did not have it.

    *fid* itself is folded in whether or not the scan found it, and that is the honest
    answer rather than a convenience: a frame whose `workspace` file could not be read is
    still the chat you are typing in, and a bar that omitted the active chat would be
    drawing a list the operator is not in. It is folded in at the FRONT of nothing —
    :func:`of_workspace`'s order decides where it lands, so the active chat does not move
    to the end of the bar because its record was unreadable.
    """
    names = of_workspace(state.workspace_for(fid))
    if is_chat(fid) and fid not in names:
        names = sorted([*names, fid], key=_order)
    return [Chat(id=n, harness=harness_of(n), active=n == fid) for n in names]


def others(fid: str) -> list[str]:
    """The chat ids in *fid*'s workspace that are not *fid*.

    What "is there anything to switch to" is asked as, in one place, so the doorway that
    refuses a picker and the bar that hides itself cannot answer it differently.
    """
    return [c.id for c in roster(fid) if not c.active]


#: What a frame alone in its workspace is told instead of a picker over one row —
#: itself. Its own sentence rather than `choose.NO_CHANGES`': that one is about a
#: workspace with no cross-repo change, this is about a workspace with nothing to switch
#: BETWEEN, and it names what makes a second one rather than leaving the operator to
#: guess. A picker over the row you are already on is an offer charter knows it cannot
#: honour, which is the same rule the change doorway keeps.
ONLY_CHAT = ("this workspace has one chat — open another with `charter <harness>` in "
             "this workspace, and it joins this one as a second tab")


def check(fid: str, chat: str) -> Outcome:
    """Whether frame *fid* may switch to *chat*, and the one line to say either way.

    **Validation only: nothing here selects a window, and that is what lets the palette
    and `charter frame-chat` ask the same question.** The palette asks it before it
    spawns, so a refusal reaches the pane the operator is still looking at; the command
    asks it again because it is also typeable by hand, on a name nobody drew. Two
    askers, one rule — the shape `switch.to_persona` already has for a name off a picker
    and a name off an argv.

    Refusals, in the order they are checked, and each names a thing the operator can act
    on:

    * **not a chat id** — :data:`ID_RE`, the alphabet a frame id reaches tmux under. A
      name off a picker charter built cannot fail this; one typed at `charter frame-chat`
      can, and it is the last point before the name becomes a state path.
    * **not a chat of this workspace** — an unknown name is a question, never an implicit
      create (`switch.py`'s rule, one noun over), and the existing names go in the message
      the way `switch.to_persona` already answers an unknown persona.
    * **already here** — refused rather than performed. A switch to the chat you are in
      would tear down this chat's panels and split them again for no change on screen,
      which is a re-layout an operator did not ask for and ~90 ms of blank panes; and the
      row is drawn with `choose.MARK` beside it, so the operator can see they are already
      there before they press it.
    * **not on this frame's tmux server** — and this one is about an IDENTITY where every
      rule above it is about a name (#684). A workspace's chats are the directories whose
      `workspace` file names it, and that file says nothing about where the chat is
      RUNNING: charter has two servers (its own `-L charter` and, inside an operator's
      tmux, theirs), a chat can be open on each, and `state.frame_server` is the record
      that tells them apart. Pane ids are per-server — `%3` on one is somebody else's pane
      on the other — so a switch that crossed servers would aim a `select-window` at a
      real, live, unrelated pane and be told it worked. Compared as recorded, with no
      default filled in for a missing marker: every chat this charter launches records one
      on both paths, so an absent value is a truncated record rather than a migration
      (`of_workspace` already keeps old `{workspace}-{pid}` frames out of the roster
      entirely), and "charter cannot tell" is the same answer as "somewhere else" for a
      switch that is about to move a client.
    * **no usable harness pane** — the target's window cannot be named. `select-window`
      is aimed at the chat's own harness pane (measured on tmux 3.7c and 3.2: a pane id
      resolves to that pane's window), and `state.harness_pane` is the record that holds
      it. A chat launched by a charter that predates that record, or one whose directory
      was truncated, has nothing to aim at — and guessing at a pane id is the one thing
      `frame/layout.py`'s module docstring measures the cost of. `tmuxctl.PANE_ID_RE` and
      not merely "non-empty", for #475's reason and at #475's boundary: this value comes
      off disk and is about to be a `-t` target, and `%1;kill-server` in that file is
      the shape that already cost this project a `kill-server` armed on every resize.

    **Liveness is deliberately NOT a refusal here**, and that is not an oversight: it is
    the one question this module cannot answer without tmux, and asking it twice — once
    for the row and once for the switch — would be two answers to it. `cmd_chat` selects
    the window and reports what tmux said, which is the reading that is true at the
    instant it matters rather than the instant the palette opened.

    **Nor is the tmux SESSION, for the same reason and with a sharper edge** (#684). The
    server above is a record and belongs here; which session a chat's window is in is a
    fact only tmux holds, it moves while the palette is open, and `cmd_launch` makes the
    workspace the session while membership here is read from a file `charter workspace
    use` can repoint. `cmd_chat` asks it (`commands_frame._pane_place`) at the one moment
    it decides anything, and refuses there.
    """
    shown = contain.one_line(chat)
    if not ID_RE.fullmatch(chat or ""):
        return Outcome(False, f"'{shown}' cannot name a chat")
    names = [c.id for c in roster(fid)]
    if chat not in names:
        return Outcome(False, f"no chat '{shown}' here — have: {_some(names)}")
    if chat == fid:
        return Outcome(False, f"already in chat '{shown}'")
    if state.frame_server(chat) != state.frame_server(fid):
        return Outcome(False, f"chat '{shown}' is not on this frame's tmux server, so "
                              "charter cannot move this client to its window")
    if pane_of(chat) is None:
        return Outcome(False, f"charter has no usable record of chat {shown}'s harness "
                              "pane, so it cannot find its window — relaunch that chat")
    return Outcome(True, f"chat → {shown}")
