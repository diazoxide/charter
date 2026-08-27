---
version: unreleased
headline: The frame reads as an application, and tmux is what paints the background
---

*"lets add some background to our top/bottom/side bars. making them like a UI APP"*

A fill on a one-row bar is a colour, not a UI. What makes a terminal frame read as an
application is a region with a name, a consistent inset, a row you can see you are on, and
a status you can read without colour vision — and all four of those are on now, whatever
your `charter.toml` says, because none of them names a colour charter chose.

```
 ▪ personas 5
▸ steward          ✎47      <- the whole row, inverted, to the pane's edge
 ▫ forge            ✎9
 ▫ reddit           ✎7
 ▪ todos 3
 - ship the sidebar
```

**The heading is bold and the count stays dim.** No row is added anywhere — the frame's
panels are the same height they were, and the fifteen-plus tests that assert a panel's
exact line count never had to change.

**The active persona's row is the whole row**, inverted across the pane to its last
column, rather than a `▸` at the start of it. Reverse video is your own foreground and
background exchanged, so it is correct on every colour scheme — including the Solarized
palettes, where every grey charter could have picked is somebody's background.

That one had a defect worth naming, because it only appears once you build it: **a reverse
row cancels itself at the first escape inside it that resets everything**, and charter's
rows carry one after every coloured span. Highlighted naively, `▸ steward ✎47` is inverted
for two words and plain for the rest of the pane. The fix re-asserts reverse after each
reset — and its own first version was this project's recurring bug for the sixth time:
"resets everything" is a *numeric* parameter value, so `\x1b[00m` and `\x1b[1;00m` reset
everything too and a test written against the string `\x1b[0m` passes with the row half
highlighted.

**A status is never colour alone.** Every one in the frame carries a glyph or a word that
says the same thing — `⚠` on an alert, `⚑`/`✗` on a persona whose charter is a draft or
broken, a number beside a badge. That is now a test rather than a habit: every panel is
drawn healthy and drawn wanting attention, every escape is stripped from both, and the two
must still differ.

**`NO_COLOR` is honoured, and until now nothing in charter honoured it.** Set it to
anything at all — including the empty string, which is what a shell that exports it with
no value gives you — and the panels emit no escape sequences at all. The same is true of a
panel whose output is not a terminal (`charter panel top --session x > /tmp/log`), which
used to write a clear-screen and full colour into the file.

## A background behind the panels, if you want one

```toml
[frame]
chrome = "dark"     # or "light", or "off" — the default
```

**tmux paints it; charter sets an option.** `window-style` and `window-active-style` are
settable per pane, and tmux fills the pane's whole rectangle from them. So the background
costs nothing on a repaint, cannot wrap a line, covers the cells no renderer wrote,
survives a resize and a reattach, comes back by itself when a dead panel is respawned into
the same pane — and the focused panel is a shade off the others for free, drawn from tmux's
own idea of which pane is live. It is set per pane, so **the pane your agent runs in is
never touched.**

**It ships `off`, and there is no `auto`.** Charter cannot see your theme: a colour query
through tmux gets no answer, and `$COLORTERM` inside a pane describes the terminal that
started the tmux *server* — detach at your desk, reattach over ssh, and every panel still
reads the old answer. An `auto` that guessed would be a guess wearing the word for a
measurement, and a default background charter picked would make a frame that was fine
before an upgrade come back worse for anyone whose terminal is the other colour. Turning it
on is one line; the four elements above are on either way.

**It is a word and never a style string**, and that is a boundary rather than a
simplification: tmux expands formats inside a style value at draw time, and `charter.toml`
is a committed file that arrives from someone else's machine. Anything that is not one of
the three words — a fourth word, a style, a list, a table — leaves the frame at `off` and
charter still runs. `NO_COLOR` refuses the background too: no colour on your screen caused
by charter means none, whichever process puts the bytes there.

## To adopt

Nothing, for the four elements — upgrading is the whole of it. For the background, add
`chrome = "dark"` (or `"light"`) to `[frame]` in your plane's `charter.toml` and relaunch
the frame. charter's own plane has done exactly that.
