"""What each edge of the frame says.

Content comes from the renderers `statusline.py` already has — they are composed here,
never rewritten, so a fix to a repo row or an alert lands in both surfaces at once.

**A panel measures its own pane, never `$COLUMNS` — and not as a fallback either.** A
panel process is started as a tmux pane command, which inherits the *launching* shell's
environment whole. Measured: a tmux pane 22 columns wide, launched from a shell that had
exported `COLUMNS=200`, ran a probe that saw `COLUMNS env='200'` while its real tty was
22 columns. `charter.tui.term_width()` reads `$COLUMNS` first — correctly, for the status
line, where stdout is a pipe and the environment is the only source of the truth. That
order is exactly wrong here: trusting it would lay every panel out at the OUTER
terminal's width and wrap catastrophically inside its own narrow pane. `tui.term_width()`
itself is left alone (the status line depends on its env-first order); :func:`_width`
asks the pane's own tty directly.

**The last resort is a constant, not the environment** (#591). :func:`_width` used to
fall through to `tui.term_width()` when the pane could not be measured, which put
`$COLUMNS` back on the path by the back door: a panel whose stdout has no tty behind it
laid itself out at the launching terminal's width, and `panel._hold` — the failure paint,
which runs precisely when something has already gone wrong — painted a 400-column line
into a 24-column pane. :data:`_DEFAULT_COLS` is what it answers instead, beside
:data:`_DEFAULT_ROWS`, which has always been a constant for exactly this reason. Both
halves of a pane's size now come from the same place and neither comes from the shell.

**And the pane is CLAIMED rather than looked up** (#606). Measuring "the descriptor this
process is writing to" was written as `sys.stdout.fileno()`, which is a mutable global any
library the process imports may replace: with Textual's `redirect_stdout` installed, a real
150x10 pane measured 80x24 here and nothing raised. `frame/pane.py` is the one place that
answers which rectangle this process was given, and both halves below take one reading of
it — see that module for the property, and for why a size that cannot be taken is `None`
rather than a plausible number.
"""

from __future__ import annotations

import bisect
import os
import time
from typing import NamedTuple

from .. import contain, tui
from . import pane

#: The spinner's own frames, and how long each is held. Ten braille cells, each one
#: column wide (`unicodedata.east_asian_width` is `N` for U+2800–U+28FF, so `tui.width`
#: counts them as 1) — nothing here is drawn wider than the static `⋯` it falls back to.
#:
#: **Zero runtime dependencies, and no animation loop either.** The frame is chosen from
#: the clock rather than advanced by a counter (:func:`spinner_frame`), so nothing has to
#: own the spinner's state, nothing has to be reset when a panel repaints for an unrelated
#: reason, and two panels drawing at the same moment necessarily draw the same frame.
#:
#: The period matches `panel.TICK`: a panel that is animating repaints once per tick, so a
#: shorter period would name frames nobody ever sees and a longer one would repaint
#: without changing anything.
SPINNER = "⠋⠙⠹⠸⠼⠴⠦⠧⠇⠏"
SPINNER_PERIOD = 0.2

#: How many rows the full-height `right` panel draws at `terse`, and how many rows of
#: repo table `repos` keeps there. `top` and `bottom` have no equivalent — each is one
#: row at every density (`layout.SLOT_SIZE`), so what "less" means for them is FIELDS,
#: not rows.
#:
#: **`repos` is where the rows are, and #515 is why that is one slot rather than two.**
#: #488 had `bottom` carrying both, so `terse` meant "one field on the attention row AND
#: this many table rows" — one level name governing two unrelated budgets in one pane.
#: Split apart, `terse` limits `_bottom` to a single field and limits this pane to this
#: many rows, and a density that buys back rows still buys them from the slot that
#: actually has rows to give.
_TERSE_ROWS = 4

#: Rows a bordered section spends on its own `▪ <label> N` heading before it has said
#: anything about its content — `_sidebar_head`, one row.
#:
#: **One constant because three places did this arithmetic and one of them is in another
#: module** (#740). :func:`repos_rows_wanted` adds it to the rows the table wants,
#: :func:`_repos` subtracts it from the pane it measures, and `layout._table_min_rows`
#: reads :data:`_TABLE_MIN_ROWS` below to decide whether a pane is worth splitting at all.
#: Spelled as `1 +` and `- 1` in the first two and as a literal in the third, the day the
#: heading gains a row is the day the launcher sizes a pane the renderer then overflows —
#: which is the sizer-and-renderer disagreement #500 shipped twice on the other axis.
_HEAD_ROWS = 1

#: The shortest `repos` pane that can say anything about a repo — its heading plus one
#: table row.
#:
#: **Read by `layout.visible_slots` (through `layout._table_min_rows`), which is what
#: stops a shorter one being split at all.** #740: at 120x10 the frame spent three rows
#: — two tmux rules and `▪ repos 8` — on a count, having just decided it could not afford
#: the one row that names the workspace and the persona. `_table_lines` answers `[]` on a
#: budget of zero, so that pane was a label for content there was no room for; below this
#: the slot now goes the way `right` and the two bars go.
#:
#: The other axis of `layout._table_min_cols`, which reads `statusline._LEFT_W` for the
#: identical reason: the number the RENDERER stops drawing at and the number the LAUNCHER
#: stops splitting at must be one number.
_TABLE_MIN_ROWS = _HEAD_ROWS + 1


def spinner_frame(now: float | None = None) -> str:
    """Which frame of :data:`SPINNER` this instant shows.

    Read off `time.monotonic()` rather than advanced by a caller — see :data:`SPINNER`.
    *now* is for tests, which need a specific frame rather than whichever one the clock
    happened to be on.
    """
    t = time.monotonic() if now is None else now
    return SPINNER[int(t / SPINNER_PERIOD) % len(SPINNER)]


def tab_spinner_frame(now: float | None = None) -> str:
    """Which frame of :data:`TAB_SPINNER` this instant shows.

    :func:`spinner_frame`'s twin over the other sequence, and a second function rather
    than a *seq* parameter on the first: a caller that could pass a sequence could pass
    :data:`SPINNER` — braille, which is not held to this row's width rule — and the whole
    of :data:`TAB_SPINNER`'s docstring is about which characters may go on a strip whose
    click map is per column. One name, one sequence, one place the rule is written down.

    Every tab spinning on a strip shows the SAME frame at any instant, because both read
    the clock rather than a per-tab counter. That is the readable answer as well as the
    cheap one: a row of tabs pulsing out of step reads as noise, and a row pulsing
    together reads as *these two are working*.
    """
    t = time.monotonic() if now is None else now
    return TAB_SPINNER[int(t / SPINNER_PERIOD) % len(TAB_SPINNER)]


def verbosity(fid: str) -> str:
    """How much this frame's panels say: ``"terse"`` or ``"normal"``.

    Two sources, in one order that is the whole of #387's "the hotkey overrides for the
    running frame only": the frame's OWN recorded density (`state.density`, written by
    the palette's density rows and by nothing else) first, and `[frame] density` from
    charter.toml
    behind it. A frame nobody has touched reads the configured value; a frame whose
    operator has pressed the hotkey reads their choice, for as long as that frame runs and
    not one moment longer — `state.reap` deletes the file with the rest of the directory.

    **The override is VALIDATED before it is allowed to win, not merely read.** A plain
    `state.density(fid) or config.FRAME["density"]` looks equivalent and is not: the file
    holds text, so a truncated write or a hand edit gives a non-empty value that is not a
    level, which is truthy — it beats the configured value and then degrades to
    `DEFAULT_VERBOSITY` at the last step. The operator's own `[frame] density = "minimal"`
    would be silently discarded by a corrupt byte in a file they have never heard of.
    Asking `density_level` first makes "not a level" and "nothing recorded" the same
    thing, which is what they mean here.

    `instance.verbosity_for` is what turns whichever survives into a verbosity, so an
    unknown level from either source degrades identically. Read at call time, never
    cached: a panel repaints on a version bump, and a density change bumps the version
    precisely so that this is re-read.
    """
    from .. import config, instance
    from . import state
    override = instance.density_level(state.density(fid))
    return instance.verbosity_for(override or config.FRAME["density"])


def _frame_workspace(fid: str) -> str:
    """Which workspace THIS FRAME is drawing — `state.workspace_for`, which owns the rule.

    **A panel that re-resolves the workspace gets a different answer, and #512 is what
    that costs.** `state.record_workspace`'s docstring has the full walk through
    `workspace.resolve`'s rungs and why a panel process reaches none of the ones that
    ordinarily decide it; the short version is that a panel falls all the way to the
    declared default while the launcher — one ordinary shell, one rung up — resolved
    something else. On the plane that reported #512 that gap was the whole bug: `bottom`
    drew `default`'s empty repo list into a pane the launcher had sized for
    `harness-wrapper`'s three rows, and the rows only appeared once the first tool call's
    hook refreshed the cache from inside the HARNESS, which resolves it correctly.

    Kept as a named function here rather than inlined at the three call sites, because
    what it means is a `slots` fact: every panel goes through it — `_top` names the
    workspace, `_bottom` counts its todos and its alerts, `_repos` draws its table — so
    the things a frame says about "where am I" cannot disagree with each other. That is a
    failure an operator reads immediately: a header saying `default` above a table listing
    another workspace's repos.
    """
    from . import state
    return state.workspace_for(fid)


def _sidebar_live(fid: str) -> bool:
    """Is the `right` panel on screen in THIS frame, right now?

    Asked so `_top` can stop repeating what the sidebar already says (#530) — and asked
    LIVE, on every repaint, because the answer moves while the frame runs. #387's density
    hotkey and `commands_frame.cmd_density` re-lay-out a running frame, and `right` is the
    first slot `layout.visible_slots` drops on ANY shortage, so a value decided once at
    launch is wrong the moment the operator presses a key or resizes the window.

    **`state.panes` is the record, and it is what a PANEL can answer this from.** It is
    written where a frame's shape is actually DECIDED — by `_draw_panels` at launch and
    again by `cmd_density` after `_relayout` — from the panes tmux really gave back, so a
    slot whose `split-window` failed is absent from it exactly like a slot the density
    dropped. The alternatives are all worse in the same direction: `instance
    .density_slots` is what was *asked for* rather than what is *there*, `[frame] slots`
    is what the operator configured, and an environment variable is whatever was true at
    launch — the one thing the docstring above says it must not be.

    **tmux CAN be asked now, and the sentence that used to be here said it could not**
    (#714). `commands_frame._PANEL_SLOT_OPTION` puts the component's id on each panel pane,
    so `list-panes` answers which pane charter meant as `right` — which is exactly what a
    re-layout reconciles against, because a record rewritten whole on every re-layout could
    not be the authority there.

    It is not what this function should ask, and the reason is the shape of the question
    rather than the cost. A panel is a `charter panel` PROCESS in a pane of its own: it
    would be asking the server about a sibling that may not have been split yet. `right` is
    split last on the shipped frame, so `top`'s first paint can precede that pane
    EXISTING — no reading of tmux, however authoritative, turns that into `True`. #748 was
    that race, and reading tmux here would have moved which file it was against and closed
    nothing.

    **#748 is closed on the WRITING side, and this function is unchanged by it.**
    Measured on real launches: 16 of 90 on an idle machine and 16 of 25 on a loaded one
    drew the roster here and kept it, because the launch recorded the frame's shape
    without moving the frame's version and `top` — not in :data:`ANIMATED` — had no reason
    to ask a second time. The pane's own history is the proof rather than the rate: on
    every one of those launches `top` painted exactly ONCE. Neither of the
    two remedies #748 proposed shipped: deferring the first paint would have made every
    panel wait on a file, and making the gather's bump wait on the record would have put
    the correction behind a detached child that `_spawn_gather` is allowed to fail to
    spawn. `commands_frame._draw_panels` bumps the frame immediately after
    `state.record_panes` instead, so a paint that lost the race is followed by one that
    reads the shape. A `False` here is now always either transient or true.

    **The cost is one small JSON read on a slot that is not animated.** `top` is not in
    :data:`ANIMATED`, so it repaints on a version bump, never on `panel.TICK` — and both
    writers of this record bump the version precisely so that the panels re-read, which is
    exactly when this answer can have changed. It reads the same directory `_top` already
    opens four times over (`_frame_workspace`, `verbosity`, `state.harness_session`, the
    recorded gauge), and nothing on the idle path (`panel._running`, whose one `stat` per
    tick #387 pinned) goes anywhere near it.

    **False when charter cannot tell**, which covers the corrupt file and the frame
    launched by a charter that predates `record_panes` alike. That is the safe direction
    and not merely the convenient one: false means the roster comes BACK to the top bar,
    which is at worst the duplication this issue is about, while a wrong `True` would take
    the plane's only roster off a screen that has no sidebar to replace it.
    """
    from . import state
    return "right" in state.panes(fid)


#: Columns a renderer assumes when this pane's own tty cannot be measured at all — the
#: width half of the pair :data:`_DEFAULT_ROWS` is the height half of, and the same case:
#: stdout piped somewhere with no tty behind it (a test, or `charter panel bottom
#: --session x > /tmp/log` run by hand).
#:
#: **80 rather than `tui.term_width()`, and #591 is why.** That helper is env-first by
#: design, so reaching for it here answered with the LAUNCHING terminal's `$COLUMNS` —
#: which a panel process inherits whole (see this module's own docstring) and which
#: describes a terminal the pane is a small rectangle inside. The number is the same one
#: `tui.term_width`'s own *default* carried, so a pane with nothing to measure and nothing
#: exported is laid out exactly as it was; what is gone is the branch that let a 400-column
#: shell decide a 24-column pane's paint.
_DEFAULT_COLS = 80


def _width() -> int:
    """The pane's own width in columns — measured, never read out of the environment.

    `pane.size()` asks the descriptor this process CLAIMED, which for a panel launched as
    a tmux pane command IS the pane — not a pipe, not the launching terminal, and (since
    #606) not whatever a provider's library has since rebound `sys.stdout` to. Only when
    that answers `None` — a pane charter has no usable measurement of, which `frame/pane.py`
    defines and this function does not second-guess — does this fall through to
    :data:`_DEFAULT_COLS`, the way :func:`_height` has always answered
    :data:`_DEFAULT_ROWS`. The measured value is returned as reported, with no artificial
    floor clamped over it: a pane that really is 10 columns wide getting reported as 20
    would be exactly the theorising this function exists to avoid — it would tell every
    caller here there is room that the real pane does not have.

    **The fallback is this caller's stated default and nothing infers it was taken**, which
    is the half #606 is about. `panel._unmeasured` is where a pane that could not be
    measured stops being indistinguishable from a real 80x24 one, because that is the
    caller with something to do about it; this is the half that has to hand a renderer a
    number whatever happened. The same two-function split `commands_frame._measure_window`
    (answers `None`, so `cmd_resize` can refuse) and `_window_size` (takes
    `_FALLBACK_SIZE`, because `cmd_launch` must draw) already draw between them.
    """
    measured = pane.size()
    return _DEFAULT_COLS if measured is None else measured.columns


#: Rows a renderer assumes when this pane's own tty cannot be measured at all — the same
#: number, and the same case, as `panel._DEFAULT_ROWS`: stdout piped somewhere with no
#: tty behind it (a test, or `charter panel bottom --session x > /tmp/log` run by hand).
_DEFAULT_ROWS = 24


def _height() -> int:
    """The pane's own height in rows — measured, exactly the way :func:`_width` measures
    its width, and for the same reason `panel._rows` measures it there.

    **A renderer needs this now, and #488 is why.** Until `bottom` grew the repo table,
    every renderer emitted whatever it had and `panel._paint` clamped the LINE COUNT
    afterwards — right for a one-row strip, wrong the moment a renderer has to CHOOSE
    which rows to spend its pane on. Clamping after the fact would cut the table off at
    whatever came last, which is the unranked slice `statusline._pick_rows` exists to
    prevent (its own docstring: thirteen clean repos shown and the dirty one hidden).
    So the budget is measured before anything is composed, and `panel._paint`'s clamp
    goes back to being the safety net it is elsewhere rather than the mechanism.

    **The same descriptor and the same judgement as :func:`_width`.** Both halves ask
    `pane.size()` rather than each running its own `os.get_terminal_size` with its own
    fallback, so a pane charter could not measure can never come back 150 columns wide and
    24 rows tall — which is what two independent asks of a moving `sys.stdout` could
    produce, and did (#606). They are still two calls, one per question: a pane that is
    resized between them answers honestly twice, and `_tick`'s SIGWINCH repaint is what
    reconciles the frame.
    """
    measured = pane.size()
    return _DEFAULT_ROWS if measured is None else measured.lines


class _Doors:
    """Which COLUMNS of a one-row strip are a doorway to the palette.

    The third of the three maps in this module, and the same finding each time: a pointer
    event arrives as a NUMBER (`frame/events.py`), and the only thing that knows what is
    at that number is the pass that composed the row. :class:`_Viewport` answers it for
    the repo table's rows, :class:`_Tabs` for a bar's columns, and this for the two
    one-row strips — :func:`_top` and :func:`_bottom`.

    **A SET of columns rather than a column→name mapping, and the difference is the whole
    design of #751.** The two strips have exactly three fields a click can act on — the
    workspace chip, the persona head, and the `F2 palette` hint — and all three lead to
    the same place, because the palette is the only chooser charter has and *a click
    cannot choose*. `key` is in `component.EVENT_KINDS` and deliberately not in
    `events.DELIVERED` (the harness owns the keyboard), so the select-then-confirm shape
    `_repos_events` uses on the table is not available on a strip: whatever a click on one
    of these does, it has to finish on the pointer alone. Opening the chooser is the one
    thing that does, and it is what the row already advertises.

    So the destination does not vary, and a mapping whose values nothing branches on would
    be data no input could make observable — the survivor the deletion sweep reports and
    `_Tabs`' own docstring argues away one axis over. What the field was CALLED is the
    renderer's business and a test's; what reaches the handler is "is this cell a door".

    **The bounds check is structural for :class:`_Tabs`' reason.** A column nothing
    published is absent whether it is ``-1``, ``4096``, one of the two cells of a ` · `
    separator, or the empty space past the last field — `in` has no wrong answer to give
    the way a tuple indexed with a negative number has.

    **Everything else on both strips answers nothing, and each has a case**: the charter
    version and the context gauge on `top`, and the todo count, the alert, the in-flight
    spinner, the news and the selected repo's detail on `bottom`, are READOUTS. They are
    the frame reporting, not offering; the repo detail is the sharpest of them, because it
    is the readout of a selection made on another pane and a click on it could only mean
    "select what is already selected", which is the one gesture `_Tabs.switch_to` and
    `builtins._repos_events` both already refuse.

    One object at module scope for :class:`_Tabs`' reason: `panel.run` resolves one
    component and draws it for the life of the process, so "which strip is this" has
    exactly one answer here.
    """

    __slots__ = ("_cols",)

    def __init__(self) -> None:
        self._cols: frozenset[int] = frozenset()

    def forget(self) -> None:
        """Back to a strip nobody has drawn — for a test, and only a test.

        `_Viewport.forget`'s reason exactly: production never calls it, because a panel
        process is born here and the object dies with the process.
        """
        self.publish(())

    def publish(self, columns) -> None:
        """Record which columns of the row that was just painted are doors.

        Columns of the component's OWN canvas — the rectangle `ctx.width` describes, which
        is what `events.Dispatcher._on_canvas` delivers a click in, with `[frame] pad`
        already subtracted.

        **Every rung of both ladders publishes, including the rungs that draw no door at
        all.** That is `_Viewport.blank`'s finding and `_bar.row`'s discipline: `_top` has
        a terse rung and a too-narrow rung, `_bottom` drops whole fields as the row gets
        starved and keeps exactly one at `terse`, and a strip that kept a stale map
        through a resize would open the palette from a cell whose field the operator can
        see is gone.
        """
        self._cols = frozenset(columns)

    def opens_palette(self, col: int) -> bool:
        """Whether a click at canvas column *col* should open this frame's palette."""
        return col in self._cols


#: The one strip this process draws into. See :class:`_Doors` for why there is exactly
#: one; `frame/builtins._strip_events` is the other half and holds no state of its own,
#: so the handler and the renderer cannot come to disagree about where the doors are.
DOORS = _Doors()


def _door_columns(limit: int, *fields) -> set[int]:
    """The columns *fields* occupies, given as ``(start, text)`` pairs, clipped to *limit*.

    Measured with `tui.width` and never `len`, for the reason every other measurement in
    this module is: the glyphs on these two rows (`⬢`, `◆`, `⚡`) are exactly the ones
    `statusline._persona_chip_cells`' comment says have broken a column twice, and the
    strings arrive carrying SGR that `len` would count as cells.

    A field whose text is empty contributes nothing — which is how a `top` with no gauge
    and a `bottom` whose `hotkey` field was dropped by `_fit_fields` publish the absence
    rather than a zero-width door at somebody else's column.

    ***limit* is the width the finished row was `tui.truncate`d to, and clipping to it is
    not tidiness.** Both strips end in a truncate, and a field that ran past the pane is
    drawn as a `…` or not drawn at all — so a door published at the columns the field
    WOULD have had is a door on a cell the operator can see is empty. This is
    `_Viewport.blank`'s rule at the finest grain the row offers: what is published
    describes what was drawn, never what was composed.

    There is deliberately no lower clamp beside the upper one. Every *start* here is
    composed out of widths — a `tui.width` sum starting at column 0 — so a negative one is
    not a case this can be given, and a `max(0, start)` guarding against it would be a line
    no input could turn red: the survivor the deletion sweep reports and this repository
    deletes.
    """
    cols: set[int] = set()
    for start, text in fields:
        cols.update(range(start, min(limit, start + tui.width(text))))
    return cols


def _top(fid: str) -> str:
    """Identity: where you are, pinned or not, and who you are being.

    **Task 4 investigation (#385) — the context/cache gauge cannot run here, and this
    deliberately does not call it.** The plan's slot table lists "context/session when
    available" for `top`, naming `statusline._session_strip(payload, sid)` and
    `statusline._context_gauge(payload)` as the wide layout's source for it. Read end to
    end before writing anything here, per the task brief's own instruction:

    `_context_gauge(payload)` is gated on `payload.get("context_window")` at every single
    branch — `ctx NN%` needs `cw["used_percentage"]`, `cache NN%` needs
    `cw["current_usage"]`'s two token counts, and even the REBUILD/cold-streak history
    (`_record_turn`, `_rebuilds`, `_cache_hint`) only runs *nested inside* the
    `if read or write:` block those same live numbers gate — so there is no path through
    this function that produces anything without a payload carrying real numbers THIS
    turn. And there is exactly one source for that payload: `statusline.main` does
    `payload = json.load(sys.stdin)` — Claude Code's per-turn JSON, sent only to the
    process it invokes as the configured `statusLine` command. A frame panel is started
    once, as a long-lived tmux pane command (`panel.run`, polling `state.version`), never
    re-invoked per turn and never handed that stdin — `gather.py`'s own module docstring
    already establishes this identically for the gather side ("No per-session payload").

    So `_context_gauge({})`/`_context_gauge(None)` is not a degraded case here, it is
    the ONLY case, forever, no matter what the session has done — calling it would add a
    line of code whose return value this docstring can already prove is always `[]`.
    Composing `_session_strip(payload, sid)` instead would not help: it is exactly
    `[*_context_gauge(payload), *_session_news(sid)]`, so with the gauge half structurally
    dead the call would only ever surface `_session_news`'s half — which is `bottom`'s
    own, explicit assignment (see `_bottom`'s docstring), not `top`'s.

    **That argument stopped applying, and #413 is where it stopped.** Every word of it is
    still true of `_context_gauge`, which is why this function still does not call it —
    but its conclusion ("so the frame cannot have a gauge") rested on a premise that is no
    longer the whole picture: a panel now has a FILE to read. The suppressing
    `statusline.main` is the one process that sees both Claude Code's session id and this
    frame's, so it writes the mapping down (`state.record_harness_session`), and
    `record_usage` writes the numbers themselves — including, since #413, the context
    percentage, which is the one figure nothing can re-derive. `statusline
    .recorded_context_gauge` composes exactly what `_context_gauge` composes, from the
    recorded history instead of a live payload, so the two surfaces cannot disagree about
    a threshold or a label.

    **What has NOT changed is the rule the old argument was built on**: a gauge that
    silently reads zero is worse than no gauge. Every way of not knowing — a frame with no
    recorded session (a harness that is not Claude Code is never handed a usage payload at
    all), a session with no turns recorded yet, a history written entirely by a charter
    that predates the fourth field — answers `[]` and draws nothing, rather than a
    confident `ctx 0%`. Pinned by `tests.test_frame_slots.TopRenderer`.

    **The persona ROSTER is drawn only when the sidebar is not (#530), and the active
    persona always is.** The operator asked why the top bar lists every persona when the
    sidebar lists them too, and they were right for the frame they were looking at: since
    #516 `_right` draws the same names with a heading, memory badges, vault dots, health
    marks and in-flight badges, in an aligned column — so the top bar's `◇ personas …`
    said strictly less about exactly the same thing. But it is a CONDITION and not a
    deletion, because `layout.visible_slots` drops `right` first on any shortage: on a
    terminal too narrow or too short for a sidebar the top bar is the plane's only roster
    again, and a `charter statusline` session outside a frame has no sidebar at all
    (`statusline._persona_line`, this row's other caller, is left exactly as it was).

    `◆ <active>` is on the other side of that line, and stays whatever else is on screen.
    The roster is a LIST — a thing the sidebar can hold more of, better — while the active
    persona is IDENTITY: "who am I being" is read here, next to the workspace, which is
    the question this whole row exists to answer. The sidebar does mark the active one
    with `▸`, but that is a mark inside a list, not an answer beside a workspace.

    `_sidebar_live` is what decides, per repaint, and its docstring argues the record it
    reads. What this function must not do is reassemble the roster's words to drop half of
    them: `statusline._persona_line_parts` hands over the row already split, the same way
    `PersonaChip` hands `_right` its chips, so nothing here repeats what either surface
    says and neither can drift from the other.

    **At `terse` the version goes, and nothing else does.** `top` answers "where am I and
    who am I being", and of the things on it the charter version is the only one that
    reads the same on every frame on this machine all day — it is a fact about the
    install, not about where you are standing. The workspace and the persona ARE the
    answer, so a density that dropped either would leave a row not earning its line. The
    gauge stays for the opposite reason: it is the one field on this row that is different
    every turn, and a density that buys back columns should not spend them on the field
    that had something new to say.

    **The version carries the dev-channel chip, `statusline._dev_chip()` (#457).** #386
    made a frame suppress the status line entirely, and `_brand` is the only OTHER place
    the `dev` word was ever drawn — so a framed session on the dev channel had the word
    nowhere on screen: the surface that knew was blanked, and this one, which replaced
    it, never imported `channel` at all. The chip follows the version rather than getting
    a branch of its own, for the same reason `_brand`'s docstring gives: it is a fact
    about the version, not a fourth thing on this row, so it goes exactly where the
    version goes — including out, at `terse`, since a density that meant to buy back a
    whole line does not mean to keep half of it.

    **The version sits at the row's RIGHT-HAND END (#516), and everything else stays
    where it was.** The split is what the operator asked for and it is also the reading
    the row already had: the left of this bar answers *where am I and who am I being* —
    the workspace, the gauge, the persona — and the version answers *which build is
    saying so*, which is not part of that sentence. Pushed to the far edge it stops
    reading as a fourth field of the identity and starts reading as the bar's own
    signature, which is what it is.

    **Right-aligned by PADDING, never by truncating the identity into it.** `tui.pad`
    on the left half is what places the version, so a wide row simply has spaces in
    between; and when the two halves genuinely do not both fit, the VERSION is dropped
    whole rather than the workspace being cut to make room for it. That is the same
    "shown whole or dropped whole" discipline `_fit_fields` keeps one slot over, and it
    is the same field `terse` already drops — so a starved row and a terse row lose the
    same thing, rather than a narrow terminal inventing a third way for this bar to
    degrade. The `dev` chip travels inside that one f-string, so it goes and comes back
    with the version and can never be left behind on its own.
    """
    from .. import __version__, statusline
    from . import state
    # The FRAME's workspace, not this pane's own guess at one (#512) — see
    # :func:`_frame_workspace`. `$CHARTER_WORKSPACE` is the one rung a panel does share
    # with its launcher (`commands_frame._frame_identity_env` carries it, empty when the
    # launcher had none), so "the operator pinned this by hand" is a question this process
    # can still answer.
    #
    # The variable is compared to the NAME DRAWN, not asked about through
    # `workspace.source()`: the `*` claims the environment chose *this* name, and a marker
    # that only knows the variable is set somewhere will say so over a name the variable
    # did not name — which is what `state.workspace_for`'s rung 0 is about. Comparing the
    # value keeps the two halves of the chip agreeing by construction rather than by both
    # happening to consult the same rung, and it drops `source()`'s `from_path` walk and
    # pointer reads off this slot.
    ws = _frame_workspace(fid)
    pin = "*" if ws == os.environ.get("CHARTER_WORKSPACE", "").strip() else ""
    # Identity always; the roster only when nothing else on screen is drawing it (#530).
    # `_persona_line_parts` is what decides which words are which — this row picks its
    # pieces and never assembles any, for the reason `_right`'s docstring gives about the
    # chips: a fact recomposed here is a fact free to drift from the surface it copies.
    line = statusline._persona_line_parts()
    persona = "" if line is None else line.rendered(roster=not _sidebar_live(fid))
    # Two file reads on a slot that repaints only on a version bump (`top` is not in
    # `ANIMATED`), and nothing is read at all for a frame with no recorded session —
    # which is every frame whose harness is not Claude Code.
    gauge = " ".join(statusline.recorded_context_gauge(
        state.harness_session(fid) or ""))
    left = f" ⬢ {ws}{pin}"
    identity = f"{left}  {gauge}  {persona}" if gauge else f"{left}  {persona}"
    # **The two doors on this row, and both are measured from the pieces that were
    # composed rather than found in the finished string** (#751). `left` is the workspace
    # chip and starts at column 0; the persona field is `PersonaLine.head` — *identity*,
    # the `◆ steward` half — which begins after `left`, the two spaces, and the gauge with
    # its own two spaces when there is one. The ROSTER half that may follow it is left
    # inert deliberately: `_top` draws it only when the sidebar is not (#530), so a door
    # there would exist on some frames and not others for a reason no operator can see.
    #
    # Composed rather than searched for the reason this function already gives about the
    # chips: a fact recovered from a rendered row by looking for `◆` in it is a second
    # reading of what was drawn, free to drift from the first.
    #
    # **One publish, above the ladder rather than on each of its three rungs.** All three
    # draw *identity* from column 0 and differ only in what they put to the RIGHT of it —
    # nothing, the version, or a `…` — so the doors are at the same columns on every rung
    # and clipping to *w* is what the difference costs them. `_bar.row` reaches the same
    # place from the other direction, with a closure, because its rungs draw different
    # names; here there is nothing for a second call to say differently, and two calls
    # would be two places for that to stop being true.
    w = content_width("top")
    head_at = tui.width(left) + 2 + (tui.width(gauge) + 2 if gauge else 0)
    DOORS.publish(_door_columns(w, (0, left),
                                (head_at, "" if line is None else line.head)))
    if verbosity(fid) == "terse":
        return tui.truncate(identity, w)
    build = f"charter {__version__}{statusline._dev_chip()} "
    # `+ 1` is the one column that must separate them; without it a full-width identity
    # would butt straight up against the version and read as one word.
    if tui.width(identity) + tui.width(build) + 1 > w:
        return tui.truncate(identity, w)
    return tui.Row(tui.Cell(identity, w - tui.width(build)),
                   tui.Cell(build, tui.width(build)), gap="").render(w)[0]


