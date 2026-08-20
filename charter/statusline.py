"""Claude Code status-line renderer for the control plane.

Wired via ``.claude/settings.json`` → ``statusLine``. Claude Code pipes a JSON
payload on stdin (session/model/workspace context) and renders this command's
stdout in the footer on every turn.

Contract we honor (see docs/workspaces.md): read *all* of stdin, stay fast, no
network, never raise (fall back to a minimal string), and exit 0. ANSI colour
and multiple lines are supported.

**On subprocesses.** This used to claim "no git subprocess", which was never true
of the module as a whole and misled at least one change into asserting it: dirt
and ahead/behind come from :func:`_run_state`, one ``git status --porcelain
--branch`` per tree, and origin's URL costs another. What *is* true is narrower
and still worth stating, because it is the rule new work has to follow:

* **Branches never fork.** They are read straight from ``.git/HEAD`` — see
  :func:`_branch` — because a branch is needed for every row and a subprocess per
  row is what the cheap read exists to avoid.
* **Nothing new may add one per row.** :func:`_piece_state` and
  :func:`_piece_summary` are file reads for exactly this reason, and
  :func:`charter.worktree.dirs_for` exists so that listing pieces does not become
  a ``git worktree list`` per clone on every turn.

So: a bounded number of subprocesses proportional to *repos*, none proportional
to rows, and none at all for anything that can be read off the filesystem.

This module only *gathers* content (repos, branches, CI, personas) and
declares the layout; all width math lives in :mod:`charter.tui`, whose nodes
guarantee that no emitted line ever exceeds the terminal width — overflow is
truncated with ``…``, never wrapped (a wrap shears every column below it).

Note: Claude Code does **not** pass the session's environment to the status
line, so an ``$CHARTER_WORKSPACE``-pinned session shows the active-file/default here
even though its commands honor the env var. Cosmetic only.
"""

from __future__ import annotations

import argparse
import json
import re
import subprocess
import sys
import time
from pathlib import Path

from . import config, tui

# ANSI — status lines render escape codes.
_R, _DIM, _BOLD, _UNDER = "\033[0m", "\033[2m", "\033[1m", "\033[4m"
_CYAN, _YELLOW, _MAGENTA, _GREEN = "\033[36m", "\033[33m", "\033[35m", "\033[32m"
_BLUE, _RED = "\033[34m", "\033[31m"

# Box-drawing, for an unbroken tree. These are East-Asian *Ambiguous* — a terminal may
# draw them one cell or two — and unlike the frame and the divider, the count here is NOT
# equal across rows: a repo row carries two, a padding row one, the column header none.
# So a terminal that draws them wide moves some rows and not others, which is exactly the
# drift this layout spent a long time chasing. Accepted deliberately, because the header's
# position no longer depends on any glyph (it pads with `_HEAD_PAD` spaces), so the worst
# case is a ragged tree rather than a header that disagrees with its own column.
#
# `_TREE_WT` must stay textually distinct from `_TREE_END`: the "tree keeps going" rewrite
# in `render` searches backwards for the last elbow, and when the worktree row shared the
# marker it rewrote that row instead of the repo above it.
_TREE_MID, _TREE_END, _TREE_PIPE, _TREE_WT = "├─ ", "└─ ", "│  ", "╰─ "
# Bounds the TOTAL rows `_repo_rows` returns — repo rows + each repo's one-line worktree
# summary + the trailing "…(+N more)" — not merely the repo count: a repo with worktrees
# emits 2 lines, so counting repos alone let the footer grow past its budget.
_MAX_REPO_LINES = 14  # keep the footer from growing unbounded

# The SAME budget for the persona column, and for the same reason. The repo side was capped
# and the persona side was not — and because the layout pads the shorter column to match the
# longer, personas alone drove total height: measured at 35 lines for 30 personas, whatever
# the repo count. A status line taller than the conversation is not a status line.
_MAX_PERSONA_LINES = 14

# Fixed column widths for the strict-table repo view (visible chars).
_NAME_W, _BRANCH_W, _CI_W = 32, 34, 12
_MR_W = 6  # fixed MR cell, so a right-hand persona column stays aligned
_GAP = "  "  # between repo-table cells
# One divider on every row, so even a terminal that draws it two cells wide shifts
# every row identically — see `_boxed` for why that is the safe case.
_COL_SEP = f" {_DIM}│{_R} "  # divider between the repos and personas columns
# Both column headers are indented by exactly this, in SPACES, so a header's text starts
# in the same column as the text of the rows beneath it — `* steward` and `├─ iam-service`
# each put their first letter two columns in. Spaces rather than a matching glyph on
# purpose: the point is that no font gets a vote on where a header's text begins. This
# shipped broken both ways — a `◈` on the personas header rendered wide and pushed its
# title a column right of the chips; removing the glyph then left the title two columns
# LEFT of them, because the glyph had been doing the indenting.
# Markers: a column header's, and a persona chip's. All three are East-Asian **Neutral**
# (U+25AA ▪, U+25B8 ▸, U+25AB ▫), which is the whole point — the tables give Neutral
# exactly one cell everywhere, so a header's marker and a chip's bullet can never
# disagree about where the text after them begins. The previous set (`◈` header, `◆`/`○`
# chips) was East-Asian *Ambiguous*: a font drew `◈` two cells and the personas title
# rendered a column right of every name below it.
_MARK_HEAD, _MARK_ACTIVE, _MARK_IDLE = "▪", "▸", "▫"
_HEAD_PAD = f"{_MARK_HEAD} "
# Visible width of the whole left/repo block: "  " + "├─ " + name + gaps + branch + ci + mr.
_LEFT_W = 2 + 3 + _NAME_W + 2 + _BRANCH_W + 2 + _CI_W + 2 + _MR_W
_RIGHT_MIN_W = 36  # a persona column narrower than this is not worth showing
# Render to (COLUMNS − this). The pane gives LESS than `$COLUMNS` advertises, and the
# amount was measured rather than guessed: at 2, a line ending exactly at COLUMNS−2 lost
# its final character to the host's own `…` crop (the brand rendered `⬢ charter 0.10…`),
# so the usable width is COLUMNS−3. 4 leaves one spare column.
#
# This only became loud with the frame. Before it, a single line — the right-aligned
# brand — reached the right edge, so only that line was ever cropped; a right border puts
# EVERY row against the edge, which turned one truncated word into a column of `…` down
# the whole status line.
_SAFETY = 4
#: Marks a body line as a horizontal rule for :func:`_boxed` to draw as ``├───┤``.
#: A sentinel rather than a pre-drawn string because only `_boxed` knows the frame's
#: final width — and because a rule has to survive `tui.truncate` without being cropped
#: into a shorter line. NUL cannot occur in real content, so it can never collide.
_RULE_LINE = "\x00charter-rule\x00"
_BRAND_GAP = 3  # min blank columns between content and the right-aligned brand
# Extra headroom the brand demands beyond `width`. A real session cropped the brand to
# `⬢ charter 0.10…` — impossible from `_with_brand`, which fits-or-drops — so the pane
# gives ~1 column less than COLUMNS advertises. The brand is the one thing that must
# never be half-rendered, so it alone pays this margin.
_BRAND_MARGIN = 2

#: GitLab pipeline status → (colour, glyph, label). Glyphs are single-width so
#: columns stay aligned.
_CI_MARK = {
    "success": (_GREEN, "✓", "passed"),
    "failed": (_RED, "✗", "failed"),
    "running": (_CYAN, "●", "running"),
    "pending": (_YELLOW, "○", "pending"),
    "created": (_YELLOW, "○", "pending"),
    "preparing": (_YELLOW, "○", "pending"),
    "waiting_for_resource": (_YELLOW, "○", "queued"),
    "scheduled": (_YELLOW, "○", "scheduled"),
    "manual": (_DIM, "‖", "manual"),
    "canceled": (_DIM, "⊘", "canceled"),
    "skipped": (_DIM, "»", "skipped"),
}


def _ci_part(status: str | None) -> str:
    """Coloured ``<glyph> <label>`` markup for a pipeline status ('' if none)."""
    if not status:
        return ""
    color, glyph, label = _CI_MARK.get(status, (_DIM, "?", status))
    return f"{color}{glyph} {label}{_R}"

#: Distinct colours cycled per repo (magenta, blue, cyan, green, yellow, then
#: bright variants). Assigned by position so adjacent repos always differ; a
#: repo keeps its colour across renders as long as the workspace's set is stable.
_PALETTE = (
    "\033[35m", "\033[34m", "\033[36m", "\033[32m",
    "\033[33m", "\033[95m", "\033[94m", "\033[92m",
)
def _active(session_id: str | None = None, cwd: str | None = None) -> tuple[str, str]:
    """``(workspace, source)`` for the SESSION, not for this process.

    ``cwd`` is the session's directory out of the payload. It matters because the cwd rung
    outranks every pointer, so reading the hook's own directory there does not merely miss
    a better answer — it overrides the right one. The renderer already trusts the payload
    for :func:`_current`, and the two must agree: marking a row in one workspace under a
    header naming another is worse than either mistake alone.
    """
    from . import workspace
    return (workspace.resolve(session_id=session_id, cwd=cwd),
            workspace.source(session_id=session_id, cwd=cwd))


def _count_workspaces() -> int:
    from . import workspace
    return len(workspace.list_workspaces())


# Cache-trend detection. A single cold turn is normal (you just switched model, compacted, or
# the session is warming up) — only a SUSTAINED cold streak means the prefix is churning every
# turn, which is the expensive failure mode. Same cadence discipline as the memory nudge:
# silent by default, speaks only with evidence, and goes quiet the moment it recovers.
_COLD_BELOW = 50     # a turn whose input is <50% cache-read counts as cold
_COLD_STREAK = 3     # consecutive cold turns before we say anything
_TREND_KEEP = 16     # ring buffer: recent turns retained per session


def _usage_file(sid: str) -> Path:
    return config.SESSIONS_DIR / f"{sid}.usage"


def _record_turn(sid: str, hit: int, read: int, write: int) -> list[int]:
    """Append this turn's cache-hit % to the session's trend and return the recent history.

    The status line can render several times per turn, so a sample is only appended when the
    underlying API numbers CHANGE — the payload reflects the most recent API response, so an
    identical (read, write) pair is the same turn re-rendered, not a new one."""
    if not sid:
        return []
    f = _usage_file(sid)
    try:
        rows = [ln for ln in f.read_text().splitlines() if ln.strip()]
    except OSError:
        rows = []
    stamp = f"{read},{write},{hit}"
    if rows and rows[-1] == stamp:            # same API response → same turn, don't double-count
        return [int(r.rsplit(",", 1)[1]) for r in rows]
    rows.append(stamp)
    rows = rows[-_TREND_KEEP:]
    try:
        f.parent.mkdir(parents=True, exist_ok=True)
        f.write_text("\n".join(rows) + "\n")
    except OSError:
        pass
    return [int(r.rsplit(",", 1)[1]) for r in rows]


def _history(sid: str) -> list[tuple[int, int]]:
    """The session's recorded (cache_read, cache_write) pairs."""
    try:
        out = []
        for ln in _usage_file(sid).read_text().splitlines():
            p = ln.split(",")
            if len(p) == 3:
                out.append((int(p[0]), int(p[1])))
        return out
    except (OSError, ValueError):
        return []


