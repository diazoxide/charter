---
version: unreleased
headline: The tmux tests wait for the thing itself instead of sleeping, a dead pane tmux never had a status for stops being read as an exit code, and no test module can hide classes below its `if __name__` trailer any more
---

Three ways charter's own suite could be green about something it had not measured. None of
them reaches a plane you run; all three reach anyone who trusts a green run — which, on the
release workflow, is the one place a wrong call cannot be undone.

**A menu the test only assumed was open.** `MenuClientIntegration` pressed the hotkey on one
attached terminal, slept 0.8 seconds, and then typed into the other. Under cpu contention the
menu was not open yet, so the keystroke landed in the pane instead, and the test failed on an
assertion about charter — about half of runs on a one-cpu tmux 3.4. The negative half was
worse than flaky: "the other client's key must not select in this menu" is satisfied for free
by a menu that has not opened, so a late menu made that assertion pass for no reason at all.

There is no format that reports an open menu. All 168 that `display-message -a` names were
probed against tmux 3.7c with two real terminals attached to one session, before and after a
`display-menu`: only a byte counter moved, and it moved for the terminal with no menu too.
But a menu is *drawn*, and these tests own the master fd of the terminal it is drawn on. Each
attached client now has a reader thread accumulating everything tmux paints on it, and the
tests wait for the menu's own row to appear there — on the presser's terminal and, asserted
separately, on nobody else's. The 0.8 s guess is gone from every one of them; the wait
returns in about 50 ms on an unloaded machine and holds on a loaded one. The other client's
keystroke is waited on too, by watching the pane's own `cat` echo it — so "it did not select"
is now about a key that was really delivered and really spent.

Two tests whose only assertion was that a hostile `#(…)` label creates no canary gained a
precondition they never had: the label has to be observed on screen first. A menu that never
rendered used to satisfy them.

One test resisted the whole approach, and it is worth naming because the first fix for it was
wrong. `-` is not tmux's spelling for "no shortcut" — it is an ordinary key, which is why rows
past the ninth are keyed with the empty string — and the test that measures this presses `-` on
a rendered eleven-row menu and requires that nothing ran. Waiting for tmux to repaint proves
the `-` was *read*; it does not bound the `run-shell` → `charter frame-action` → `subprocess`
chain whose absence is the whole assertion, and a repaint arrives two orders of magnitude
sooner than that chain finishes. Swapping the sleep for the repaint made the test pass whether
or not `-` fired the row — the same defect as the sleep, wearing a fix's clothes. An assertion
about an absence needs a *bound*, not a readiness signal, so it now gets one from a pacing
control: the eleventh row's own action, dispatched to the same tmux server only after the
repaint proves the `-` was consumed. Identical work, started strictly later — so when the
pacer's canary lands, anything `-` could have started has had longer, and the absence is
measured. Restoring the constant `-` that this row exists to refuse fails the test again;
without the pacer it did not.

**A death tmux never had the status of, read as an exit code.** On a loaded tmux 3.4 about one
pane death in seventeen closes the pane's fd before tmux has the child's status, and never
gets it — `pane_dead=1`, an empty status, the `pane-died` hook array never run, permanently.
charter reports an unknown death as `1`. `EarlyDeathIntegration` compares that number against
`7` and says "a single argument must reach a shell", about a machine where the argument
reached a shell perfectly well. That is what CI showed: red on two Python versions in one run,
green on a re-run of identical bytes, and it has already cost a re-run on the publish path.

Those four tests now spend a fresh pane when a trial measured nothing, exactly as the rest of
the module has since #487 — with the void *asserted* rather than assumed, so a pane tmux
destroyed, or one it holds a status for but ran no hook for, is a loud failure naming which.
The probe has to be armed in the server's own config file here, because there is no gate to
hold this pane open: the whole subject is a command that dies before the frame is drawn.
Nothing is widened; `7` is still the only value that passes.

**The probe's evidence moved into tmux's own state.** It used to `touch` a file, so "tmux never
ran the array" and "the marker could not be written" were the same reading — sabotage the
probe and a real failure became a *skip*. It is now a tmux pane option, set by tmux in the same
act as running the array, with no shell and no filesystem in between. The mechanism is proved
usable — written and read back — before anything depends on it, so a tmux that cannot carry the
evidence says so by name instead of turning every death into a skip.

**And 26 test modules hid classes below their `if __name__` trailer.** `unittest.main()` raises
`SystemExit`, so everything written after it is dead in the one invocation the trailer exists
for: `python3 tests/test_doctor_shadowed.py` reported `Ran 8 tests / OK` where
`python3 -m unittest tests.test_doctor_shadowed` ran 14 — and the six it dropped were the ones
that exercise the real seam, the eight that survived the pure-function ones. Across the 26,
154 tests were invisible to a direct run. CI never saw it, because discovery imports a module
rather than executing it; all 154 do run and do pass there. The gap belonged to whoever was
debugging one file, which is when a green run is trusted hardest.

The 26 trailers moved to the ends of their files, and a guard now walks the suite and fails on
the 27th — naming the module, the line, what got hidden and both ways out. All 26 got there the
same way, by someone appending a class to a finished file and not noticing the trailer above
the insertion point, so the guard is the point and the sweep is the smaller half.

*(Later in this same release the menu those two classes drove was replaced by the palette,
and both were deleted with it — see* F2 opens a palette you type into*. The discipline they
established did not go with them: the palette's own integration tests wait for its rows to
appear on the pane rather than sleeping and hoping.)*

Nothing to adopt: none of this changes what charter does on your plane.
