---
version: unreleased
headline: The chat and workspace bars are tabs you can click, not captions that list names
---

*"I put both bars on my plane, clicked a tab, and nothing happened."*

Nothing was going to. `chats` and `workspaces` shipped as components a plane could place, and
both were registered with no `events` and no `on_event` at all — so tmux routed the report to
the pane, charter decoded it, and the dispatcher dropped it for a kind the component had never
declared. A bar with no handler is a caption that happens to list names.

**Both bars declare `click` now, and a click switches.**

```
  workspaces   alpha  *harness-wrapper   showcase   todos          <- click `alpha`
  chats       *api.1   api.2                                       <- click `api.2`
```

A chat tab runs exactly the switch `F2` → `chat` runs — the same command, the same five
refusals, the same sentence on your own screen when one fires. A workspace tab runs
`charter frame-switch --workspace <name>`. Neither is a shortcut past those: the click starts
the command charter already had, which is what puts a refusal somewhere you can read it. A
panel has no status line of its own, and a click that silently does nothing is the report this
entry is answering.

**A click on the repo table only ever *selects*, and a click on a tab *switches*. That is a
decision, and it is worth the paragraph.** The rule the table keeps is that nothing
irreversible rides on a pointer event, because one can arrive unpaired — a drag begun on a
pane border delivers exactly one release, measured. Three things put a tab bar on the other
side of it:

- **Charter acts on the press, and the press is never the unpaired half.** A press is
  delivered because the pointer was over that pane when the button went down. A drag that
  began elsewhere and happens to release over the bar delivers only a release, and switches
  nothing.
- **A switch is undone by the identical gesture.** The tab you left is still on the bar, one
  click away. Nothing is created, nothing is destroyed, nothing is started.
- **Nothing could ever finish a select-then-confirm gesture here.** On the table, "selected"
  is a real state — a highlighted row, its detail on the attention strip — and `Enter` in the
  palette is what chooses. A bar has no such state: the tab you are on *is* the selection, and
  the `*` beside it is how you see that. And a keypress does not reach a panel at all, because
  tmux gives your typing to the active pane and that pane is your harness. A bar that merely
  selected would draw a second mark nothing on the machine could act on.

**What a click on the wrong cell does, which is nothing — deliberately, in four places.** The
heading. The gap between two names. A `+N` where the names did not fit, which stands for
names that are not on the row at all. And the empty space past the last tab. Charter resolves
a click against the columns the paint actually wrote, published by the same pass that drew the
row, so a cell no tab was drawn into is not a cell a tab was clicked in — it will not pick the
nearest name for you. **Clicking the tab you are already on does nothing too**, rather than
tearing four panels down and splitting them again to arrive where you were; it is also what
stops a double-click switching twice.

**One consequence to know before you place a bar on a narrow frame.** Where the whole list
does not fit, the row draws the page yours falls on and counts what is off each end — so the
names a click can reach are the ones on that page, and on a row narrow enough to hold only
your own name there is nothing on the bar to click at all. That is honest rather than broken:
`F2` reaches every chat and every workspace in two keystrokes at every width, which is what
"the bar is a readout" has always meant. *The tab bars draw the tabs that fit* in this same
release has how the page is chosen and why it does not move under your pointer.

**You need `[frame] mouse = true`, and you need to have placed a bar.** Neither is on by
default and this entry does not change that: with mouse off, tmux asks your terminal to report
from the *active* pane's request alone, so a click on a panel is not an event charter drops —
it is an event that never happens. What it no longer costs is your keyboard: clicking a tab
acts where you pointed and leaves you typing in the harness.

Verified end to end on a real tmux — 3.7c and the 3.2 floor — with a real client on a real
pty, a real `charter panel` process holding the bar's pane, and a real detached `charter
frame-chat` / `charter frame-switch` started by that panel out of its own handler: the client
lands on the other chat's window, the frame lands on the other workspace, the bar repaints
with the mark moved, and the keyboard never leaves the harness.
