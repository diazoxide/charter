"""What each edge of the frame says.

Content comes from the renderers `statusline.py` already has — they are composed here,
never rewritten, so a fix to a repo row or an alert lands in both surfaces at once.

**A panel measures its own pane, never `$COLUMNS`.** A panel process is started as a
tmux pane command, which inherits the *launching* shell's environment whole. Measured: a
tmux pane 22 columns wide, launched from a shell that had exported `COLUMNS=200`, ran a
probe that saw `COLUMNS env='200'` while its real tty was 22 columns.
`charter.tui.term_width()` reads `$COLUMNS` first — correctly, for the status line, where
stdout is a pipe and the environment is the only source of the truth. That order is
exactly wrong here: trusting it would lay every panel out at the OUTER terminal's width
and wrap catastrophically inside its own narrow pane. `tui.term_width()` itself is left
alone (the status line depends on its env-first order); `_width` below asks the pane's
own tty directly instead, and only reaches for `tui.term_width()` as a last resort, when
there is no tty to measure at all.
"""

from __future__ import annotations

import os
import sys
import time

from .. import tui

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


def spinner_frame(now: float | None = None) -> str:
    """Which frame of :data:`SPINNER` this instant shows.

    Read off `time.monotonic()` rather than advanced by a caller — see :data:`SPINNER`.
    *now* is for tests, which need a specific frame rather than whichever one the clock
    happened to be on.
    """
    t = time.monotonic() if now is None else now
    return SPINNER[int(t / SPINNER_PERIOD) % len(SPINNER)]


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

    **`state.panes` is the record, and it is the only one that can answer this.** It is
    written where a frame's shape is actually DECIDED — by `_draw_panels` at launch and
    again by `cmd_density` after `_relayout` — from the panes tmux really gave back, so a
    slot whose `split-window` failed is absent from it exactly like a slot the density
    dropped. The alternatives are all worse in the same direction: `instance
    .density_slots` is what was *asked for* rather than what is *there*, `[frame] slots`
    is what the operator configured, and an environment variable is whatever was true at
    launch — the one thing the docstring above says it must not be. tmux itself cannot be
    asked either: `list-panes` reports ids and geometry and nothing that says which pane
    charter meant as `right` (see `state.record_panes`).

    **The cost is one small JSON read on a slot that is not animated.** `top` is not in
    :data:`ANIMATED`, so it repaints on a version bump, never on `panel.TICK` — and a
    density change bumps the version precisely so that surviving panels re-read, which is
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


