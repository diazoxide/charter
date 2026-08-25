---
version: unreleased
headline: Switch workspace and persona from inside the frame, and pick one before it opens
---

Two round trips are gone. Changing workspace meant quitting the frame, running `charter
workspace use <name>`, and launching again; choosing where to work at all meant knowing
before you typed `charter claude`, because the launcher resolved a workspace silently and
went straight in.

**`F2` now lists them.** The menu the hotkey already opened — "Detach", and the three
density levels — has two more rows, each opening a submenu:

```
┌─charter───────────────────────────┐        ┌─charter · workspace───┐
│ Detach                        (1) │        │   default         (1) │
│   density: minimal            (2) │        │ * harness-wrapper (2) │
│ * density: normal             (3) │   ▸    │   release-0-54    (3) │
│   density: full               (4) │        │   user-reporting  (4) │
│ workspace: harness-wrapper  ▸ (5) │        └───────────────────────┘
│ persona: forge  ▸             (6) │
└───────────────────────────────────┘
```

Submenus rather than two more keys, deliberately: a tmux key table is server-wide with no
per-session form, so a second and third `bind` would cost every frame on the machine two
more keys and still give you nothing inside a tmux of your own, where charter binds no key
at all. One hotkey, already paid for.

**Choosing one moves the frame, not a file something might read.** A workspace switch
writes the choice under the frame's own id — the same pointer `charter workspace use`
writes from inside the frame, which is what has always made the panels follow — records it
as the frame's workspace so a panel respawned later agrees with the ones already up,
re-gathers the repo table for the new workspace, and *then* bumps the frame so every panel
repaints. The gather happens before the bump on purpose: a panel reads the version first
and the cache second, so bumping first would repaint the new workspace's name over the old
workspace's repos. A persona switch is the same thing minus the gather.

**And the menu follows.** It is rebuilt after every switch, so the next `F2` names the
workspace you are in rather than the one you left, the `*` sits on the row you chose, and
a workspace or persona made since the frame opened is in the list. The menu is rebuilt
after a *refused* switch too — the frame did not move, but your plane may have, and a
pinned frame is the one that would otherwise never see a new name.

**Rows past the ninth are drawn with no shortcut**, and that is now literally true. They
used to be given the key `-`, on the belief that tmux spells "no key" that way. It does
not: `-` is a real key, and on a list long enough to reach the tenth row a stray hyphen
performed a real workspace switch while every row under it advertised a `(-)` that did
nothing. Ten rows is where the digits run out; the arrow keys reach the rest.

**Two switches are refused, and a refusal is put on your screen.** A frame launched with
`$CHARTER_WORKSPACE` or `$CHARTER_PERSONA` set is *pinned*: the variable is in every panel
pane's environment for as long as the pane lives, and no file charter writes outranks it.
Reporting "switched" and then drawing the pin would be the worse outcome, so the menu says
`cannot switch: $CHARTER_WORKSPACE pins this frame to '<name>'` instead. A name that is
not there is refused with the ones that are — the menu never creates a workspace. Both
land as a one-line tmux message on the screen of whoever pressed the key, which matters
with two terminals attached to one frame: a message sent without naming a client was
measured landing on the most recently attached one regardless of who asked.

**The session lock moves with you rather than locking you out.** `charter workspace use`
locks a session to what it selected so a workspace cannot be swapped out from under a
running task. But a keypress on a menu *is* you — and the switcher's own first write would
otherwise take a lock that its second write hit, leaving a switcher that worked exactly
once. So the menu overrides the lock and names what it overrode:
`workspace → beta  (lock moved from 'alpha')`. Nothing moves without you being told what
moved.

## And a picker at launch

If **nothing chose a workspace** — no `--workspace`, no `$CHARTER_WORKSPACE`, not standing
in a workspace tree, no per-session or per-terminal pointer, no declared default — the
launch was about to answer `default`, a name nobody picked. It asks now:

```
  charter · which workspace?

     1  * default          —
     2    harness-wrapper  7 repos
     3    user-reporting   1 repo

     n    create a new workspace
     q    cancel — start nothing

  workspace [default]:
```

A number, a name, or Enter for the marked one. The clone count is there because the repo
table is empty until the first gather lands, so at pick time it is the only thing on
screen that tells two workspaces apart.

`n` prompts for a name, checks it against the workspace alphabet, and asks
`create <name> and switch to it? [y/N]`. Anything but a `y` goes back to the list. A typo
creates nothing, a cancelled picker creates nothing, and naming a workspace that already
exists selects it rather than making a second one — creating is a real side effect on your
plane and the fat-fingered path had to be the one that does not take it.

`q`, Ctrl-C, or a closed stdin end the launch having started nothing, with exit code 130.
Not 0: a script that ran `charter claude` and got 0 back would read a frame that never
started as one that ran and exited cleanly.

**Picking is the confirmation that locks**, because that is what choosing a workspace has
always meant — and the launch says so on the line after your answer rather than leaving
you to find out at the next `charter workspace use`. What makes it liveable is that the
frame has its own way out: `F2 → workspace` overrides the lock and tells you it did, so a
choice you just made at a prompt does not send you back to a shell to change it.

**It does not ask twice, and it cannot ask a script.** What you pick is written as your
terminal's own pointer, so the next launch from that terminal has an answer and goes
straight in — you answer once, not every launch. And the prompt sits below every
non-interactive exit charter already had: `--no-frame`, a redirected stdout, and now a
stdin that is not a terminal each return before it, so `charter claude` run from a script
or by another agent cannot block on a question nobody is there to answer.

## New flags

`charter <harness> --workspace <name>` runs in that workspace and skips the picker — the
top rung of charter's own precedence, said on the command line. `--pick` asks even when
something already chose, for the launch where you want to move and a pointer says
otherwise. A pin still outranks both: `$CHARTER_WORKSPACE` cannot be moved from inside the
frame either, so offering a choice that could not take effect would be worse than not
offering one.

Nothing to adopt — upgrading is the whole of it. Inside a tmux you already have, charter
still binds no key, so there is no menu there; `charter frame-switch --workspace <name>`
and `--persona <name>` do the same job typed by hand from inside the frame.