# A cache REBUILD is the expensive event, and it is invisible in the hit *ratio*: in steady
# state only the new exchange is written (~1–3k tok against a ~700k cached prefix), so the ratio
# sits at ~100% and a rebuild shows up as a single dipped turn you can easily miss. Measured on
# real sessions: one rebuild cost 696,088 tokens — ~139× everything charter/prompt trimming
# saves in a whole session. So track rebuilds explicitly and cumulatively.
# Signature: the read collapses (the prefix no longer matched) AND a large write replaces it.
_REBUILD_MIN_WRITE = 15_000   # tokens; normal turns write ~1–4k
_REBUILD_READ_DROP = 0.5      # read fell to <50% of the previous turn
_REBUILD_LOUD = 200_000       # cumulative rebuild cost that earns an explanation


def _rebuilds(rows: list[tuple[int, int]]) -> tuple[int, int]:
    """(count, total tokens) of prefix rebuilds in a session's (read, write) history."""
    n = cost = 0
    for i, (read, write) in enumerate(rows):
        if write < _REBUILD_MIN_WRITE:
            continue
        prev = rows[i - 1][0] if i else 0
        # a big write with a collapsed read = the prefix was rebuilt, not merely appended to
        # (reading a huge file also writes a lot, but the read stays high — not a rebuild)
        if i == 0 or read < prev * _REBUILD_READ_DROP:
            n += 1
            cost += write
    return n, cost


def _fmt_tok(n: int) -> str:
    return f"{n/1_000_000:.1f}M" if n >= 1_000_000 else (f"{n//1000}k" if n >= 1000 else str(n))


def _cold_streak(trend: list[int]) -> int:
    """How many of the most recent turns in a row were cache-cold."""
    n = 0
    for hit in reversed(trend):
        if hit < _COLD_BELOW:
            n += 1
        else:
            break
    return n


def _cache_hint(streak: int) -> str | None:
    """One short, actionable line — only once the cache has been cold for several turns."""
    if streak < _COLD_STREAK:
        return None
    return (f"{_RED}⚠ cache cold {streak} turns{_R}{_DIM} — model/effort switch or MCP toggle "
            f"churns the prefix; prefer {_R}{_BOLD}/rewind{_R}{_DIM} over /compact{_R}")


def _context_gauge(payload: dict) -> list[str]:
    """Live **context + prompt-cache health** from the status-line payload.

    Token efficiency is mostly decided by *prompt caching*: Claude Code re-sends the whole
    request each turn, and the API serves the unchanged prefix from cache at ~10% of the input
    rate. So the number that matters isn't how big the prompt is — it's what share of it is
    **read from cache** rather than re-written. A high read:write ratio means the prefix is
    stable; if cache *creation* stays high turn after turn, something keeps changing the prefix
    (a model/effort switch, an MCP server connecting, a plugin toggle, `/compact`).

    Renders ``ctx NN%`` (context window used) and ``cache NN%`` (share of this turn's input
    served from cache). Both are absent early in a session and right after `/compact`, when the
    payload has no usage yet — we simply show nothing rather than a misleading 0.

    The cache figure wore a ``⚡`` until the persona chips gave that glyph a meaning of its own
    — *a dispatch is running* — which then rendered on the same strip as the chips' aggregate,
    two bolts apart only by a ``%``. The bolt went to the fact that has two rendering sites and
    needs them to read as one thing; the gauge took a word, which is what its sibling ``ctx``
    already had and what a rate nobody can guess from a symbol always wanted."""
    cw = (payload or {}).get("context_window") or {}
    out: list[str] = []
    pct = cw.get("used_percentage")
    if isinstance(pct, (int, float)):
        col = _GREEN if pct < 50 else (_YELLOW if pct < 80 else _RED)
        out.append(f"{_DIM}ctx{_R} {col}{int(pct)}%{_R}")
    cu = cw.get("current_usage") or {}
    read = cu.get("cache_read_input_tokens") or 0
    write = cu.get("cache_creation_input_tokens") or 0
    if read or write:
        hit = round(100 * read / (read + write))
        # <50% sustained = the prefix is churning; that's the expensive failure mode.
        col = _GREEN if hit >= 80 else (_YELLOW if hit >= 50 else _RED)
        # Dim label, coloured number — the exact shape `ctx NN%` above uses, so the two
        # session gauges read as a pair rather than as a word and a symbol.
        out.append(f"{_DIM}cache{_R} {col}{hit}%{_R}")
        try:
            sid = payload.get("session_id") or ""
            trend = _record_turn(sid, hit, read, write)
            # Rebuilds are the dominant cost and are invisible in the ratio — surface them
            # cumulatively so the price of a mid-task switch stays on screen.
            n, cost = _rebuilds(_history(sid))
            if n:
                col = _RED if cost >= _REBUILD_LOUD else _YELLOW
                out.append(f"{col}↻{n} {_fmt_tok(cost)}{_R}")
            if cost >= _REBUILD_LOUD:
                out.append(f"{_DIM}rebuilt prefix — model/effort switch, MCP toggle or a resumed "
                           f"session; pick model+effort at session start{_R}")
            else:
                hint = _cache_hint(_cold_streak(trend))
                if hint:
                    out.append(hint)
        except Exception:
            pass          # diagnostics must never break the footer
    return out


def _stale_structure(ws: str) -> bool:
    """True if the active workspace's on-disk structure is behind the current layout
    (created by an older version of charter) → flag it with a reinit tip. Best-effort, fast."""
    try:
        from . import workspace
        return workspace.needs_reinit(ws)
    except Exception:
        return False


def _clone_dirs(ws: str) -> list[Path]:
    from . import workspace
    return workspace.clones(ws)


def _repo_trees(ws: str) -> list[Path]:
    """Every repo the workspace works in — the plane's root tree, then its clones.

    Same list `charter gl-refresh` fetches forge state for, by construction (both call
    :func:`charter.workspace.repo_trees`), so a row can never be drawn for a repo whose
    CI was never fetched.
    """
    from . import workspace
    try:
        return workspace.repo_trees(ws)
    except Exception:
        return _clone_dirs(ws)


def _available() -> int:
    """The denominator in `repos N/M` — how many repos this plane could clone.

    Asked of `inventory.repos()` rather than the file's own `count`, because the plane's
    own repo is clonable whether or not `discover` has run. Reading the count alone
    rendered `repos 0/0` on a plane where `charter clone <its own repo>` works — the
    status line contradicting the CLI about the same question, which is the split this
    layout has paid for before.
    """
    try:
        from . import inventory
        return len(inventory.repos())
    except Exception:
        return 0


def _vaults() -> int:
    try:
        return len(json.loads(config.VAULTS_REGISTRY.read_text()).get("vaults", {}))
    except Exception:
        return 0


def _branch(repo_dir: Path) -> str:
    """Current branch read straight from HEAD (no git subprocess).

    Handles a clone and a linked worktree alike — see :func:`charter.util.branch_of`,
    which is where the two ``.git`` layouts are reconciled.
    """
    from . import util
    try:
        return util.branch_of(repo_dir)
    except Exception:
        return "?"


_STATE_TTL = 5.0  # seconds a cached repo-state is trusted before re-checking


def _repo_states(dirs: list[Path]) -> dict:
    """Map repo dir -> {dirty, ahead, behind}, cached with a short TTL so
    `git status` runs at most once per repo per few seconds, not every render."""
    cache_file = config.STATE_DIR / "cache" / "repostate.json"
    try:
        cache = json.loads(cache_file.read_text())
    except Exception:
        cache = {}
    now = time.time()
    out, changed = {}, False
    for d in dirs:
        key = str(d)
        ent = cache.get(key)
        if ent and (now - ent.get("ts", 0)) < _STATE_TTL:
            out[d] = ent
        else:
            st = _run_state(d)
            st["ts"] = now
            cache[key] = st
            out[d] = st
            changed = True
    if changed:
        try:
            cache_file.parent.mkdir(parents=True, exist_ok=True)
            cache_file.write_text(json.dumps(cache))
        except Exception:
            pass
    return out


def _run_state(d: Path) -> dict:
    """One `git status --porcelain --branch` → dirty flags + ahead/behind counts.

    TWO notions of dirt, from the one command, because two callers legitimately want
    different ones:

    * ``dirty`` — anything at all, untracked files included. What a repo row's ``*``
      means, and right for a tree you are actually working in: a new file you have not
      added yet is still work in progress.
    * ``tracked_dirty`` — the same minus untracked (``??``) entries, i.e. exactly what
      ``git status --untracked-files=no`` reports. What :func:`_plane_root_alert` uses,
      and it is `doctor`'s `check_plane_root` that makes it necessary rather than merely
      nicer: that check asks git the ``-uno`` question, so a plane-root warning built on
      the wider notion would have the status line calling the root dirty every turn
      while `doctor` called it clean — two answers to one question, which is worse than
      either answer alone. The reason `doctor` narrowed it applies here identically:
      memory defaults to ``share = "local"``, so ``personas/*/memory/`` accumulates
      untracked files on any plane a few days old, and a warning that is permanently on
      is furniture.

    Computed here, in the one place that already reads git's answer, rather than by a
    second `git status` in the caller: the plane root's state would otherwise cost a
    subprocess on every render, since only this path is behind `_repo_states`' TTL cache.
    """
    try:
        r = subprocess.run(
            ["git", "-C", str(d), "status", "--porcelain=v1", "--branch"],
            stdout=subprocess.PIPE, stderr=subprocess.DEVNULL, text=True, timeout=3,
        )
    except Exception:
        return {"dirty": False, "tracked_dirty": False, "ahead": 0, "behind": 0}
    dirty, tracked_dirty, ahead, behind = False, False, 0, 0
    for ln in r.stdout.splitlines():
        if ln.startswith("## "):
            m = re.search(r"\[([^\]]*)\]", ln)
            if m:
                for part in m.group(1).split(","):
                    part = part.strip()
                    if part.startswith("ahead "):
                        ahead = _int(part[6:])
                    elif part.startswith("behind "):
                        behind = _int(part[7:])
        elif ln.strip():
            dirty = True
            # `??` is the only untracked form in porcelain v1 (`!!` needs --ignored,
            # which is not asked for). Everything else — modified, staged, renamed,
            # unmerged — is a tracked path, and is what `-uno` would have printed.
            if not ln.startswith("??"):
                tracked_dirty = True
    return {"dirty": dirty, "tracked_dirty": tracked_dirty, "ahead": ahead, "behind": behind}


def _int(s: str) -> int:
    try:
        return int(s)
    except Exception:
        return 0


def _markers(state: dict) -> tuple[str, str, bool]:
    """(plain, coloured, is_dirty) suffix: `*` dirty (yellow), `↑N` ahead/unpushed
    (cyan), `↓N` behind (blue)."""
    dirty = bool(state.get("dirty"))
    ahead, behind = int(state.get("ahead") or 0), int(state.get("behind") or 0)
    plain = coloured = ""
    if dirty:
        plain += "*"; coloured += f"{_YELLOW}*{_R}"
    if ahead:
        plain += f"↑{ahead}"; coloured += f"{_CYAN}↑{ahead}{_R}"
    if behind:
        plain += f"↓{behind}"; coloured += f"{_BLUE}↓{behind}{_R}"
    return plain, coloured, dirty


