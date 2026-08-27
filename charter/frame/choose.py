"""The pickers: choosing a workspace or a persona from inside a running frame.

**This is `frame/overlay.py` over a list of NAMES, and that is the whole of it** — the
seam `frame/palette.py` named when it wrote `rows(offers)` beside `Palette(catalogue=…)`:

    Task 6's workspace and persona pickers are the same :class:`Palette` over a
    different row source, and a picker that had to subclass something would be a
    second surface wearing this one's name.

So there is no `Picker` class here. There is a function from names to `overlay.Row`s, a
function from a chosen row back to the name it stood for, and one call into
`frame/switch.py`. Everything modal, every key, the scrolling window, the containment and
the escape hatch are already decided one module down and are not restated.

**The same rows are reached two ways, and only one of them costs a directory read.** A
doorway row (:func:`open_rows`) opens a picker over :func:`roster`, which is browsing — you
do not know the name. Typing at the top level reaches the very same rows through
:func:`labelled`, which is switching — you do. The palette gathers them the first time
something is typed and never while the query is empty (`frame/palette.Palette.query_only`),
so an operator who opened `F2` to press `detach` does not pay to enumerate forty
workspaces, and one who knows the name spends `F2`, the name, Enter — the doorway's Enter
back.

**The picker runs in the palette's OWN pane, and that is a property rather than a saving.**
The alternative — a palette row that spawns a second `charter` which splits a second
overlay pane — races the palette's own teardown: `commands_frame._close_palette` sends
`select-pane ; kill-pane ; set-option @charter_hatch` as one chained command the instant
the row has been chosen, and a picker pane that had already selected and zoomed itself
would be un-selected, un-zoomed and un-hatched by it. Reusing the pane has no interleaving
to lose: `palette.own_the_tty` hands one surface to the next inside the same raw-mode
window, and the pane is handed back exactly once, at the end.

**A picker row's id is deliberately not an action id.** `frame/action.py` holds every
action id to `component._ID_RE` — lower-case letters, digits, underscores and at most one
dot — and the palette dispatches on the id of whatever was chosen. If a picker's rows used
that alphabet, a provider shipping an action called `pick.workspace` would take the
keypress. The `:` in :data:`OPEN_ID` and :data:`NAME_ID` cannot appear in an action id, so
the two namespaces cannot meet; `overlay.Row` imposes no alphabet of its own, and none of
these ids is ever drawn or ever reaches tmux.

That same fact is what keeps these ids out of the FILTER now that name rows sit in the
top-level list. `frame/palette.matches` matches a row's id only when the id is one a
provider's documentation could name — `component.usable_id`, the same question
`frame/action.py` asks — because :data:`NAME_ID` is charter's own counter and nobody types
`workspace:n7`. Matched blindly it would make `n` list every name on the plane and
`persona` list every persona, which is a filter answering a question nobody asked.

**Containment is `frame/overlay.py`'s, asked for once, where the width arithmetic is.**
`Surface.render` runs `contain.one_line` over every title and note *before* `tui.width`
sees them (#472) — so a workspace directory holding a newline, a U+2028 or an escape
sequence becomes one row there, and a second `contain.one_line` on the way in here would
be a line no test could go red without. That is not a claim, it is the shape this
repository has now been bitten by four times, and `builtin_actions._register_names` — the
code this module replaces — carried exactly such a line: its `contain.one_line(name)` was
masked by `Action.__post_init__` containing the title it landed in, and its test stayed
green over the deletion. `tests/test_frame_pickers.py` measures the property against
hostile names instead of restating the call.

**What is NOT display text goes to `frame/switch.py` raw**, which is the other half of the
same rule. :meth:`Roster.name_of` answers the name a row stood for, un-contained, because
`switch.to_workspace` is the one place a name is checked against `workspace.valid_name`
and refused — a name repaired on the way in would be a name charter switched to having
never looked at.

**Nothing here creates anything and nothing here touches tmux.** `switch.py`'s reasons,
both: an unknown name is a refusal with the existing names beside it, and the one
`display-message` that puts an outcome on screen belongs to `commands_frame`.
"""

from __future__ import annotations

from typing import NamedTuple

from .. import contain
from . import overlay, switch

#: The two nouns a picker offers. Values rather than an enum for the reason the rest of
#: this package uses strings — they appear in a test's failure message as themselves —
#: and they are also what `charter frame-switch` already spells its flags with, so the one
#: word travels from the row to the command without a table in between.
WORKSPACE, PERSONA = "workspace", "persona"

