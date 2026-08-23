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

from .. import tui


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
    """Identity: where you are, pinned or not, and who you are being."""
    from .. import __version__, statusline, workspace
    ws = workspace.resolve()
    src = workspace.source()
    pin = "*" if src == "$CHARTER_WORKSPACE" else ""
    persona = statusline._persona_line() or ""
    left = f" ⬢ {ws}{pin}"
    right = f"{persona}  charter {__version__} "
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


def _repo_line(r: dict, color: str) -> str:
    """One repo, one line: name (bold+underline if it is where you are standing,
    coloured by the same per-position palette `_repo_rows` cycles), branch with
    its dirty/ahead/behind markers, then CI and an open change if there is room.

    `_markers` and `_CI_MARK` are `statusline.py`'s own — called, not
    reimplemented, so a fix to what a marker or a CI glyph means lands here the
    moment it lands in the wide table. Only the CI *glyph* is kept, not
    `_ci_part`'s trailing label (`"✓ passed"`): a narrow pane spends its columns
    on the branch and the markers first, and the label would just be the first
    thing `tui.truncate` throws away.
    """
    from .. import statusline as sl
    emph = f"{sl._BOLD}{sl._UNDER}" if r.get("current") else ""
    name = f"{emph}{color}{r['name']}{sl._R}"
    _plain, marks, _dirty = sl._markers(r)
    branch = f"{sl._DIM}{r.get('branch') or '?'}{sl._R}{marks}"
    line = f"{name} {branch}"
    ci = r.get("ci")
    if ci:
        c, glyph, _label = sl._CI_MARK.get(ci, (sl._DIM, "?", ci))
        line += f" {c}{glyph}{sl._R}"
    change = r.get("change")
    if change:
        sigil = r.get("sigil") or "!"
        line += f" {sl._GREEN}{sigil}{change}{sl._R}"
    return line


def _piece_line(p: dict) -> str:
    """One piece (worktree), indented under its repo — same cells as
    :func:`_repo_line` minus the palette colour and the bold/underline emphasis,
    which belong to a REPO's row, not one of its pieces."""
    from .. import statusline as sl
    _plain, marks, _dirty = sl._markers(p)
    branch = f"{sl._DIM}{p.get('branch') or '?'}{sl._R}{marks}"
    return f"{sl._DIM}{sl._TREE_WT}{sl._R}{p['name']} {branch}"


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
    `current`) is already sitting in that one gather. `_pick_rows` is called
    rather than reinvented for the same reason: it already carries the lesson
    `statusline.py` paid for in production (an unranked slice of 18 clones
    showed thirteen clean repos on `main` and hid the one dirty repo you were
    actually standing in) — `_pick_rows` wants directory-shaped keys (`.name`,
    hashable), so each cache row is wrapped in a bare `_RowKey` rather than a
    `Path`: nothing here touches a filesystem, and a `Path` would imply one
    exists to touch (and, unlike a `Path`, two `_RowKey`s never compare equal
    just because their names happen to match — see its own docstring).

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
    when there is exactly one repo to spend them on). A MULTI-repo workspace's
    per-repo worktree COUNT — the `⑂N` badge and one-line piece-name summary
    `_repo_rows` draws via a fresh, always-live `worktree.dirs_for(active,
    d.name)` call of its own — was never part of Task 1's cache at all, and
    nothing here re-derives it with a live call of its own to patch that over:
    see this task's report.
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

    budget = sl._MAX_REPO_LINES
    capped = len(keys) > budget
    show = sl._pick_rows(keys, (budget - 1) if capped else budget,
                         cur_repo, by_key, by_key) if capped else keys

    lines = [tui.truncate(_repo_line(by_key[k], color_by_name[by_key[k]["name"]]), w)
             for k in show]

    if capped:
        shown = set(show)
        hidden = [k for k in keys if k not in shown]
        quiet = not any(_needs_attention(by_key[k]) for k in hidden)
        note = ", clean" if quiet else ""
        lines.append(tui.truncate(f"{sl._DIM}…(+{len(hidden)} more{note}){sl._R}", w))

    for p in (data.get("worktrees") or []):
        lines.append(tui.truncate(_piece_line(p), w))

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
    """
    from .. import statusline as sl

    w = _width()
    chips = sl._persona_chips()
    if not chips:
        return tui.truncate(f"{sl._DIM}no personas{sl._R}", w)
    return "\n".join(tui.truncate(c, w) for c in chips)


def _bottom(fid: str) -> str:
    """What still wants attention, and how to act on it.

    Only the first alert, deliberately: bottom is a fixed single-row pane, and
    `_alerts()` already returns its entries in priority order, so this picks which one
    survives rather than leaving it to whichever one happens to fit before
    `tui.truncate`'s ellipsis — the same truncation-order reasoning `statusline.py`
    names inline wherever a row has to choose what to drop.

    The hotkey is READ, not spelled out: `[frame] hotkey` is configurable, and this row
    hardcoded `F2 menu` — so a plane on `hotkey = "F1"` had its own panel telling every
    operator the wrong key, on every repaint, forever. `config.FRAME` is the resolved
    value `commands_frame.conf_text` binds, so there is one source for what the panel
    says and what the frame actually does.
    """
    from .. import config, statusline, workspace
    ws = workspace.resolve()
    todos = statusline._todo_count(ws)
    alerts = statusline._alerts(ws)
    parts = [f"{todos} todo" + ("s" if todos != 1 else "")]
    parts.extend(alerts[:1])
    parts.append(f"{config.FRAME['hotkey']} menu")
    return tui.truncate(" · ".join(p for p in parts if p), _width())


#: Every slot charter can draw. `panel.run` refuses a name that is not in here rather
#: than painting an empty pane, because an empty pane reads as a broken frame.
SLOTS = {"top": _top, "bottom": _bottom, "left": _left, "right": _right}


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