def _current(payload: dict) -> tuple[str | None, str] | None:
    """``(workspace, repo)`` that the session's cwd is inside, if any.

    The workspace is ``None`` when the cwd is inside no workspace at all — standing in the
    plane root is the ordinary way that happens. A caller comparing against the active
    workspace must read ``None`` as "matches whichever workspace is active".

    Worktrees are resolved FIRST, and to their *piece* name, which is what the nested rows
    are labelled with. This never worked in either layout: an in-plane worktree sits at
    ``workspaces/<ws>/.worktrees/<repo>/<piece>``, so the plain workspace arithmetic below
    read its repo as the literal ``.worktrees`` and matched no row, leaving the cwd
    unmarked. Where worktrees are the rows you move between, an
    unmarked one is the whole feature missing.

    ``workspaces/`` is checked next and, once the cwd is known to be under it, this returns
    without consulting the root tree even when there is no repo component (e.g. the cwd is
    ``workspaces/<ws>`` itself). ``WORKSPACES_DIR`` lives *under* ``ROOT``, so falling
    through would report the root tree as current while you stand in a workspace.
    """
    ws = payload.get("workspace") or {}
    cwd = ws.get("current_dir") or payload.get("cwd") or ""
    if not cwd:
        return None
    try:
        here = Path(cwd).resolve()
    except Exception:
        return None

    try:
        from . import worktree as _wt
        loc = _wt.locate(here)
    except Exception:
        loc = None
    if loc:
        return (loc[0], loc[2])          # (workspace, piece)

    try:
        parts = here.relative_to(config.WORKSPACES_DIR.resolve()).parts
    except Exception:
        parts = None
    if parts is not None:
        return (parts[0], parts[1]) if len(parts) >= 2 else None

    return None


#: Marks the persona last seen in a tree. The same glyph the persona column uses for the
#: ACTIVE persona, deliberately: both answer "who is here", and one glyph for one question
#: is what keeps the two columns readable together. East-Asian Neutral, like every other
#: marker in this layout — see the width notes at the top of the module.
_PRESENCE = "▸"


def _presence_text(ws: str, repo: str, piece: str | None) -> str:
    """``▸steward now`` / ``▸steward 7m +1`` for a tree, or ``""``.

    An **observation**, never an assertion. Charter cannot verify that anybody is working,
    so this reads the way `silent 3d` already does — a name and an age, with the reader
    drawing the conclusion. A bare ``▸steward`` would be the claim of activity ADR 0011
    refuses to let charter make, which is why a fresh beat says ``now`` rather than nothing.

    ``+N`` counts other personas seen in the same tree inside `pieces.PRESENCE_WINDOW`. The
    heartbeat is one overwritten file, so a second worker used to vanish without trace; the
    count is the honest admission that the cell is not the whole truth, and `charter
    worktree list` is where the full picture belongs — the split `_piece_state` already
    keeps when it drops a declaration's reason.

    Returns ``""`` on anything unreadable: every-turn render path, and a footer that blanks
    is worse than one that shows less.
    """
    try:
        from . import pieces as _p
        got = _p.presence(ws, repo, piece)
    except Exception:
        return ""
    if not got:
        return ""
    who, age, others = got
    tail = f" {_DIM}+{others}{_R}" if others else ""
    return f"{_MAGENTA}{_PRESENCE}{who}{_R} {_DIM}{age}{_R}{tail}"


def _branch_cell_for(branch_text: str, presence: str, marks_plain: str = "",
                     marks_col: str = "", is_dirty: bool = False) -> str:
    """The branch cell's contents: branch, markers, and presence **if it still fits**.

    The losing order matters more than the layout. Markers and the branch name are true of
    the tree; presence is an extra observation about who happened to be standing in it. So
    the branch is truncated only so far as the markers demand — the rule this cell already
    kept — and presence is appended only out of whatever room is genuinely left over. On a
    narrow pane presence disappears and nothing that was there before moves.
    """
    br = tui.truncate(branch_text, max(1, _BRANCH_W - tui.width(marks_plain)))
    out = f"{_YELLOW if is_dirty else _DIM}{br}{_R}{marks_col}"
    if not presence:
        return out
    used = tui.width(br) + tui.width(marks_plain)
    room = _BRANCH_W - used
    need = tui.width(_strip(presence)) + 1        # +1 for the separating space
    return out + f" {presence}" if room >= need else out


def _strip(text: str) -> str:
    import re as _re
    return _re.sub(r"\x1b\[[0-9;]*m", "", text)


def _presence_for_dir(d) -> str:
    """Presence for whatever kind of tree *d* is — a worktree or a clone.

    Both, because `_tree_cells` draws both and working directly in a clone is ordinary; it
    was invisible only because the liveness hook returned early outside a worktree.
    """
    try:
        from . import worktree as _wt, workspace as _ws
        loc = _wt.locate(d)
        if loc:
            return _presence_text(loc[0], loc[1], loc[2])
        clone = _ws.clone_of(d)
        return _presence_text(clone[0], clone[1], None) if clone else ""
    except Exception:
        return ""


def _tree_cells(lead: str, label: str, d, states, branches, gl, branch=None) -> tui.Row:
    """One table row — ``<lead><label>`` in the name column, then branch+markers, CI, change.

    Shared by repo rows and nested worktree rows so the two cannot drift: a worktree's row
    is the *same four cells* as its repo's, differing only in the lead glyphs. Building
    them twice is precisely how a nested row ends up a column off from the one above it,
    which this layout has already paid for more than once.

    The name cell's width is the same total whatever the lead costs — `tui.Cell` pads and
    truncates to it — so a longer lead spends its columns out of the label, never out of
    the branch column's starting position.

    *branch* overrides the looked-up branch text — pass ``""`` to print the markers alone.
    The markers are never part of that override: dirty and ahead/behind are true of the
    tree whatever its branch is called, so they survive an emptied branch cell.
    """
    name = tui.Cell(f"{lead}{label}", 2 + 3 + _NAME_W)

    # branch + markers: truncate the *branch* so the markers always survive
    marks_plain, marks_col, is_dirty = _markers(states.get(d, {}))
    text = branches.get(d, "?") if branch is None else branch
    branch = tui.Cell(_branch_cell_for(text, _presence_for_dir(d),
                                       marks_plain, marks_col, is_dirty), _BRANCH_W)

    info = gl.get(d, {})
    ci = tui.Cell(_ci_part(info.get("ci")), _CI_W)
    change = info.get("change")
    sigil = info.get("sigil") or "!"   # old caches (pre-forge-protocol) carry no sigil
    mr_cell = tui.Cell(f"{_GREEN}{sigil}{change}{_R}" if change else "", _MR_W)

    return tui.Row(name, branch, ci, mr_cell, gap=_GAP)


def _pick_rows(dirs, budget: int, cur_repo, states, gl) -> list[Path]:
    """Which repos get a row when there are more repos than rows.

    The cap was a bare positional slice, so with the list in directory order the rows went
    to whatever sorted first and `…(+N more)` swallowed the rest. Observed with 18 clones:
    thirteen rows of clean `aaa-svc-NN` on main, and the hidden five held every dirty
    repo, the only off-main branch and both failing pipelines. The table was showing the
    repos with nothing to say. Worse, the repo you were standing in could be among the
    hidden, so the bold you-are-here marker attached to nothing on screen.

    Selection is ranked; **display order is not**. The chosen set is re-sorted back into
    the original order before drawing, because a row that moves as its state changes stops
    being a place you can look — you would re-read the whole table every turn to find the
    repo you were just in. Same reason `_repo_rows` colours by position in the FULL list
    rather than by position among the shown: a repo keeps its colour and its row whether
    or not its neighbour went dirty.
    """
    order = {d: i for i, d in enumerate(dirs)}

    def rank(d):
        st = states.get(d) or {}
        info = gl.get(d) or {}
        return (
            0 if d.name == cur_repo else 1,        # where you are, always
            0 if st.get("dirty") else 1,
            0 if (st.get("ahead") or st.get("behind")) else 1,
            0 if info.get("ci") in ("failed", "running") else 1,
            0 if info.get("change") else 1,
            order[d],                              # stable: original order breaks ties
        )

    chosen = sorted(dirs, key=rank)[:budget]
    return sorted(chosen, key=lambda d: order[d])


def _detail_worktrees(ws: str, dirs) -> list[Path]:
    """The worktrees to draw as full rows, or ``[]`` to keep the one-line summary.

    With exactly ONE repo — the monorepo shape, and equally a fleet workspace holding a
    single clone — `_MAX_REPO_LINES` leaves about a dozen rows unspent while the compact
    summary crams every piece onto one line as bare names: no branch, no dirty flag, no
    CI, no change. There is nothing to compress, so spend the rows. At two or more repos
    the summary is still the right trade, because a repo must never lose its row to
    another repo's worktrees.

    ONE predicate, whose result is passed to everything that needs it. `render` scans
    these directories for branch/dirty/CI and `_repo_rows` draws them; deciding it
    separately in each is how a row gets drawn for a directory nobody scanned, which
    renders as `?` in the branch column and blanks everywhere else — strictly worse than
    the summary line it replaced.
    """
    if len(dirs) != 1:
        return []
    try:
        from . import worktree as _wt
        return _wt.dirs_for(ws, dirs[0].name)[: _MAX_REPO_LINES - 1]
    except Exception:
        return []


def _piece_state(ws: str, repo: str, piece: str) -> str:
    """What a piece row says about itself: its declaration, or how long it has been silent.

    Never a verdict. `silent 3d` is an age and whether that is a problem is the reader's
    call — charter has not verified that the worker is gone, and ADR 0009's rule about
    unearned diagnoses applies with more force here than anywhere, because the status line
    is read at a glance and its wording is taken as settled.

    File reads only: this is on the every-turn path and the module's contract forbids a git
    subprocess. Returns "" rather than raising on anything unreadable — a footer that
    blanks is worse than one that shows less.
    """
    try:
        from . import pieces as _p
        said = _p.outcome(_p.declaration_for(ws, repo, piece))
        if said:
            return said.split(":")[0]  # the reason belongs in `worktree list`, not here
        quiet = _p.silence(ws, repo, piece)
        return f"silent {quiet}" if quiet else ""
    except Exception:
        return ""


def _piece_summary(ws: str) -> str | None:
    """The workspace line's piece cell — counts, and the oldest silence.

    Omitted entirely when the workspace has no pieces, exactly as the todo count is: a
    figure that renders on every session including the ones it means nothing for is how a
    line stops being read at all.

    Existence comes from `worktree.dirs_for`, which is filesystem-only by design — its own
    docstring records that listing those directories IS reading git's output, since
    `git worktree add`/`remove` are what create and delete them.
    """
    try:
        from . import pieces as _p
        from . import worktree as _wt
        base = _wt.root(ws)
        repos = sorted(d.name for d in base.iterdir() if d.is_dir())
    except (OSError, Exception):
        return None

    total = done = gave_up = 0
    quiet: list[str] = []
    for repo in repos:
        try:
            for wt in _wt.dirs_for(ws, repo):
                total += 1
                d = _p.declaration_for(ws, repo, wt.name)
                if d and d["event"] == "done":
                    done += 1
                elif d:
                    gave_up += 1
                else:
                    s = _p.silence(ws, repo, wt.name)
                    if s:
                        quiet.append(s)
        except Exception:
            continue
    if not total:
        return None

    parts = [f"{_DIM}pieces{_R} {total}"]
    if done:
        parts.append(f"{_GREEN}{done} done{_R}")
    if gave_up:
        parts.append(f"{_YELLOW}{gave_up} abandoned{_R}")
    if quiet:
        parts.append(f"{_DIM}{len(quiet)} silent {max(quiet, key=_silence_rank)}{_R}")
    return " ".join(parts)


