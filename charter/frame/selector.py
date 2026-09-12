"""The profile selector: no harness starts until somebody picks a profile.

**Opening charter used to start a harness nobody asked for.** Bare `charter` launched
`[harness] default` and `+` launched the kind of the chat it was pressed from, so an empty
workspace got a harness session before anybody said which one they wanted. A new chat's
window now opens with this in its harness pane, and the harness starts there once a profile
is chosen (`frame/launcher.cmd_frame_launch --select`).

**It is the F2 palette's picker, not a second surface.** Type to filter, Enter to choose,
Esc to cancel — `palette.Palette` over a different row source, which is exactly what
`frame/choose.py`'s workspace and persona pickers are. What it adds is a footer
(`overlay.Surface.footer`) and one rule the palette does not have.

**That rule: Enter on a refused row does not close it** (ruling 6). The palette closes on a
refused Enter and puts the reason on the frame's attention row; there the surface is a pane
split off a running harness, so closing it costs nothing. Here the surface IS the chat's
pane, and closing it closes the chat — so the reason goes in the footer, the row stays, and
only Esc closes the window. A pick the fresh check at launch refuses comes back the same way
(ruling 30): `frame/launcher._select_in_pane` hands it back as a :class:`Refused` and the
row is redrawn with it.

**It always shows, even where one profile is available.** Skipping it would bring back the
harness nobody picked on a one-harness machine; one profile costs one Enter.

**Every profile-derived string drawn here is contained** (ruling 35). `charter.local.toml`
is a file a chat can write, and a `\\r` or an ESC in a `command` could otherwise redraw the
approval prompt to show a harmless command while another one runs. The values are shown
escaped (`contain.readable`), never interpreted.

**Nothing here is on the import path** (ruling 43). `frame/launcher.py` imports this at call
time, because every `charter hook …` process builds the parser and derives config.
"""

from __future__ import annotations

import shutil
from dataclasses import dataclass
from pathlib import Path
from typing import NamedTuple

from .. import contain, profiles
from . import overlay, palette

#: What a pane that started nothing exits with — the shell's own number for "ended by the
#: operator", and what `_launch` reads back to say nothing at all about the chat.
CANCELLED_EXIT = 130

#: What every row's id starts with, so a chosen row names a profile without the title —
#: which is `contain.readable`'s answer and may have been clipped — having to be parsed.
ROW_PREFIX = "profile:"

LABEL = "charter · which profile?"

#: The one key that always works here, and the one this surface may never clip away: at the
#: selector Esc is the whole of the way out, and it is the ONLY thing that still works in a
#: list where every row is refused.
ESC_HINT = "esc close this chat"

#: `enter start`, not `enter choose`, and no `F12`: there is no harness in this pane yet to
#: go back to, and Esc here closes the chat rather than an overlay over one.
FOOTER = f"  up/down move   enter start   {ESC_HINT}"

NOTHING_TO_PICK = ("charter: no profile can start here — every row above says why. Nothing "
                   "was started; fix one of them and open a chat again.")

NOT_ON_PATH = "not on PATH: {cmd}"

CONFIRM = "run this? {shown}"

#: The confirm's one row. A `Surface` with no rows answers `None` for a CHOOSE — which
#: `Surface.run` cannot tell from a cancel — so the question has something to be answered
#: with. `y` is the key; the row is what the operator reads while deciding.
CONFIRM_ROW = overlay.Row(id="confirm:yes", title="y   run it",
                          note="any other key goes back to the list")

CONFIRM_FOOTER = f"  y run it   any other key goes back   {ESC_HINT}"


class Choice(NamedTuple):
    """The profile the operator picked. A type of its own rather than a bare string,
    because ``None`` from :func:`pick` means *Esc* and an empty name would not."""

    profile: str


class Refused(NamedTuple):
    """A pick the launch refused, carried back into the next selector (ruling 30).

    Held apart from the row's own state because it outranks it: the row was drawn from what
    was true when the pane painted, and this is what the fresh check said a moment ago.
    """

    profile: str
    why: str


