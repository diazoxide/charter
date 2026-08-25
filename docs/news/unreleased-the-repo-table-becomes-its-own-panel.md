---
version: unreleased
headline: The repo table becomes its own panel, and the attention row moves to the last line
---

The bottom of the frame was one pane holding two unrelated things stacked with nothing
between them — the status one-liner, and then the repo table running straight on out of
it:

```
0 todos · ⚠ plane root charter · dirty · work belongs in a workspace clone · ⚡1 running · F2 menu
├─ charter                    main↓9        ✓ passed
│  ├─ frame-inside-tmux                     ✓ passed
│  ├─ guard-all-or-nothing                  ✓ passed
```

They are two panes now, with a rule between them that tmux draws:

```
 ⬢ demo*                                                                        charter 0.53.0
─────────────────────────────────────────────────────────────────────── ──────────────────────
                                                                       │▪ personas 3
  (your agent session)                                                 │▸ steward          ✎47
                                                                       │
                                                                       │▪ todos 2
                                                                       │- wire the bottom split
                                                                       │- look at it
───────────────────────────────────────────────────────────────────────────────────────────────
▪ repos 6
  ├─ api-gateway                       main
  ├─ billing                           feat/invoices*   ✗ failed   !214
  ├─ charter                           main*↑9
  ├─ docs-site                         main             ? passed
  ├─ scheduler ⑂3                      main
  └─ web-app                           main↓2
───────────────────────────────────────────────────────────────────────────────────────────────
2 todos · ⚠ plane root charter · dirty · ⚡1 running · F2 menu
```

Top to bottom: identity, your session, the repo table, what wants attention.

**The attention row is last, and that is deliberate.** You asked for the repo list first
and the `0 todos · …` line after it, and there is a reason beyond the ask: the table's
height is its content's, so whichever of the two sits lower moves up and down the screen
as repos are cloned or go quiet. Anchoring the alert to the terminal's last line is worth
more than anchoring the table. An alert you have to go looking for is one you read late.

**The table is headed now**, `▪ repos 6` — the same heading style the sidebar's sections
got last release, from the same code, so the frame's two bordered components are labelled
the same way. It also gives the tree something to hang from: the first row is `├─`, a glyph
that means *there is more above me*, and once the status line moved out there was nothing.

**A workspace with no clones says so instead of showing you an empty box.** While the
table shared a pane with the status row, having nothing to table just meant no rows and
the pane was the one-line strip it always was — absence said it. In a bordered component
of its own, absence is an empty rectangle, and an empty rectangle reads as a table that
failed to draw. So it says what it is, with the command that changes it:

```
  no clones in demo · charter clone <repo> -w demo
```

That is a different sentence from `⋯ gathering this workspace's repos…`, which is what you
still get in the second before the launch's background gather lands. Not-looked-at-yet and
nothing-there are two different facts and drawing them the same way is what once made a
frame read as "no repos" on a plane full of them.

**Below 95 columns the table is not drawn, and now the pane is not there either.** A
frame narrower than the table's own columns never drew one — a row trimmed past the branch
loses the CI glyph and the open-change count, and a dirty, failing repo then reads as
clean. That used to be invisible, because the pane went on drawing the status row above the
missing table. On its own it would be a bordered box with nothing in it, so the slot is
dropped and its rows go back to your session. If you narrow a frame that is *already*
running, the pane cannot be un-split — a resize changes sizes, not which panes exist — so
it shrinks to one row and says `⋯ too narrow for the repo table — 95 columns needed`.

**The density levels are re-derived rather than patched.** They differ by edges again,
which is what a preset over `slots` can honestly express:

| level | edges |
|---|---|
| `minimal` | the two one-row strips, and nothing that costs rows by the repo |
| `normal` | the repo table as well |
| `full` | the sidebar as well — every edge charter draws |

`minimal` used to be `normal` with at most four rows of table, which after this split would
have cost your session a border row it did not cost before in order to show four repos. A
level whose whole purpose is handing rows back does not negotiate over four of them: it
drops the component. A fourteen-repo workspace now gets fifteen rows and a border back
rather than four rows. If you want the table at `minimal`'s verbosity, write the `slots`
list by hand and declare the level beside it — an explicit list wins over a preset.

## What to do

Nothing, if you have never written `[frame] slots` in your `charter.toml`. The default is
`["top", "bottom", "repos", "right"]` and you get the frame above.

If you *have* written that list, it still launches and it simply has no table — `slots` is
the primitive, and charter does not add a slot to a list you wrote by hand. Add `repos` to
it, or delete the line and take the default. Note the order: the list is the order the
panes are **split** in, and every split but `top`'s goes directly below your session, so a
slot listed *later* sits *higher* on screen. `bottom` before `repos` is what puts the table
above the strip.

One thing under the hood, in case you ever wondered why your panes drift after a resize:
tmux's `resize-pane` moves exactly one boundary, so in a stack of N panes only N-1 heights
can be asserted. With two strips they always traded with your session and charter never had
to name it. With three, the two below your session started trading with each other instead
— measured, at 200x50: the table came back one row tall and the attention strip six. The
harness pane is told its own height now, and the table takes what is left.
