"""Choosing a workspace before the harness starts — #518.

`charter <harness>` resolved one silently and went straight into the frame; choosing
where to work meant quitting, running `charter workspace use <name>`, and launching
again. This is the prompt that replaces that round trip.

**Where it lives, and why it is not a tmux menu.** #518 offered two homes: the
`layout.PLACEHOLDER` moment inside the frame, or a plain terminal prompt before tmux
takes over. This is the second, for a reason the first cannot answer: a picker must be
able to say *no*, and cancelling inside the frame means tearing down a session, a
harness pane and every hook that was armed for it — a launch that half happened. Before
tmux, cancelling is a `return`. The cost is the honest one #518 names: it looks like
charter's CLI rather than like the frame. It is charter's CLI — nothing has been drawn
yet.

**Nothing here touches the plane.** This module renders and reads; it returns a
:class:`Choice`, and `commands_frame.cmd_launch` is what creates, switches and launches.
That split is what makes "a cancelled picker creates nothing" a property of the code
rather than a promise: there is no create call in this file to reach.

**Input and output are injected**, not `input()` and `print()`. A picker is exactly the
shape of test this repo keeps getting wrong — one that reads the developer's machine
instead of the repo — and a test that has to own a tty to check that `q` cancels would
be testing tmux, pty allocation and the terminal, none of which is the property. `read`
returns one line or ``None`` for end-of-input; `write` takes finished text.

**Names are contained before they are measured.** A workspace name is a committed value
(#472 was filed because a table sized its columns from a raw name), so `contain.one_line`
runs first and `tui.width` — never `len` — does the arithmetic afterwards.
"""

from __future__ import annotations

from typing import Callable, NamedTuple

from .. import contain, tui

#: The three things the operator can decide. ``USE`` and ``CREATE`` carry a name that has
#: already passed `workspace.valid_name`; ``CANCEL`` carries none and means the launch
#: must stop having done nothing at all.
USE, CREATE, CANCEL = "use", "create", "cancel"

#: How many unusable answers in a row before the picker gives up and cancels. A tty
#: reaches end-of-input and the loop ends on ``None``; this is for the stream that
#: neither answers nor ends, where an unbounded loop would be a hang with no message —
#: the failure #518 says a picker must never be.
_MAX_TRIES = 20

#: Marker for the workspace the launch would have resolved on its own. `_DENSITY_MARK`'s
#: `*` for `_DENSITY_MARK`'s reason: `●`/`◆` are East-Asian *Ambiguous* and
#: `statusline._persona_chips` records them breaking this layout twice.
_MARK = ("*", " ")

_R, _DIM, _BOLD = "\033[0m", "\033[2m", "\033[1m"


class Choice(NamedTuple):
    """What the operator decided. ``name`` is empty for :data:`CANCEL`."""

    action: str
    name: str = ""


class Row(NamedTuple):
    """One workspace as the picker shows it: its name, and how many clones it holds.

    The count is what makes the list worth reading — #512's own note on #518 is that a
    freshly picked workspace draws an empty repo table until the first gather, so the
    number of clones is the only thing on screen at pick time that says which of these
    is the one with the work in it. It is one `iterdir` per workspace, paid once, before
    tmux starts — not on any repaint path, so the idle-cost property is untouched.
    """

    name: str
    clones: int


def rows(names: list[str], count: Callable[[str], int]) -> list[Row]:
    """*names* with their clone counts. ``count`` is injected so the renderer can be
    exercised without a plane under it."""
    return [Row(n, count(n)) for n in names]


def render(rows_: list[Row], current: str, width: int) -> str:
    """The list, as the operator sees it.

    Two columns, sized from the CONTAINED names (see the module docstring), with the
    whole line truncated to *width* so a plane with a very long workspace name wraps
    nothing. `tui.pad` and `tui.truncate` do the arithmetic; `len` appears nowhere.
    """
    on, off = _MARK
    shown = [(contain.one_line(r.name), r) for r in rows_]
    name_w = max([tui.width(n) for n, _ in shown] or [0])
    out = [f"\n  {_BOLD}charter{_R}{_DIM} · which workspace?{_R}\n"]
    for i, (name, r) in enumerate(shown, start=1):
        mark = on if r.name == current else off
        repos = "—" if not r.clones else f"{r.clones} repo" + ("s" if r.clones > 1 else "")
        line = (f"    {_DIM}{i:>2}{_R}  {mark} {tui.pad(name, name_w)}  "
                f"{_DIM}{repos}{_R}")
        out.append(tui.truncate(line, width))
    out.append(f"\n    {_DIM} n{_R}    create a new workspace")
    out.append(f"    {_DIM} q{_R}    cancel — start nothing\n\n")
    return "\n".join(out)


