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
    """Every chat id whose record names *workspace*, in ordinal order.

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
    when a bar repaints, not on a tick. `state.reap` is what bounds it.
    """
    try:
        # `is_dir()` inside the same `try` as the scan, deliberately, and it is one guard
        # rather than two. A `DirEntry.is_dir()` can raise `OSError` of its own — a
        # `stat` on an entry that went away between the scan and the question, or one the
        # process may not stat — and a second `try` around it would be a line that only a
        # race could turn red, which is a line no test can pin. Answered the same way for
        # the same reason the outer one is: a bar that could not read the directory draws
        # nothing.
        names = [e.name for e in os.scandir(state._root()) if e.is_dir()]
    except OSError:
        # No frame root at all is the ordinary answer on a plane that has never launched
        # one, and an unreadable one is the same answer for the caller: no chats to
        # offer. Never a raise — a picker that could not scan refuses with its own
        # sentence one layer up.
        return []
    return sorted((n for n in names
                   if is_chat(n) and state.frame_workspace(n) == workspace),
                  key=_order)


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
    if sep and tail.isdigit():
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


def roster(fid: str) -> list[Chat]:
    """Every chat in *fid*'s workspace, with *fid* marked.

    **Keyed on the FRAME's workspace, not this process's** (#512) — `state.workspace_for`
    is the one rule every frame surface asks, and resolving locally would list another
    plane's chats on this frame's screen.

    *fid* itself is folded in whether or not the scan found it, and that is the honest
    answer rather than a convenience: a frame whose `workspace` file could not be read is
    still the chat you are typing in, and a bar that omitted the active chat would be
    drawing a list the operator is not in. It is folded in at the FRONT of nothing —
    :func:`of_workspace`'s order decides where it lands, so the active chat does not move
    to the end of the bar because its record was unreadable.
    """
    names = of_workspace(state.workspace_for(fid))
    if fid and is_chat(fid) and fid not in names:
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
    askers, one rule — the shape `switch.to_workspace` already has for a name off a
    picker and a name off an argv.

    Refusals, in the order they are checked, and each names a thing the operator can act
    on:

    * **not a chat id** — :data:`ID_RE`, the alphabet a frame id reaches tmux under. A
      name off a picker charter built cannot fail this; one typed at `charter frame-chat`
      can, and it is the last point before the name becomes a state path.
    * **not a chat of this workspace** — an unknown name is a question, never an implicit
      create (`switch.py`'s rule, one noun over), and the existing names go in the message
      the way `switch.to_workspace` already answers an unknown workspace.
    * **already here** — refused rather than performed. A switch to the chat you are in
      would tear down this chat's panels and split them again for no change on screen,
      which is a re-layout an operator did not ask for and ~90 ms of blank panes; and the
      row is drawn with `choose.MARK` beside it, so the operator can see they are already
      there before they press it.
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
    """
    shown = contain.one_line(chat)
    if not ID_RE.fullmatch(chat or ""):
        return Outcome(False, f"'{shown}' cannot name a chat")
    names = [c.id for c in roster(fid)]
    if chat not in names:
        return Outcome(False, f"no chat '{shown}' here — have: {_some(names)}")
    if chat == fid:
        return Outcome(False, f"already in chat '{shown}'")
    if not tmuxctl.PANE_ID_RE.fullmatch(state.harness_pane(chat) or ""):
        return Outcome(False, f"charter has no usable record of chat {shown}'s harness "
                              "pane, so it cannot find its window — relaunch that chat")
    return Outcome(True, f"chat → {shown}")
