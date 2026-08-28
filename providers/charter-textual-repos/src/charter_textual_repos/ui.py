"""The Textual application — one app class, driven two ways.

Both charter components in this package build the same :class:`ReposApp`. The adapter
runs it with the **headless** driver on a background thread and reads its composited
screen back as lines; the live component runs it with the **linux** driver on the pane's
own tty, on the main thread, where it owns the terminal.

**One app class for both on purpose.** The experiment's question is whether the
`Component` contract survives contact with a real widget framework, and two different
widget trees — one for the shape charter can host, one for the shape it cannot — would
answer a question nobody asked. What differs between the two components is entirely the
DRIVER and who owns the loop, which is exactly the axis the finding sits on.
"""

from __future__ import annotations

import threading
import time

from textual.app import App, ComposeResult
from textual.binding import Binding
from textual.containers import Vertical
from textual.widgets import DataTable, Static

from .rows import Row, heading, rows_of

#: The columns, in charter's own left-to-right order: name, branch, markers, CI, change.
COLUMNS = ("repo", "branch", "", "ci", "change")


class ReposApp(App):
    """Charter's repo table as a Textual `DataTable`.

    The CSS is deliberately small and deliberately *dark-only*. Charter's own chrome is
    theme-safe by default (`instance.FRAME_FIELDS`' ``chrome`` note: a default that can
    make an existing working frame worse on upgrade must be opt-in), and Textual's
    default theme is not — every widget paints its own background over the operator's
    terminal colours. That difference is not a styling detail: it is the reason a Textual
    panel cannot be turned on by default in a frame that promises not to repaint someone's
    terminal in colours they did not choose.
    """

    CSS = """
    Screen { background: $surface; layout: vertical; }
    #head { dock: top; height: 1; background: $panel; color: $text; padding: 0 1; }
    #foot { dock: bottom; height: 1; background: $panel; color: $text-muted;
            padding: 0 1; }
    DataTable { height: 1fr; }
    DataTable > .datatable--cursor { background: $accent; }
    """

    BINDINGS = [
        Binding("q", "quit", "quit", show=True),
        Binding("j,down", "cursor_down", "down", show=False),
        Binding("k,up", "cursor_up", "up", show=False),
        Binding("r", "note_refresh", "refresh", show=False),
    ]

    def __init__(self, *, gathered=None, note: str = "") -> None:
        super().__init__()
        self._pending = gathered or {}
        self._note = note
        #: Every pointer event the app has seen, for the mouse measurement. A counter
        #: rather than a log: the question is whether events arrive at all, and a log
        #: would be a second thing to keep.
        self.clicks = 0
        self.scrolls = 0
        self.keys = 0
        #: Set once the widget tree is mounted and has been composited at least once.
        #: `App.is_running` is true several milliseconds earlier (measured: running at
        #: 5.6 ms, first non-blank composite at 18.2 ms), and a repaint that lands in that
        #: window reads a screen of blank strips — a pane that is convincingly empty
        #: rather than obviously broken, which is the failure mode #512 was about.
        self.ready = threading.Event()

    # -- composition -------------------------------------------------------- #

    def compose(self) -> ComposeResult:
        yield Static("", id="head")
        with Vertical():
            yield DataTable(id="table", cursor_type="row", zebra_stripes=False)
        yield Static("", id="foot")

    def on_mount(self) -> None:
        self.apply(self._pending)
        self.set_interval(1.0, self._retitle)
        # One loop turn after mount, so the compositor has arranged the tree at least
        # once before anything copies its screen out.
        self.call_after_refresh(self.ready.set)

    def _columns(self, table: DataTable) -> None:
        """Declare the columns once, from wherever asks first.

        Not in `on_mount`, and that is a bug this experiment hit and is worth keeping the
        fix visible for: the adapter drives the app from *another thread*, so a repaint
        can land between `compose` (the `DataTable` exists) and `on_mount` (the columns
        exist). `add_row` then raises ``More values provided than there are columns``,
        `Registry.draw` catches it, and the pane reads `textual.repos failed to draw`.
        Charter contained it exactly as §4b promises — the provider still shipped a broken
        pane.
        """
        if not table.columns:
            for col in COLUMNS:
                table.add_column(col or " ", key=col or "marks")

    # -- data --------------------------------------------------------------- #

    def apply(self, gathered) -> None:
        """Replace the table's contents with *gathered* — one `ctx.gather` snapshot.

        Rebuilt whole rather than diffed, which is charter's own choice one layer up
        (`frame/panel.py`: "Repaints whole, never diffed: a five-row pane is a few hundred
        cells"). A `DataTable` of fourteen rows is the same argument, and a diff would be
        a second model of the plane's state living in this package with nothing keeping it
        honest.
        """
        self._pending = gathered or {}
        try:
            table = self.query_one("#table", DataTable)
        except Exception:
            return                      # before compose; `on_mount` applies it
        self._columns(table)
        cursor = table.cursor_row
        table.clear()
        for r in rows_of(self._pending):
            table.add_row(*self._cells(r))
        if cursor:
            try:
                table.move_cursor(row=min(cursor, table.row_count - 1))
            except Exception:
                pass
        self.query_one("#head", Static).update(heading(self._pending))
        self._retitle()

    def _cells(self, r: Row) -> tuple[str, ...]:
        lead = "▸ " if r.current else ("  ╰ " if r.piece else "  ")
        return (f"{lead}{r.name}", r.branch, r.marks, r.ci, r.change)

    def _retitle(self) -> None:
        """The footer: how old this snapshot is, and what the pointer has seen.

        **The age is on screen because it is the finding.** A component's `render(ctx)`
        is handed one snapshot and has no way to ask for another (`frame/ctx.py`:
        "A component never gets a way to fetch a fresher one: refreshing is the frame's
        decision, on the frame's clock"). In the adapter that is invisible, because
        charter calls `render` again with a new one. In the live component `render` never
        returns, so nothing ever calls it again, and this clock is the only thing on
        screen that admits it.
        """
        at = self._pending.get("gathered_at") if self._pending else None
        age = f"{time.time() - float(at):.0f}s" if at else "?"
        parts = [f"snapshot {age} old", f"clicks {self.clicks}",
                 f"scroll {self.scrolls}", f"keys {self.keys}"]
        if self._note:
            parts.append(self._note)
        try:
            self.query_one("#foot", Static).update("  ·  ".join(parts))
        except Exception:
            pass

    # -- events, counted so the tmux measurement has something to read -------- #

    def on_mouse_down(self, _event) -> None:
        self.clicks += 1
        self._retitle()

    def on_mouse_scroll_down(self, _event) -> None:
        self.scrolls += 1
        self._retitle()

    def on_mouse_scroll_up(self, _event) -> None:
        self.scrolls += 1
        self._retitle()

    def on_key(self, _event) -> None:
        self.keys += 1
        self._retitle()

    def action_cursor_down(self) -> None:
        self.query_one("#table", DataTable).action_cursor_down()

    def action_cursor_up(self) -> None:
        self.query_one("#table", DataTable).action_cursor_up()

    def action_note_refresh(self) -> None:
        self.refresh(repaint=True, layout=True)
