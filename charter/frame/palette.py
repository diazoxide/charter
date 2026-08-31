"""The palette: every action, narrowed by typing, run by Enter.

**`F2` is the palette, and the menu no longer exists** (§4h). Not a new key beside the
old one — the menu was always trying to be a palette, and keeping both would leave two
answers to "how do I do a thing", which is how the single menu became weird in the first
place. `display-menu`, `charter frame-menu`, `charter frame-action` and the recorded
`actions.json` table they resolved through are gone rather than deprecated.

**This is `frame/overlay.py` with a filter on it, not a second surface.** Everything the
overlay already decided — full-pane rather than `display-popup`, modal, one key that
always leaves, a click that only ever selects, containment before width arithmetic —
holds here unchanged, because :class:`Palette` IS an `overlay.Surface`. What this module
adds is exactly two things: rows built from `frame/actions.py`'s offers, and a query the
operator types.

**The rows are ACTIONS. The palette invents no command list of its own.** :func:`rows` is
the whole of the seam, and it is deliberately a function from offers to
`overlay.Row` rather than anything cleverer: Task 6's workspace and persona pickers are
the same :class:`Palette` over a different row source, and a picker that had to subclass
something would be a second surface wearing this one's name.

**A NAME is not an action either, and it reaches this list without becoming one.** Task 6
removed forty workspace `Action`s and was right to — an action's contract is
fire-and-report, and forty of them meant forty `run`s each starting a second charter
process — but the doorway it left cost the operator a keypress on the thing they do most.
:attr:`Palette.query_only` is the seam that gives it back: rows the FILTER reaches which
browsing does not, gathered the first time something is typed and never while the query is
empty. Still rows, still no `Action`, and still nothing spawned until Enter.

**Four rules the plan states and a first implementation loses.**

*An unavailable action is listed WITH ITS REASON.* `frame/actions.py` already refuses to
produce a reasonless unavailable offer; this module refuses to drop the row. The session
lock and `$CHARTER_WORKSPACE` refuse a workspace switch today, and an operator cannot ask
about an option they cannot see — that is #512's "no repos" drawn over a plane that had
them, one surface along.

*No row cap.* charter's nine-row limit was charter's, not tmux's (§2 blamed tmux for it;
tmux 3.1c drew 20 rows fine). The overlay scrolls, so a cap would only hide rows.

*Filtering is case-insensitive.* An operator typing `detach` must find `Detach`. Both
sides are `casefold`ed rather than `lower`ed, because `lower` gets Turkish dotless-i and
German sharp-s wrong and a palette that cannot find a row is indistinguishable from one
that does not have it.

*The name you typed in FULL is the row Enter runs* (#732). Everything else keeps the
position the catalogue gave it — see :func:`narrow`, which states the whole ordering rule
and why the two things it does not do (score, cap) are still not done.

**The query never reaches a parser.** It is built one keypress at a time out of
`overlay.decode`'s printable single-character events, so no newline and no escape
sequence can enter it; it is nevertheless contained before it is measured or drawn, for
the reason every other display string here is (#472) — the guard belongs at the join, not
at whichever writer happens to exist today.
"""

from __future__ import annotations

import os
import select
import sys
import termios
import tty
from dataclasses import dataclass, field
from typing import Callable, Iterable

from .. import contain
from . import component, overlay, pane

#: What the header says the palette is, before the operator types anything.
HEADING = "charter"

#: What separates the heading from what has been typed. A slash, `vi`'s own search
#: prompt, rather than a `>`: `_MARK`'s selected row already starts with `> `, and two
#: different `>`s a line apart is a header an operator reads as a row.
PROMPT = " /"

#: How long :func:`own_the_tty`'s reader waits before answering "nothing arrived".
#:
#: That answer is not idleness — it is what resolves a lone `\x1b` into an Escape
#: keypress (`overlay.decode`'s *final*), and it is what makes a `window-resized` repaint
#: without a keystroke, because `Surface.run` asks for the pane's size on every
#: iteration. So it is a repaint clock, and 0.2 s is chosen the way `frame/notify.py`
#: chooses its own: fast enough that Escape does not feel stuck, slow enough that an idle
#: palette is not a poll loop.
TICK = 0.2

