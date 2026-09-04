"""The frame's shape, decided before tmux is involved at all.

Pure on purpose. Everything that decides *what the frame looks like* lives here and
returns plain lists of strings, so the whole shape is under test on a machine with no
tmux, and so the argv rule below is enforced mechanically instead of by review.

The one thing this module reads from outside its own arguments is the running
interpreter's path, via `util.self_relaunch_argv()` in `panel_command` — no process is
started, nothing is measured, and the result is as deterministic as any other string
here. It is read HERE rather than passed in because the two callers that start a panel
otherwise each hand in their own copy of that argv, and one of them promptly got it
wrong (#390's `-P`, missing from the respawn path — see `panel_command`'s own
docstring). Same reasoning as the never-join-argv rule below: the property is worth more
enforced by construction than remembered at every call site.

**Nothing here ever joins argv.** Pinned against tmux 3.7c: `new-session … printf
'hello;touch INJ'` passed as separate arguments creates no file, and the same text as one
string creates it. Workspace, repo, branch and persona names all reach a frame from
committed files or `.git/HEAD`, so a joined string would be the `gh -F` bug again.

**Every split targets the harness pane's id — never a `session:0.0`-style index.**
Measured against tmux 3.7c, and the reason this module has two entry points instead of
one `plan()` that emits every command up front: tmux renumbers pane INDICES on every
split. Starting a session whose only pane is the harness (`%0`, index 0) and running
`split-window -t session:0.0 -v -b -l 1` leaves the layout as index 0 = the new one-row
panel (`%1`) and index 1 = the harness (`%0`) — the split moved, the harness didn't, and
now sits at the index the split just vacated. A second split still targeting
`session:0.0` therefore divides the FIRST PANEL, not the harness, and once that panel is
down to a single row tmux refuses outright: `size or position no space for a new pane`.
The bottom panel is never built. Silently — nothing downstream currently checks a split's
return code — so a four-slot frame ships with one panel and no error.

Pane IDs don't have this problem: tmux never reuses or renumbers a pane's `%N` id for its
lifetime, index churn or not. `session_argv` asks tmux to print the harness's id at
creation time (`-P -F '#{pane_id}'`); the caller reads it off stdout and hands it to
`panel_argvs`, which targets it — and only it — for every split.
"""

from __future__ import annotations

from typing import NamedTuple

from . import builtins as _builtins, tmuxctl
from .component import Fixed
from .. import util

#: The order slots are dropped in as the terminal shrinks. The side first — a side panel
#: costs the harness columns, so it goes as soon as space is tight in EITHER dimension,
#: not only when columns themselves are the short one — then `repos`, whose table cannot
#: be drawn in a pane narrower than `statusline._LEFT_W` or shorter than
#: :func:`_table_min_rows`, then the two bars, then the top, whose row is worth less than
#: the status strip's alerts and more than everything above it.
#:
#: **The rule the tail of this list encodes: a rung is dropped when the pane it would get
#: cannot carry the thing the rung exists for, and among rungs that can, the ones whose
#: facts another surface already reaches go first** (#740). That is what puts `top` last:
#: it is one row carrying two facts nothing else on a short terminal says — which
#: workspace you are in and which persona you are being — while `repos` spends a border
#: and a heading before its first repo row, and the bars are reminders `F2` replaces in
#: two keystrokes. `bottom` is past the end of the list for the same reason, one step
#: further.
#:
#: **`bottom` is not here, and it is the one slot that never is.** It is the attention
#: strip — the one alert and the command that fixes it — which is the whole reason a
#: frame is worth drawing at all on a terminal too small for anything else.
#:
#: **`left` is not here any more, and #488 is why.** It drew repo rows recomposed for a
#: 22-column pane; `repos` now draws the same rows as the full-width table the status
#: line draws, so the sidebar's only remaining job was a lesser copy of its neighbour's.
#: Retiring it hands those 22 columns back to the harness at every density.
#:
#: **Phase 5's two bars go above `top`, and they are here even though charter does not
#: place either of them** (`frame/builtins.py` says why). This list is a filter over
#: whatever a plane's arrangement holds, so an entry for a component nobody placed costs
#: nothing and is not dead: the day a plane writes `[[frame.component]] use = "chats"`,
#: the bar degrades in the order §3.6 decided rather than in whatever order it happened
#: to be listed in. They go first because they are readouts and the palette reaches every
#: chat and every workspace in two keystrokes at every width — so a short terminal loses
#: the reminder and nothing else, which is exactly what `top` cannot say for itself.
_DROP_ORDER = ("right", "repos", "chats", "workspaces", "top")

#: The entries of :data:`_DROP_ORDER` that go at `min_rows` itself — the ones with no
#: threshold of their own, dropped whole the moment rows are the tight dimension.
#:
#: **This is what makes :data:`_DROP_ORDER` a constant rather than a comment.** Until
#: Phase 5 nothing read that list: `visible_slots` spelled `s != "right"` and `s != "top"`
#: by hand, so the order was documented in one place and implemented in another, and an
#: entry added to it changed nothing at all. Derived, deleting an entry changes what a
#: short terminal draws — which is the property the deletion sweep can see.
#:
#: The three exceptions are NAMED rather than the members being listed, so a component
#: added to `_DROP_ORDER` joins the row drops by default. That is the safe direction: a
#: new readout that a short terminal keeps is a frame with less harness in it, and a new
#: one it drops is a reminder the palette already replaces.
#:
#: **`top` became the third exception in #740, and that is the whole of that issue's
#: "the ladder is inverted at the bottom".** Every entry here is a reminder `F2` reaches
#: in two keystrokes; `top`'s two facts — the workspace and the persona — are reached by
#: nothing else on a terminal with no sidebar, so dropping it in the same breath as the
#: bars was ranking it below a `repos` pane that had room for its heading and no repo. It
#: now goes only at the floor below which every panel goes (`visible_slots`' last
#: clause), and `repos` has a rows threshold of its own (:func:`repos_fits_rows`) —
#: measured, that trade never costs the harness a row at any height between the two
#: floors, because a `repos` pane starved to its floor costs the same border-plus-one-row
#: that `top` does. See `visible_slots`.
_ROW_DROPS = tuple(s for s in _DROP_ORDER if s not in ("right", "repos", "top"))

#: The frame charter itself draws, as components — `frame/builtins.py`, asked ONCE at
#: import for the edges and sizes every constant below is derived from.
#:
#: **This module used to carry four hand-written tables of per-slot facts** — which slots
#: cost columns, which are a fixed height, which one is variable, how big each is — and a
#: fifth spelled inline as ``slot == "top"``. Each was correct and each was a separate
#: thing to remember, which is what a component's own declaration replaces: the registry
#: is asked, and nothing here derives a slot's geometry from its position in a list.
#:
#: A module-level build is safe because `builtins.build` reads nothing and starts nothing
#: — six frozen dataclasses whose renderers are reached lazily — and because charter's
#: own six are fixed. A config boundary placing a plane's `[[frame.component]]` tables
#: builds its OWN registry (see `instance.frame_components`); it does not edit this one.
_BUILTIN = _builtins.build()

#: Edges whose split takes COLUMNS off the pane it is carved from, and edges whose split
#: takes ROWS. The `-h`/`-v` half of :func:`panel_argvs`, said once.
_COLUMN_EDGES = ("left", "right")
_ROW_EDGES = ("top", "bottom")

#: The edges tmux has to be told to place BEFORE the harness (`split-window -b`) rather
#: than after it. Also derived rather than spelled ``slot == "top"``: `left` is retired
#: (#488) and would belong here the day it came back, and a second answer to "which side
#: is this on" is what :data:`_COLUMN_EDGES` above already refuses to be.
_BEFORE_EDGES = ("top", "left")

#: The placed built-ins, in split order — the registry's own order, not this module's
#: reading of one. `personas` and `todos` are absent: the `sidebar` composite draws them
#: inside its own pane, so they are registered and never split for, which
#: `registry.Registry.on_edge` is what enforces.
_PLACED = tuple(c for c in _BUILTIN.all() if c.id in _builtins.SLOT_OF)


def _cells(c) -> int:
    """How many cells *c* is given when nothing has measured its content yet.

    ``Fixed(n)`` is *n*, and that is the whole of `top`, `bottom` and `right`. A
    ``Content`` or ``Fill`` slot has no answer to give here — its height is a function of
    what is in it and of what the window can spare — so it gets the floor every size
    policy already has (`component.cells` refuses a size below 1 cell: a panel nobody can
    see is a panel nobody asked for). That floor is not a stand-in for the real number:
    :func:`repos_rows` is, and :func:`slot_sizes` is what routes a caller to it.
    """
    return c.size.n if isinstance(c.size, Fixed) else 1


class _Derived(NamedTuple):
    """The five per-slot facts this module used to keep as five hand-written tables."""

    size: dict[str, int]
    edge: dict[str, str]
    column: tuple[str, ...]
    fixed_rows: tuple[str, ...]
    variable_rows: frozenset[str]


def _derive(placed) -> _Derived:
    """Read all five off *placed* — the components, in split order, that own a pane.

    One function rather than five comprehensions at module scope, and the reason is that
    a comprehension at module scope cannot be asked a second question. This can: a test
    hands it an arrangement whose sidebar is `Content()` or whose table sits on `top` and
    checks that every one of the five moves together, which is the property the five
    separate tuples never had — they agreed by having been edited in the same commit.

    Keyed by the name the component is SPELLED with — its committed slot name where it
    has one (`builtins.SLOT_OF`), and its own id where it does not. The four aliases are
    what `[frame] slots`, `charter panel top` and every caller in `commands_frame` already
    say, so they keep answering; a provider's component has no committed spelling and is
    keyed by the id it was placed under, which is what lets a plane's own arrangement be
    derived here at all.

    That fallback is the line Phase 1 could not cross: this used to be `SLOT_OF[c.id]`,
    a `KeyError` by design for any component charter did not write, so the one function
    every per-slot fact in this module is derived from could not be shown a provider.
    """
    def name(c) -> str:
        return _builtins.SLOT_OF.get(c.id, c.id)

    return _Derived(
        size={name(c): _cells(c) for c in placed},
        edge={name(c): c.edge for c in placed},
        column=tuple(name(c) for c in placed if c.edge in _COLUMN_EDGES),
        fixed_rows=tuple(name(c) for c in placed
                         if c.edge in _ROW_EDGES and isinstance(c.size, Fixed)),
        variable_rows=frozenset(name(c) for c in placed
                                if c.edge in _ROW_EDGES
                                and not isinstance(c.size, Fixed)),
    )


_SHIPPED = _derive(_PLACED)