#: Order silence ages so the OLDEST is the one reported. Coarse strings ("3m", "5h", "2d")
#: do not sort lexically — "9m" would beat "2d" — and the oldest is the whole point.
_UNIT_SECS = {"m": 60, "h": 3600, "d": 86400}


def _silence_rank(age: str) -> int:
    try:
        return int(age[:-1]) * _UNIT_SECS.get(age[-1], 0)
    except (ValueError, IndexError):
        return 0


def _repo_rows(dirs, active, cur, states, branches, gl, detail_wts=()) -> list[tui.Node]:
    """One table row per clone, nested under the workspace like a tree:

        ├─ <repo>   <branch><markers>   <ci>   <sigil><change>

    repo in its own colour (current repo bold+underlined); dirty→branch yellow
    `*`; ahead `↑N` cyan; behind `↓N` blue; pipeline ✓/✗/●/… ; open change `!N`/`#N`
    green, in that clone's own forge's notation (GitLab `!`, GitHub `#`).

    Column widths are declared per cell; the kit pads/truncates so sibling
    rows stay aligned and nothing ever exceeds the render width.

    Bounded by `_MAX_REPO_LINES` TOTAL rows, not by repo count: repos are prioritised
    over worktree rows (every repo gets its own row before any repo's worktrees get
    nested rows), so a repo is never dropped just because an earlier repo's worktrees
    ate the budget.

    Every row here is a clone the workspace owns. The plane's own tree used to appear
    among them, drawn in the plane's colour to say "this repo IS the control plane"; it is
    no longer a row at all, because listing it beside repos you are meant to edit is what
    invited editing it (docs/adr/0008).
    """
    if not dirs:
        return []
    # `cur[0] is None` = the cwd is in the root tree, which is a member of EVERY
    # workspace, so it is current whichever one is active.
    cur_repo = cur[1] if (cur and (cur[0] is None or cur[0] == active)) else None
    n = len(dirs)
    capped = n > _MAX_REPO_LINES
    show = _pick_rows(dirs, _MAX_REPO_LINES - 1, cur_repo, states, gl) if capped else dirs
    # What's left of the total-row budget after every shown repo gets its own row (and,
    # if capped, the trailing "…(+N more)" line) — spent on nested worktree rows below.
    wt_budget = _MAX_REPO_LINES - len(show) - (1 if capped else 0)

    # Palette index by position in the FULL list, not among the shown. Counting within
    # `show` meant a repo changed colour whenever a neighbour entered or left the cap —
    # and with ranked selection that happens the moment anything goes dirty, so the one
    # channel carrying "this row is that repo" would churn turn to turn.
    palette_ix = {d: i for i, d in enumerate(dirs)}

    rows: list[tui.Node] = []
    for i, d in enumerate(show):
        color = _PALETTE[palette_ix.get(d, i) % len(_PALETTE)]
        emph = f"{_BOLD}{_UNDER}" if d.name == cur_repo else ""

        # worktrees: a ⑂N badge here, plus either full nested rows (`_detail_worktrees`)
        # or the one-line summary below.
        try:
            from . import worktree as _wt
            wts = _wt.dirs_for(active, d.name)
        except Exception:
            wts = []
        kids = list(detail_wts) if (len(show) == 1 and detail_wts) else []
        kids = kids[:wt_budget]
        # The badge is a count of what you CANNOT see. With every piece drawn as its own
        # row below, `⑂2` above two visible rows is just noise restating them.
        badge = f"{_DIM} ⑂{len(wts)}{_R}" if len(kids) < len(wts) else ""

        # A repo with rows nested beneath it is not where the tree ends, so it keeps `├─`.
        is_last = (not capped) and (i == len(show) - 1) and not kids
        tree = _TREE_END if is_last else _TREE_MID

        # indent + tree + name (padding lands outside the style, not underlined)
        rows.append(_tree_cells(f"  {_DIM}{tree}{_R}",
                                f"{emph}{color}{d.name}{_R}{badge}",
                                d, states, branches, gl))

        if kids:
            wt_budget -= len(kids)
            for j, w in enumerate(kids):
                # `╰─` for the last child, NOT `└─`. `render`'s "the tree keeps going"
                # rewrite searches backwards for the last `└─` to turn into `├─`; a child
                # sharing that marker gets rewritten instead of its repo. That is the
                # exact bug `_TREE_WT` was introduced to prevent — see its definition.
                mark = _TREE_WT if j == len(kids) - 1 else _TREE_MID
                emph_w = f"{_BOLD}{_UNDER}" if w.name == cur_repo else ""
                # `charter worktree add <repo> <piece>` names the branch after the piece,
                # so by default these two columns print the same word twice and the branch
                # cell spends `_BRANCH_W` restating the name beside it. Empty it when they
                # agree (the dirty/ahead/behind markers still render — they are true of the
                # tree, not of its name), so the column comes to mean "this piece is NOT on
                # the branch you would assume" — the only case worth 34 columns.
                wb = branches.get(w, "?")
                # When the branch restates the piece name — the default, since `worktree
                # add` names them alike — the column was already being emptied as 34
                # wasted columns. Spend them on what the piece SAID instead. When the
                # branch differs it still wins: "this piece is not on the branch you would
                # assume" is the surprise, and a surprise outranks a status.
                state = _piece_state(active, d.name, w.name)
                rows.append(_tree_cells(f"  {_DIM}{_TREE_PIPE}{mark}{_R}",
                                        f"{emph_w}{_DIM}{w.name}{_R}",
                                        w, states, branches, gl,
                                        branch=state if wb == w.name else wb))
        # ONE summary line per repo, newest piece first — a fixed cost no matter how many
        # worktrees exist, so the footer can't grow with them (the ⑂N badge above carries
        # the true total, and overflow truncates with … rather than dropping pieces
        # silently). The lead glyph is VISIBLE: Claude Code's footer collapses a
        # whitespace-only prefix to column 0 — the same trap the persona column works
        # around below — which would drop this line to the left margin and stop it
        # reading as a child of its repo.
        elif wts and wt_budget > 0:
            wt_budget -= 1
            lead = f"  {_DIM}{_TREE_PIPE}{_R} {_DIM}{_TREE_WT}{_R}"
            pieces = tui.truncate(" · ".join(w.name for w in wts),
                                  max(1, _LEFT_W - tui.width("  │   ╰─ ")))
            rows.append(tui.Text(f"{lead}{_DIM}{pieces}{_R}"))

    if capped:
        # Say whether the hidden ones matter. Selection is ranked, so anything with a
        # dirty tree, an unpushed commit, a failing pipeline or an open change is already
        # on screen — which makes "all clean" a claim the code can actually back, and
        # turns the overflow line from a worry into an answer.
        hidden = [d for d in dirs if d not in set(show)]
        quiet = not any((states.get(d) or {}).get("dirty")
                        or (states.get(d) or {}).get("ahead")
                        or (states.get(d) or {}).get("behind")
                        or (gl.get(d) or {}).get("ci") in ("failed", "running")
                        or (gl.get(d) or {}).get("change") for d in hidden)
        note = ", all clean" if quiet else ""
        rows.append(tui.Text(f"  {_DIM}{_TREE_END}…(+{len(hidden)} more{note}){_R}"))
    return rows


def _persona_line() -> str | None:
    """Footer rows for personas: the *active* (adopted) persona with its vault,
    then a roster of every persona — each is also dispatchable as a sub-agent.

    Returns None when no personas are defined, so non-persona projects stay to
    one line.
    """
    try:
        from . import persona
        names = persona.list_personas()
        if not names:
            return None
        names = sorted(names)
        active = persona.resolve_active()
        if not active:
            avail = f"{_DIM} · {_R}".join(f"{_DIM}{n}{_R}" for n in names)
            return f"{_DIM}◆ persona none{_R} · {avail}{_DIM} · charter persona use <name>{_R}"
        # Active (adopted) persona: name + vault health (the role reads as noise —
        # the name already says it).
        seg = f"{_MAGENTA}◆{_R} {_BOLD}{active}{_R}"
        vault = persona.vault_of(active)
        if vault:
            # Same mark as the chips, from the same function rather than a second one
            # applying "the same" rule: this renderer is the fallback, so a divergence
            # here would only ever be seen on the day the layout is already broken.
            seg += f"{_DIM} · vault {_R}{vault}{_vault_dot(vault)}"
        # The other personas, on the same line — each dispatchable as a sub-agent.
        others = [n for n in names if n != active]
        if others:
            chips = f"{_DIM} · {_R}".join(f"{_DIM}{n}{_R}" for n in others)
            seg += f"{_DIM} · ◇ agents {_R}{chips}"
        return seg
    except Exception:
        return None


#: How long a vault's health is trusted before re-checking. Long, because the answer
#: almost never changes (a vault becomes healthy once, when you create it) and the fresh
#: answer is one `charter vault list` away — while the stale one costs a subprocess.
_VAULT_TTL = 60.0

#: Within a single render, several personas commonly name the same vault. Memoised here
#: so the disk cache is read once per vault per process rather than once per chip.
_vault_memo: dict = {}


def _vault_health(vault: str) -> tuple[bool, str]:
    """``(ok, detail)`` for a vault, cached on disk with a short TTL.

    A `1password` or `reference` vault's ``health()`` shells out to ``op``. This is the
    status line, which renders on EVERY turn, and both call sites below ran it once per
    persona chip with no cache — profiled at 96% of render time, and with a realistic
    ~250ms `op` round trip a ten-persona roster measured ~3s of wall clock per turn. With
    1Password's desktop integration each of those calls is also a chance to raise an
    unprompted biometric prompt. The payoff on screen is one character.

    Same cache shape as :func:`_repo_states` — ``STATE_DIR/cache``, a timestamp per key,
    written best-effort — so there is one idiom here for "expensive truth, cheaply
    re-read", not two.
    """
    if vault in _vault_memo:
        return _vault_memo[vault]

    from .secrets import registry
    cache_file = config.STATE_DIR / "cache" / "vaulthealth.json"
    try:
        cache = json.loads(cache_file.read_text())
    except Exception:
        cache = {}
    now = time.time()
    ent = cache.get(vault)
    if ent and (now - ent.get("ts", 0)) < _VAULT_TTL:
        got = (bool(ent.get("ok")), ent.get("detail") or "")
        _vault_memo[vault] = got
        return got

    ok, detail = registry.provider_for(vault).health()
    cache[vault] = {"ok": bool(ok), "detail": detail or "", "ts": now}
    try:
        cache_file.parent.mkdir(parents=True, exist_ok=True)
        cache_file.write_text(json.dumps(cache))
    except Exception:
        pass
    _vault_memo[vault] = (ok, detail)
    return ok, detail


