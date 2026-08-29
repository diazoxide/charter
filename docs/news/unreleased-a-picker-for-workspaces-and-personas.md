---
version: unreleased
headline: The palette stops listing every workspace and starts opening a picker for them
---

The palette arrived earlier in this same release handing you one flat list with everything
in it — detach, three densities, and then every workspace and every persona the plane has,
one row each. On a plane with thirty workspaces the four things a frame can *do* were four
rows in thirty-seven.

**One row per noun now, and each opens a list of its own:**

```
charter · 12 to choose from
> workspace: alpha — pick another
    persona: steward — pick another
    change: component-api-2 — pick another
    detach — leave the harness running
    repo: select the next row
    repo: select the previous row
    density: minimal
    density: normal
  * density: full
    chrome: off
  * chrome: dark
    chrome: light
```

Two of those doorways are this change; `change:` is a third, added with the cross-repo change
surface later in the same release, on exactly the same mechanism.

Enter on `workspace:` redraws the **same pane** with the names alone:

```
workspace · 4 to choose from
  * alpha
>   beta
    default
    zebra
```

Type to narrow, Enter to switch, Escape to leave having changed nothing — the same keys,
the same scrolling, the same `F12` back to the harness. It is not a second surface: a
picker *is* the palette, over a different set of rows.

**The palette still answers "where am I" without opening anything.** That is why the row
says `workspace: alpha` rather than just `workspace`, and it is why typing the name you are
already on still finds it.

**A workspace is a name, not an action, and that was the actual defect.** Charter's action
contract promises *fire-and-report*: `run` starts work and returns. Forty workspaces
registered as forty actions meant forty `run`s, each of which started a whole second
charter process to write two files. The picker chooses the name first and switches once, in
the pane you are already looking at — no second process, and nothing racing the palette's
own teardown for the pane.

**A refused switch says so, on your own screen.** Three things can need a sentence and all
three get one:

* a frame launched with `$CHARTER_WORKSPACE` set is *pinned* — that variable is in every
  panel pane's environment for as long as the pane lives, and nothing charter writes
  outranks it. The row carries `cannot switch: $CHARTER_WORKSPACE pins this frame to
  'alpha'` and opens no picker at all, rather than showing you a list of moves that would
  not happen. The persona row is unaffected: one pin, one noun.
* a workspace that stopped existing between the list being drawn and Enter being pressed is
  refused with the names that *do* exist beside it, never created.
* a switch that overrode the session lock says what it overrode — `workspace → beta  (lock
  moved from 'alpha')`. An agent inside the frame took that lock, and its next command
  would otherwise act on a workspace nobody told it had moved.

**A hostile name is one row and runs nothing.** A workspace or persona is a directory a
commit can add, and a filesystem forbids only `/` and NUL — so a name can hold a newline, a
U+2028, an escape sequence, a quote or a `#`. Every name is made one line *before* any
column width is measured, so it draws as exactly one row with its escapes visible; and the
switch checks the name against the same alphabet `charter workspace use` does, so a name
charter would not accept is refused rather than acted on.

**And the switch is still the frame's own identity, not a pointer.** Choosing a name writes
the per-session pointer under the frame's id and the frame's recorded workspace, refreshes
the repo table, and *then* bumps the frame — which is what makes every panel repaint against
the new plane rather than one of them noticing eventually.

Nothing to adopt. `charter frame-switch --workspace <name>` still works typed by hand, and
still says what it did.
