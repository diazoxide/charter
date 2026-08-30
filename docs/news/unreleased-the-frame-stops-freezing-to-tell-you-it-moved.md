---
version: unreleased
headline: The frame stops freezing for four seconds to tell you it moved, and stops spinning after the work ends
---

Two defects, one mechanism. Every `F2` switch froze the whole frame for four seconds
saying what it had just done (#729), and the attention row kept a spinner turning over
work that had already finished (#727). Both are the frame having entirely correct state
and failing to put it on screen at the moment it changed.

## A switch no longer costs four seconds of a screen you cannot see

Choosing a workspace, a persona or a chat put its outcome up as a tmux
`display-message -d 4000`. **A tmux client does not redraw its panes while a message is
up**, so the sentence announcing the switch was also what hid it. Measured with an outer
terminal mirroring a real panel, `capture-pane` on both at once:

```
                                    origin/main      this branch
the pane had repainted at              0.52 s           0.33 s
the operator's screen showed it at     4.28 s           0.33 s
-> stale screen for                    3.76 s           0.00 s
```

and at the tmux **3.2 floor**, the same rig, the same shape: 3.85 s of stale screen before,
none after. The freeze tracks `-d` exactly — `-d 200` froze for 0.20 s, `-d 750` for
0.74 s, `-d 4000` for 4.03 s — so the four seconds bought nothing at all. `docs/frame.md`
measures a chat switch at about a third of a second; that is what it now takes to see.

**The outcome goes on the attention row instead**, charter's own last row, for a few
seconds, and then the row goes back to what it was saying. `instance.FRAME_DENSITY` puts
`bottom` in every level charter ships, so it is the one surface besides the identity row
that is always there to be written on — and it is the row `docs/frame.md` already promises
is never dropped, which is exactly the promise an outcome line needs. A refusal gets ten
seconds where a success gets four: when a switch takes, the identity row above has already
changed to say so, and a refusal has nothing else confirming it.

## And it can be aimed at your frame, which the message never could

`display-message -t <pane>` does not choose the screen. `-t` is the target for **format
evaluation**; the client is `-c`, and with no `-c` tmux picks its own current client.
Measured on tmux 3.7c and at the 3.2 floor, two sessions on one server with a terminal
attached to each: a message aimed at a pane of session `sa` was drawn on the terminal
attached to `sb`, and not on `sa`'s at all.

charter had already been bitten by this by hand and read it as a property of an *empty*
target — `cmd_chat` carries a guard and a comment saying so. It is not: a well-formed `%N`
naming the right frame's own harness pane leaks identically, because the pane was never
what selected the screen. On a control plane with several frames on one socket, a refusal
about one frame was being drawn across another operator's.

A panel reads its own frame's state, so there is no direction for that to leak in — and
every client attached to the frame sees the row, which is strictly more than the one client
`-c` could name. The hotkey bind stops carrying `#{client_name}` altogether: it was
threaded through a tmux bind, a CLI positional and a subprocess relaunch for that one
consumer. `charter frame-palette` still *accepts* the positional, because a bind installed
by an older charter is still sitting in a running server's key table across the upgrade.

## The spinner stops when the work does

The attention row kept claiming work was running after the last record cleared, holding
whichever spinner frame it had last drawn — which is precisely what a hung frame looks
like. `panel._watch` repainted while something was in flight and had no reason to repaint
on the tick after the last record went: nothing resized, the version had not moved, no
event arrived. So the pane kept the last frame drawn into it, and only something unrelated
bumping the frame ever cleared it.

Reproduced on a real client with the tracker directory empty:

```
                                 origin/main      this branch
row stopped claiming it runs     never (15 s+)      0.21 s
```

on both tmux versions. One sighting during the audit survived a detach and reattach and was
still reading `⠸ 1 running` fifteen minutes later. The loop now carries the previous
answer and spends exactly one more paint on the falling edge — and one more only, so
`docs/frame.md`'s promise that the frame goes completely still is still true.

**The two fixes are one line each because they are the same missing edge.** A notice
expiring and a spinner stopping are both "what this row would draw now differs from what is
on it, because the clock moved, and nothing will say so" — which is why the loop's third
repaint reason is now called `ticking` rather than `animating`, and why the switch outcome
could move onto that row at all: without the falling edge it would have become a sentence
that never went away.