#: How much of the tty is read at once. One `read` never has to return a whole sequence —
#: `overlay.decode` holds a partial one back — so this only bounds how many events one
#: iteration can carry.
_CHUNK = 4096


def rows(offers: Iterable) -> tuple[overlay.Row, ...]:
    """Every offer as one row. **The row source, and the seam Task 6 reuses.**

    One row per offer, in the order `ActionRegistry.offers` gave them, with nothing
    dropped and nothing capped. An unavailable offer's reason becomes the row's note —
    `frame/actions.py` guarantees that string is non-empty exactly when the offer is
    unavailable, so "listed with its reason" and "listed at all" are the same decision
    here rather than two.

    **`available` and not `reason` is what becomes `Row.refused`**, even though
    `frame/actions.py` guarantees the two agree. The offer holds both because they are two
    facts, and a row that re-derived one from the other would be the surface where they
    could come apart: an offer available with a reason on it, or refused with an empty
    one, would draw as its opposite here rather than as the defect it is one module down.

    A `tuple`, so what a `Palette` was built with cannot be edited underneath it by
    whoever passed it in.
    """
    return tuple(overlay.Row(id=o.id, title=o.title, note=o.reason,
                             mark=o.mark, refused=not o.available)
                 for o in offers)


def matches(query: str, row: overlay.Row) -> bool:
    """Whether *row* survives *query* — a case-insensitive substring of what is READABLE.

    **Title and id, never the note.** The title is what the operator sees and the id is
    what a provider's documentation calls the thing (`acme.deploy`), so both are things
    somebody types on purpose. The note is charter's own sentence about why a row cannot
    run, and matching it would make typing `lock` list every action that merely mentions
    one — a filter that answers a question nobody asked.

    **And the id only when it is an ACTION id**, which is the same sentence read
    carefully rather than a new rule: an id earns its place in the filter by being the
    name a provider's documentation gives the thing. `frame/choose.py`'s row ids are not
    that — they are charter's own counter, `workspace:n7`, never drawn and never typed —
    and now that name rows sit in this list, matching them blindly would make `n` list
    every name on the plane and `persona` list every persona. `component.usable_id` is
    the question `frame/action.py` already asks of an action id, asked here rather than
    re-spelled, so "can be an action id" and "is matched as one" cannot drift apart.

    `casefold` rather than `lower` — see :func:`typed`, which is where the whole of the
    case rule now lives, because :func:`exact` asks the identical question of the
    identical string and two spellings of it is two places for it to be wrong.

    **The id is not folded, and the guard in front of it is why.** `component.usable_id`
    holds an id to `^[a-z][a-z0-9_]{0,31}(\.[a-z][a-z0-9_]{0,31})?$` — measured: every
    spelling with a capital in it answers False — so an id that reached the comparison
    cannot carry case, and `casefold` on it is a claim that it can. The deletion sweep
    reported that call as a line nothing could go red without, and it was right. What the
    removal depends on is the guard being asked FIRST, which
    `tests/test_frame_palette.py` pins as a property of the pair rather than of the order
    they happen to be written in today.
    """
    q = typed(query)
    if q in row.title.casefold():
        return True
    return component.usable_id(row.id) and q in row.id


def typed(query: str) -> str:
    """What the operator typed, contained and case-folded — the one normalisation.

    **One function because two callers ask the identical question.** :func:`matches`
    decides which rows survive and :func:`exact` decides which of the survivors goes
    first, and a palette that filtered case-insensitively and then ranked
    case-sensitively would find the row and put the cursor somewhere else, which an
    operator cannot tell from not finding it. Spelled twice, that is two lines to keep in
    step; spelled once, it is one line with both callers' tests behind it — which is also
    what the deletion sweep said about the copy, reporting the second `casefold` as a line
    nothing could go red without.

    `casefold` and not `lower`: `lower` gets Turkish dotless-i and German sharp-s wrong.

    Contained for the reason every display string here is (#472) — the guard belongs at
    the join rather than at whichever writer happens to exist today. The query cannot hold
    a newline by the route it is built (`Palette.handle` takes printable single
    characters), and this function is also reachable from a caller that did not build it
    that way.
    """
    return contain.one_line(query).casefold()