#: Rows a horizontal panel occupies, and columns a vertical one does.
#:
#: **`repos`' entry is a FLOOR, not its size** (#488, moved off `bottom` by #515). Every
#: other slot is fixed: `top` says one row's worth of identity, `bottom` is the one-row
#: attention strip, `right` is a column of persona chips. `repos` carries the table,
#: which is as many rows as there are repos — so its real height is :func:`repos_rows`,
#: and this is what it never goes below (and what it is, on a plane with no clones at
#: all, where the pane says so in one line). Callers ask :func:`slot_sizes` rather than
#: indexing this directly, so nothing has to remember which of the two questions it is
#: asking.
#:
#: Derived from each component's declared size (:func:`_cells`) rather than written out,
#: so `right`'s 22 columns and the floor under the table are stated in one place — the
#: component — and read here.
SLOT_SIZE = _SHIPPED.size

#: Which side of the harness each slot is split off, read straight from the component
#: that draws it. The one fact :func:`panel_argvs` needs that is not a size.
SLOT_EDGE = _SHIPPED.edge

#: Rows the harness keeps whatever the repo table would like. The frame exists to show
#: the plane's state around an agent session, and a session squeezed into three rows is
#: not one anybody can read — so the table gives up rows before the harness does.
#:
#: Not hypothetical, and this number is what stands between the operator and it: measured
#: against tmux 3.7c, `resize-pane -t <the table pane> -y 40` in a 20-row window left the harness
#: pane **1 row tall**. tmux clamps to what the window has and takes the remainder from
#: its neighbour without complaint, so an over-large height is not refused — it is
#: granted, out of the one pane that matters.
HARNESS_MIN_ROWS = 12

#: The most rows a tab strip may be sized to when its names will not fit on one (#829).
#:
#: **A ceiling, and the budget is a separate question.** What a strip may actually spend
#: is what the harness can spare above :data:`HARNESS_MIN_ROWS`, and :func:`slot_sizes` is
#: where that is worked out; this is the statement that a strip is a strip however many
#: rows are going spare. Both are needed: a plane with forty chats in a hundred-row window
#: has rows to burn, and a forty-chat strip is not a readout any more — it is the list,
#: and the list is `charter frame-palette`, which reaches every chat at every width.
#:
#: **Three, measured through the same cut `slots._bar` draws with**, over this
#: repository's own fifteen workspaces: they need 2 rows at 160 columns, 3 at 120, 4 at
#: 100 and 6 at 80. So three draws every workspace this plane has at every width #725
#: measured an operator running at, and stops short of the width where the strip would be
#: taking six rows off the harness to become a list.
#:
#: The top of the cycle :data:`BAR_ROWS_KEY` walks and NOT the height a frame launches at
#: — that is :data:`BAR_ROWS_DEFAULT` (#880). `commands_frame._slot_sizes` reads this
#: frame's own choice through :func:`bar_rows_cap` and hands the answer to
#: `frame.slots.bar_rows_wanted` as its *cap*, never applying it twice: this module caps a
#: strip at what the harness can spare and the SIZER caps it at what a strip is, and a
#: second `min` here would be a bound no input could make observable — the survivor
#: `tools/sweep.py` reports and this repository deletes.
BAR_MAX_ROWS = 3

#: How tall a tab strip is when nobody has said otherwise — one row, on every run (#880).
#:
#: **The launch default and NOT the ceiling**, which are two numbers and used to be one.
#: `slots.bar_rows_wanted` composed at 1, 2 … :data:`BAR_MAX_ROWS` and kept the tallest
#: height it FILLED, so a plane with many names came up two rows deep whether or not its
#: operator wanted the harness two rows shorter. A strip that overflows is not a strip that
#: has asked for the room: `+N` is clickable and opens the palette, which lists every name,
#: so a collapsed strip is one press from the complete list rather than a dead end.
#:
#: The operator raises it with :data:`BAR_ROWS_KEY`, which cycles 1 → 2 → 3 and back
#: (:func:`next_bar_rows`). A plane that always wants three says so once in its own
#: `[[frame.component]] size` (#687), which :func:`_grown` still honours — that is a pin
#: and this is a default, and neither is the other.
BAR_ROWS_DEFAULT = 1

#: Every height a tab strip may be at — :data:`BAR_ROWS_DEFAULT` through
#: :data:`BAR_MAX_ROWS`.
#:
#: **A range and not two comparisons, and the deletion sweep is what settled it.** Written
#: as ``BAR_ROWS_DEFAULT <= now <= BAR_MAX_ROWS``, the LOWER boundary is an equivalent
#: mutant by construction: the fallback for a value below the range IS the bottom of the
#: range, so `<` and `<=` answer the same number for every input there is, and
#: `shift-boundary` would report it as a survivor forever. A membership test has no
#: boundary to move and says exactly the same thing.
_BAR_ROWS = range(BAR_ROWS_DEFAULT, BAR_MAX_ROWS + 1)

#: The key that cycles a tab strip's height, bound by `commands_frame.conf_text`.
#:
#: **Here rather than in `commands_frame`, because `instance` has to know it too.** A
#: component's own toggle key may not collide with a key charter has already bound
#: (`instance.component_arrangement`'s `bound` set, which is where `overlay.HATCH_KEY` and
#: the mouse keys already are); `instance` importing `commands_frame` would close a cycle,
#: and this module is the one that owns bar heights and is already imported from there.
#:
#: **A third shipped `bind -n`, and the cost is stated rather than waved past.** A root-table
#: binding is server-wide and takes the key before the harness pane sees it, which is why
#: charter binds NO component toggle by default. It claims exactly two keys today — `F2`
#: for the palette, `F12` for the escape hatch — and this is the third. It is affordable for
#: the same reason those two are: `F3` is not a key Claude Code, codex or opencode binds,
#: it is next to the palette an operator already knows, and unlike a toggle it is charter's
#: own gesture rather than one of an open set. Unlike `F2` it is NOT configurable, exactly
#: as `F12` is not: one key, one meaning, and `instance` refuses a component that wants it.
BAR_ROWS_KEY = "F3"


def bar_rows_cap(rows: int | None) -> int:
    """How tall this frame's strips may grow right now — the *cap*
    `slots.bar_rows_wanted` takes, out of what the operator has cycled to.

    *rows* is `state.bar_rows`' answer: ``None`` for a frame nobody has pressed
    :data:`BAR_ROWS_KEY` in, which is every frame at launch, and which is
    :data:`BAR_ROWS_DEFAULT`.

    **``None`` needs no branch of its own**, and there was one until the deletion sweep
    asked about it: ``None`` is not in :data:`_BAR_ROWS`, so the membership test already
    answers for it with the same number a `rows is not None` guard in front would have
    produced. Written as two expressions, one of them was an equivalent mutant by
    construction — `collapse-ifexp` to `rows` changed no answer for any input — which is
    the line this repository deletes rather than documents.

    **A cap and not a height.** A strip still grows only as far as its own names need
    (`slots.bar_rows_wanted` measures, `_grown` spends), so pressing the key on a plane
    whose names already fit on one row changes nothing on screen — which is the honest
    outcome: there is nothing to put on a second row. What the key raises is the ceiling
    that was stopping a strip which DOES overflow.

    **Anything outside :data:`_BAR_ROWS` degrades to the default rather than clamping**,
    and that is `instance.density_level`'s discipline: the value is read back off a file,
    so `0`, `7`, `-1` and a truncated write are all "this frame has not chosen", not "this
    frame chose something charter will round for it". Clamping a `7` to `3` would leave a
    frame silently at the ceiling because a byte went missing.
    """
    return rows if rows in _BAR_ROWS else BAR_ROWS_DEFAULT


def next_bar_rows(rows: int | None) -> int:
    """The height :data:`BAR_ROWS_KEY` moves to from *rows* — 1 → 2 → 3 → 1.

    **Through :func:`bar_rows_cap` and not off the raw number**, so the cycle has exactly
    one idea of what "where we are now" means: a frame that has chosen nothing is at
    :data:`BAR_ROWS_DEFAULT` and its first press asks for two, and a file charter cannot
    read is at the default too rather than sending the next press somewhere no one asked
    for. Written as one clamp shared with the reader, because two clamps are two chances to
    disagree about whether `3` wraps.

    Wrapping rather than stopping at the ceiling: every press has to do something, which is
    `builtin_actions._register_strip`'s own rule for the tab walk one surface over. A key
    that silently did nothing at the top is a key an operator presses twice and then stops
    trusting.
    """
    now = bar_rows_cap(rows)
    return now + 1 if now < BAR_MAX_ROWS else BAR_ROWS_DEFAULT

#: One row per pane border. tmux charges a horizontal split one row for the divider on
#: top of the pane's own height — measured on 3.7c: a 50-row window split `-l 8` reads
#: back as a 41-row harness and an 8-row panel, which is 49. Counted explicitly rather
#: than folded into :data:`HARNESS_MIN_ROWS` so the arithmetic in :func:`repos_rows`
#: says what it is doing.
_BORDER_ROWS = 1

#: One column per pane border, the vertical counterpart of :data:`_BORDER_ROWS` and
#: measured the same way — on tmux 3.7c, a 110-column window split `-h -l 22` reads back
#: as an 87-column harness and a 22-column sidebar, which is 109.
_BORDER_COLS = 1

#: Which of the SHIPPED slots take COLUMNS off the pane they are split from rather than
#: rows — the statement of charter's own frame, derived from each component's declared
#: edge rather than written out as ``== "right"``.
#:
#: :func:`repos_cols` and :func:`panel_argvs` both have to agree about which splits cost
#: columns, and they now ask :func:`_edge_of` rather than this tuple — one predicate,
#: ``edge in _COLUMN_EDGES``, which answers for a component this plane placed as well as
#: for charter's own. This is that same predicate frozen over the shipped frame, which is
#: what the geometry test asserts against literals.
_COLUMN_SLOTS = _SHIPPED.column

#: The horizontal strips whose height is a CONSTANT, and therefore the rows
#: :func:`repos_rows` has to subtract before it may spend what is left. `right` is not
#: here because it costs columns, and `repos` is not here because it is the slot being
#: sized.
#:
#: **`bottom` joined this list in #515 and that is the whole arithmetic change.** It used
#: to be the variable-height slot itself, so the only fixed strip to subtract was `top`;
#: it is now the one-row attention strip it was before #488, and the table it used to
#: carry is `repos`. A `repos_rows` still subtracting `top` alone would hand the table
#: two rows the status strip and its own border are already using, and tmux grants an
#: over-large height out of the neighbour rather than refusing it — the harness.
#:
#: Both exclusions this comment states are now the components' own declarations: `right`
#: is out because its edge costs columns, `repos` because its size is `Content()` rather
#: than `Fixed`. Neither was enforced by anything while this was a written-out tuple.
#:
#: Read through :func:`_is_fixed_row`, which is where a component this plane placed gets
#: the same question answered from its own edge.
_FIXED_ROW_SLOTS = _SHIPPED.fixed_rows