class _RowKey:
    """A directory-shaped key for one cache row: `_pick_rows` wants something with
    a `.name` it can compare against `cur_repo` and use as a `states`/`gl` dict
    key (ordinarily a `Path`) — this is that, minus the filesystem, since nothing
    in :func:`_table_lines` touches one. Identity-hashed on purpose (no `__eq__`
    override): two repos can share a name only if the cache itself is
    inconsistent, and identity keeps every row distinct even then, the same
    guarantee a real `Path` object would not actually make either (two `Path`s
    for the same string compare equal)."""

    __slots__ = ("name",)

    def __init__(self, name: str) -> None:
        self.name = name


def _needs_attention(r: dict) -> bool:
    """True when a repo/piece row has something on it worth a look — the same
    four facts `_repo_rows`' own "…(+N more)" line checks before it dares say
    "all clean": dirty, unpushed/behind, a failing or running pipeline, an open
    change. Shared by :func:`_table_lines`' own overflow line so the two claims
    (which repos are hidden, and whether that is safe to say) cannot drift
    apart."""
    return bool(r.get("dirty") or r.get("ahead") or r.get("behind")
               or r.get("ci") in ("failed", "running") or r.get("change"))


class _Line(NamedTuple):
    """One composed row of the repo table, and **which repo it is a row about**.

    The second field is the whole reason this is a pair rather than a string. A click
    arrives as a row NUMBER (`frame/events.py`), and the only thing that knows what is on
    that row is the pass that composed it: the table's rows are a ranked window over the
    repos, with piece rows nested under them and an overflow line that is about no repo at
    all, so nothing downstream can work back from an index to a name without redoing the
    picking — which would be a second answer to "which repos are on screen", and the two
    would disagree the moment a repaint landed between the paint and the click.

    *repo* is the repo the line BELONGS to, not the row's own subject: a piece row carries
    its parent repo's name (`gather`'s ``worktrees`` rows carry ``repo``), so every row of
    the tree resolves to a repo and a click anywhere in it selects something. The one line
    that answers ``None`` is `…(+N more)`, which stands for rows that are not on screen —
    there is nothing there to select, and saying so is better than picking one of them.
    """

    repo: str | None
    text: str


class _Viewport:
    """Where the `repos` table is scrolled to, and what it drew last — for ONE pane.

    **A panel process holds one pane, so this is one object at module scope rather than a
    map keyed by anything.** `panel.run` resolves a component name once and then draws that
    one component for the life of the process (`panel._component_painter` closes over the
    same decision), so "which pane is this" has exactly one answer here and a key for it
    would be a second, weaker one. Tests reach the same object and :meth:`forget` is what
    puts it back — the reset is the price of the singleton and is not hidden.

    **It holds an intent, and the renderer is what settles it against the data.** The
    handler moves an offset knowing nothing about how many rows the table has or how tall
    the pane is — `Component.on_event` is handed no ctx by contract (§4f), and it must not
    grow a second reading of a plane the repaint is about to read anyway. So :meth:`settle`
    is called by `_repos` with the bound it just computed, and that is also the answer to
    *what happens when the offset outlives a table that shrank under it*: the next paint
    clamps it down and the pane draws the last window there is, rather than an empty table
    the operator has to guess their way back out of.

    **The offset counts pane ROWS, not repos** (#663). A table is repo rows and the piece
    rows nested under them, and a viewport whose unit was the repo could not move at all on
    a plane with one clone — which was every control plane charter runs on.

    :attr:`limit` is that bound REMEMBERED, which is what lets the handler refuse a scroll
    that would change nothing. Zero until the first paint, and the first paint always
    happens before the first event can: `panel._watch` seeds its resize flag ``True`` and
    calls `_tick` before it ever reaches `_wait`. So a pane exactly tall enough for its
    content answers every wheel notch with "nothing moved" from the very first one.
    """

    __slots__ = ("offset", "limit", "_rows")

    def __init__(self) -> None:
        self.offset = 0
        self.limit = 0
        self._rows: tuple[str | None, ...] = ()

    def forget(self) -> None:
        """Back to a pane nobody has scrolled or drawn — for a test, and only a test.

        Production never calls it: a panel process is born here and the object dies with
        the process. It exists because the alternative for a test is reaching into
        ``__slots__`` by name, which is a test that keeps working after the attribute it
        pokes has stopped being the one that matters.
        """
        self.offset = 0
        self.limit = 0
        self._rows = ()

    def settle(self, limit: int) -> int:
        """Record how far this table may scroll, and answer where it actually is.

        The clamp is here and nowhere else. `_scroll_limit` computes the bound from the
        table's ROW count (:func:`_content_rows`) and the pane's rows; this is what applies
        it, so an offset left over from a longer table — the operator scrolled to the
        bottom, then a repo or a worktree was removed — comes back as the largest offset
        the CURRENT table has, on the very next paint.

        Never below zero, and that half is not symmetry: :meth:`move` already refuses to go
        under, so what this guards is a *limit* that has gone negative, which
        `_scroll_limit` cannot produce — and a `max` here would be a second answer to a
        question that one already answers. It is spelled `min` alone for that reason.
        """
        self.limit = limit
        self.offset = min(self.offset, limit)
        return self.offset

    def move(self, rows: int) -> bool:
        """Scroll by *rows*, and answer whether anything actually moved.

        **The answer IS the repaint decision** (§4f: a handler answers truthy for "repaint
        me"), and answering it here rather than in the handler is what makes "a pane tall
        enough for its content does not scroll" a property of one object with one test
        rather than an accident of two. A wheel notch on such a pane finds ``limit`` zero,
        moves nothing, and costs the frame no paint at all.
        """
        moved = min(max(self.offset + rows, 0), self.limit)
        if moved == self.offset:
            return False
        self.offset = moved
        return True

    def publish(self, rows) -> None:
        """Record which repo each row of the pane is about, the paint that just happened.

        Indexed by the PANE's row, not the table's: `_repos` puts a heading on row 0 and
        the table under it, and the one-line answers it draws instead of a table
        (`_gone_lines`, `_unknown_lines`, `_unreadable_lines`, `_empty_lines`,
        `_too_narrow_lines`) publish nothing at all —
        through :meth:`blank`, which is the one call that says so, because such a paint
        has a bound to record as well. So a click on the heading, on a message, or below
        the last row lands on ``None`` and is not a selection — which is the same rule
        `events.Dispatcher._on_canvas` applies to the pad, one axis over: cells the
        component did not draw a row into are not cells it was clicked in.
        """
        self._rows = tuple(rows)

    def blank(self) -> None:
        """What a paint that drew NO TABLE leaves behind: no map, and nowhere to scroll.

        **The two halves of a paint are recorded together, because a paint that recorded
        one and not the other is the defect this method exists to make unwriteable.**
        `_repos` has five ways out and four of them draw a single sentence instead of a
        table (`_too_narrow_lines`, `_gone_lines`, `_unknown_lines`, `_empty_lines`). Each
        cleared the click map and none of them touched the bound, so a pane that had been
        scrolled and then lost its table — the terminal narrowed below `statusline._LEFT_W`,
        the gather cache went away, the workspace's last clone was removed, the workspace
        itself was removed — kept whatever
        :meth:`settle` recorded for the TALLER table that was there before. The handler
        reads that bound and nothing else (§4f), so every wheel notch over the sentence
        answered truthy and the panel repainted a byte-identical line, once per notch:
        motion the operator can see is not happening, charged to their terminal, which is
        the exact cost :func:`_scroll_limit`'s own docstring refuses one paragraph over.

        **Zero and not "leave the bound alone", because zero is what the arithmetic
        already answers for this pane.** `_scroll_limit` answers 0 for any budget at or
        below one row, and a pane drawing one sentence has no rows for a table at all — so
        this is that function's answer for the shape rather than a second rule beside it.
        The offset goes with it through :meth:`settle`'s own clamp, which is the same
        thing that happens to a table starved to no rows by a resize (that path reaches
        `settle` and always did); the two shapes now agree instead of differing by which
        line `_repos` returned from.
        """
        self.publish(())
        self.settle(0)

    def repo_at(self, row: int):
        """The repo on pane row *row*, or ``None`` — the paint the operator was looking at.

        Out of range answers ``None`` rather than raising, and a negative index answers
        ``None`` rather than counting from the end — which is what Python's own indexing
        would have done and is the one wrong answer available here: `overlay.Event` rows
        are already 0-based and non-negative, so a negative one would mean charter's
        arithmetic was wrong, and reporting the LAST row for it would hide that.
        """
        return self._rows[row] if 0 <= row < len(self._rows) else None


#: The one viewport this process's `repos` pane scrolls. See :class:`_Viewport` for why
#: there is exactly one; `frame/builtins.py` is the other half, and it holds no state of
#: its own so that the handler and the renderer cannot come to disagree about where the
#: table is scrolled to.
VIEWPORT = _Viewport()

#: Rows one wheel notch moves the table. One, deliberately: a terminal sends one report
#: per notch and tmux forwards each, so a larger step multiplies whatever the operator's
#: mouse or trackpad already decided — and the pane is at most `statusline._MAX_REPO_LINES`
#: rows, where a three-row step would put the whole table past you in two flicks.
SCROLL_ROWS = 1


def _piece_rows(data: dict) -> list[dict]:
    """The rows this table nests UNDER a clone, or none at all.

    `statusline._detail_worktrees`' rule, asked of the cache rather than of the filesystem:
    full piece rows exist only where the workspace resolves to exactly one repo, because at
    two or more a repo must never lose its row to another repo's pieces — every other
    plane's pieces are a `⑂N` badge on the repo's own row instead. `gather.scan` already
    writes ``worktrees`` on that condition alone; asking it again here is what keeps a cache
    written by another version from putting a piece row under each of six repos.

    **One function because two callers must not disagree.** :func:`_table_lines` composes
    these rows and :func:`_content_rows` counts them for :func:`_scroll_limit`; a count that
    included a row the composer would not draw leaves an offset that scrolls onto nothing,
    and one that excluded a drawn row leaves the bottom of the plane permanently out of
    reach. #663 is what the second of those looks like when it ships.

    **`data["repos"]` and `data["worktrees"]`, with no `or []` under either**, which is the
    reason :func:`_selected_detail` already gives for the same subscript: `gather.cached`
    answers ``None`` for anything whose `repos` or `worktrees` is not a LIST
    (`gather._shaped_like_a_scan` checks both), so a fallback here could only ever stand in
    for an empty list with an empty list. The sweep found both surviving, correctly — a
    line that cannot change an outcome is not documentation of an intent.
    """
    return list(data["worktrees"]) if len(data["repos"]) == 1 else []


def _content_rows(data: dict) -> int:
    """How many ROWS of table this gather wants — the quantity :func:`_scroll_limit` bounds.

    **Rows and not repos, and #663 is the whole cost of the difference.** The pane renders
    rows: the budget is in rows, the overflow line takes a row, and :meth:`_Viewport.repo_at`
    maps a row. A limit computed from the repo COUNT is therefore the one unit in that
    expression that is not the pane's, and it agrees with this number only on a plane where
    no repo has pieces. On the one-clone-many-worktrees plane — the ordinary state of a
    control plane, and the state of the one #658 shipped from — it answered 0 for every
    pane height, so the wheel was inert and the pieces past the pane's last row were
    dropped with nothing on screen saying so.

    Subscripted rather than `.get`-with-a-fallback for the reason :func:`_piece_rows`
    states: `gather.cached` is what refuses a scan whose `repos` is not a list, so a
    fallback here stands in for an empty list with an empty list.
    """
    return len(data["repos"]) + len(_piece_rows(data))


def _scroll_limit(rows: int, budget: int) -> int:
    """The largest offset a table of *rows* content rows may hold in a *budget*-row pane.

    *rows* is :func:`_content_rows` — repo rows and piece rows together, which is what the
    pane actually draws. See that function for why counting repos here was #663.

    **Zero whenever all of it already fits, and that is the tested nothing.** The `repos`
    pane is sized to its content (`layout.repos_rows`), so on an ordinary plane the pane is
    exactly tall enough and this answers 0 — every wheel notch then moves nothing, repaints
    nothing and costs nothing, because :meth:`_Viewport.move` reads this bound before it
    changes anything. A scroll that quietly did nothing *because the arithmetic happened to
    cancel* would be indistinguishable from a scroll that was dropped, which is the class of
    convincing-empty this module refuses everywhere else. It is still the ordinary answer
    for a one-clone plane: what changed is that it is now the answer because the pieces FIT,
    not because they were never counted.

    The row the overflow line takes is subtracted here for the same reason
    :func:`_table_lines` reserves it out of the budget rather than trimming it off the end:
    it is a row of the pane and a row the content does not get. Reserved on exactly the same
    condition — more rows than the pane has — so the two cannot disagree about whether that
    row exists, which would leave the last row permanently one notch out of reach.

    **A pane with room for one row is a pane with room for the overflow line and nothing
    else**, and it answers 0 too. :func:`_table_lines` spends a one-row budget on
    `…(+N more)` rather than on an arbitrary row — "there is more here than fits" outranks
    "here is one of them" — so every offset over such a table draws the identical line. A
    limit taken from the content count alone would let the wheel repaint that line forty
    times, each repaint byte-identical to the last: motion the operator can see is not
    happening, charged to their terminal.
    """
    if rows <= budget:
        return 0
    if budget <= 1:
        return 0
    return rows - (budget - 1)


def _table_row(lead: str, name_markup: str, r: dict, width: int,
               branch_override: str | None = None) -> str:
    """One row of the WIDE repo table — the same four columns
    `statusline._tree_cells` draws, at the same declared widths, so a row in the
    frame's `bottom` pane and a row in the status line's own table line up
    character for character.

    **Composed here rather than by calling `_tree_cells` itself, and the reason is
    the idle-cost rule, not style.** That function ends in `_presence_for_dir(d)`,
    which is a `worktree.locate`/`workspace.clone_of` pair — a filesystem walk PER
    ROW, on every repaint. `bottom` is the one animated slot (:data:`ANIMATED`), so
    at `panel.TICK` a fourteen-row table would be seventy directory walks a second
    while any dispatch is in flight. #387 pinned a panel's idle tick at exactly one
    `stat` and #488 must not spend that back: everything below comes out of
    `gather.read(fid)`'s cache and touches no repo directory at all.

    The cost is stated rather than hidden: the frame's table has no presence column
    ("who else is standing in this tree"). It is the one field of `_tree_cells` that
    cannot be answered from the gather, and `_branch_cell_for` already treats it as
    the field that loses its columns first — so the frame draws the cell exactly as
    the status line draws it on a pane too narrow for presence.

    Everything else IS `statusline.py`'s own — `_markers`, `_branch_cell_for`,
    `_ci_part`, `_CI_MARK`, the four column widths, `_GAP` — called, not
    reimplemented, so a fix to what a marker or a CI glyph means lands in both
    surfaces at once.

    *branch_override* is `_tree_cells`' own `branch=` parameter under a clearer name:
    ``""`` prints the markers alone, which is what a piece whose branch merely
    restates its own name gets (see :func:`_table_lines`).
    """
    from .. import statusline as sl

    marks_plain, marks_col, is_dirty = sl._markers(r)
    # Parenthesised, though Python's precedence already reads it this way: the `or "?"`
    # is the DEFAULT for a missing branch, not an alternative to the override — and an
    # override of `""` (a piece whose branch restates its own name) must stay empty
    # rather than turn into `?`.
    text = (r.get("branch") or "?") if branch_override is None else branch_override
    branch_cell = sl._branch_cell_for(text, "", marks_plain, marks_col, is_dirty)

    change = r.get("change")
    sigil = r.get("sigil") or "!"
    mr = f"{sl.accent('ok')}{sigil}{change}{sl._R}" if change else ""

    row = tui.Row(tui.Cell(f"{lead}{name_markup}", 2 + 3 + sl._NAME_W),
                  tui.Cell(branch_cell, sl._BRANCH_W),
                  tui.Cell(sl._ci_part(r.get("ci")), sl._CI_W),
                  tui.Cell(mr, sl._MR_W),
                  gap=sl._GAP)
    return row.render(width)[0]


def _table_lines(data: dict, width: int, budget: int, *, offset: int = 0,
                 selected: str | None = None) -> list[_Line]:
    """The repo table the `repos` pane draws under its heading, at most *budget* lines.

    **Each line comes back with the repo it is about** (:class:`_Line`), because a click
    arrives as a row number and this is the only pass that knows what is on a row. See that
    class for why the alternative — resolving an index back to a name afterwards — is a
    second answer rather than a lookup.

    **This is #488's actual answer**: the frame used to show LESS of the plane's repo
    state than the status line it suppresses (#386), because the only slot drawing repos
    was a 22-column sidebar whose own docstring conceded that `_NAME_W` (32) and
    `_BRANCH_W` (34) alone exceed the whole pane. The table is drawn at the widths it was
    designed for — on `bottom` when #488 shipped it, and since #515 in `repos`, a pane of
    its own whose whole width is the table's.

    Reads ONLY *data* — one `gather.read(fid)` in the caller — and never a repo
    directory, a `git status` or a `glstate.read_for` of its own. Every field a row needs
    (`name`, `branch`, `dirty`, `tracked_dirty`, `ahead`, `behind`, `ci`, `change`,
    `sigil`, `current`, `worktree_count`) is already in that one gather.

    `statusline._pick_rows` is called rather than reinvented, for the reason its own
    docstring records in production: an unranked slice of 18 clones showed thirteen clean
    repos on `main` and hid the one dirty repo you were standing in. It wants
    directory-shaped keys (`.name`, hashable), so each cache row is wrapped in a bare
    :class:`_RowKey` — nothing here touches a filesystem, and a `Path` would imply one
    exists to touch.

    *budget* is the `repos` pane's real height minus its HEADING (`_height() - 1` in
    :func:`_repos`, floored by :func:`_table_cap`), measured before anything is composed.
    The row it subtracts is `▪ repos N`, not the attention row — that is another pane's
    since #515, and a budget still reserving a row for it would cost the table its
    lowest-ranked repo row for nothing. The budget is spent in priority order —
    repo rows first, then piece rows, with the `…(+N more)` line that admits what was
    dropped reserved out of it — so a short pane loses DETAIL rather than losing a repo.
    That ordering is `statusline._repo_rows`' own (`wt_budget` there), kept because the two
    tables are meant to read alike.

    Piece rows are :func:`_piece_rows` — ``data["worktrees"]`` where the workspace resolves
    to exactly one repo, `statusline._detail_worktrees`' rule verbatim. Every OTHER repo's
    pieces get a `⑂N` badge on the repo's own row instead, from `worktree_count`; the badge
    means "there is more you cannot see here", so it is dropped whenever every piece has a
    row **on this screen** — counted from the pieces actually DRAWN and not from the ones
    the cache holds, or a scrolled table would drop the badge while nine of the fourteen
    were off the pane.

    *offset* is how far down the table's own rows the window has been scrolled, and it
    changes nothing at all at zero — which is the whole of how this stayed compatible.
    **It is one index into one list, and the list is the repo rows in ranking order
    followed by the piece rows in the cache's** — the same order the budget is spent in
    above, read one window further along rather than truncated. So while the window still
    holds repo rows it is `statusline._pick_rows`' own `[offset:offset + room]` slice of
    the ranking (the repos come back in priority order, the display order is still the
    cache's, and scrolling back to zero lands on the exact bytes that were there before the
    first notch); once every repo row is above the window it is an index into the pieces,
    which is what #663 is. The `…(+N more)` line needs no arithmetic of its own either: what
    is HIDDEN is the complement of what was drawn, which is true at every offset. An offset
    past what this table can hold is the CALLER's to clamp and is deliberately not clamped
    again here — `_repos` settles it against :func:`_scroll_limit` on the same *data* and
    the same *budget* this is handed, and a second clamp would be a second answer to where
    the bottom is (`statusline._pick_rows` refuses the same thing for the same reason).

    **The one-clone plane is where this changed, and it changed because it was wrong.** A
    table of one repo and fourteen pieces in a six-row pane used to draw six rows and say
    nothing about the nine it dropped, because `capped` counted repos and one repo always
    fits. It now reserves the overflow row on the same condition every other shape does.
    Offset zero on a many-clones plane is byte-for-byte what it was.

    *selected* is the repo `state.selection` says this frame has picked, and the row for it
    is drawn in reverse video by `frame/chrome.reverse` — the same call `persona_section`
    already makes for the persona a frame is on, at the same place in the pipeline (a
    FINISHED row, at the width the renderer measured), so there is one answer to what a
    chosen row looks like in this frame. A name matching no row highlights nothing, which
    is what a selection left over from a repo that has since gone degrades to.
    """
    from .. import statusline as sl
    from . import chrome

    repos = data.get("repos") or []
    if not repos or budget <= 0:
        return []
    # **Too narrow for the table is NO table, not a cut one**, and the alternative is
    # the exact failure this plan's Global Constraints name. Every column after the
    # branch — the CI glyph, an open change — sits at a fixed offset past
    # `_NAME_W + _BRANCH_W`, so a pane narrower than the row simply loses them off the
    # right-hand end, and a dirty, CI-failing, unpushed repo renders as a clean-looking
    # `charter  main`. Refusing to draw says "no room to say" where a trimmed row says
    # "nothing to say".
    #
    # **Ordinary, not exotic — an 80-column terminal reaches here.** #515 changed WHO
    # arrives: `layout.visible_slots` now drops `repos` outright below `_LEFT_W`, reading
    # the width from `layout._table_min_cols` so the launcher's drop and this refusal are
    # one number, so an 80-column LAUNCH has no `repos` pane at all. What still lands here
    # is a running frame narrowed by `cmd_resize`, which changes sizes and never which
    # panes exist, and `charter panel repos` piped into a narrow terminal by hand — and
    # `_repos` turns this `[]` into :func:`_too_narrow_lines` rather than an empty box.
    # The PANE can also be narrower than the window: the slot order is the geometry, so a
    # `[frame] slots` naming `right` before `repos` insets this pane by `right`'s columns
    # and its border (`layout.repos_cols`). Which is why :func:`_table_cap` — not this
    # function — is what the LAUNCHER asks, and why it asks with the width of the PANE
    # rather than of the window.
    if width < sl._LEFT_W:
        return []

    keys = [_RowKey(r["name"]) for r in repos]
    by_key = dict(zip(keys, repos))
    cur_repo = data.get("current_repo")
    palette = {r["name"]: sl._PALETTE[i % len(sl._PALETTE)] for i, r in enumerate(repos)}

    pieces = _piece_rows(data)
    # **What overflows is ROWS, pieces included** (#663). Counting repos here made this
    # `False` on every one-clone plane however many worktrees hung off it, so a table that
    # could not fit fifteen rows into six drew six and admitted nothing.
    capped = _content_rows(data) > budget
    # `- 1` and no `max(1, …)` underneath it: the overflow line is reserved OUT of the
    # budget rather than appended on top of it and trimmed off at the end. With a one-row
    # budget the trimmed version showed a single repo row and dropped the `…(+N more)`
    # line — a pane claiming that one clean repo is the whole plane, which is the
    # false-clean reading this module refuses everywhere else. A budget of exactly one
    # therefore spends it on the note, which is the honest half of the pair: "there is
    # more here than fits" outranks "here is an arbitrary one of them".
    room = budget - (1 if capped else 0)
    # **Ranked unconditionally, and the `if capped else keys` that used to be here is
    # gone.** The sweep found the two arms indistinguishable and it is right: an UNCAPPED
    # table has `room >= len(keys)` by definition and its offset is 0 (`_scroll_limit`
    # answers 0 and `_Viewport.settle` holds it there), and `_pick_rows` re-sorts its pick
    # back into cache order — so `[0:room]` of the ranking IS `keys`, element for element.
    # The branch was an optimisation over two sorts of at most `statusline._MAX_REPO_LINES`
    # rows on a pane that repaints on a version bump, and what it cost was a second shape
    # for this line to have: with it, an offset over a table that fits moved the pieces and
    # not the repos. One window over one list, at every offset, capped or not.
    show = sl._pick_rows(keys, room, cur_repo, by_key, by_key, offset=offset)

    # **The window is one window over one list, and the list is repo rows THEN piece rows.**
    # That is the order this function already spent its budget in (`statusline._repo_rows`'
    # `wt_budget`: a repo must never lose its row to another repo's pieces), so making the
    # offset an index into it is the same ordering read one step further along rather than
    # a second kind of offset. Where the window falls among the pieces is therefore
    # whatever is left of it once the repo rows have been passed — zero while any repo row
    # is still on screen, and `offset - len(keys)` once they are all above it.
    # The `max` is on the START and not on the length: `offset - len(keys)` is negative
    # while a repo row is still on screen and a negative index would count from the END of
    # the pieces. `room - len(show)` needs no such guard — `_pick_rows` never returns more
    # than the *room* it was given and an uncapped table's repos already fit in it, and a
    # slice whose stop is below its start is empty either way.
    start = max(0, offset - len(keys))
    kids = pieces[start:start + room - len(show)]

    shown_pieces: dict = {}
    for p in kids:
        shown_pieces[p.get("repo")] = shown_pieces.get(p.get("repo"), 0) + 1

    lines: list[_Line] = []
    for i, k in enumerate(show):
        r = by_key[k]
        total = r.get("worktree_count") or 0
        badge = (f"{sl._DIM} ⑂{total}{sl._R}"
                 if total and shown_pieces.get(r["name"], 0) < total else "")
        emph = f"{sl._BOLD}{sl._UNDER}" if r.get("current") else ""
        # A repo with rows nested beneath it is not where the tree ends, so it keeps
        # `├─` — `statusline._repo_rows`' own rule, and the reason `_TREE_WT` exists.
        is_last = (not capped) and i == len(show) - 1 and not kids
        tree = sl._TREE_END if is_last else sl._TREE_MID
        row = _table_row(f"  {sl._DIM}{tree}{sl._R}",
                         f"{emph}{palette[r['name']]}{r['name']}{sl._R}{badge}",
                         r, width)
        # **The highlight goes on LAST and re-asserts itself through the row's own resets**
        # — `chrome.reverse`'s whole reason, measured on this exact shape of row: every
        # coloured span here ends in `sl._R`, and a `\x1b[7m` wrapped naively around the
        # outside dies at the first one. Applied to the finished row rather than composed
        # into it, so this function composes one row and one function decides what a chosen
        # row looks like.
        #
        # **What it does NOT do is keep the cells' colours, and that line used to say it
        # did** (#736). The branch cell is `sl._YELLOW` when the repo is dirty and the CI
        # cell is `sl._GREEN`/`sl._RED`, and inside a reversed run those sit on the
        # terminal's own FOREGROUND — yellow on light grey, on a dark theme, on the one row
        # that says which repo you picked. `chrome.reverse` deletes them; the glyphs and
        # `NoStatusIsCarriedByColourAlone` are what still say what the cells mean.
        lines.append(_Line(r["name"],
                           chrome.reverse(row, width) if r["name"] == selected else row))

    for j, p in enumerate(kids):
        # A piece row is where the tree ends only when nothing follows it, and the
        # `…(+N more)` line follows every capped table — the same `not capped` the repo
        # rows above ask before they take `_TREE_END`. Without it a starved one-clone plane
        # closes its tree with `╰─` and then prints ten more rows' worth of admission
        # underneath, which is the glyph saying the opposite of the line below it.
        mark = sl._TREE_WT if j == len(kids) - 1 and not capped else sl._TREE_MID
        emph = f"{sl._BOLD}{sl._UNDER}" if p.get("current") else ""
        # `charter worktree add <repo> <piece>` names the branch after the piece, so by
        # default these two columns print the same word twice — empty the branch cell
        # when they agree, exactly as `statusline._repo_rows` does, so the column comes
        # to mean "this piece is NOT on the branch you would assume". The markers still
        # render: dirty and ahead/behind are true of the tree whatever its branch.
        wb = p.get("branch") or "?"
        # **A piece row belongs to its parent REPO**, which is what keeps one namespace
        # rather than two. `gather` writes `repo` on every worktree row, so a click on a
        # nested row selects the clone it is a piece of — and a repo name and a piece name
        # can never come to mean the same selection, which they could if pieces were
        # selectable in their own right (a piece is named after a branch, and nothing stops
        # one being named after a sibling clone).
        lines.append(_Line(p.get("repo"), _table_row(
            f"  {sl._DIM}{sl._TREE_PIPE}{mark}{sl._R}",
            f"{emph}{sl._DIM}{p['name']}{sl._R}", p, width,
            branch_override="" if wb == p.get("name") else None)))

    if capped:
        # **Every row that is not on screen, repo rows and piece rows alike**, which is
        # what makes the count honest at every offset: what is hidden is the complement of
        # what was drawn, and that stays true wherever the window is. The pieces are
        # excluded by INDEX rather than by membership — two worktrees of the same clone on
        # the same branch are equal dicts, and `p not in kids` would call one of them
        # shown because the other was.
        hidden = ([by_key[k] for k in keys if k not in set(show)]
                  + pieces[:start] + pieces[start + len(kids):])
        quiet = not any(_needs_attention(r) for r in hidden)
        note = ", all clean" if quiet else ""
        # **Which SIDE they are on, which `+N more` never said** (#741). The count was
        # always right — `hidden` is the complement of what was drawn, true at every
        # offset — but `more` asserts a direction the window has not earned. Driven at
        # offsets 0, 4 and 8 over twelve repos in five rows, the old line read
        # `…(+8 more, all clean)` all three times: eight below, four-and-four, and eight
        # ABOVE are three different places to be standing and the row said one thing.
        # Scrolled to the bottom it pointed under itself at nothing.
        #
        # The window is one window over one list — repo rows then piece rows, the order
        # the budget above is spent in — so *offset* IS how many rows are above it and
        # what is left of `hidden` is below. Taken from the numbers the rows were sliced
        # with rather than re-derived: a second arithmetic for "where is the window" is a
        # second answer, which is `_Line`'s own reason one column over.
        above = offset
        below = len(hidden) - above
        # **Each count is drawn only for a side that HAS something on it** — `_bar`'s rule
        # one axis over, where a leading `+3` beside a page starting mid-list is what stops
        # a lone trailing count claiming the row holds the first names. `0 above` would be
        # a field that is always false and always drawn.
        where = ", ".join(f"{n} {side}" for n, side in
                          ((above, "above"), (below, "below")) if n)
        # **About no repo, so a click here selects nothing.** It stands for the rows that
        # are NOT on screen, and picking one of them because the operator clicked the line
        # that says they exist would be charter answering a question nobody asked.
        #
        # **Last, under the rows it is about.** It used to be appended before the piece
        # rows, which could not be told apart while `capped` and a piece row were mutually
        # exclusive — they no longer are, and a "there is more below" line with three more
        # rows below it is the note pointing at the wrong place.
        lines.append(_Line(None, tui.truncate(
            f"  {sl._DIM}…({where}{note}){sl._R}", width)))
    return lines[:budget]


