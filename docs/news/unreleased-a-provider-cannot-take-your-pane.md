---
version: unreleased
headline: A provider's library cannot take your pane, and a size charter cannot read is not 80x24
---

A charter panel paints by writing to `sys.stdout`, and it measures its own rectangle from
that same descriptor. So in a panel process `sys.stdout` is not an output stream — **it is
the pane**. Which meant that any library a provider's component imported and that touched
that global took the pane away from charter, and nothing said so.

```
$ python3 -c "... sys.stdout = a stream answering isatty()=True, fileno()=-1 ..."
  slots._width()  = 80        # the real pane was 150x10
  slots._height() = 24
```

Measured in a real 150x10 pane: **a correct first paint, then blank on every repaint.
Nothing raised, nothing logged.** The panel went on painting five times a second into the
library's in-memory log while laying the frame out for a rectangle nobody had, and because
nothing went wrong, none of charter's own containment fired: a provider's component is
supposed to cost its own pane when it breaks, and this evaded that promise rather than
violating it.

It is not about any one library. `textual.redirect_stdout` is where it was found, but
`rich`, `click`, `tqdm`, `colorama`, a progress bar and a logging handler installed at
import all reach for the same name.

## The pane is claimed once, before a provider's module is imported

`charter panel` now takes the descriptor it was given at its first line — above the point
where charter imports anything a provider supplies — and everything downstream paints,
measures and asks about colour through that claim. A library that rebinds `sys.stdout`
afterwards moves the global and does not move your pane.

Three things were reading the global and all three follow the pane now: the paint, the
`_width`/`_height` a renderer lays itself out from, and the `isatty` behind `NO_COLOR`'s
sibling question — which had been told a pane was a terminal by a stand-in that was not
one.

## And "charter could not measure this" no longer looks like a real 80x24 pane

The fallback was silent, so a pane that could not be measured was indistinguishable from a
pane that had been measured and was 80 by 24. Two halves, told apart by the thing that
actually differs rather than by whether the measurement raised:

- **Your output is not a terminal** — `charter panel bottom --session x > /tmp/log`, run
  by hand for debugging, or a test. There is no rectangle to be wrong about, so charter
  draws at the stated default exactly as it always has.
- **It IS a terminal and will not report a size.** That is a real rectangle of unknown
  shape, and the pane now says `charter: pane size unknown` instead of a frame laid out
  from a guess. It is painted per redraw and never held: a pty that has not been given a
  window size yet gets one moments later, and the resize that follows draws the frame.

The second case is more ordinary than it sounds — a pty created without a window size
reports zero columns until something sets one, which is what `os.openpty`, some CI shells
and a terminal attached before its size is negotiated all hand you. A zero got through the
panel's measurement as a width, which is the same defect the status line's own width
already had one function over.

## To adopt

Nothing. Upgrading is the whole of it.
