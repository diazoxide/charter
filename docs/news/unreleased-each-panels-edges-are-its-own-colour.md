---
version: unreleased
headline: Each panel's edges are drawn in that panel's own colour
---

The seam this release closed was closed by putting a background behind the frame's rules.
That background went on the **window** — one value for every rule in the frame — so a panel
whose colour was not the frame's ended up with rules in a colour it does not wear.

**On tmux 3.7 and newer it does not have to be one value.** `pane-border-style` and
`pane-active-border-style` became per-pane options in 3.7, so each panel now carries its own
edges and the window's stay bare. Which colour a panel's edges are needs no configuration:
the surface that panel's own `bg` resolves to, or the frame-wide `chrome` colour where it
names none — the same expression its interior is painted from, so a pane and its edges cannot
come out two colours.

The rules that run round the pane your harness is in are a separate question with a separate
answer — see *One rule, one colour — including the three that run round your own session*.

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

Both of a pane's edge options always carry the identical value, which is the same rule that
stops a rule changing colour at the active pane's corner; measured across four focus states,
the frame does not move when focus does.

`NO_COLOR` takes the background off the edges as well as off the panes. `chrome = "off"`
removes them, and on tmux 3.7+ that is a real per-pane removal rather than one that would
reach the window.

Nothing to adopt: upgrading is the whole of it.