def _unknown_lines(width: int) -> list[str]:
    """What the repo table draws when charter has not gathered this frame's repos YET —
    which is a different claim from "this workspace has no repos", and #512 is the whole
    cost of drawing them the same.

    `cmd_launch` deletes the cache before it draws anything (`gather.discard`, so a
    recycled pid cannot adopt a dead frame's rows) and a detached refresh fills it a beat
    later (`commands_frame._spawn_gather`). Between the two there is a real window in
    which the honest answer is "not known yet" — and an empty table is not that answer,
    it is a confident `no repos` on a plane that may have fourteen. That is the same
    false-clean reading `_table_lines` refuses for a starved pane ("too narrow for the
    table is NO table, not a cut one") and the same one the `left` sidebar was retired
    for in #488; this is it said out loud instead of implied by absence.

    **A pane too narrow for the table says nothing here either**, which matters: a line
    that appeared while the rows were unknown and vanished the moment they were known
    would read as "the repos went away". That is not enforced HERE, though — the width
    rule for this pane lives in :func:`_table_cap`, which answers 0 below
    `statusline._LEFT_W`, and `_repos` already refuses to compose anything on a budget of
    0 (and since #515 `layout.visible_slots` does not even split the pane at that width).
    A second copy of the rule in this function would be unreachable through the only
    caller there is, and a guard no test can turn red is exactly the kind that passes
    because a DIFFERENT guard caught it.

    One line, always: there is exactly one thing to say and repeating it down a tall pane
    would be padding. Bounded through `tui.truncate` like every other line in this module
    — `⋯` and `…` are both East-Asian *Ambiguous*, the class `statusline._persona_chips`
    records as having "broken this layout twice", so their width is measured rather than
    counted.
    """
    from .. import statusline as sl

    return [tui.truncate(f"  {sl._DIM}⋯ gathering this workspace's repos…{sl._R}", width)]


def _unreadable_lines(fid: str, ws: str, width: int) -> list[str]:
    """What the `repos` pane draws when this frame HAS a cache file and cannot read it.

    **The third state, and #735 is the cost of not having had one.** `gather.cached`
    answers ``None`` four ways — no frame directory, no cache file, a file that is not
    JSON, a file that parses to something that is not a scan — and :func:`_unknown_lines`
    was drawn for all four. The first two are a gather that has not landed yet, which is
    a five-second wait; the last two are a gather that will never land, because a panel
    "never gathers on its own — it reads the cache or says it has none"
    (`docs/frame.md`). Spelled identically, `⋯ gathering this workspace's repos…` was
    permanent on a pane with nothing else on it.

    **What separates them is a fact, not a duration** — `gather.unreadable`, which asks
    the filesystem about the same file the caller has just failed to read. The rejected
    alternative was to time-box the gathering message, and that is a guess wearing a
    fact's clothes: a plane with forty clones on a cold mount crosses any N that a corrupt
    file crosses, and this pane would then call a slow gather broken, which is worse than
    calling a broken one slow. `gather.save` replaces the file atomically, so there is no
    half-written moment for this line to appear in either.

    **It names the command, because the operator inside a stuck frame cannot derive it.**
    `charter frame-gather` requires `--session` and `--workspace` — required on purpose, so
    a detached child never guesses a frame's workspace (#512) — and neither is discoverable
    from a pane. Both are filled in here from the frame being drawn, which is the shape
    :func:`_empty_lines` already has and the reason it has it: a line that names a problem
    and not its fix costs a row and settles nothing. The palette carries the same command as
    a row (`frame/builtin_actions._register_regather`); this line is what an operator who
    has not opened the palette reads, and it does not name a key because a frame running
    as a window in an operator's own tmux has no `F2` of charter's to name.

    **Nothing is repaired here.** A renderer that deleted the file would put a write on the
    repaint path #387 pinned at one `stat`, and would destroy the evidence of whatever wrote
    it. The pane says what is true and the operator decides.

    One line, bounded through `tui.truncate` like every other line in this module — *fid*
    and *ws* are both arbitrary length. **The bound is about that length and not about
    content**, since #752: `_repos` reaches this branch only past `workspace.exists(ws)`,
    which name-checks before it asks the filesystem, so a `$CHARTER_WORKSPACE` no rung ever
    checked now reaches :func:`_gone_lines` instead and never this one. See
    :func:`_empty_lines`, which says the same thing at length about the line beside it.
    """
    from .. import statusline as sl

    return [tui.truncate(
        f"  {sl._DIM}unreadable repo cache · charter frame-gather --session{sl._R} "
        f"{fid}{sl._DIM} --workspace{sl._R} {ws}", width)]


def _gone_lines(ws: str, width: int) -> list[str]:
    """What the `repos` pane draws when this frame's workspace is not on disk at all.

    **The fourth state, and #752 is the cost of not having had one.** Every other sentence
    this pane can say is about a workspace that is THERE: `_unknown_lines` is a gather that
    has not landed in it, `_unreadable_lines` is a cache that will not read for it,
    :func:`_empty_lines` is a scan of it that found no clones. A frame outlives the
    directory it draws — `charter workspace remove`, a `git clean` on the plane, a
    teammate's pull and a plain `mv` all take one away while a frame is running, and a
    frame is by definition long-lived enough for that to happen — and absence was spelled
    with whichever of those three the cache happened to reach. The reported reading is the
    first: `0 todos`, no rows, and `⋯ gathering this workspace's repos…` for as long as the
    frame stayed up, because a panel never gathers on its own and none of the four ways a
    workspace can be taken away is something charter can hook.

    **Asked from the two branches that were about to say "nothing here", and never above
    the cache.** ``gather.json`` lives under `.charter/` and not under `workspaces/`, so a
    removed workspace leaves its last scan exactly where it was — and this line does NOT
    override it. A panel "never gathers on its own — it reads the cache or says it has
    none" (`docs/frame.md`), so a renderer that contradicted its own cache from a `stat`
    would be re-deriving, on every repaint of every frame, state the gather owns. What that
    leaves is a table that is stale rather than a pane that is wrong about which state it
    is in, and it converges on its own: `gather.scan` reads `workspace.clones`, which
    answers nothing for a workspace with no directory, so the first refresh after the
    removal empties the cache and this line is what the pane draws.

    **A fact at the moment it is asked**, `workspace.exists` — which is `gather.unreadable`'s
    rule for the noun one level up, and for the same reason: the alternative is to infer
    absence from an empty scan, and "gathered, nothing there" and "there is nothing to
    gather" are two different claims. #512 is what drawing two claims as one sentence
    already cost this pane once.

    **It names the command that makes the workspace exist again**, which is
    :func:`_empty_lines`' shape and its reason — a line that names a problem and not its fix
    costs a row and settles nothing. `charter workspace create <ws>` is the same remedy
    `commands_workspace` already prints for this fact (`no workspace 'x' (create it: …)`),
    and it is the one that works whether or not the workspace was LIVE; `restore` needs a
    committed manifest, which a LOCAL workspace never has. No key is named, for
    :func:`_unreadable_lines`' measured reason: a frame running as a window in an
    operator's own tmux has no `F2` of charter's to offer.

    **Nothing is repaired by drawing.** Re-creating the directory would put a write on the
    repaint path #387 pinned at one `stat`, would hide whatever removed it, and would hand
    back a workspace with none of the clones it had. The pane says what is true and the
    operator decides — `_unreadable_lines`' rule, and the same one that makes a RENAMED
    workspace a different answer entirely: `workspace.rename` repoints the chat, so this
    line is never how a rename shows up (#795).

    One line, bounded through `tui.truncate` like every other line in this module. *ws* is
    arbitrary length AND arbitrary content — `state.workspace_for`'s last rung hands back
    `$CHARTER_WORKSPACE` untouched (see :func:`_empty_lines` on exactly this) — so the
    `tui.sanitize` under that truncate is what keeps a newline in the name from making this
    pane two rows tall.
    """
    from .. import statusline as sl

    return [tui.truncate(
        f"  {sl._DIM}no workspace{sl._R} {ws}{sl._DIM} · charter workspace create "
        f"{ws}{sl._R}", width)]


def _empty_lines(ws: str, width: int) -> list[str]:
    """What the `repos` pane draws once the gather HAS run and found no clones.

    **A pane of its own cannot be silent about this, and that is what #515 changed.**
    While the table shared `bottom` with the attention row, a workspace with no clones
    simply produced no table rows and the pane was the one-line strip it always was —
    absence said it. Split into its own bordered component, the same silence is an empty
    rectangle, and an empty rectangle is a claim nobody made: it reads as a table that
    failed to draw rather than as a workspace with nothing in it.

    So it is said, with the command that changes it — the shape every `statusline._alerts`
    row already has, because a line that names a problem and not its fix costs a row and
    settles nothing. *ws* is the FRAME's workspace (`_frame_workspace`), the same one the
    count was taken from, so the command names the workspace the pane is actually about
    rather than whatever this process would have resolved for itself (#512).

    Distinct from :func:`_unknown_lines` on purpose: "not gathered yet" and "gathered,
    nothing there" are two different claims and #512 is the whole cost of drawing them
    the same. That one is reached when `gather.cached` answers ``None``; this one when it
    answers a real scan with no rows in it.

    **No `contain.one_line` over *ws*, and since #752 the reason is LENGTH rather than
    content.** It used to be neither, and the true reason is worth spelling twice. It was
    never that every rung of `state.workspace_for` checks its answer against
    `instance.WORKSPACE_NAME_RE` (`^[A-Za-z0-9][A-Za-z0-9._-]*$`) — rung 0 does, through
    `valid_name`, and the LAST rung does not: it is `workspace.resolve()`, which hands back
    `$CHARTER_WORKSPACE` stripped and otherwise untouched, so
    `CHARTER_WORKSPACE='ev\\nil\\x1b[31m;rm -rf /'` really did arrive here verbatim.

    **It cannot any more, and that is a fact about the CALLER and not about this line.**
    `_repos` reaches this branch only past `workspace.exists(ws)`, which asks `valid_name`
    before it asks the filesystem (`workspaces/..` is a real directory, so a
    filesystem-only predicate would answer "present" for a name that is not a workspace).
    So the name in this sentence has passed the alphabet, which holds no newline and no
    ESC — and :func:`_gone_lines` is where a name nobody checked now lands, and where the
    `tui.sanitize` half of the argument moved with it.

    What is NOT bounded by that check is the WIDTH: a workspace name is arbitrary length,
    and an untruncated line here is a pane wider than itself, which wraps into the second
    row this slot must never have. So the line is measured through `tui.truncate` like
    every other line in this module — for the reason stated, and not for one that stopped
    being true, because naming the wrong reason is how a guard gets deleted later against
    a property that was never the one it had.
    """
    from .. import statusline as sl

    return [tui.truncate(
        f"  {sl._DIM}no clones in{sl._R} {ws}{sl._DIM} · charter clone <repo> -w "
        f"{ws}{sl._R}", width)]


def _too_narrow_lines(width: int, pad: int = 0) -> list[str]:
    """What the `repos` pane says when it exists but is narrower than its own table.

    **Reachable only by a RESIZE, and that is the whole reason it exists.** At launch
    `layout.visible_slots` does not split this pane below `layout._table_min_cols()` at
    all — a bordered rectangle with nothing in it is the "no repos" lie #515 removed. But
    `cmd_resize` re-sizes panes; it does not create or destroy them, so narrowing a
    running frame's terminal leaves a `repos` pane that `_table_lines` correctly refuses
    to draw a cut table into. Before this, that was a blank bordered row: the same lie,
    reached by dragging instead of by launching.

    So the pane says what it is. One line, the width it needs stated as a number the
    operator can act on, and read from `statusline._LEFT_W` rather than spelled here for
    the reason `layout._table_min_cols` gives: the number that stops the table drawing and
    the number this line quotes must be the same one.

    **And *pad* is added to it, because the number has to be the width the PANE needs.**
    `statusline._LEFT_W` is what the TABLE needs; a component that asked for ``pad = 3``
    spends six cells of its pane before the table is composed at all
    (:func:`content_width`), so a 97-column pane refuses and a message quoting 95 sends the
    operator to widen a terminal that is already wide enough. They widen, it still refuses,
    and the config key that caused it is not on screen anywhere. With the pad in the sum
    the number is one they can act on: widen by it and the table draws. Zero by default, so
    every pane that has never named a pad quotes exactly what it quoted before.

    Bounded through `tui.truncate` like every other line here — this line is drawn at
    widths below 95 by definition, and `⋯` is East-Asian *Ambiguous*.
    """
    from .. import statusline as sl

    return [tui.truncate(
        f"  {sl._DIM}⋯ too narrow for the repo table — {sl._LEFT_W + 2 * pad} columns "
        f"needed{sl._R}", width)]


def _table_cap(fid: str, width: int) -> int:
    """The most rows of repo table `_repos` will draw in a *width*-column pane —
    **every reason the renderer draws fewer rows than there is content**, in one place.

    Three of them, and until #500 only the first was written down where the LAUNCHER
    could read it:

    * **Too narrow is no table at all.** Below `statusline._LEFT_W` (95)
      :func:`_table_lines` refuses outright rather than trimming a row into a false-clean
      `charter  main`, and that is not an exotic width: every ordinary 80-column terminal
      is here. Since #515 that answer is also what `layout.visible_slots` reads (through
      `layout._table_min_cols`) to decide the `repos` pane is not split at all at those
      widths — one number, two decisions, rather than a renderer that refuses and a
      launcher that splits a pane for the refusal to sit in.
    * **`terse` keeps :data:`_TERSE_ROWS`.** A density that buys the harness back its
      rows has to buy them from the slot that has rows to give, so `minimal` asks for a
      SHORTER pane, not the same pane with more of it blank.
    * **`statusline._MAX_REPO_LINES` bounds the rest**, the same total-row budget the
      wide table keeps (repo rows plus the `…(+N more)` line, not repo COUNT — see that
      constant's own comment), so a workspace with forty clones gets fourteen rows of
      table rather than forty.

    **Both sides of "how tall is the table pane?" call this, and that is the whole
    point.** :func:`repos_rows_wanted` asks it to size the pane before it exists;
    :func:`_repos` asks it with the pane's own measured width to bound what it draws
    into it. A cap applied on one side only is exactly the defect #500 fixes: the sizer
    used to answer from the repo count alone, so an 80-column frame with six repos got a
    seven-row pane to draw one line in, and `minimal` on a wide terminal got an
    eleven-row pane to draw five — rows taken from the harness and left blank, and
    re-taken by `cmd_resize` on every subsequent resize.

    Takes *width* rather than measuring it: the renderer has a pane to measure and the
    launcher has only a window it has not split yet. **Both must be the PANE's width, and
    that is not always the window's** — `layout.repos_cols` is where the launcher's side
    of it is computed, because a `[frame] slots` naming `right` before `repos` insets
    this pane by 23 columns (measured on tmux 3.7c: 87 in a 110-column window). Sizing
    from the window's width there is the second half of the same defect, and it is what
    round 2 of #500 shipped.
    """
    from .. import statusline as sl
    if width < sl._LEFT_W:
        return 0
    cap = sl._MAX_REPO_LINES
    if verbosity(fid) == "terse":
        cap = min(cap, _TERSE_ROWS)
    return cap


def repos_rows_wanted(fid: str, *, pane_cols: int) -> int:
    """How many rows `_repos` would fill for this frame in a *pane_cols*-column PANE.

    **One answer to "how tall is the table pane", read by both sides of the question.**
    The renderer spends the pane it was given (:func:`_table_lines`' *budget*); the
    LAUNCHER has to decide how tall to make that pane before any panel exists, and the
    `window-resized` recompute has to decide again with the window's new size. If those
    two disagreed, a frame would come up with a pane taller than its content (blank rows
    the harness could have had) or shorter (a table cut off with nothing saying so) —
    and both were shipped by #488, because this asked the repo count and nothing else
    while the renderer also asked the width and the density. Pinned by a test that
    renders `_repos` into a pane of exactly this height, at exactly this width and at
    every density, and counts the lines that come back.

    *pane_cols* is required, and keyword-only so no caller can pass a row count by
    mistake. A pane narrower than `statusline._LEFT_W` draws no table at all and is not
    split at all (`layout.visible_slots`), so the zero this answers there is the honest
    one rather than a pane sized for nothing — see :func:`_table_cap`, which is where
    every reason the renderer draws fewer rows than there is content now lives.

    **The PANE's columns, not the window's, and the name is the guard.** Round 2 of #500
    called this argument `cols` and every caller handed it the window's width, which is
    only this pane's width when nothing took columns off it first: with a `[frame] slots`
    of `["right", "top", "bottom", "repos"]` a 120-column window gives a 97-column table
    pane (measured, tmux 3.7c), so a six-repo plane was split seven rows tall for a table
    the panel then refused to draw. The number is `layout.repos_cols`' answer, and the
    keyword is what makes the old, wrong call a `TypeError` at the call site rather than
    a frame with six blank rows in it — the same discipline `window_cols=` already
    applies one level up.

    **The :data:`_HEAD_ROWS` added is the pane's own heading, and it is no longer the
    attention row.** This used to add that row, because `bottom` drew it and the table
    into one pane; they are two panes now (`bottom` is the one-row strip it was before
    #488, and `layout.SLOT_SIZE` states its height as the constant it is). What the row buys here
    is `▪ repos N` — the same heading `_right` gained in #516, from the same
    `_sidebar_head`, so the frame's two bordered components are labelled the same way and
    the tree beneath it has something to hang from. Chrome the density does not buy back,
    exactly as the sidebar's heading is not (`_TERSE_ROWS` bounds the table's ROWS; the
    label is what the component is).

    Zero is a real answer, and `layout.repos_rows` is what turns it into the one-line
    pane that says so (`_empty_lines`): a workspace with no clones, or a gather that has
    not run yet. Those two lines carry no heading — `▪ repos 0` above "no clones in demo"
    would be the same fact twice in a two-row pane.

    `gather.row_count` is what makes this affordable at launch: it answers from the
    frame's cache when there is one and from a plain directory listing when there is not,
    and never runs a git sweep. See its own docstring. It is asked SECOND, after the
    width test that can answer zero, so a narrow frame does not pay for a count it is
    about to discard.

    **The component's own pad comes off *pane_cols* first, and #500 is why it is here
    rather than nowhere.** `_repos` composes at :func:`content_width`, so a padded pane's
    table is planned for `pane_cols - 2 * pad`; asking `_table_cap` for the unpadded width
    would size this pane from a number the renderer never sees, which is exactly the
    sizer-and-renderer disagreement #500 shipped twice. In the band between
    `statusline._LEFT_W` and `_LEFT_W + 2 * pad` that difference is the whole answer: the
    unpadded question says "a table fits, give it eight rows" and the pane then draws one
    line saying it is too narrow. :func:`pad_for` is the same function `pad_of` is, asked
    with the launcher's width instead of a pane it cannot measure.
    """
    from . import gather
    cap = _table_cap(fid, pane_cols - 2 * pad_for("repos", pane_cols))
    if cap <= 0:
        return 0
    rows = gather.row_count(fid)
    return _HEAD_ROWS + min(rows, cap) if rows else 0


#: Columns the persona NAME cell is never squeezed below once the badges have a column
#: of their own — the two-cell marker plus enough of a name to tell two personas apart.
#:
#: The badge column is sized from the WIDEST badge on screen, so without a floor one
#: persona holding three dispatches — ` ✎47 ⚡3 2h?`, twelve columns once `⚡`'s two cells
#: are counted — would take all twelve off every NAME in a 22-column sidebar, leaving ten
#: for the marker and the name together (`▫ reddit-o…`). Past this point the BADGE column
#: is what gives way, and a badge that no longer fits is cut by its own cell rather than
#: by pushing a name out of the pane.
_NAME_MIN_W = 12

#: The most rows the sidebar's todo section may occupy — its heading, its items and its
#: `…(+N more)` line together.
#:
#: A cap on top of the pane's own height, because the two answer different questions: the
#: pane says how many rows there ARE, and this says how many of them a todo list has any
#: business taking. `right` is the persona column first — that is what the slot is for
#: everywhere else charter names it — so on a forty-row terminal the todos get a section
#: and the rest of the pane stays quiet, rather than the list running to the bottom of the
#: screen. `charter ws todo` is where the whole list lives; this is the frame's reminder
#: that it does, not a replacement for it.
_MAX_TODO_LINES = 8

#: The most rows the sidebar's `changes` section may occupy — its heading, its rows and
#: its `…(+N more)` line together.
#:
#: :data:`_MAX_TODO_LINES`' argument, one section over: the sidebar is the persona column
#: first, so a list that ran to the bottom of a forty-row terminal would be taking the
#: pane from what it is for. `charter change show <slug>` is where a whole change lives
#: (§3.4 calls it the monorepo view); this is the frame's answer to *"what am I in the
#: middle of"*, not a replacement for it.
_MAX_CHANGE_LINES = 4

#: The screen column every row's content starts in, everywhere in the frame — one
#: constant, read, rather than a `"  "` spelled at each call site.
#:
#: It is `statusline._HEAD_PAD`'s own width, and `TheInsetIsOneConstant` pins the two
#: equal rather than this module importing `statusline` at import time (every other
#: reference here is a deliberate function-level import — a panel repaints on a clock and
#: `statusline` is the largest module charter has). So the number lives here and the
#: agreement is a test, which is the same trade `panel._DEFAULT_ROWS` makes with
#: `slots._DEFAULT_ROWS`.
#:
#: "Content is inside something" is what an inset says, and it says it for free: no row
#: is added, no width changes, and the value is the one already in use. What changes is
#: that a renderer added next year asks for it instead of typing two spaces, which is how
#: the sidebar's sections came to line up in the first place (`_HEAD_PAD`'s own comment
#: in `statusline.py` records the two ways that shipped broken).
INSET = 2


def _inset(marker: str = "") -> str:
    """*marker* fitted to exactly :data:`INSET` columns — a row's own left edge.

    `tui.pad`, never `marker + " "`: `_persona_chip_cells`' own comment records that this
    layout has twice been broken by a glyph a font drew wider than the Unicode tables
    claim, and `pad` measures with `tui.width` so a two-cell marker takes the inset it
    already has rather than pushing its row one column right of every other.
    """
    return tui.pad(marker, INSET)


#: The narrowest content a padded pane will accept. Below it the pad is dropped WHOLE.
#:
#: :data:`_NAME_MIN_W` rather than a number of this function's own: it is the narrowest row
#: the frame composes anywhere — a persona's two-cell marker plus enough name to tell two
#: personas apart, and the floor past which the sidebar gives up its badges instead. A pane
#: left with less than that is not a narrower panel; it is an empty one, and a frame with a
#: hole in it is what every refusal in `layout.visible_slots` and `component_tables` exists
#: to prevent (#535: a missing repo table is a plane that appears to have no clones).
_PAD_MIN_CONTENT = _NAME_MIN_W


def pad_for(name, cols: int) -> int:
    """The pad *name* asks for, afforded against a pane of *cols* columns.

    **The width is passed in for :func:`_table_cap`'s own reason**, and it is the same
    reason twice: the RENDERER has a pane to measure (:func:`pad_of`, below) and the
    LAUNCHER has only a window it has not split yet (`repos_rows_wanted`, which runs in a
    different process on a different terminal and would measure the operator's shell if it
    measured anything). One function, two widths, so the pane the launcher sizes and the
    pane the renderer draws into cannot come to disagree about how many cells the pad took.
    A second copy of this arithmetic on the launcher's side is exactly the shape #500
    shipped when the sizer answered from the window's width and the renderer from the
    pane's.

    See :func:`pad_of` for the argument about where the cells come from and why an
    unaffordable pad is dropped whole rather than clamped.
    """
    from .. import config, instance
    pad = instance.component_style(config.FRAME, name)["pad"]
    return pad if cols - 2 * pad >= _PAD_MIN_CONTENT else 0


def pad_of(name) -> int:
    """Cells of horizontal inset this pane draws, each side — the pad *name* can AFFORD.

    **Padding is charter's to draw, and that follows from where the surface comes from.**
    tmux paints the pane's background (`instance.FRAME_PANE_BG`, pane options, no cost on
    a repaint) and tmux insets nothing: there is no pane option that moves content off the
    rectangle's left edge. So the inset is composed, here, where the rows are.

    **It comes OUT of the content budget and is never added beside it**, which is the one
    thing about this that could not be decided by taste. `statusline._row_plan` gives up
    whole cells in a written-down order when the repo table is narrow, and #506 is what a
    row composed for one width and painted at another costs — the CI marker falls off the
    right-hand end and a failing repo renders clean. A pad added on top of a full-width
    plan is that defect with a new cause, and `chrome.fill`'s own measurement says what
    happens at the other end: **W+1 shears the pane**, one cell of overflow wrapping every
    row below it onto the next line. So the renderer is told a narrower pane, plans for the
    narrower pane, and the cells it gave up are the cells the pad occupies.

    **And it is dropped WHOLE below :data:`_PAD_MIN_CONTENT`, not clamped down to fit.**
    Clamping would be a value read, validated and then quietly changed into a different one
    — the shape `instance.FRAME_PANE_PAD_MAX` refuses at the config boundary, said again
    one layer down — and the operator would have no way to tell a pane that took their
    three cells from one that took one. Whole-or-nothing is also the rule the table beside
    it already keeps for a marker (`statusline._row_plan`: shown whole or dropped whole).

    **What it does NOT do is keep a renderer above that renderer's OWN floor**, and that is
    deliberate rather than missed. `repos` refuses to draw a cut table below
    `statusline._LEFT_W` and says so; a pad that pushes a 97-column pane under that line
    gets the refusal, and the operator's own ``pad`` is what put it there. Teaching this
    function each renderer's minimum would be a second copy of `layout._table_min_cols()`
    drifting from the first (#547). Two things carry that instead, and both are the
    existing number asked with the pad in it rather than a new one:
    :func:`repos_rows_wanted` sizes the pane from the width the RENDERER will see, so a
    frame does not open a tall pane for a table that will refuse; and
    :func:`_too_narrow_lines` quotes the width the PANE needs, so the operator reads a
    number they can act on and widening by it works.

    `layout.visible_slots` is deliberately left alone, and the consequence is bounded and
    stated: between `_table_min_cols()` and `_table_min_cols() + 2 * pad` the launcher
    still splits a `repos` pane, and what the operator gets there is a one-row pane saying
    how many columns it needs. That is a pane that says what is wrong, which is what
    `_too_narrow_lines` exists to be; the alternative — a slot silently absent — is the
    "no clones" lie #515 removed.

    Read at call time through `config.FRAME`, never cached, exactly as :func:`verbosity`
    reads the density: a panel repaints on a version bump and a relaunch is what changes
    this. `instance.component_style` is the one walk over the arrangement, shared with the
    launcher (`commands_frame._split_panels`), so the pane that gets a colour and the pane
    that gets an inset cannot come to disagree about which component they are.

    **What a repaint pays for this, measured rather than waved at.** Per paint, a padded
    pane costs one `component_style` walk inside :func:`content_width` (which measures the
    tty exactly as often as it did before) and, on `render`'s way out,
    one more walk plus one more `TIOCGWINSZ`. On this machine: `component_style` is
    **1.22µs** for the last of four placements (`right`, the worst case) and 0.27µs for
    the first, and `os.get_terminal_size` on a real pty is **1.27µs**. Call it 3µs against
    the **4 816µs** one `render("right")` already costs — the number `ANIMATED`'s own
    comment records, and the budget it exists to protect. **0.06%.** Nothing here is on
    the idle path at all: a panel at rest is one `stat` (`state.version`) and paints
    nothing, and this runs only when something is being painted.
    """
    return pad_for(name, _width())


