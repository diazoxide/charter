---
version: unreleased
headline: A click and a scroll reach the panel you pointed at, in its own cells, without taking your keyboard with them
---

`Component.events` lists six kinds. Three of them started firing last release; `click` and
`scroll` are the two this one adds. `key` remains the one charter does not carry, and the
reason has not changed: the harness owns the keyboard, so the only keystrokes a panel could
ever see are the ones you typed into the wrong pane.

## Why the pointer was held back, and what changed

The objection was written down at the time: tmux routes a pointer by **position**, and a
click does not focus the pane it lands in — so a click on an unfocused panel would be a
second focus, disagreeing with your keyboard's, with nothing anywhere saying which of the
two a component was being driven by.

What removed it is the release before this one. A component can now see `focus` and `blur`,
so *you are pointing at me* and *you are typing into me* are two states it can render
differently instead of one it has to guess at. The two alternatives stayed refused:
click-to-focus, because the frame exists to keep the harness the thing you type into; and
delivering only to the active pane, because that makes the pointer useless everywhere
except the one pane you are already typing in.

So charter delivers **where the pointer is**, and never moves focus.

## What was measured

On tmux **3.7c** and again at the **3.2 floor**, byte-identical on both — a real server, a
real client on a real pty, two panes split from a 120x30 window, reports injected exactly as
a reporting terminal sends them:

```
tmux mouse off, harness active, both panes asked for reporting
  click window col 100 (panel at left=80)  ->  panel reads b'\x1b[<0;20;5M'   active: harness
  click window col 81                      ->  panel reads b'\x1b[<0;1;5M'    active: harness
  click the border between them            ->  nobody                        active: harness
  wheel over the panel                     ->  panel reads b'\x1b[<64;20;5M'  active: harness
  with pane-border-status top, window row 5 ->  arrives as row 4
```

Three things follow, and all three are now pinned by cases that drive a real tmux:

* **The keyboard does not follow the pointer.** The active pane never changed in any of it.
* **tmux has already done its own arithmetic.** `pane_left` and any border row are gone
  before the bytes arrive, so charter must not subtract them again — and does not.
* **A border cell is a cell in neither pane.** A click there reaches no program at all,
  with or without a `pane-border-style` set, so there is no border case to write.

## What a component is told

`row` and `col` are cells of the component's **own rectangle** — the one `ctx.width` and
`ctx.height` describe. If you set `[frame] pad`, those cells are charter's, not the
component's, and charter takes them off before delivering; an event landing in the margin is
not delivered at all. That is the one subtraction charter does, and it is charter's because
charter drew the pad — tmux has never heard of it.

Nothing is delivered against a pane charter could not measure either. `slots._width()`
answers a stated 80-column fallback for such a pane so that a renderer always has a number,
and translating a click against that would report a cell of an invented canvas — at the one
moment the pane is showing `charter: pane size unknown` rather than the component's rows.

`name` is the button (`left`, `middle`, `right`) or the wheel's direction (`up`, `down`).
Right- and middle-clicks measurably reach a panel, so a component that acts on one button
can now tell. Modifier keys are not reported: a shift-click is a `left` click, which is what
whoever pressed it meant.

Buttons charter has no name for are dropped rather than reported as one it does. That is
less obvious than it sounds, because an SGR button number is not one number: xterm keeps it
in three separate bit positions, and the thumb buttons on an ordinary mouse arrive as
128–131 — which tmux was measured forwarding to a panel verbatim. Taken as the low two bits
alone, a thumb-button press is a `left` click, and a component acting on left clicks acts on
it. The same reassembly is what keeps a drag from arriving as a click at every cell it
crosses, and a shifted wheel from scrolling the way you did not.

A click arrives **twice** — a press, then a release, told apart by `pressed`. Either can
arrive without the other, because tmux routes each by where the pointer was at the time; a
drag that starts on a border and ends in a pane delivers a lone release. Act on one of them.
A component that waits for a matching pair waits forever the first time somebody drags out
of its pane.

## What `[frame] mouse` does, said plainly

It is still **off** by default, and off it still means charter promises nothing: tmux asks
your terminal to report the mouse from the *active* pane's request alone, so with your
harness active and not asking, a click on a panel produces no bytes at all. Nothing is
dropped — the event never happens. Whether it does is decided by the program you ran, which
charter does not own. **Give every pointer affordance a key as well.**

Turning it on makes reporting unconditional, and there are now two prices rather than one.
The first was already documented: you lose your terminal's own drag-select, and no release
will ever remove that. The second was measured for this change and is new to the docs:

```
mouse off  click a panel  ->  panel receives it,  active pane: the harness
mouse ON   click a panel  ->  panel receives it,  active pane: THE PANEL
either     wheel a panel  ->  panel receives it,  active pane: the harness
```

With tmux's own mouse on, tmux selects the pane under the pointer before forwarding the
click. That is click-to-focus arriving from tmux rather than from charter, and charter does
not rebind it away: the binding that would have to change is in tmux's root key table, which
is server-wide and shared by every frame charter launches, and dropping it would also take
away clicking back to a pane — including your harness. `F12` still returns you to it.

Point-to-act is therefore what `mouse = false` gives you, and what `mouse = true` buys is
certainty that the event fires at all. The wheel never moves your keyboard either way.

## What has not changed

A handler is still a notification and not a renderer: nothing it returns reaches the pane,
and it is handed no `ctx`. A handler that raises is still retired, with its pane saying which
component stopped and why. A panel whose component declares nothing charter delivers still
opens no input path, leaves its pane's terminal mode alone, and sleeps exactly as it did —
and a component that declares only `focus` is not handed a mouse request it never asked for,
which is the same promise one declaration finer than before.
