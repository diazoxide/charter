---
version: unreleased
headline: The surface is one keystroke, the floor was measured, and a component can match without charter drawing over it
---

Three things the frame's visual design left open, closed: the pane surface stopped
requiring a file edit, the tmux version charter actually floors on got measured instead of
assumed, and a component charter did not write can now look like one that it did.

**`F2` carries the surface.** `chrome: off`, `chrome: dark` and `chrome: light`, with the
one you are on marked. Choosing one repaints the frame you are looking at — no pane moves,
nothing is re-laid-out — and `charter.toml` is not touched: the change lives in the frame's
own state directory and is gone when the frame ends. The whole argument for shipping
`chrome` defaulting to `off` was that a default which repaints a stranger's terminal can
make a working frame worse on upgrade; the cost of that argument was the operator on a dark
terminal who wanted the fill and had to go find a config key. These three rows are what
they pay instead.

`charter frame-chrome <off|dark|light>` is the same thing typed by hand. Choosing `off`
**removes** the tmux options rather than setting a third look — a palette row that reported
success and left the surface exactly where it was would be the convincing empty this spec
argued an `auto` value out of existence for. And a pane the frame splits later comes up in
the surface the frame *is* on, not the one it launched with: change the surface, then change
the density, and the new pane used to arrive bare beside three surfaced ones.

**The floor was measured, and it was not gated.** Everything behind the surface —
`window-style` being settable per pane, honouring a colour and silently ignoring every
attribute, the sixteen palette names resolving — had been measured on tmux 3.7c and on
nothing else, while charter floors at 3.2. So 3.2 was built from source and the whole of it
was re-run there, one style per server, reading the escapes off an attached client's wire:

| at tmux 3.2 | |
|---|---|
| `set -p window-style` pane-scoped? | yes — sibling panel, harness pane and the window all read back `''` |
| colour only? | yes — `reverse`, `dim` and `bold` each put **no SGR at all** on the wire |
| do the 16 names resolve? | all sixteen — `bg=black`→`ESC[40m` … `bg=brightwhite`→`ESC[107m` |
| a refused `set-option` | rc 1, `invalid style:`, previous value intact — reported, not fatal |

Sixty-six of sixty-eight answers were byte-identical to 3.7c. The two that were not were
the measuring harness rather than tmux: it batched four styled panes into one session, and
tmux emits an SGR only when the style *changes* — so four panes that downsample to the same
colour produce one escape and three silences, which reads as "three styles tmux ignored"
and is nothing of the sort. One pane per server, they matched too. **The failure direction
was believed safe and is now measured**, which was the point: `chrome` carries no version
gate, and the absence of one is asserted rather than left to be noticed.

**A component charter did not write can match it.** `ctx` now carries the frame's own
recipes — `heading`, `muted`, `selected`, `ok`, `warn`, `bad`, `reset` and `inset` — as a
read-only mapping of strings:

```python
def render(ctx):
    c = ctx.chrome
    return [f"{c['inset']}{c['heading']}metrics{c['reset']}{c['muted']} 12{c['reset']}"]
```

Every value is a plain attribute or one of the sixteen names your palette defines, never a
colour charter picked, and `inset` is the literal left margin the rest of the frame starts
at. Under `NO_COLOR` every escape in it is the empty string and the keys stay — a component
built on them does not break there.

There is no `surface` or `focus` recipe, and that is deliberate: those two are tmux pane
options, no renderer can write one, and handing over an empty string for them would be a
recipe that claims to give you something charter does not have. There is no `chrome`
argument on the recipes either, for the same reason turned around — `off`, `dark` and
`light` select a pane background, and the measurement above says `window-style` ignores
every attribute, so nothing a renderer can write changes with the word. An argument that
cannot change an answer is a line this repository deletes.

What charter still does not promise is that your pane looks like its own. It does not
overdraw your heading and it does not take a row out of your rectangle to make things line
up. What it guarantees is what it already guaranteed: your paint stops at your rectangle,
your failure costs your pane and not the session, and your output is contained before it
reaches the terminal. A pane that clashes is a pane whose component chose to — which is
honest, because it *is* different.

`ctx` gaining a field is a widening of what a stranger's code may reach, and `ctx.Ctx`'s
own docstring says that should cost a test change and the conversation that goes with it.
The conversation is §7 of the spec; the cost is three assertions in
`tests/test_component_ctx.py` that now name `chrome`, spelled out as a list rather than
read off the constant, so that a name quietly added later fails them instead of being
carried into them.