def content_rect(name, cols: int) -> tuple[int, int]:
    """*name*'s canvas inside a pane *cols* wide: the column its cells START in, and how
    many there are.

    The first number is what :func:`inset_rows` puts in front of every row; the second is
    what :func:`content_width` tells the renderer. They were derived separately for as
    long as only the renderer needed them, and #607's pointer half is what made the pair
    one question: a click arrives in the PANE's coordinates and a component draws in
    THESE, so `frame/events.py` needs the origin and the width or it translates a click
    by one pad into a rectangle sized by another.

    **The width is passed in rather than measured here**, which is :func:`pad_for`'s own
    reason a second time: the renderer has a pane it measured through :func:`_width`, and
    `events.Dispatcher._on_canvas` has one it measured through `pane.size()` — because it
    must be able to tell a pane it could NOT measure from one that is 80 columns wide, and
    `_width`'s stated fallback cannot say which it was. One arithmetic, two callers, each
    supplying the reading it is entitled to.

    What this does NOT claim is that the pad and the width a PAINT uses come from one
    reading: `panel._component_text` still asks :func:`content_width` and then
    :func:`inset_rows`, which measures again. That window is older than this function and
    is not closed by it.
    """
    pad = pad_for(name, cols)
    return pad, cols - 2 * pad


def content_width(name) -> int:
    """The cells *name*'s renderer may compose into — the pane's width less its pad.

    **Two questions, kept apart.** :func:`_width` answers *how wide is this pane*, measured
    from the pane's own tty and reported as measured, with no floor clamped over it; this
    answers *how wide is my canvas*, which is the first question minus what the operator
    asked to leave empty at each edge. Every renderer wants the second, which is why this
    is what they call — and why :func:`_width`'s own contract is untouched: a caller that
    needs the rectangle (`panel._hold`, painting a failure into a pane whose renderer is
    the thing that failed) still gets the rectangle.

    Never negative for a pad the pane can afford — :func:`pad_for` has already dropped one
    it cannot — and `tui.truncate` answers ``""`` for a non-positive width regardless, so
    the arithmetic here needs no second guard of its own.

    The pane is measured ONCE and handed to both halves of :func:`content_rect`, rather
    than `_width() - 2 * pad_of(name)`: that spelling asks the tty twice, and worse, asks
    it twice at two different moments — a `SIGWINCH` landing between them would compose a
    row from one width and inset it by a pad afforded against another.
    """
    return content_rect(name, _width())[1]


def inset_rows(text: str, name: str) -> str:
    """*text*'s rows moved right by *name*'s pad — the other half of :func:`content_width`.

    **Takes finished rows and must not hand them back to `tui`.** `chrome.fill`'s module
    docstring has the measurement: `tui._finish` strips trailing whitespace *including*
    whitespace hiding behind trailing SGR, so a row that went through here and then back
    through a `tui` node would come back at its natural width with the inset gone. This is
    called on `render`'s way out, after every renderer has finished, for that reason.

    **The pad goes on the LEFT of the whole row, outside every span**, which is what makes
    it background rather than paint. A row that ends in an open span (`chrome.reverse`
    composes one) keeps its highlight to the content's last column and no further, so a
    selected row is inset like every other row instead of bleeding into the margin — which
    is also the only answer consistent with the pad coming out of the budget, since the
    highlight was composed at :func:`content_width` and has no cells beyond it to fill.

    The right-hand pad is not spelled at all and does not need to be: nothing is written
    there, so it is the pane's own background — tmux's paint, the cells no renderer wrote,
    already filled before charter's first byte reached the pane.

    **A pad of zero returns *text* unchanged, and there is no early return saying so.**
    ``if not pad: return text`` was written first and deleted: ``" " * 0`` is ``""`` and
    ``"\\n".join(s.split("\\n"))`` is ``s``, so the line below already answers a pad of
    zero with the string it was given — the deletion sweep found the guard surviving,
    correctly, because it could not change an outcome. `fill`'s two deleted guards are the
    same finding in the same shape, and `TheDeletedEarlyReturnStaysDeleted` is what keeps
    the property those guards were reaching for pinned without the guard.

    Deleted rather than kept as a fast path, because the cost it saves is not one this
    module spends: this is one pass over a pane's worth of rows, a few microseconds
    against `render`'s measured 4 816µs, and `panel._write` splits the same string again
    one call later regardless.

    **`replace` rather than a split and a join, and the sweep is why.** The obvious
    spelling — ``"\\n".join(lead + line for line in text.split("\\n"))`` — carries an
    equivalent mutant that no test could ever pin: `str.split` and `str.rsplit` with no
    ``maxsplit`` are the same function, so `tools/sweep.py`'s ``swap-synonym`` operator
    turns one into the other and nothing anywhere can tell. That is not a missing test, and
    "add a suppression" is the move this repo has no mechanism for on purpose. Saying it
    without a split says the same thing and leaves nothing to swap: **every newline gains
    the lead after it, and the first line gains it in front.** Verified equal to the split
    form on every case these rows can take — empty, no newline, trailing newline, blank
    lines, a bare ``\\r`` — at every pad including zero.
    """
    lead = " " * pad_of(name)
    return lead + text.replace("\n", "\n" + lead)


def _sidebar_head(label: str, count: int, width: int) -> str:
    """One section heading for the `right` sidebar — ``▪ personas 6``.

    Composed from `statusline.py`'s OWN header constants (`_HEAD_PAD`, which carries
    `_MARK_HEAD`, and `_DIM`) rather than a string of this module's, so the frame's
    sidebar and the status line's persona column cannot come to disagree about what a
    column header looks like — the same delegation this module already practises with
    `_markers`, `_ci_part` and the table's column widths.

    Lower case, and a bare word with no glyph of its own. The frame's chrome is lower case
    wherever it speaks (`no personas`, `3 todos`, `F2 palette`), and `_HEAD_PAD`'s own
    comment in `statusline.py` records that a decorative glyph on a header shipped broken
    twice: a header is the one row with no sibling beneath it to reveal that a font drew
    it wider than the Unicode tables claim.

    **The label is BOLD and the count stays dim, and no row is added.** A region with a
    name is what makes a pane read as part of an application rather than as output, and
    weight is the whole of what it costs here: `tui.strip_ansi` sees `▪ personas 6`
    before and after, the width arithmetic is untouched (SGR is zero columns), and the
    fifteen-plus tests that assert a panel's exact LINE COUNT never learn this happened.
    A heading ROW is the change with the widest blast radius in the frame and it buys
    nothing weight does not.

    Bold on the label only. The marker keeps the dim it had — it is furniture, not the
    name — and the count keeps it too, so the heading reads as one word with a number
    after it rather than as two equal facts.
    """
    from .. import statusline as sl
    return tui.truncate(
        f"{sl._DIM}{sl._HEAD_PAD}{sl._R}{sl._BOLD}{label}{sl._R}{sl._DIM} {count}{sl._R}",
        width)


def _persona_total(cells) -> int:
    """How many personas *cells* stands for: the rows that name one, plus the personas a
    `…(+N more)` row admits it is standing in for.

    Added up from `PersonaChip.hidden` rather than read back out of the rendered note,
    which would tie a heading's number to the wording of a sentence — the same reason
    `statusline._persona_chip_cells` records a flagged persona in a set instead of looking
    for its glyph afterwards.
    """
    return sum(1 for c in cells if c.name is not None) + sum(c.hidden for c in cells)


def _cap_personas(cells: list, keep: int) -> list:
    """*cells* trimmed to at most *keep* rows, the last of them saying how many personas
    it dropped.

    **One trimmer for both reasons the sidebar shows fewer personas than exist** — a
    `terse` density asking for less, and a pane too short to hold the list. "Which
    personas survive" is one question, and answering it in two places is how the two
    answers come to disagree; a short pane at `terse` asks it once here.

    `_persona_chip_cells` is already ordered — `persona.by_use`: the plane's declared
    default, then most-dispatched first (#882) — so what survives is the top of an order
    rather than an arbitrary handful, and a row that was ALREADY standing for hidden
    personas folds its count into the new row rather than losing it.

    **That order used to be "the active persona first", and the trim reads no worse for
    the change.** Both orders put something worth seeing at the top; this one puts the
    same thing there on every frame, so a sidebar shortened by a density key drops the
    persona nobody dispatches rather than whichever name happened to be next after the one
    the operator was standing on. The active persona can now be among what is dropped —
    it is still named on the top bar (`identity`, `◆ <name>`), which is the row that
    answers "who am I being" when the column cannot.
    """
    from .. import statusline as sl
    if keep <= 0 or len(cells) <= keep:
        return cells
    shown = cells[:max(0, keep - 1)]
    hidden = _persona_total(cells[len(shown):])
    return [*shown,
            sl.PersonaChip(None, f"{sl._DIM}{_inset()}…(+{hidden} more){sl._R}", "",
                           hidden)]


def _badge_width(cells: list, width: int) -> int:
    """How many columns the badge column takes on a *width*-wide persona table.

    The widest badge ON SCREEN, measured with `tui.width` and never `len` — `✎ ◌ ⚑ ✗ ⚡`
    and the vault dot are exactly the glyphs `statusline._persona_chip_cells`' own comment
    says have broken this layout twice — bounded by :data:`_NAME_MIN_W` so a persona with
    three dispatches in flight cannot take a 22-column sidebar's names away from it.

    **Extracted because two passes need the same number and must not compute it twice.**
    :func:`_persona_rows` lays the cell out and :func:`persona_section` publishes the
    column it starts at, so a click can tell a badge from a name; a second copy of this
    arithmetic is a second answer to "where does the badge column begin", and the two
    disagree the first time either is edited. It is the same reason `_Chips` is published
    by the pass that composed the rows rather than re-derived when a click arrives.

    **The floor is on the OUTSIDE of the `min`, and that position is the deletion sweep's
    finding rather than a preference.** Written `min(widest, max(0, width - _NAME_MIN_W))`
    — which is where this arithmetic sat for as long as only `_persona_rows` used it — the
    clamp is an exact equivalent: measured over 3,280 cell configurations at every width
    from 0 to 40, dropping it changed not one drawn row and not one resolved click,
    because a negative width fell into `_persona_rows`' own ``badge_w <= 0`` branch and
    was treated as zero there. A guard whose effect another branch already provides is a
    line no input can turn red.

    Outside the `min` it is this function's own contract instead — *a column is never a
    negative number of cells* — which is one assertion on one call
    (`_badge_width(cells, 8) == 0`) rather than a property of whatever the caller does
    next. It also makes :func:`persona_section`'s ``width - badge_w`` honest: a negative
    badge width published a badge column starting PAST the pane, which no click could
    reach and which was therefore wrong in the way that only shows up later.

    The two forms are equal wherever the old one was defined — the widest badge is never
    negative, so `min(widest, 0)` and `max(0, min(widest, negative))` are both zero.
    """
    return max(0, min(max((tui.width(c.badges) for c in cells), default=0),
                      width - _NAME_MIN_W))


def _persona_rows(cells: list, width: int) -> list[str]:
    """The persona chips drawn as a TABLE: names down the left, badges in a right-hand
    column of their own (#516).

    The badges used to start wherever a name happened to end, so the column was ragged
    and one long name pushed its own badge past every other. The column's width is the
    widest badge ON SCREEN — measured with `tui.width`, never `len`, because `✎ ◌ ⚑ ✗ ⚡`
    and the vault dot are exactly the glyphs `_persona_chip_cells`' own comment says have
    broken this layout twice — bounded by :data:`_NAME_MIN_W` so a persona with three
    dispatches in flight cannot take a 22-column sidebar's names away from it.

    **The name is contained BEFORE the arithmetic, not after** (#472). Neither number
    here is derived from a persona name: the badge column comes from the badges, the name
    column is the pane's remainder, and `tui.Cell`'s own `pad`/`truncate` is what fits the
    name into the column it was given — sanitising it on the way, which is the containment.
    A row whose name is too long loses its own tail to a `…` and moves nothing else.

    Badges keep the leading separator `_mem_badge`/`_health_mark`/`_inflight_badge` each
    put on themselves, so the cells are joined with no gap and `head + badges` is still
    byte-for-byte what `statusline._persona_chips` renders.
    """
    badge_w = _badge_width(cells, width)
    rows: list[str] = []
    for c in cells:
        if badge_w <= 0 or c.name is None:
            # A `…(+N more)` row names no persona and carries no badge — it is a sentence
            # about the list, so it spans the pane rather than being padded into a name
            # cell whose column it has no business lining up with.
            rows.append(tui.truncate(c.head + c.badges, width))
        else:
            rows.append(tui.Row(tui.Cell(c.head, width - badge_w),
                                tui.Cell(c.badges, badge_w), gap="").render(width)[0])
    return rows


def todo_total(data: dict) -> int:
    """How many todos this workspace has OPEN, unclipped — the number the `▪ todos N`
    heading carries and the number the palette's reader counts against.

    `gather._MAX_TODOS` bounds the LIST the cache holds; `todo_count` is the count of what
    was there before the bound. A cache written by an older charter has the items and no
    count, so this falls back to what it can actually see rather than reporting zero hidden
    todos it cannot name — and it refuses a count SMALLER than the list, which is a cache
    whose two halves disagree.

    **Named because two surfaces read it** (#742). The sidebar's heading is one, and
    `frame/builtin_actions._read_todo` — the palette row that reads the todos the sidebar
    had no rows for — is the other. Spelled twice, the reader would have said `3/20` under
    a heading saying `todos 400`, which is the drift a shared answer cannot have. Takes the
    snapshot rather than a fid: the sidebar has `gather.read`'s and an action has its
    `ctx.gather`, and neither should reach for the other's.
    """
    items = [t for t in (data.get("todos") or []) if isinstance(t, dict)]
    raw = data.get("todo_count")
    # `max` and not `raw if raw >= len(items) else len(items)`, which is what this said
    # while it lived inside `_todo_rows`. The two are the same function — at `raw ==
    # len(items)` both arms answer the same number — so `>=` there could be flipped to `>`
    # by the deletion sweep and nothing downstream could tell, which is a comparison no
    # test can turn red. Written as the bound it is, there is no branch left to get wrong.
    return max(raw, len(items)) if isinstance(raw, int) else len(items)


def _todo_rows(data: dict, width: int, budget: int) -> list[str]:
    """This workspace's open todos, heading included, in at most *budget* lines.

    **Read from the gather cache, never from the todo directory** — *data* is
    `gather.read(fid)`'s, and `gather.scan` is what opened and parsed those files, once,
    on a plane-state bump. `todos.open_todos` is one file read per todo; a panel repaints
    on every version bump and `bottom` repaints five times a second while work is in
    flight, so a renderer that called it would be the per-row filesystem work #387 pinned
    a panel's idle tick against. Same rule, same cache, same reason as the repo table.

    **Which ones, and how many.** `open_todos` is oldest-first and that ordering is the
    point of it — what surfaces is what is being avoided, rather than what you already
    have in mind — so the rows are its own top, not a sample. The section spends
    *budget* in the same priority order `_table_lines` spends its own: the heading, then
    items, then the `…(+N more)` line that admits what was dropped, which is RESERVED out
    of the budget rather than appended and trimmed off the end. A budget of exactly two
    therefore says the count and how much is hidden, because "there is more here than
    fits" outranks "here is an arbitrary one of them".

    **The `…(+N more)` line has a route beside it now, and it is a keypress** (#742). It
    used to advertise hidden content that nothing on the machine could reach: no click, no
    key, no palette row — only `charter workspace todo` typed into the harness, which is
    the surface the frame exists to replace. `frame/builtin_actions._register_todos` is
    that route — one repeatable palette row that reads the open todos out, in full, on the
    palette's own header. It is deliberately a KEY and not a wheel: `[frame] mouse` is off
    on most planes, so a pointer-only answer would be inert on exactly the frames this was
    reported from, and the row itself must go on answering nothing (`_Chips.hit`'s rule for
    the persona column's own overflow line — a row that stands for items it does not name
    cannot resolve to one).

    **Nothing at all when there is nothing open**, unlike `_bottom`'s `0 todos`. That row
    is one row on a strip that is never blank anyway; this is a heading plus an empty
    space in a column, which is furniture within a day — and then a real todo appearing in
    it draws no more attention than the zero did. `_bottom` keeps its unconditional count,
    so the frame still says `0 todos` somewhere: the two surfaces are not saying the same
    thing twice, they are saying it at two different costs.

    **Done todos never appear**, which is the whole of what `open_todos` returns: a closed
    todo is not something the frame is asking you to look at, and a list you have to read
    past is not a list you act on.

    A todo's title is a COMMITTED value — someone else's machine wrote it into this
    plane's repo — so `contain.one_line` bounds it BEFORE any width arithmetic touches it,
    which is the ordering #472 was filed for. The width maths here does not read the title
    at all (`tui.truncate` fits the finished row to the pane), so the bound cannot be
    walked around by a title long enough to matter.
    """
    from .. import contain, statusline as sl
    items = [t for t in (data.get("todos") or []) if isinstance(t, dict)]
    # `_HEAD_ROWS` for the heading, and one row under it: a heading with nothing beneath
    # claims this workspace has no todos, which is the false-clean reading this module
    # refuses everywhere. The constant rather than a literal `2` for :data:`_HEAD_ROWS`'
    # own reason.
    if not items or budget <= _HEAD_ROWS:
        return []
    total = todo_total(data)
    room = budget - _HEAD_ROWS              # the heading is not negotiable
    shown = items[:room]
    if total > len(shown):
        shown = items[:max(0, room - 1)]    # reserve the line that says how many are left
    rows = [_sidebar_head("todos", total, width)]
    for t in shown:
        title = contain.one_line(t.get("title") or "")
        # An ASCII `-`, and the bullet every eye reaches for first (`·`, `•`) is exactly
        # the one this column may not have: both are East-Asian *Ambiguous*, so a terminal
        # may draw either two cells wide and shift every todo title one column right of
        # the persona names above them — the drift `_persona_chip_cells`' own comment says
        # has broken this layout twice. `-` is Narrow everywhere, which is
        # `_inflight_badge`'s own reason for spelling presumed-dead as `?`.
        #
        # Two columns of marker, so a title begins in the same column as a persona name
        # and as both headings — `_HEAD_PAD` exists to make that one column, and a section
        # whose rows started somewhere else would undo it. :data:`INSET` is that column,
        # asked for rather than spelled: a marker plus a hand-typed space is the same
        # arithmetic done again, and the second copy is the one that moves.
        rows.append(tui.truncate(f"{sl._DIM}{_inset('-')}{sl._R}{title}", width))
    hidden = total - len(shown)
    if hidden > 0:
        # The count alone, with no command beside it — unlike `_persona_chip_cells`'
        # `…(+N more · charter persona list)`, which is written for a status line 36
        # columns wide at its narrowest. This pane is 22 (`layout.SLOT_SIZE`), and
        # `…(+4 more · charter ws todo)` is 28: the command would be cut off on every
        # frame charter actually draws, and half a command name is worse than none —
        # it is a thing to type that does not work.
        rows.append(tui.truncate(f"{sl._DIM}{_inset()}…(+{hidden} more){sl._R}", width))
    return rows


class _Hit(NamedTuple):
    """What `_Chips.hit` resolved a click to — which of the two things, and about whom.

    A pair rather than two methods, because *which cell did the pointer land in* is one
    question with one answer and asking it twice is how a renderer and a handler come to
    disagree. `builtins._persona_events` branches on :attr:`explain` and never re-derives
    it.

    **A bool rather than a pair of named string kinds, and the deletion sweep is why.**
    This was ``kind: str`` against two module constants (``SWITCH = "switch"``,
    ``EXPLAIN = "explain"``), and the sweep reported both spellings as survivors — rightly
    so: every producer and every consumer went through the constant, so the VALUE was
    unobservable and any string would have done. That is a literal nothing can turn red.
    A bool has no spelling to get wrong, and flipping it is caught by the first case that
    clicks a badge.

    The column matters more than the vocabulary here: there are exactly two things on a
    persona row, they are decided by which cell the pointer landed in, and a third would
    be a different question (which cell?) rather than a third value of this one.
    """

    #: True when the pointer was in the BADGE cell — say what the glyphs mean. False when
    #: it was in the name cell — adopt this persona.
    explain: bool
    name: str


#: What a click on the badge column puts on the attention row — #753, answered in the
#: frame rather than in `charter docs show frame`.
#:
#: **One legend for every row, not a readout of the row that was clicked**, and that is a
#: deliberate limit rather than the first draft of a better version. `PersonaChip.badges`
#: is a RENDERED string by the time this module sees it; saying what *this* persona's
#: badges mean would take either decoding those glyphs back out — the drift
#: `statusline.PersonaChip`'s own docstring exists to prevent — or a handler reading the
#: plane, which §4f forbids it (`on_event` is handed no ctx, precisely so a handler cannot
#: grow a second reading of a plane the repaint is about to read anyway). A legend needs
#: neither, and the question an operator actually has — *what is that glyph* — is the one
#: it answers.
#:
#: Kept to the width of an ordinary frame's attention row on purpose: `state.say` hands
#: this to `_bottom`, where it is the top-priority field and is still `tui.truncate`d to
#: the pane. The full table, with the sentence each of these compresses, is
#: `docs/frame.md`'s — this is the reminder, not the reference.
BADGE_LEGEND = ("◦ no usable vault · ! vault unhealthy · ⚑ draft charter · "
                "✗ broken config · ✎ memories · ◌ session notes · ⚡ in flight")


class _Chips:
    """Which persona each ROW of the sidebar's persona column is about, and who you are.

    Also where the BADGE column starts, so a click can tell *be this persona* from *what
    is that glyph* (#753) — see :meth:`hit`.

    `_Tabs` on the other axis, and the argument for existing is the one that class makes
    for a bar and `_Viewport` makes for the table: a pointer event arrives as a NUMBER,
    and the only thing that knows what is at that number is the pass that composed the
    rows. :func:`persona_section` is that pass.

    **The column needs a published map for `_Tabs`' reason — the ladder is not
    invertible.** :func:`_cap_personas` drops the tail of an ORDER (the active persona
    first, then anything carrying a health mark) and replaces it with one `…(+N more)`
    row, and `terse` caps the list again at :data:`_TERSE_ROWS`; so "which persona is on
    row 4" cannot be worked back from the roster and the pane's height, and re-deriving it
    when a click arrives would be a second answer to what is on screen, disagreeing with
    the first the moment a repaint lands in between.

    **The map and the mark are written by ONE call**, which is `_Tabs.publish`'s rule and
    `_Viewport.blank`'s finding: a column that published its rows and left a stale *here*
    would answer "you are already there" for a persona the operator can see is not in
    reverse video.

    **Rows that name no persona hold ``None``, and rows below the section are out of
    bounds**: the `▪ personas N` heading and the `…(+N more)` line are the first kind (it
    stands for personas that are *not* drawn — `_Tabs` refuses its `+14` for the identical
    reason); the `no personas` sentence, the blank separator rows the sidebar puts between
    its sections, and every todo and change row below them are the second. Both answer
    ``None`` out of :meth:`switch_to`, which is where the whole refusal lives.

    **It is `_Viewport`'s sequence rather than `_Tabs`' mapping, and the sweep is what
    settled that.** The first version filtered the empty rows out of a dict, and the
    deletion sweep reported the filter as an exact equivalent — an absent key and a
    ``None`` value were indistinguishable to everything downstream, so the guard was a
    line no input could turn red. A bar's columns really are sparse (a `_BAR_GAP` belongs
    to nothing); a column of rows is not. See :meth:`publish`.

    One object at module scope for `_Tabs`' reason. Note that the `sidebar` composite and
    the standalone `personas` component both reach it through the same
    :func:`persona_section` call, and the persona rows are the FIRST thing either pane
    draws — so one base of row 0 is right for both, and neither caller offsets anything.
    """

    __slots__ = ("_rows", "_here", "_badge_at")

    def __init__(self) -> None:
        self._rows: tuple[str | None, ...] = ()
        self._here = ""
        self._badge_at = 0

    def forget(self) -> None:
        """Back to a column nobody has drawn — for a test, and only a test."""
        self.publish((), "", 0)

    def publish(self, rows, here: str, badge_at: int) -> None:
        """Record which persona the paint that just happened put on each row.

        *rows* is indexed by a row of the component's own canvas — `[frame] pad` insets
        columns only (`inset_rows`), so a pane row and a composed line are the same number
        here — and holds the RAW persona name, never the drawn one. `_persona_rows` runs
        every name through `tui.Cell`, which contains and may truncate it (#472); what
        goes into `charter frame-switch --persona` has to be the name on disk.
        `_Tabs.publish` is this decision one axis over.

        **A SEQUENCE with ``None`` in it, which is `_Viewport.publish`'s shape and
        deliberately not `_Tabs`'.** A bar's columns are sparse — a `_BAR_GAP` is two
        cells belonging to nothing — so a mapping is right there and absence is the
        answer. A column of rows is dense: every row of this section was drawn, and the
        ones that name no persona (the `▪ personas N` heading, the `…(+N more)` line) are
        rows with nothing on them rather than rows that are not there. `_Viewport.publish`
        spells exactly this with its leading ``None`` for the table's own heading.

        It also stops "no persona here" having two spellings. A dict with the empty rows
        filtered out has both an absent key and — if a filter is ever dropped — a ``None``
        value meaning the same thing, and the deletion sweep found that filter to be
        precisely equivalent: nothing downstream could tell the two apart, so the guard
        was a line no input could turn red.

        *here* is the persona this frame is ON, taken from `PersonaChip.active` in the
        same paint that drew the reverse-video row — data the chips carry, never a search
        for `▸` or magenta in a rendered head, which is `persona_section`'s own rule for
        the highlight itself. The map and the mark are written by ONE call for
        `_Tabs.publish`'s reason.

        *badge_at* is the first column of the badge cell `_persona_rows` laid out, or
        ``0`` when the pane was too narrow to give the badges a column of their own — the
        rung where `_badge_width` answers zero and every row is drawn full width. It is
        the third thing this one call writes, for the same reason the mark is: a paint
        that moved the badge column and left a stale offset behind would explain a glyph
        for a cell the operator clicked a NAME in.
        """
        self._rows = tuple(rows)
        self._here = here
        self._badge_at = badge_at

    def hit(self, row: int, col: int):
        """What a click at canvas *row*, *col* on this column means, or ``None``.

        **Two answers, because the persona column has two things on it** (#753). A row is
        a NAME with a BADGE COLUMN to the right of it, and the two are asking different
        questions: the name is *be this persona*, the badges are *what is that glyph*.
        `_persona_rows` already draws them as two cells, so resolving a click against
        which cell it landed in is reading the layout that is on screen rather than
        inventing a second one.

        A hit with :attr:`_Hit.explain` false is the front door `builtins._persona_events`
        starts; a true one puts :data:`BADGE_LEGEND` on the attention row through
        `state.say`, which is the dwell #729 built and the reason this needs no surface of
        its own.

        **Explaining works on the persona you already are, and switching does not.** *"What does
        the flag on my own row mean"* is the commonest form of the question this answers,
        and nothing about it is a no-op — where re-adopting the persona you are being is
        exactly the nothing `_Tabs.switch_to` refuses, and is also what keeps a
        double-click from switching twice.

        **The bounds check is spelled out, and `_Viewport.repo_at` is why**: a tuple
        indexed with a negative number answers the LAST row — a wrong answer that hides a
        wrong reading — so a row outside what was painted is refused here rather than
        wrapped around. `_Tabs` gets this structurally from a mapping; a sequence does
        not, and this is the price of the shape :meth:`publish` argues for.

        **A row that names no persona answers ``None`` for BOTH kinds**, badge column or
        not: the `▪ personas N` heading and the `…(+N more)` line hold ``None``, and
        `_persona_rows` draws the second of those full width with no badge cell at all —
        so a click past where the badges would have been is a click on a sentence about
        the list, which explains nothing and switches nothing.

        ``_badge_at`` of ``0`` is the rung where the pane was too narrow to give the
        badges a column (`_badge_width` answered zero); the ``> 0`` test is what keeps
        every column on such a row a name rather than making column 0 a badge.
        """
        name = self._rows[row] if 0 <= row < len(self._rows) else None
        if name is None:
            return None
        if self._badge_at > 0 and col >= self._badge_at:
            return _Hit(True, name)
        return None if name == self._here else _Hit(False, name)


#: The one persona column this process draws. See :class:`_Chips`;
#: `frame/builtins._persona_events` is the other half and holds no state of its own.
CHIPS = _Chips()