def _vault_dot(vault: str | None) -> str:
    """Compact vault mark for persona chips — **only when the vault cannot be used**.

    * nothing — no vault declared, or one that is registered and healthy;
    * ``◦`` dim — declared but not usable *here*: this machine has no vault by that
      name, or it has one whose file does not exist yet;
    * ``!`` yellow — registered and unhealthy (unreadable, bad provider config).

    Silence is the point, and it is the same argument :func:`_health_mark` is built on:
    a ✓ on every chip on every turn is furniture within a day, after which a real fault
    inside the column reads like a zero. The four-state version spent a character per
    persona per render to say "fine".

    Worse, its two dim ``·`` states were different facts wearing one glyph — a persona
    that needs no vault, and a persona *declaring* ``vault: X`` that this machine has
    never registered. The second is the only vault fact worth a character, and it was
    the one you could not see.

    The two unusable reasons deliberately collapse into one ``◦``. Their fixes differ
    (`charter vault add` vs `charter secret set`), a chip can carry neither, and
    `charter persona list` already prints both in words — so a third glyph would buy a
    distinction the reader still has to leave the status line to act on.
    """
    try:
        from .secrets import registry
        if not vault:
            return ""
        if vault not in registry.vaults():
            return f" {_DIM}◦{_R}"
        ok, detail = _vault_health(vault)
        if not ok:
            return f" {_YELLOW}!{_R}"
        # `plain-file.health()` returns ok=True with "not created yet (<path>)" for a
        # registered vault holding nothing. Reading only `ok` once painted a green ✓ on
        # a vault that did not exist; it now reads as unusable, which it is.
        if "not created yet" in (detail or ""):
            return f" {_DIM}◦{_R}"
        return ""
    except Exception:
        return ""


def _health_mark(name: str, known: set[str] | None = None) -> str:
    """Health mark for a persona chip — **only when something is wrong**.

    ``⚑`` the charter is a draft, so charter generates no sub-agent for it and it
    cannot be dispatched; ``✗`` its config is broken (dangling ``extends:``/``uses:``,
    or an inheritance cycle). A healthy persona gets nothing: a row of ✓s becomes
    furniture within a day, and then a real ✗ inside it draws no more attention than a
    zero would.

    Soft findings — no role, no ``delegate-when`` — deliberately do not appear here.
    They are real, and `lint`/`doctor` have room to explain them; a chip does not.

    Cost is why this calls :func:`persona.structural_errors` rather than `lint`: the
    full lint walks the plugin cache, and even `lint(deep=False)` pays for the vault,
    role, delegate-when and unknown-key checks (plus an import of `commands_persona`)
    that produce nothing a chip can show. This renders on every single turn.
    """
    try:
        from . import persona
        if persona.is_draft(name):
            return f" {_YELLOW}⚑{_R}"
        if persona.structural_errors(name, known=known):
            return f" {_RED}✗{_R}"
        return ""
    except Exception:
        return ""


def _mem_count(name: str, shared: bool = False, ephemeral: bool = False,
               session: str | None = None) -> int:
    """Count a persona's (or the shared namespace's) memories in one quadrant.
    Cheap: one dir glob. Best-effort — never breaks the status line."""
    try:
        from . import persona
        return len(persona.memories(name, shared=shared, ephemeral=ephemeral, session=session))
    except Exception:
        return 0


def _todo_count(ws: str) -> int:
    """How many todos the given workspace still has open.

    Workspace-scoped by construction (:func:`charter.todos.count_open` is), and that is
    the whole reason this number may sit on the top line at all: it is a property of the
    active workspace, so it belongs beside that workspace's name. A total across every
    workspace would be a figure no command could reproduce — `charter ws todo` lists one
    workspace, because a workspace is the unit of task isolation.

    Cheap for the same reason :func:`_mem_count` is: one directory glob, no parse. This
    renders on EVERY turn, so nothing here may read the network or walk a repo.

    0 on any failure, never an exception. The count is the least important thing on the
    line — trading the entire status line for one digit, which is what letting this
    escape into `render`'s fallback would do, is the wrong bargain by a wide margin.
    """
    try:
        from . import todos
        return todos.count_open(ws)
    except Exception:
        return 0


def _mem_badge(persistent: int, ephemeral: int = 0) -> str:
    """Coloured memory-count badge: ``✎N`` persistent (green, committed) + ``◌N``
    ephemeral (yellow, session scratch). '' when both are zero."""
    parts = []
    if persistent:
        parts.append(f"{_GREEN}✎{persistent}{_R}")
    if ephemeral:
        parts.append(f"{_YELLOW}◌{ephemeral}{_R}")
    return (" " + " ".join(parts)) if parts else ""


def _inflight_by_persona() -> dict[str, tuple[int, float, bool]]:
    """``persona → (dispatches held, epoch start of the OLDEST, oldest presumed dead)``.

    Computed once per render and handed to every chip: `inflight.live_records` is a
    directory glob, which is affordable on a surface that draws every turn, but not
    once per persona on a thirty-persona roster.

    The oldest wins because the newest answers nothing. Three dispatches out, the
    freshest a minute old, is not news; three out with one at ``2h`` is the whole
    reason the age is on screen. The flag rides with the oldest for the same reason:
    it qualifies the age that gets drawn, and the oldest is the record that crosses
    the presumed-dead threshold first.
    """
    try:
        from . import inflight
        out: dict[str, tuple[int, float, bool]] = {}
        for name, started, dead in inflight.live_records():
            n, oldest, oldest_dead = out.get(name, (0, started, dead))
            if started < oldest:
                oldest, oldest_dead = started, dead
            out[name] = (n + 1, oldest, oldest_dead)
        return out
    except Exception:
        return {}


def _inflight_badge(entry: tuple[int, float, bool] | None) -> str:
    """``⚡2 4m`` for a persona with dispatches in flight, ``⚡ 45m?`` when the oldest has
    outlived every expectation, '' for a persona with none.

    **Count only above one.** A lone ``⚡1`` is the same non-fact as `todo 0` or `✎0`,
    both of which render as nothing here: presence is the signal, and the digit only
    starts carrying information once there is more than one of them.

    **Age always**, in `pieces._presence_age`'s vocabulary — so a dispatch ages in the
    same units as the `silent 12m` a couple of rows away, and so ``0m`` (technically
    correct, reads as broken) renders as ``now`` for the same reason it does there.
    Coarse on purpose: this line refreshes every ten seconds, so a seconds figure would
    be a number nobody could trust to be current.

    **``?`` past the presumed-dead threshold** (#308). The age keeps climbing — the record
    is no longer deleted, so this is where `2h` and `3d` become reachable at all — and the
    mark says *presumed dead, not confirmed*, which is the whole of what charter knows: it
    cannot tell a killed process from a sub-agent still grinding. Hence a question mark and
    not `✗`, and hence no colour escalation: it stays in the age's dim, because a red mark
    would claim the certainty the mark exists to disclaim. ``?`` is ASCII and East-Asian
    *Narrow*, unlike the Ambiguous glyphs that have broken this column twice.

    It reads with the only other `?` on this line rather than against it: `pieces` draws a
    bare `?` where a presence age is unreadable, and both mean *charter cannot vouch for
    this number*. They are never confusable — that one replaces an age, this one trails one.
    """
    if not entry:
        return ""
    try:
        from datetime import datetime, timezone
        from . import pieces
        n, started, presumed_dead = entry
        age = pieces._presence_age(datetime.fromtimestamp(started, timezone.utc),
                                   datetime.now(timezone.utc))
        mark = "?" if presumed_dead else ""
        return f" {_YELLOW}⚡{n if n > 1 else ''}{_R} {_DIM}{age}{mark}{_R}"
    except Exception:
        return ""


def _persona_chips(session: str | None = None) -> list[str]:
    """One chip per persona (active first) for the status-line right column, each
    tagged with its memory counts (``✎`` persistent + ``◌`` ephemeral). Every
    persona is also dispatchable as a sub-agent."""
    try:
        from . import persona
        names = sorted(persona.list_personas())
        if not names:
            return []
        active = persona.resolve_active()
        order = ([active] if active in names else []) + [n for n in names if n != active]
        known = set(names)   # computed once for the whole column, not once per persona
        flying = _inflight_by_persona()   # one glob for the column, not one per chip
        chips = []
        flagged_names: set[str] = set()
        for n in order:
            dot = _vault_dot(persona.vault_of(n))
            badge = _mem_badge(_mem_count(n), _mem_count(n, ephemeral=True, session=session))
            health = _health_mark(n, known=known)
            flight = _inflight_badge(flying.get(n))
            if health:
                # `_health_mark` speaks ONLY when something is wrong, so its presence is
                # the signal — recorded here rather than recovered from the rendered chip,
                # which would tie truncation to a glyph choice.
                flagged_names.add(n)
            # The marker is always exactly two columns, so every persona name starts in
            # the same column as every other AND as the column header above them — the
            # header carries `_MARK_HEAD` for exactly that reason. All three markers are
            # East-Asian Neutral, so no font gets to disagree about that width. Badges
            # trail the name, where nothing after them has to line up.
            #
            # `flight` goes LAST of the badges because it is the only one that changes
            # while you watch: its count moves and its age ticks, and every character
            # after a badge that changes moves with it. Behind it there is nothing to
            # move. `⚡` is East-Asian *Wide* — two cells, unambiguously, unlike the
            # Ambiguous glyphs that have broken this layout twice — and it is trailing,
            # so its width never reaches a name.
            if n == active:
                chips.append(f"{_MAGENTA}{_MARK_ACTIVE} {_BOLD}{n}{_R}{dot}{badge}{health}{flight}")
            else:
                chips.append(f"{_DIM}{_MARK_IDLE} {n}{_R}{dot}{badge}{health}{flight}")
        if len(chips) > _MAX_PERSONA_LINES:
            # Which ones survive matters more than how many. Keeping the first N
            # alphabetically would drop exactly the personas worth seeing, so the order is
            # by what a truncated column is FOR: the active persona, then anything carrying
            # a health mark (`_health_mark` speaks only when something is wrong), then the
            # rest. The count of what is hidden is shown rather than implied — the same
            # contract `_repo_rows` keeps with its own "(+N more)".
            keep = _MAX_PERSONA_LINES - 1          # one row spent saying what was dropped
            by_name = dict(zip(order, chips))
            head = [n for n in order[:1] if n == active]
            rest = [n for n in order if n not in head]
            ordered = head + [n for n in rest if n in flagged_names] \
                           + [n for n in rest if n not in flagged_names]
            shown = ordered[:keep]
            hidden = len(order) - len(shown)
            chips = [by_name[n] for n in shown]
            chips.append(f"{_DIM}  …(+{hidden} more · charter persona list){_R}")
        return chips
    except Exception:
        return []


