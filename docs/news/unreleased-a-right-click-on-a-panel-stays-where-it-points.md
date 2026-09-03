---
version: unreleased
headline: A right-click on a panel stays where it points too, and tmux's own pane menu is still there
---

**If you set `[frame] mouse = true`, right-clicking a panel no longer takes your keyboard
with it. Right-clicking anything charter did not create — your harness, a pane you split
yourself — still opens tmux's own pane menu, exactly as it always did.**

Charter stopped left clicks moving the keyboard in 0.54.0. Right clicks still did, and
right-clicking became something you would actually do the moment chat tabs grew a menu.
The cause is the same one, one button over: tmux's default root binding for
`MouseDown3Pane` forwards the report only *after* `select-pane -t =`, and a charter panel
always takes that branch — a panel that declares `click` or `scroll` asks its own terminal
to report, which is what makes it clickable, and that is exactly the condition tmux tests.

On the gesture the menu is for this cost nothing: the menu takes the keyboard anyway. On a
**miss** it cost you a click on the harness, or `F12` — and a miss is most of what a
right-click on a bar is, because the heading and the padding are about no tab at all.

## Why it is not the same fix as the left button's

`MouseDown1Pane`'s default is two commands, so charter could write out the half worth
keeping. This one's is not. Read back off a real tmux 3.7c, `MouseDown3Pane` is a
conditional whose other branch is tmux's whole pane menu — Copy Line, Paste, Horizontal
Split, Kill, Zoom — built out of `#{mouse_word}`, `#{mouse_hyperlink}`,
`#{pane_floating_flag}` and a dozen more formats. **1849 characters on 3.7c, and 1378
different characters on 3.2**, whose menu has no hyperlink rows and no floating panes and
whose quoting is not even the same style.

Applying the left button's fix here was measured, on a real server with a real client on a
real pty, on both versions:

```
bind                      right-click a panel   right-click harness  right-click your own split
tmux's own default        delivered, MOVED      untouched            tmux's pane menu
the left button's shape   delivered, unchanged  untouched            MOVED, and NO MENU
charter's (the wrap)      delivered, unchanged  untouched            tmux's pane menu
```

The middle row is the trade this release refused. It fixes a focus steal one click puts
right by deleting a documented tmux affordance from panes charter has nothing to do with,
inside charter's own window.

## What charter does instead

It reads your server's own binding back with `list-keys` at launch and re-emits it with its
own panel test in front of it — so the branch that runs for every pane charter did not
create is *whatever was already there*. **If you rebound that key yourself, your binding is
what still runs.** A hard-coded else-branch could not have promised that at all.

Two smaller things fell out of the measurement and are recorded in the code rather than
here: `list-keys -T root MouseDown3Pane` prints the binding on tmux 3.2 and prints nothing
at all, at rc 0, on 3.7c — so charter lists the whole root table and picks the line out of
it. And the bind is issued as its own tmux command rather than written into the frame's
config file: it carries a page of tmux's own config language as an argument, and one
unbalanced brace in a binding charter did not write would otherwise have taken `mouse`,
`history-limit` and the `F2` hotkey down with it.