def ask(rows_: list[Row], current: str, *, read: Callable[[], str | None],
        write: Callable[[str], None], name_ok: Callable[[str], bool],
        width: int = 80) -> Choice:
    """Draw the list, read one decision, and answer with it. Creates nothing.

    Refusals loop back to the list rather than ending the launch: a mistyped number is a
    mistype, not a decision, and #518's whole complaint is about being given no choice.
    End-of-input (`read` returning ``None``) is :data:`CANCEL` — that is what a closed
    stdin means, and it is the one answer that can never be a hang.

    An empty line takes *current*, which is the row already marked. That is the "just get
    on with it" key, and it is the same answer the launch would have given with no picker
    at all — so an operator who does not care pays one keystroke, not a decision.
    """
    write(render(rows_, current, width))
    names = [r.name for r in rows_]
    for _ in range(_MAX_TRIES):
        write(f"  workspace {_DIM}[{contain.one_line(current)}]{_R}: ")
        line = read()
        if line is None:
            write("\n")
            return Choice(CANCEL)
        s = line.strip()
        if not s:
            return Choice(USE, current)
        if s in ("q", "Q"):
            return Choice(CANCEL)
        if s in ("n", "N"):
            got = _ask_new(names, read=read, write=write, name_ok=name_ok)
            if got is not None:
                return got
            write(render(rows_, current, width))
            continue
        if s.isdigit() and 1 <= int(s) <= len(names):
            return Choice(USE, names[int(s) - 1])
        # The name itself is accepted too — an operator who knows where they are going
        # should not have to find its row number. Checked against the LIST, never against
        # `name_ok` alone: a shape-valid name that is not there is a typo, and creating on
        # a typo is exactly the litter #518 refuses.
        if s in names:
            return Choice(USE, s)
        write(f"  {_DIM}not one of these — a number, a name, n to create, q to cancel{_R}\n")
    return Choice(CANCEL)


def _ask_new(names: list[str], *, read, write, name_ok) -> Choice | None:
    """The create branch. ``None`` means "back to the list", having created nothing.

    Three gates before a :data:`CREATE` ever comes back, which is #518's "creating is not
    free" in code:

    * **empty** — the operator changed their mind at the prompt. Back to the list.
    * **the name must pass `workspace.valid_name`** (injected as *name_ok*, so this module
      never has to import `workspace` and the one rule stays `instance.workspace_name_ok`).
      A refusal names the alphabet rather than only saying no.
    * **an existing name is a USE, not a CREATE** — asking to create something that is
      already there is not an error and must not be a second `workspace.ensure`; it is
      the operator naming a workspace the long way round.

    Then an explicit `y`. Anything else — including a bare Enter — is back to the list.
    A default of no is the whole point: the fat-fingered path has to be the one that
    creates nothing.
    """
    write(f"  new workspace name {_DIM}(letters, digits, . _ -){_R}: ")
    raw = read()
    if raw is None:
        write("\n")
        return Choice(CANCEL)
    name = raw.strip()
    if not name:
        return None
    if not name_ok(name):
        write(f"  {_DIM}'{contain.one_line(name)}' is not a workspace name — letters and "
              f"digits, then . _ -{_R}\n")
        return None
    if name in names:
        return Choice(USE, name)
    write(f"  create {_BOLD}{contain.one_line(name)}{_R} and switch to it? "
          f"{_DIM}[y/N]{_R}: ")
    ans = read()
    if ans is None:
        write("\n")
        return Choice(CANCEL)
    if ans.strip().lower() not in ("y", "yes"):
        return None
    return Choice(CREATE, name)