def _session_news(sid: str | None) -> list[str]:
    """Counters for what has happened **in this session** — in flight, denied,
    recorded, dispatched.

    Deliberately silent when nothing is happening. A counter that renders every turn
    becomes furniture within a day, and then a real guard denial appearing in it gets
    no more attention than a zero would. Presence IS the signal.

    Returns the pieces, not a line: they join the context gauges on the session strip,
    because they answer the same question (*what is happening right now*) — which is
    exactly what they did NOT do while they rendered inside the repo column, where
    "⛊ 1 denied" sat under a repo tree and read as news about a repo.

    Everything here is already computed elsewhere and costs well under a millisecond;
    the status line renders on every turn, so nothing may be added that reads the
    network or walks a repo.
    """
    out: list[str] = []
    try:
        from . import inflight
        live = inflight.live()
        if live:
            # A bare count. The names moved onto the persona chips, beside the personas
            # they are about — here they made the reader match a name against a roster
            # ten rows above to learn anything, and cost the width to do it.
            #
            # The aggregate stays because the chips are croppable: that column caps at
            # `_MAX_PERSONA_LINES` and disappears entirely below `_LEFT_W + _RIGHT_MIN_W`,
            # so this is what survives a narrow pane — the same contract `_repo_rows`
            # keeps with its own "(+N more)" row.
            out.append(f"{_YELLOW}⚡ {len(live)}{_R}")
    except Exception:
        pass

    try:
        from . import trace
        ev = trace.read(sid) if sid else []
        kinds: dict[str, int] = {}
        for e in ev:
            k = e.get("event")
            if k:
                kinds[k] = kinds.get(k, 0) + 1
        if kinds.get("deny"):
            out.append(f"{_RED}⛊ {kinds['deny']} denied{_R}")
        if kinds.get("memory"):
            out.append(f"{_GREEN}✎ {kinds['memory']}{_R}{_DIM} recorded{_R}")
        if kinds.get("dispatch"):
            out.append(f"{_DIM}⇢ {kinds['dispatch']} dispatched{_R}")
    except Exception:
        pass
    return out


def _persona_exists(name: str) -> bool:
    """Whether a persona definition is on disk. One `Path.exists`, so it is affordable on
    a surface that renders every turn — and it asks `persona.def_path`, which owns where a
    charter may live (directory layout, or the legacy flat file)."""
    try:
        from . import persona
        return persona.def_path(name).exists()
    except Exception:
        return True   # unknown is not "missing": never manufacture an alert from a failure


def _alerts(active: str) -> list[str]:
    """Full-width alert lines — a pinned-version mismatch, workspaces needing reinit,
    a nested plane, a plane root being worked in.

    Kept off the session strip and out of both columns: these are not telemetry but
    *actionable* problems that carry the command that fixes them, and they are about
    the control plane rather than this session's activity. They render only when real,
    so they cost no rows on a healthy control plane.

    The plane-root check goes LAST because this function has ONE guard: an exception
    anywhere below leaves the alerts already appended in `out` and drops everything
    after it. The two above it carry commands that fix problems of their own, so the
    newest addition is the one that pays if something here goes wrong.
    """
    out: list[str] = []
    try:
        from . import __version__, config, instance as _instance, workspace as _ws
        locked = _instance.locked_version(_instance.load(config.ROOT))
        if locked and locked != __version__:
            out.append(f"{_YELLOW}⚠{_R} {_DIM}charter{_R} {__version__} {_DIM}→ pinned{_R} "
                       f"{locked}{_DIM} · charter version sync{_R}")
        # A declared front door that names nothing resolves to no persona at all, and
        # every surface that would have shown one shows nothing instead — including this
        # one, whose persona chip simply disappears. The absence is the symptom and it is
        # unreadable; the row is what makes it a message. `doctor` has room to explain.
        declared = _instance.default_persona_of(_instance.load(config.ROOT))
        if declared and not _persona_exists(declared):
            out.append(f"{_YELLOW}⚠{_R} {_DIM}front door{_R} {declared} {_DIM}— no such "
                       f"persona · charter persona default <name>{_R}")
        stale = [w for w in _ws.list_workspaces() if w != active and _ws.needs_reinit(w)]
        if stale:
            out.append(f"{_YELLOW}⚠{_R} {_DIM}reinit{_R} {len(stale)} {_DIM}ws · "
                       f"charter ws reinit --all{_R}")
        # Fires only when the resolution was OVERRIDDEN into a nested plane — `find_root`
        # now hops outward through `workspaces/`, so reaching here at all means
        # $CHARTER_ROOT pinned this session inside the inner one. Standing in a nested
        # clone no longer produces this row, and must not: the memory and the vault go to
        # the outer plane now, so the warning would be false and would cost a row on every
        # render of a session that is doing nothing wrong.
        #
        # Both planes by PATH, never `Path.name`. Clone charter into its own plane and both
        # directories are called `charter`, so this row used to render "memory and vault go
        # to charter, not charter" — it fired correctly and said nothing, which is why the
        # nesting went unexplained for so long (#200).
        # Only when the hop was OVERRIDDEN — `config.NESTED_ORIGIN == ROOT` means
        # $CHARTER_ROOT pinned this session inside the nested plane, so charter
        # really is writing there. After a plain hop ROOT is the outer plane and
        # this row would be false.
        outer = _nested_under() if getattr(config, "NESTED_ORIGIN", None) == config.ROOT else None
        if outer is not None:
            from .util import short_path
            out.append(f"{_RED}⚠{_R} {_DIM}nested plane{_R} — {_DIM}memory and vault go to"
                       f"{_R} {short_path(config.ROOT)}{_DIM}, not{_R} {short_path(outer)}"
                       f"{_DIM} · unset CHARTER_ROOT{_R}")
        root_line = _plane_root_alert()
        if root_line:
            out.append(root_line)
    except Exception:
        pass
    return out


#: Fallback names for "the branch this tree is meant to sit on", tried in order and only
#: when the repository itself does not say. Convention, not guesswork: a name counts only
#: when a ref by that name actually exists in this repo.
_ROOT_DEFAULTS = ("main", "master")


def _common_git_dir(gitdir: Path) -> Path:
    """The git directory holding the SHARED refs behind *gitdir*.

    A linked worktree's git dir (``<main>/.git/worktrees/<name>``) holds only what is
    per-worktree — HEAD, the index — while every ref, including ``origin/HEAD`` and the
    branches, lives in the common directory its ``commondir`` file names. ``$CHARTER_ROOT``
    may legitimately point at a worktree (``root._plane_of`` documents that escape hatch),
    and reading refs from the per-worktree directory there finds none of them: the plane
    would look like a repo with no default branch rather than one being worked in.
    """
    try:
        txt = (gitdir / "commondir").read_text().strip()
    except OSError:
        return gitdir                      # a clone's git dir IS the common dir
    if not txt:
        return gitdir
    p = Path(txt)
    return p if p.is_absolute() else (gitdir / p)


def _ref_exists(common: Path, ref: str) -> bool:
    """Whether *ref* (e.g. ``refs/heads/main``) exists — loose file **or** packed.

    Both forms are normal and either can be the only one: a fresh ``git init`` writes
    loose refs, while anything that has been ``gc``'d keeps them in ``packed-refs``.
    Checking only the loose form reports an established repo as having no ``main`` at
    all, which here would silently disable the branch half of the warning on exactly the
    long-lived planes it is for.
    """
    if (common / ref).is_file():
        return True
    try:
        packed = (common / "packed-refs").read_text()
    except OSError:
        return False
    # `<sha> refs/heads/main`, one per line; `^<sha>` peel lines never carry a ref name.
    return any(ln.rstrip().endswith(f" {ref}") for ln in packed.splitlines())


def _default_branch(gitdir: Path) -> str | None:
    """The branch this tree is meant to sit on, or ``None`` when nothing says.

    ``origin/HEAD`` is the repository's own answer — ``git clone`` writes it and
    ``git remote set-head`` maintains it — so it wins outright: a plane living on
    ``trunk`` must never be told every turn that it is off ``main``.

    ``None`` is a real answer, and it is what lets the branch half of the warning go
    quiet. A plane with no remote and a hand-named branch has no default to be off, and a
    warning manufactured from a guess would fire forever on a tree nobody is misusing —
    which is precisely the furniture this element is designed not to become. Dirtiness
    still speaks; only the branch claim is withheld.

    Filesystem-only, like :func:`charter.util.branch_of`, because this runs on every
    render: two small reads answer it exactly, where a ``git`` fork would be paid over
    and over for the same constant.
    """
    common = _common_git_dir(gitdir)
    try:
        head = (common / "refs" / "remotes" / "origin" / "HEAD").read_text().strip()
    except OSError:
        head = ""
    prefix = "ref: refs/remotes/origin/"
    if head.startswith(prefix):
        return head[len(prefix):].strip() or None
    for name in _ROOT_DEFAULTS:
        if _ref_exists(common, f"refs/heads/{name}"):
            return name
    return None


def _head_detached(gitdir: Path) -> bool:
    """True when HEAD names a commit rather than a branch.

    Its own read rather than an inference from :func:`_branch`, which collapses the two:
    a detached HEAD reads back from there as a short sha, and a short sha is not
    distinguishable from a branch someone named after one. The distinction has to be
    made where git makes it — HEAD is a symref (``ref: refs/heads/…``) or it is not.

    Reads the gitdir it is GIVEN, never the common dir — the opposite of
    :func:`_default_branch`. HEAD is per-worktree (that is the whole point of a linked
    worktree), while refs are shared, so following ``commondir`` here would report the
    main tree's branch for a worktree standing on its own.

    ``False`` when HEAD cannot be read at all. "Unknown" is not "detached", and
    manufacturing the most alarming of the three states out of a failed read is exactly
    the false alarm this element cannot afford.
    """
    try:
        txt = (gitdir / "HEAD").read_text().strip()
    except OSError:
        return False
    return bool(txt) and not txt.startswith("ref:")