#: Both, in the order the palette offers them. Workspace first: it is the plane a frame is
#: looking at, and a persona is who is looking.
NOUNS = (WORKSPACE, PERSONA)

#: Which launch pin outranks a switch of each noun — the rung nothing charter writes can
#: reach, because it is already in every panel pane's environment (`switch.py`'s module
#: docstring). Named here because the row that opens a picker is the surface that reports
#: it, and the reporting and the refusal must read the same variable.
PIN = {WORKSPACE: "CHARTER_WORKSPACE", PERSONA: "CHARTER_PERSONA"}

#: What marks the name the frame is on right now.
#:
#: **ASCII, deliberately** — `overlay._MARK`'s own rule and `statusline._persona_chips`'
#: measurement behind it: `●`, `◆` and the pointing triangles are East-Asian *Ambiguous*
#: and have broken this layout twice. Both entries are the same width by construction, so
#: the mark moving does not move the names beside it.
MARK = ("* ", "  ")

#: The id of the palette row that OPENS a picker, and of one name row inside one. See the
#: module docstring for why both carry a `:` — it is what keeps them out of the action
#: alphabet, and it is never drawn.
OPEN_ID = "pick:{}"
NAME_ID = "{}:n{}"


class Roster(NamedTuple):
    """One picker's rows, and the names they stand for.

    The two are carried together and matched by ROW ID rather than by title, because the
    title carries :data:`MARK` and a committed name that `overlay.Surface.render`
    contains before drawing — so the string on screen is not the string to switch to, and
    a surface that matched on what it drew would switch to a repaired name or to nothing.
    """

    #: :data:`WORKSPACE` or :data:`PERSONA`. Carried so a caller that opened a picker does
    #: not have to remember which one it asked for.
    noun: str
    #: What the overlay draws, in the order `switch.workspaces`/`switch.personas` gave.
    rows: tuple[overlay.Row, ...]
    #: The raw names, index-aligned with :attr:`rows`. Never contained — see the module
    #: docstring.
    names: tuple[str, ...]

    def name_of(self, row: overlay.Row) -> str | None:
        """The name *row* stood for, or ``None`` for a row that is not this roster's.

        ``None`` rather than a raise or an empty string: an id that is not here means the
        surface answered with a row this roster did not build, and `switch.to_workspace`
        would read `""` as a name to check rather than as an absence.
        """
        for r, name in zip(self.rows, self.names):
            if r.id == row.id:
                return name
        return None


def names_of(noun: str) -> list[str]:
    """Every name *noun* offers, from `frame/switch.py`'s own listers.

    Asked there rather than re-derived, so the list a picker draws and the list a refusal
    names ("no workspace 'x' — have: …") cannot disagree. Both are already held to
    `workspace.valid_name`/`persona.valid_name` on the way out, which is #442's rule
    applied where a directory name becomes a row.
    """
    return switch.workspaces() if noun == WORKSPACE else switch.personas()


def current(noun: str, fid: str) -> str:
    """The name frame *fid* is on right now — the row that gets :data:`MARK`.

    `switch.current_persona` answers ``None`` for a plane with no persona at all, which is
    an ordinary answer rather than a missing one; it becomes `""`, which matches no name
    and therefore marks no row.
    """
    if noun == WORKSPACE:
        return switch.current_workspace(fid)
    return switch.current_persona(fid) or ""


def pin_reason(noun: str, fid: str) -> str:
    """Why *fid* will not switch its *noun*, or `""` when it will.

    The same sentence `switch.to_workspace` refuses with, built from the same read
    (`switch._pin`), so the row and the keypress cannot describe one frame two ways. `""`
    exactly when the frame is not pinned, which is what makes "available" and "has no
    reason" one decision here rather than two that can contradict each other on screen.
    """
    pinned = switch._pin(fid, PIN[noun])
    if not pinned:
        return ""
    return (f"cannot switch: ${PIN[noun]} pins this frame to "
            f"'{contain.one_line(pinned)}'")


