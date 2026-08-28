"""The pane this process was given.

**A panel's pane IS `sys.stdout`.** The descriptor it paints into is the same one it
measures its rectangle from, so that name is not an output stream here — it is the
rectangle. Which makes it exactly the wrong thing to ask, because what is bound to it is a
mutable global that any library the process imports may replace. Textual's
`redirect_stdout` installs a `_PrintCapture` answering `isatty() -> True` and
`fileno() -> -1`; `rich`, `click`, `tqdm`, `colorama`, a progress bar and a logging handler
installed at import all reach for the same name. Measured on `main` in a real 150x10 pane
(#605, #606)::

    pane:            slots._width() = 150   slots._height() = 10
    after a rebind:  slots._width() = 80    slots._height() = 24   colour_ok() = True

A correct first paint, then blank on every repaint, and a frame laid out for a rectangle
nobody has. **Nothing raised and nothing was logged**, so `registry.Registry.draw`'s catch
never fired: §4b's promise that a broken component costs its own pane was not violated, it
was evaded — which is worse than a crash, because a crash is at least a report.

**The property is "the pane this process was given"; the spelling is "whatever
`sys.stdout` is bound to at the instant of the ask".** Asking the spelling is the ninth
instance of that mistake closed this month (#547, #558, #537, #498, #577, #594) and the
only silent one. So the pane is claimed ONCE, by the process that owns one, above anything
a stranger wrote: `panel.run` claims it before `builtins.build()`, which is where
`registry.Registry.place` runs `importlib.import_module` on a provider's module for the
first time. That ordering is `tests/_ttyguard`'s, one layer over — it installs ABOVE the
import that pulls charter in, because `util._USE_COLOR` is `sys.stderr.isatty()` evaluated
at THAT import (#545/#546). A claim is what "above" means when the thing to get above is a
provider rather than an import of charter itself.

**Not at import of this module, and the reason is that a claim is scoped work.** Charter
is one CLI: the same interpreter runs `charter panel`, `charter statusline` and a hook,
and only the first of those is handed a pane. Claiming at import would make every one of
them hold a "pane" that is a pipe, and would make the claim outlive the panel that took it
— `panel.run(once=True)` is called in-process by tests, exactly as `_install_sigwinch` is,
and that function's own `finally` is the precedent for putting back what was replaced.

**What a claim does not defend against, stated rather than implied.** It holds the stream
OBJECT, so a library that rebinds `sys.stdout` cannot move charter's pane out from under
it. A library that `dup2`s over fd 1, or closes the stream, has genuinely taken the
descriptor away, and the only answer to that would be charter holding an `os.dup` of its
own for the life of every panel — a second open fd, forever, against something no reported
case does. Those failures are also LOUD: the write raises, and `panel.run` paints why and
holds the pane. Silence is what this module is about.

**And a size is a rectangle or it is nothing.** :func:`size` answers `None` for a pane it
could not measure *and* for one that answers zero, which is #594's judgement made in the
one place that has a single source to make it about. That zero is ordinary rather than
exotic: a pty created without a window size reports zero columns until somebody calls
`TIOCSWINSZ`, which is what `os.openpty` hands you, and so what tooling, some CI shells and
a terminal attached before its size is negotiated all hand you. `slots._width` and
`slots._height` are the halves that carry a stated fallback for a caller that has to draw
anyway — the same split `commands_frame._measure_window`/`_window_size` already draws, and
for the same reason it draws it: "charter could not read this" must never be spelled the
same way as a real 80x24.
"""

from __future__ import annotations

import os
import sys

#: The pane this process claimed, or ``None`` for a process that has claimed none.
#:
#: A module-level global because the thing it describes is one: a process is given one
#: pane, and the panel that owns it, the renderer measuring it and the chrome deciding
#: whether it takes colour must all be talking about that same rectangle — which is the
#: entire defect (#606) said the other way round.
_CLAIMED = None

#: What every question here treats as "this stream cannot say". A `StringIO`'s `fileno`
#: raises `io.UnsupportedOperation`, which is both an `OSError` and a `ValueError`; a
#: closed stream raises `ValueError`; a library's stand-in may have no such attribute at
#: all; and a real descriptor with no tty behind it raises `OSError`. The same set
#: `tests/_ttyguard`'s `says_it_is_a_terminal` settled on, and named once so the two
#: questions below cannot come to disagree about which stream is answerable.
_CANNOT_SAY = (AttributeError, OSError, ValueError)


def claim():
    """Take what `sys.stdout` is bound to NOW as this process's pane, and answer whatever
    claim this replaces so the caller can put it back.

    Called once, at the top of the work that owns a pane, before anything a provider
    supplies can be imported — see the module docstring for why that ordering is the whole
    fix and not an implementation detail of it.

    Returns the previous claim rather than nothing, which makes :func:`release` a restore
    instead of a reset. `panel.run(once=True)` is called in-process by tests, so a claim
    that ended by clearing the global would leave the NEXT caller in that process reading
    "no pane" when there was one — the same reason `_install_sigwinch` hands `run` back the
    handler it displaced instead of `SIG_DFL`.
    """
    global _CLAIMED
    held, _CLAIMED = _CLAIMED, sys.stdout
    return held


def release(held) -> None:
    """Put back the claim *held* — whatever :func:`claim` answered, ``None`` included."""
    global _CLAIMED
    _CLAIMED = held


def stream():
    """This process's pane, or `sys.stdout` for a process that claimed none.

    **The fallback is not a second answer to the same question**, which is worth saying in
    a module about exactly that mistake. A process that never claimed a pane does not have
    one: a hook, `charter statusline`, a test calling `slots._width()` on its own. For all
    of them "this process's ordinary output" IS `sys.stdout` and nothing has been captured
    that could disagree with it. The claim is what turns that from a guess into a fact, and
    only a process that is given a rectangle takes one.
    """
    return sys.stdout if _CLAIMED is None else _CLAIMED


def size():
    """This pane's own rectangle as the pane reports it, or ``None`` when charter has no
    usable measurement of it.

    ``None`` covers both ways of having nothing: a stream that cannot be asked (no
    `fileno`, a closed one, a real descriptor with no tty behind it — a panel run by hand
    with its output piped to a file), and a tty that answers with a zero. A zero is not a
    size, and judging it here rather than at each caller is #594's shape with one source
    instead of three: there, three sources each answered "a number or nothing" and
    `tui.term_width` judged the answer, because the judgement belongs wherever there is
    exactly one of it to write. Here there is one source and several callers, so it goes
    the other way round — and `slots._width` and `slots._height` cannot then come to
    disagree about whether this pane was measured at all.

    The whole `os.terminal_size` rather than a column count, because a rectangle is what a
    pane is: a caller that gets a size back has BOTH numbers from ONE reading of ONE
    descriptor, which is the property the two separate `os.get_terminal_size` calls this
    replaced could never state.
    """
    try:
        measured = os.get_terminal_size(stream().fileno())
    except _CANNOT_SAY:
        return None
    return measured if measured.columns > 0 and measured.lines > 0 else None


def is_tty() -> bool:
    """Whether this pane is a terminal — ``False`` for a pane that cannot say.

    Asked of the claimed stream, so a library's stdout stand-in claiming ``True`` answers
    for itself and not for charter's rectangle (#606: Textual's `_PrintCapture` says
    ``isatty() -> True`` from behind fd -1). A stream that raises is not a terminal for any
    purpose here, which is the direction `NO_COLOR` already points: a frame that cannot
    tell does not colour.
    """
    try:
        return bool(stream().isatty())
    except _CANNOT_SAY:
        return False