def _width() -> int:
    """The pane's own width in columns — measured, never read out of `$COLUMNS`.

    `os.get_terminal_size(sys.stdout.fileno())` asks the file descriptor this process is
    actually writing to, which for a panel launched as a tmux pane command IS the pane —
    not a pipe, not the launching terminal. Only when that raises (stdout redirected to
    something with no tty behind it at all, e.g. a test, or a panel run for debugging
    with its output piped to a file) does this fall back to `tui.term_width()`, which is
    the env-first helper the status line also uses. The measured value is returned as
    reported, with no artificial floor clamped over it: a pane that really is 10 columns
    wide getting reported as 20 would be exactly the theorising this function exists to
    avoid — it would tell every caller here there is room that the real pane does not have.
    """
    try:
        return os.get_terminal_size(sys.stdout.fileno()).columns
    except OSError:
        return tui.term_width(default=80, floor=20)


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
    """
    try:
        return os.get_terminal_size(sys.stdout.fileno()).lines
    except OSError:
        return _DEFAULT_ROWS


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
    if verbosity(fid) == "terse":
        return tui.truncate(identity, _width())
    build = f"charter {__version__}{statusline._dev_chip()} "
    w = _width()
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
    mr = f"{sl._GREEN}{sigil}{change}{sl._R}" if change else ""

    row = tui.Row(tui.Cell(f"{lead}{name_markup}", 2 + 3 + sl._NAME_W),
                  tui.Cell(branch_cell, sl._BRANCH_W),
                  tui.Cell(sl._ci_part(r.get("ci")), sl._CI_W),
                  tui.Cell(mr, sl._MR_W),
                  gap=sl._GAP)
    return row.render(width)[0]


def _table_lines(data: dict, width: int, budget: int) -> list[str]:
    """The repo table the `repos` pane draws under its heading, at most *budget* lines.

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
    repo rows first, then the `…(+N more)` line that admits what was dropped, then piece
    rows — so a short pane loses DETAIL rather than losing a repo. That ordering is
    `statusline._repo_rows`' own (`wt_budget` there), kept because the two tables are
    meant to read alike.

    Piece rows come from ``data["worktrees"]``, which `gather.scan` populates only when
    the workspace resolves to exactly one repo — `statusline._detail_worktrees`' rule
    verbatim. Every OTHER repo's pieces get a `⑂N` badge on the repo's own row instead,
    from `worktree_count`; the badge means "there is more you cannot see here", so it is
    dropped whenever every piece already has its own row.
    """
    from .. import statusline as sl

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

    capped = len(keys) > budget
    # `budget - 1` and no `max(1, …)` underneath it: the overflow line is reserved OUT of
    # the budget rather than appended on top of it and trimmed off at the end. With a
    # one-row budget the trimmed version showed a single repo row and dropped the
    # `…(+N more)` line — a pane claiming that one clean repo is the whole plane, which is
    # the false-clean reading this module refuses everywhere else. A budget of exactly one
    # therefore spends it on the note, which is the honest half of the pair: "there is
    # more here than fits" outranks "here is an arbitrary one of them".
    show = (sl._pick_rows(keys, budget - 1, cur_repo, by_key, by_key)
            if capped else keys)

    pieces = list(data.get("worktrees") or [])
    shown_pieces: dict = {}
    for p in pieces:
        shown_pieces[p.get("repo")] = shown_pieces.get(p.get("repo"), 0) + 1

    # Rows left for piece detail once every repo has its own row and, if capped, the
    # overflow line has been reserved. `statusline._repo_rows` spends its budget in
    # exactly this order and for exactly this reason: a repo must never lose its row to
    # another repo's pieces.
    room = budget - len(show) - (1 if capped else 0)
    kids = pieces[:max(0, room)] if len(show) == 1 else []

    lines: list[str] = []
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
        lines.append(_table_row(f"  {sl._DIM}{tree}{sl._R}",
                                f"{emph}{palette[r['name']]}{r['name']}{sl._R}{badge}",
                                r, width))

    if capped:
        hidden = [k for k in keys if k not in set(show)]
        quiet = not any(_needs_attention(by_key[k]) for k in hidden)
        note = ", all clean" if quiet else ""
        lines.append(tui.truncate(
            f"  {sl._DIM}…(+{len(hidden)} more{note}){sl._R}", width))

    for j, p in enumerate(kids):
        mark = sl._TREE_WT if j == len(kids) - 1 else sl._TREE_MID
        emph = f"{sl._BOLD}{sl._UNDER}" if p.get("current") else ""
        # `charter worktree add <repo> <piece>` names the branch after the piece, so by
        # default these two columns print the same word twice — empty the branch cell
        # when they agree, exactly as `statusline._repo_rows` does, so the column comes
        # to mean "this piece is NOT on the branch you would assume". The markers still
        # render: dirty and ahead/behind are true of the tree whatever its branch.
        wb = p.get("branch") or "?"
        lines.append(_table_row(f"  {sl._DIM}{sl._TREE_PIPE}{mark}{sl._R}",
                                f"{emph}{sl._DIM}{p['name']}{sl._R}", p, width,
                                branch_override="" if wb == p.get("name") else None))
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

    **No `contain.one_line` over *ws*, and the reason is `tui.sanitize`, not the name
    rungs.** It would be comfortable to say every rung of `state.workspace_for` checks its
    answer against `instance.WORKSPACE_NAME_RE` (`^[A-Za-z0-9][A-Za-z0-9._-]*$`) — rung 0
    does, through `valid_name` — but the LAST rung does not: it is `workspace.resolve()`,
    which hands back `$CHARTER_WORKSPACE` stripped and otherwise untouched. Measured:
    with `CHARTER_WORKSPACE='ev\\nil\\x1b[31m;rm -rf /'` and no session pointer and no
    recorded launch workspace, rung 0 rejects it, rungs 1 and 2 have nothing, and rung 3
    returns that exact string — which arrives here verbatim.

    What contains it is the `tui.truncate` call below, which runs `tui.sanitize` first:
    the newline is not charter's markup, so it is replaced and the pane still gets ONE
    line; the SGR is passed through as colour, which costs zero columns and cannot move a
    cursor. That is the guarantee this module leans on everywhere — a value nobody has
    thought about yet is contained at the point it is drawn, not by a rung nobody
    re-checked. `_top` interpolates the same value on the same terms, and has since
    before #515. Naming the wrong reason is how a guard gets deleted later for being
    redundant against a property that was never true.

    What is NOT free either way is the WIDTH: the name is arbitrary length, so the line
    is measured through `tui.truncate` like every other line in this module.
    """
    from .. import statusline as sl

    return [tui.truncate(
        f"  {sl._DIM}no clones in{sl._R} {ws}{sl._DIM} · charter clone <repo> -w "
        f"{ws}{sl._R}", width)]


