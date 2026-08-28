---
version: unreleased
headline: The frame stops drawing a box around your own session
---

The last release put a background behind the frame's rules so a surfaced frame stopped
showing a one-cell seam between its panels. It put that background on the **window**, which
is every rule in the frame — including the three that run between a panel and the pane your
harness is in. A plane whose four panels were all `brightblack` got a grey box drawn round
its own session on three sides. Reported off a screenshot, again.

The cell between two panes belongs to neither, and a window-wide answer cannot tell the
panels apart from the one pane charter does not own. **On tmux 3.7 and newer it does not
have to.** `pane-border-style` and `pane-active-border-style` became per-pane options in
3.7, so each panel now carries its own edges, the window's stay bare, and the harness —
never written, which is the same construction ADR 0018 already rests on — keeps your
terminal's own. Read back off an attached client's wire, charter's real four-panel shape at
100x24:

```
window-wide   harness top ESC[100m   harness right ESC[100m   panel|panel ESC[100m
per pane      harness top ESC[49m    harness right ESC[49m    panel|panel ESC[100m
```

Three edges change and nothing else does: no rule between two panels of one colour is a
different colour, which is the seam the last release closed and this one keeps closed.

**tmux 3.2 to 3.6 keeps the window-wide answer, and that is a measurement rather than
caution.** In those releases the two options are window options only — but `set -p` does not
say so. It returns **0 and writes the window**, so the last panel written would decide every
rule in the frame, and turning the surface off would `set -p -u` charter's own border
settings away for the whole window. There is no refusal to catch and a probe would have to
perform that write to find out, so this is a version gate: `options-table.c` says
`OPTIONS_TABLE_WINDOW` in 3.2, 3.3a, 3.4, 3.5, 3.6 and 3.6a and gains
`OPTIONS_TABLE_PANE` in 3.7, and both sides were run — 3.7c, and a 3.2 built from source.
A tmux charter cannot get a version out of takes the window-wide answer too, because that
one is correct everywhere and the other is correct only above the line.

Which colour a panel's edges are is unchanged and still needs no configuration: the surface
that panel's own `bg` resolves to, or the frame-wide `chrome` colour where it names none —
the same expression its interior is painted from, so a pane and its edges cannot come out
two colours. Both of a pane's edge options always carry the identical value, which is the
same rule that stops a rule changing colour at the active pane's corner; measured across
four focus states, the frame does not move when focus does.

`NO_COLOR` takes the background off the edges as well as off the panes. `chrome = "off"`
removes them, and on tmux 3.7+ that is a real per-pane removal rather than one that would
reach the window.

Nothing to adopt: upgrading is the whole of it.