def _plane_root_alert() -> str | None:
    """One line when the **plane root** is being worked in — dirty, detached, or off its
    default branch. ``None`` otherwise, which is the ordinary case.

    The root is the directory holding ``charter.toml``: the control plane itself, and
    since ADR 0007 not a repo row at all — a plane's `repo_trees` is its clones and
    nothing else. That absence is exactly why this line exists (docs/adr/0008). Two
    sessions that both sit in the root share one working tree and one HEAD and thrash
    each other's branches, while charter reports two different workspaces and lists no
    tree that would hint at why. **Not presenting a tree is not the same as preventing
    work in it**, and the failure is invisible in the one surface a user would check.

    An *alert* rather than a row, and deliberately: this is not a property of the active
    workspace (the top line) nor of this session (the bottom strip), so it sits with the
    other actionable control-plane problems, full width. Full width also costs the layout
    nothing — the row has no right-hand neighbour, so unlike a row in the repo column it
    cannot shear a column by being one cell wider than `tui.width` believes.

    All findings share ONE line. A row is spent on every single turn, and "dirty AND off
    main" is one situation with two symptoms.

    "Dirty" here means ``tracked_dirty`` — untracked files excluded, which is exactly
    what `doctor`'s `check_plane_root` asks git for (``--untracked-files=no``). Both the
    reason and the agreement matter. The reason: memory defaults to ``share = "local"``,
    written to disk and never committed for you, so ``personas/*/memory/`` fills with
    untracked files on any plane a few days old and the wider notion would put this line
    on screen permanently — furniture, which is the one thing this element must not
    become. The agreement: `doctor` and the status line answer the same question about
    the same tree, and two answers to one question is worse than either alone.
    """
    try:
        if not config.HAS_CONTROL_PLANE:
            return None                    # `charter --version` outside a plane, etc.
        from . import util
        root = config.ROOT
        gitdir = util.git_dir(root)
        if gitdir is None:
            # Two cases, one answer. `charter init` in a fresh directory does not run
            # `git init` (that is the README's 60-second path), and a plane created in a
            # SUBDIRECTORY of some other repo has no `.git` of its own either. Asking git
            # about the directory anyway would answer for the surrounding repo, and
            # reporting a tree the user never named as "the plane root" is worse than
            # saying nothing.
            return None
        # Its own `_repo_states` call rather than joining `render`'s scan list, because
        # that list also feeds `glstate.read_for`/`maybe_spawn`: a directory in it has
        # forge state fetched for it in the background. The root has no row, so that
        # would be network work for something nothing can display. The TTL cache is
        # shared, so the cost here is one `git status` per few seconds, not per render.
        state = _repo_states([root]).get(root) or {}
        branch = _branch(root)
        default = _default_branch(gitdir)

        bits = []
        # `tracked_dirty`, never `dirty` — see this function's docstring and `_run_state`.
        # `.get` defaults to False rather than to `dirty`: a cache entry written seconds
        # ago by an older charter has no such key, and a few silent seconds is a better
        # failure than a warning derived from the notion `doctor` disagrees with.
        if state.get("tracked_dirty"):
            # The word, not the repo table's `*`. A marker reads as a marker beside a
            # column of them; this line has no siblings to be read against.
            bits.append(f"{_YELLOW}dirty{_R}")
        if _head_detached(gitdir):
            # Reported on its own terms and WITHOUT a default to compare against, unlike
            # the branch case below. Detachment is a fact about HEAD alone, it is the
            # most alarming of these states rather than the least — a commit made here is
            # unreachable the moment anything else is checked out — and staying silent
            # about it on a plane whose default cannot be discovered would be the wrong
            # way round. `doctor`'s `check_plane_root` reports it the same way and on the
            # same terms; the words match so the two cannot be read as different findings.
            bits.append(f"{_YELLOW}detached HEAD{_R}")
        # `?` is `_branch`'s "HEAD unreadable" — nothing to compare, so nothing to claim.
        # A detached HEAD never reaches here: it is already said above, and saying it
        # again as "on 1a2b3c4, not main" would dress the worst state up as an ordinary
        # branch you happened to be on.
        elif default and branch not in ("?", default):
            bits.append(f"{_DIM}on{_R} {branch}{_DIM}, not{_R} {default}")
        if not bits:
            return None

        # Order IS truncation order, and the words that must survive a narrow pane are
        # the ones naming what the line is about: `plane root` first, then which root.
        # Nothing but ASCII and `⚠` — the same glyph the alerts above already use, so
        # this adds no character whose width some font gets to disagree about.
        sep = f"{_DIM} · {_R}"
        return (f"{_YELLOW}⚠{_R} {_DIM}plane root{_R} {root.name}{sep}"
                + sep.join(bits)
                + f"{_DIM} · work belongs in a workspace clone{_R}")
    except Exception:
        # The status line's one hard contract. A root that cannot be read costs this
        # line and nothing else.
        return None


def _nested_under() -> Path | None:
    """The OUTER control plane whose ``workspaces/`` contains this one, if any.

    ``root.find_root`` takes the innermost ``charter.toml`` — the git/cargo/npm contract,
    and the right one. But a plane clones product repos into its workspaces, and a product
    repo may itself carry a ``charter.toml`` — `charter init` is run inside existing repos.
    So `cd`
    into ``workspaces/<ws>/<repo>`` and the active plane silently becomes a different one:
    different personas, a different vault, and a memory write landing somewhere the user
    did not choose. Nothing said so.

    Only walks upward comparing paths — no config is parsed for the ancestors beyond the
    marker's presence, because this runs on every render.

    The rule itself lives in :func:`charter.root.enclosing_plane`, which `doctor` also uses.
    It was implemented twice, here and there, which is how two surfaces come to disagree
    about what "nested" means — and the one that disagrees is always the one nobody was
    looking at. This wrapper keeps the render-path contract (never raise) and nothing else.
    """
    try:
        from . import root as _r
        return _r.enclosing_plane(config.ROOT)
    except Exception:
        return None


def _root_marker() -> str:
    from . import root as _r
    return _r.MARKER


def _session_strip(payload: dict, sid: str | None) -> str:
    """The bottom zone: everything true of **this session**, on one line.

    ``ctx``/``cache`` (context window + prompt-cache health) sit here rather than in
    the top line because they describe the session, not the workspace — the top line
    answers *where am I*, and mixing a session gauge into it was most of why the old
    header read as unrelated items in a row.

    Every gauge here carries a word; the one glyph on the strip is ``⚡``, and it means
    exactly what it means on a persona chip — a dispatch is running. That is the whole
    reason the cache rate gave the bolt up: a fact rendered in two places has to read
    the same in both, and a second meaning beside it on one line erases both.

    Empty string when there is nothing to report (a fresh session or one just past
    ``/compact`` has no usage yet): the brand alone does not justify a row.
    """
    parts = [*_context_gauge(payload), *_session_news(sid)]
    return f"{_DIM} · {_R}".join(parts) if parts else ""


def _brand() -> str:
    """`⬢ charter x.y.z`, plus `↑ a.b.c` when a newer release is cached.

    Read-only and offline: the version is this process's own, and the "newer?"
    answer comes from a cache another process fills. `update.maybe_spawn` may fork
    a detached child, but nothing here ever waits on the network — the status line
    renders on every turn.
    """
    from . import __version__, update
    out = f"{_DIM}⬢ charter {__version__}{_R}"
    try:
        update.maybe_spawn()
        newer = update.newer_than(__version__)
    except Exception:
        newer = None
    if newer:
        out += f" {_YELLOW}↑{newer}{_R}"
    return out


def _boxed(body: str, width: int) -> str:
    """Frame the whole status line: ``+---+`` above and below, ``|`` down each side.

    Applied last, over finished lines, so the box cannot perturb the column maths that
    ran inside it.

    Box-drawing here, ASCII in the tree, and the split is not arbitrary. These
    characters are East-Asian *Ambiguous*: a terminal may draw them one cell or two.
    What breaks a layout is not width itself but width that differs *between rows* —
    every row carries exactly one left border, one divider and one right border, so a
    terminal drawing them wide shifts every row identically and the columns stay true.
    The tree markers are the opposite: `|- ` on a repo row, nothing on the header, so an
    unexpected width there moves one row and not its neighbour. That is the asymmetry
    that caused the original drift, and it stays ASCII.

    The frame earns its two rows by being a *ruler*: with a right edge, a row whose
    content renders wider than ``tui.width`` believes pushes its own ``|`` past the
    others, so drift becomes a thing you can see and point at instead of a mystery. The
    left edge and both headers stay honest regardless, since everything up to a row's
    last alignment point is ASCII.

    Silently returns the body unchanged if the pane is too narrow to frame — the box is
    decoration and must never cost content.
    """
    try:
        inner = width - 4                      # "│ " + content + " │"
        if inner < 20:
            # Unframed: a rule has no side borders to join, so it is dropped rather than
            # printed. The sentinel must never reach a terminal.
            return "\n".join(ln for ln in body.split("\n") if ln != _RULE_LINE)
        top = f"{_DIM}┌{'─' * (width - 2)}┐{_R}"
        bot = f"{_DIM}└{'─' * (width - 2)}┘{_R}"
        rule = f"{_DIM}├{'─' * (width - 2)}┤{_R}"
        out = [top]
        for ln in body.split("\n"):
            if ln == _RULE_LINE:
                # Drawn here, not in layout, so it is always exactly as wide as the top
                # and bottom borders — a rule that disagreed with them would be the most
                # visible possible defect.
                out.append(rule)
                continue
            ln = tui.truncate(ln, inner)
            out.append(f"{_DIM}│{_R} {ln}{' ' * max(0, inner - tui.width(ln))} {_DIM}│{_R}")
        out.append(bot)
        return "\n".join(out)
    except Exception:
        return body


def _zone_rules(body: str, has_strip: bool) -> str:
    """Insert the zone dividers: one under the workspace line, one above the strip.

    Applied to the finished body rather than threaded through the layout branches, which
    works because every branch lays out the same two anchors — the workspace summary is
    always the first line, and the session strip, when there is one, is always the last.
    Splicing here keeps a divider out of `_columns`, where it would be padded and
    truncated as though it were content.

    No divider above a strip that does not exist: an empty session (or one just past
    `/compact`) renders no strip, and a rule there would separate the tree from nothing
    but the bottom border.
    """
    lines = body.split("\n")
    if len(lines) < 2:
        return body                     # nothing to divide
    out = [lines[0], _RULE_LINE, *lines[1:]]
    if has_strip:
        out.insert(len(out) - 1, _RULE_LINE)
    return "\n".join(out)


def _with_brand(body: str, width: int) -> str:
    """Right-align the brand on the last line, if it fits without crowding.

    Appended after layout rather than inside it: the columns are width-constrained
    and threading a right-hand chunk through them would push real content out.
    Dropped entirely on a narrow pane — branding must never cost a repo row.

    ``_BRAND_MARGIN`` is why the check is not simply "does it fit in ``width``". A real
    session rendered ``⬢ charter 0.10…``, which this function cannot produce — it fits
    or it drops, it never truncates. So the crop came from outside: the pane gave one
    column less than ``COLUMNS`` promised. Rather than guess the exact reserve, keep a
    margin, so an off-by-one anywhere (the pane's, or a terminal that draws ``⬢`` two
    cells wide) costs the brand instead of shearing it into nonsense.
    """
    try:
        brand = _brand()
        lines = body.split("\n")
        if not lines:
            return body
        last = lines[-1]
        if last == _RULE_LINE:
            # Defensive: `_zone_rules` never leaves a divider last, but welding the brand
            # onto one would produce a line that is part box-drawing and part text.
            return body
        used, need = tui.width(last), tui.width(brand)
        if used + need + _BRAND_GAP + _BRAND_MARGIN > width:
            return body                      # no room: content wins
        lines[-1] = last + " " * (width - used - need) + brand
        return "\n".join(lines)
    except Exception:
        return body


