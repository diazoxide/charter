---
version: unreleased
headline: A provider's library cannot take the palette's pane either, and F2 stops opening a pane it never draws in
---

The panel process stopped asking `sys.stdout` what its rectangle is. The palette process
had not. `F2` is the other charter process that is handed a pane, and it had the same
ordering the other way up.

`charter frame-palette --pane` builds its action registry first — which is
`importlib.import_module` on every installed provider, the first instant anything a
stranger wrote runs in that process — and only then asks for the tty. So a provider whose
module installs `textual.redirect_stdout`, a `rich` console, `colorama` or a logging
handler at import had already replaced the global by the time the palette resolved it.

Measured on `main`, driving a whole real palette in a 150x10 pane with one such provider
installed:

```
pane transcript : ''
library's log   : '\x1b[?1049h\x1b[?25l\x1b[?1006h\x1b[?1000h…'
raised          : OSError('[Errno 25] fd -1 is not a terminal')
```

Not one byte reached the pane — not even the alternate-screen enter, which is written
before anything is measured. And the raise landed in a process that had already run its
own teardown, so the traceback went into a pane tmux was about to kill. What an operator
sees is `F2` carving a pane off the harness, drawing nothing in it, and killing it again.
The command's documented "always exit 0" went with it.

## The pane is claimed where the process learns it has one

`charter/frame/pane.py` already answers "the pane this process was given", and `panel.run`
already claims it above the point where a provider can be imported. The palette now does
the same thing in the same shape: `cmd_palette` claims before it calls `_draw_palette` and
releases in its own `finally`, exactly as `panel.run` claims before `panel._run`.

That placement is the whole of the change and it was the part that needed deciding. The
claim has to sit above the registry build, and `_draw_palette`'s existing `try` starts
below it — its `finally` is what kills the palette's pane. Folding the claim in there would
have moved four lines under that `finally` to satisfy an ordering, changing when the pane
is closed. Putting it in the enclosing function instead leaves `_draw_palette` exactly as
it was.

The other half of `cmd_palette` — the `run-shell` child that carves the pane off the
harness without `--pane` — claims nothing, deliberately. That process paints in no
rectangle and its stdout is a pipe. `frame/pane.py`'s fallback exists for exactly that
process, and a claim there would record a pipe as "the pane this process was given".

`palette.own_the_tty`'s `out` now defaults to the claimed pane rather than to `sys.stdout`.
Every caller that passes its own stream — the tests that drive a real pty — is unchanged,
and a process that claimed no pane still gets `sys.stdout`, which is what that default
already meant.

## What is still true, and stated rather than left to be discovered

`own_the_tty` reads `sys.stdin.fileno()` for raw mode, and a library that rebinds *that*
global makes `termios.tcgetattr` raise. That is a different failure from this one: it is
loud in the code's own terms rather than silent, it is not what the palette paints or
measures through, and it is not covered by a claim about the pane. It stays open.