class Pending(NamedTuple):
    """What Tasks 3 and 4 have to say about one profile's row.

    *refused* is whether Enter may start it, *note* is the row's right-hand sentence, and
    *ask* is whether Enter shows the profile's command and asks in place. The three are one
    record because the two states they describe differ in all three: *not wired* refuses,
    while *not approved yet* does not refuse — it asks.
    """

    refused: bool
    note: str
    ask: bool


# --------------------------------------------------------------------------- #
# The seam Tasks 3 and 4 fill.
# --------------------------------------------------------------------------- #


def pending(p: profiles.Profile, *, cwd: Path) -> Pending | None:
    """Whether *p* is approved and wired, or ``None`` when neither has anything to say.

    **One function, because the two answers are asked in one order and never apart.**
    Task 3's approval comes first and Task 4's wiring second, and that order is ruling 1
    rather than a preference: detecting wiring RUNS the profile's own command
    (`[*command, "plugin", "list", "--json"]`), so a profile whose launch record does not
    match is never probed — its row says it is not approved yet instead. Splitting them
    would put that ordering in every caller.

    *cwd* is the chat's own directory, which is what makes the wiring answer this chat's:
    Claude Code follows the most specific covering scope (local > project > user), so the
    settings files beside the chat decide, and the cache stamp has to cover them rather
    than only the plane root's (the fourth review's nit on this task).

    ``None`` on this branch, which is what makes every installed profile's row runnable
    until Tasks 3 and 4 are merged and this body is filled in. It runs no subprocess, so
    the selector on `main` spawns nothing: ruling 11's promise holds by construction here
    and by measurement once the probe exists.
    """
    return None


def approve(p: profiles.Profile) -> str:
    """Record the operator's yes for *p*, or say why it could not be recorded.

    :func:`pending`'s other half and the reason it is a seam rather than a caller's branch:
    the ask is charter's approval of a `command`, so the surface that asked is the surface
    that records it. Task 3's `profiletrust.record_launched` fills this in.

    **The write error rather than a bool** (the fourth review's nit): a launch record that
    fails to write refuses rather than re-asking, so the sentence has to reach the footer.
    ``""`` is "recorded".
    """
    return ""


# --------------------------------------------------------------------------- #
# The rows.
# --------------------------------------------------------------------------- #


def read(root: Path) -> profiles.ProfileSet:
    """Every profile this plane has, with the git check applied — `charter harness list`'s
    own two calls, so the selector and the listing say one thing about one file.

    The check is a `git --no-optional-locks status` and it runs HERE because somebody is
    looking at a selector: it never runs on a config read, and never in a hook.
    """
    return profiles.with_ignore_check(profiles.current(), profiles.ignore_check(root))


