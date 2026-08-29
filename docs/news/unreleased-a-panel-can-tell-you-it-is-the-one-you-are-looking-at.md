---
version: unreleased
headline: A component can be told when its pane is focused, blurred or resized — the events it was allowed to declare and never received
---

`Component.events` has been part of the contract for two phases. A provider could write
`events = ["scroll"]`, pass every check charter makes, and receive nothing, ever. There was
no dispatcher. The vocabulary was validated and read by nothing.

There is one now. It arrived carrying `focus`, `blur` and `resize`, and by the end of this
same release it carries `click` and `scroll` too — five of the six kinds
(`frame/events.py`'s `DELIVERED`). This entry is the three that came first; the pointer half
has its own, *A click and a scroll reach the panel you pointed at*.

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

`key` is the one kind charter does not carry, and that has not changed. Your harness owns
the keyboard: tmux sends typing to the active pane, which is the harness's, so the only
keystrokes a panel could ever see are the ones you typed into the wrong pane — and acting
on those is worse than dropping them.

`click` and `scroll` were held back at this point, and the answer written down for them was
*not yet* rather than *no*. What held them was a real measurement — **tmux routes a pointer
by position, and a click does not focus the pane it lands in**, so a click on an unfocused
panel would be a second focus, disagreeing with where your keyboard is going, with nothing
anywhere saying which one is driving the component. What removed it is `focus` and `blur`
themselves: with both delivered, *you are pointing at me* and *you are typing into me* are
two states a component can render differently rather than one it has to guess at. Both are
delivered by the end of this release; *A click and a scroll reach the panel you pointed
at* has the measurements.

Declaring a kind that does not fire stays legal and stays free. A declaration says what
your component *handles*; it was never a promise from charter that the event happens.

## What a declaration costs now

The two halves go together. A component that declares `events` and supplies no `on_event`
is refused when it loads, and so is one that supplies `on_event` and declares nothing. That
is a real change for a component written against the old contract — but only for one that
declared events it was never receiving, which is the defect this release is about.

A panel whose component declares nothing charter delivers is untouched: no dispatcher is
built, its pane's terminal keeps whatever mode it was in, and the loop sleeps exactly as it
did. Every panel charter draws is in that group at this point in the release — by the end of
it `repos` declares `scroll` and `click` and the other three still declare nothing, so a
frame you did not add a component to is the frame you had before.

## When a handler goes wrong

A handler that raises is retired — no further events reach it — and its pane draws the
reason instead of its rows, the same answer charter already gives a provider that fails to
import. A working `render` does lose its pane to a message when its handler breaks, and
that is the trade: a component that quietly stops being interactive is indistinguishable
from one nothing has happened to yet.

A handler that never returns freezes that one pane. The other panels keep painting, your
harness is untouched, and `F12` still takes you back to it — the escape hatch runs in
tmux's own key table and needs no charter code to be working. That pane also keeps the
terminal mode charter set for it, so typing into it echoes nothing until you kill it; the
restore runs when the panel stops, and a handler that never returns never lets it.

## One more refusal

A **part of a composite** cannot declare events. A composite draws its parts inside its own
pane, so a part is never placed on the frame and charter dispatches to whatever owns the
pane. A part that declared events would get none — this release's own defect, one level
down — so it is refused when it registers, naming both components and saying to put the
declaration on the composite.

## Nothing to adopt

If you have not written a frame component, nothing changes for you.