def persona_section(width: int, height: int, *, terse: bool) -> list[str]:
    """The sidebar's persona rows: the heading, and every chip a *height*-row pane fits.

    Named because the registry names it — `frame/builtins.py` places this as the
    `personas` component, one of the two parts of the `sidebar` composite. It is
    :func:`_right`'s own first half, moved rather than rewritten: `_right` calls it, so
    there is one implementation of "what the persona column says" and not a component's
    copy sitting beside the panel's.

    Takes *terse* rather than reading `verbosity(fid)` itself, so the pane that draws BOTH
    sections asks the frame's density once and hands the same answer to each. Two reads
    would be two small file reads per repaint where one was enough, and — worse — two
    chances for the halves of one pane to disagree if the density changed between them.

    A plane with no personas at all says so in one line. It is not an empty column:
    an empty column is indistinguishable from a pane that failed to draw, which is the
    reading the frame refuses everywhere else.

    **The active persona's row is the whole row, in reverse video, to the pane's edge**
    — the element that most makes a list read as a list you are *in* rather than a list
    you are looking at. Three things about where it is applied:

    * **To the FINISHED row `_persona_rows` returns**, never to the cells it composes.
      Line 892 is a `tui.Row(...).render(width)[0]` and `tui._finish` deletes a pad that
      goes in before it — measured, a 40-cell highlighted row back at 15 (`chrome.py`).
    * **Here, not in `_right`**, so the composite pane and its `personas` component reach
      it through the same helper. `tests/test_builtin_components.py:272` asserts raw byte
      equality between `slots.render("right")` and its two parts joined; a highlight
      applied one level up would be the one thing the two sides did not share.
    * **From `PersonaChip.active`**, which the chips carry as data — not by looking for
      `▸` or magenta in a rendered head.

    It needs no `[frame] chrome`: reverse video names no colour (it is the operator's own
    foreground and background exchanged), so it is right on every theme, and tmux
    converts even its own colours to reverse on a client that has none.
    """
    from . import chrome
    from .. import statusline as sl
    cells = sl._persona_chip_cells()
    if not cells:
        # **The rung that draws no persona publishes too**, which is `_Viewport.blank`'s
        # whole finding and `_bar.row`'s discipline: a pane that said `no personas` while
        # a map from the paint before it was still standing would switch this frame to a
        # persona nobody can see on the row that was clicked.
        CHIPS.publish((), "", 0)
        return [tui.truncate(f"{sl._DIM}no personas{sl._R}", width)]
    keep = height - 1                       # the heading takes a row off the list
    if terse:
        keep = min(keep, _TERSE_ROWS)
    cells = _cap_personas(cells, keep)
    rows = _persona_rows(cells, width)
    # The leading `None` is the `▪ personas N` heading, which belongs to no persona —
    # `_Viewport.publish`'s own `(None, *…)` for the table's heading, one section over.
    # The `…(+N more)` row already carries `name is None` from `_cap_personas`, so it
    # lands as a row with nothing on it without a filter here: it stands for the personas
    # that are NOT on screen, which is why `_Tabs` refuses its `+14` too.
    # `_badge_width` rather than a second copy of its arithmetic: `_persona_rows` lays the
    # cell out at exactly this column and a click has to resolve against what was DRAWN.
    #
    # **Zero when there is no badge column, spelled out rather than left to fall out of
    # the subtraction.** `width - 0` is `width`, which is a column no click can reach and
    # would therefore have degraded correctly by accident — the kind of accident that
    # stops being one the first time either side of it is edited. `_Chips.hit` reads this
    # as "was a badge cell drawn at all", so it has to be a number that says so.
    badge_w = _badge_width(cells, width)
    CHIPS.publish((None, *(c.name for c in cells)),
                  next((c.name for c in cells if c.active), ""),
                  width - badge_w if badge_w else 0)
    return [_sidebar_head("personas", _persona_total(cells), width),
            *(chrome.reverse(row, width) if c.active else row
              for c, row in zip(cells, rows))]


def todo_section(fid: str, width: int, budget: int, *, terse: bool) -> list[str]:
    """This workspace's open todos, in at most *budget* rows — the `todos` component.

    :func:`_right`'s own second half, moved for the reason :func:`persona_section` gives.
    *budget* is what the pane has left after the personas; the density's own cap
    (:data:`_TERSE_ROWS` or :data:`_MAX_TODO_LINES`) is applied here rather than by the
    caller, so a caller that has rows to spare cannot spend more of them on todos than
    the density allows.

    **The cache is opened only when there is a row to draw it into.** `gather.read` is
    reached after the budget is known to be positive, never before — a pane with no room
    must not open a file it is about to discard, which is the same ordering
    :func:`repos_rows_wanted` keeps and the same reason.

    How SHORT is too short is `_todo_rows`' rule and is asked there once: this decides
    only whether there is any room at all.

    **The workspace is the FRAME's, handed in, never one the panel resolved for itself**
    (#526) — :func:`_frame_workspace`, the rule every other surface of the frame asks.
    `gather.read` falls through to `gather.scan` on a cold cache, and `scan` with no
    workspace argument calls `workspace.resolve()` **in the panel process**, which #512
    established reaches none of the rungs that can speak for the frame: `$CHARTER_WORKSPACE`
    arrives empty by design (#411), the cwd is the plane root, the per-session pointer is
    keyed on the FRAME id, and the per-terminal pointer on the panel's own `$TMUX_PANE`.
    It lands on the declared default. So a frame's first paint listed **`default`'s** open
    todos under this plane's `▪ todos N` heading — and that is worse here than the blank
    repo table #512 fixed, because a populated list reads as an answer: three todos an
    operator has never seen read as three todos they have.

    **The scan fallback stays, and that is a decision rather than an inheritance.** #525
    took it away from `bottom` and gave that slot a `⋯ gathering…` placeholder instead,
    on the grounds that `bottom` is the one ANIMATED slot — it repaints five times a
    second while work is in flight, and a cold `scan()` is a git sweep per repaint. The
    sidebar is not animated: one scan, on one paint, and a column that draws its todos
    immediately is worth having. The two slots therefore read the cache by two different
    rules, deliberately, and the difference is the repaint rate rather than the section.
    """
    from . import gather
    budget = min(budget, _TERSE_ROWS if terse else _MAX_TODO_LINES)
    if budget <= 0:
        return []
    return _todo_rows(gather.read(fid, workspace=_frame_workspace(fid)), width, budget)


def _right(fid: str) -> str:
    """The plane's personas, and this workspace's open todos underneath them.

    Persona chips carry memory badges, in-flight badges and vault dots because
    `statusline._persona_chip_cells` already builds them all, and this calls it
    rather than reassembling the same facts out of
    `_mem_badge`/`_inflight_badge`/`_inflight_by_persona`/`_vault_dot`/
    `_mem_count` itself. A fix to any one of those five lands here the moment it
    lands in the status line's right column — nothing in this module could drift
    from it, because nothing in this module repeats what it does.

    **#516 asked for the badges in a column, and that is why the parts exist.**
    `_persona_chips` returns one flat string per persona, with the name, the vault dot
    and all three badges already concatenated — nothing left to put in a column. The
    honest fix was upstream: `statusline` grew `PersonaChip`, `_persona_chips` became
    ``head + badges`` over it, and this reads the same parts the status line composes.
    Recomposing the chip HERE out of the five helpers would have satisfied the same
    screenshot while reintroducing exactly the drift the paragraph above rules out.

    No per-turn session id to hand it: unlike `statusline.render`, a frame panel
    never receives Claude Code's JSON payload on stdin (see `gather.py`'s own
    module docstring for the identical point about the gather side).
    `_persona_chip_cells()`'s own default (`session=None`) is exactly right here —
    its ephemeral-memory count falls back through `charter.session.current`,
    which reads `$CHARTER_SESSION_ID`/`$CLAUDE_CODE_SESSION_ID` out of the
    environment a launched panel process inherits whole from the harness (the
    same env var `notify.plane_changed` already depends on being set there).

    No guard of its own around the call: `_persona_chip_cells` already swallows
    every failure internally and answers `[]` (its own docstring's "never
    breaks the status line"), and `render`'s caller-side `try/except` covers
    whatever gets past that — the same trust `_top`/`_bottom` already place in
    it rather than each re-wrapping their own calls into `statusline.py`.

    **Two headings, because a bare column of names told a newcomer nothing** (#516) —
    `_sidebar_head` is where their register is argued, and it is `statusline.py`'s own.
    The personas heading counts every persona this plane has, not the rows that fit:
    `_persona_total` adds the hidden ones back from `PersonaChip.hidden`, so the heading
    and the `…(+N more)` line beneath it can never contradict each other.

    **The pane is measured, and the personas are served first.** `_cap_personas` bounds
    the list by `_height()` — a `terse` density and a short pane reach it through the same
    call — and the todos take what is left, capped by :data:`_MAX_TODO_LINES`. That
    ordering is deliberate rather than incidental: `right` is the persona column
    everywhere else charter names it, so a pane too short for both loses the section that
    is duplicated elsewhere (`charter ws todo`, and `bottom`'s own count) rather than the
    one that is not. How SHORT is too short is `_todo_rows`' own rule and is asked there
    once — this function only decides whether there is any room at all to be worth
    opening the cache for, which is a question about a file read rather than about a row.

    **The two sections are :func:`persona_section` and :func:`todo_section`**, and this is
    what composes them into one pane. The registry says the same thing in its own
    vocabulary — `frame/builtins.py` registers them as the `personas` and `todos` parts of
    the `sidebar` composite, with `personas` taking what is left (`Fill()`) and the todos
    capped — and it says it by pointing at these two functions, not by keeping a second
    copy of them. Charter never splits a pane (§4d); a composite is how two things share
    one, and this function is that composition for the one composite charter ships.
    """
    w, h = content_width("right"), _height()
    # Asked ONCE and handed to both sections: one pane draws them, so one density decides
    # how much both of them say.
    terse = verbosity(fid) == "terse"
    lines = persona_section(w, h, terse=terse)

    # The blank row between the two sections is counted OUT of the todo budget rather
    # than added on top of it, so the section can never be the row that overflows the
    # pane — the same reservation `_table_lines` makes for its own overflow line.
    rows = todo_section(fid, w, h - len(lines) - 1, terse=terse)
    if rows:
        lines.extend(["", *rows])
    # The changes go LAST, and the order is a priority rather than a preference: the
    # personas are what this column is for, the todos are what this workspace is being
    # asked to do, and a change is the coarser "what am I in the middle of" that
    # `charter change show` answers in full. Each section reserves its own blank row out
    # of what is left, so the one that overflows is always the lowest-priority one.
    changes = changes_section(fid, w, h - len(lines) - 1)
    if changes:
        lines.extend(["", *changes])
    return "\n".join(lines)


def _inflight_field() -> str:
    """`⠙ 2 running`, or nothing at all when nothing is.

    **The one moving thing in the frame, and it moves only while work is genuinely in
    flight.** `inflight` records work when it STARTS and clears it when it ends (see
    that module's docstring — the completion tally cannot answer this), so "is anything
    running right now" is a question about files on disk rather than about anything
    charter has to keep in memory or poll for. `panel._running` is what decides whether the
    panel repaints often enough for the spinner to move; this function only decides what
    the row says, and it says nothing when there is nothing to say. Empty means the field
    is dropped whole by `_fit_fields`, which is what makes idle completely still: no
    spinner, no zero, no furniture.

    **`kind=None` — every kind of work, and #420 is the whole of it.** #387 promised a
    spinner for "a dispatch, clone or `gl-refresh`" and delivered dispatches, because
    `inflight.start` had one caller. It now has three, and this is the one reader that
    asks for all of them: the row counts records and never names them, so a clone and a
    dispatch are both simply "running" here. The readers that NAME what is running —
    the dispatch-overlap nudge, the per-persona chips — keep `inflight`'s dispatch-only
    default, which is what stopped this from being a one-line change (see that module's
    own docstring for the sentence a `clone` record would otherwise have produced).

    Presumed-dead records get their own, DELIBERATELY STATIC piece. A record past
    `inflight.PRESUMED_DEAD_SECONDS` is one nobody should still be expecting (its process
    was killed, or PostToolUse never fired), and animating it would claim progress that is
    not happening — and would do it for up to `PRUNE_SECONDS`, a full day of a spinning
    panel on an idle machine. `⋯` is the still glyph for exactly that, and it is also the
    retreat the whole animation falls back to if it ever measures badly.

    Not `statusline._session_news`'s job, which is why `_bottom` asks it for everything
    EXCEPT this (`inflight=False`): that helper's `⚡ N` is the same fact drawn for a
    surface that repaints once per turn, where a spinner would be a still picture of a
    random frame. Two surfaces, two ways of drawing one fact, one place each — rather than
    both on the same row saying it twice.
    """
    from .. import inflight, statusline as sl
    records = inflight.live_records(kind=None)
    if not records:
        return ""
    running = sum(1 for _agent, _started, dead in records if not dead)
    stalled = len(records) - running
    parts = []
    if running:
        parts.append(f"{sl.accent('warn')}{spinner_frame()} {running} running{sl._R}")
    if stalled:
        parts.append(f"{sl._DIM}⋯ {stalled} stalled{sl._R}")
    return " ".join(parts)


#: The attention row's fields a density may not trim — see :func:`_fit_fields`' *exempt*.
#:
#: **One name, and the set is the point rather than the name.** `hotkey` is the only field
#: on that row that is chrome rather than news, and #743 is what ranking it against news
#: did: `density = minimal` — the arrangement with no repo table, no sidebar and no
#: version, where `F2` is the only route to the repo table, the todos, the workspace, the
#: persona and getting back to `full` — was the one arrangement that stopped saying `F2`
#: exists. A frozenset rather than a `name == "hotkey"` in the loop so that the question
#: "which fields are chrome" is one a test can ask directly, and so the next field of this
#: kind joins a list rather than an `or`.
_ALWAYS = frozenset({"hotkey"})


def _fit_fields(priority: list[tuple[str, str]], width: int,
                limit: int | None = None,
                exempt: frozenset[str] = frozenset()) -> set[str]:
    """Which names among *priority* — an ordered list of ``(name, text)`` pairs, highest
    priority first — fit *width* once joined with `` · ``, decided one at a time in that
    order: each field's FULL text counted against what is left, included whole or
    dropped whole, never character-sliced. The first non-empty field is always kept even
    when it alone is over budget (so a row this feeds is never blank on its own account —
    a trailing `tui.truncate` at the call site is what actually protects the pane in that
    case, the same "safety net, not the fix" role it plays in :func:`_row`).

    Factored out of :func:`_bottom` so this priority-and-width logic is testable against
    exactly the fields a test wants, in isolation — `_bottom` also always carries a todo
    count and a hotkey hint that would otherwise compete for the same narrow budgets an
    adversarial test needs to construct.

    *limit* caps how many fields survive REGARDLESS of width — how a `terse` density asks
    for less on a row that has room for more. It reuses this one priority order rather
    than `_bottom` keeping a second, shorter list of its own: "which field matters most"
    is one question, and answering it twice is how the two answers come to disagree about
    whether an alert outranks a todo count. ``None`` (the default) is "as many as fit",
    which is exactly what every caller wanted before densities existed.

    *exempt* names fields *limit* does not count and does not stop — **and it is a
    different KIND of field, not a favoured one** (#743). Every other field on this row is
    news about the plane: an alert, a spinner, a selection, a todo count. Ranking them
    against each other is exactly right, and at `terse` keeping only the loudest is the
    density doing its job. The hotkey hint is not news; it is the one thing on screen that
    says how to drive the frame — and `minimal` is the arrangement where the palette is
    the only route to anything, including back to `full`. Ranking it against news is what
    dropped it from the only frame that needed it, leaving a two-strip frame advertising
    nothing at all to an operator who closed and reopened their terminal.

    **Width still applies to an exempt field**, which is what keeps this an exemption from
    the density and not from the arithmetic: a starved pane drops the hint like anything
    else, because a hint sliced in half is the false-clean failure this whole function
    refuses. And the loop `continue`s past a capped field rather than breaking out of it,
    so an exempt field LAST in the priority order is still reached — which the hotkey is,
    being the one field that is always rediscoverable another way.
    """
    sep_w = tui.width(" · ")
    budget = width
    keep: set[str] = set()
    capped = 0
    for name, text in priority:
        if not text:
            continue
        if limit is not None and name not in exempt and capped >= limit:
            continue          # over the density's budget — but a later exempt field is not
        need = tui.width(text) + (sep_w if keep else 0)
        if need > budget and keep:
            continue          # doesn't fit and something already does — drop it whole
        keep.add(name)
        if name not in exempt:
            capped += 1
        budget -= need
    return keep


def _selected_detail(fid: str) -> str:
    """The selected repo said in WORDS — the attention row's right-hand field, or ``""``.

    **The `repos` table shows glyphs because it has four columns to fit forty repos into;
    this has one row and one repo, so it spends it on words.** `!` and a number, a coloured
    branch cell and a CI mark are a table's vocabulary and they are learned; `dirty`,
    `2 ahead` and `CI failed` are what somebody who has just pointed at a row wants read
    back to them. Nothing here is a second SOURCE — every field comes out of the same
    gather row `_table_row` drew, so the two surfaces cannot disagree about a repo, only
    about how much space they have to say it in.

    **`clean` is `_needs_attention`'s answer and not a separate reading of the same
    fields.** A detail that listed the problems and said nothing when there were none would
    be an absence standing in for a claim — the reader cannot tell "nothing is wrong" from
    "this field was cut". The predicate is the one the `…(+N more), all clean` note already
    asks, so the row and the note cannot come to disagree about what clean means.

    **Empty for every plane that has never selected anything**, which is what keeps this
    field free: `_fit_fields` drops an empty field whole, so the attention row of a frame
    nobody has clicked is byte-identical to the one before this existed.

    **What a repaint pays.** One `state.selection` read — a `read_text` of a file that is
    usually not there — and, only when it IS, one `gather.cached`. That second read is the
    one worth stating, because `bottom` is the frame's ANIMATED slot and repaints at
    `panel.TICK` for the whole length of a dispatch: with a row selected, that is one small
    JSON read five times a second, against the `_inflight_field` record scan the same tick
    already pays. It is charged only to a frame whose operator has selected something, and
    it is the same cache the `repos` pane beside it reads on every one of its own paints.

    **`gather.cached` and never `gather.read`, for `_repos`' reason exactly**: `read` falls
    back to a live `scan()`, and a panel must not sweep. A frame whose gather has not landed
    yet has nothing to say about the selected repo and says nothing, rather than walking
    every clone on the plane five times a second to find out.
    """
    from . import gather, state

    name = state.selection(fid)
    if not name:
        return ""
    data = gather.cached(fid)
    if data is None:
        return ""
    # `data["repos"]`, with no `or []` under it: `gather.cached` answers `None` for
    # anything whose `repos` is not a LIST (`gather._shaped_like_a_scan`), so a fallback
    # here could only ever stand in for an empty list with an empty list. The sweep found
    # it surviving, correctly — a line that cannot change an outcome is not documentation
    # of an intent.
    row = next((r for r in data["repos"] if r.get("name") == name), None)
    if row is None:
        # A selection pointing at a repo this plane no longer has. The table drew no
        # highlight for it either (`_table_lines` matches on the same name), so both
        # surfaces go quiet together — which is the honest pair. Saying "gone" here would
        # be the only thing on screen claiming a repo ever existed.
        return ""
    return _detail_text(row)


def _detail_text(row: dict) -> str:
    """*row*'s state as the words :func:`_selected_detail` puts on the attention row.

    Split out so the composition can be tested against a row without a plane, a frame
    directory or a gather cache underneath it — the same reason `_fit_fields` is not inside
    `_bottom`. It is also the whole of what a reader has to check against `_table_row`: two
    surfaces, one set of fields, and this is the list.

    Every part is dropped when it is not true, so a clean repo on its own branch reads
    `▪ charter · main · clean` and a busy one reads
    `▪ charter · fix/x · dirty · 2 ahead · CI failed · !123 · ⑂3`.
    """
    from .. import statusline as sl

    parts = [f"{sl._BOLD}{row.get('name')}{sl._R}", row.get("branch") or "?"]
    if row.get("dirty"):
        parts.append("dirty")
    if row.get("ahead"):
        parts.append(f"{row['ahead']} ahead")
    if row.get("behind"):
        parts.append(f"{row['behind']} behind")
    if row.get("ci"):
        parts.append(f"CI {row['ci']}")
    if row.get("change"):
        parts.append(f"{row.get('sigil') or '!'}{row['change']}")
    if row.get("worktree_count"):
        parts.append(f"⑂{row['worktree_count']}")
    if not _needs_attention(row):
        parts.append("clean")
    return f"{sl._DIM}▪{sl._R} " + f"{sl._DIM} · {sl._R}".join(parts)


def _bottom(fid: str) -> str:
    """What still wants attention, and how to act on it — the frame's last row.

    **One row, always, and #515 is what made that true again.** Between #488 and now this
    pane carried the attention row AND the repo table stacked underneath it with nothing
    between them, which is exactly what the operator reported: two different kinds of
    thing sharing a pane, and the eye with nothing to separate them by. The table has its
    own bordered pane now (:func:`_repos`), and this is the one-row strip it was before —
    isolated the way `top` is, and sitting on the terminal's LAST row (see
    `layout.panel_argvs` for the split order that puts it there, measured).

    **Last row rather than first, and it is a property rather than a preference.** The
    table's height is its content's (`layout.repos_rows`), so whichever of the two sits
    below the other moves up and down the screen as repos are cloned, go dirty, or drop
    off. Anchoring the ATTENTION row is worth more than anchoring the table: it is the
    surface #488 protected from eviction, and an alert you have to go looking for is one
    you read late. So the table floats and the alert does not.

    Only the first alert, deliberately: `_alerts()` already returns its entries in
    priority order, so this picks which one survives rather than leaving it to whichever
    happens to fit before an ellipsis — the same truncation-order reasoning
    `statusline.py` names inline wherever a row has to choose what to drop.

    **Task 4 (#385) adds `statusline._session_news(sid)`** — this session's own
    denied/recorded/dispatched counters, deliberately silent unless something actually
    happened (see `_session_news`'s own docstring). Unlike the context/cache gauge (see
    `_top`'s docstring for why THAT stays out), `_session_news` needs no per-turn Claude
    Code payload at all — `inflight.live()` reads a shared on-disk tracker and
    `trace.read(sid)` reads the session's own trace log, both real independent of any
    stdin JSON. It only needs a session id, which — like `_right`'s persona chips — a
    panel has none passed to it; `session.current()` reads the same
    `$CHARTER_SESSION_ID`/`$CLAUDE_CODE_SESSION_ID` a launched panel inherits whole from
    the harness (`_right`'s docstring makes the identical point for `_persona_chips`).

    **Six candidate fields on that row, each shown WHOLE or dropped WHOLE — never one
    assembled string cut once from the right.** The attention row is one row and its
    WIDTH is the frame's own, not a narrow side panel's, so ordinarily every field fits
    comfortably and this never has to choose. But `layout.py`'s own module docstring
    records a REAL tmux 3.7c resize that transiently starves a pane before the corrective
    hook snaps it back — not hypothetical, reachable by an ordinary window resize at any
    moment. A naive single `tui.truncate` over the joined line would risk Task 3's own
    Critical on perfectly ordinary data: an alert exists specifically to carry the command
    that fixes it, and slicing into that command mid-word reads as "no problem here" — the
    exact false-clean failure this plan's Global Constraints call out by name.

    **This row is also where a switch says what it did** (#729), and that is the highest
    priority on it while it lasts. It is the surface the outcome of an F2 choice moved
    ONTO, off `display-message`, because a tmux client does not repaint its panes while a
    message is up — measured at four seconds of a frozen screen on tmux 3.7c and at the
    3.2 floor alike, spent hiding the very repaint the message announced. This row is the
    right destination rather than merely an available one: `instance.FRAME_DENSITY` puts
    `bottom` in every level charter ships (`minimal`, `normal`, `full`), so it is the one
    surface besides `top` that is always there to be written on — and `docs/frame.md`'s
    promise that this row is never dropped is exactly the promise an outcome line needs.
    It is also per-FRAME, read by that frame's own panel off that frame's own state, which
    is the second half of what `display-message` could not do: `-t <pane>` selects the
    format target and not the client, so an outcome about one frame was being drawn on
    whichever client attached most recently (measured on both versions — see
    `state.say`).

    Priority order, highest first: the switch outcome (`state.notice` — a few seconds
    long, the direct answer to the last thing the operator CHOSE, and the only field here
    that is about an instant rather than a state); the one alert (`_alerts()`'s own top
    pick — an actionable control-plane problem, carrying its own fix); the in-flight spinner
    (:func:`_inflight_field` — work happening RIGHT NOW, and the only thing on this row
    that will be different in a second); the selected repo's detail
    (:func:`_selected_detail` — the direct answer to the last thing the operator DID, which
    is why it outranks the two ambient fields below it and not the two urgent ones above);
    `_session_news` (this session's own activity — silent unless it already has something
    to say, so its mere presence is the signal); the todo count (persistent state, not
    urgent); the configured hotkey hint (the one thing always rediscoverable another way,
    so it is first to give up its columns). Once decided, the survivors are RE-JOINED in
    the original reading order (todo, alert, inflight, news, hotkey, repo) — priority
    governs only who is dropped when the pane is starved, not how a healthy pane reads.

    **The selected repo's detail is the row's "right side" and is drawn LAST**, which is
    what the ask for a tooltip resolves to on a row composed left to right: last is as far
    right as a field gets without a second layout rule for one field. It is also empty on
    every plane that has never selected anything, and `_fit_fields` drops an empty field
    whole — so a frame nobody has clicked draws the row it always drew.

    **At `terse` it is subject to the same one-field limit as everything else**, so a
    selection made on a minimal frame shows only when nothing above it has anything to say.
    That is the density doing what it is for rather than an exception carved for the newest
    field; `charter frame-density full` is a keypress away and the highlight in the table
    is still there either way.

    **`_session_news` is asked to leave its own in-flight count out** (`inflight=False`).
    Both would otherwise draw the same fact from the same tracker on the same row —
    `⚡ 2 · ⠙ 2 running` — and the duplicate would be the one thing on the row a reader
    could not explain. The status line keeps `⚡ 2` unchanged: it repaints once per turn,
    where a spinner is a still picture of an arbitrary frame.

    **At `terse` exactly one field of NEWS survives** — the highest-priority one that has
    anything to say. On a quiet plane that is the todo count, so the row is never blank;
    the moment something is wrong or something is running, the row is that instead.
    `_fit_fields` does it through the same priority order it already uses for width, so
    "less" and "too narrow" cannot disagree about what matters.

    **The hotkey hint is not news and the density does not trim it** (#743, :data:`_ALWAYS`).
    It used to be, and the result was that `minimal` — a frame of two one-row strips, with
    the repo table, the sidebar, the todos, the workspace switch, the persona switch and
    the way back to `full` all behind `F2` — was the single arrangement that stopped
    saying `F2` anywhere on screen. An operator who chose it and reopened their terminal
    had a frame advertising nothing. So `terse` keeps one field of news **and** the hint:
    `7 todos · F2 palette` rather than `7 todos`. A narrow pane still drops it, because
    that is width and not density.

    The hotkey is READ, not spelled out: `[frame] hotkey` is configurable, and this row
    used to hardcode `F2 palette` — so a plane on `hotkey = "F1"` had its own panel telling
    every operator the wrong key, on every repaint, forever. `config.FRAME` is the
    resolved value `commands_frame.conf_text` binds, so there is one source for what the
    panel says and what the frame actually does.
    """
    from .. import config, session as _session, statusline as sl
    from . import state, tmuxctl
    # The FRAME's workspace (#512), for the todo count as much as for the alerts: they
    # are statements about one workspace on one row, and a panel that resolved its own
    # would count another workspace's todos beside this one's alerts.
    ws = _frame_workspace(fid)
    todos = sl._todo_count(ws)
    alerts = sl._alerts(ws)
    news = sl._session_news(_session.current(), inflight=False)
    w = content_width("bottom")

    # Unconditional, unlike `news` below — this predates Task 4 and stays exactly as it
    # was (`0 todo` included) rather than adopting `_session_news`'s "silent unless
    # something happened" discipline on the side; changing an existing field's own
    # presence rule is not this task's job.
    todo_text = f"{todos} todo" + ("s" if todos != 1 else "")
    alert_text = alerts[0] if alerts else ""
    news_text = f"{sl._DIM} · {sl._R}".join(news) if news else ""
    # Nothing to advertise inside a tmux charter did not start: it binds no key there
    # at all (`commands_frame._launch_in_operator_tmux` says why — a key table is
    # server-wide in tmux, and what the palette would offer there is what the
    # operator's own prefix key already does). A row still printing `F2 palette`
    # there would be telling every
    # operator about a key that does nothing, on every repaint — the same defect the
    # hardcoded `F2` was, reached through the other server instead of the wrong config.
    hotkey_text = ("" if tmuxctl.is_operator_socket(state.frame_server(fid))
                   else f"{config.FRAME['hotkey']} palette")

    inflight_text = _inflight_field()
    repo_text = _selected_detail(fid)
    # The outcome of the last thing the operator CHOSE, for the few seconds it is worth
    # saying (#729). Empty the rest of the time, and `_fit_fields` drops an empty field
    # whole — so a frame nobody has just switched draws the row it always drew.
    notice_text = state.notice(fid)

    # Decide who survives, highest priority first (see this function's own docstring
    # above for why); `_fit_fields` does the actual budgeting so it can be tested in
    # isolation.
    keep = _fit_fields(
        [("notice", notice_text), ("alert", alert_text), ("inflight", inflight_text),
         ("repo", repo_text), ("news", news_text), ("todo", todo_text),
         ("hotkey", hotkey_text)], w,
        limit=1 if verbosity(fid) == "terse" else None, exempt=_ALWAYS)

    # Re-assembled in the original reading order, not priority order — priority decided
    # only who was cut. `repo` is LAST, which is the "right side" the selected row's
    # detail was asked for: this row is composed left to right and joined with ` · `, so
    # last is as far right as a field gets without a second layout rule for one field.
    fields = {"notice": notice_text, "alert": alert_text, "inflight": inflight_text,
              "news": news_text, "todo": todo_text, "hotkey": hotkey_text,
              "repo": repo_text}
    order = ("notice", "todo", "alert", "inflight", "news", "hotkey", "repo")
    parts = [fields[n] for n in order if n in keep]
    # **The one door on this row is the `F2 palette` hint, and it is the sharpest case in
    # #751**: it is the only affordance charter advertises on screen, drawn as a key name
    # beside a noun, which is a button everywhere else an operator has seen one. Clicking
    # it now does what pressing the key does.
    #
    # **Its column is walked out of `parts` rather than searched for in the joined row**,
    # for `_top`'s reason: the text is `config.FRAME['hotkey']` and an operator on
    # `hotkey = "F1"` would have a row this function could not find its own field in by
    # looking — and the alert on an ordinary plane puts several ` · ` on the row, so
    # "the field after the second separator" is not a thing to search for either.
    #
    # **Every rung publishes**, including the ones that draw no door: `_fit_fields` drops
    # `hotkey` first when the row is starved (it is last in the priority list), `terse`
    # keeps exactly one field and it is never this one, and a frame inside the operator's
    # own tmux has no hotkey to advertise at all (`hotkey_text` is empty there, so
    # `_door_columns` contributes nothing). A strip that kept a stale map through any of
    # those would open the palette from a cell the operator can see is empty.
    #
    # **#729's `notice` is a seventh field and the walk needs no case for it**, which is
    # the property this shape was chosen for: the loop reads `order`, so a field added at
    # the head of the row moves the hint's columns by exactly its own width plus a
    # separator, and the door follows without anything here being told about it. It is
    # also correctly INERT — a notice is the outcome of something the operator already
    # chose, dwelling for a few seconds, which is a readout in the same sense the todo
    # count and the selected repo's detail are.
    sep_w = tui.width(" · ")
    at, doors = -sep_w, []
    for name in order:
        if name not in keep:
            continue
        # Paid at the TOP of the iteration and started one separator behind, which is
        # `_tab_columns`' own trick for the identical off-by-one: the first field is not
        # preceded by a separator, and a `if at:` in here would be a branch that could
        # only ever be observed by a first field of zero width — which `_fit_fields`
        # already refuses to keep.
        at += sep_w
        if name == "hotkey":
            doors.append((at, fields[name]))
        at += tui.width(fields[name])
    DOORS.publish(_door_columns(w, *doors))
    return tui.truncate(" · ".join(parts), w)


