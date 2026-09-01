# Charter may run the harness, but never draws it

ADR 0015 settled where charter sits relative to a harness it lives *inside*: "Charter
targets harnesses, not one host. It keeps only the policy the current harness cannot
express… where a harness's ceiling is lower, `charter doctor` names the deficit by name."
That ADR's whole shape assumes charter is a guest — a hook, a status line, a plugin —
running inside a process someone else started.

`charter <harness>` inverts the relationship it describes. Charter is no longer only the
guest; for this command it is the process that **starts** the harness, in a frame it
composes around it — the harness in the middle, charter's own panels on the edges. That is
a second question ADR 0015 never had to answer, and answering it wrong twice — first by
guessing wrong about what the boundary even is — is why it gets its own record rather than
a paragraph appended to that one.

## The candidate that looked obvious

The natural instinct, having already built `charter/tui.py` and a status line, is to keep
going: read the harness's own stdout, parse its escape sequences, draw the whole frame with
one renderer charter owns end to end. A spike built exactly that — a Textual widget backed
by `pyte`, a terminal-in-a-terminal, running `claude` and `opencode` inside it.

It worked. Both harnesses rendered correctly. That is precisely why this had to be settled
by measurement rather than argument: a design that fails outright is easy to reject, and
this one did not fail.

## What was measured

darwin, Python 3.14.4, `textual` 8.2.8, `pyte` 0.8.2, tmux 3.7c, a 150×42 frame, one
agent-shaped corpus (an ordinary Claude Code session's own output, replayed).

| | Textual + pyte | tmux, end to end |
| --- | --- | --- |
| throughput | **1.85 MB/s** | **25.2 MB/s** |
| parse alone | 2.4 MB/s (0.9 MB/s with scrollback enabled) | ~37 MB/s |

Rendering itself was never the bottleneck on either side — Textual's own paint measured
7.2 ms/frame, a 138 fps ceiling nowhere close to being reached. The gap is the parse:
`pyte` interpreting the harness's escape sequences in Python, against tmux's own C parser
doing the identical job roughly fifteen times faster, and scrollback made `pyte` alone
almost three times slower again. Two implementations of the same well-specified problem —
one already correct and shipped in every environment charter runs on, one freshly written
in the language charter happens to be written in.

**Both arms rendered `claude` and `opencode` correctly.** This was a cost decision, not a
feasibility one — the kind of decision measurement resolves and argument alone does not,
because "which one is faster" and "which one is right" are different questions and only
the spike could answer the first.

## The half a benchmark cannot show

Speed is the half that is measurable in an afternoon. The half that is not: what the
Textual/pyte widget was, and was not, at the moment it was measured. It drew text. It did
not draw a cursor. Still owed, unwritten: mouse, scrollback beyond raw buffer access,
bracketed paste, OSC sequences, wide characters, and `?2026` (synchronized output) — each
one a real terminal behaviour a real coding-agent session produces, and each one a place
`pyte` could silently render something subtly wrong rather than fail loudly. `pyte` itself
was last released 2023-11-12 — a terminal parser is exactly the kind of code where a
one-cell drift in cursor math shows up as a garbled screen months later, on somebody else's
terminal, and the library that would need patching is not actively maintained.

This project ships `dependencies = []` — every dependency is a promise to keep re-checking,
by hand, forever. Owning a terminal emulator would mean owning the correctness of
`\x1b[?2026h`, of double-width CJK glyphs, of an OSC 8 hyperlink escape, indefinitely,
in a widget that at 120 lines had implemented perhaps a third of what a real one needs —
against tmux, which has already solved this problem, ships on every machine charter already
requires, and needs nothing from charter to keep solving it.

## The decision

**tmux composes the rectangles and does every part of terminal emulation. Charter draws
only its own panels — the edges — and never touches the harness's own pane: never reads
its output, never parses its escape sequences, never decides what a cursor or a colour
means inside it.** `charter/frame/tmuxctl.py` is the one module in the codebase allowed to
shell out to `tmux`, precisely so this boundary has exactly one place it could be crossed
by accident.

Read the other direction, the same rule is ADR 0015's boundary, moved: that ADR drew the
line at what a harness can express and let charter's own reach change harness by harness.
This one draws a line at what charter *runs*, and keeps that line fixed regardless of which
harness is on the other side of it — the frame is identical whether the pane inside it is
`claude`, `codex`, or a command charter has never met (`charter frame -- <cmd>`), because
charter never has to understand what is in that pane to draw around it.

## Consequences

* Charter's frame code needs no terminal-emulation dependency at all — `dependencies = []`
  survives this feature.
* The floor for `charter <harness>` is tmux's own version (3.2 for the frame's menu, 3.3
  for its resize-recovery hook — `charter/frame/tmuxctl.py`), not a Python library's
  release cadence.
