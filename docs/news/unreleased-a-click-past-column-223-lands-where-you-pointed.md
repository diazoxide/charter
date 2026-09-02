---
version: unreleased
headline: A click past column 223 lands on the tab you pointed at, and now there is a test that says so
---

Nothing changed in charter for this one. What changed is that a hazard nobody had measured
is now measured, and the line that protects you from it is pinned so it cannot be tidied
away.

**The hazard.** Every terminal limitation the frame has run into so far degrades the same
safe way: the event never happens. You click a panel with `[frame] mouse` off, and no bytes
are produced at all. You click one from a Linux console, which speaks none of the mouse
protocols, and nothing arrives. A dead affordance is annoying; it is not wrong.

This one was different. The original mouse protocol spells a column as a single byte at
`column + 32`, so column 224 needs byte 256 — and wraps. tmux's own release notes say it
got that wrong until 3.3: *"Do not report mouse positions (incorrectly) above the maximum
of 223"*. tmux 3.2 is the floor charter promises to run on, and a tab bar is the widest
single row the frame draws. On a 244-column window, a wrapped column is not a click that
does nothing — it is a click on a **real but different tab**, which would switch you to a
workspace you never pointed at.

**Measured on the real 3.2 binary and on 3.7c, at 244 columns, with a real client on a real
pty.** There are two legs and only one of them can wrap:

| | tmux 3.2 | tmux 3.7c |
|---|---|---|
| your terminal → tmux | tmux asks for SGR (`?1006h`) **unconditionally** | same |
| tmux → a pane asking for SGR | `col 240` arrives as `col 240` | `col 240` arrives as `col 240` |
| tmux → a pane asking *without* SGR | `col 240` arrives as **col ~16** | clamped to 223 |

So the wrap is real, it is on the third row, and the single thing keeping charter off that
row is that every panel asks for SGR *first*: `\x1b[?1006h` then `\x1b[?1000h`. That looks
like belt-and-braces and is load-bearing at the floor.

Two things worth knowing that the measurement also settled:

- **tmux does not consult terminfo for this.** Stock macOS ships a 2015 `xterm-256color`
  with no `XM`/`xm` capability at all, and tmux 3.2 still asked for SGR. A terminfo-driven
  program would have fallen back to the wrapping encoding *inside* a modern terminal.
- **The old-terminal fallback is silent, not wrong.** A terminal too old to speak SGR sends
  the original form into tmux, tmux forwards it to the pane verbatim, and charter reads the
  payload bytes as stray keypresses — which the frame never delivers to a component. Never
  fires, rather than fires wrongly.

**No refusal was written.** A guard declining clicks past column 223 on tmux 3.2 would have
been declining clicks that measurably arrive correctly. What shipped instead is the test:
a real 244-column frame on both versions, clicking columns 100, 222, 223 and the last
column of the pane, each of which must come back as the column it was aimed at. Delete the
SGR request and all four fail.
