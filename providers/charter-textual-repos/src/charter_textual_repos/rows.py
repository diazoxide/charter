"""The repo table's DATA, derived from ``ctx.gather`` and from nothing else.

This module is the honest half of the experiment. Everything here is computed from the
mapping `charter.frame.ctx` serves under the name ``gather`` — the same snapshot
`charter/frame/slots.py:_table_lines` draws from — and this package imports no part of
charter at all. If a column below is wrong, the fix is in this file; if a column below is
*missing*, the fix is in charter's `gather.scan`, and that is a finding rather than a
patch.

**What ``gather`` serves, exactly** (`charter/frame/gather.py:_entry` and `scan`):

``name``, ``branch``, ``dirty``, ``tracked_dirty``, ``ahead``, ``behind``, ``ci``,
``change``, ``sigil``, ``current``, ``worktree_count`` per repo row, plus a top-level
``workspace``, ``current_repo``, ``repos``, ``worktrees``, ``todos``, ``todo_count`` and
``gathered_at``.

**What it does not serve, and what that cost this widget.** `slots._table_row` draws a
*presence* column — "who else is standing in this tree" — and cannot fill it from the
gather either; its own docstring concedes the cost. The Textual version has the same
hole for the same reason, which is the correct outcome: the seam held, and a provider
sees exactly what charter's own renderer sees. There is no back door here to make the
column appear.
"""

from __future__ import annotations

from dataclasses import dataclass

#: What each CI state is drawn as. Charter's own `statusline._CI_MARK` is the model; the
#: glyphs are re-spelled here rather than imported, because importing `charter.statusline`
#: from a provider would make this package depend on charter's internals — which is the
#: opposite of what the entry-point seam is for. The cost is stated: a change to charter's
#: CI vocabulary does not reach this table, and a provider has no way to be told.
CI_MARK = {
    "passed": "ok",
    "failed": "fail",
    "running": "run",
    "pending": "…",
    None: "",
}

#: The dirty/ahead/behind marker column, in charter's own order: worktree dirt, then
#: unpushed, then unpulled.
def markers(row: dict) -> str:
    """The one-cell-per-fact marker string for *row* — ``*`` dirty, ``↑n``, ``↓n``."""
    out = []
    if row.get("dirty"):
        out.append("*" if row.get("tracked_dirty") else "?")
    if row.get("ahead"):
        out.append(f"↑{int(row['ahead'])}")
    if row.get("behind"):
        out.append(f"↓{int(row['behind'])}")
    return " ".join(out)


@dataclass(frozen=True)
class Row:
    """One line of the table, already reduced to text.

    A frozen dataclass rather than the raw dict, because `ctx` hands a provider a
    read-only *view* of the snapshot whose repo rows are still mutable dicts
    (`ctx.SERVES`' own docstring says the containment is shallow and why). Copying the
    six fields this widget draws is cheap, and it means nothing in this package can write
    into the snapshot the next component in the repaint is about to read.
    """

    name: str
    branch: str
    marks: str
    ci: str
    change: str
    current: bool
    piece: bool


def _change(row: dict) -> str:
    """The open change cell — ``!123``, or the forge's own sigil where it has one."""
    change = row.get("change")
    if not change:
        return ""
    return f"{row.get('sigil') or '!'}{change}"


def _row(raw: dict, *, piece: bool) -> Row:
    name = str(raw.get("name") or "?")
    branch = str(raw.get("branch") or "?")
    pieces = int(raw.get("worktree_count") or 0)
    if pieces and not piece:
        name = f"{name} ⑂{pieces}"
    return Row(name=name,
               branch="" if piece and branch == raw.get("name") else branch,
               marks=markers(raw),
               ci=CI_MARK.get(raw.get("ci"), str(raw.get("ci") or "")),
               change=_change(raw),
               current=bool(raw.get("current")),
               piece=piece)


def rows_of(gather) -> list[Row]:
    """Every repo, then every piece, as :class:`Row` — the order charter's table uses.

    Repos first and pieces after, matching `slots._table_lines`' budget order ("a short
    pane loses DETAIL rather than losing a repo"). The ranking `statusline._pick_rows`
    applies when the pane is too short for every row is deliberately NOT reimplemented
    here: it is charter's, it is not reachable from `ctx`, and guessing at it would
    produce the unranked slice its own docstring records shipping once — thirteen clean
    repos on `main` shown, and the one dirty repo you were standing in hidden.

    So this returns everything and lets the *widget* scroll, which is the one thing a
    widget framework can do that a line-based renderer cannot. That is the strongest
    argument the experiment found for Textual, and it is worth being explicit that it
    only pays off in the takeover mode where scrolling is reachable.
    """
    if not gather:
        return []
    out = [_row(r, piece=False) for r in (gather.get("repos") or [])]
    out += [_row(w, piece=True) for w in (gather.get("worktrees") or [])]
    return out


def heading(gather) -> str:
    """The pane's own heading — ``repos N`` over the workspace name."""
    n = len(gather.get("repos") or []) if gather else 0
    ws = (gather.get("workspace") or "") if gather else ""
    return f"repos {n}" + (f"  ·  {ws}" if ws else "")