#: The slots whose height is a function of their content rather than a constant — the
#: ones :func:`slot_sizes` answers with :func:`repos_rows` instead of :data:`SLOT_SIZE`.
#:
#: **It is also which pane is the DEPENDENT one when sizes are re-applied**, and that is
#: not a second meaning bolted on. tmux's `resize-pane -y` moves exactly one boundary, so
#: in a vertical stack of N panes only N-1 heights can be asserted; assert them all and
#: the result depends on the order, which is how a re-assertion in split order came out
#: with the table one row tall and the attention strip six (measured on 3.7c at 200x50:
#: `top,bottom,repos` left `%3 h=1` and `%2 h=6` — the two sizes swapped panes). The pane
#: that must be left to take the remainder is the one whose size is already a function of
#: everything else, which is this one. See `commands_frame._reassert_sizes`.
#:
#: The complement of :data:`_FIXED_ROW_SLOTS` over the horizontal edges, and written as
#: that rather than as a second list: a slot cannot be in both, or in neither, because
#: one predicate decides it. `Content` and `Fill` both land here — a slot whose height is
#: its content's and a slot whose height is what is left are the same kind of dependent
#: pane as far as `resize-pane` is concerned.
VARIABLE_ROW_SLOTS = _SHIPPED.variable_rows


def _key(name):
    """*name* as the five tables above key it: a built-in id resolved to its alias.

    The tables are keyed by the committed spelling because that spelling is committed, and
    a component id is the currency — so `identity` and `top` must reach one entry, not
    two. `builtins.SLOT_OF` is the same one table `_derive` keyed them with, read in the
    same direction.
    """
    return _builtins.SLOT_OF.get(name, name) if isinstance(name, str) else name


def _arrangement():
    """The placements this plane resolved, or nothing — the one read of `config.FRAME`.

    **One line, because there are two callers and the fallback in it is unpinned.**
    :func:`_placed_here` reads the arrangement for a component charter did not write and
    :func:`pinned_repo_rows` reads it for the one built-in whose height a plane may
    commit; the `or ()` is the same defence for both, and a second copy of it is a second
    line nothing can fail without. `tools/sweep.py` reported that copy the day it appeared
    and `docs/news/unreleased-the-deletion-sweep-is-a-thing-the-repo-runs.md` records the
    original at this module's line 289 as a survivor it examined — so the answer to a
    second one is to have one, not to accept two.

    It stays a fallback rather than a plain subscript. `instance.frame_of` sets
    ``components`` before its own early return, deliberately ("a key present on one path
    and absent on another is two shapes for one answer"), so nothing in production reaches
    here without it — but `config.FRAME` is module state that a caller may replace whole,
    and a `KeyError` out of either reader costs a launch its entire frame. Pinning it
    would mean a test constructing a `config.FRAME` that `frame_of` cannot produce, which
    is a test of the line rather than of any property.

    Imported inside the call, the way :func:`_table_min_cols` reaches for `statusline` and
    for the same reason: this module is imported by every launch path, and `config`'s
    import resolves the plane root.
    """
    from .. import config
    return config.FRAME.get("components") or ()


def _placed_here() -> dict[str, tuple[str, int]]:
    """name → (edge, cells) for what THIS PLANE places and charter did not write.

    Empty on every plane that spells its frame with `[frame] slots`, which is charter's
    own and very nearly everyone's — the five tables above are the whole answer there, and
    this costs a `dict.get` on a mapping that is already resolved.

    **Read from the resolved config rather than passed down through six signatures.**
    `instance.frame_of` already resolves a plane's `[[frame.component]]` tables into
    placements carrying an edge and a size policy, and `config.FRAME` already holds them;
    threading an extra argument through `slot_sizes`, `panel_argvs`, `repos_cols`,
    `repos_rows` and `harness_rows` would be five more parameters each caller could get
    wrong and each test could omit — which is how the two answers to "how wide is the
    table's pane" came apart in #500.

    The read itself is :func:`_arrangement`'s, which is where the "imported inside the
    call" reasoning moved when a second caller appeared.

    Only names the shipped tables do not already carry. A plane cannot move charter's own
    `right` from here — `component_tables` refuses an edge on a built-in that disagrees
    with its declaration, and refuses a size on the three whose geometry `layout` derives
    at import, because a value read, validated and then ignored is the convincing empty
    this phase was written against. The repo table is the exception both of those rules now
    have (`instance._built_in_size`), and it is still not read HERE: its height is
    :func:`repos_rows`', and :func:`pinned_repo_rows` is what reads the number for it.

    **The signature count above is THIS function's measurement and does not transfer.**
    It is what the five named functions would each have to carry to answer one question
    for a name none of them knows in advance: where a placed component sits and how many
    cells it costs. The repo strip's pin is one number for one slot with one consumer, and
    #661 is what came of borrowing the count anyway — measured, that path is two
    signatures and three call sites. See :func:`pinned_repo_rows`.
    """
    out: dict[str, tuple[str, int]] = {}
    for placed in _arrangement():
        name = placed.get("slot")
        if isinstance(name, str) and name not in SLOT_SIZE:
            out[name] = (placed["edge"], _policy_cells(placed["size"]))
    return out


def _policy_cells(size) -> int:
    """A size POLICY as cells, by the one rule :func:`_cells` already states."""
    return size.n if isinstance(size, Fixed) else 1


def pinned_repo_rows() -> int | None:
    """The height this plane PINNED the repo strip to, or ``None`` for the shipped policy.

    **The one per-plane override charter's own geometry has, and the reason it is the only
    one.** Every other placed built-in is `Fixed` in its own declaration, so
    :data:`SLOT_SIZE` — derived once, at import — is the whole answer for it and a
    committed number could only be read and ignored (`instance._built_in_size` argues that
    half). `repos` is `Content()`: its height never enters that table, it is computed by
    :func:`repos_rows` from the arrangement it is HANDED at every launch and again on every
    `window-resized`, and this is what makes a number there mean something.

    **The one function in this module that reads a committed file, and it is public so
    that its callers can be counted.** It used to be called from inside
    :func:`repos_rows`, borrowing :func:`_placed_here`'s "rather than threaded through
    five signatures" word for word — and that cost was mispriced (#661). `repos_rows` was
    this module's one provably pure function and its tests were written to that property,
    so a `size` in the plane's own `charter.toml` answered a caller that had passed
    `content_rows=4` with `15`: `layout.repos_rows(content_rows=4, window_rows=50,
    slots=["top","bottom","repos"])` returned the committed number. On this repo, whose
    `charter.toml` is tracked, that turned six tests red for everyone the moment an
    operator did what the feature's own news entry told them to do.

    The measurement the borrowed reason skipped: the pin is ONE number for ONE slot with
    ONE consumer, so the path is `repos_rows` ← `slot_sizes` ← the three sites in
    `commands_frame` that already build `content_rows` the same way. Two signatures, and
    the three sites share `commands_frame._slot_sizes`, which is where this is called from
    and the only place it is. `_placed_here`'s five is real for `_placed_here` — an edge
    and a cell count for a name `panel_argvs`, `repos_cols` and `harness_rows` each have
    to ask about — and it does not transfer to this.

    ``isinstance(placed["size"], Fixed)`` asks which POLICY the arrangement resolved to,
    which is the property and not a stand-in for it: a plane that writes its arrangement
    out and puts no ``size`` on the table holds `Content()` there — the shipped policy
    spelled longhand — and reading that as a pin would hand such a plane a one-row strip.
    The name is tested too, because a placement's ``size`` says nothing about which
    component it belongs to; without it the first placement in file order would decide the
    table's height, and on charter's own plane that is `identity`, `Fixed(1)`.
    """
    for placed in _arrangement():
        if placed.get("slot") == "repos" and isinstance(placed["size"], Fixed):
            return placed["size"].n
    return None


def _edge_of(name):
    """Which side of the harness *name* attaches to, or ``None`` for a name nothing placed.

    ``None`` and not a default. Every caller below asks a membership question of the
    answer (`in _COLUMN_EDGES`, `in _ROW_EDGES`, `in _BEFORE_EDGES`), and a name charter
    knows nothing about must fall out of all three rather than be assigned a side — which
    is the same filter-don't-refuse degrade `slot_sizes` has always made one line up.
    """
    key = _key(name)
    if key in SLOT_EDGE:
        return SLOT_EDGE[key]
    placed = _placed_here().get(key)
    return placed[0] if placed else None


def _size_of(name):
    """How many cells *name* is given, or ``None`` for a name nothing placed."""
    key = _key(name)
    if key in SLOT_SIZE:
        return SLOT_SIZE[key]
    placed = _placed_here().get(key)
    return placed[1] if placed else None


def _is_fixed_row(name) -> bool:
    """Whether *name* is a horizontal strip whose height :func:`repos_rows` must subtract.

    Two sources, one question, and the branch is which of them knows about *name*.
    Charter's own components answer from :data:`_FIXED_ROW_SLOTS`, the table `_derive`
    built from their declared size and edge — so `repos` stays out of it for being
    `Content()` and `right` for costing columns, exactly as before. A component this plane
    placed answers from its edge alone, because a committed size is a NUMBER: the config
    boundary refuses a `[[frame.component]]` table that does not carry one, so there is no
    third kind for the second branch to be wrong about.
    """
    key = _key(name)
    if key in SLOT_EDGE:
        return key in _FIXED_ROW_SLOTS
    return _edge_of(key) in _ROW_EDGES


def _table_min_cols() -> int:
    """The narrowest pane the repo table can be drawn in at all — `statusline._LEFT_W`,
    READ rather than copied.

    `slots._table_cap` answers 0 below this width and `slots._table_lines` refuses
    outright ("too narrow for the table is NO table, not a cut one"), so a `repos` pane
    split narrower than this is a bordered rectangle with nothing in it — which reads as
    "this workspace has no repos" on a plane that has fourteen. :func:`visible_slots`
    drops the slot instead, and it must drop it at exactly the width the RENDERER stops
    drawing at: a second copy of the number here would be a guard matching a spelling,
    and the two would come apart the first time a column width moved.

    Imported inside the call rather than at module scope, the way `frame/slots.py`
    already reaches for the same module: this file is imported by every launch path and
    `statusline` pulls in `config`, whose import resolves the plane root.
    """
    from .. import statusline as sl
    return sl._LEFT_W


