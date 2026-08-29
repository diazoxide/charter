---
version: unreleased
headline: The frame's input test stops reading its own client's arrival as a resize
---

`tests/test_frame_input_reaches_a_component.py` arrived with #636 and flaked from the day
it landed. Because it was a **new** module, every branch in flight had to prove the red
was not its own: three agents on three different branches each reproduced it on a clean
`origin/main`, each checked that it passed in isolation, each compared two runs at the
same sha — and each arrived independently at "not mine". A fourth pull request sat on a
`test (3.12)` failure that was this and nothing else.

That cost is the reason this was worth chasing to the bottom rather than re-running. A
flaky new module does not just waste time; it makes **a genuine regression in that file
indistinguishable from the known noise**, which is the same confusion #561 is about from
the other direction.

## It was not tmux, and it was not load

The failure always read the same way:

```
AssertionError: 'SIZE-0' not found in 'SIZE-1\n\n\n'
```

`acme.sized` is the fixture's third panel and it counts `resize` events — `SIZE-0` means
"nothing has resized this pane". Two cases assert it, as their way of saying *this event
reached the pane it was about and no other*.

The count was right. The fixture was wrong, and by exactly one:

```
new-session -x 100 -y 24     ->  window 100x24
pty.fork()                   ->  pty     80x24     <- the kernel's default, not a choice
tmux attach                  ->  window  80x24     <- tmux resizes to fit the client
                                 every pane: SIGWINCH
                                 acme.sized: SIZE-0 -> SIZE-1
```

The session is created at 100 columns. `pty.fork` — and `os.forkpty` under it — takes no
window size, so the pty the client attaches on comes up at the kernel's default, measured
here and on `ubuntu-latest` as 80x24. tmux then resizes the window to fit the client that
just arrived, a resize reaches every pane in it, and the panel that counts resizes counted
one. **The test was reading its own fixture's client attaching as an event.**

## Why it flaked instead of simply failing

`note_resize` compares the pane's rectangle to the last one the panel saw, so a panel only
counts a resize it was *running to observe*. The fixture attaches the client immediately
after splitting the four panels, so the outcome is a race between one tmux client
connecting and four Python interpreters importing charter:

* client connects first — the panel's first tick already measures the new size, nothing
  changed, `SIZE-0`, green;
* a panel finishes booting first — it measures 100, then 80, `SIZE-1`, red.

On an idle machine the interpreters lose that race reliably and the suite is green. Under
load the ordering stops being reliable, which is why it moved between cases run to run,
why it vanished in isolation, and why the same commit passed and failed in two CI runs.

Counted on a loaded box, 60 runs of the module on `main`: **2 runs failed, 3 assertions,
every one of them `SIZE-0`/`SIZE-1`**, split across the two cases that assert it — the
same two the reports named.

## One case was passing without testing anything

`test_resizing_the_window_reaches_a_component_that_asked_for_it` resizes the window and
waits for `SIZE-1`. When the attach had already made it `SIZE-1`, that wait was satisfied
**before `resize-window` ran** — the case passed, having asserted nothing about the resize
it exists to test. On a quiet box the panel then reached `SIZE-2` fast enough for the same
case to fail instead. One bug, both readings, depending on the scheduler.

It now asserts `SIZE-0` first. A case that reads a running total only means something if
it pins where the count started.

## The fix is a construction, not a wait

There is no new sleep. The pty is created at the size tmux says the window **already is**
— asked rather than re-spelled, which is `_rect`'s own rule (#514) one method over — so
the client arrives agreeing with the session and the attach is not a geometry change at
all. Nothing has to settle, because nothing moves.

Two checks stand behind it, in #609's shape rather than #598's, since the probe is what
decides:

* the window's size is compared across the attach, so a tmux that resized anyway fails in
  `setUp` naming the reason, instead of surfacing three cases later as a stray `SIZE-1`;
* the four first-paint rows are read a **second** time once all four have painted. The
  polling helper returns on the first match, so on its own it only ever says "this pane
  showed X at some instant" — never "X is what it shows". A panel that painted the row and
  then took an event satisfied it and was still wrong for every case below. That is
  precisely the shape #648 arrived as.

Measured with the race pinned to its losing side — panels booted and painted before the
client attaches, which is what a loaded box drifts toward — `main` fails every run and
never once comes up clean; the same construction on this branch is clean every run. The
fix stops the test depending on which process the scheduler picked, rather than making it
luckier.

Nothing to adopt: this is the test suite, and no production code changed. `frame/` behaves
exactly as it did — the claims the file makes about tmux were right, and it is the fixture
that was not establishing them.
