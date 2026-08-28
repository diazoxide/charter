---
version: unreleased
headline: A component can be told when its pane is focused, blurred or resized — the events it was allowed to declare and never received
---

`Component.events` has been part of the contract for two phases. A provider could write
`events = ["scroll"]`, pass every check charter makes, and receive nothing, ever. There was
no dispatcher. The vocabulary was validated and read by nothing.

There is one now, and it delivers three of the six kinds.

## What fires

```python
component.Component(
    id="acme.metrics", title="Metrics", edge="right", size=12,
    events=("focus", "blur"),
    on_event=lambda ev: remember(ev.kind),   # answer truthy to repaint
    render=lambda ctx: [line_for(remembered())],
)
```

`focus` and `blur` when your pane becomes, or stops being, the one you are looking at.
`resize` when its rectangle moves. That is enough for a panel to dim itself when you look
away, to show a hint only while you are on it, or to drop a layout it cached for a width
it no longer has.

Measured against real tmux 3.7c before any of it was written — two panes, a real attached
client, one pane asking for focus reporting and one not:

```
focus-events on    the pane that asked      READ b'\x1b[I' on select, b'\x1b[O' on leave
                   the pane that did not    nothing, ever
focus-events off   the pane that asked      nothing, ever
```

So this works on charter's own server, where charter writes `set -g focus-events on`, and
does not inside a tmux you already have, where charter writes no config at all. Your
terminal has to report focus too. That is the honest limit and it is the direction the
contract asks you to degrade in: **never fires, rather than fires wrongly.**

## What does not fire, and why it is still declarable

`key`, `click` and `scroll` are decoded and routed nowhere.

`key` is not delivered because your harness owns the keyboard. tmux sends typing to the
active pane, which is the harness's, so the only keystrokes a panel could ever see are the
ones you typed into the wrong pane — and acting on those is worse than dropping them.

`click` and `scroll` are the harder one, and the answer is *not yet* rather than *no*.
Mouse reporting comes from the active pane's own request, which is why `[frame] mouse`
defaults to off and costs you your terminal's text selection when it is on. The experiment
behind this work measured the rest: **tmux routes a pointer by position, and a click does
not focus the pane it lands in.** So a click on an unfocused panel would be a second focus,
disagreeing with where your keyboard is going, with nothing anywhere saying which one is
driving the component. Charter would rather deliver nothing than deliver that, and the
decision about what a click on a non-active pane should mean is written down in the pull
request rather than guessed at here.

Declaring a kind that does not fire stays legal and stays free. A declaration says what
your component *handles*; it was never a promise from charter that the event happens.

## What a declaration costs now

The two halves go together. A component that declares `events` and supplies no `on_event`
is refused when it loads, and so is one that supplies `on_event` and declares nothing. That
is a real change for a component written against the old contract — but only for one that
declared events it was never receiving, which is the defect this release is about.

A panel whose component declares nothing charter delivers is untouched: no dispatcher is
built, its pane's terminal keeps whatever mode it was in, and the loop sleeps exactly as it
did. Charter's own four panels are in that group, so the frame you have today is the frame
you had yesterday.

## When a handler goes wrong

A handler that raises is retired — no further events reach it — and its pane draws the
reason instead of its rows, the same answer charter already gives a provider that fails to
import. A working `render` does lose its pane to a message when its handler breaks, and
that is the trade: a component that quietly stops being interactive is indistinguishable
from one nothing has happened to yet.

A handler that never returns freezes that one pane. The other panels keep painting, your
harness is untouched, and `F12` still takes you back to it — the escape hatch runs in
tmux's own key table and needs no charter code to be working.

## Nothing to adopt

If you have not written a frame component, nothing changes for you.