def _table_min_rows() -> int:
    """The shortest `repos` pane that can say anything about a repo at all —
    `slots._TABLE_MIN_ROWS`, READ rather than copied.

    :func:`_table_min_cols`' argument on the other axis, and #740 is the measurement that
    asked for it. `_repos` spends its first row on the `▪ repos N` heading and hands what
    is left to `_table_lines`, which draws nothing at all on a budget of zero — so a
    one-row pane is three rows of terminal (two tmux rules and the row itself) saying the
    number 8, on a terminal that has just been judged too short for the one row naming the
    workspace. Measured on tmux 3.7c and the 3.2 floor at 120 columns, on the ladder as it
    was: at 20 rows the table pane came back 3 rows and drew two repos, at 19 it was 4 and
    drew three, and from **16 down it was 1 and drew none** — a heading, and the two pane
    rules around it, at every height the operator's report was written from.

    Read from the renderer for the reason :func:`_table_min_cols` gives in full: the
    height at which the table stops saying anything and the height at which the launcher
    stops splitting a pane for it must be ONE number, or the two come apart the first time
    the heading gains a row.

    Imported inside the call, like :func:`_table_min_cols` and for the same reason.
    """
    from . import slots
    return slots._TABLE_MIN_ROWS


def repos_row_cap(slots: list[str] | tuple[str, ...], *, window_rows: int) -> int:
    """The most rows a `repos` pane may have in a *window_rows*-row window drawing
    *slots* — what is left after every other horizontal strip and after the harness's own
    floor.

    **One function because two decisions read it** (#740). :func:`repos_rows` applies it
    as the cap on what the table's content wants, and :func:`repos_fits_rows` asks whether
    it leaves room for a repo row at all. Spelled twice, the launcher would have decided
    the pane was worth splitting from one arithmetic and sized it from another — which is
    exactly the sizer-and-renderer disagreement `_table_min_cols` exists to prevent one
    axis over.

    Every horizontal strip in :data:`_FIXED_ROW_SLOTS` costs its own height plus
    :data:`_BORDER_ROWS`; `right` costs columns and is not counted. `repos` itself is not
    counted either — it is the variable-height slot this is the budget FOR — so a caller
    may pass the whole slot list including it, which is what both callers do.

    Can come out at or below zero: a 16-row window has no rows to spare at all.
    :func:`repos_rows` floors the pane at 1 there because `panel_argvs` cannot split a
    zero-row pane, and :func:`repos_fits_rows` is what stops one being split.
    """
    other = sum(_size_of(s) + _BORDER_ROWS for s in slots if _is_fixed_row(s))
    return window_rows - other - _BORDER_ROWS - HARNESS_MIN_ROWS


def repos_fits_rows(slots: list[str] | tuple[str, ...], *, window_rows: int) -> bool:
    """Would a `repos` pane in a *window_rows*-row window drawing *slots* be tall enough
    to draw a repo row in?

    :func:`repos_fits` on the other axis, and the drop #740 is about. Below this the pane
    is its heading and nothing else — `▪ repos 8` between two tmux rules, three rows of a
    ten-row terminal spent on a number — and the slot goes the way `right` and the bars
    go, rather than existing as a label for content there is no room for.

    **Asked with the slots that SURVIVED the rungs above it**, which is what makes this
    the precedence #740 asked for rather than a second opinion: `top` is tested after this
    one and is not dropped by rows at all any more, so its two rows are already spent when
    this asks what is left. A `repos` pane and the identity strip both cost a border plus
    one row at the bottom of the ladder, so the trade is exactly even in rows and
    lopsided in facts — one count against a workspace name and a persona.

    Rows do not depend on the split ORDER the way columns do (a horizontal strip costs the
    same rows wherever it was split), so unlike :func:`repos_fits` this needs no
    order-of-panes caller beside `visible_slots`: `commands_frame._relayout` re-splits a
    permutation of the same slot list, which this answers identically.
    """
    return repos_row_cap(slots, window_rows=window_rows) >= _table_min_rows()


def repos_cols(slots: list[str] | tuple[str, ...], *, window_cols: int) -> int:
    """How wide the `repos` pane actually is, in a *window_cols*-column window whose
    slots were split in *slots*' order — **not the window's own width** (#500).

    `panel_argvs` splits every slot off the HARNESS pane in list order, so a side slot
    split before `repos` has already narrowed the pane `repos` is then carved out of.
    Measured on tmux 3.7c, window 120x40, `-l 22` for `right` and `-l 6` for the table:

    * ``["top", "bottom", "repos", "right"]`` (the shipped order) — the table reads back
      **120** wide; `right` is split off the harness afterwards and sits beside the
      harness only (`%3 top=32 h=6 w=120`, `%4 top=2 left=98 h=29 w=22`).
    * ``["right", "top", "bottom", "repos"]`` — the table reads back **97**, which is
      ``120 - 22 - 1``.

    That difference is a promise, not an accident: `instance.frame_of` keeps an
    operator's `[frame] slots` order verbatim and
    `tests/test_frame_config.py::test_the_operators_own_slot_order_is_kept_exactly`
    pins it, precisely because the order IS the geometry. What #500 got wrong was not the
    order — it was handing `slots.repos_rows_wanted` the window's width regardless, so a
    table pane that had been inset beside the sidebar was sized for a table its own pane
    was too narrow to draw. Below :func:`_table_min_cols` the renderer draws no table at
    all, so a 110-column frame with `right` first got a seven-row pane and one line in
    it, six rows taken off the harness and left blank.

    Order in, order out — this reads *slots* rather than a set, and stops at `repos`.
    A slot split AFTER `repos` costs it nothing (measured above), and killing one gives
    the columns back (`kill-pane` on `right` widened the same pane from 87 to 110),
    which is why the list a caller passes has to be the order its panes were actually
    split in: at launch that is the drawable slot list, and for a running frame it is the
    recorded pane map's own order, survivors first and later splits appended.

    With no `repos` in *slots* this answers the width one WOULD be split to, which is
    what :func:`visible_slots` asks it before deciding whether the slot survives at all.

    :func:`pane_cols` is the walk; this names the one slot charter's own geometry has
    always asked it about, so the twenty-odd callers that say `repos_cols` keep saying it.
    """
    return pane_cols(slots, "repos", window_cols=window_cols)


def pane_cols(slots: list[str] | tuple[str, ...], slot: str, *,
              window_cols: int) -> int:
    """How wide *slot*'s pane is, split in *slots*' order in a *window_cols* window.

    :func:`repos_cols` generalised to any slot, and generalised rather than copied because
    the second caller wants the identical rule: a tab strip is sized from the names it can
    draw at its own width (`slots.bar_rows_wanted`), and a strip split after `right` is
    carved out of a pane the sidebar has already narrowed by 23 columns. That is #500 word
    for word, one slot over — and #500 is what a second copy of this loop would be
    reproducing rather than reusing.

    Everything :func:`repos_cols` says about the walk holds here and is said there: order
    in, order out; a slot split AFTER *slot* costs it nothing; a *slot* absent from the
    list is answered with the width one WOULD be split to.

    *slot* is resolved through :func:`_key` like every other name this module is handed, so
    a caller may ask about `identity` or about `top` and get one answer rather than two.
    """
    stop = _key(slot)
    out = window_cols
    for name in slots:
        if _key(name) == stop:
            break
        if _edge_of(name) in _COLUMN_EDGES:
            out -= _size_of(name) + _BORDER_COLS
    return max(0, out)


def repos_fits(order: list[str] | tuple[str, ...], *, window_cols: int) -> bool:
    """Would a `repos` pane split in *order*, in a *window_cols*-column window, be wide
    enough to draw a table in at all?

    **One rule, two callers, and the second one is why it is a function.**
    :func:`visible_slots` asks it at launch, where *order* is the configured slot list.
    `commands_frame._relayout` asks it for a density change, where *order* is the
    surviving panes in their recorded split order followed by what is about to be split —
    which is NOT the level's own list, and which is the whole of #500's round 3. A frame
    that already has `right` and grows a table gets that table split off a harness pane
    the sidebar has already narrowed by 23 columns, so the level's list says 110 and the
    pane is 87.

    Without this shared, both said different things and the disagreement was visible: the
    launch filter kept `repos` (from the level's order, full width) and the sizer floored
    it at one row (from the pane's order, too narrow) — a bordered rectangle with nothing
    in it, which is exactly the "no repos" lie #515 split the pane to avoid.
    """
    return repos_cols(order, window_cols=window_cols) >= _table_min_cols()