def rows(have: profiles.ProfileSet, *, cwd: Path, start: str | None = None,
         after: Refused | None = None) -> tuple[overlay.Row, ...]:
    """One row per profile, in `charter harness list`'s order, with its state on it.

    **Declared profiles always; a built-in only when its program is installed.** A built-in
    comes out of charter's own registry rather than out of anybody's file, so a kind nobody
    has installed is not a row that says why — it is a harness this machine does not have.
    A DECLARED profile is the opposite: the operator wrote it down, so a missing command is
    a row with its reason on it (#512 — an option you cannot see is one you cannot ask
    about).

    **A refused profile is a row too.** `profiles.current` drops a profile it refused into
    `have.refused`, and `with_ignore_check` moves every declared one there when git would
    carry the file — so those names would otherwise simply not appear, which is the one thing
    #512 refuses. A whole-FILE refusal has no name and no row; it is `doctor`'s and
    `charter harness list`'s to report.

    The state, first match wins:

    ===============================  ========  ==================================
    State                            refused   Note
    ===============================  ========  ==================================
    the launch just refused it       yes       the refusal (ruling 30)
    `have.refused` holds the name    yes       the reason, the file's or git's
    the command is not on `PATH`     yes       ``not on PATH: <cmd>``
    :func:`pending` has an answer    its own   its own
    otherwise                        no        ``<kind> · <env and command>``
    ===============================  ========  ==================================

    *start* MARKS its row and nothing else. A `default` naming a profile this machine lacks
    marks nothing (ruling 18) — there is no row to mark — and the cursor is
    :class:`Selector`'s to place.
    """
    order = list(profiles.builtins())

    def place(name: str) -> tuple[int, int, str]:
        # Built-ins in registry order, then everything declared by name — and a refused
        # name keeps the place it would have had, so a `[harness.claude]` charter refused
        # is drawn where `claude` was rather than at the bottom of the list.
        return (name not in order, order.index(name) if name in order else 0, name)

    out: list[tuple[tuple[int, int, str], overlay.Row]] = []
    for p in have.profiles.values():
        program = profiles.expanded_command(p)[0]
        installed = shutil.which(program) is not None
        if p.source == profiles.BUILTIN and not installed:
            continue
        if after is not None and after.profile == p.name:
            refused, note = True, after.why
        elif not installed:
            refused, note = True, NOT_ON_PATH.format(cmd=contain.readable(program))
        else:
            state = pending(p, cwd=cwd)
            refused, note = ((False, f"{p.kind} · {profiles.display(p)}")
                             if state is None else (state.refused, state.note))
        out.append((place(p.name),
                    overlay.Row(id=ROW_PREFIX + p.name, title=contain.readable(p.name),
                                note=note, mark=p.name == start, refused=refused)))
    for r in have.refused:
        if not r.name:
            continue
        # **These ids carry a CONTAINED name where a profile row's carries the raw one**,
        # and the asymmetry is stated rather than tidied away because it cannot be tidied:
        # `profiles.Refused.name` is `contain.readable`'s answer and the original is not
        # kept, so there is nothing raw to put here. What it costs is exact and bounded: a
        # refused name holding a byte `contain.readable` escapes never equals `p.name`
        # above, so the `after` branch cannot match one. It does not need to — `after` is a
        # profile the LAUNCH resolved, and a name `read` refused never reaches a launch.
        out.append((place(r.name),
                    overlay.Row(id=ROW_PREFIX + r.name, title=r.name, note=r.reason,
                                refused=True)))
    return tuple(row for _place, row in sorted(out, key=lambda pair: pair[0]))


@dataclass
class Selector(palette.Palette):
    """The palette over profile rows, with the selector's own label and bottom line."""

    label: str = LABEL

    #: The profile the cursor OPENS on, or ``""`` for `palette.aim`'s own answer.
    #:
    #: **Whether a row may be opened on is not decided here** — :func:`opens_on` decides it
    #: for a preselected default (ruling 18: never on a row whose Enter can only say why),
    #: and a return trip after a refused Enter passes the refused row deliberately, because
    #: the operator is looking at the reason for the row they just pressed.
    #:
    #: Applied once, at the opening paint. After a keystroke the operator is looking at a
    #: different list and `palette.aim` re-aims it, which is `Palette._refilter`'s own
    #: decision and not one this surface gets to take back.
    on: str = ""

    def _refilter(self) -> None:
        # No early return for an empty :attr:`on`, deliberately: `""` makes *wanted* a
        # string no row id can equal (every one carries a name after the prefix), so the
        # loop below is already the no-op an early return would have been — and a guard
        # that only restates what the next line says is the shape this repository deletes
        # (the sweep reported exactly that one as a survivor).
        super()._refilter()
        wanted, self.on = (ROW_PREFIX + self.on if self.on else ""), ""
        for i, row in enumerate(self.rows):
            if row.id == wanted:
                self._sel = i
                return


@dataclass
class Confirm(overlay.Surface):
    """`run this? <command>` — Task 3's ask, in the pane that asked it.

    A surface rather than a prompt for the reason the selector is one: this pane is in raw
    mode with the alternate screen up, so a `input()` underneath it would be typed into a
    rectangle nothing is drawing.

    `y` and nothing else, because this is an approval: an operator who pressed a key they
    did not mean goes back to the list rather than approving a command. A pointer event is
    not an answer to a question at all, so it is ignored rather than read as a no.
    """

    rows: tuple[overlay.Row, ...] = (CONFIRM_ROW,)
    footer: str | None = CONFIRM_FOOTER
    #: No `· 1 to choose from` after the command: this surface asks one question, and the
    #: count would describe the widget rather than the thing being approved. What it shows
    #: instead, where a command is wider than the pane, is how much it hid
    #: (`overlay._clipped`) — a `y` must never approve a command cut without a word.
    counted: bool = False

    def handle(self, ev: overlay.Event, height: int) -> str | None:
        if ev.kind != overlay.KEY:
            return None
        return overlay.CHOOSE if ev.name in ("y", "Y") else overlay.CANCEL