def exact(query: str, row: overlay.Row) -> bool:
    """Whether *query* IS this row's name, rather than a piece of it.

    The same two strings :func:`matches` looks at and the same `casefold` on both sides,
    so a row can never be exact without also being a match — the ordering below is a
    refinement of the filter and not a second, disagreeing opinion about what the operator
    typed.

    **The title is the name now**, which is what makes this askable at all. Until #749 a
    picker's row arrived here as `"* alpha"` — two characters of mark composed into the
    title by whoever built the row — so "is this the name they typed" would have been a
    prefix-strip against a constant, in a function that would then have to know which row
    sources mark and which do not. The mark is `overlay.Row.mark` and the title is `alpha`.

    **An empty query is not a name**, and the guard is here rather than in :func:`narrow`
    because that is where it is true: nobody has typed anything, so nothing can be what
    they typed. Without it a row with an empty title would be "exact" for `F2` with
    nothing typed and would sort to the top of the unfiltered palette — which is the one
    thing the two-bucket sort promises cannot happen.
    """
    q = typed(query)
    if not q:
        return False
    if q == row.title.casefold():
        return True
    return component.usable_id(row.id) and q == row.id


def narrow(catalogue: Iterable[overlay.Row], query: str) -> tuple[overlay.Row, ...]:
    """*catalogue*, keeping the rows :func:`matches` keeps, **exact names first**.

    No fuzzy scoring and no cap, and the order is disturbed in exactly one way, stated as
    a rule:

        **An exact match comes first, an actionable one ahead of a refused one, and
        everything else keeps the position it already had.**

    Two buckets, and a stable sort, so *within* each the catalogue's own order is
    untouched: the doorways in `choose.NOUNS` order, then every action in registration
    order, then the names (`Palette._reachable` decides that much). A plane with forty
    workspaces still cannot bury `detach` under the ones whose names contain a `d`,
    because none of those forty is `detach`.

    **What this fixes is #732, and it was a real two-keystroke route failing on the most
    obvious input there is.** `docs/frame.md` sells `F2` + the name + Enter as the way to
    switch when you know where you are going. A chat id is `<workspace>.<n>`, so on a
    plane with a workspace `alpha` the row `chat: alpha.1 — pick another` holds `alpha` as
    a substring of its TITLE, and it sorted above the `alpha` workspace row because a
    doorway is in the catalogue and a name is not. Worse, that doorway was refused — one
    chat, nothing to pick — so Enter opened nothing, switched nothing, and put a sentence
    about chats on the screen of an operator who had typed a workspace's name.

    **A refused row is still listed, and it is still listed with its reason.** That is
    #512's rule and this does not touch it: an operator cannot ask about an option they
    cannot see, and hiding the pinned workspace or the doorway that cannot open would be
    the defect that rule exists to prevent. What moves is only which row Enter lands on
    first, and only among rows the operator has typed the *whole* name of.

    **The "no ranking" this replaces defended itself with a property this class does not
    have.** It argued that reordering "would move the row under the cursor out from under
    it between keystrokes" — but `Palette._refilter` sets the selection back to the top on
    every single edit, deliberately and with its own docstring saying so. There was no row
    under the cursor to move. The real cost of reordering is that the LIST reads
    differently, which is why the sort is two buckets rather than a score: with nothing
    typed, no row is an exact match for `""`, so `F2` draws exactly the list it drew
    before this function learned to sort.
    """
    kept = [r for r in catalogue if matches(query, r)]
    return tuple(sorted(kept, key=lambda r: (0, r.refused) if exact(query, r) else (1, 0)))


