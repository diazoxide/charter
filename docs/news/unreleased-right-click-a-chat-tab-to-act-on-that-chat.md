---
version: unreleased
headline: right-click a chat tab and charter draws a menu about that tab — its transcript, and closing it
---

*"All functionality in the F2 menu — extract them and integrate the features on
components. Tabs should have right-click context-menu support."*

Right-click a tab on the `chats` bar and charter draws two rows about **that** chat:

```
  chat api.2
  > chat: previous transcript — api.2
    chat: close api.2 — stop it and do not bring it back
```

Both were already in `F2`, and both were about the chat you were *in*. On a tab they are
about the tab, so closing another chat no longer means switching to it first.

## Two findings decided the shape, and both were the opposite of what was expected

**Right-click already reached charter's panels. It needed no tmux binding.** Measured on
tmux 3.2 and 3.7c against a pane requesting exactly what charter's panels request, with an
SGR button-2 report injected into a real client:

```
mouse=off  custombind=no   PRESS ^[[<2;50;7M  RELEASE ^[[<2;50;7m  bind fired: no
mouse=on   custombind=no   PRESS ^[[<2;50;7M  RELEASE ^[[<2;50;7m  bind fired: no
mouse=on   custombind=yes  PRESS —           RELEASE ^[[<2;50;7m  bind fired: YES
```

With tmux's own mouse off no mouse binding fires at all. With it on, tmux's *default*
`MouseDown3Pane` tests `#{mouse_any_flag}` — which is 1 precisely because the panel asked
for reporting — and takes its `send-keys -M` branch, so tmux's own menu does not appear
either. The third row is the trap: **a custom `bind -n MouseDown3Pane` that omits `send -M`
swallows the press.** Binding is what would have broken this. Charter binds nothing.

The events were already decoded, already delivered and already named `right`; they were
dropped by one comparison.

**And exactly two rows in the whole palette are about a tab.** Catalogued by scope: detach,
both next/previous pairs, the densities, the chromes, the todo row and the regather are
about the FRAME; the workspace, persona and chat pickers and `charter: quit` are about the
PLANE. There is no `stop` distinct from close, and no rename in the frame at all. So this
is not a reduction of `F2` — the palette keeps every row it had, because none of the others
has a tab to sit on.

## Charter draws it, not `display-menu`

`frame/menu.py` was deleted rather than deprecated when the palette arrived, and re-measured
at charter's 3.2 floor `display-menu` is worse than it was then: **no styling flags at all**
(`-s`, `-S`, `-H`, `-b` each rc 1, so a refused row cannot be dimmed or explained), a key as
well as a command per item, per-client, totally modal at the server — every panel process on
the window stops being drawn while it is up — and a hard `client_height − 2` item cap past
which it **draws nothing and exits 0**.

So the menu is `frame/palette.py` over a different row source: the same overlay pane, carved
off the same harness, by the same argv, behind the same sweep that stops a second `F2`
leaving an invisible pane holding a live process, and with the same `F12` armed before the
surface can capture anything.

## Close is last, and it is still confirmed

Charter puts destructive rows at the bottom of a list because a palette's cursor starts on
the first row that can run. A menu that opens under the pointer arrives with even less
warning than one that opens under a keypress, so that placement matters more here, not less.

`chat: close` starts nothing. It opens the same warning `F2 → chat: close` opens — the same
plan, narrowed to one chat, naming what stopping it costs and what will not come back — and
the keypress on *that* is what stops the harness. A chat with nothing left to stop gets a row
saying so and no confirming row at all, so there is no Enter that quietly succeeds at
nothing.

`F2` keeps every row either way. A right-click menu is invisible until you try it, so it is
a faster route to two things and never the only one.

## If your terminal keeps button 2, nothing happens — and that is the whole failure mode

The measurement above injected bytes into a pty, which **bypasses the terminal emulator**.
Whether a given emulator forwards button 2 to a mouse-reporting application or serves its own
context menu is emulator-dependent and configurable — iTerm2 3.6.11 ships
`"Button,1,1,," -> kContextMenuPointerAction` beside a profile with `Mouse Reporting` on, and
which wins is not determinable from its plist.

So every path degrades to *never fires*: an absent or unspellable tab is not an error, it is
the ordinary palette opening instead. Nothing is refused, nothing is half-drawn, and `F2`
still reaches both rows.

## One cost, measured and left open

With `[frame] mouse = true`, tmux's forwarding branch for button 3 is
`{ select-pane -t = ; send-keys -M }` — it **selects the pane before forwarding**. That is
the focus steal charter already fixed for the left button, one button over, and it is not
closed here: a right-click that lands on a panel and opens nothing leaves the keyboard on
that panel until you click the harness or press `F12`. It costs the gesture this change is
for nothing, because a menu that opens takes the keyboard anyway and gives it back on the way
out.

The left button's fix works because tmux's default for that key is two commands charter can
write out verbatim in the else-branch. Button 3's else-branch is a `display-menu` a page long
built out of a dozen formats, and it differs between the tmux versions charter supports — so
replacing it would take tmux's own pane menu away from the harness and from panes you split
yourself, inside charter's window, to fix a steal that one click puts right. It is written up
where it lives rather than fixed in passing.