def _too_narrow_lines(width: int) -> list[str]:
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

    Bounded through `tui.truncate` like every other line here — this line is drawn at
    widths below 95 by definition, and `⋯` is East-Asian *Ambiguous*.
    """
    from .. import statusline as sl

    return [tui.truncate(
        f"  {sl._DIM}⋯ too narrow for the repo table — {sl._LEFT_W} columns "
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

    **The `+ 1` is the pane's own heading, and it is no longer the attention row.** This
    used to add that row, because `bottom` drew it and the table into one pane; they are
    two panes now (`bottom` is the one-row strip it was before #488, and
    `layout.SLOT_SIZE` states its height as the constant it is). What the row buys here
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
    """
    from . import gather
    cap = _table_cap(fid, pane_cols)
    if cap <= 0:
        return 0
    rows = gather.row_count(fid)
    return 1 + min(rows, cap) if rows else 0


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

    `_persona_chip_cells` is already ordered (the active persona first, then anything
    carrying a health mark), so what survives is the top of an order rather than an
    arbitrary handful, and a row that was ALREADY standing for hidden personas folds its
    count into the new row rather than losing it.
    """
    from .. import statusline as sl
    if keep <= 0 or len(cells) <= keep:
        return cells
    shown = cells[:max(0, keep - 1)]
    hidden = _persona_total(cells[len(shown):])
    return [*shown,
            sl.PersonaChip(None, f"{sl._DIM}{_inset()}…(+{hidden} more){sl._R}", "",
                           hidden)]


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
    badge_w = min(max((tui.width(c.badges) for c in cells), default=0),
                  max(0, width - _NAME_MIN_W))
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
    if not items or budget < 2:
        return []
    # `todo_count` is the UNCLIPPED total (`gather._MAX_TODOS` bounds only the list), so a
    # cache written by an older charter — which has the items and no count — falls back to
    # what it can actually see rather than reporting zero hidden todos it cannot name.
    raw = data.get("todo_count")
    total = raw if isinstance(raw, int) and raw >= len(items) else len(items)
    room = budget - 1                       # the heading is not negotiable
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
        return [tui.truncate(f"{sl._DIM}no personas{sl._R}", width)]
    keep = height - 1                       # the heading takes a row off the list
    if terse:
        keep = min(keep, _TERSE_ROWS)
    cells = _cap_personas(cells, keep)
    rows = _persona_rows(cells, width)
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
    """
    from . import gather
    budget = min(budget, _TERSE_ROWS if terse else _MAX_TODO_LINES)
    if budget <= 0:
        return []
    return _todo_rows(gather.read(fid), width, budget)


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
    w, h = _width(), _height()
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
        parts.append(f"{sl._YELLOW}{spinner_frame()} {running} running{sl._R}")
    if stalled:
        parts.append(f"{sl._DIM}⋯ {stalled} stalled{sl._R}")
    return " ".join(parts)


