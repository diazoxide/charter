---
version: unreleased
headline: The `F2 palette` hint is a button, the persona column is a list you pick from, and every glyph on it has a name
---

*"I clicked the one affordance the frame advertises and nothing happened. Then I read
`frame/slots.py` to find out what `◦` meant."*

Three reports from one evening of driving a real frame, and they were one defect wearing
three hats: charter drew nouns, drew a roster with a cursor-looking marker on it, drew the
words `F2 palette`, and handled no pointer event on any of them. Of the four panels charter
ships, `repos` was the only one that had ever declared an event at all.

## `F2 palette` does what it says

```
7 todos · F2 palette · ▪ ledger · main · clean
          ^^^^^^^^^^  click it, and the palette opens
```

It was the only affordance charter advertised on screen and the one thing on the row that
could not be operated the way it is drawn — a key name beside a noun, which is a button
everywhere else you have ever seen one. It is a button now. So are the two nouns on the
identity row above it:

```
 ⬢ alpha    ◆ steward                                        charter 0.55.0
 ^^^^^^^    ^^^^^^^^^  both open the same palette
```

**All three go to the same place on purpose.** `⬢ alpha` names the workspace you are *on*
and `◆ steward` the persona you *are*, so a click on either can only mean *let me pick
another one* — and picking needs a list. The palette is that list, it is where both nouns
were already reachable, and opening it finishes on the pointer alone: it makes itself the
active pane, so your keyboard reaches the rows it just drew. Charter cannot grow a second,
pointer-driven chooser inside a one-row pane, because a keypress does not reach a panel at
all — tmux gives your typing to the active pane, and that pane is your harness.

**Everything else on those two rows still answers nothing, and each one is a readout rather
than an offer**: the charter version, the context gauge, the todo count, the alert, the
in-flight spinner, this session's news, and `▪ ledger · main · clean`. That last is the
sharpest — it is the readout of a row you selected on *another* pane, so a click on it could
only mean "select what is already selected", which is the one gesture the repo table and the
tab bars both already refuse. So does the ` · ` between two fields, and so does the space
past the last one.

## A persona row is a row you can pick

```
▪ personas 6
▸ steward
▫ docs ◦             ⚑     <- click it, and the frame is being `docs`
▫ forge ◦            ⚑
```

The column was already drawn as an inverted-row list with a marker on the active one, which
is the visual vocabulary of something you pick from. Clicking one runs exactly what `charter
frame-switch --persona docs` runs — the same command, the same refusals, the same sentence on
your own screen when one fires.

**A click *switches* here, for the same three reasons it switches on the tab bars**: charter
acts on the press, which is never the half a drag delivers unpaired; the row you left is
still in the column one click away; and nothing on the machine could finish a
select-then-confirm gesture, so a column that merely selected would draw a second mark
nothing could ever act on.

Clicking the persona you already *are* does nothing, which is also what stops a double-click
switching twice. Neither does the `▪ personas 6` heading, the `…(+N more)` row — it stands
for the personas that are *not* drawn — the `no personas` line, or anything below the column.

## Every glyph on a persona row now has a name — and says it in the frame

**Click the badge column on any persona row and the legend lands on the attention row for
ten seconds:**

```
▫ forge ◦            ⚑     <- click the ⚑
                             ↓
◦ no usable vault · ! vault unhealthy · ⚑ draft charter · ✗ broken config · ✎ memories …
```

That is the dwell a workspace switch already uses to say what it did, so the answer costs
no extra row and clears itself. It works on the row you are already on, which is where the
question usually comes from — and the *name* half of the row still switches, so a persona
row is two cells that mean two things.

`docs/frame.md` names all ten glyphs in a table, so `charter docs show frame` is the long
version. The two that sent an operator into the source:

- **`◦`** — this persona **declares a vault charter cannot use here**: either this machine
  has no vault by that name, or it has one whose file does not exist yet. `charter persona
  create` writes the declaration and nothing writes the vault, so this is the ordinary state
  of a persona nobody has given credentials to. **It is not a warning.**
- **`⚑`** — the persona's **charter is a draft**, so charter generates no sub-agent for it
  and it cannot be dispatched. `charter persona show <name>` says what it still needs.

A plane whose six personas came from `charter persona create` and were never given a vault
shows `◦` on every row and `⚑` beside most of them. Six personas, five flags, nothing wrong —
which was exactly the reading that made this worth writing down.

## What has not changed

**You still need `[frame] mouse = true`**, and this entry does not change that: with mouse
off, tmux asks your terminal to report from the *active* pane's own request alone, so a click
on a panel is not an event charter drops — it is an event that never happens. `F2` reaches
every one of these routes at any width and with no mouse at all, which is the rule charter
holds itself to: a pointer affordance always has a key or a palette row beside it.

**Your keyboard stays on the harness.** A persona switch acts where you pointed and leaves
you typing where you were. The palette is the deliberate exception, exactly as it is from the
key: a modal surface you cannot type into is not one.

**One thing on the sidebar is still unreachable, and it is worth saying rather than
implying.** The todo list's `…(+N more)` line still cannot be opened from the frame — the
hidden todos are `charter ws todo`'s. Every cheap way to reach them was worse than the honest
count: a wheel with no keyboard twin, or a scroll offset shared across processes that would
bump the whole frame on every notch. It wants a read-only list surface, which is its own
change.

Verified end to end on a real tmux — 3.7c and the 3.2 floor — with a real client on a real
pty, real `charter panel` processes holding the strip's and the sidebar's panes, and a real
detached `charter frame-palette` / `charter frame-switch --persona` started by those panels
out of their own handlers: the palette pane appears and takes the keyboard, `▸` moves to the
persona that was clicked, the keyboard never leaves the harness for the switch, and a click on
a readout, a separator, a heading, or the row you are already on does nothing at all.
