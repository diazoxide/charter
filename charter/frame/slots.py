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
#: repo table `bottom` keeps there. `top` has no equivalent — it is one row at every
#: density (`layout.SLOT_SIZE`), so what "less" means for it is FIELDS, not rows.
#:
#: `bottom` is BOTH now (#488): at `terse` its attention row keeps one field and its
#: table keeps this many rows, because a density that buys back rows has to buy them
#: from the slot that actually has rows to give.
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
    the density menu and by nothing else) first, and `[frame] density` from charter.toml
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
    cached: a panel repaints on a version bump, and the density menu bumps the version
    precisely so that this is re-read.
    """
    from .. import config, instance
    from . import state
    override = instance.density_level(state.density(fid))
    return instance.verbosity_for(override or config.FRAME["density"])


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
    """
    from .. import __version__, statusline, workspace
    from . import state
    ws = workspace.resolve()
    src = workspace.source()
    pin = "*" if src == "$CHARTER_WORKSPACE" else ""
    persona = statusline._persona_line() or ""
    # Two file reads on a slot that repaints only on a version bump (`top` is not in
    # `ANIMATED`), and nothing is read at all for a frame with no recorded session —
    # which is every frame whose harness is not Claude Code.
    gauge = " ".join(statusline.recorded_context_gauge(
        state.harness_session(fid) or ""))
    left = f" ⬢ {ws}{pin}"
    tail = (persona if verbosity(fid) == "terse"
            else f"{persona}  charter {__version__}{statusline._dev_chip()} ")
    right = f"{gauge}  {tail}" if gauge else tail
    return tui.truncate(f"{left}  {right}", _width())


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
    """The repo table `bottom` draws under its attention row, at most *budget* lines.

    **This is #488's actual answer**: the frame used to show LESS of the plane's repo
    state than the status line it suppresses (#386), because the only slot drawing repos
    was a 22-column sidebar whose own docstring conceded that `_NAME_W` (32) and
    `_BRANCH_W` (34) alone exceed the whole pane. `bottom` is the frame's full-width
    slot, so the table goes here and is drawn at the widths it was designed for.

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

    *budget* is the pane's real height minus the attention row, measured by
    :func:`_height` before anything is composed. The budget is spent in priority order —
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
    # **Ordinary, not exotic — an 80-column terminal lands here.** `bottom` is split
    # BEFORE `right` (the slot order IS the geometry — `instance.FRAME_FIELDS`), so its
    # width is the whole WINDOW's; and `[frame] min-cols` (100) gates `right` and `top`
    # only — `layout.visible_slots` keeps `bottom` all the way down to `min_cols // 2`.
    # So every frame between 50 and `_LEFT_W - 1` (94) columns draws the attention row
    # and no table, which is why :func:`_table_cap` — not this function — is what the
    # LAUNCHER asks, and why it asks with the same width. The attention row above is
    # unaffected either way; it does its own per-field budgeting (`_fit_fields`).
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


def _table_cap(fid: str, width: int) -> int:
    """The most rows of repo table `_bottom` will draw in a *width*-column pane —
    **every reason the renderer draws fewer rows than there is content**, in one place.

    Three of them, and until #500 only the first was written down where the LAUNCHER
    could read it:

    * **Too narrow is no table at all.** Below `statusline._LEFT_W` (95)
      :func:`_table_lines` refuses outright rather than trimming a row into a false-clean
      `charter  main`, and that is not an exotic width: `layout.visible_slots` keeps
      `bottom` down to `min_cols // 2` (50), so every ordinary 80-column terminal is here.
    * **`terse` keeps :data:`_TERSE_ROWS`.** A density that buys the harness back its
      rows has to buy them from the slot that has rows to give, so `minimal` asks for a
      SHORTER pane, not the same pane with more of it blank.
    * **`statusline._MAX_REPO_LINES` bounds the rest**, the same total-row budget the
      wide table keeps (repo rows plus the `…(+N more)` line, not repo COUNT — see that
      constant's own comment), so a workspace with forty clones gets fourteen rows of
      table under its attention row rather than forty.

    **Both sides of "how tall is `bottom`?" call this, and that is the whole point.**
    :func:`bottom_rows_wanted` asks it with the WINDOW's width to size the pane;
    :func:`_bottom` asks it with the pane's own measured width to bound what it draws
    into it. A cap applied on one side only is exactly the defect #500 fixes: the sizer
    used to answer from the repo count alone, so an 80-column frame with six repos got a
    seven-row pane to draw one line in, and `minimal` on a wide terminal got an
    eleven-row pane to draw five — rows taken from the harness and left blank, and
    re-taken by `cmd_resize` on every subsequent resize.

    Takes *width* rather than measuring it: the renderer has a pane to measure and the
    launcher has only the window it is about to split. Same number by construction —
    `bottom` is split BEFORE `right` (`instance.FRAME_FIELDS`' order is the geometry), so
    the pane's width IS the window's.
    """
    from .. import statusline as sl
    if width < sl._LEFT_W:
        return 0
    cap = sl._MAX_REPO_LINES
    if verbosity(fid) == "terse":
        cap = min(cap, _TERSE_ROWS)
    return cap


def bottom_rows_wanted(fid: str, *, cols: int) -> int:
    """How many rows `_bottom` would fill for this frame in a *cols*-column window.

    **One answer to "how tall is `bottom`", read by both sides of the question.** The
    renderer spends the pane it was given (:func:`_table_lines`' *budget*); the LAUNCHER
    has to decide how tall to make that pane before any panel exists, and the
    `window-resized` recompute has to decide again with the window's new size. If those
    two disagreed, a frame would come up with a pane taller than its content (blank rows
    the harness could have had) or shorter (a table cut off with nothing saying so) —
    and both were shipped by #488, because this asked the repo count and nothing else
    while the renderer also asked the width and the density. Pinned by a test that
    renders `_bottom` into a pane of exactly this height, at exactly this width and at
    every density, and counts the lines that come back.

    *cols* is required, and keyword-only so no caller can pass a row count by mistake.
    A frame narrower than `statusline._LEFT_W` draws no table at all, so it wants the
    one-row strip `bottom` always was — see :func:`_table_cap`, which is where every
    reason the renderer draws fewer rows than there is content now lives.

    `+ 1` is the attention row — the alert, the spinner, this session's news, the todo
    count and the hotkey hint — which `_bottom` always draws and which #488 is explicit
    that the table joins rather than evicts.

    `gather.row_count` is what makes this affordable at launch: it answers from the
    frame's cache when there is one and from a plain directory listing when there is not,
    and never runs a git sweep. See its own docstring. It is asked SECOND, after the
    width test that can answer zero, so a narrow frame does not pay for a count it is
    about to discard.
    """
    from . import gather
    cap = _table_cap(fid, cols)
    return 1 + (min(gather.row_count(fid), cap) if cap > 0 else 0)


def _right(fid: str) -> str:
    """Persona chips — memory badges, in-flight badges and vault dots included,
    because `statusline._persona_chips` already builds one chip as all four
    combined (`◆ name` + vault dot + memory badge + health mark + in-flight
    badge) and this calls it rather than reassembling the same facts out of
    `_mem_badge`/`_inflight_badge`/`_inflight_by_persona`/`_vault_dot`/
    `_mem_count` itself. A fix to any one of those five lands here the moment it
    lands in the status line's right column — nothing in this module could drift
    from it, because nothing in this module repeats what it does.

    No per-turn session id to hand it: unlike `statusline.render`, a frame panel
    never receives Claude Code's JSON payload on stdin (see `gather.py`'s own
    module docstring for the identical point about the gather side).
    `_persona_chips()`'s own default (`session=None`) is exactly right here —
    its ephemeral-memory count falls back through `charter.session.current`,
    which reads `$CHARTER_SESSION_ID`/`$CLAUDE_CODE_SESSION_ID` out of the
    environment a launched panel process inherits whole from the harness (the
    same env var `notify.plane_changed` already depends on being set there).

    No guard of its own around the call: `_persona_chips` already swallows
    every failure internally and answers `[]` (its own docstring's "never
    breaks the status line"), and `render`'s caller-side `try/except` covers
    whatever gets past that — the same trust `_top`/`_bottom` already place in
    it rather than each re-wrapping their own calls into `statusline.py`.

    **At `terse` this keeps :data:`_TERSE_ROWS` chips and says so.** A slice, not a
    narrower chip: `_persona_chips` builds one chip as `◆ name` plus its vault dot,
    memory badge, health mark and in-flight badge all together, and dropping any of
    those parts would mean reassembling the chip here out of the five helpers this
    function exists specifically not to duplicate. So the panel drops whole personas
    and adds `_left`'s own `…(+N more)` line, which is the honest way to show fewer of
    a list — `_persona_chips` is already ordered, so what survives is the top of an
    order rather than an arbitrary handful.
    """
    from .. import statusline as sl

    w = _width()
    chips = sl._persona_chips()
    if not chips:
        return tui.truncate(f"{sl._DIM}no personas{sl._R}", w)
    if verbosity(fid) == "terse" and len(chips) > _TERSE_ROWS:
        hidden = len(chips) - (_TERSE_ROWS - 1)
        chips = [*chips[:_TERSE_ROWS - 1], f"{sl._DIM}…(+{hidden} more){sl._R}"]
    return "\n".join(tui.truncate(c, w) for c in chips)


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
    """What still wants attention, and how to act on it.

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
    assembled string cut once from the right.** The attention row is one row whatever
    this pane's height is, and its WIDTH is the frame's own, not a narrow side panel's,
    so ordinarily every field fits comfortably and this never has to choose. But
    `layout.py`'s own module docstring records a REAL tmux 3.7c resize that transiently
    starves a pane before the corrective hook snaps it back — not hypothetical, reachable
    by an ordinary window resize at any moment. A naive single `tui.truncate` over the
    joined line would risk Task 3's own Critical on perfectly ordinary data: an alert
    exists specifically to carry the command that fixes it, and slicing into that command
    mid-word reads as "no problem here" — the exact false-clean failure this plan's
    Global Constraints call out by name.

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
    used to hardcode `F2 menu` — so a plane on `hotkey = "F1"` had its own panel telling
    every operator the wrong key, on every repaint, forever. `config.FRAME` is the
    resolved value `commands_frame.conf_text` binds, so there is one source for what the
    panel says and what the frame actually does.

    **Under it, the wide repo table — #488.** The frame suppresses the status line
    (#386) and until now showed LESS of the plane's repo state than the line it replaced:
    the only slot drawing repos was a 22-column `left` sidebar recomposing a table
    designed for 66. `bottom` is the frame's full-width slot, so the table lives here
    now, at `statusline.py`'s own column widths, and `left` is retired rather than left
    drawing a lesser copy of its neighbour.

    **The attention row is not evicted by it — it is the first line, always.** The alert
    and the command that fixes it outrank any repo row; the table gets what is left of
    the pane after it (`_table_lines`' own *budget*). On a plane with no clones there is
    no table at all and this is exactly the one-row strip it always was.

    **The pane is measured, not assumed.** :func:`_height` reads this pane's own tty the
    way :func:`_width` reads its width; :func:`bottom_rows_wanted` is what told the
    LAUNCHER how tall to make it, and both go through :func:`_table_cap` with the same
    width and the same density — so on an untouched frame the two agree exactly and no
    row is either blank or cut, at every width the frame is drawn at and at every level
    the density menu offers. A pane that is shorter anyway — a transient mid-resize
    size, or a window with no rows to spare — costs the table its lowest-priority rows
    through `_pick_rows`' ranking, never the attention row.

    **One `gather.read`, and no repo directory touched.** #387 pinned a panel's idle tick
    at exactly one `stat` and `bottom` is the ONE animated slot (:data:`ANIMATED`), so a
    table that walked a directory per row would pay that back fourteen times over at five
    repaints a second. Everything the table draws comes out of the cache; see
    :func:`_table_row` for the one column that costs (presence) and is therefore absent.
    """
    from .. import config, session as _session, statusline as sl, workspace
    from . import state, tmuxctl
    ws = workspace.resolve()
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
    # server-wide in tmux, and the menu's one entry is one the operator's own prefix
    # already does). A row still printing `F2 menu` there would be telling every
    # operator about a key that does nothing, on every repaint — the same defect the
    # hardcoded `F2` was, reached through the other server instead of the wrong config.
    hotkey_text = ("" if tmuxctl.is_operator_socket(state.frame_server(fid))
                   else f"{config.FRAME['hotkey']} menu")

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
    lines = [tui.truncate(" · ".join(parts), w)]

    # The table gets what is left of the pane below the attention row, bounded by
    # `_table_cap` — the SAME call `bottom_rows_wanted` made to size this pane, with this
    # pane's own measured width instead of the window's. Asked for fewer ROWS rather than
    # sliced afterwards, so what survives at `terse` is still `_pick_rows`' ranked subset
    # (the repo you are standing in, the ones with something on them) rather than
    # whichever happened to come first — the same discipline the `terse` chip list in
    # `_right` keeps.
    budget = min(_height() - len(lines), _table_cap(fid, w))
    if budget > 0:
        from . import gather
        lines.extend(_table_lines(gather.read(fid), w, budget))
    return "\n".join(lines)


#: Every slot charter can draw. `panel.run` refuses a name that is not in here rather
#: than painting an empty pane, because an empty pane reads as a broken frame.
SLOTS = {"top": _top, "bottom": _bottom, "right": _right}


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
    return sorted({s for s in configured if s not in SLOTS})


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