def _fit_fields(priority: list[tuple[str, str]], width: int,
                limit: int | None = None) -> set[str]:
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
    """
    sep_w = tui.width(" · ")
    budget = width
    keep: set[str] = set()
    for name, text in priority:
        if not text:
            continue
        if limit is not None and len(keep) >= limit:
            break
        need = tui.width(text) + (sep_w if keep else 0)
        if need > budget and keep:
            continue          # doesn't fit and something already does — drop it whole
        keep.add(name)
        budget -= need
    return keep


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

    **Four candidate fields on that row, each shown WHOLE or dropped WHOLE — never one
    assembled string cut once from the right.** The attention row is one row and its
    WIDTH is the frame's own, not a narrow side panel's, so ordinarily every field fits
    comfortably and this never has to choose. But `layout.py`'s own module docstring
    records a REAL tmux 3.7c resize that transiently starves a pane before the corrective
    hook snaps it back — not hypothetical, reachable by an ordinary window resize at any
    moment. A naive single `tui.truncate` over the joined line would risk Task 3's own
    Critical on perfectly ordinary data: an alert exists specifically to carry the command
    that fixes it, and slicing into that command mid-word reads as "no problem here" — the
    exact false-clean failure this plan's Global Constraints call out by name.

    Priority order, highest first: the one alert (`_alerts()`'s own top pick — an
    actionable control-plane problem, carrying its own fix); the in-flight spinner
    (:func:`_inflight_field` — work happening RIGHT NOW, and the only thing on this row
    that will be different in a second); `_session_news` (this session's own activity —
    silent unless it already has something to say, so its mere presence is the signal);
    the todo count (persistent state, not urgent); the configured hotkey hint (the one
    thing always rediscoverable another way, so it is first to give up its columns). Once
    decided, the survivors are RE-JOINED in the original reading order (todo, alert,
    inflight, news, hotkey) — priority governs only who is dropped when the pane is
    starved, not how a healthy pane reads.

    **`_session_news` is asked to leave its own in-flight count out** (`inflight=False`).
    Both would otherwise draw the same fact from the same tracker on the same row —
    `⚡ 2 · ⠙ 2 running` — and the duplicate would be the one thing on the row a reader
    could not explain. The status line keeps `⚡ 2` unchanged: it repaints once per turn,
    where a spinner is a still picture of an arbitrary frame.

    **At `terse` exactly one field survives** — the highest-priority one that has anything
    to say. On a quiet plane that is the todo count, so the row is never blank; the moment
    something is wrong or something is running, the row is that instead. `_fit_fields`
    does it through the same priority order it already uses for width, so "less" and
    "too narrow" cannot disagree about what matters.

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
    w = _width()

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

    # Decide who survives, highest priority first (see this function's own docstring
    # above for why); `_fit_fields` does the actual budgeting so it can be tested in
    # isolation.
    keep = _fit_fields(
        [("alert", alert_text), ("inflight", inflight_text), ("news", news_text),
         ("todo", todo_text), ("hotkey", hotkey_text)], w,
        limit=1 if verbosity(fid) == "terse" else None)

    # Re-assembled in the original reading order, not priority order — priority decided
    # only who was cut.
    fields = {"alert": alert_text, "inflight": inflight_text, "news": news_text,
              "todo": todo_text, "hotkey": hotkey_text}
    parts = [fields[n] for n in ("todo", "alert", "inflight", "news", "hotkey")
             if n in keep]
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
    edges in. The `- 1` is the heading and nothing else — the attention row is another
    pane's now, and a budget still reserving a row for THAT would lose the lowest-ranked
    repo row to nothing.

    A pane that is shorter anyway — a transient mid-resize size, or a window with no rows
    to spare — costs the table its lowest-priority rows through `_pick_rows`' ranking,
    and the `…(+N more)` line is reserved out of the budget rather than trimmed off the
    end (see :func:`_table_lines`), so a starved pane never claims one clean repo is the
    whole plane.

    **Three states, three different sentences, because they are three different claims.**
    A gather that has not run yet is `_unknown_lines`; a gather that ran and found
    nothing is :func:`_empty_lines`; anything else is the table. #512 is the cost of
    drawing the first two the same, and #515 is the cost of drawing the second one as an
    empty rectangle — which is what an unmodified `_table_lines` returning `[]` would now
    be, since this pane no longer has an attention row above it to make its emptiness
    read as "nothing to add".

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
    """
    w = _width()
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
        return "\n".join(_too_narrow_lines(w))
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
        return "\n".join(_unknown_lines(w))
    repos = data.get("repos") or []
    if not repos:
        # Gathered, and there is nothing in it. Said out loud rather than left as an
        # empty bordered rectangle — see :func:`_empty_lines`. No heading: `▪ repos 0`
        # above "no clones in demo" is the same fact twice in a two-row pane.
        return "\n".join(_empty_lines(_frame_workspace(fid), w))
    # The heading takes the first row and the table spends what is left. Asked for fewer
    # ROWS rather than sliced afterwards, so what survives at `terse` (or in a pane a
    # resize starved) is still `_pick_rows`' ranked subset — the repo you are standing
    # in, the ones with something on them — rather than whichever happened to come
    # first, the same discipline the `terse` chip list in `_right` keeps.
    budget = min(_height() - 1, cap)
    return "\n".join([_sidebar_head("repos", len(repos), w),
                      *(_table_lines(data, w, budget) if budget > 0 else [])])


#: Every slot charter can draw. `panel.run` refuses a name that is not in here rather
#: than painting an empty pane, because an empty pane reads as a broken frame.
SLOTS = {"top": _top, "bottom": _bottom, "repos": _repos, "right": _right}


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
    return _builtins.supplies(cid)


def render(slot: str, fid: str) -> str:
    """Draw *slot*, or a one-line explanation of why it could not be drawn.

    Never raises. A panel that dies leaves a hole in the frame, which is worse than a
    line saying what went wrong — the same promise `statusline.render` already makes and
    documents.
    """
    fn = SLOTS.get(slot)
    if fn is None:
        return tui.truncate(f" charter: unknown slot {slot}", _width())
    try:
        return fn(fid)
    except Exception as e:
        return tui.truncate(f" charter: {slot} unavailable ({type(e).__name__})", _width())