def render(payload: dict | None = None) -> str:
    """FINDING M9: the module docstring promises this NEVER crashes — that guarantee
    lives entirely in this function's two `try/except` blocks, so EVERY step that can
    fail must sit inside one of them. Before this, `_repo_rows`/`_persona_chips` (and
    the summary/pin/reinit assembly) ran in the GAP between the two — a bad repo row (or
    a broken persona) crashed `render()` itself, the exact thing this docstring says can
    never happen. Everything through `chips` now shares the FIRST try/except, so a
    failure anywhere in data-gathering falls back to the same minimal string."""
    payload = payload or {}
    try:
        active, src = _active(payload.get("session_id"),
                              (payload.get("workspace") or {}).get("current_dir"))
        nws = _count_workspaces()
        # The active workspace's clones, and only those. The plane's own tree is
        # deliberately not a row: it is the plane, not a repo you work in, and drawing it
        # beside the clones is what invited editing it (docs/adr/0008).
        dirs = _repo_trees(active)
        avail = _available()
        nv = _vaults()
        cur = _current(payload)
        # Render a hair under COLUMNS (which Claude Code sets to the pane width) so a
        # line never fills the last column (which the terminal would wrap).
        frame_w = max(24, tui.term_width(default=80, floor=24) - _SAFETY)
        # Everything below lays out inside the frame, so it gets the pane minus the
        # box's own chrome ("| " each side). `_boxed` re-widens to `frame_w` at the end.
        width = max(24, frame_w - 4)
        # Worktrees drawn as full rows need the same per-directory data their repo does,
        # so they are scanned alongside it — a row can only show what `scan` gathered.
        detail_wts = _detail_worktrees(active, dirs)
        scan = [*dirs, *detail_wts]
        states = _repo_states(scan)
        branches = {d: _branch(d) for d in scan}
        from . import glstate
        gl = glstate.read_for(scan, branches)
        glstate.maybe_spawn(scan, active)

        pin = f"{_YELLOW}*{_R}" if src == "$CHARTER_WORKSPACE" else ""
        # Reinit tip sits right after the name so it survives truncation on narrow panes.
        # Nothing informational goes in front of it: it is the one item on this row that
        # reports something BROKEN, and it carries the command that fixes it. A pane with
        # room for one item and not two must spend that room on the problem.
        reinit = f"{_YELLOW}⚠ reinit: {_BOLD}charter ws reinit{_R}" if _stale_structure(active) else None
        # Zone 1 — WHERE I am. Identity and navigation only: which workspace is active,
        # what it still means to do, and how many others exist to switch to. Everything
        # that used to ride along here (repo count, vault count, ctx/cache) described
        # something else and now sits with the thing it describes.
        #
        # Open todos are a property of the ACTIVE WORKSPACE, so the layout's one rule —
        # a count lives next to what it counts — puts them here rather than on the
        # session strip (they outlive the session; that is the point of the store) or in
        # the repo column (they are not about a repo). Beside the name rather than on a
        # row of its own, because a row is spent on every single turn and what it would
        # carry is usually one digit.
        #
        # It follows the reinit tip rather than preceding it. This row's order IS its
        # truncation order, and a warning outranks information: on a pane wide enough
        # for the tip or the count but not both, the item naming a broken structure has
        # to be the one that survives. The count pays nothing for that in practice —
        # reinit renders only when the on-disk layout is actually stale, so essentially
        # every turn reads `⬢ <name> · todo N · ws M`, with the count still against the
        # name whose todos it counts.
        #
        # Zero renders NOTHING, the same discipline `_session_news` keeps: a `todo 0`
        # present every turn is furniture within a day, and then a real `todo 7` in that
        # spot draws no more attention than the zero did. Presence is the signal.
        #
        # A plain word, no glyph. `tui.width` only knows what the Unicode tables claim,
        # and this layout has twice paid for a character a font drew wider than that
        # (see the header comments below) — a decoration here could only cost columns,
        # since the label already reads. `todo` singular because it is exactly the
        # subcommand that shows them, `charter ws todo`, so the label doubles as the way
        # to read the detail — the same thing `ws N` does for `charter ws`.
        ntodo = _todo_count(active)
        summary = f"{_DIM} · {_R}".join(filter(None, [
            f"{_CYAN}⬢{_R} {_BOLD}{active}{_R}{pin}",
            reinit,
            f"{_DIM}todo{_R} {ntodo}" if ntodo else None,
            _piece_summary(active),
            f"{_DIM}ws{_R} {nws}",
        ]))

        sid = payload.get("session_id")
        repo_lines = [r.render(_LEFT_W)[0]
                      for r in _repo_rows(dirs, active, cur, states, branches, gl,
                                          detail_wts)]
        chips = _persona_chips(sid)
    except Exception:
        return f"{_CYAN}⬢{_R} charter"

    try:
        # Zone 2 — one header row, one head per column, each stating what its own
        # column holds. `repos N/M` used to sit in the top line and `personas`/`vaults`
        # in the right column, so two structurally identical facts rendered in two
        # different places; a count now always sits next to what it counts.
        shared_badge = _mem_badge(_mem_count("_", shared=True),
                                  _mem_count("_", shared=True, ephemeral=True, session=sid))
        # NEITHER header carries a decorative glyph, deliberately, and the reason is the
        # same for both: a header is the only row of its kind, so a glyph on it is
        # exercised by nothing else. `tui.width` trusts the Unicode tables; a font that
        # draws a character wider than its table claims shifts everything after it on
        # that row — and a header's row is the one row with no neighbour to reveal the
        # drift. Content rows are safe by contrast precisely because they repeat: the
        # thirteen chip rows line up with each other, which is what proves `◆`/`○`, and
        # the repo rows prove `├ ─ │ ⑂ ✓ ✗ ·`.
        #
        # Both mistakes shipped. A `◫` on this line pushed the whole right-hand column
        # one space over. Then a `◈` on the personas header — past the column's own
        # alignment point, so the divider and bullets still lined up perfectly — pushed
        # the *word* "personas" one space right of every chip title below it, which is
        # the tell: the bullets agreed and the titles did not.
        #
        # So: labels only up here. Decoration lives on the content rows, where a
        # sibling would expose it.
        # `/<avail>` is "of this many in the inventory", so it is only printed when there
        # IS an inventory. A monorepo plane never runs `discover` — its one repo is
        # already here — and rendering `repos 1/0` there states the opposite of the truth.
        # A fleet plane before its first `discover` gets the same honest bare count.
        left_head = f"{_DIM}{_HEAD_PAD}repos{_R} {len(dirs)}" + (
            f"{_DIM}/{avail}{_R}" if avail else "")
        header = f"{_DIM}{_HEAD_PAD}personas{_R} {len(chips)}" + (
            f"{_DIM} · vaults{_R} {nv}" if nv else "") + (
            f"{_DIM} · shared{_R}{shared_badge}" if shared_badge else "")
        strip = _session_strip(payload, sid)
        alerts = _alerts(active)
        # `left_head` means the left column is never empty, so two columns no longer
        # require a cloned repo — a fresh workspace still gets the grouped layout with
        # an honest `◫ repos 0/38`.
        if chips and width >= _LEFT_W + _RIGHT_MIN_W:
            # Summary on its own full-width line; below it, two columns — repos left,
            # personas right. The header sits beside a full repo row (pairing it with
            # the short summary row misaligned it on some terminals), so it lines up
            # with the chips. When personas outnumber repos, continue the repo tree
            # with │ so no row is blank on the left (Claude Code collapses those to col 0).
            # News goes in the rows the repo tree already leaves blank — free
            # vertically, so it costs no width and works at 80 columns.
            left = [left_head, *repo_lines]
            right = [header, *chips]
            if len(right) > len(left):
                # The tree keeps going below the last repo, so its `└─` must become
                # `├─`. Find the row that actually carries the elbow — `left[-1]` is
                # not it whenever the last repo emitted a worktree summary line
                # underneath, which left a `└─` sitting above a column of `│`.
                for i in range(len(left) - 1, -1, -1):
                    if _TREE_END in left[i]:
                        left[i] = left[i].replace(_TREE_END, _TREE_MID, 1)
                        break
                while len(left) < len(right):
                    left.append(f"  {_DIM}{_TREE_PIPE}{_R}")
            rows = [tui.truncate(summary, width), _columns(left, right, width)]
            rows += [tui.truncate(a, width) for a in alerts]
            if strip:
                rows.append(tui.truncate(strip, width))
            body = "\n".join(rows)
        elif chips:
            body = _columns([summary, left_head, *repo_lines, header, *chips,
                             *alerts, *([strip] if strip else [])], None, width)
        else:
            body = _columns([summary, left_head, *repo_lines,
                             *alerts, *([strip] if strip else [])], None, width)
        # After layout, before the frame: the dividers are furniture, and letting them
        # through `_columns` would have them padded and cropped as content.
        body = _zone_rules(body, has_strip=bool(strip))
    except Exception:
        # Never crash the status line if layout fails — plain truncated stack.
        plain = [summary, *repo_lines]
        p = _persona_line()
        if p:
            plain.append(p)
        body = "\n".join(tui.truncate(ln, width) for ln in plain)

    # Brand first (it right-aligns against the content width), then the frame around it.
    return _boxed(_with_brand(body, width), frame_w) + "\n"


def _columns(left_lines: list[str], right_lines: list[str] | None, width: int) -> str:
    """Compose one or two columns with the stdlib tui kit, clamped to *width*
    (overflow cropped with …, never wrapped; no trailing whitespace)."""
    if right_lines:
        node: tui.Node = tui.Columns([(list(left_lines), _LEFT_W),
                                       (list(right_lines), None)], gap=_COL_SEP)
    else:
        node = tui.Stack(*left_lines)
    return "\n".join(node.render(width))


#: How often the ambient render repaints. The same cadence charter writes into Claude
#: Code's `statusLine.refreshInterval`, so the plane state ages at one rate whichever
#: harness you are looking at it through.
WATCH_INTERVAL = 10.0

_HOME_CLEAR = "\033[H\033[J"
_HIDE_CURSOR, _SHOW_CURSOR = "\033[?25l", "\033[?25h"


def watch(interval: float = WATCH_INTERVAL) -> int:
    """Repaint the plane state in place until Ctrl-C. ``charter statusline --watch``.

    The remedy for a harness with no status bar (ADR 0015). Claude Code renders the line
    every turn because it has a socket for one; opencode has none, and Codex's
    `tui.status_line` takes built-in segments rather than a command. This needs neither —
    a spare terminal, no multiplexer, and the same render on every harness.

    **What it cannot show, and says so.** There is no session payload here, so the token
    and context columns are blank. A render that looks like the real thing while silently
    omitting a column teaches the reader to trust a number that is not on the screen.

    The cursor is hidden while painting and restored on the way out, including on Ctrl-C:
    an ambient display that leaves the operator's terminal broken is a worse failure than
    the one it set out to fix. A render that raises is drawn as a single line rather than
    ending the loop, for the same reason `main` guards it — this process is meant to sit
    there for hours.
    """
    out = sys.stdout
    out.write(_HIDE_CURSOR)
    try:
        while True:
            try:
                body = render({})
            except Exception:
                body = f"{_CYAN}⬢{_R} charter — render failed; still watching"
            out.write(f"{_HOME_CLEAR}{body}\n")
            out.write(f"{_DIM}no session attached — token and context columns are blank; "
                      f"this is the plane, not the session. Ctrl-C to stop.{_R}\n")
            out.flush()
            time.sleep(interval)
    except KeyboardInterrupt:
        return 0
    finally:
        out.write(_SHOW_CURSOR)
        out.flush()


def main(argv=None) -> int:
    ap = argparse.ArgumentParser(prog="charter statusline", add_help=True)
    ap.add_argument("--watch", action="store_true",
                    help="repaint the plane state in place until Ctrl-C — an ambient "
                         "status line on a harness that has no bar of its own.")
    ap.add_argument("--interval", type=float, default=WATCH_INTERVAL,
                    help=f"seconds between repaints with --watch (default {WATCH_INTERVAL:g}).")
    # `argv or []`, never None: `parse_args(None)` reads sys.argv, and this function is
    # called programmatically (and by the crash-guard test) with no arguments at all —
    # where inheriting the parent process's flags would turn a render into a usage error.
    a = ap.parse_args(argv or [])
    if a.watch:
        return watch(interval=a.interval)
    try:
        payload = json.load(sys.stdin)
    except Exception:
        payload = {}
    # Defense in depth (FINDING M9): `render()` itself is guarded end-to-end, but this
    # is the outermost boundary of the whole subprocess — it must never crash even if a
    # future bug (or a monkeypatch, or an import-time surprise) reaches past render's
    # own guard, since a raise here takes the entire status-line subprocess down.
    try:
        out = render(payload)
    except Exception:
        out = f"{_CYAN}⬢{_R} charter"
    print(out)
    return 0
