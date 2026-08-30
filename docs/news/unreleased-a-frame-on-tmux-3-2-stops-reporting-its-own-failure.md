---
version: unreleased
headline: A frame on tmux 3.2 stops reporting its own failure on every launch
---

Every `charter claude` on tmux 3.2 printed this, and then came up fine:

```
✗ charter frame: styling the frame's own rules failed — `tmux -L charter set-option -w
  -t %0 pane-border-indicators off`: invalid option: pane-border-indicators
```

3.2 is charter's supported floor. `pane-border-indicators` arrived in tmux **3.3**, so
that line was charter naming an option the operator's tmux has never had — on every
single launch, for as long as the frame has pinned its own chrome. Nothing was broken:
the frame drew, the other four border options were pinned, and the one that could not be
pinned does not exist on 3.2 to be inherited from anyone's `.tmux.conf`. What the
operator got was a report of a failure with no consequence, in the two seconds before the
terminal switched to the alternate screen.

Each of the five options charter pins now carries the oldest tmux it may be issued to, and
a launch issues the ones this tmux actually has. On 3.3 and newer nothing changes — all
five are pinned exactly as before. On 3.2 four are pinned and the fifth is not mentioned.

**The floor is a field of the table now, not a fourth check.** This was the third time
charter had shipped a tmux capability with no version behind it — pane-scoped border
styles and the `window-resized` hook each already had a gate written for them alone, and
a third one would have left the fourth just as easy to add unguarded. So an option cannot
be added to the frame's chrome without stating the tmux it needs, and there is a test that
runs the real binary and checks every floor in that table against it in both directions:
a floor set too low goes red on the tmux that lacks the option, and one set too high goes
red on the tmux that has it.

Nothing to adopt — upgrading is the whole of it, and it applies to frames launched after
upgrading rather than to one already running.