def _repos(fid: str) -> str:
    """This workspace's repo table, in a bordered pane of its own — #515.

    **The table used to be the bottom half of `_bottom` and that is the defect.** #488
    put it there because `bottom` was the frame's full-width slot and the 22-column
    `left` sidebar it replaced could not draw a table at all. What shipped was one pane
    holding two unrelated things stacked with no rule between them: the attention row,
    then repo rows running straight on out of it. This is that table given its own pane,
    which is the only way tmux can draw a rule between the two — and the rule is tmux's,
    drawn from the five window options `commands_frame._CHROME` pins (#514), never a box
    a renderer paints for itself.

    **This is #488's actual content, unchanged**: the frame used to show LESS of the
    plane's repo state than the status line it suppresses (#386), because the only slot
    drawing repos was a 22-column sidebar whose own docstring conceded that `_NAME_W`
    (32) and `_BRANCH_W` (34) alone exceed the whole pane. The table is drawn here at the
    widths it was designed for.

    **It is headed, `▪ repos 6`, from `_sidebar_head` — the same helper `_right` heads
    its own sections with (#516).** A bordered box of tree rows next to a bordered box
    whose sections are all labelled reads as an overflow of its neighbour rather than as
    a component, which is the exact impression #515 exists to remove; and the table's
    first row is `statusline._TREE_MID` (`├─`), a glyph that means "there is more above
    me" and had nothing above it once the attention row moved out. One row, and the count
    is the plane's repo count — a fact the frame did not previously show anywhere.

    **The pane is measured, not assumed.** :func:`_height` reads this pane's own tty the
    way :func:`_width` reads its width; :func:`repos_rows_wanted` is what told the
    LAUNCHER how tall to make it, and both go through :func:`_table_cap` at the same
    density and at THIS PANE's width — measured here, predicted there by
    `layout.repos_cols`, which is the only reason the launcher's number can match a pane
    the sidebar has narrowed. So on an untouched frame the two agree exactly and no row
    is either blank or cut, at every width the frame is drawn at, at every level the
    palette offers, and in whatever order the operator's `[frame] slots` puts the
    edges in. The :data:`_HEAD_ROWS` taken off is the heading and nothing else — the
    attention row is another pane's now, and a budget still reserving a row for THAT
    would lose the lowest-ranked repo row to nothing.

    A pane that is shorter anyway — a transient mid-resize size, or a window with no rows
    to spare — costs the table its lowest-priority rows through `_pick_rows`' ranking,
    and the `…(+N more)` line is reserved out of the budget rather than trimmed off the
    end (see :func:`_table_lines`), so a starved pane never claims one clean repo is the
    whole plane.

    **Four states, four different sentences, because they are four different claims.** A
    workspace that is not on disk at all is :func:`_gone_lines` and is asked FIRST (#752);
    a gather that has not run yet is `_unknown_lines`; a gather that ran and found nothing
    is :func:`_empty_lines`; anything else is the table. #512 is the cost of drawing two of
    them the same, and #515 is the cost of drawing "nothing there" as an empty rectangle —
    which is what an unmodified `_table_lines` returning `[]` would now be, since this pane
    no longer has an attention row above it to make its emptiness read as "nothing to add".

    **One `gather.cached`, and no repo directory touched.** #387 pinned a panel's idle
    tick at exactly one `stat` — `panel._tick`'s single `state.version(fid)`, which every
    slot pays every `panel.TICK` whether it is animated or not, so this pane is a fourth
    panel process and a fourth such read: the frame's idle cost is up a third on #515, not
    unchanged. What this slot does not pay is the SECOND one: it is not in
    :data:`ANIMATED`, so `panel._watch`'s `animates and bool(_running(...))`
    short-circuits before `_running` is ever called, where `bottom` reads the in-flight
    record set as well. And neither number is the one that would have mattered — a table
    that walked a directory per row would cost fourteen walks per repaint. Everything the
    table draws comes out of the cache; see :func:`_table_row` for the one column that
    costs (presence) and is therefore absent.

    **This is also the pane that can be SCROLLED and CLICKED, and both halves are settled
    here rather than in the handler** (#607's first consumer). `frame/builtins.py` declares
    `scroll` and `click` and moves :data:`VIEWPORT`; a handler is handed no ctx by contract
    (§4f), so it knows neither how many repos there are nor how tall this pane is. Both
    numbers are read on this line and nowhere else:

    * :func:`_scroll_limit` is how far the window may move, asked about
      :func:`_content_rows` — the table's ROWS, pieces included, which is #663 — and handed
      to :meth:`_Viewport.settle`, which clamps an offset left over from a longer table and
      leaves the handler a bound it can refuse a pointless scroll against. On the ordinary
      plane — a pane sized to its own content — it is 0 and the wheel does nothing at all.
    * :meth:`_Viewport.publish` records which repo each PANE ROW is about, so a click that
      arrives as a row number resolves against the paint the operator was looking at. The
      three one-line answers below publish nothing, which is what makes a click on
      `gathering this workspace's repos…` not a selection.

    **All five ways out record BOTH, and the four that draw no table say so in one call**
    (:meth:`_Viewport.blank`). They used to clear the map and leave the bound where the
    last table put it, so a pane whose terminal had narrowed, whose cache had gone or whose
    workspace had lost its last clone still answered every wheel notch truthy — repainting
    one static sentence, once per notch, off a bound for a table that was no longer on
    screen. That is the same stale-bound reading the `settle` below is unconditional for;
    the guard was on the one path that already reached it, and these three went round it.

    `state.selection` is read here and handed to :func:`_table_lines`, which is what draws
    the chosen row in reverse video. One extra file read per repaint of a pane that is not
    animated — it repaints on a version bump or a resize, and a click is a version bump
    (`frame/builtins.py` bumps so the ATTENTION pane, a different process, redraws too).
    """
    from . import state
    w = content_width("repos")
    # `_table_cap` is the SAME call `repos_rows_wanted` made to size this pane, with the
    # pane's own measured width instead of the window's.
    cap = _table_cap(fid, w)
    if cap <= 0:
        # A pane narrower than `statusline._LEFT_W`. The LAUNCHER does not split one
        # (`layout.visible_slots`), but `cmd_resize` only re-sizes panes — it neither
        # creates nor destroys them — so narrowing a running frame's terminal arrives
        # here with a real pane to fill. Said out loud rather than left blank: see
        # :func:`_too_narrow_lines`. A renderer must not depend on the launcher's filter
        # for its own correctness either — `charter panel repos` run by hand into a
        # narrow terminal reaches exactly here, and an unbounded `_table_lines` would
        # draw a false-clean `charter  main` into it.
        # The pad goes with the width, so the number the line quotes is what this PANE
        # needs rather than what the table needs — see :func:`_too_narrow_lines`. Asked of
        # :func:`pad_of` and not subtracted back out of `w`, because `pad_of` is the one
        # that knows whether the pad was afforded at all.
        VIEWPORT.blank()
        return "\n".join(_too_narrow_lines(w, pad_of("repos")))
    from . import gather
    # `gather.cached`, never `gather.read` (#512). The two differ by exactly one thing:
    # `read` falls back to a live `scan()` when there is no cache, and a PANEL is the one
    # caller that must not have that fallback. Two reasons, and both are rules this
    # module already keeps:
    #
    # * **A panel does not sweep.** `_table_lines`' own docstring promises it never
    #   reaches a repo directory, a `git status` or a `glstate.read_for`; #387 pinned an
    #   idle tick at one `stat`, and a paint that scans is a cold sweep every time the
    #   plane version moves. `cmd_launch` calls `gather.discard` before it draws, so a
    #   fresh frame reached that fallback BY DESIGN, on the very repaints an operator is
    #   watching.
    # * **An empty table is not the same claim as an unknown one.** `read`'s fallback
    #   returns `repos: []` for "the scan found nothing" and for "the scan ran in the
    #   wrong workspace" alike, and `_table_lines` draws both as no rows at all — a pane
    #   that says "no repos" on a plane full of them. `None` here keeps the two apart,
    #   and :func:`_unknown_lines` says which one this is.
    #
    # Nothing is left blank waiting: `commands_frame._spawn_gather` kicks a detached
    # refresh at launch which writes this cache and bumps the version, the same shape
    # `update.maybe_spawn`/`glstate.maybe_spawn` already use, and every `posttooluse*`
    # hook refreshes it after that (`notify.plane_changed`).
    data = gather.cached(fid)
    if data is None:
        VIEWPORT.blank()
        # **Two states, not one** (#735). `cached` collapses "nothing has been written
        # yet" and "what was written cannot be read" into the same `None`, and drawing
        # both as :func:`_unknown_lines` made a permanent failure indistinguishable from a
        # five-second wait — on a pane that has no other route out, because nothing
        # repaints its way back to a readable cache. `gather.unreadable` is the second
        # question, asked ONLY here and only once `cached` has already said `None`: it is
        # a fact about a file on disk at the moment this line is drawn, never a timeout.
        #
        # **Three, not two** (#752). Both sentences below are about a workspace that is
        # THERE — one gather has not landed, one never will — and neither is true of a
        # workspace that is not on disk at all, which is the state a frame reaches by
        # outliving its own directory (`charter workspace remove`, a `git clean`, a
        # teammate's pull, a plain `mv`). Asked FIRST of the three, because "there is
        # nothing to gather" is the reason the other two are waiting for something that is
        # not coming. `workspace.exists` is `gather.unreadable`'s rule one noun up: a fact
        # about the filesystem at the moment the pane is drawn, never a duration.
        #
        # `ws` is resolved HERE and in the `if not repos` branch below, rather than once
        # above the cache, so that the path which draws a TABLE pays for neither: the
        # `state.workspace_for` behind `_frame_workspace` is two file reads (`identity`
        # and the launch record) and this pane is repainted on every version bump. That is
        # exactly where it was resolved before #752 — the new question is asked beside the
        # old ones and did not move them onto the hot path.
        from .. import workspace as ws_mod
        ws = _frame_workspace(fid)
        if not ws_mod.exists(ws):
            return "\n".join(_gone_lines(ws, w))
        if gather.unreadable(fid):
            return "\n".join(_unreadable_lines(fid, ws, w))
        return "\n".join(_unknown_lines(w))
    repos = data.get("repos") or []
    if not repos:
        # Gathered, and there is nothing in it. Said out loud rather than left as an
        # empty bordered rectangle — see :func:`_empty_lines`. No heading: `▪ repos 0`
        # above "no clones in demo" is the same fact twice in a two-row pane.
        VIEWPORT.blank()
        # The other half of the same question, and it is asked twice rather than once
        # above the cache ON PURPOSE. Above it, this would override a table the cache
        # still has — and a panel "never gathers on its own: it reads the cache or says it
        # has none" (docs/frame.md), so a renderer that contradicted its own cache from a
        # `stat` would be re-deriving state the gather owns, on every repaint of every
        # frame. Here it costs nothing on the path that draws a table and separates the
        # two claims that were one sentence: `no clones in <ws>` is a workspace with
        # nothing cloned into it, which is not what an absent one is.
        from .. import workspace as ws_mod
        ws = _frame_workspace(fid)
        if not ws_mod.exists(ws):
            return "\n".join(_gone_lines(ws, w))
        return "\n".join(_empty_lines(ws, w))
    # The heading takes the first row and the table spends what is left. Asked for fewer
    # ROWS rather than sliced afterwards, so what survives at `terse` (or in a pane a
    # resize starved) is still `_pick_rows`' ranked subset — the repo you are standing
    # in, the ones with something on them — rather than whichever happened to come
    # first, the same discipline the `terse` chip list in `_right` keeps.
    budget = min(_height() - _HEAD_ROWS, cap)
    # **`settle` is unconditional, and the `if budget > 0` that used to guard the call
    # below went with it.** `_table_lines` already answers `[]` for a non-positive budget
    # (its first line), so the conditional could not change an outcome — the sweep's own
    # definition of a line that should not be there — and it could change one HERE, in the
    # direction that matters: a pane starved to no rows by a resize would have kept
    # whatever bound the last taller paint recorded, and the wheel would go on moving an
    # offset over a table that is not on screen. `_scroll_limit` answers 0 for that pane.
    # **:func:`_content_rows` and not `len(repos)`** — the bound is over the rows this pane
    # draws, and a repo with pieces draws more than one. See that function for #663.
    offset = VIEWPORT.settle(_scroll_limit(_content_rows(data), budget))
    lines = _table_lines(data, w, budget, offset=offset, selected=state.selection(fid))
    # The heading is row 0 of the PANE and belongs to no repo, so the map the handler
    # resolves a click against starts with it — see :meth:`_Viewport.publish`.
    VIEWPORT.publish((None, *(ln.repo for ln in lines)))
    return "\n".join([_sidebar_head("repos", len(repos), w),
                      *(ln.text for ln in lines)])


#: How a member's derived state is spelled on a row. `change.MEMBER_STATES`' own words,
#: upper-cased, held in one table so the pane and `charter change show` cannot come to
#: disagree — and so that a state charter grows later is a `KeyError` here rather than a
#: blank column.
#:
#: **`UNKNOWN` is a word and not a blank**, which is the whole of #561 on a surface. An
#: empty cell reads as "fine, nothing to report"; `UNKNOWN` reads as *charter did not
#: look*, which is what it means — a member's request state and its checks at its head sha
#: are a forge read this pane does not make.
_CHANGE_STATE_WORD = {"unknown": "UNKNOWN", "blocked": "BLOCKED", "landed": "landed"}

#: How long the age of a reading is shown for before it is just "a while". Beyond an hour
#: the number stops being the thing you act on — what you act on is that it is stale — so
#: the row says so in a word instead of counting up for ever.
_AGE_STALE = 3600


def _age(then, now: float, *, short: bool = False) -> str:
    """``just now`` / ``4m ago`` / ``stale`` for a snapshot taken at *then*.

    *short* is the same three answers in the cells a 22-column sidebar has to spare —
    ``now`` / ``4m`` / ``old``. One function with a flag rather than two formatters,
    because the BOUNDS are the thing that must not drift: a surface that called anything
    under a minute "now" while another called it "just now" at ninety seconds would be two
    clocks wearing one name.

    **The pane draws the age of what it is showing, and that is the cost half of §4g.** A
    refresh is an action, not a tick; between two of them the rows on screen are as old as
    the last one, and a surface that did not say so would be indistinguishable from one
    that was live. `gathered_at` is `time.time()` at scan, so this is a subtraction and
    reads no clock the snapshot did not already carry.

    A missing or unreadable timestamp answers ``?`` rather than ``just now``: a cache
    written by an older charter has none, and dating it to the present is exactly the
    confident wrong answer ADR 0009 forbids.
    """
    try:
        age = now - float(then)
    except (TypeError, ValueError):
        return "?"
    if age < 0 or age >= _AGE_STALE:
        return "old" if short else "stale"
    if age < 60:
        return "now" if short else "just now"
    return f"{int(age // 60)}m" + ("" if short else " ago")


def _change_rows(rows: list, width: int, budget: int, age: str,
                 chosen: str | None = None) -> list[str]:
    """The `changes` section of the sidebar: a heading, then one row per change.

    **Nothing at all when this workspace has no changes**, and that is `_todo_rows`' own
    rule and its argument: a heading over an empty space in a 22-column column is
    furniture within a day, and then a real change appearing in it draws no more attention
    than the emptiness did. It is also what makes this section free on the planes that
    never use it — which is the whole reason the change surface is a section here rather
    than a pane of its own (`frame/builtins.py` records the measurement).

    **The aggregate goes on the HEADING and the counts on the rows.** Twenty-two columns
    cannot carry a slug, a fraction and a state word on one line, and the aggregate is the
    thing you glance for — `change.worst` over every change's own worst member, so it can
    never be greener than the worst member of the worst change.

    **The change the picker chose carries the mark and does NOT move to the top.** A list
    whose rows reorder with state is a list nobody learns (`builtin_actions._register_density`
    settles the same question for its own rows), and the mark costs two cells that are the
    same two cells whichever row has it.

    The `…(+N more)` line is RESERVED out of the budget rather than appended and trimmed,
    which is `_table_lines`' rule: "there is more here than fits" outranks "here is an
    arbitrary one of them".
    """
    from .. import change as change_mod, statusline as sl
    from . import choose

    if not rows or budget <= 0:
        return []
    state = _CHANGE_STATE_WORD.get(
        change_mod.worst([r.get("state") for r in rows]), "UNKNOWN")
    head = tui.truncate(
        f"{sl._DIM}{sl._HEAD_PAD}{sl._R}{sl._BOLD}changes{sl._R}"
        f"{sl._DIM} {len(rows)} {state} {age}{sl._R}", width)
    room = budget - 1
    if room <= 0:
        return [head]
    shown = rows if len(rows) <= room else rows[: max(room - 1, 0)]
    out = [head]
    # The count column is fixed, so the NAME is what gives way — a fraction cut in half
    # says nothing, while a clipped slug is still the slug you recognise.
    counts = [f"{int(r.get('landed') or 0)}/{int(r.get('total') or 0)}" for r in shown]
    cw = tui.column("", counts, gap=0)
    mw = tui.width(choose.MARK[0])
    for r, count in zip(shown, counts):
        # The mark is `frame/choose.py`'s, not a second one: it is what the picker's own
        # rows carry for "the one you are on", and two spellings of that would be two
        # answers on one screen. Both entries are the same width by construction, so the
        # mark moving does not move the names beside it.
        mark = choose.MARK[0] if r.get("change") == chosen else choose.MARK[1]
        name = contain.one_line(r.get("change") or "")
        room_for_name = max(width - cw - mw - 1, 1)
        out.append(tui.truncate(
            f"{sl._DIM}{mark}{sl._R}"
            f"{tui.pad(tui.truncate(name, room_for_name), room_for_name)} "
            f"{sl._DIM}{count}{sl._R}", width))
    if len(shown) < len(rows):
        out.append(tui.truncate(
            f"{sl._DIM}…(+{len(rows) - len(shown)} more){sl._R}", width))
    return out


def changes_section(fid: str, width: int, budget: int) -> list[str]:
    """This workspace's cross-repo changes, in at most *budget* rows — the `changes` part
    of the sidebar composite.

    `todo_section`'s shape and its rules. **The cache is opened only when there is a row
    to draw it into**: the budget is checked before `gather.read` is reached, because a
    pane with no room must not open a file it is about to discard.

    **The workspace is the FRAME's, handed in, never one the panel resolved for itself**
    (#512/#526). `gather.read` falls through to `gather.scan` on a cold cache, and `scan`
    with no workspace argument resolves in the PANEL process — which lands on the declared
    default, and a populated list of another plane's changes reads as an answer.

    **No subprocess, on any path.** Everything drawn here is in the one snapshot
    `gather.scan` wrote — the records and the landing declarations, both file reads. A
    repaint that asked the forge would put five reads on every plane-state bump, which is
    §4g's idle-tick property spent on a pane nobody was looking at.
    """
    from . import gather
    budget = min(budget, _MAX_CHANGE_LINES)
    if budget <= 0:
        return []
    from . import state as st
    data = gather.read(fid, workspace=_frame_workspace(fid))
    return _change_rows(data.get("changes") or [], width, budget,
                        _age(data.get("gathered_at"), time.time(), short=True),
                        st.frame_change(fid))


#: Every slot charter can draw. `panel.run` refuses a name that is not in here rather
#: than painting an empty pane, because an empty pane reads as a broken frame.
SLOTS = {"top": _top, "bottom": _bottom, "repos": _repos, "right": _right}


#: What marks the chat (or workspace) a bar's row says you are IN, and what marks the
#: others. Two entries of the same width by construction, so the mark moving does not move
#: the names beside it.
#:
#: **ASCII, deliberately** — `overlay._MARK`'s own rule and `_persona_chips`' measurement
#: behind it: `●`, `◆` and the pointing triangles are East-Asian *Ambiguous* and have
#: broken this layout twice. `choose.MARK` is the same decision one surface over and is
#: deliberately NOT reused: that one is `("* ", "  ")`, two cells, because a picker draws
#: one name per row and the mark owns a column of its own; a bar draws several names on
#: one row, so a two-cell blank in front of every inactive name would spend exactly the
#: columns the names are competing for.
_BAR_MARK = ("*", " ")

#: The frames a chat tab cycles through while that chat's harness is working (#853).
#:
#: **It takes the mark's cell rather than adding one, and that is the whole of why a
#: spinner is affordable on a strip whose click map is per COLUMN.** Every tab already
#: carries a one-cell prefix (:data:`_BAR_MARK`) and every frame here is one cell, so a
#: chat starting a turn changes not one column of the strip's arithmetic: the cut
#: (:func:`_cuts`), the row count (:func:`bar_rows_wanted`) and the map
#: (:func:`_tab_columns`) all answer exactly what they answered when nothing was working.
#: A spinner drawn *beside* a name instead would re-cut the strip the moment a sibling
#: started thinking, and the cell an operator was about to press would hold another chat's
#: name — the double-press #767 exists to prevent, arriving through a spinner.
#:
#: **The ACTIVE chat keeps its `*` and never shows this**, which is a limit rather than an
#: oversight. There is one cell and the two facts compete for it; `*` wins because it is
#: the only one of the two that has no other way to be seen. `chrome.block` paints the
#: active tab in reverse video, but `[frame] chrome = "off"` is the shipped default and
#: `panel._write` strips SGR under `NO_COLOR`, so on a plain plane the `*` is the ONLY
#: thing saying where you are (:func:`_compose` says so at the paint). The chat you are
#: typing in, meanwhile, is the one whose harness you can watch directly. The readout is
#: for the chats you are not looking at.
#:
#: **Which glyphs, and the two that were asked for and refused.** The request was Claude
#: Code's own spinner, `· ✢ ✶ ✳ ✽ ✻`. `tui.width` answers 1 for all six, and `tui.width` is
#: not the whole question on this row: it reads the East-Asian tables, and an *Ambiguous*
#: character is one a terminal may draw two cells wide while those tables say one. That is
#: the failure :data:`_BAR_RULE` is ASCII to avoid and the one `statusline._persona_chips`
#: records breaking this project's layout twice. Measured with `unicodedata`:
#:
#: ==========  ========  =====  ==========================================
#: glyph       codepoint  EAW   verdict
#: ==========  ========  =====  ==========================================
#: ``·``       U+00B7    **A**  refused — Ambiguous
#: ``✢``       U+2722    N      kept
#: ``✳``       U+2733    N      refused — carries an emoji presentation
#:                              (``✳️``, U+2733 U+FE0F), so a terminal with
#:                              an emoji fallback font may draw it wide
#: ``✶``       U+2736    N      kept
#: ``✽``       U+273D    **A**  refused — Ambiguous
#: ``✻``       U+273B    N      kept
#: ==========  ========  =====  ==========================================
#:
#: What survives is Neutral, which is the property `statusline`'s own marker test asserts
#: (`▪▸▫`, chosen after `◈`/`◆` shipped broken) and the strongest one available short of
#: ASCII. **None of them is a character a chat id may contain** (`chats.ID_RE` is
#: ``[A-Za-z0-9._-]``), and that is a separate requirement with its own reason: the mark
#: sits flush against the name, so an ASCII pulse like ``.oOo`` would draw ``Oapi.3`` and
#: put a character that could be part of a name where the operator reads the name.
#:
#: Three frames, cycled out and back so the sparkle grows and shrinks the way the
#: requested one does. It reads off the same clock as :data:`SPINNER` — see
#: :data:`SPINNER_PERIOD`, whose argument (short enough to be seen, long enough to be
#: worth a repaint) is about `panel.TICK` and is the same argument here.
TAB_SPINNER = "✢✶✻✶"

#: Columns a bar spends between two names. Two, so a name reads as one name — a single
#: space runs `api.1 api.2` together at the widths where this matters most.
#:
#: **The floor rather than the whole answer now**: :func:`_bar_gap` is what a rung actually
#: joins with, and on a plane that draws its rules it is one cell wider. This constant
#: stays the blank form because it is also the gap between the heading and the first tab —
#: a seam between a label and a strip, which is not the seam a rule is about.
_BAR_GAP = 2

#: What a bar draws between two tabs on a plane whose seams are visible.
#:
#: **ASCII, and this one is not the same argument as :data:`_BAR_MARK`'s — it is a harder
#: one.** `│` (U+2502) is what an IDE draws here and it is East-Asian *Ambiguous*: a
#: terminal may draw it one cell or two, and `tui.width` says one. The repo table draws
#: box glyphs (`statusline._TREE_MID` and its neighbours) and can afford to, because its
#: click map is per ROW — a glyph that comes out two cells wide makes a row ragged and
#: moves no row index. **A bar's map is per COLUMN.** One separator drawn a cell wider than
#: it was measured shifts every tab right of it by one, ten separators shift the tenth tab
#: by ten, and the operator presses `fleet` and lands on `default`. That is `EVENT_KINDS`'
#: "fires wrongly", reached through a glyph rather than through a protocol.
#:
#: So the rule the bar draws is the one glyph whose width no terminal disagrees about.
#: `▏`, `▎`, `●` and the pointing triangles are the same refusal for the same reason, and
#: `statusline._persona_chips` records two of them breaking this layout already.
_BAR_RULE = "|"


def _bar_gap() -> str:
    """The cells a bar puts between two of its own fields.

    Two blanks on a plane whose seams are hidden — charter's shipped default, and byte for
    byte what every bar drew before rules reached this row. `" | "` on a plane that asked
    for visible rules, which is one cell more.

    **The same key, one scope in.** ``[frame] rules`` is the operator saying whether they
    want to see the structure of the frame or a surface with no seams in it
    (`instance.FRAME_RULES` carries the four reports that produced it). The seam between
    two tabs is that question about a row instead of about a pane, and answering it from a
    second key would let one frame draw pane borders and no tab rules, which is exactly the
    disagreement that key exists to end.

    **It costs nothing on the shipped default**, which is what makes it affordable at all:
    a plane at ``rules = "hidden"`` spends not one column on this, and a plane that asked
    for rules pays one cell per gap — fourteen of them on this project's own fifteen
    workspaces, which the ladder gives up a whole name for rather than overflowing.

    Read at call time through `config.FRAME` rather than cached, exactly as
    `chrome.dim_ok` and :func:`pad_of` read the keys they need: a panel is a long-lived
    process and `charter.toml` is re-read when the frame relaunches. The import is
    function-level for this module's own repaint-path reason.
    """
    from .. import config, instance
    if instance.look_of(config.FRAME).rules == "visible":
        return f" {_BAR_RULE} "
    return " " * _BAR_GAP


