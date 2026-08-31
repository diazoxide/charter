---
version: unreleased
headline: The frame reads on the terminal it is on — paired foregrounds, an uncoloured highlight, a live-pane cue, and the accent palette is yours
---

*"we need to care about text colors too, maybe also make them configurable, because on
bright background bright text is not readable"*

Four things, one cause. Every colour decision in the frame was taken on a dark terminal, and
each of these is what that costs on a light one — or the mirror of it, which is the half a
developer on a dark terminal never sees fail.

## A `chrome` word carries the text that goes on it

`chrome = "light"` set `bg=white` and nothing else, so every cell no renderer coloured drew
in the foreground *your terminal* picked to sit on *its own* background. On a dark theme
that is white. The attention strip's own bytes, measured:

```
ESC[0m ESC[47m 7 todos · F2 palette · ESC[2m…
       ^^^^^^^ the pane's `bg=white`, and no `SGR 3x` anywhere after it
```

Not a background that clashes — a panel whose text is not there. `dark` is the same failure
mirrored on a light theme. Both are charter's own recipes, so both now carry their own
foreground: `bg=black,fg=white` and `bg=white,fg=black`.

**Two words and not seventeen, and the line is where the measurement stops.** `black` and
`white` are the poles of the sixteen — a theme is not free to render its white darker than
its black — so charter can say what goes on them. It cannot say what goes on `bg = "blue"`,
so a pane that named its own background still gets no foreground it did not ask for;
`[frame] text` is how that pane is told, and if you set it, it wins.

## The row you are on stops painting yellow on your foreground

`docs/frame.md` argued the selected row is right on every colour scheme *precisely because
reverse video names no colour*. Both surfaces that use it named one. Read off a live frame:

```
ESC[7m ESC[35m ▸ ESC[1m steward ESC[0;7m        ESC[32m ✎47 ESC[0m
ESC[7m   ESC[2m ├─ ESC[0;7m ESC[36m billing       ESC[33m main* ESC[39m
```

`SGR 7`, and then magenta, cyan and **yellow** — colours chosen for your background, painted
on a cell whose background is now your foreground. On a dark theme the selected repo row
drew yellow on light grey, which is the worst pair the sixteen can make, on the one row
charter uses to say "this is the one you picked".

Every colour inside the inversion is **deleted** — a deletion and never a substitution,
because charter cannot know what your magenta looks like on your own foreground but it can
decline to paint a pair it has no way to check. Nothing the row was saying is lost: the
glyphs still say it, and charter's suite already fails if a status stops being
distinguishable with its escapes stripped. Bold and dim survive; neither can be wrong on a
ground charter cannot see. It is done in `chrome.reverse` rather than at the two call sites,
so a component composing with `ctx.chrome["selected"]` gets the same answer.

## Something finally says which pane is live

With the shipped `chrome = "off"` and no `bg`, every pane had your terminal's background,
both rule styles were pinned to one value on purpose, and the answer to "where is the
keyboard" was: nowhere. That matters more than it sounds, because `F12` exists for "you are
in a pane that has stopped answering" and the frame gave no way to notice you were in one.

tmux marks the active pane's borders with a small arrow, and charter pins that on now. Read
off an attached client, charter's own shape, the same rule with the focus in three places:

```
harness active   ESC[2m ─↑─────────────────────────────────┴──────────────
sidebar active   ESC[2m ───────────────────────────────────┴─↑────────────
footer active    ESC[2m ─↓─────────────────────────────────┴─↓────────────
```

**One escape, at the start of the row, and none anywhere else in it.** That is what makes
this not the two-coloured-rule defect arriving through a new door: the rule is the same
one-colour rule charter has drawn since #514 and the only thing that moved is which cell
holds an arrow. A cue made of a second *style* — the "weight rather than a colour" the issue
proposed — does put a seam mid-line, measured, and is the arrangement charter did not take.

Over a surface with the shipped `rules = "hidden"` the arrow is the colour of the cell behind
it, like the rest of that rule, so it disappears exactly where the pane's own one-shade-off
background is already telling you. Below tmux 3.3 the option does not exist and that plane
gets the frame it had.

## `ok`, `warn` and `bad` are words your plane chooses

```toml
[frame]
ok   = "green"      # any of `bg`'s seventeen words — these three are the defaults
warn = "yellow"
bad  = "red"
```

This is the gap `[frame] text` named and could not close. `text` fixes a pane whose
background is *inverted* relative to your terminal's; on a terminal that is already light it
has nothing to do, and what is hard to read there is these three. They shipped
un-configurable on the argument that they are slots in **your** palette rather than colours
charter picked — which says the colour is yours, and does not say it is legible on the
ground it is drawn on. Those are different claims. **Yellow on tan is the pair no palette
was designed for.**

Charter still cannot compute which of the sixteen would be legible — the sixteen names have
no fixed RGB, a colour query through tmux gets no reply, and `$COLORTERM` inside a pane
describes the terminal that started the *server*. So it asks, in the vocabulary you already
answer `bg` and `text` in.

`default` is a real answer and often the best one: it is the pane's **own** foreground, which
is your `text` where you set one and your terminal's where you did not. `warn = "default"` is
"stop colouring the warnings".

One reading, so charter's own panels, the status line and a component composing with
`ctx.chrome["warn"]` cannot come out three colours. A component that hard-coded its own green
is left alone and still shows it — charter neither recolours somebody else's green nor
escapes it into their pane as text. And nothing that is not a verdict moves: a repo's
identity colour, `running` on a pipeline, and the muted `manual`/`canceled`/`skipped` marks
stay exactly where they were.

## Unchanged

A plane that says nothing gets a byte-identical frame. `chrome = "off"` with no `bg` and no
`text` emits no pane option at all, the shipped accents are the three colours charter has
always drawn, and `chrome.served_params()` is the same seven numbers it was — a plane cannot
narrow what a component may write, only widen it. `NO_COLOR` still wins over all of it.
