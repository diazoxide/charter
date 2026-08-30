---
version: unreleased
headline: Editing a running frame's arrangement stops duplicating every panel it places
---

Adding two `[[frame.component]]` tables to a plane whose frame was already running left
**six panel panes where there should have been two**, and the frame's own state recorded
only the newest of each:

```
%366 152x1 panel workspaces --session harness-wrapper.1
%367 152x1 panel workspaces --session harness-wrapper.1
%368 152x1 panel workspaces --session harness-wrapper.1
%369 152x1 panel chats      --session harness-wrapper.1
%370 152x1 panel chats      --session harness-wrapper.1
%371 152x1 panel chats      --session harness-wrapper.1

.charter/frame/harness-wrapper.1/panes:
  {"top":"%362","bottom":"%363","repos":"%364","right":"%365",
   "workspaces":"%368","chats":"%371"}
```

The four orphans were still running and still drawing, each holding a ~24 MB `charter
panel` process, and they had squeezed that operator's harness from 38 rows to 30 with
panes whose cause was invisible from the screen. Nothing in charter would ever have reaped
them.

## The cause is a file, not a loop

`state.record_panes` rewrites the pane map **whole** on every re-layout. Splitting a second
pane for a component therefore *deletes* the first one's id — and after that no reader that
goes through `state.panes` can see that pane at all: not the kill loop that would have
dropped it, not the teardown a chat switch runs, not the resize path.

So a re-layout asked the record three questions and got the wrong answer to all three at
once. It is also why removing a component from `charter.toml` sometimes left its panel
running: the same reconciliation in the other direction, blind for the same reason.

## The window is asked now, not the record

Every pane charter splits already carried `@charter_panel`, the mark that keeps a click on
a panel from stealing the keyboard. It says a pane is *a* panel and not *which*, so it
grows a second option beside it — `@charter_panel_slot`, carrying the component's id — and
a re-layout now asks tmux which panes this window has and which components they draw:

```
$ tmux list-panes -t %362 -F '#{pane_id} #{@charter_panel} #{@charter_panel_slot}'
%362
%366 1 workspaces
%368 1 workspaces
```

A component the arrangement still wants and a pane already exists for is **adopted**
instead of split again. A component it no longer names loses its pane whether or not the
record can still see it. The record is still read — it decides which of several panes for
one component survives, so a healthy frame reconciles to exactly what it already had — but
it is no longer the only thing that can see a pane.

## What is never killed

A pane charter did not split. The mark is the whole discriminator, and it is compared
against charter's own literal rather than read as a tmux truth value, because the cost of
being wrong here is a pane rather than a click: your own splits, the palette's overlay and
the harness itself carry no mark and are left alone, at any arrangement and at `want=[]`.

**A frame launched by an older charter keeps its panels.** Its panes carry the mark and no
component id, so charter can see they are its own and cannot see which component each one
is — and a positional guess on a window whose panes have moved is exactly how you kill the
wrong pane. Those panes are left running, the frame is no worse off than it was, and its
record still drops a component's panel the way it always did. Every pane split from this
version on carries an id, so a frame heals forward rather than being repaired by guesswork.

Cost: one `tmux list-panes` per re-layout and per launch — a single round trip, ~5 ms
against a real tmux 3.7c — and one `set-option -p` per panel as it is created. Measured on
tmux 3.7c and at charter's 3.2 floor; identical on both.