@dataclass
class Palette(overlay.Surface):
    """An `overlay.Surface` whose rows narrow as the operator types.

    Everything modal, every key that is not text, the scrolling window, the containment
    and the escape hatch are the base class's and are not restated here — see
    `frame/overlay.py`. This adds a query and the two keys that edit one.

    :attr:`rows` is derived and must never be assigned from outside: it is
    :attr:`catalogue` narrowed by :attr:`query`, recomputed on every edit, so the list on
    screen and the list Enter chooses from cannot come apart.
    """

    #: Every row this palette could ever show, before filtering. The full offer list.
    catalogue: tuple[overlay.Row, ...] = ()

    #: What the operator has typed. Empty is "show everything", not "show nothing".
    query: str = ""

    #: What the header says before the query is appended. Held apart from
    #: `Surface.heading`, which this class rewrites on every keystroke.
    label: str = HEADING

    #: Rows a QUERY reaches and browsing does not. Called with no arguments the first time
    #: :attr:`query` is non-empty, and never while it is empty. ``None`` — the default,
    #: and what every picker passes — is "there are none".
    #:
    #: **The laziness is the whole reason this is a callable and not a second tuple.**
    #: `frame/choose.py`'s names are directory listings off the plane: `switch.workspaces`
    #: globs `workspaces/` and `switch.personas` globs `personas/`, once each per read. An
    #: operator who opens the palette to press `detach` has asked no question about names,
    #: and a catalogue built eagerly would enumerate forty workspaces to answer it. Passing
    #: rows here instead of a function would have done that work at the call site, which is
    #: the same cost one line earlier.
    query_only: Callable[[], tuple[overlay.Row, ...]] | None = None

    #: What :attr:`query_only` answered, kept so it is asked ONCE per palette rather than
    #: once per keystroke — see :meth:`_reachable`. ``None`` is "not asked yet", which is
    #: what makes that distinguishable from a plane that genuinely has no names.
    _found: tuple[overlay.Row, ...] | None = field(default=None, init=False)

    #: What the last repeated row ANSWERED, appended to the heading until the next
    #: keystroke — see :meth:`report`. Held apart from :attr:`heading` because the heading
    #: is derived and is rebuilt from scratch on every edit; a sentence written straight
    #: into it would either be lost at the next keystroke or, worse, kept and appended to
    #: again, growing by one clause per Enter.
    said: str = ""

    def __post_init__(self) -> None:
        self._refilter()

    def _reachable(self) -> tuple[overlay.Row, ...]:
        """Everything :func:`narrow` may keep right now: the catalogue, and — once
        something has been typed — :attr:`query_only`'s rows after it.

        **Empty query, catalogue only, and that is a cost promise rather than a display
        one.** It is what makes `F2` on a plane with forty workspaces read the same three
        state files it read before this attribute existed. A version that gathered eagerly
        and merely hid the rows would draw identically and be the thing this refuses.

        **Asked once, not once per keystroke.** The gather is memoised in :attr:`_found`,
        so `beta` costs one directory read rather than four, and backspacing to nothing and
        typing again costs none. The names are therefore resolved at the FIRST keystroke
        rather than when the palette opened, which is the same freshness the doorway path
        has — `commands_frame._roster` hands both paths the one roster — and a palette
        lives for as long as somebody is looking at it.

        **After the catalogue, never before.** `narrow` reorders only exact matches, so this
        is where the order of the two GROUPS is decided: every action and both doorways
        keep the position they had before names existed, and a plane with forty workspaces
        cannot bury `detach` under the ones whose names happen to contain a `d`. A name
        the operator has typed in full goes first wherever it was gathered — that is
        #732, and it is a decision about one row rather than about these two blocks.
        """
        if not self.query or self.query_only is None:
            return self.catalogue
        if self._found is None:
            self._found = tuple(self.query_only())
        return self.catalogue + self._found

    def _refilter(self) -> None:
        """Recompute the visible rows, the header, and where the cursor sits.

        The selection goes back to the top on every edit, deliberately: after a keystroke
        the operator is looking at a different list, and keeping an index into the old one
        would leave the cursor on whichever row happened to land in that position.
        `_top` follows it because `Surface._window` recomputes from the selection, so
        there is no second scroll state to keep in step.
        """
        self.rows = narrow(self._reachable(), self.query)
        self._sel = 0
        self._top = 0
        self.said = ""
        self._headline()

    def _headline(self) -> None:
        """Compose :attr:`heading` from the three things that make it up.

        One place, called by :meth:`_refilter` and by :meth:`report`, so that "what the
        header says" cannot be assembled two ways — a second spelling is how a repeated
        action's sentence would come to be appended to a heading that already had one.
        """
        self.heading = (self.label + (PROMPT + self.query if self.query else "")
                        + (f" · {contain.one_line(self.said)}" if self.said else ""))

    def report(self, said: str) -> None:
        """Say *said* in the header, leaving the query and the cursor exactly as they are.

        **What a repeatable row answers with, and the only surface it can answer on**
        (#746). `frame/overlay.modal_argvs` zooms this pane over the whole window, and its
        own measurement is that a zoomed pane's siblings are not drawn — so an action that
        moves the repo table's selection while the palette is up moves a highlight that is
        not on screen. The header is what the operator is looking at.

        **Not `_refilter`**, which is the whole reason this is a method rather than a
        caller writing `heading`. That function resets the selection to the top by design,
        and a repeat that reset it would move the cursor off the row the operator is
        pressing Enter on — one repeat, and then a re-filter and a re-aim for the next.
        Nothing here touches :attr:`rows`, :attr:`_sel`, :attr:`_top` or :attr:`query`.

        An empty *said* clears rather than keeps: an action that answered nothing has
        nothing to report, and leaving the previous sentence up would attribute it to the
        keypress that just happened.
        """
        self.said = said
        self._headline()

    def handle(self, ev: overlay.Event, height: int) -> str | None:
        """One event: text edits the query, everything else is the overlay's.

        **A key is text when it is one printable character**, which is exactly what
        `overlay.decode` emits for a printable byte and never what it emits for a named
        key — every name that surface has (`up`, `enter`, `escape`, `pgdn`, …) is longer
        than one character. Testing the shape rather than excluding a list of names means
        a key added to `decode` tomorrow cannot silently start typing itself into the
        query.

        Backspace on an empty query is a no-op rather than a cancel. Escape is the one
        input that means "leave now" and it must be the only one: an operator who typed
        four characters and pressed backspace five times has not asked to close the
        palette.
        """
        if ev.kind == overlay.KEY:
            if ev.name == "backspace":
                if self.query:
                    self.query = self.query[:-1]
                    self._refilter()
                return None
            if len(ev.name) == 1 and ev.name.isprintable():
                self.query += ev.name
                self._refilter()
                return None
        return super().handle(ev, height)