def visible_slots(slots: list[str], cols: int, rows: int,
                  min_cols: int, min_rows: int) -> list[str]:
    """Which of *slots* fit in a *cols* x *rows* terminal.

    Degradation, not refusal: below the floor the harness simply gets the whole terminal,
    which is the same choice `statusline.render` makes when it runs out of width. Follows
    `_DROP_ORDER`: `right` is the first to go, on ANY shortage — a terminal that is short
    on rows cannot spare a side panel's own divider any more than a narrow one can spare
    its columns — then `repos`, on either axis, then the bars, and `top` last of all.

    **What this drops LAST, which is #740's question.** A rung goes when the pane it would
    get cannot carry the thing the rung exists for, and among rungs that still can, the
    ones whose facts another surface reaches go first. That ordering is `_DROP_ORDER`'s and
    it is now honoured on the rows axis as well as the columns one:

    * `right` — any shortage. It costs columns AND a divider.
    * `repos` — a pane too narrow for a table (:func:`repos_fits`) or too short for one
      repo row (:func:`repos_fits_rows`). Both read the renderer's own numbers.
    * `chats`, `workspaces` (:data:`_ROW_DROPS`) — at `min_rows`, whole. Readouts `F2`
      replaces in two keystrokes.
    * `top` — only at the floor in the last clause, with everything else. One row, two
      facts, and on a terminal with no sidebar it is the plane's only roster (`slots._top`,
      #530).
    * `bottom` — never. The one alert and the command that fixes it.

    **Until #740 that ladder was a cliff with `repos` standing outside it.** `_ROW_DROPS`
    took `top` the instant rows fell below `min_rows` while `repos` had no rows test at
    all, so at 120x10 the frame spent three rows (two tmux rules and `▪ repos 8`) on a
    count having just decided it could not afford the one row that names the workspace.
    A `repos` pane starved to its floor and the identity strip cost the same two rows —
    `_BORDER_ROWS` plus one — so the harness pays nothing for the exchange. Measured on
    tmux 3.7c and the 3.2 floor at 120 columns, eight clones, at every height from 19 down
    to 10: the harness pane keeps exactly what it kept at 19 and from 16 down, and GAINS a
    row at 17 and two at 18, where the table had been drawing one or two repos. Those one
    or two repo rows are what this costs, and they are the trade `_DROP_ORDER` always
    named.

    **`repos` goes on a width its own renderer cannot draw in, and that width is read
    from the renderer** (:func:`_table_min_cols`). This is the drop `bottom` did not need
    while it carried both things: below `statusline._LEFT_W` the table refuses to draw at
    all, so the pane kept drawing the attention row above it and the operator saw no
    difference. Split apart (#515), the same case is a bordered rectangle with nothing in
    it — a pane that says "no repos" on a plane full of them, which is the false-clean
    reading the frame refuses everywhere else. The test is the PANE's width, not the
    window's: a `[frame] slots` naming `right` first insets this pane by 23 columns, so
    :func:`repos_cols` is asked with the slots that survived the line above.

    `bottom` never goes here, and it is the only slot that never does: it is the one
    alert and the command that fixes it, which is why a frame is worth drawing on a
    terminal with room for nothing else.

    **The row-edge drops read :data:`_ROW_DROPS`, which is derived from
    :data:`_DROP_ORDER` — and until Phase 5 that constant was read by nothing at all.** It
    documented an order this function then spelled out by hand, so §3.6's instruction to
    "join `_DROP_ORDER` above `top`" would have changed no behaviour whatever: the two
    bars would have survived a shortage that took `top`, which is the wrong way round for
    a readout the palette makes redundant. Derived, the list is load-bearing — an entry
    deleted from it changes what a short terminal draws — and that is the difference
    between a constant and a comment with a name.
    """
    keep = list(slots)
    if cols < min_cols or rows < min_rows:
        keep = [s for s in keep if s != "right"]
    if rows < min_rows:
        keep = [s for s in keep if s not in _ROW_DROPS]
    # **One drop, two reasons** — a pane too narrow to draw a table, or too short to draw
    # a repo row in one. Written as two `if`s with the same body, the second one's
    # `"repos" in keep` was a conjunct nothing could ever turn red (the deletion sweep
    # reported exactly that): a re-filter for a name that is not in the list rebuilds the
    # identical list, so the membership half could not change an outcome. `remove` is what
    # makes it load-bearing — it raises on a frame with no table rather than quietly doing
    # nothing — and it says "take the table out" instead of "rebuild the list without it".
    #
    # Both questions are asked about `keep` rather than about *slots*: these run after the
    # two filters above, so the width is the one this pane is actually inset to and the
    # rows are what is left with `top` already holding its own. That ordering is the
    # precedence #740 asked for — see :func:`repos_fits_rows`.
    if "repos" in keep and not (repos_fits(keep, window_cols=cols)
                                and repos_fits_rows(keep, window_rows=rows)):
        keep.remove("repos")
    if cols < min_cols // 2 or rows < min_rows // 2:
        keep = []
    return [s for s in slots if s in keep]


def repos_rows(*, content_rows: int, window_rows: int,
               slots: list[str] | tuple[str, ...] = (),
               pinned_rows: int | None = None) -> int:
    """How many rows the `repos` pane gets: what its content wants, floored and capped.

    Pure arithmetic, deliberately — this is the whole of #488's "how tall is the table?"
    and it is decided here, with no tmux and no filesystem, so both callers that need an
    answer (a launch, and the `window-resized` hook's own recompute) necessarily get the
    same one.

    **Every term is an argument, and *pinned_rows* is one of them for a measured reason.**
    #660 reached `config.FRAME` from inside this function instead, and #661 is what that
    cost: a `size` committed to the plane's own `charter.toml` made this answer `15` to a
    caller that had passed `content_rows=4`, `window_rows=50` and neither bound binding —
    a number out of a file the caller never named, from the module's one function whose
    tests assert it is arithmetic. `None` is "this plane pinned nothing", which is every
    plane that does not say otherwise and every test that is asking about the arithmetic;
    `layout.pinned_repo_rows` is the read, and `commands_frame._slot_sizes` is the one
    caller that makes it.

    * **The floor is `SLOT_SIZE["repos"]`.** A workspace with no clones still has one
      line to draw — that it has none, and the command that gets it one
      (`slots._empty_lines`). Never zero: a zero-row pane is one tmux refuses to split at
      all.
    * **The cap leaves the harness :data:`HARNESS_MIN_ROWS`**, and it is
      :func:`repos_row_cap` — the same arithmetic :func:`repos_fits_rows` asks before the
      pane is split at all (#740), rather than a second copy of it here. *window_rows* is
      the whole window and *slots* is what else is being drawn in it; asking for the whole
      slot list rather than a pre-computed number is what keeps that decision in one place
      instead of at each call site.
    * **Between the two, the content wins — unless the plane pinned a height.**
      *content_rows* is what `slots._repos` would actually fill
      (`slots.repos_rows_wanted`), so a two-repo plane gets a two-row strip rather than a
      fourteen-row one padded with blanks. That is the DEFAULT and it is the answer for
      every plane that does not say otherwise; a ``size`` on the table's own
      `[[frame.component]]` table replaces it with a constant, which arrives here as
      *pinned_rows* and is the operator asking for a strip that does not move when a
      clone is added or removed.

    **A pin replaces the content, not the floor and not the cap**, and the cap is why.
    tmux does not refuse an over-large height, it grants it out of the neighbour: measured
    on 3.7c, `resize-pane -t <the table pane> -y 40` in a 20-row window left the HARNESS
    pane one row tall. A committed ``size = 40`` is that command with a config file in
    front of it, and a plane's frame is committed and shared — so a pin degrades in a
    short terminal exactly as a fourteen-repo plane's table already does, and the operator
    keeps :data:`HARNESS_MIN_ROWS` of the session the frame is drawn around. The floor
    holds for the same reason it always did: `panel_argvs` cannot split a zero-row pane.

    The cap can come out below the floor — a 16-row window has no rows to spare at all —
    and the floor wins then, because `panel_argvs` has to be able to split the pane at
    all. **Since #740 no LAUNCH reaches that**: :func:`repos_fits_rows` drops the slot
    rather than splitting a pane with room for a heading and no repo, so the floor is now
    reached only by `cmd_resize`, which re-sizes panes and neither creates nor destroys
    them — the same asymmetry `slots._too_narrow_lines` exists for one axis over. What
    protects the harness at the very bottom is :func:`visible_slots`' last clause, which
    drops every slot below half the size floors.
    """
    floor = SLOT_SIZE["repos"]
    wanted = content_rows if pinned_rows is None else pinned_rows
    return max(floor, min(wanted, repos_row_cap(slots, window_rows=window_rows)))


def column_sizes(slots: list[str] | tuple[str, ...]) -> dict[str, int]:
    """Every slot in *slots* whose split costs COLUMNS, mapped to how many it costs.

    The half of :func:`slot_sizes` that depends on nothing else: a side panel is
    :data:`SLOT_SIZE` columns wide in every window it is ever drawn in, whatever the rows
    are and whatever the table has to say. Read through :func:`_size_of` and
    :func:`_edge_of`, the same two questions :func:`slot_sizes` asks — so this is a second
    loop over one answer, never a second table of how wide a side panel is.

    **Split out because the ORDER the two are applied in is load-bearing (#510).**
    `commands_frame._reassert_sizes` has to put the side panels back at their own width
    before it may ask tmux how wide the variable-row pane beside them is, because tmux
    redistributes every pane proportionally on a window resize and a scaled sidebar
    answers for a geometry that is one command away from not existing. Measured on tmux
    3.7c, a 120x40 frame with `right` split first, grown to 200x40: `right` came back
    **62** columns and the table pane read **137**; after `resize-pane -x 22` on `right`
    the same pane read **177**, which is what :func:`repos_cols` says it is. Same
    agreement at 60, 110 and 300 columns.
    """
    out: dict[str, int] = {}
    for slot in slots:
        if _edge_of(slot) not in _COLUMN_EDGES:
            continue
        cells = _size_of(slot)
        if cells is not None:
            out[slot] = cells
    return out


def resize_flag(slot) -> str | None:
    """Which ``resize-pane`` axis re-asserts *slot*'s size, or ``None`` for a pane tmux is
    never told the size of — `commands_frame._apply_sizes`' filter.

    **Derived, because a hand-written table of per-slot facts is what this module stopped
    keeping.** :data:`SLOT_EDGE`, :data:`SLOT_SIZE`, :data:`_COLUMN_SLOTS`,
    :data:`_FIXED_ROW_SLOTS` and :data:`VARIABLE_ROW_SLOTS` are all `_derive`'s, built once
    at import from what each component declares, precisely so that a component charter did
    not write answers the same questions through the same two readers (:func:`_edge_of`,
    :func:`_size_of`) rather than being missing from a literal. `commands_frame` kept a
    sixth such table in its own module — ``{"top": "-y", "bottom": "-y", "right": "-x"}`` —
    and it was the one nothing derived.

    **What that cost, once Phase 5 made a `[[frame.component]]` table reachable.** A placed
    component travels under its own id (`chats`, `workspaces`, a provider's), which is in
    no literal here; :func:`slot_sizes` sizes it, :func:`harness_rows` CHARGES the harness
    for its rows, and `_apply_sizes` then issued no `resize-pane` for it at all. So the
    harness's explicit ``-y`` took those rows out of a neighbour and the one pane nothing
    asserts — the variable row slot — absorbed the whole error. Measured on tmux 3.7c, a
    frame with `chats` placed at `size = 1`, grown from 200x40 to 200x90 and put through
    the real `window-resized` handler::

        want   top 1   chats 1   harness 76   repos 7   bottom 1
        got    top 1   chats 7   harness 76   repos 1   bottom 1

    which is #515's own failure — two sizes swapping panes — in the one place #515 did not
    reach. It is stable rather than transient: three further resizes with no window change
    reproduced it exactly.

    **``None`` for the variable row slot, and that is the same decision it always was.**
    tmux's `resize-pane` moves exactly ONE boundary, so in a stack of N panes only N-1
    heights are free; assert all N and the outcome depends on the order. The pane left out
    is the one whose height is already a function of every other (:data:`VARIABLE_ROW_SLOTS`),
    the harness is told its height explicitly (:func:`harness_rows`), and the table lands on
    exactly :func:`repos_rows`' answer without anything naming it. Measured on tmux 3.7c at
    200x50, asserting `top`, `bottom` and `repos` in split order: the table came back **1**
    row tall and the attention strip **6**.

    **``None`` for a name nothing placed**, which is :func:`_edge_of`'s own contract read
    one step on: a slot charter knows nothing about falls out of both edge tests rather
    than being assigned an axis, and `_apply_sizes` skips it exactly as the literal's
    ``.get`` used to. The axis for the two it does know is the same one `panel_argvs`
    already encodes as `-v`/`-h` at split time, asked of the same :func:`_edge_of`.
    """
    if _key(slot) in VARIABLE_ROW_SLOTS:
        return None
    edge = _edge_of(slot)
    if edge in _COLUMN_EDGES:
        return "-x"
    return "-y" if edge in _ROW_EDGES else None