def opens_on(listed: tuple[overlay.Row, ...], start: str | None) -> str:
    """Which row a preselected *start* opens the cursor on — ``""`` for `palette.aim`.

    **Ruling 18, and it is one sentence in two halves.** A `default` naming a profile this
    machine lacks marks no row and opens on none of its own: there is nothing to open on.
    A `default` that IS declared and cannot run — its command uninstalled, its file
    committable, its wiring gone — is the same case for the operator, because Enter on it
    can only say why, and *Enter must always do something* is the rule the cursor exists to
    keep. Both fall through to the palette's own answer, the first row that can run.
    """
    name = start or ""
    wanted = ROW_PREFIX + name
    return name if any(row.id == wanted and not row.refused for row in listed) else ""


def _footer(listed: tuple[overlay.Row, ...], after: Refused | None) -> str:
    """The bottom line: what the last Enter refused, else that nothing can start, else the
    keys — and :data:`ESC_HINT` in every one of them.

    **The reason for what the operator just pressed outranks the summary**, which is ruling
    30 read exactly: Enter on a refused row shows THAT row's reason. Checked the other way
    round, a refused Enter in an all-refused list would answer with the summary — the one
    state where the operator most needs the row's own sentence. :data:`NOTHING_TO_PICK` is
    the FIRST-paint answer for a list where nothing can run, which says so while it is being
    read rather than only after an Enter has found out.

    **The key hint comes first when a reason is present**, and that is a clipping decision
    rather than a typographic one: a reason is arbitrary-length text out of the profile
    file, so whatever is last is what a narrow pane cuts (`overlay._clipped` says how much
    it took). Esc is the only thing that works in the state this footer describes, so it is
    the half that must survive.
    """
    said = (after.why if after is not None
            else (NOTHING_TO_PICK if not any(not r.refused for r in listed) else ""))
    return f"  {ESC_HINT}   ·   {said}" if said else FOOTER


def pick(*, cwd: Path, root: Path, start: str | None = None,
         after: Refused | None = None, fd: int | None = None, out=None) -> Choice | None:
    """Own the pane until a profile is picked. The :class:`Choice`, or ``None`` for Esc.

    Loops on a refused row with the reason in the footer (ruling 6) and on a new or changed
    profile that was not approved, so what comes back is a name the operator meant to start.
    Whether it CAN start is asked again by the launch, fresh (review B2), and a refusal
    there re-enters this function through *after*.

    The profiles are re-read every round rather than once: the operator may have edited
    `charter.local.toml` in another window while the selector was up, and a list that could
    not see that would answer Enter with a reason the file no longer gives.
    """
    while True:
        have = read(root)
        listed = rows(have, cwd=cwd, start=start, after=after)
        surface = Selector(catalogue=listed, footer=_footer(listed, after),
                           on=(after.profile if after is not None
                               else opens_on(listed, start)))
        chosen = palette.own_the_tty(surface, fd=fd, out=out)
        if chosen is None:
            return None
        name = chosen.id.removeprefix(ROW_PREFIX)
        p = have.profiles.get(name)
        if chosen.refused or p is None:
            # `p is None` is a row for a name `read` refused, and it is also the row of a
            # profile that stopped existing between the paint and the Enter. One answer for
            # both: the note says why, and the list comes back.
            after = Refused(name, chosen.note)
            continue
        state = pending(p, cwd=cwd)
        if state is not None and state.ask:
            if palette.own_the_tty(Confirm(heading=CONFIRM.format(
                    shown=profiles.display(p))), fd=fd, out=out) is None:
                after, start = None, name
                continue
            why = approve(p)
            if why:
                after = Refused(name, why)
                continue
        return Choice(name)