def own_the_tty(surface: overlay.Surface, *, fd: int | None = None, out=None,
                then: Callable[[overlay.Row], overlay.Surface | None] | None = None
                ) -> overlay.Row | None:
    """Put this process's own tty in raw mode and let *surface* have it until it is done.

    **Raw mode is what makes the surface modal at all.** Without it the terminal line
    discipline holds every keystroke until Enter, echoes it where the overlay is drawing,
    and turns Ctrl-C into a signal the surface never sees — `overlay.decode` reads
    `\\x03` as "leave", which is only true once nothing else is going to turn it into a
    signal first.

    The restore is a ``finally`` for `Surface.run`'s reason one layer down: a surface
    that raised must still hand the tty back, or the operator is left in a terminal that
    looks broken and takes a `reset` to fix.

    *read* answers `b""` when nothing arrived within :data:`TICK` — which is not idleness
    but the tick `overlay.decode` needs to resolve a lone Escape, and the clock a resize
    repaints on — and `None` at end of input, which `Surface.run` treats as a cancel.
    That is the one answer that can never become a wedge.

    No fallback anywhere in *size*: a process whose stdout is not a terminal never
    reaches this function, because `tcgetattr` on the line above has already refused. A
    default size here would be a rectangle charter invented for a pane it could not
    measure, which is `frame/slots.py`'s measured 22-column pane reporting 200 columns,
    one surface over.

    ---

    **`out` defaults to the pane this process was CLAIMED, never to `sys.stdout`** — #611,
    which is #606's property reaching the other pane-owning process. This name is not an
    output stream here, it is the rectangle: it is what the surface paints into AND the
    descriptor `size` measures, so the two cannot be allowed to come from a mutable global
    that any library the process imported may have replaced. `commands_frame.cmd_palette`
    claims above `builtin_actions.build`, which is where a provider's module first gets to
    run (Textual's `redirect_stdout`, a `rich` console, `colorama`, a logging handler
    installed at import) — measured on `main` before the claim: every byte of one whole
    palette, the alternate-screen enter included, went into a `_PrintCapture` and the pane
    got nothing at all, while `size` asked `os.get_terminal_size(-1)` and raised into a
    pane `_close_palette` was already killing. `frame/pane.py` carries the argument.

    `pane.stream()` and not `pane.size()`: this function measures the descriptor it is
    painting into, whichever stream that is, so an explicit *out* (every test with a real
    pty passes one) is measured as itself rather than against a claim nobody took. The
    fallback inside `stream()` is `sys.stdout` for a process that claimed no pane, which
    is the same answer this parameter defaulted to before and is why no existing caller
    moves.

    ---

    **`then` is how one pane holds more than one surface**, and it is Task 6's whole
    mechanism: a chosen row is offered to it, and a `Surface` coming back is run next, in
    this same pane, without the tty leaving raw mode in between. `None` — the default, and
    what `then` answers for every row that is not a doorway — ends the loop and returns
    the row, exactly as before.

    That is what a picker is (`frame/choose.py`): the palette's own pane, redrawn from a
    different row source. The alternative shape, a palette row that starts a second
    charter to split a second overlay pane, races the palette's own teardown —
    `commands_frame._close_palette` selects the harness, kills this pane and re-arms the
    escape hatch as ONE chained tmux command the instant a row has been chosen, and a
    second pane that had already selected and zoomed itself would be undone by it.

    `then` is charter's own code and answers `None` for every row a picker offers, so the
    loop is a two-level tree rather than an open one. It is deliberately not bounded by a
    count: a number here would be a limit on how deep a surface may nest, invented to
    guard against charter's own code being wrong, and the honest answer to a surface that
    will not end is the one `frame/overlay.py` already gives — `HATCH_KEY`, matched by
    tmux's own root key table before any byte reaches this process.
    """
    fd = sys.stdin.fileno() if fd is None else fd
    out = pane.stream() if out is None else out
    before = termios.tcgetattr(fd)
    tty.setraw(fd)
    try:
        read, write = _reader(fd), _writer(out)
        def size() -> tuple[int, int]:
            return tuple(os.get_terminal_size(out.fileno()))
        while True:
            chosen = surface.run(read=read, write=write, size=size)
            nxt = then(chosen) if (then is not None and chosen is not None) else None
            if nxt is None:
                return chosen
            surface = nxt
    finally:
        termios.tcsetattr(fd, termios.TCSADRAIN, before)