def roster(noun: str, fid: str) -> Roster:
    """Every name *noun* has, as rows, with the one *fid* is on marked.

    No cap and no reordering: `frame/palette.py`'s `narrow` filters this list as the
    operator types and never reorders it, so the row under the cursor stays under the
    cursor between keystrokes.

    The note column is left empty. The one refusal a picker could carry per row is the
    launch pin, and that is reported by the row that OPENS the picker (:func:`open_rows`)
    — where it stops the pane being drawn at all, one keypress earlier. A reason repeated
    on every row of a picker that cannot be reached while it is true is a line nothing
    could go red without.
    """
    names = tuple(names_of(noun))
    now = current(noun, fid)
    rows = tuple(overlay.Row(id=NAME_ID.format(noun, i),
                             title=f"{MARK[0] if n == now else MARK[1]}{n}")
                 for i, n in enumerate(names))
    return Roster(noun=noun, rows=rows, names=names)


def labelled(roster: Roster, reason: str = "") -> tuple[overlay.Row, ...]:
    """*roster*'s rows for the top-level list: noted with *reason*, or with the KIND.

    **The kind is what tells `zeb` the persona from `zeb-api` the workspace** when both
    are answers to one query. Inside a picker the heading already says which noun the pane
    is showing and every row is that noun, so the label there would be forty copies of a
    word the header carries once — which is why :func:`roster` leaves the note empty and
    this is a second function rather than a field on the first.

    **The kind is a LABEL and never a search term.** It goes in the note, which
    `frame/palette.matches` deliberately does not match, so typing `persona` finds the
    doorway row that says so and not every persona the plane has. That is the same
    decision that keeps `lock` from listing every action whose reason mentions one.

    **A *reason* displaces it, and that inverts :func:`roster`'s own objection rather than
    contradicting it.** That function refuses to repeat the launch pin on every row because
    a pinned frame's picker cannot be opened at all — the doorway stops one keypress
    earlier — so the line would be one no test could go red without. Typing does not go
    through the doorway, so on THIS path the pinned rows are reachable, the reason is live,
    and `frame/palette.py`'s first rule applies unchanged: an unavailable row is listed
    *with its reason*, because an operator cannot ask about an option they cannot see
    (#512). The reason names `$CHARTER_WORKSPACE` or `$CHARTER_PERSONA`, so the kind is
    still on the row; it is a sentence instead of a word.

    ``_replace`` rather than a fresh `overlay.Row`, so the id is carried over untouched:
    it is what :meth:`Roster.name_of` matches on, and a labelled row that minted its own
    would be a row that stands for no name.
    """
    return tuple(r._replace(note=reason or roster.noun) for r in roster.rows)


def open_rows(fid: str) -> tuple[overlay.Row, ...]:
    """The two rows the palette carries: one per noun, each opening its picker.

    **They are rows and not actions, and the difference is honest rather than
    bureaucratic.** `frame/action.py`'s contract is *fire-and-report* — ``run`` starts
    work and returns, and `ActionRegistry.invoke` never waits for it — and opening a
    picker starts nothing at all: it replaces the surface in the pane the operator is
    already looking at. An `Action` whose ``run`` did nothing would pass every test in
    that contract and describe nothing that happens.

    The title names the name in use, so the palette still answers "which workspace am I
    on" without opening anything, and so typing `alpha` still finds the row when `alpha`
    is where the frame is. The note is the pin reason, which is what makes a pinned frame
    say so **before** a keypress — Task 4's rule, unchanged, one row instead of forty.
    """
    out = []
    for noun in NOUNS:
        now = current(noun, fid)
        out.append(overlay.Row(
            id=OPEN_ID.format(noun),
            title=(f"{noun}: {now} — pick another" if now else f"{noun} — pick one"),
            note=pin_reason(noun, fid)))
    return tuple(out)


def noun_of(row: overlay.Row) -> str | None:
    """Which picker *row* opens, or ``None`` when it opens none.

    How `commands_frame._draw_palette` tells a navigation row from an action row. Matched
    against the two ids this module mints rather than by splitting on `:`, so a row id
    that merely contains a colon cannot be read as an instruction.
    """
    for noun in NOUNS:
        if row.id == OPEN_ID.format(noun):
            return noun
    return None


def switch_to(noun: str, fid: str, name: str) -> switch.Outcome:
    """Move frame *fid* to *name*, and answer with what happened and what to say.

    One call into `frame/switch.py` and nothing else, which is what makes "the switch
    moves the frame's own identity and bumps it" (#411/#412) a property of one function
    rather than of every surface that switches. The pointer under the frame's id, the
    frame's recorded workspace, the gather cache and the bump are all that function's, in
    that order, and a picker that wrote any of them itself would be the second answer #411
    is about.
    """
    if noun == WORKSPACE:
        return switch.to_workspace(fid, name)
    return switch.to_persona(fid, name)