class _Tabs:
    """Which name each CELL of a bar is about, and which of them you are on.

    :class:`_Viewport` one axis over, and it exists for the reason that class states for
    the repo table: a pointer event arrives as a pair of NUMBERS (`frame/events.py`), and
    the only thing that knows what is at them is the pass that composed the strip.

    **The key is `(row, col)` and it was `col` alone until #829.** A strip is one row when
    its names fit on one and as many as its pane was given when they do not, so "which
    name is at column 40" stopped having an answer the moment there could be a second row
    of names under the first. A column-keyed map on a two-row strip does not degrade — it
    answers the row above about a click on the row below, which is `component.EVENT_KINDS`'
    *fires wrongly* rather than *never fires*, and the whole reason :data:`_BAR_RULE` is
    ASCII. There is no default row for the same reason: a caller that forgot to say which
    row it is asking about must be a `TypeError` here, not a switch to the wrong tab.

    **A bar needs it MORE than the table does, because the ladder is not invertible.**
    :func:`_bar` has four rungs and three of them draw a different set of names — every
    name, or the one you are on plus a count of the rest, or `2/3`, or nothing at all —
    so "which name is at column 40" is not a question anything downstream can work back
    to from the names and the width. It would have to re-walk the ladder, which is a
    second answer to what is on the row, and the two disagree the moment a repaint lands
    between the paint and the click.

    **One object at module scope rather than one per bar.** `panel.run` resolves a
    component name once and draws that one component for the life of the process, so a
    process drawing the chat bar never draws the workspace bar and "which bar is this"
    has exactly one answer here — :class:`_Viewport`'s own argument, and a key for it
    would be the same second, weaker one. Tests reach the same object and :meth:`forget`
    is what puts it back.

    **The map and the mark are written by ONE call**, which is what `_Viewport.blank`
    exists to enforce for the pair that class holds. A bar that published its columns and
    left a stale *here* behind would answer "you are already there" for a tab the
    operator can see is not marked, and the reverse would switch to a tab that has since
    stopped being drawn. There is no way to write down one half here, so there is nothing
    for a second method to keep in step.

    **Columns are a mapping and not a tuple, and that is where the bounds check went.**
    `_Viewport.repo_at` spells `0 <= row < len(...)` and argues it, because a tuple
    indexed with a negative number answers the LAST row — a wrong answer that hides a
    wrong reading. A mapping has no such answer to give: a column nothing drew into is
    absent whether it is `-1`, `4096` or the two cells of a :data:`_BAR_GAP`, so the
    property is structural rather than guarded. That is `builtins.places`' finding in a
    different shape — a guard no input can make observable is a line the deletion sweep
    reports and this repository deletes.
    """

    __slots__ = ("_cols", "_here", "_more", "_add")

    def __init__(self) -> None:
        self._cols: dict[tuple[int, int], str] = {}
        self._here = ""
        self._more: frozenset[tuple[int, int]] = frozenset()
        self._add: frozenset[tuple[int, int]] = frozenset()

    def forget(self) -> None:
        """Back to a bar nobody has drawn — for a test, and only a test.

        `_Viewport.forget`'s reason exactly: production never calls it, because a panel
        process is born here and the object dies with the process, and the alternative
        for a test is reaching into ``__slots__`` by name.
        """
        self.publish({}, "")

    def publish(self, columns: dict, here: str, more=(), add=()) -> None:
        """Record what the paint that just happened put in each cell, and where you are.

        *columns* maps a ``(row, column)`` of the component's OWN canvas — the rectangle
        `ctx.width` and `ctx.height` describe, which is what `events.Dispatcher._on_canvas`
        delivers a click in — to the name of the tab drawn there. Absent means the operator
        clicked a cell this bar drew no tab into: the heading, the blank under it on a
        strip that grew a second row, the gap between two tabs, EITHER `+N` count,
        the `n/N`, the empty space past the last name. `events.Dispatcher._on_canvas` applies
        the identical rule one axis over for the pad, and `_Viewport.publish` applies it
        for the table's heading row.

        *here* is the name this frame is ON, taken from the same paint that drew the
        mark. Not re-resolved when a click arrives: `switch.current_workspace` reads the
        plane, and a handler is handed no ctx by contract (§4f) precisely so it does not
        grow a second reading of a plane the repaint is about to read anyway. It is also
        the only reading that can agree with the `*` the operator was looking at.

        **RAW names, never the drawn ones.** :func:`_bar` runs `contain.one_line` over
        every name before it measures one (#472), and that is a REPAIR — what comes out
        is display text and what goes into `choose.switch_to` or `charter frame-chat`
        has to be the name on disk. The mark is matched on the raw name for the same
        reason one function down; this is that decision arriving at the other end.

        *more* is the columns of the fields that stand for names NOT on the row — both
        `+N` counts, and the `n/N` of the rung that has no names at all. **A third thing
        this call writes rather than a second method**, for the reason the class docstring
        gives about the pair it already held: a rung that published its columns and left
        a stale overflow behind would open a picker from a cell that is now a tab, or
        leave a count inert that the paint has just drawn. There is no way to write down
        one of the three here, so there is nothing for a second method to keep in step.

        *add* is the columns of the add-chat affordance (:data:`ADD_CHAT`), on the one
        rung and the one bar that draws it. A fourth thing rather than a second sentinel
        inside *more* for :meth:`more_at`'s own reason: they are different questions with
        different answers — one says *show me the rest of what is here*, the other says
        *make a new one* — and a caller told "this cell is special" would have to ask a
        second question anyway to know which.

        A `frozenset` and not a range, because the two counts are two disjoint runs and
        the narrow rung's is a third — and because :meth:`more_at` asks about ONE cell,
        which is the same question `_cols` answers and should be the same kind of lookup.
        """
        self._cols = dict(columns)
        self._here = here
        self._more = frozenset(more)
        self._add = frozenset(add)

    def switch_to(self, row: int, col: int):
        """The tab a click at canvas cell (*row*, *col*) should switch to, or ``None``.

        **The rule lives here rather than in the handler**, which is `_Viewport.move`'s
        choice for its own answer: "a click on the tab you are already on does nothing"
        becomes a property of one object with one test instead of an agreement between a
        renderer and a handler that could stop holding. Re-selecting what is already
        selected is not news — the same sentence `frame/builtins._repos_events` keeps for
        the table — and here it is worth more, because re-switching is not free: a chat
        switch is 41 tmux invocations and ~360 ms of panes being torn down and split
        again, all of it to arrive where you already were.

        **One comparison, and there is deliberately no `name is None` beside it.** A
        cell nothing drew into is absent from the mapping, so ``get`` answers ``None``,
        and ``None`` is never a name any caller publishes — so the expression below
        already answers ``None`` for it. A guard in front of that is a line no input can
        make observable, which is exactly what the deletion sweep reports as a survivor.
        """
        name = self._cols.get((row, col))
        return None if name == self._here else name

    def tab_at(self, row: int, col: int):
        """The tab drawn at cell (*row*, *col*) — **including the one you are on**.

        **The same map :meth:`switch_to` reads, without its one subtraction**, and the
        difference between the two is the difference between the two gestures rather than
        a relaxation of a rule. A left click SWITCHES, so "the tab you are already on"
        answers nothing: re-switching is 41 tmux invocations and ~360 ms of panes being
        torn down and split again, all of it to arrive where you already were. A right
        click opens a menu ABOUT a tab (`frame/tabmenu.py`), and the commonest chat an
        operator closes is the one they are in — `F2 → chat: close` has always meant
        exactly that — so a menu that refused the marked tab would refuse the ordinary
        click.

        **It is a second method and not a flag on the first**, for :meth:`add_at`'s
        reason: they are different questions with different answers, and a caller told
        "this cell is special" would have to ask a second question anyway to know which.
        Every caller of `switch_to` feeds what it answers to a command that switches; a
        `switch_to(..., here=True)` would put the tab you are on into that argv on the day
        somebody passed the flag by mistake.

        **A cell nothing drew into is still nothing**, structurally rather than by a
        guard: `_cols` is a mapping, so a column the strip drew no tab into is absent
        whether it is `-1`, `4096`, a `_BAR_GAP`, either `+N` count or the `+` that makes
        a chat — see the class docstring, and :meth:`more_at` and :meth:`add_at` for the
        two fields that are drawn there and are deliberately not tabs. No cell answers two
        of the three, which `_bar` makes true by building all four sets from one walk of
        one composition.
        """
        return self._cols.get((row, col))

    def more_at(self, row: int, col: int) -> bool:
        """Whether cell (*row*, *col*) is a field standing for names the strip could not
        draw.

        **The other half of "a click on a cell nothing was drawn into does nothing".** A
        `+9` is not nothing: the operator can see it, it says there are nine more, and it
        is drawn precisely where the row ran out of names to show. It was inert — pressed,
        and reported, by the operator this change is for — and answering nothing there was
        the one place the rule was serving the code rather than the reader.

        What it opens is the palette rather than a page of its own, and that is §3.6
        arriving where it was always heading: *the bar is a readout, never the mechanism*,
        and the palette reaches every chat and every workspace at every width including
        the widths where the bar can draw no name at all. So a count hands off to the
        mechanism at exactly the point where the readout ran out of room.
        `frame/builtins._bar_events` is what performs the hand-off; this is only which
        cells it is about.

        **It is not `switch_to`'s answer wearing a sentinel**, and the separation is the
        point: that method answers "which name", and every caller of it feeds the name to
        a command that switches. A count has no name — that is what makes it a count — so
        a sentinel would be a value every one of those callers would have to learn to not
        switch to. Two questions, two methods, and a cell that is neither answers no to
        both.
        """
        return (row, col) in self._more

    def add_at(self, row: int, col: int) -> bool:
        """Whether cell (*row*, *col*) is the affordance that makes a NEW chat.

        *"`+` button not working for creating new session."* :data:`ADD_CHAT` was a
        SENTENCE — `+ charter <harness> opens another` — which is true, which names the
        command that does it, and which begins with a `+` on a row of clickable tabs. It
        was read as a button, which is the only way it could have been read.

        **The two things this is not.** It is not :meth:`switch_to`: there is no chat to
        switch to yet, which is the whole point of pressing it. It is not :meth:`more_at`
        either — that stands for chats that EXIST and are off the row, and opening a picker
        over them is the opposite of making a new one. Three questions, three methods, and
        a cell that is none of them answers no to all three.

        **They cannot be drawn on one strip, which is structural rather than lucky.**
        :func:`_bar` draws the affordance only where the whole list is on the strip — the
        rung that fits every name on one row, or a grown strip whose rows hold them all —
        and draws a `+N` only where it is not. So a strip carrying `+` never carries a
        `+9` on any of its rows, and the two fields that both begin with a `+` are never on
        screen together to be confused with each other.
        """
        return (row, col) in self._add


#: The one tab strip this process's bar draws into. See :class:`_Tabs` for why there is
#: exactly one; `frame/builtins._bar_events` is the other half, and it holds no state of
#: its own so that the handler and the renderer cannot come to disagree about which tab is
#: where.
TABS = _Tabs()


def _tab_columns(row: int, start: int, drawn, gap: int) -> dict:
    """Which canvas cell each drawn tab owns — the map :meth:`_Tabs.publish` takes.

    *drawn* is the ``(name, field)`` pairs the rung actually put on *row*, in the order
    it put them: the raw name a click switches to, and the text that was drawn for it.
    *row* is which row of the strip they were drawn on — 0 on a strip of one row, and the
    reason the map is keyed by a pair (:class:`_Tabs`). *start* is the column the first
    field begins in, and *gap* is how many cells the rung
    put between two of them (:func:`_bar_gap`). So this walks the composition once more
    rather than guessing at it, and the two things that could have been guessed wrong
    are settled here in one place: **the mark belongs to the tab it marks** (it is drawn
    against that name and there is nothing else it could be a click on), and **the gap
    between two tabs belongs to neither** — those cells are separator, the operator can
    see they are not a name, and picking the nearer one for them would be the clamp
    `events.Dispatcher._on_canvas` refuses one rectangle out. That holds for a gap holding
    a :data:`_BAR_RULE` exactly as it did for a gap of two blanks: a rule is a seam
    somebody drew *between* two tabs, and a click on the seam has no tab to be about.

    *gap* is a parameter rather than a read of :func:`_bar_gap` here, and it is the same
    discipline the rest of this pass keeps: :func:`_bar` resolves the plane's answer ONCE
    and hands the same number to the cut, the composition and this map. Three readings of
    a key `charter.toml` is re-read behind is three chances for the map to describe a row
    the operator is not looking at.

    ``tui.width`` and never ``len``, for the reason every other measurement in this module
    gives: a field is display text that has already been through `contain.one_line`, and
    the column a name ENDS in is a question about cells. It is also what makes the
    highlight free: :func:`_bar` paints the marked field in reverse video, `tui.width`
    counts no SGR, and the map comes out the same either way.

    The walk starts one gap behind *start* and pays the gap at the top of every iteration,
    so there is no "first field is different" branch to get wrong or to test — the same
    trade `builtin_actions._SELECT_STEPS` makes when it writes its two starting points
    down as data rather than deriving them from a sign.
    """
    cols: dict[tuple[int, int], str] = {}
    at = start - gap
    for name, field in drawn:
        at += gap
        width = tui.width(field)
        for col in range(at, at + width):
            cols[(row, col)] = name
        at += width
    return cols


def _span(row: int, start: int, text: str) -> list[tuple[int, int]]:
    """The cells *text* occupies when it is drawn on *row* starting at column *start*.

    Two lines, and it exists so the fields that are NOT tabs are measured by the same rule
    the tabs are (:func:`_tab_columns`): `tui.width` and never `len`, off the string the
    rung actually composed and never off a search of the finished row.

    **An absent field is an empty span rather than a special case.** ``_span(r, n, "")``
    contributes no cell, so :func:`_bar` can hand both counts to the same expression
    whether or not the page it cut carries either — which is the branch that would
    otherwise have to be written twice and got right twice.
    """
    return [(row, col) for col in range(start, start + tui.width(text))]


def _cuts(fields: list[str], room: int, gap: int) -> list[int]:
    """Where each row's worth of *fields* starts and stops, inside *room* — the page
    boundaries the windowed rung is cut along.

    :func:`_bar`'s windowed rung. *fields* are the already-marked, already-contained tab
    texts; the answer is the cut points, ``[0, …, len(fields)]``, so page *i* is
    ``fields[out[i]:out[i + 1]]`` and each page is wide enough to draw with room left for
    the two counts that stand for what it leaves out.

    **It answers the whole cut rather than one page, and that is #829.** A strip is one
    row when its names fit on one and as many rows as its pane was given when they do not,
    so `_bar` needs the page its mark falls in AND the pages next to it. Handing back one
    page and calling this again for the next would be the same walk done twice with two
    chances to disagree; the cut is one list and `_bar` takes a run out of it.

    *gap* is how many cells go between two fields — :func:`_bar_gap`, resolved once by
    :func:`_bar` and handed to the cut, the composition and the map together, so that a
    plane which draws its rules cannot cut a page for one gap and draw it with another.

    **The pages depend on nothing but the names, the width and the gap, and that is the
    whole design.** The list is cut into consecutive pages left to right — greedily, as
    many whole names as fit, then the next page from where that one stopped. Nothing about
    WHERE THE MARK IS enters the cut, so switching to a tab that is on a drawn page redraws
    that page unchanged with only the `*` moved. That is what :func:`_bar` then leans on
    twice: to pick the page the mark falls in on a one-row strip, and to pick the RUN of
    pages it falls in on a strip that grew. The gap belongs in that list and changes nothing about the property: it is
    fixed for the life of a frame, so it cannot move a page out from under a pointer — the
    same standard the WIDTH is held to, which moves only on a resize that redraws the row
    anyway.

    That property is what makes a windowed strip safe to click, and the alternative is
    where it was measured. A window CENTRED on the marked tab moves every column each time
    the operator switches, so the cell they just pressed holds a different name a moment
    later: on this project's own fifteen workspaces at 160 columns, six of the nine drawn
    tabs answer a second press at the identical column with a SECOND, different workspace.
    `_Tabs.switch_to`'s "the tab you are on is not news" cannot catch that — the name at
    that column really did change — so a double-click would switch twice, which is the one
    thing that rule exists to stop. Pages cannot: a drawn tab is by construction on the
    current page, so switching to it cannot turn the page.

    **Anchoring to the previous window would buy the same steadiness and would have to be
    remembered.** A panel is one long-lived process per slot, so state does survive an
    ordinary repaint — but not `cmd_respawn`, not the respawn hook a dead pane fires, and
    not a density change that re-splits the panels. The row would then depend on history
    the operator cannot see, and two frames showing the same plane at the same width could
    disagree. Pages need nothing remembered.

    **Both counts are paid for before a name is, and the trailing one is sized for the
    worst case.** The leading `+N` is exactly ``+start``, which is known before the page is
    built. The trailing one is not: whether it is drawn at all depends on where the page
    ends, which is what is being computed. Reserving the widest count the list can produce
    breaks that circle for the price of at most a cell or two on the final page, where the
    reserve is spent on a count that is never drawn. The alternative is an iteration whose
    fixed point is not obvious, to save a column.

    At least one field per page, always, so a name wider than the whole row cannot spin
    this. Such a page overflows, and :func:`_bar` measures the row it composed and gives
    the rung up rather than drawing it — the ladder's own rule, one rung down.
    """
    # The widest count either end can carry: every name but one left out. Measured with
    # `tui.width` for this module's own reason, even though charter mints the digits.
    tail = gap + tui.width(f"+{len(fields) - 1}")

    def room_for(start: int) -> int:
        """What a page beginning at *start* may spend on names and the gaps between."""
        return room - tail - (gap + tui.width(f"+{start}") if start else 0)

    def reach(start: int) -> int:
        """Where a page beginning at *start* ends when it is filled to the brim."""
        used, stop = tui.width(fields[start]), start + 1
        budget = room_for(start)
        while stop < len(fields) and used + gap + tui.width(fields[stop]) <= budget:
            used += gap + tui.width(fields[stop])
            stop += 1
        return stop

    cuts = [0]
    while cuts[-1] < len(fields):
        cuts.append(reach(cuts[-1]))
    # **The last page is not allowed to be a lone tab while the boundary can move**, and
    # that is #767. Every page but the last is filled to the brim, so the last one holds
    # the REMAINDER — and a remainder shrinks as the pages grow. With the marked name
    # sorting last that made a WIDER bar draw fewer names, and at 228 columns on a
    # fifteen-workspace plane it collapsed to a single tab: the one the operator was
    # already on, which `_Tabs.switch_to` correctly refuses. #758 returning at a width
    # wider than the one that fixed it.
    #
    # Moving the final cut one name left is decided on the NAMES and the WIDTH alone, so
    # it cannot move a page out from under a click — the property `_Tabs` depends on and
    # the reason this is not a window centred on the mark. Measured across 82 lists at
    # every room from 5 to 300: no page moves when the frame switches to a tab drawn on it.
    #
    # **Balancing every page instead was measured and is WORSE**, which is why only the
    # last one is rescued. Equal-sized pages ignore what names actually cost: on this
    # project's own fifteen workspaces it drew 4/4/5/7/7 tabs at 100/120/160/200/240
    # columns where filling each page draws 4/6/8/10/13, and across those 82 lists it left
    # MORE pages holding a single tab (6,969 against 4,775), not fewer. Packing the last
    # page from the right instead removes the drops and puts them on a middle name: the
    # same fifteen workspaces then draw 8 tabs at 160 and 6 at 200.
    #
    # **One condition, and the two that used to sit beside it were deleted rather than
    # documented.** They were written as guards against an empty page before the last one
    # and against a page before it that could not span the gap, and the deletion sweep
    # reported both as survivors — `<=` to `<` and `<` to `<=` changed nothing. Measured
    # rather than read, over 1,194,017 pages: `reach(cuts[-3]) < moved` is **never true**,
    # because `cuts[-3]` never moves and `reach` of it IS the greedy `cuts[-2]` this loop
    # only ever walks down from; and `moved <= cuts[-3]` is never the only reason to stop
    # — it is true 11,804 times and the condition below is true at every one of them,
    # while that condition alone stops the loop 757 times. Dropping both leaves every one
    # of those 1,194,017 pages identical. An equivalent mutant and dead code are the same
    # finding, and this repository deletes rather than suppresses.
    #
    # **A `len(cuts) >= 3` conjunct went the same way, and it is the more interesting of
    # the three.** It was not unreachable: with a SINGLE field the cuts are `[0, 1]` and
    # it is what stops the loop, 665 times in the sweep below. What it is not is
    # OBSERVABLE. A one-name list reaches this rung only when that name did not fit the
    # row, and the body composed from it is that same name — so `_bar`'s own measurement
    # refuses the rung whatever this returns. That is the identical masking already
    # recorded one function down for `if len(names) > 1`, which the sweep found for the
    # same reason. Measured over 1,963,800 rows, keeping the conjunct and dropping it draw
    # byte-identical output, with no runaway loop.
    #
    # The property it was informally protecting — that a page never starts left of zero,
    # so no negative column can reach :data:`TABS` — is not left to a masked guard. It is
    # asserted directly by `tests/test_frame_bars.AClickResolvesAgainstWhatWasDrawn
    # .test_no_row_at_any_width_ever_draws_or_maps_a_column_left_of_zero`, which stays
    # true if the measurement above ever stops masking it.
    while cuts[-1] - cuts[-2] < 2:
        moved = cuts[-2] - 1
        if reach(moved) < cuts[-1]:
            break
        cuts[-2] = moved
    # **What this does NOT restore is a monotone COUNT, and that is a stated limit rather
    # than an oversight.** The remainder still shrinks as the pages grow, so a name that
    # sorts late can still lose one tab as the pane widens — 10 such widths between 60 and
    # 280 on a fifteen-name list, down from 12, each of exactly one name. Those two numbers
    # are asserted by `tests/test_frame_bars.TheLadderGivesUpWholeThings
    # .test_the_limit_this_cut_does_not_fix_is_ten_widths_of_exactly_one_name`, so a change
    # to the cut has to come back and restate its cost rather than leaving this paragraph
    # to rot into folklore. What it does
    # guarantee is that the row is never reduced to the tab you are standing on while
    # there was room for another, which is the harm: at every width from 150 columns up,
    # the marked page holds at least two names where it used to hold one.
    return cuts


def _bar(head: str, names: list[str], here: str, width: int, *,
         note: str = "", rows: int = 1, busy=()) -> list[str]:
    """One bar: *head*, then *names* with *here* marked, in *rows* x *width* cells.

    :func:`_compose` composes it and this is what PUBLISHES it — the one call that writes
    :data:`TABS`, so "the map describes what is on screen" is a property of two lines
    rather than an agreement between six returns. The split is #829's: the launcher has to
    ask how many rows this strip's names need before any pane exists
    (:func:`bar_rows_wanted`), and a sizing question that published a click map would have
    the launcher's answer overwrite the panel's — a map describing a strip nobody is
    looking at.

    *busy* is :func:`_compose`'s and is passed straight through — see there.
    """
    lines, cols, more, add = _compose(head, names, here, width,
                                      note=note, rows=rows, busy=busy)
    TABS.publish(cols, here, more, add)
    return lines