def _reader(fd: int) -> Callable[[], bytes | None]:
    """A `read` for `Surface.run`: bytes, `b""` for a tick, `None` for end of input.

    The `select` is what turns a blocking `read` into a poll with a deadline, and the
    empty answer it produces is load-bearing rather than an optimisation — see
    :data:`TICK`. A zero-length `os.read` on a tty is end of input and nothing else: the
    pane's writer is gone, so waiting for more would be waiting forever, and
    `Surface.run` cancels rather than spinning.

    **"The other end is gone" has two spellings and this platform is not the one that
    decides which.** Measured, on the same test, in the same commit: closing a pty's
    other end makes the next read return `b""` on macOS and raise
    ``OSError: [Errno 5] Input/output error`` on Linux — CI turned red on 3.11 and 3.14
    for exactly this, against a suite that was green twice over on the machine it was
    written on. Both are end of input, both mean cancel, and answering them differently
    would have made the palette raise out of its own loop on every Linux operator's
    machine while looking correct on the author's.
    """
    def read() -> bytes | None:
        if not select.select([fd], [], [], TICK)[0]:
            return b""
        try:
            chunk = os.read(fd, _CHUNK)
        except OSError:
            return None
        return chunk if chunk else None
    return read


def _writer(out) -> Callable[[str], None]:
    """A `write` for `Surface.run`, flushed every call.

    One paint is one `write` (`Surface._paint`), so a flush per call is a flush per paint
    — and an unflushed paint is a pane that is blank until the next keystroke, which
    looks exactly like the wedge the escape hatch exists for.
    """
    def write(s: str) -> None:
        out.write(s)
        out.flush()
    return write
