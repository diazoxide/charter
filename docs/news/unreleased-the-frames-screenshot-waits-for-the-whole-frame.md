---
version: unreleased
headline: The frame's screenshot tests stop photographing a half-painted frame
---

`ChromeIsOneColour` is the only place in this suite that looks at what an operator looks
at, and on a loaded machine it was photographing frames that had not finished arriving.
The messages it produced never said so. They said this:

```
AssertionError: Lists differ: [(1, 9, 100, ['default', 'default']), …34 more…] != []
```

— a rule at row 1, running from column 9 to column 100, with an unpainted pane on each
side of it. Every panel in that screenshot had been handed `window-style bg=brightblack`
before the client ever attached, so a settled screen cannot have a `default` pane next to
a rule. It reads exactly like a #514 regression, and it sends the next person to audit
`pane_border_options`, which is correct. That is what made this the more dangerous of the
two flakes found in this class (#713 was the other), and why it was split out rather than
bundled.

**The gate measured the wrong fact.** The frame is drawn by a client on the inner server
writing into a pty that the OUTER server reads, and `_screenshot` stopped at the first
capture that contained **any** rule:

```python
if _rule_states(shot):
    break
```

tmux emits a redraw borders first, so the first capture with a rule in it is routinely a
capture with the borders drawn and the pane surfaces not yet arrived. Idle, the whole paint
lands between two polls and nothing notices. Loaded, the outer server is descheduled
part-way through reading it, the gate is satisfied by the fragment, and the test asserts
about a screen that was never the frame. "Some rules exist" and "this frame has finished
arriving" are different questions, and only the second one was ever wanted.

**The fix is an ordering, not a wait** (#650's rule; no `sleep` was added and no deadline
was lengthened). Once the frame's paint is demonstrably under way, a short mark is TYPED
into the harness pane — whose program is `cat`, whose pty echoes it — and the shot is the
first capture that holds the mark. A pty is a stream: the mark's bytes are written after
the redraw's and cannot overtake them, so the mark on the outer pane's screen is proof the
outer server consumed the whole redraw first. It survives tmux's one escape from that
ordering as well: a client whose terminal cannot keep up has its pending output
**discarded**, and while it is in that state the mark is discarded with it — tmux then
invalidates and redraws the entire screen, so the mark still cannot arrive without the
frame it marks.

A rule is still waited for, and it is now the precondition rather than the answer: tmux
composes a whole-screen redraw in one pass into one buffer, so one border cell reaching the
outer server proves the rest of that frame is already written and ordered ahead of anything
typed next. Typed any earlier, the mark would be part of the pane content that same redraw
is still drawing, and would prove only that one pane had arrived.

**Three candidate gates were measured rather than argued, 600 shots each under load.**

| gate | wrong at this loop's 0.1s cadence | at 0.01s |
|---|---|---|
| any rule has appeared (what was there) | 29/600 | 30/600 |
| every row of the capture is full width | 10/600 | 6/600 |
| two captures in a row are identical | 0/600 | **5/600** |
| the typed mark has come back | **0/600** | **0/600** |

"Every pane is non-empty" is not on that list because it cannot be asked here: every pane
in these tests runs `cat` and writes nothing, so a painted pane is a rectangle of spaces.
Its nearest measurable cousin — every row of the capture arrived at full width — is cheap
and wrong: it catches a paint truncated at a row boundary and misses one that stopped after
the borders. And "it stopped changing" is a duration wearing a fact's clothes: it is right
only while the gap between two polls is longer than the longest pause inside a paint, which
is a fact about the machine's load and not about the frame — halve the cadence and it
starts being wrong.

**Measured end to end, both worktrees run at once under one load**, three legs each,
135 runs per worktree per binary, 14-core darwin, `python3.14`.

| binary | leg | runs | failed |
|---|---|---|---|
| tmux 3.7c | `main` | 135 | **6** |
| tmux 3.7c | this branch | 135 | **0** |
| tmux 3.2 | `main` | 135 | **18** |
| tmux 3.2 | this branch | 135 | **0** |

Every one of `main`'s 24 failures is one of #719's own two:
`test_no_shipped_design_leaves_a_seam_between_two_panes_of_one_colour` (19) and
`test_an_unsurfaced_rule_really_does_leave_a_seam_between_painted_panes` (5) — the two
messages the issue was filed with, and neither of them about colour. The tmux 3.2 leg
needed #716's four deterministic floor failures patched out of the way first, or the
surfaced screenshots never run at all; that patch is measurement scaffolding and is not in
this branch.

Per screenshot rather than per run, which is the more sensitive instrument: `main` stopped
at a screen that was not the one the frame settled to on **5 of 716** shots on 3.7c and
**8 of 263** on 3.2; this branch on **0 of 960** and **0 of 264**.

Two claims the mark makes are now asserted rather than promised. It reaches the shot
through the nested client and its cells read as the harness pane's own background, so
nothing this class reads off a screenshot can see it. And the ordering itself is measured
on this tmux rather than argued from tmux's source: six rounds repaint a pane and type a
mark behind the repaint, and the screen carrying each mark carries exactly one background
and a new one — a screen caught between two paints carries both, which is the frame this
was photographing.

The module's deterministic tmux 3.2 failures are unchanged, four in this class and all of
them #716's.

Nothing to adopt — no production behaviour changed.
