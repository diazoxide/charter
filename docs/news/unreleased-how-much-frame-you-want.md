---
version: unreleased
headline: Say how much frame you want, and the frame moves only while work does
---

The frame had one shape. You could change which edges it drew, one slot at a time, in
`charter.toml` — and then live with it until you edited the file again and relaunched.

**`[frame] density` is a preset over `slots`, not a second way to configure the same
thing.** Three levels: `minimal` (one-line top and bottom, each saying only its most
important thing), `normal` (the same two strips, saying everything they have), and `full`
— all four edges, which is the frame this release ships. A level expands to exactly the
slot list you could have written by hand, so nothing else in charter has to know presets
exist — the probe, `charter doctor` and the size floors all read the one resolved list.
If you *did* write a `slots` list, it wins: `slots` is the primitive, and a preset does
not get to outrank the thing it is a preset for.

`density = "full"` and writing nothing are the same frame, and that is checked rather
than trusted: the suite refuses a shipped `density` that does not expand to exactly the
shipped `slots`, in the same order and at the same verbosity. Two ways of asking for the
default that quietly disagreed would be worse than having only one.

**`F2` now changes the density of the frame you are sitting in.** The menu lists all three
levels with a dot on the one in effect; choosing one splits or closes panes live, re-asserts
every panel's size, and repaints at the new verbosity. It does **not** write to
`charter.toml`. That file is yours — hand-maintained, committed, full of your comments — and
charter's standing rule is that machine-written state belongs somewhere a machine may
rewrite whole. So the override lives in the frame's own state directory, and it lives
exactly as long as the frame does: relaunch and you are back to what your file says.

**The frame animates, and only while there is something to animate.** A dispatch still
running puts a spinner and a count on the bottom row — `⠙ 2 running`. The moment the last
one finishes, the frame is completely still again. Only the bottom row moves: the spinner
is the one thing on any panel that changes without something having changed, so the other
three keep repainting on news alone.

**It is dispatches only.** A long `charter clone` or `gl-refresh` will not spin it.
Charter records a dispatch when it *starts* — that is what makes two agents in one working
tree visible — and keeps no equivalent record for its own long-running commands. Reusing
the dispatch tracker for them would make the overlap nudge announce a clone as a peer
agent, so it needs a second kind of record; filed as #420.

That "completely" is the part worth spending a paragraph on, because a spinner is exactly
the kind of feature that quietly costs a machine something forever. Charter already records
a dispatch when it *starts* — that is what makes two agents editing one working tree
visible at all — so a panel does not poll anything to find out. It asks one question per
tick: a single `stat` of that tracker's own directory, whose timestamp moves only when a
record is created or removed. The records themselves are read only when that answer changes.
Measured on macOS/APFS: about **5µs per tick**, five times a second, next to the ~26µs a
panel already spent checking whether the frame's version had moved — roughly 0.003% of one
core, and zero repaints. Reading the records on every tick instead, the obvious way to write
this, measured 3.8x that and would have paid it whether or not anything was running.

A dispatch charter has stopped believing in — no result after thirty minutes, so the process
was most likely killed — is still reported and deliberately not animated: `⋯ 1 stalled`.
Such a record is kept for a day so a stuck dispatch stays visible, and a spinner turning
beside it would be claiming progress that stopped half an hour ago.

**And one warning finally moved somewhere you can read it.** tmux's `window-resized` hook —
what puts the panels back to their proper size after you resize the terminal — needs tmux
3.3, while charter's floor is 3.2. So a tmux 3.2 passed the floor cleanly, showed a green
tick in `charter doctor` and in `charter frame-probe`, and silently had no resize recovery
at all. Charter did know, and said so — into the terminal 86 bytes before tmux switched to
the alternate screen, which is nowhere. Both surfaces now name it, alongside the two
ceilings that moved there in 0.51.0, and nothing is printed on the launch path any more.

Nothing to adopt: upgrading is the whole of it. `charter frame-probe` will tell you if your
tmux is one of the ones that was quietly missing resize recovery.
