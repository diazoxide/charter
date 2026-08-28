---
version: unreleased
headline: Each pane can carry its own background and its own inset
---

*"i think will be good to make background colors of panes configurable / on sidebar and
repo-list pane add padding / so each pane can have custom background color from
charter.toml, paddings also can be configured"*

`chrome = "dark"` on a terminal that is already black paints four panes the colour the
terminal already was. The frame gained a background and no structure — and the screenshot
that came with the report showed the other half of it too: every panel's content starting
in column 0, flush against the pane's edge.

Both are the same finding. **A frame reads as an application because its regions are told
apart**, and one frame-wide word cannot tell them apart: whatever it says, it says about
all four panes at once.

So each component says its own:

```toml
[[frame.component]]
use = "identity"
bg = "blue"

[[frame.component]]
use = "attention"
bg = "black"

[[frame.component]]
use = "repos"
bg = "black"
pad = 2

[[frame.component]]
use = "sidebar"
bg = "brightblack"
pad = 2
```

Read back out of a real tmux — `show -p` per pane, then `capture-pane -p`:

```
top     window-style=bg=blue         window-active-style=bg=brightblue
 ⬢ demo*  ◆ persona none · forge · release · statusline · steward        charter 0.53.0

repos   window-style=bg=black        window-active-style=bg=brightblack
  ▪ repos 3
    ├─ charter                           main*
    ├─ charter-docs                      feat/panes
    └─ harness-wrapper                   main*

right   window-style=bg=brightblack  window-active-style=bg=black
  ▪ personas 4
  ▫ forge
  ▫ release
  ▫ statusline
  ▫ steward

harness window-style=unset
```

**`bg` is one of seventeen words**: `default`, the eight ANSI colours and their eight
`bright` forms. Names and never `colour236` or `#1c1c1c`, for the reason the rest of
charter's colour uses names — `blue` is a slot in *your* palette and a cube index is a fixed
point no theme moves — and this file is committed, so the colour you write is read on a
machine whose theme you have never seen. The focused pane is drawn in the other member of
the pair (`blue` focuses to `brightblue`), so you can still see which pane is live. tmux
paints it, as it already did for `chrome`: no cost on a repaint, nothing to wrap, and it
comes back by itself when a panel dies and is respawned into the same pane.

It does not need `chrome` to be on. `chrome`'s default is `off` because a background
charter chose is wrong on somebody's terminal; a `bg` is a line you wrote by hand about one
pane.

**`pad` is how many cells that pane leaves empty at its left and right edges.** Charter
draws this one, because tmux paints backgrounds and insets nothing — and the whole of the
care is in where the cells come from. **The pad comes out of the pane's own width.** The
repo table already gives up columns in a written-down order when its pane is narrow; a
padded pane simply starts that arithmetic two cells earlier, so the row is composed for the
narrower pane rather than composed wide and then pushed off its own right edge. One cell of
overflow does not look like a margin — it wraps every row below it onto the next line.

On a pane too narrow to afford it, the pad is dropped **whole** rather than quietly
reduced, so a narrow frame looks exactly as it did before you wrote one. And if a `pad`
does push the repo table under the width it needs, the pane says so — with your pad already
added into the number it quotes, so widening by it works.

There is no vertical pad. `identity` and `attention` are one row each, so a top pad would
not inset them, it would delete them; the repo table is sized to its content, so a vertical
pad there removes a repo rather than moving one, and which repo goes is a ranking charter
made on purpose. Horizontal padding gets narrower; vertical padding disappears.

A component charter did not write gets both without opting in to anything: its `ctx.width`
is the padded width, and its rows are inset on the way out.

`NO_COLOR` still refuses every background — the fill is tmux's paint, not charter's, and
"no colour on your screen caused by charter" means none whichever process puts the bytes
there. A `bg` charter does not know, or a `pad` outside `0`–`8`, takes the whole arrangement
out of play the way any other unusable value in `[[frame.component]]` does: you see your
arrangement ignored, which is visible, rather than one pane quietly losing what it asked
for.

Nothing to adopt — every existing frame draws exactly as it did. `chrome` is unchanged and
is still the right way to say one thing about the whole frame.