* Charter's panels stay simple by construction: a top/bottom strip is one line, measuring
  its own pane and repainting whole on every change charter's own hooks report — there is
  no cursor, no scrollback, no input focus for charter's own code to get subtly wrong,
  because none of that is charter's to draw.
* The harness's own pane keeps every terminal behaviour it already has, including ones
  charter has never heard of, because tmux is already handling it and charter never gets
  between the harness and the terminal it is actually talking to.
* A future feature that genuinely needs to read the harness's own output (say, to react to
  what it printed) needs its own measurement and its own ADR — this one settles rendering,
  not observation, and conflating the two is how a boundary like this erodes one convenient
  exception at a time.

## Amendment, 2026-09-01: charter reads that pane at two moments, and draws in it at none

The bullet above asked for a measurement and its own record before charter read the
harness's pane. **The measurement showed charter was already reading it**, and had been
since #384 — so what follows is a correction to this ADR's own description of the code
rather than a new permission granted to it.

`commands_frame._pane_last_words` runs `tmux capture-pane -p -S -` on the harness pane on
**both** launch paths, and its docstring records the 3.7c measurement that put it there: a
registered harness whose binary is missing produced *zero bytes* of output and exit 127, and
that capture is the only thing that turns it into a sentence. §4f of
`docs/superpowers/specs/2026-08-30-charter-opens-like-an-ide.md` then asked for a second
read: tmux history dies with its session, so quitting a plane discards every visible
transcript, and *"less invasive"* cannot mean that.

So the rule is stated as it actually holds, rather than as a prohibition with two
undocumented exceptions:

**tmux composes the rectangles and does every part of terminal emulation. Charter draws only
its own panels — the edges — and never draws in the harness's own pane, never parses its
escape sequences, and never decides what a cursor or a colour means inside it. It READS that
pane at exactly two moments, both of which are moments the pane is about to stop existing:**

1. **a harness that died before the frame was drawn** (`_pane_last_words`) — the only chance
   to say anything at all, because nothing is ever drawn on that path;
2. **a chat being stopped by `charter: quit`** (`_capture_transcript`) — bounded to the last
   2,000 lines and 512 KB, written to that chat's own file under `.charter/frame/`, and
   **offered on the way back rather than replayed**. `F2 → chat: previous transcript` opens
   it in a pager in a window of its own; the reopened harness's pane starts clean.

**Both exceptions are bounded by the same property, and it is the property that keeps this a
boundary rather than a preference: charter reads only what is about to be destroyed, and
writes nothing back.** The two failures this ADR was written against — owning a terminal
parser, and drawing where tmux draws — are untouched by either. Nothing here parses an
escape sequence: `-e` keeps them and `-N` keeps the trailing spaces `-e` alone trims, and the
bytes go to a file and to `less -R`, both of which understand them better than charter would.

**What is still refused, sharpened rather than repeated.** Reading that pane to *react* to
what it printed — a hook on its output, a parse of its state, a decision made from its
content — is still a different feature and still needs its own measurement and its own
record. The distinguishing question is now written down so the next reader does not have to
infer it: *does charter read this pane at a moment it is ending, and does it write nothing
back?* Two yeses is this amendment. Anything else is a new one.

**Measured cost, because "capture it" is not free.** One 200-column pane at charter's shipped
`history_limit = 50000` took the shared tmux server from 3.7 MB to **130 MB**, and
`capture-pane -p -S -` pipes that whole history through charter's own process. That is why
the capture asks tmux for the last N lines (`-S -2000`) rather than for everything and
trimming afterwards: the bound belongs where the memory is. Verified on tmux 3.7c and at the
3.2 floor.
