---
version: unreleased
headline: The frame turns focus events on, and the mouse trade turns out to be permanent rather than deferred
---

Two small things, both of which were charter saying something that was not true.

## Focus events exist now

A panel could not tell that it had stopped being the active pane. The frame's component
contract lists six event kinds, two of which are `focus` and `blur` — and they never fired,
on any machine, for anyone. tmux ships `focus-events` **off**, and with it off tmux writes
`\x1b[?1004l` to your terminal and never delivers a pane focus transition at all. The
option was simply missing from the config charter writes for its own server.

It is there now. `charter frame` turns `focus-events` on for the private tmux server it
launches, so a component that declares `focus` or `blur` receives them — which is what the
dispatcher later in this release delivers through (*A component can be told when its pane is
focused, blurred or resized*).

Two honest edges on that, both measured rather than assumed:

* **Inside a tmux you already have, they still do not fire.** `focus-events` is a tmux
  *server* option — it lives in `show-options -s`, and setting it for one session sets it
  for every session on that server. Charter writes nothing at all into your own tmux (that
  boundary has not moved), so it will not turn this on for you either. `set -s focus-events
  on` in your own config is how you get it there.
* **Your terminal emulator has to report focus for any of it to work.** Not all do.

The rule the contract now states for a provider: a declared event kind degrades to *never
fires*, never to *fires wrongly*.

## "A trade a later release will avoid" — it will not

`[frame] mouse` has been off by default since it shipped, with a note saying that turning it
on trades your terminal's own drag-select for clickable panels, and that the trade "belongs
to a later release that actually ships clickable panels". Both halves of that were wrong.

This is that release. And measured against tmux 3.1c, 3.2 and 3.7c, the trade was never
conditional and no later release could have removed it: tmux asks your terminal to report
the mouse based on the **active pane's** request alone, so the instant any mouse-requesting
pane is active your terminal is reporting and its own selection is gone for the whole
window. **There is no state in which charter's panels are clickable and native selection
survives.** Leaving tmux's own `mouse` off does not dodge the trade — it only makes it
depend on which pane happens to be focused.

Which is the second thing, and the one worth knowing before you write a component:

**With `[frame] mouse` off, whether a panel is clickable is decided by the harness, not by
charter.** With the harness pane active and nothing having asked the terminal to report,
a click on a panel produces no bytes at all — there is nothing for charter to drop, because
the event never happens. If the harness you ran does request mouse reporting, panels become
clickable while it is active, and you lose drag-select for as long as it is. Charter does
not own that program and will not claim otherwise.

So `click` and `scroll` in the component contract declare what a component **handles**,
never that the event fires — charter delivers both by the end of this release (*A click and a
scroll reach the panel you pointed at*), and this paragraph is why delivering them is still
not a promise that they happen. Give every pointer affordance a key as well. A surface charter
draws over a whole pane and drives itself is the exception and needs no setting: while it is
open it *is* the active pane, so its own request is the one that reaches your terminal.

## What to do

Nothing. `[frame] mouse` still defaults to off and still means what it meant; the frame
gains one tmux option it should always have set. If you have been waiting for the release
that makes panels clickable without costing you text selection, stop waiting — that release
does not exist, and charter now says so where you would look for it. Panels *are* clickable
by the end of this one; what no release removes is the price.
