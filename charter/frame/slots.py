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

#: How many rows a full-height panel (`left`, `right`) draws at `terse`. The horizontal
#: strips have no equivalent — `top` and `bottom` are one row at every density
#: (`layout.SLOT_SIZE`), so what "less" means for them is FIELDS, not rows.
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
    own, explicit assignment (see `_bottom`'s docstring), not `top`'s. Calling it here
    too would either duplicate `bottom` byte-for-byte or (worse, since a panel's `sid` may
    differ in nothing from `bottom`'s) read as two different panes agreeing by
    coincidence. Per the task brief: "a gauge that silently reads zero is worse than no
    gauge" — so this is left out, not stubbed in, until #386 (which owns the suppression
    and recording question this finding feeds) decides a panel should have its own
    persisted usage snapshot to read. Pinned by `tests.test_frame_slots.TopRenderer`.

    **At `terse` the version goes, and nothing else does.** `top` answers "where am I and
    who am I being", and of the three things on it the charter version is the only one
    that reads the same on every frame on this machine all day — it is a fact about the
    install, not about where you are standing. The workspace and the persona ARE the
    answer, so a density that dropped either would leave a row not earning its line.
    """
    from .. import __version__, statusline, workspace
    ws = workspace.resolve()
    src = workspace.source()
    pin = "*" if src == "$CHARTER_WORKSPACE" else ""
    persona = statusline._persona_line() or ""
    left = f" ⬢ {ws}{pin}"
    right = persona if verbosity(fid) == "terse" else f"{persona}  charter {__version__} "
    return tui.truncate(f"{left}  {right}", _width())


class _RowKey:
    """A directory-shaped key for one cache row: `_pick_rows` wants something with
    a `.name` it can compare against `cur_repo` and use as a `states`/`gl` dict
    key (ordinarily a `Path`) — this is that, minus the filesystem, since nothing
    in :func:`_left` touches one. Identity-hashed on purpose (no `__eq__`
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
    change. Shared by :func:`_left`'s own overflow line so the two claims (which
    repos are hidden, and whether that is safe to say) cannot drift apart."""
    return bool(r.get("dirty") or r.get("ahead") or r.get("behind")
               or r.get("ci") in ("failed", "running") or r.get("change"))


def _row(lead: str, name_markup: str, r: dict, width: int, badge_n: int = 0) -> str:
    """One row — *lead* glyphs, a name, then branch+markers, CI, an open
    change, and (last, lowest priority) a `⑂N` piece-count badge — each field
    sized against its OWN budget rather than the assembled line trimmed once
    from the right at the end.

    **Fix round 1, finding 1.** The first version of this function built the
    whole line (`f"{name} {branch}{marks} {ci} {change}"`) and let a single
    `tui.truncate` at the call site cut whatever didn't fit off the end. On
    this project's own branches — `worktree-recall-since`,
    `browser-session-scope`, `global-shim-refresh`: 21-28 characters is the
    NORM here, not an edge case — `name + " " + branch` alone already fills a
    22-column pane, so the cut always landed on the markers, the CI glyph and
    the change sigil. A dirty, CI-failing, unpushed repo rendered as
    `"charter worktree-reca…"` — a FALSE CLEAN reading, worse than not having
    the panel at all for a panel that exists to surface exactly that.

    `statusline._branch_cell_for` already keeps the right rule for the wide
    table (`_BRANCH_W - width(marks)`, so the branch shrinks and the markers
    never do) — this generalises it to a whole LINE's budget rather than one
    fixed-width cell, and extends the same "shrink the least important field
    first" idea to CI and an open change: they are appended whole or not at
    all (never character-truncated — half a glyph reads as broken, an absent
    one reads honestly as "no room to say"), in priority order behind the
    markers, and the branch — read last on this line, and the one field
    genuinely fine to abbreviate — is what actually gives up its columns.

    `_markers` and `_CI_MARK` are `statusline.py`'s own — called, not
    reimplemented, so a fix to what a marker or a CI glyph means lands here
    the moment it lands in the wide table.
    """
    from .. import statusline as sl

    _plain, marks, _dirty = sl._markers(r)
    marks_w = tui.width(marks)

    ci = r.get("ci")
    ci_text = ""
    if ci:
        c, glyph, _label = sl._CI_MARK.get(ci, (sl._DIM, "?", ci))
        ci_text = f" {c}{glyph}{sl._R}"
    ci_w = tui.width(ci_text)

    change = r.get("change")
    change_text = ""
    if change:
        sigil = r.get("sigil") or "!"
        change_text = f" {sl._GREEN}{sigil}{change}{sl._R}"
    change_w = tui.width(change_text)

    badge_text = f" {sl._DIM}⑂{badge_n}{sl._R}" if badge_n else ""
    badge_w = tui.width(badge_text)

    branch = r.get("branch") or "?"
    lead_w = tui.width(lead)

    budget = max(0, width - lead_w)
    name_w = min(tui.width(name_markup), max(1, budget // 2))
    name_shown = tui.truncate(name_markup, name_w)
    budget -= tui.width(name_shown)

    sep = " " if budget > 0 else ""
    budget -= tui.width(sep)

    # What's left for the branch, once the markers glued to it are reserved.
    remaining = budget - marks_w
    # CI, an open change, and the piece badge are included only while there is
    # still at least 1 column left over for the branch itself after they are —
    # dropped whole, priority order, rather than let any of them crowd the
    # branch out entirely.
    show_ci = bool(ci_text) and (remaining - ci_w) >= 1
    tail_w = ci_w if show_ci else 0
    show_change = bool(change_text) and (remaining - tail_w - change_w) >= 1
    if show_change:
        tail_w += change_w
    show_badge = bool(badge_text) and (remaining - tail_w - badge_w) >= 1
    if show_badge:
        tail_w += badge_w

    branch_w = max(1, remaining - tail_w)
    branch_shown = tui.truncate(f"{sl._DIM}{branch}{sl._R}", branch_w)

    line = f"{lead}{name_shown}{sep}{branch_shown}{marks}"
    if show_ci:
        line += ci_text
    if show_change:
        line += change_text
    if show_badge:
        line += badge_text
    # Safety net, not the fix: correct budgeting above should already leave
    # this a no-op (`tui.truncate`'s own fast path returns unchanged input
    # unmodified) — kept only so a rounding slip degrades to a trimmed line
    # rather than an over-width one, the same belt-and-braces `panel.py`
    # already keeps around its own measurements.
    return tui.truncate(line, width)


def _repo_line(r: dict, color: str, width: int, badge_n: int = 0) -> str:
    """One repo, one line: name (bold+underline if it is where you are
    standing, coloured by the same per-position palette `_repo_rows` cycles),
    then :func:`_row`'s shared branch/markers/CI/change/badge composition.

    *badge_n* is the repo's `⑂N` — the pieces NOT already shown as their own
    rows below it (see :func:`_left`'s own docstring: `0` when every piece
    already has a row, or when the repo has none)."""
    from .. import statusline as sl
    emph = f"{sl._BOLD}{sl._UNDER}" if r.get("current") else ""
    name_markup = f"{emph}{color}{r['name']}{sl._R}"
    return _row("", name_markup, r, width, badge_n=badge_n)


def _piece_line(p: dict, width: int) -> str:
    """One piece (worktree), indented under its repo — same composition as
    :func:`_repo_line` minus the palette colour and the bold/underline
    emphasis, which belong to a REPO's row, not one of its pieces."""
    from .. import statusline as sl
    lead = f"{sl._DIM}{sl._TREE_WT}{sl._R}"
    return _row(lead, p["name"], p, width)


def _left(fid: str) -> str:
    """Repo rows, narrow: the same per-tree facts `_tree_cells` draws in the wide
    status-line table (name, branch, dirty/ahead/behind markers, CI, open
    change), recomposed for a 22-column pane rather than reusing the `tui.Node`s
    built for the wide one — `_NAME_W` (32) and `_BRANCH_W` (34) alone already
    exceed this whole pane's width, which is exactly why "share the gather, not
    the composition" is this plan's own rule.

    Reads ONLY `gather.read(fid)` — the cache Task 1 built and Task 2 keeps
    refreshed — never a repo directory listing, a `git status`, or a
    `glstate.read_for` of its own; every field a row needs (`name`, `branch`,
    `dirty`, `tracked_dirty`, `ahead`, `behind`, `ci`, `change`, `sigil`,
    `current`, `worktree_count`) is already sitting in that one gather.
    `_pick_rows` is called rather than reinvented for the same reason: it
    already carries the lesson `statusline.py` paid for in production (an
    unranked slice of 18 clones showed thirteen clean repos on `main` and hid
    the one dirty repo you were actually standing in) — `_pick_rows` wants
    directory-shaped keys (`.name`, hashable), so each cache row is wrapped in
    a bare `_RowKey` rather than a `Path`: nothing here touches a filesystem,
    and a `Path` would imply one exists to touch (and, unlike a `Path`, two
    `_RowKey`s never compare equal just because their names happen to match —
    see its own docstring).

    `left` is a full-height pane with rows to fill, unlike `top`/`bottom`'s fixed
    single row — but nothing in this module measures how many it actually has
    (`_width` measures COLUMNS; there is no equivalent asked for here). The
    budget handed to `_pick_rows` is `_MAX_REPO_LINES`, the same cap the wide
    table itself uses, reused rather than invented fresh; `panel.py`'s own
    height clamp (`_rows()` there — see its "Height is this module's job"
    section) is what actually protects a short pane from an overflow.

    Piece rows come from `data["worktrees"]`, which `gather.scan` populates
    ONLY when the workspace resolves to exactly one repo — it mirrors
    `statusline._detail_worktrees`'s own single-repo rule verbatim (see that
    function's docstring: spending rows on piece detail is only a good trade
    when there is exactly one repo to spend them on). Every OTHER repo's
    pieces — the ones never turned into rows of their own — still get a `⑂N`
    badge on their repo's own line, from `worktree_count` (fix round 1,
    finding 2, #385: this used to be missing from the cache entirely for a
    multi-repo workspace; `gather.py`'s own module docstring records the
    fix). `_repo_line` is handed `0` whenever every one of a repo's pieces
    already has its own row — the badge means "there is more you cannot see
    here," not "this repo has pieces."

    **At `terse` the budget drops to :data:`_TERSE_ROWS` and piece rows go.** `_pick_rows`
    is asked for fewer rows rather than the result being sliced afterwards, so the rows
    that survive are still the ones worth keeping (the repo you are standing in, the ones
    with something on them) rather than whichever happened to come first — the exact
    lesson `statusline.py` paid for and this function already borrows. The `…(+N more)`
    line and its "all clean" claim come along unchanged, so a terse panel still says how
    much it is not showing, which is what makes showing less honest rather than
    misleading. Pieces are dropped whole: they are DETAIL under a repo that still has its
    own row, so losing them costs no repo its line.
    """
    from .. import statusline as sl
    from . import gather

    data = gather.read(fid)
    repos = data.get("repos") or []
    w = _width()
    if not repos:
        return tui.truncate(f"{sl._DIM}no repos{sl._R}", w)

    color_by_name = {r["name"]: sl._PALETTE[i % len(sl._PALETTE)]
                     for i, r in enumerate(repos)}
    keys = [_RowKey(r["name"]) for r in repos]
    by_key = dict(zip(keys, repos))
    cur_repo = data.get("current_repo")

    # How many of a repo's pieces already have their own row below it
    # (`data["worktrees"]`, single-repo-only — see the docstring above), so
    # the badge can say "there is more" rather than double-count what is
    # already on screen.
    shown_pieces: dict = {}
    for p in (data.get("worktrees") or []):
        shown_pieces[p.get("repo")] = shown_pieces.get(p.get("repo"), 0) + 1

    def _badge_for(r: dict) -> int:
        total = r.get("worktree_count") or 0
        return total if shown_pieces.get(r["name"], 0) < total else 0

    terse = verbosity(fid) == "terse"
    budget = _TERSE_ROWS if terse else sl._MAX_REPO_LINES
    capped = len(keys) > budget
    show = sl._pick_rows(keys, (budget - 1) if capped else budget,
                         cur_repo, by_key, by_key) if capped else keys

    lines = [_repo_line(by_key[k], color_by_name[by_key[k]["name"]], w,
                        badge_n=_badge_for(by_key[k]))
             for k in show]

    if capped:
        shown = set(show)
        hidden = [k for k in keys if k not in shown]
        quiet = not any(_needs_attention(by_key[k]) for k in hidden)
        # Same wording as `_repo_rows`' own overflow line (`statusline.py`) —
        # this used to say bare ", clean", a needless divergence between the
        # two surfaces for the exact same claim.
        note = ", all clean" if quiet else ""
        lines.append(tui.truncate(f"{sl._DIM}…(+{len(hidden)} more{note}){sl._R}", w))

    if not terse:
        for p in (data.get("worktrees") or []):
            lines.append(_piece_line(p, w))

    return "\n".join(lines)


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
    flight.** `inflight` records a dispatch when it STARTS and clears it when it ends (see
    that module's docstring — the completion tally cannot answer this), so "is anything
    running right now" is a question about files on disk rather than about anything
    charter has to keep in memory or poll for. `panel._running` is what decides whether the
    panel repaints often enough for the spinner to move; this function only decides what
    the row says, and it says nothing when there is nothing to say. Empty means the field
    is dropped whole by `_fit_fields`, which is what makes idle completely still: no
    spinner, no zero, no furniture.

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
    records = inflight.live_records()
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

    **Four candidate fields now, not three, each shown WHOLE or dropped WHOLE — never
    one assembled string cut once from the right.** `bottom` is one fixed row
    (`layout.SLOT_SIZE["bottom"] == 1`) but its WIDTH is the frame's own, not a narrow
    side panel's, so ordinarily every field fits comfortably and this never has to choose.
    But `layout.py`'s own module docstring records a REAL tmux 3.7c resize that
    transiently starves a fixed-size pane before the corrective hook snaps it back — not
    hypothetical, reachable by an ordinary window resize at any moment (Task 3's fix
    round 2 pinned the identical shape for `left`). A naive single `tui.truncate` over the
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
    return tui.truncate(" · ".join(parts), w)


#: Every slot charter can draw. `panel.run` refuses a name that is not in here rather
#: than painting an empty pane, because an empty pane reads as a broken frame.
SLOTS = {"top": _top, "bottom": _bottom, "left": _left, "right": _right}


#: Which slots draw something that CHANGES ON ITS OWN, with no version bump and no
#: resize behind it. Exactly the renderers that reach :func:`spinner_frame`, which today
#: is `_bottom` and only `_bottom` (through :func:`_inflight_field`).
#:
#: **This is what scopes the animation's cost to the pane that needs it.** `panel._watch`
#: runs one process per slot, so an unscoped "is work in flight" would repaint all four at
#: `panel.TICK` for the whole length of a dispatch — three of them redrawing byte-identical
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

    Answers empty as of Task 3: `left`/`right` landed beside `top`/`bottom` in
    :data:`SLOTS`, so every slot `instance.FRAME_SLOTS` accepts and
    `layout.SLOT_SIZE` sizes now has a renderer. Kept rather than deleted —
    the next slot this frame grows will pass through here on day one exactly
    the way `left`/`right` did until this task, and three callers need exactly
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