def _compose(head: str, names: list[str], here: str, width: int, *,
             note: str = "", rows: int = 1, busy=()):
    """The strip *head*/*names*/*here* composes to, and the cells its tabs landed in.

    Answers ``(lines, columns, more, add)`` — the rows to draw, the ``(row, col)`` map
    :meth:`_Tabs.publish` takes, and the two cell sets that are not tabs. :func:`_bar` is
    the caller that publishes; nothing here writes anything down.

    *rows* is how many rows the PANE has, which the strip grows into only as far as its
    names need (#829). One is the shipped shape and every rung below is unchanged at it.

    **One function for both bars, so the two cannot degrade differently.** That is why it
    exists rather than each renderer composing its own: `workspaces` and `chats` sit on
    adjacent rows of the same frame, and two ladders would have them give up their names
    at two different widths — which reads as a bug in whichever gave up first.

    **The ladder, and every rung drops a whole thing rather than truncating one.**

    1. **Every name in full**, *here* marked. What a wide row shows.
    2. **The PAGE the one you are in falls on, plus a count at each end** (`+5  …  +9`).
       Every name on the row goes WHOLE, which is `_fit_fields`' own discipline one row
       over: a list cut off mid-name is a list where `api-staging` and `api-standby` are
       the same string, and half a name is worse than an honest count. :func:`_cuts` is
       the cut and carries why it is a page rather than a window centred on *here*.

       **On a strip of *rows* rows this rung draws *rows* consecutive pages**, and the run
       is picked the way the page is: the pages are grouped into runs of *rows* from the
       start of the list, and the strip draws the run its marked page falls in. That keeps
       the property the single page has and the whole cut exists for — a run is a function
       of the NAMES, the WIDTH and the row count alone, so switching to a tab that is drawn
       redraws the same run with only the `*` moved, and no cell the operator just pressed
       holds a different name a moment later. A leading `+N` goes on the FIRST row of the
       run and a trailing one on the LAST, because that is where the names they stand for
       actually are.

       **The last run can be short, and that is a stated cost rather than an oversight.**
       Runs are cut from the START of the list, so a list of four pages drawn three rows at
       a time has a last run of one — and a frame standing on it draws one row in a
       three-row pane. Filling that run by starting it at ``pages - rows`` instead was
       measured and is the WRONG trade: it makes a run depend on which page the mark is on
       at the boundary (from the tail run, pressing a tab drawn on page 2 would repaint as
       the run starting at page 0), so the cell the operator just pressed holds a different
       name a moment later — the double-press this whole cut exists to stop, arriving one
       axis over. Blank rows at the bottom of a strip are what a short run costs; a switch
       to the wrong tab is what filling it would. It is also only reachable while the row
       count is CAPPED: `bar_rows_wanted` asks for the rows that hold the whole list, and a
       run that is the whole list is never short.

       **This rung used to draw the marked name alone**, and on a real plane that made
       the whole bar inert: fifteen workspaces need 274 columns for rung 1, so every
       width an operator actually runs at drew `*harness-wrapper  +14` — one tab, the one
       they were already on, which `_Tabs.switch_to` correctly refuses. The bar was
       clickable and reached nothing. A page reaches five more at 120 columns, seven at
       160 and nine at 200.
    3. **`n/N` alone** — which position you are in and how many there are. This is the
       rung §3.6 calls "marks only", and a count IS what a mark per chat degenerates to
       once there is no room to draw one per chat. It says strictly more: `2/3` tells you
       where you are, three dots do not.
    4. **Nothing at all.** A bar with no room for rung 3 draws no row rather than a
       fragment of one.

    **There is no :data:`_NAME_MIN_W` check here, and its absence is the ladder working
    rather than a rule dropped.** §3.6 asks that the bar "never truncates a name below
    `slots._NAME_MIN_W` (12 cells) — below that it shows marks only", which is a floor
    against a TRUNCATION. This renderer never truncates: every rung drops whole names, so
    the property the floor exists to guarantee — that no name is ever shown in part — is
    unconditional here rather than conditional on a width. Written as an `if`, the check
    would be a line no input can reach past the rung above it, which is precisely the
    equivalent mutant the deletion sweep asks be deleted rather than documented.
    `tests/test_frame_bars.TheLadderGivesUpWholeThings
    .test_no_name_is_ever_shown_in_part_at_any_width` pins the property directly, at every
    width from 0 to 200.

    **`contain.one_line` runs before any of this arithmetic** (#472). Every name here has
    already been through a name check where it was read (`switch.workspaces`,
    `chats.of_workspace`), so this is a floor rather than the whole guard — but the
    arithmetic below is exactly the position #472 was filed about, where a table sized its
    columns from a raw name and one separator made the row wider than the pane.

    *note* is an extra field drawn after the names when there is room for it whole — the
    "add chat" affordance, and nothing else today. Dropped first, before any name, because
    it is a reminder and the names are the readout.

    *busy* is the names whose harness is working right now (#853) — chat ids, straight off
    `inflight.working_chats`. Each one drawn on the strip gets :data:`TAB_SPINNER` in the
    cell :data:`_BAR_MARK` would otherwise leave blank, so **the ladder above cannot see
    it**: every field is the width it was without it, the cut is the cut it always was, and
    :func:`bar_rows_wanted` — which runs in the LAUNCHER, where nothing knows or should
    know which chat is thinking — composes the same rows it will draw. That is why *busy*
    defaults to empty here rather than being read inside: the sizer and the renderer ask
    one arithmetic, and only the renderer has an opinion about the clock.

    Empty for the `workspaces` strip, which is not a strip of chats and has no harness to
    be working.

    **Every rung answers with its own cell map**, which is what makes the bar clickable at
    all and is the same discipline `_repos` keeps for the table's rows: the handler
    resolves a click against WHAT WAS DRAWN rather than against what it thinks would have
    been. Only the names actually on the strip get cells — the heading, the blank under it,
    the gaps, BOTH `+N` counts, the `n/N` and the affordance are cells this bar drew no tab
    into and a click on one of them switches nothing. **The rungs that draw NOTHING answer
    too**, with an empty map, and that is the half `_Viewport.blank` exists for one axis
    over: a bar that kept its last map through a resize down to `2/3`, down to one row, or
    down to no row at all, would switch to a tab the operator can see is not on screen.

    **The windowed rung sharpens that rather than softening it.** It draws a different
    slice of the names at every width and at every row count, so a map kept across a resize
    would answer with a name that is genuinely somewhere else on the strip — not merely
    absent from it. The map is built from the composition itself (`rung`'s *before*, and
    :func:`_tab_columns` walking the fields each row actually joined), so there is no
    second walk of the ladder to disagree with the first.
    """
    # **`head` and `note` are NOT contained, and the deletion sweep is what settled it.**
    # Both are charter's own literals — `"chats"`, `"workspaces"`, :data:`ADD_CHAT` — and
    # no caller can hand this an open-alphabet one, so a `contain.one_line` on either is a
    # call whose result is provably its argument. The sweep found both as survivors for
    # exactly that reason: no input could make the mutation differ. The NAMES are
    # contained, below, which is where the open alphabet actually is.
    from . import chrome
    lead = _inset() + head + " " * _BAR_GAP
    # **The rows under the first start where the first row's tabs do.** The heading names
    # the strip once — a `chats` repeated down the left of a three-row strip is the same
    # word three times where names are competing for columns — and the blank under it is
    # what keeps every row's tabs in one column, so a run of pages reads as one block
    # rather than as three rows that happen to be adjacent. It is also what lets every page
    # be cut against ONE `room`: a second row starting further left would be a wider row
    # than the cut was made for, and the cut is what a click's stability rests on.
    under = " " * tui.width(lead)
    # **Resolved ONCE, here, and handed to everything below it.** The cut
    # (:func:`_cuts`), the composition and the map (:func:`_tab_columns`) all need to
    # agree about how wide the seam between two tabs is, and `charter.toml` is re-read
    # behind :func:`_bar_gap` — so three readings is three chances for the map to describe
    # a row that was drawn with a different gap. This is `_Viewport.blank`'s discipline
    # said about a number instead of about a pair of fields.
    gap = _bar_gap()
    gapw = tui.width(gap)

    def rung(drawn_rows=(), more=(), add=()):
        """This rung's rows, and the cell map for the tabs they actually drew.

        **Every way out of the ladder goes through here**, which is what keeps "the map
        describes the strip" a property of one function rather than an agreement between
        six returns. `_repos` learnt that the expensive way: three of its four exits
        cleared the click map and none of them cleared the scroll bound, and
        `_Viewport.blank` is the method that finding produced.

        *drawn_rows* is one ``(before, body, drawn)`` per row of the strip, top down. No
        rows at all is the rung that draws nothing — rung 4, and a bar with no names. It
        still returns a map, an empty one, for the reason the docstring above gives.

        *before* is what a row draws between its lead and its FIRST tab — the leading `+3`
        of the windowed rung, and nothing else today. It is passed rather than glued onto
        *body* by the caller because the map is measured from where the first tab starts: a
        rung that composed its own prefix would be publishing columns three cells left of
        the names it drew, which is a click landing on the tab beside the one the operator
        pressed. Every other row starts its tabs at the lead and says so by passing "".

        *more* is the cells of the fields that stand for names this rung could NOT draw
        — both `+N` counts, and the `n/N`. Passed here rather than worked out inside
        :data:`TABS` for :func:`_tab_columns`' whole reason: the rung that composed the
        strip is the only thing that knows where it put them, and a second walk of the
        ladder to find them again is a second answer to what is on screen.

        *add* is the same for :data:`ADD_CHAT`, on the one rung that draws it.
        """
        cols: dict = {}
        lines: list[str] = []
        for r, (before, body, drawn) in enumerate(drawn_rows):
            head_cells = lead if r == 0 else under
            cols.update(_tab_columns(r, tui.width(head_cells + before), drawn, gapw))
            lines.append(head_cells + before + body)
        return lines, cols, more, add

    def row(body: str = "", drawn=(), before: str = "", more=(), add=()):
        """A rung that draws ONE row, said the way three of the four rungs say it.

        :func:`rung`'s single-row case, and a wrapper rather than a shape every rung has to
        spell: three of the four draw exactly one row and said so before #829 gave the
        fourth some more, and rewriting them to hand over a one-element list of triples
        would be three call sites edited to say what they already said. An empty *body* is
        the rung that draws nothing — rung 4, and a bar with no names — which is no rows at
        all rather than one blank one.
        """
        return rung([(before, body, drawn)] if body else [], more, add)

    if not names:
        return row()
    # **Which row is yours is decided on the RAW names; only the drawing uses the
    # contained ones.** `choose.Roster` makes the same split one surface over and for the
    # same reason: `contain.one_line` is a repair, so two names that differ only in what
    # it repairs are one string after it, and a mark matched on the drawn text would
    # follow the repair rather than the identity. Neither caller can reach that today
    # (`chats.ID_RE` and `workspace.valid_name` both refuse the characters `one_line`
    # touches), which is exactly why it is written as an index now rather than found as a
    # bug by whichever caller stops. The same split reaches :data:`TABS`, which is handed
    # the raw name beside the drawn field: a click has to switch to what is on disk.
    at = names.index(here) if here in names else -1
    shown = [contain.one_line(n) for n in names]
    # **The mark's cell carries the spinner too, and it is decided on the RAW name for the
    # reason the paragraph above gives about the index**: `busy` holds chat ids off disk,
    # `shown` holds text `contain.one_line` may have repaired, and matching a mark against
    # the repaired form would follow the repair rather than the identity.
    #
    # `i != at` before the membership test, so the active tab keeps its `*` — see
    # :data:`TAB_SPINNER` for why that one cell goes to the mark rather than to the
    # spinner. Read once, outside the comprehension: every tab on a strip shows the same
    # frame (`tab_spinner_frame`), and reading the clock per name would let a wide strip
    # start a row on one frame and finish it on the next. Read unconditionally, too: an
    # `if busy` in front of it would change no output at all, only one `time.monotonic()`,
    # which is the equivalent mutant this repository deletes rather than documents.
    frame = tab_spinner_frame()
    marked = [f"{_BAR_MARK[0] if i == at else (frame if names[i] in busy else _BAR_MARK[1])}{n}"
              for i, n in enumerate(shown)]
    # **The same fields twice: one set to MEASURE and one set to DRAW.** `chrome.block`
    # adds no cell — `tui.width` counts no SGR — so the two are the same width by
    # construction and either could have been measured. Measuring the plain one is what
    # makes that a property of the code rather than a fact a reader has to know about
    # `tui.width`, and it is the same split `chrome`'s module docstring insists on
    # everywhere else: compose and measure, THEN paint.
    #
    # **The block covers the mark and the name together**, which is exactly what
    # :func:`_tab_columns` gives that tab as its columns and what `_Tabs.switch_to`
    # answers for — so what the operator sees highlighted is what a click there is about.
    # And it is reverse video rather than a colour for `chrome`'s stated reason: it is the
    # operator's own two colours exchanged, so it cannot be wrong on a theme charter
    # cannot see, and `[frame] chrome = "off"` — the shipped default — has nothing to do
    # with it. Under `NO_COLOR` `panel._write` strips it and the `*` is still there, which
    # is why the mark stays and was not replaced by the highlight.
    painted = [chrome.block(f) if i == at else f for i, f in enumerate(marked)]
    room = width - tui.width(lead)
    joined = gap.join(marked)
    if note and tui.width(joined) + gapw + tui.width(note) <= room:
        # The affordance's own cells, measured off this composition — :func:`_span`'s
        # reason, and the same walk the counts get one rung down. It is drawn only where
        # the whole list is on the strip, which is why a `+` and a `+9` are never on one.
        return row(gap.join(painted) + gap + note, zip(names, marked),
                   add=_span(0, tui.width(lead + joined + gap), note))
    if tui.width(joined) <= room:
        return row(gap.join(painted), zip(names, marked))
    if at >= 0:
        # **No `if len(names) > 1` here, and it is unreachable rather than merely
        # untested.** With ONE name the page is that name, both counts are absent, and the
        # body this composes IS `joined` — so the rung above asks exactly what this one
        # asks and always answers first. The sweep found the old conditional as a survivor
        # for that reason and the shape has not changed.
        cuts = _cuts(marked, room, gapw)
        # **The run of pages this strip's *rows* rows draw, and the page the mark is on is
        # in it by construction.** The pages are grouped from the start of the list into
        # runs of *rows*, so the run is `bisect`'s page index divided by the row count —
        # a function of the names, the width and the row count and of nothing that moves
        # when the operator switches. `rows == 1` makes every run one page, which is
        # exactly the answer this rung gave before it could grow.
        marked_page = bisect.bisect_right(cuts, at) - 1
        run = (marked_page // rows) * rows
        stop = min(run + rows, len(cuts) - 1)
        first, last = cuts[run], cuts[stop]
        # Neither count is a tab. `+9` stands for names that are not on the row, so there
        # is nothing there to switch to and saying so is better than picking one of them —
        # `_Viewport.publish`'s rule for `…(+N more)`, one axis over. **Two of them, and
        # the left one is what a windowed strip owes the operator**: a single trailing
        # `+14` beside a page that starts in the middle of the list says the names on the
        # row are the FIRST fourteen, which is false. `+5  *harness-wrapper  …  +9` says
        # where in the plane's fifteen this page sits, which is the readout `n/N` gives
        # one rung down and the reason that rung was worth keeping.
        #
        # **Both are CLICKABLE now and neither is a tab**, which are two statements that
        # sit together rather than in tension. `+9` still names no chat and still switches
        # nothing; what it does is open the palette, because the palette is what the bar
        # hands off to when the readout runs out of room (`_Tabs.more_at`). The operator
        # who reported this had pressed one, which is the strongest evidence available
        # about what a `+9` looks like it does.
        leading = f"+{first}" if first else ""
        trailing = f"+{len(names) - last}" if last < len(names) else ""
        # **The affordance rides the run that holds the WHOLE list**, and only that one.
        # `_Tabs.add_at`'s structural promise is that a strip carrying `+` carries no `+N`
        # anywhere, and this is where it is kept: neither count exists exactly when the run
        # starts at the head of the list and reaches its end, which is the multi-row form
        # of the rung above. A `+` beside a `+9` on two rows of one strip would be the two
        # `+` fields on screen together that the promise exists to prevent.
        # The affordance rides only a run that holds the WHOLE list — no leading count and
        # no trailing one — which is `_Tabs.add_at`'s structural promise in the shape a
        # strip with rows needs it: a `+` and a `+9` are never on screen together, and a
        # second ROW is on screen with the first. `note` is already `""` where a caller
        # passed none, so the conjunct testing it again was a line no input could make
        # observable and is not written.
        tail = note if not leading and not trailing else ""
        # **Which row each field that is NOT a name goes on, decided ONCE and not per
        # row.** The leading count belongs beside the first tab drawn and the trailing one
        # after the last, because that is where the names they stand for actually are; on a
        # one-row strip both land on the one row, which is the answer this rung gave before
        # it could grow. Written as two edits to a list of blanks rather than as a
        # `r == 0`/`last_row` test inside the loop: the loop below is then straight-line,
        # and four conditional expressions that each had to be got right per row become two
        # statements that are true of the run.
        span = range(run, stop)
        befores = [""] * len(span)
        afters = [""] * len(span)
        if leading:
            befores[0] = f"{leading}{gap}"
        if trailing or tail:
            afters[-1] = f"{gap}{trailing or tail}"
        start = tui.width(lead)
        plain, painted_rows, ends = [], [], []
        for r, page in enumerate(span):
            lo, hi = cuts[page], cuts[page + 1]
            tabs = gap.join(marked[lo:hi])
            ends.append(start + tui.width(befores[r] + tabs + gap))
            plain.append((befores[r], tabs + afters[r]))
            painted_rows.append((befores[r], gap.join(painted[lo:hi]) + afters[r],
                                 list(zip(names[lo:hi], marked[lo:hi]))))
        # Where the fields that are not names landed, taken from the composition above
        # rather than searched for in the finished string — :func:`_tab_columns`'
        # discipline for the names, kept for the rest. **No branch here at all**:
        # :func:`_span` answers an empty span for an empty string, and `leading`, `trailing`
        # and `tail` are already `""` where this run does not carry them, so a run at the
        # head of the list contributes no leading cells by arithmetic rather than by a
        # test somebody has to keep true.
        # **`bottom` and not `last`**, which is a name this rung already gave to the index
        # one past the last NAME on the run: `trailing` is computed from that one and
        # everything below is about a ROW, and two meanings for one word on a strip whose
        # whole subject is which row a field is on is the reading error to refuse.
        bottom = len(plain) - 1
        counts = _span(0, start, leading) + _span(bottom, ends[bottom], trailing)
        add = _span(bottom, ends[bottom], tail)
        # Measured, not assumed. :func:`_cuts` puts at least one name on a page, so a name
        # wider than the whole row composes a body that overflows — and this ladder gives a
        # rung up rather than drawing part of anything. Measured on the PLAIN rows for the
        # reason `painted` states: they are the same width as the painted ones, and the one
        # that is the same width by construction is the one to hold a refusal on. EVERY row
        # is measured, not just the first: a run whose second page overflows is a strip
        # drawing part of a name on a row nothing else would have looked at.
        if all(tui.width(before + body) <= room for before, body in plain):
            return rung(painted_rows, more=counts, add=add)
    counted = f"{at + 1}/{len(names)}" if at >= 0 else str(len(names))
    if tui.width(counted) <= room:
        # **The narrow rung is a count too, and it is the one where this matters most.**
        # `2/3` stands for every name the row could not draw — all of them — so it is the
        # widest form of the same field, on the only rung where the bar reaches nothing at
        # all. A frame narrow enough to fall here had a strip that could be pointed at and
        # not pressed; it opens the palette now, which is the surface that works at every
        # width.
        return row(counted, more=_span(0, tui.width(lead), counted))
    return row()


#: The chat bar's add-chat affordance (§3.6) — a BUTTON now, and one cell of it.
#:
#: *"`+` button not working for creating new session."*
#:
#: **This was a sentence and the sentence was the defect.** It read `+ charter <harness>
#: opens another`, which is true, which names the command that does it, and which begins
#: with a `+` at the end of a row of clickable tabs. Every terminal an operator has used
#: puts a `+` there and every one of them means *new*. So it was pressed, and a sentence
#: cannot be pressed. `commands_frame.cmd_new_chat` is what it starts now
#: (`frame/builtins._bar_events`), and `_Tabs.add_at` is which cell.
#:
#: **A bare `+` rather than `+ new`, and the cost is stated rather than hidden.** What is
#: lost is the sentence naming the command an operator could type instead — and what is
#: gained is that they do not have to, which is the whole change. `docs/frame.md` keeps
#: the sentence, where it can be as long as it needs to be; a strip competing for columns
#: is the wrong surface for a paragraph. Every extra cell here is a cell off the names,
#: and the names are the readout.
#:
#: **ASCII, for :data:`_BAR_RULE`'s reason and not merely :data:`_BAR_MARK`'s.** A click on
#: this row is resolved by COLUMN, so a glyph a terminal may draw two cells wide moves
#: every field after it. `+` is one cell everywhere, and it is the last field on the row,
#: so even its own width is not load-bearing — which is exactly the argument that would
#: make somebody reach for a prettier glyph, and the reason the rule is stated as the row's
#: rather than as this constant's.
#:
#: **It cannot share a row with a `+N`**, which is what keeps the two `+` fields from being
#: confused: :func:`_bar` draws this only on the rung where every name fits, and draws a
#: count only on the rung where they do not.
ADD_CHAT = "+"


def working_chats() -> frozenset:
    """Which chats' harnesses are working right now — a set of chat ids, possibly empty.

    The renderer's half of the turn tracker (`inflight.working_chats`), and a wrapper
    rather than a direct call for the reason every read on this repaint path has one:
    **never raises.** A panel that threw out of `render` loses its pane, and a tracker
    charter cannot read is "nothing is working" — which degrades to the strip that was
    drawn before this feature existed rather than to a hole in the frame.

    A set rather than the tracker's `(chat, last_seen)` pairs: what a strip does with this
    is one membership test per name, and the times are the PANEL's half of the same read
    (`panel._working`, which needs them to know when its cached answer expires). Two
    readers of one tracker, each taking the part it has a use for — `inflight.live_records`
    and `inflight.live` are the same split one tracker over.
    """
    from .. import inflight
    try:
        return frozenset(chat for chat, _seen in inflight.working_chats())
    except Exception:  # noqa: BLE001 - a readout must never cost a pane
        return frozenset()


def _chats_strip(fid: str):
    """What the chat strip is a strip OF — its heading, its names, the one you are typing
    in, and its note.

    Never raises for a plane it cannot read: `chats.roster` already answers with the chat
    asking and nothing else for a frame root it could not scan, and `_bar` answers `[]`
    for an empty list.
    """
    from . import chats as chats_mod
    return "chats", [c.id for c in chats_mod.roster(fid)], fid, ADD_CHAT


def _workspaces_strip(fid: str):
    """:func:`_chats_strip` one noun over. The name is the FRAME's
    (`switch.current_workspace` → `state.workspace_for`), never one this pane resolved for
    itself — #512, and the same rung order `_top` reads two rows up.

    **No `+`, and that is a decision rather than an omission.** A new CHAT is nothing but a
    press: its id is allocated, its workspace is the one this chat is in for life (§4j), and
    there is nothing for an operator to type or for charter to validate. A new WORKSPACE is
    a directory and a NAME — `workspace.valid_name`, `workspace.ensure`, and #518's whole
    argument that "a picker that creates on a typo leaves litter". A `+` here would have to
    open something that takes a name, which is `charter workspace create` and is not a
    thing a one-row strip can be.
    """
    from . import switch as switch_mod
    return ("workspaces", switch_mod.workspaces(),
            switch_mod.current_workspace(fid), "")


#: The slots that draw a TAB STRIP, mapped to what each one is a strip of.
#:
#: :data:`SLOTS` one component family over, and a table rather than a pair of literals for
#: that constant's reason: it is what makes "which slots have a height that follows their
#: content" a question :func:`bar_rows_wanted` can answer for a name, and what makes a
#: third strip added to charter reach the sizer on the day it is written rather than the
#: day somebody remembers this file. The deletion sweep can see an entry removed from
#: here; it could see nothing at all in a comment.
BARS = {"chats": _chats_strip, "workspaces": _workspaces_strip}


def bar_rows_wanted(fid: str, slot: str, *, pane_cols: int, cap: int) -> int:
    """How many rows *slot*'s tab strip needs to draw every name it has, in a
    *pane_cols*-column PANE — never fewer than one and never more than *cap* (#829).

    *"panes are resizable — and user can resize and it should show more tabs opened on new
    resized rows."* A strip cannot make its own pane taller: tmux owns pane geometry and
    both bars declare `Fixed(1)`, so the row an overflowing strip needs has to be asked for
    by the LAUNCHER (`layout.slot_sizes`, through `commands_frame._slot_sizes`) and this is
    the question it asks.

    **One answer to "how tall is this strip", read by both sides of it** —
    :func:`repos_rows_wanted`'s whole argument, one slot family over, and it is stronger
    here because this asks the renderer itself. The rung a strip lands on is a four-step
    ladder with a greedy cut inside it; a second arithmetic that predicted the row count
    from the names and the width would be a second implementation of that ladder, and the
    two would disagree the first time a rung moved. :func:`_compose` is the ladder, so what
    this counts is what the pane will actually hold.

    It composes and never publishes, which is what :func:`_compose` exists for: this runs
    in the LAUNCHER's process and in the `frame-resize` child, and a sizing question that
    wrote :data:`TABS` would leave a map describing a strip nobody is looking at.

    *cap* is a caller's, never a policy read here — `layout.BAR_MAX_ROWS`, threaded exactly
    the way `layout.repos_rows` takes *pinned_rows* and for the same reason: applying a
    ceiling here AND in `layout._grown` would be a bound no input could make observable,
    which is the survivor `tools/sweep.py` reports and this repository deletes. It also
    bounds the loop, so a plane with two hundred chats costs *cap* compositions and not
    two hundred.

    **The component's own pad comes off *pane_cols* first**, for the reason
    :func:`repos_rows_wanted` gives about #500: `_chats` composes at :func:`content_width`,
    so a padded pane's strip is planned for `pane_cols - 2 * pad` and asking the unpadded
    question would size this pane from a number the renderer never sees.

    **The answer is the tallest height the strip FILLS, and there is deliberately no "does
    it hold every name yet" test beside it.** There was one — an early ``return rows`` the
    moment the map held every name — and the deletion sweep reported dropping it as a
    SURVIVOR. Measured rather than argued: over 406,000 inputs (32 name lists, every mark,
    every width from 0 to 300, five ceilings) the two answer the same number every time,
    and they must. A run holds every name exactly when the cut has no more pages than the
    strip has rows; at that height the run IS the whole list, so the strip fills every row
    it was given and the line below records it. A greater height cannot beat it either —
    there are no more pages to draw, so the strip draws fewer rows than it is offered and
    that height is not recorded. An equivalent mutant and dead code are the same finding,
    and this repository deletes rather than pins.

    What is left says the whole rule, and it covers two shapes at once: the rungs below the
    windowed one draw `2/3` or nothing whatever they are offered — a frame too narrow to
    draw one name is too narrow to draw one on row two — and a run that falls at the end of
    the cut can be shorter than the rows it was given (see :func:`_compose`). Measuring
    ROWS rather than rungs is also what covers a rung added below this one on the day it is
    written: what is being bought is rows.

    One for a *slot* that draws no strip — a caller that asked about `repos` gets the
    height the pane already has, which is `layout.slot_sizes`' own filter-don't-refuse
    degrade rather than a raise on a name out of a committed file. One, too, for a *cap*
    below one: `filled` is the floor and the loop simply does not run, so there is no
    `max` in front of the range for no input to make observable.
    """
    # **Not spelled `strip`**, though it is the natural word for what a `BARS` entry
    # answers about. `tools/sweep.py`'s `swap-synonym` operator reads `strip` as the string
    # method and offers `lstrip` for it, so a local of that name spends a mutation on a
    # question about nothing — the same word collision #842 records one guard over, where
    # `tests/test_claims`' `_REDACT_VERB` reads `strip` as a promise to redact.
    entry = BARS.get(slot)
    if entry is None:
        return 1
    head, names, here, note = entry(fid)
    width = pane_cols - 2 * pad_for(slot, pane_cols)
    filled = 1
    for rows in range(1, cap + 1):
        lines, _cols, _more, _add = _compose(head, names, here, width,
                                             note=note, rows=rows)
        if len(lines) == rows:
            filled = rows
    return filled


def chats_bar(fid: str, width: int, rows: int = 1) -> list[str]:
    """Which chats this workspace holds and which one you are typing in.

    **A readout, never the mechanism** (§3.6). The palette reaches every chat in two
    keystrokes at every width, including widths where this row cannot be drawn at all —
    which is why the row may degrade to a count and then to nothing, and why
    `layout._DROP_ORDER` gives it up before `top`.

    *rows* is the pane's height, which the strip grows into only as far as its names need.
    It defaults to one because one is the shape every caller had before #829 and the shape
    a strip whose names fit still has.

    **The one field on this strip that moves on its own** (#853): a chat whose harness is
    working shows :data:`TAB_SPINNER` where an idle one shows a blank. That is why `chats`
    is in :data:`BAR_ANIMATED` and why `panel._watch` gives it a ticking gate of its own —
    a gate `slots.ANIMATED`'s cannot serve, since that one asks about plane-wide in-flight
    dispatches and this chat's notice dwell, and neither is "a sibling chat's harness
    started working".
    """
    head, names, here, note = _chats_strip(fid)
    return _bar(head, names, here, width, note=note, rows=rows, busy=working_chats())


def workspaces_bar(fid: str, width: int, rows: int = 1) -> list[str]:
    """Which workspaces this plane has and which one this frame is drawing.

    :func:`chats_bar`'s rules and its ladder, one noun over — see :func:`_workspaces_strip`
    for what it draws and why it draws no `+`.
    """
    head, names, here, note = _workspaces_strip(fid)
    return _bar(head, names, here, width, note=note, rows=rows)


#: Which slots draw something that CHANGES ON ITS OWN, with no version bump and no
#: resize behind it. Exactly the renderers that reach :func:`spinner_frame`, which today
#: is `_bottom` and only `_bottom` (through :func:`_inflight_field`).
#:
#: **This is what scopes the animation's cost to the pane that needs it.** `panel._watch`
#: runs one process per slot, so an unscoped "is work in flight" would repaint every one of
#: them at `panel.TICK` for the whole length of a dispatch — the rest redrawing byte-identical
#: output. That is not free: measured on this project (8 personas, 6 repos), one
#: `render("right")` costs 4 816µs, because `statusline._persona_chips` asks
#: `persona.is_draft`, `structural_errors`, `_mem_count` twice and `_vault_dot` per
#: persona — a helper whose own docstring says "this renders on every single turn", written
#: for once a turn and not five times a second. At 5Hz that one pane alone is ~2.4% of a
#: core, for a picture that cannot change.
#:
#: Kept as data rather than inferred, and kept HONEST by
#: `tests.test_frame_density.OnlyTheAnimatedSlotAnimates`, which renders every slot twice
#: at two different clock readings with a record in flight and asserts the output differs
#: for exactly the names in here — so a renderer that gains a spinner without joining this
#: set, or leaves one behind without leaving it, is red rather than silently still.
ANIMATED = frozenset({"bottom"})

#: Which BAR slots draw something that changes on its own — the tab spinner, and today
#: `chats` and only `chats` (:data:`TAB_SPINNER`, through :func:`working_chats`).
#:
#: **A second set rather than a `chats` added to :data:`ANIMATED`, because the two name
#: different GATES and not merely different slots.** `panel._watch` ticks an `ANIMATED`
#: slot while `_running(inflight_cache) or _notice_pending(fid)` — plane-wide in-flight
#: dispatches, and this frame's own switch-notice dwell. Neither has anything to do with
#: whether a sibling chat's harness is thinking: a plane with no dispatch running would
#: never tick, and a plane with one running would tick the strip for half an hour with
#: every tab idle. Folding the two together would make each condition wake the other's
#: pane, which is exactly the unscoped repaint `ANIMATED` was written to prevent.
#:
#: So `chats` gets `panel._working`'s gate — `inflight.turn_stamp`, one `stat` — and
#: `bottom` keeps `panel._running`'s. A slot may legitimately end up in both sets one day;
#: `_watch` ORs them and neither membership implies the other.
#:
#: Kept honest the same way :data:`ANIMATED` is, by
#: `tests.test_a_chat_tab_shows_its_harness_working.OnlyTheChatStripSpins`: every slot is
#: rendered twice at two clock readings with a chat marked as working, and the output must
#: differ for exactly the names in here.
BAR_ANIMATED = frozenset({"chats"})


def unimplemented(configured) -> list[str]:
    """Which of *configured* charter sizes and accepts but has no renderer for.

    **A name, not a key of :data:`SLOTS`.** This is the filter a placed provider had to
    survive and did not: it asked "is there a renderer in charter's own table", and a
    component supplied by an installed distribution never is one, so every provider was
    dropped before `panel_argvs` could split a pane for it. It asks the frame's question
    now — *can anything draw this* — of which charter's own four renderers are one half
    and `builtins.supplies` the other. Metadata only; nothing here imports a provider.

    Answers empty: every slot `instance.FRAME_SLOTS` accepts and `layout.SLOT_SIZE`
    sizes has a renderer in :data:`SLOTS`. It stayed non-empty for a whole release
    while `left`/`right` were sized but not drawn, and #488 closed the question from
    the other end — `left` is retired from both registries at once rather than left
    half-present in one. Kept rather than deleted —
    the next slot this frame grows will pass through here on day one exactly
    the way `left`/`right` did, and three callers need exactly
    this list and must agree — `commands_frame.cmd_launch` (to skip splitting a
    pane that would be permanently dead under `remain-on-exit on`),
    `commands_frame.frame_ready` (`--probe`) and `doctor.check_frame` (both to
    SAY so, which is the only place it is said at all now) — so the question
    stays answered here, next to the registry that answers it, rather than
    three times over.
    """
    return sorted({s for s in configured if not drawable(s)})


def drawable(name) -> bool:
    """Whether `charter panel <name>` has anything to draw — the ONE answer.

    Four callers need it and must agree, which is why it is a function and not four
    membership tests: :func:`unimplemented` (so a pane is not split for a name nothing can
    fill), `frame/panel.py:run` (so a panel refuses rather than painting an empty pane),
    and `commands_frame`'s two respawn guards (so a name charter cannot resolve never
    reaches tmux config text). Two implementations of one question hide each other's
    defects (#547), and this question now has a second half — a provider's component —
    that all four have to see the same way.

    A committed slot name and the component id behind it both answer yes, because they are
    one component (`builtins.component_id`). A name an installed provider supplies answers
    yes because its module can draw it. Everything else answers no, and the caller says so
    where it can be read.

    **A property, never a spelling.** `acme.metrics` is not admitted for having a dot in
    it: it is admitted because a distribution on this machine declares it in the
    `charter.components` entry point group. `top.` looks exactly as namespaced and is
    refused, which is what keeps the respawn hook's guard a guard.
    """
    from . import builtins as _builtins
    if not isinstance(name, str):
        return False
    if name in SLOTS:
        return True
    cid = _builtins.component_id(name)
    if cid in _builtins.SLOT_OF:
        # A built-in whose renderer has been taken out of :data:`SLOTS`. It does not fall
        # through to the providers: a distribution declaring `sidebar` must not become
        # the answer to a question about charter's own component.
        return _builtins.SLOT_OF[cid] in SLOTS
    if _builtins.places(cid):
        # One of charter's own with no committed slot-name spelling — Phase 5's two bars.
        # It is drawn through the component contract (`panel._component_painter`) rather
        # than out of :data:`SLOTS`, so the `SLOT_OF` question above cannot answer for it,
        # and it does not fall through to the providers for that question's own reason: a
        # distribution declaring `chats` must not become the answer to a question about
        # charter's own component.
        return True
    return _builtins.supplies(cid)


def render(slot: str, fid: str) -> str:
    """Draw *slot*, or a one-line explanation of why it could not be drawn.

    Never raises. A panel that dies leaves a hole in the frame, which is worse than a
    line saying what went wrong — the same promise `statusline.render` already makes and
    documents.

    **The pad is applied HERE, on the way out**, and the two halves of it meet at this
    line: :func:`content_width` told the renderer a narrower pane and :func:`inset_rows`
    moves what it composed to the right. Doing it here rather than inside each renderer is
    what keeps the pad from being spelled four times, and doing it AFTER the renderer is
    `chrome.fill`'s ordering rule — these are finished rows, and a `tui` node would strip
    the inset back off (measured; see that module's docstring).

    **Neither failure line is padded, and that is what keeps "never raises" true.** The
    pad is read out of `config.FRAME` (:func:`pad_of`), so it is one more thing that can
    go wrong — and a fallback composed through the mechanism that may be the thing that
    failed is not a fallback. Both lines measure through :func:`_width` alone, which is
    one `os.get_terminal_size` with its own `OSError` guard and reads no config at all.
    The `unknown slot` line is outside the `try` entirely, so a padded one would have had
    nothing to catch it; the `unavailable` line is inside, and would have re-raised on its
    way out of the handler. A message drawn flush left in a padded pane is a message that
    is visibly not the panel, which is honest here rather than untidy.
    """
    fn = SLOTS.get(slot)
    if fn is None:
        return tui.truncate(f" charter: unknown slot {slot}", _width())
    try:
        return inset_rows(fn(fid), slot)
    except Exception as e:
        return tui.truncate(f" charter: {slot} unavailable ({type(e).__name__})",
                            _width())
