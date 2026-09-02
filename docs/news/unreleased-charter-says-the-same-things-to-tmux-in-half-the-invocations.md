---
version: unreleased
headline: A chat switch is half the wait and a third of the flicker
---

*"I see very slow rendering on switching — it seems re-rendering all and I see jumping
texts ~0.2 sec, then it's ready."*

Both halves of that are one defect, and it is not what the frame *does* — it is how many
times charter has to speak to say it. A tmux command is a round trip over a socket, ~5 ms
on the machine this was measured on and ~13.4 ms on the operator's. A four-panel chat
switch made **58** of them, and a launch made **46** before it could put anything on
screen. Two thirds read nothing back.

## What was measured

A real attached client at 200x50 with four panels, `charter frame-chat` and a whole
`charter claude` up to the attach, on tmux 3.7c and at the 3.2 floor:

| | invocations | wall clock | times the terminal is repainted |
|---|---|---|---|
| switch, 3.7c | 58 → **26** | 329 ms → **162 ms** | 45 → **17** |
| switch, 3.2 | 50 → **24** | 243 ms → **127 ms** | 41 → **15** |
| launch, 3.7c | 46 → **20** | 245 ms → **114 ms** | — |
| launch, 3.2 | 42 → **19** | 184 ms → **83 ms** | — |

The third column is the jumping. tmux redraws once per command **list**, not once per
command, so four `split-window`s sent one at a time are four screen updates ~5 ms apart —
the panels arriving one by one — and four sent as one list are one.

## What changed

Nothing charter says to tmux, and nothing about the order it says it in. tmux parses a
`;`-separated list and runs the whole thing server-side, in order, so the groups that read
nothing back go as one invocation: the window's dressing (`remain-on-exit`, the five
border options, the two rules round the harness), a doomed panel's disarm beside its own
`kill-pane`, the ten writes a launch uses to tell its window and session what they are,
each pane's own options, the respawn hooks, and the row resizes.

The four `split-window`s go together too, which needed a measurement rather than an
argument: a chain of them answers with **one pane id per line, in the order given**, on
3.7c and at the 3.2 floor alike. That is what let the splits — the ones the operator
actually watches arrive — become a single repaint.

**The switch still tears the old chat's panels down and splits fresh ones in, and that is
not the part to optimise away.** A tmux window that is not current keeps the size it had,
so panels left running in a chat you switched away from are not idle — they are drawing at
a width that is no longer their window's. The teardown is a correctness rule; what it was
not is 41 separate conversations with the server.

## What a failure costs, because a batch could have made that worse

A refused command **abandons the rest of a tmux command list** — measured on both versions:
`set-option @a 1 ; set-option nosuchoption 1 ; set-option @b 1` sets `@a`, refuses the
middle one, and never sets `@b`. Charter's frame is written around each of those writes
failing on its own, with its own sentence about what an operator loses — the escape hatch,
the exit code, the palette's own actions. So a batch that comes back non-zero is thrown
away and every write is re-issued one at a time, through exactly the calls, codes and
sentences that were there before. The fast path is one invocation; the failure path is one
wasted invocation and then the old behaviour, unchanged.

The one batch that reads a value back — the splits — cannot be replayed that way, because
a repeated `split-window` is a second pane. It counts the ids that came back instead: the
command at that index is the one tmux refused, nothing after it ran, and those are the ones
re-issued. A tmux that timed out is not retried at all, in either batch: a timeout does not
say the write did not happen.

Closes #780.