def _grown(sizes: dict[str, int], wanted: dict[str, int], *,
           window_rows: int) -> dict[str, int]:
    """*sizes* with each strip in *wanted* grown toward the rows its names need — as far
    as the harness can spare and no further (#829).

    **The budget is the harness's SURPLUS, and that is the whole of the row policy #740
    settled.** A strip growing is a strip taking rows off the harness, so what it may take
    is exactly what the harness has above :data:`HARNESS_MIN_ROWS` once every other pane
    has the height :func:`slot_sizes` just gave it — :func:`harness_rows` of the ungrown
    map, minus that floor. A window with nothing spare grows nothing, in silence, which is
    the same degrade `visible_slots` makes one rung down.

    **The repo table is NOT what pays**, and the ordering is `_DROP_ORDER`'s: the bars are
    given up before `repos` when rows run short, so they cannot be fed from it when rows
    are plentiful. `repos` already has its content's height by the time this is called and
    keeps it; the rows come out of the harness's own slack and the harness keeps its floor
    by construction, because that floor is what was subtracted to get the budget.

    **A row at a time, in *sizes*' own order, so a short budget is SHARED.** Two strips
    both wanting a second row with one row going spare is a real case — a plane that
    places both bars in a window with one row of slack — and handing it all to whichever
    was split first would leave the other looking broken for a reason nothing on screen
    explains. Round-robin gives the first strip the first row, the second the second, and
    both strips their second row before either gets a third.

    **The rounds are counted off the BUDGET rather than run under a `while`, and that is
    the deletion sweep's finding rather than a preference.** One round hands out at least
    one row while any demand and any budget remain, and no round may hand out a row the
    budget has not got — so ``range(budget)`` is more rounds than the deal can ever need,
    and a round past the last useful one is two guarded no-ops. Written as a `while` on
    ``budget > 0 and any(n > 0 …)`` instead, FIVE mutations of that one line do not
    terminate: dropping either conjunct, moving either boundary from `>` to `>=`, or
    swapping `any` for `all` each leaves a loop whose body cannot change its own condition
    (measured, with the state that hangs each one). A non-terminating mutant is CAUGHT —
    but the sweep can only report it as unmeasured, and it pays the full per-mutation
    timeout for each, which is what put nine other mutations of this branch out of time.
    The bound here is DATA, not a guard: it is what the deal costs, so removing it or
    moving it changes the answer rather than hiding a hang, and the two conditions below
    stay exactly as observable as they were.

    *wanted* is a number a CALLER measured, never a policy read here — `slots
    .bar_rows_wanted` is the measurement and `commands_frame._slot_sizes` is the one place
    it is made, for the reason :func:`repos_rows` gives about *pinned_rows*. So there is no
    :data:`BAR_MAX_ROWS` in this function: the ceiling is applied where the want is
    measured, and applying it twice would be a bound no input could make observable.

    Names *wanted* carries that *sizes* does not are dropped, on :func:`slot_sizes`'
    filter-don't-refuse discipline — a slot that is not being drawn is not a pane to grow.

    **This GROWS and never trims**, which is why the shortfall is tested for positivity in
    the loop rather than filtered out of the map: a plane that pinned its strip to three
    rows in its own `[[frame.component]]` table asked for three rows (#687), and a
    measurement saying its names fit on one is not that operator changing their mind. That
    is also what keeps #687 whole after #880 lowered the default to
    :data:`BAR_ROWS_DEFAULT`: a want of one against a pin of three leaves the three alone,
    so a plane that always wants a tall strip still says so once in its own file. There
    is no early return for an empty map either — the loop does not run and the answer is
    the map it was handed, so a guard in front of it is a line no input could make
    observable, which is the survivor `tools/sweep.py` reports and this repository deletes.
    """
    short = {slot: n - sizes[slot] for slot, n in wanted.items() if slot in sizes}
    budget = harness_rows(sizes, window_rows=window_rows) - HARNESS_MIN_ROWS
    out = dict(sizes)
    for _round in range(budget):
        for slot in short:
            if short[slot] > 0 and budget > 0:
                out[slot] += 1
                short[slot] -= 1
                budget -= 1
    return out


def slot_sizes(slots: list[str], *, window_rows: int, content_rows: int,
               pinned_rows: int | None = None,
               bar_rows: dict[str, int] | None = None) -> dict[str, int]:
    """Every slot in *slots* mapped to the size it should be given — rows for the
    horizontal strips, columns for the side.

    The one place `repos`' variable height and the other slots' fixed sizes are answered
    together, so a caller never has to know which kind a slot is. `panel_argvs` splits
    with it, `commands_frame._reassert_sizes` re-applies it, and the `window-resized`
    recompute calls it again with the window's NEW row count — which is the whole reason
    it takes *window_rows* rather than closing over a launch-time value.

    *pinned_rows* is :func:`repos_rows`' and is carried rather than read, for that
    function's reason: this is the last hop of a value the plane committed, not a second
    place to look it up. `None` — the default, and what every caller asking about the
    arithmetic passes — is a plane that pinned nothing, which is charter's own and very
    nearly everyone's.

    *bar_rows* is the same kind of number one slot family over: how many rows a slot's own
    content needs at its own pane width, measured by `slots.bar_rows_wanted` and carried
    here rather than read, for exactly *pinned_rows*' reason. It may name every slot in
    *slots*, and does: a slot with no strip to draw answers `1`, which is at or under every
    slot's own height, so :func:`_grown` spends nothing on it. `None` — the default, and
    what every caller asking about the arithmetic passes — is a frame with no strip that
    overflows, which is every frame that places no bar and every plane whose names fit.
    :func:`_grown` is what spends it, and what refuses to spend rows the harness has not
    got.

    Unknown slot names are dropped rather than raised on, matching `visible_slots`'
    filter-don't-refuse discipline: `[frame] slots` is committed, untrusted input, and by
    the time a list reaches here it has already been through `instance.FRAME_SLOTS`.
    """
    out: dict[str, int] = {}
    for slot in slots:
        if _key(slot) in VARIABLE_ROW_SLOTS:
            out[slot] = repos_rows(content_rows=content_rows, pinned_rows=pinned_rows,
                                   window_rows=window_rows, slots=slots)
            continue
        cells = _size_of(slot)
        if cells is not None:
            out[slot] = cells
    return _grown(out, bar_rows or {}, window_rows=window_rows)


def harness_rows(sizes: dict[str, int], *, window_rows: int) -> int:
    """The rows left for the HARNESS pane once every horizontal strip in *sizes* has its
    own height and its own border — the number `commands_frame._reassert_sizes` asserts
    on the harness itself after a resize.

    **Why the harness is resized at all, which it never used to be.** tmux redistributes
    every pane proportionally on a window resize, so the intended sizes have to be
    re-applied; and `resize-pane -y` moves ONE boundary — the one below the pane, or the
    one above it when the pane is last in the stack. With two strips (`top` above the
    harness, `bottom` below it) every `resize-pane` therefore traded with the harness and
    asserting both was enough. #515 makes three, and the two below the harness trade with
    EACH OTHER instead: measured on tmux 3.7c at 200x50, asserting `top`, `bottom` and
    `repos` in split order left the table 1 row and the attention strip 6 — each
    assertion undoing the last, with the harness never consulted.

    So the harness is told its height explicitly and the variable slot
    (:data:`VARIABLE_ROW_SLOTS`) is left to take the remainder. Verified against tmux
    3.7c at 200x50, 200x24, 200x100 and 90x40, with and without the sidebar, and in both
    orders that put the harness before or after the strip below it: the table lands on
    exactly `repos_rows`' answer every time.

    *sizes* is :func:`slot_sizes`' map. Slots in :data:`_COLUMN_SLOTS` cost columns, not
    rows, and are not counted — the same rule :func:`repos_rows` keeps, read from the same
    constant. Floored at 1: a zero or negative `-y` is a resize tmux refuses, and a
    refused resize leaves the frame as tmux's own redistribution left it, which is worse
    than a harness squeezed to one row in a window that has no rows for it anyway.
    """
    used = sum(n + _BORDER_ROWS for slot, n in sizes.items()
               if _edge_of(slot) not in _COLUMN_EDGES)
    return max(1, window_rows - used)


#: What a frame's window runs while charter is still setting it up, before the harness
#: replaces it. `cat` with no arguments blocks on its own stdin forever and never exits
#: on its own, which is the ONLY property required of it: `window_argv`'s docstring
#: explains why the pane has to exist, and keep existing, before `remain-on-exit` can be
#: set on it. A separate argv element, never a shell string, like everything else here.
PLACEHOLDER = ["cat"]


def _tmux(socket: str, *args: str) -> list[str]:
    """*socket* is charter's own server NAME or the operator's socket PATH — see
    `tmuxctl.server_argv`, the one place that difference becomes `-L` or `-S`."""
    return tmuxctl.server_argv(socket, *args)


def window_argv(*, socket: str, session: str, window: str, cwd: str) -> list[str]:
    """The `new-window` that puts a frame in the operator's OWN tmux, not under it.

    Detached (`-d`) and appended after the current window (`-a`): the operator is
    switched to the frame once it has actually been built (`select-window`, from the
    launcher), never onto a half-drawn window still running the placeholder.

    *session* is the operator's own session, `$N` shaped, read out of `$TMUX` by
    `tmuxctl.operator_server` — measured against tmux 3.7c, `new-window -t` refuses a
    PANE id outright ("can't specify pane here"), so the session is the target that
    works and the one `$TMUX` already carries.

    *window* is the frame id, which is also what the launcher reaps against: charter's
    frames on the operator's server are windows, not sessions, so `list-windows -a -F
    '#{window_name}'` is the liveness list there that `list-sessions` is on charter's
    own server.

    Asks for BOTH ids (`-P -F '#{window_id} #{pane_id}'`). The pane id scopes every
    split and every status query, exactly as on the private-server path; the window id
    is what `kill-window` targets when the harness is done — an index would be wrong for
    both, since tmux renumbers windows on every close just as it renumbers panes on
    every split (see this module's own docstring).

    The window is created running :data:`PLACEHOLDER` rather than the harness, and that
    ordering is the whole point. `remain-on-exit` is what keeps a dead harness pane
    askable long enough to read its exit status out of, and tmux has no way to set it on
    a pane that does not exist yet: the options a new pane would inherit it from are
    global or session-scoped, and writing either on somebody else's server is precisely
    what this path exists not to do. So the pane is created with a program that cannot
    exit, `remain-on-exit` is set ON that pane, and `respawn_argv` then puts the harness
    in it. The install race the private-server path narrows with `_PLACEHOLDER_CONF` is
    not narrowed here; it is removed.

    *cwd* is `-c`, for the same reason `respawn_argv` takes one — see its docstring.
    """
    return _tmux(socket, "new-window", "-d", "-a", "-t", session, "-n", window,
                 "-c", cwd, "-P", "-F", "#{window_id} #{pane_id}", "--", *PLACEHOLDER)


