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
SLOTS = {"top": _top, "bottom": _bottom}


def unimplemented(configured) -> list[str]:
    """Which of *configured* charter sizes and accepts but has no renderer for.

    `left`/`right` today: `instance.FRAME_SLOTS` accepts both and `layout.SLOT_SIZE`
    sizes both, while :data:`SLOTS` implements neither. Three callers need exactly this
    list and must agree — `commands_frame.cmd_launch` (to skip splitting a pane that
    would be permanently dead under `remain-on-exit on`), `commands_frame.frame_ready`
    (`--probe`) and `doctor.check_frame` (both to SAY so, which is the only place it is
    said at all now) — so the question is answered here, next to the registry that
    answers it, rather than three times over.
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
