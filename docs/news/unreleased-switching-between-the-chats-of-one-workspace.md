---
version: unreleased
headline: F2 switches between the chats of a workspace, and the panels move with you — torn down in the window you left and split into the one tmux has just resized
---

Stage 5a made a workspace a tmux session and a chat a window in it. This is the half that
lets you move between them. `F2` → `chat` lists every chat this workspace holds, marks the
one you are typing in and names the harness each is running; choosing one puts your client
on that chat's window and moves the frame's panels with it. Typing the chat's name at the
palette reaches the same row without the doorway, so a chat is two keystrokes away at every
terminal width — including the widths where no bar can be drawn at all.

**The panels follow the active chat rather than duplicating per chat, and that is a
correctness rule before it is a saving.** A background tmux window keeps STALE geometry —
measured again on this tree, on tmux 3.7c and at the 3.2 floor alike: with the client
resized 200x50 → 100x30 the active window follows and the background one is still 200
columns wide. Panels left running there are not idle, they are rendering at a width that is
not their window's. So the switch tears the old chat's panels down, selects the new window
— and tmux resizes it **at** the switch, reported correctly by the very next tmux
invocation with no sleep and no hook — and splits the new panels into a window that is
already the right size. They are born correct, which is why the switch never needs the
`window-resized` hook that does not exist on tmux 3.2 (`set-hook -w window-resized` →
`invalid option`, rc=1).

**Refusals say which one fired.** A chat of another workspace, a chat that has been reaped,
a chat whose window has gone, the chat you are already in — each is its own sentence on
your own screen, with the chats this workspace does have listed beside it. `charter
frame-chat <id>` is the same rule by another door, for typing by hand.

## Two measurements that correct the spec

**A switch is ~360 ms, not the ~16 ms the spec's §7.7 extrapolated.** That figure counted
nine tmux commands — one `select-window`, four `kill-pane`, four `split-window`. Charter's
one live pane-mutation path issues **41**: the disarm before each kill, the arm after each
split, the pane surface and border options, the resize hook, and the size re-assertion that
`kill-pane` and `split-window` both make necessary. Measured end to end with a real
attached client and four panels, six switches each way: **median 360 ms on tmux 3.7c and
395 ms at the 3.2 floor**, against a one-invocation baseline of 6.2 ms.

The decision §3.7 took does not change — 41 invocations is still enormously cheaper than
758 MB of panel processes rendering at widths that are wrong — but the argument's numbers
do, and so does the plan's instruction to chain the kills and the splits: chaining the
eight commands it names would save under a fifth of the round trips, so it is a ~10 %
improvement to the switch rather than the 3.3× the plan expected. Collapsing the whole
re-layout into one invocation is where the rest is, and that is a change to the funnel
every density change and every toggle key also goes through.

**`layout._DROP_ORDER` was read by nothing.** The spec asks the two new bars to "join
`_DROP_ORDER`, above `top`" — but `visible_slots` spelled that order out by hand, so
joining the list would have changed no behaviour and both bars would have survived exactly
the shortage that takes the identity row. The row-edge half of the list is derived from it
now, so an entry deleted from it changes what a short terminal draws.

## The two bars ship, and neither is on your frame unless you ask

`chats` and `workspaces` are components in the registry, one row each on the top edge. The
chat bar shows every chat with the one you are in marked; at widths where they do not all
fit it keeps yours whole and counts the rest (`*charter.2  +2`), and below that it says
where you are and how many there are (`2/3`). It never truncates a name, because every rung
of that ladder drops whole names rather than cutting one. With a single chat it says so and
names what opens a second.

**Neither is placed by default, and the reason is the measurement above.** A plane with one
chat is the ordinary, permanent state — the same fact that keeps the `changes` section out
of every operator's sidebar — and each placed pane is around seven of a switch's 41 tmux
invocations plus a 24 MB panel process and a row off the harness, permanently, to draw a
name `F2` already reaches. Ask for one with a `[[frame.component]]` table, which is also
the only way any component gets a toggle key:

```toml
[[frame.component]]
use = "chats"
edge = "top"
size = 1
key = "F9"
```

That route is new in itself: a component charter registers but gives no committed `[frame]
slots` word could not be placed by any configuration at all, because the config boundary
asked "is this one of the four slot names" where it meant "is this one charter places".
`frame/builtins.places` is that question asked directly, so charter's own components are
now exactly as placeable as an installed provider's.

## What this stage does not do

Creating a chat from inside a frame — `F2` → pick a harness → a new tab — is not here.
`charter <harness>` in the workspace still opens one, from a shell, and the chat bar's
affordance names that rather than a palette row that does not exist. Neither is the
per-chat token gauge, the live-chat cap, or reopening a chat that has been closed: those
are Stage 5c, and until then a chat whose window is gone has its record reaped like any
other frame's.