def respawn_argv(*, socket: str, harness_pane: str, env: dict[str, str],
                 cwd: str, harness_argv: list[str]) -> list[str]:
    """`respawn-pane -k`: the harness replaces the placeholder, in the SAME pane.

    Verified against tmux 3.7c that the pane keeps its `%N` id across the respawn, which
    is what lets `window_argv`'s reported id go on scoping the splits and the
    exit-status query afterwards.

    *env* rides on `-e`, one `NAME=VALUE` argv element each, sorted so the command is
    the same on every launch. This is the only way charter's own variables
    (`CHARTER_SESSION_ID`, `CHARTER_HARNESS`) can reach the harness here. The
    alternative tmux offers — `set-environment -t <session>` — would reach the harness
    AND hand every new shell the operator opens in that session a frame id that is not
    theirs, which is the identity collision
    `docs/superpowers/specs/2026-08-21-harness-wrapper-design.md` removed `WINDOWID` for.

    **This used to add "the private-server path gets them because `new-session` starts
    the server and the server inherits the launcher's environment", and that was #411.**
    It is true only of the launch that actually starts the server; every later frame on
    charter's shared private server finds it running and inherits the FIRST launcher's
    environment instead. `session_argv` now carries the same `-e` for the same reason —
    see its docstring for what was measured.

    **`respawn-pane -e` ADDS to the pane's environment; it does not replace it.** This
    said the opposite ("REPLACES ... so everything the harness needs is on THIS call"),
    which is why this call carries `dict(os.environ, ...)` whole. Re-measured against tmux
    3.7c: a server started with `FOO`/`BAZ` in its environment, respawned with only
    `-e BAR=…`, produced a pane holding all three and a full-length `$PATH`. So the `-e`
    set is an OVERLAY on whatever the pane would have inherited anyway.

    That matters twice. It means a caller may carry only the variables that must differ
    (which `session_argv` and `panel_argvs` on charter's own server already did — see
    `commands_frame._frame_identity_env`, and note that a `-e` is argv and argv is
    world-readable). And it meant this call site's full-environment pass was a CHOICE
    rather than a requirement.

    **That choice is gone, and #446 is why.** This call carried `dict(os.environ, …)`
    whole into `/proc/<pid>/cmdline`: measured on one real environment, 129 argv elements,
    7,773 bytes, four live 1Password service-account tokens and an npm auth token. What it
    was buying was "the base the overlay lands on is THEIR tmux server's environment, which
    may predate this plane entirely". Two things were then measured against tmux 3.7c and
    settle it (`commands_frame._guest_harness_env` carries the numbers):

    * a pane's `$PATH` is the INVOKING CLIENT's, not the server's — charter's own, the
      one `cmd_launch`'s `shutil.which` resolved the harness against. An explicit
      `-e PATH=…` does not even survive: tmux overwrites it afterwards.
    * everything else a harness reads it reads for itself, from a server environment that
      belongs to the same operator on the same machine.

    So only `commands_frame._guest_harness_env` travels here. The cost is stated in
    `docs/frame.md` beside the other named costs of being a guest: the harness inherits
    the operator's TMUX SERVER environment rather than their current shell's — exactly
    what already happens on charter's own shared server. Do not put a credential on that
    command line: it is world-readable, and this function makes no promise about it.

    Every `-e` lands before the `--` — they are `respawn-pane`'s own options, and must
    never be grafted onto the harness's own argv.

    *cwd* is `-c`, and it is not optional. A pane in a server charter did not start
    inherits its start directory from the SESSION's, which is wherever the operator
    happened to be when they first ran `tmux` — days ago, in another repo. `charter
    claude` has to run the harness where it was typed, and the panels split off this
    pane inherit its directory in turn, which is what `workspace.resolve()` reads.
    """
    return _tmux(socket, "respawn-pane", "-k", "-t", harness_pane, "-c", cwd,
                 *_env_argv(env), "--", *harness_argv)


def session_argv(*, session: str, conf: str, socket: str, cols: int, rows: int,
                 harness_argv: list[str], chat: str | None = None,
                 env: dict[str, str] | None = None) -> list[str]:
    """The `new-session` that starts the workspace's tmux session, first chat inside it.

    Detached (`-d`): this is launched from a script with no tty to hand tmux, not typed
    interactively, and without `-d` tmux would attach and the call would never return.

    Asks tmux to PRINT the new pane's id (`-P -F '#{pane_id}'`) rather than assuming one.
    That id is the whole fix the module docstring describes: the caller must capture it
    off stdout and pass it to `panel_argvs`, because a `session:0.0`-style index stops
    naming the harness after the very first split.

    *env* rides on `-e`, exactly as it does for :func:`respawn_argv` and
    :func:`panel_argvs`, and **#411 is what it is here for.** It used to be absent
    because passing the launcher's environment to the tmux CLIENT was believed to be
    enough — "`new-session` starts the server and the server inherits the launcher's
    environment", as `respawn_argv`'s own docstring put it. That is true of the launch
    that STARTS the server and of no other: every frame on charter's private server
    shares one server (see `commands_frame`'s module docstring), so a second frame's
    `new-session` finds it already running, and tmux builds the new pane's environment
    from the SERVER's global environment — captured from whichever launcher happened to
    start it, possibly days ago.

    Measured against tmux 3.7c, two frames on one private socket, reading the harness
    shell's own `$CHARTER_SESSION_ID`: the second frame's harness reported
    ``default-58069`` while its own session was ``default-58696``, and
    ``show-environment -g CHARTER_SESSION_ID`` held the first frame's id. Everything
    keyed on that variable then went to the wrong frame — `charter ws use` wrote the
    first frame's workspace pointer, and every hook bumped the first frame's version, so
    the second frame's panels were never told anything had changed. The panels
    themselves were already right: `commands_frame._session_id_env_argv` ties the id to
    the SESSION, which `split-window` honours (measured on the same server), so this
    closes the one pane that call cannot reach — the one `new-session` itself creates.

    ``None`` means "carry nothing", which is what a tmux below
    `tmuxctl.SESSION_ENV_FLOOR` gets: `-e` is not a flag `new-session` degrades on, it is
    one it refuses, and refusing takes the whole launch with it.

    *chat* names the window, the way :func:`chat_window_argv` and :func:`window_argv`
    name theirs — so the first chat of a workspace is legible in `tmux list-windows`
    beside its siblings instead of showing whatever `automatic-rename` made of the
    harness's process name. It is a READOUT and nothing reads liveness from it: measured
    on tmux 3.7c and on 3.2, `-n` does pin the name (it turns that window's
    `automatic-rename` off) and `allow-rename on` still lets the pane's own output take
    it back, which is why `commands_frame._CHAT_OPTION` exists. ``None`` for a caller
    with no chat to name, which is the only reason it is optional.
    """
    named = ("-n", chat) if chat else ()
    return _tmux(socket, "-f", conf, "new-session", "-d", "-s", session, *named,
                "-x", str(cols), "-y", str(rows), "-P", "-F", "#{pane_id}",
                *_env_argv(env),
                "--", *harness_argv)


def chat_window_argv(*, socket: str, session: str, chat: str, cwd: str,
                     harness_argv: list[str],
                     env: dict[str, str] | None = None) -> list[str]:
    """The `new-window` that adds a CHAT to a workspace's session on charter's own server.

    :func:`session_argv`'s sibling, for the launch that finds the workspace's session
    already running. The session is the workspace; a chat is one window in it; the
    harness runs in that window's first pane. Everything the two calls have in common is
    common on purpose — the reported pane id, the `-e` overlay, the harness after `--` —
    so a launcher does not have to know which of them started its frame.

    Detached (`-d`), like :func:`window_argv` and for the same reason: a client already
    attached to this session must not be dragged to a half-built window. The launcher
    `select-window`s once the frame has actually been drawn.

    **`-a`, and it is not decoration.** `-t <session>` resolves to a WINDOW, and
    `new-window` reads a resolved window target as the INDEX to create at — so without it
    tmux answers `create window failed: index 0 in use` for the second chat of a workspace
    and no chat is ever added. Measured against tmux 3.7c and tmux 3.2, which is also how
    :func:`window_argv` came to carry it.

    **Which window it resolves to is not "the session's current one", and that correction
    was measured for §4b.** tmux matches a bare target against WINDOW NAMES too, and a
    name match wins: a session `ws` whose current window is index 0 (`zzz`) resolves `-t
    ws` to index **1** when index 1 is named `ws.9`, on both versions. Charter names every
    chat window `<workspace>.<n>` — so on charter's own server a bare `-t <workspace>`
    names *a chat's window*, chosen by name matching rather than by what is on screen, and
    which chat that is moves as chats are opened and closed. Nothing in charter relies on
    it: this call passes `-a`, and every other window-scoped target charter builds is a
    `%<pane>` id (`_chat_option_argv`, `_plane_option_argv`, `overlay.arm_hatch_argv`,
    `cmd_chat`'s `select-window`). The claim is corrected here rather than deleted because
    the next person to write `-t <session>` will reach for the reason this paragraph gives.

    ``-P -F '#{pane_id}'`` and NOT the window id, deliberately: `session_argv` reports the
    pane alone, every caller downstream scopes itself to the pane (`split-window`,
    `set-hook -p`, `set-option -w -t <pane>`, `kill-window -t <pane>` — measured on tmux
    3.7c and 3.2 to resolve to that pane's own window), and a second reported field would
    be a second shape for `cmd_launch` to branch on for nothing.

    *env* rides on `-e` through :func:`_env_argv`, and **this is the whole mechanism that
    makes identity per chat.** Measured on tmux 3.7c and on tmux 3.2, one session with a
    session-wide ``set-environment CHARTER_SESSION_ID session-wide`` under it: a window
    created with ``-e CHARTER_SESSION_ID=chat-A -e CHARTER_HARNESS=codex`` reported
    ``chat-A codex`` from inside its own pane on both versions. `-e` on a window is
    available at `tmuxctl.PANE_ENV_FLOOR` (3.0), below the floor charter warns at, so
    unlike `new-session -e` there is no version below which this has to be withheld.

    *cwd* is `-c` for :func:`respawn_argv`'s reason: a new window's start directory would
    otherwise be the SESSION's, which is wherever the launcher that created the workspace
    happened to be — and the panels split off this pane inherit its directory in turn.
    """
    return _tmux(socket, "new-window", "-d", "-a", "-t", session, "-n", chat, "-c", cwd,
                 "-P", "-F", "#{pane_id}", *_env_argv(env), "--", *harness_argv)


#: Every environment variable name charter will ever put on a tmux command line, and the
#: whole of it.
#:
#: **The list is the promise, and it lives HERE because this is the only place a `-e`
#: is built.** #412 narrowed one call site (`session_argv`) and #446 was the other one
#: (`respawn_argv`) still passing `dict(os.environ, …)` whole — the same defect, found a
#: release apart, because the rule was enforced at the call sites rather than at the
#: single funnel every call site goes through. :func:`_env_argv` refuses anything else
#: outright, so the next call site cannot quietly leak the way that one did: a wide
#: environment is a loud `ValueError` in the test that first builds it, not 7,773 bytes
#: of `/proc/<pid>/cmdline` on somebody's laptop.
#:
#: Names, never a `CHARTER_` prefix glob, for `commands_frame._FRAME_IDENTITY`'s own
#: reason: a prefix would keep the promise for a variable nobody has invented yet.
#: `PATH` is the one non-charter name and `commands_frame._guest_harness_env` is where
#: its measurement is written down. Nothing here is ever a credential — that is the
#: property that makes an argv acceptable at all.
CARRIABLE = frozenset({
    "CHARTER_SESSION_ID", "CHARTER_HARNESS", "CHARTER_ROOT", "CHARTER_WORKSPACE",
    "CHARTER_PERSONA", "PATH",
})


def _env_argv(env: dict[str, str] | None) -> list[str]:
    """`-e NAME=VALUE` per entry, sorted so the command is the same on every launch.

    One argv element each, never a joined string, and always before a command's `--`:
    these are tmux's own options, not something to graft onto the program's argv. Empty
    for an empty (or absent) *env*, so a call that has nothing to carry produces exactly
    the command it always did.

    **Every name must be in :data:`CARRIABLE`.** Raising is the point: the alternative —
    dropping the extras quietly — would let a caller believe it had handed the harness a
    variable that never arrived. A silent drop makes this guard's own failures invisible;
    it does not make them rarer. Only NAMES appear in the message; a value that does
    not belong on a command line does not belong in a traceback either.
    """
    unlisted = sorted(set(env or {}) - CARRIABLE)
    if unlisted:
        raise ValueError(
            f"tmux `-e` may only carry {sorted(CARRIABLE)} — refusing "
            f"{len(unlisted)} other name(s): {unlisted}. A `-e` is argv, and argv is "
            "world-readable; see frame/layout.CARRIABLE")
    return [x for name in sorted(env or {}) for x in ("-e", f"{name}={env[name]}")]


def panel_command(*, slot: str, session: str) -> list[str]:
    """The command one panel pane runs — the part after `split-window`'s own `--`.

    *slot* is a component NAME: one of the four committed slot names, the id of the
    built-in behind one, or the id of a component an installed provider supplies. It is
    the argument `frame/panel.py:run` resolves, and the keyword keeps its old spelling
    because `commands_frame.cmd_respawn` and every test that pins this argv byte for byte
    already say it.

    Split out of :func:`panel_argvs` because a panel is started TWICE by two different
    modules: once by the launcher's `split-window`, and again by
    `commands_frame.cmd_respawn`'s `respawn-pane` after the pane's `pane-died` hook
    fires (#382). Two hand-written copies of this argv is exactly the drift that ends
    with a respawned panel running a slightly different command from the one the
    launcher spawned — a stale flag, a missing `--session` — and failing in a way that
    only ever reproduces after something has already died once.

    **The interpreter half is built here too, not passed in.** This function used to
    take a *charter_argv* and both callers handed it one, which left the ONE part of
    the argv that actually differs between them outside the shared helper — and it
    promptly drifted: the launcher moved to `util.self_relaunch_argv()` for #390's `-P`
    while `cmd_respawn`, written against the same seam a release earlier, kept a
    hand-built `[sys.executable, "-m", "charter"]`. That is #390's own failure with a
    delay fuse on it: `respawn-pane` starts the new process in the PANE's cwd, which for
    anyone dogfooding charter is a charter checkout, so the respawned panel would import
    that tree rather than the installed one and die again — this time for a reason
    nothing in the frame could show. Owning the whole argv here is what makes the
    extraction actually deliver the no-drift property it was created for: there is no
    longer a parameter for a caller to get wrong.
    """
    return [*util.self_relaunch_argv(), "panel", slot, "--session", session]


def panel_argvs(*, slots: list[str], session: str, socket: str,
                harness_pane: str,
                env: dict[str, str] | None = None,
                sizes: dict[str, int] | None = None) -> list[list[str]]:
    """One `split-window` per slot in *slots*, each carving its rectangle off *harness_pane*.

    *harness_pane* is the id `session_argv`'s caller read off tmux's stdout, and every
    split below targets that same id — never a `session:0.0`-style index, and never a
    target derived from an earlier split in this same list. Both are the mistake the
    module docstring measures: indices move under every split tmux runs, pane ids don't.

    Also asks tmux to PRINT the new pane's id (`-P -F '#{pane_id}'`, the same flags
    `session_argv` uses for the harness pane, placed the same way — before `--`, so they
    are `split-window`'s own options and never touch the `charter panel …` argv after
    it). A caller that keeps the id for each fixed-size slot can re-assert its size on a
    `window-resized` hook (see `commands_frame._resize_hook_argv`): tmux's own layout
    engine redistributes EVERY pane proportionally on a resize, `-l size` notwithstanding
    — measured against tmux 3.7c, growing a 120x30 frame to 200x50 stretched two
    one-row panels to 8 and 7 rows, snapping back only because the resize happened to be
    an exact round trip. Without the id, nothing later could target the RIGHT pane to
    correct that (an index would renumber under the very next split, same failure the
    module docstring already measures for the harness pane).

    *env* is `-e`, and it carries charter's identity and nothing else on EITHER server.
    The sentence here used to say that on somebody else's server it "carries the
    launcher's environment whole", and #446 is that sentence: a panel needs the plane it
    draws stated (`$CHARTER_ROOT`, `$CHARTER_WORKSPACE` — it inherits that server's
    otherwise, whatever their shell had when they first ran `tmux`), and it needs nothing
    else. A panel starts no subprocess at all — `frame/gather.py`, `frame/slots.py` and
    `frame/panel.py` are pure file and state reads — so it has no use for `$PATH` either,
    and its own interpreter is already absolute (`panel_command`, #390).

    **On charter's own server it carried charter's identity and nothing else already, and
    the sentence that used to be here was wrong.** It said the panels inherit the launcher's
    environment "because `new-session` is what starts that server" — true of the launch
    that starts it and of no other (#411; see :func:`session_argv`). Every later frame's
    panels inherited the FIRST launcher's, and `$CHARTER_WORKSPACE` is the sharp one:
    `workspace.resolve` ranks it above every pointer, so a panel that inherited another
    launcher's pin could not be moved by `charter ws use` at all. `$CHARTER_SESSION_ID`
    was already correct there by a different route — `commands_frame._session_id_env_argv`
    ties it to the SESSION, which tmux applies to panes split later — and this fixes the
    rest of the identity alongside it.

    Only `commands_frame._FRAME_IDENTITY` travels, never the whole environment: a `-e` is
    argv, and argv is world-readable. See `commands_frame._frame_identity_env`.

    *sizes* is :func:`slot_sizes`' answer for this window, and it exists because `repos`
    is not a fixed height (#488). ``None`` falls back to :data:`SLOT_SIZE`, which is
    `repos`' FLOOR — right for a caller that has no window to measure (a test, a
    `--probe`), wrong for a launch, which is why both launch paths pass one. Read with a
    per-slot fallback rather than replaced wholesale, so a *sizes* missing an entry
    degrades to the fixed size instead of a `KeyError` inside a launch.

    **`-b` is `top`'s alone, and the rest of the reading order is the split order run
    backwards.** Every other split is a plain `-v`, which tmux places DIRECTLY below
    the harness — so a slot split later sits ABOVE one split earlier. Measured on tmux
    3.7c in a 120x40 window, splitting `top`, `bottom`, `repos`, `right` off the
    harness in that order: `top` at row 0, harness and `right` at rows 2-30, `repos` at
    rows 32-37, `bottom` on row 39. That is the frame #515 asks for — identity, the
    session, the repo table, the attention strip on the terminal's last row — and it is
    why the shipped `slots` list names `bottom` before `repos` and not after it.
    """
    cmds: list[list[str]] = []
    for slot in slots:
        size = (sizes or {}).get(slot)
        if size is None:
            # `SLOT_SIZE[slot]` is still the last resort, and still a `KeyError` for a
            # name nothing placed: a caller has asked for a pane charter cannot size, and
            # splitting one anyway is the permanently-dead rectangle `_drawable_slots`
            # exists to prevent.
            size = _size_of(slot)
            if size is None:
                size = SLOT_SIZE[slot]
        # The component's EDGE, not a second list of names: which splits take COLUMNS is
        # exactly what :func:`repos_cols` has to know to answer how wide `repos` ends up,
        # and two copies of that fact are two things to keep in step. :data:`_COLUMN_SLOTS`
        # is that same predicate over the SHIPPED frame; asking the edge is what also
        # answers it for a component this plane placed.
        edge = _edge_of(slot)
        direction = "-h" if edge in _COLUMN_EDGES else "-v"
        # `-b` is the component's EDGE, not the name `top` — see :data:`_BEFORE_EDGES`.
        # A slot charter has no component for keeps the plain `-v`/after placement it had
        # while this was spelled `slot == "top"`, which is the same filter-don't-refuse
        # degrade `slot_sizes` makes one line above.
        before = ["-b"] if edge in _BEFORE_EDGES else []
        cmds.append(_tmux(socket, "split-window", "-t", harness_pane,
                          direction, *before, "-l", str(size),
                          *_env_argv(env),
                          "-P", "-F", "#{pane_id}", "--",
                          *panel_command(slot=slot, session=session)))
    return cmds
