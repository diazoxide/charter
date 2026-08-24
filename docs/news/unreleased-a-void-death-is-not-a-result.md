---
version: unreleased
headline: The frame's tmux integration tests stop reporting a death tmux never saw as a result
---

`tests/test_frame_tmux_integration.py` was the one module in charter's suite that could go
red on a correct tree. Two runs of the same commit at 0.52.0 — same bytes, one push-triggered
and one pull_request-triggered — disagreed on Python 3.13, and `gh run rerun --failed` went
green with nothing changed. Five different tests in that module were reported failing and then
passing on re-run the same day.

That matters more than an ordinary flake because `release.yml` runs this suite on all four
Pythons as the gate on an **irreversible** PyPI upload. A flake there fails a release that is
otherwise correct, and the recovery — re-run the failed job — is a judgement call made at the
one moment when "just re-run it" and "do not merge over a red check" are hardest to tell
apart. Here, `gh run rerun --failed` was the recovery that worked, on a tree with nothing
wrong with it — and a module that teaches an engineer under release pressure to reach for
that button is one that will eventually be re-run over a real failure.

**The failure was not what it looked like.** It reads:

```
AssertionError: Tuples differ: ('dead', 1) != ('dead', 33)
```

and the obvious reading is that the pane's program failed before reaching the `exit 33` the
test is about. It did not. `1` there is `commands_frame._UNKNOWN_DEATH_CODE`, which
`_pane_state` reports for a pane tmux calls dead and holds **no status for at all**. On a
loaded tmux 3.4 the pane's fd closes before tmux has reaped the child, and for some deaths it
never gets it: `#{pane_dead}` `1`, `#{pane_dead_status}` empty, the pane's `pane-died` array
never run — permanently.

Reproduced in an Ubuntu 24.04 container, tmux 3.4, pinned to one cpu against twelve spin
loops, running the real `respawn-pane`/`_pane_state` path 118 times:

| what tmux answered | trials | `pane-died` array ran | `_pane_state` |
| --- | --- | --- | --- |
| `1:33` | 111 | yes | `('dead', 33)` |
| `1:` | 7 | **no** | `('dead', 1)` |

Not one trial ever reported a literal status of `1`. Nothing exited 1, and there was no
readiness condition being assumed — so a longer deadline could not have fixed it, and polling
the status for a further 8 seconds never filled it in.

**A death like that is not a wrong answer, it is no answer.** The trial measured nothing about
charter, so it is now spent again on a fresh pane, up to three times, and the assertions stay
exactly as strict as they were. Nothing accepts an empty status; nothing accepts two outcomes.

Three details are the whole of whether that is honest rather than a snooze button.

- **The detector asks whether tmux ran the pane's hook array, never whether the status came
  back empty.** An empty `#{pane_dead_status}` is a *spelling* two different realities share:
  a harness genuinely killed by a signal has one too. Measured on both tmuxes this suite must
  pass on — a signal death answers `1:` and the array runs (40/40 on 3.4, 15/15 on 3.7c); a
  void answers `1:` and the array never runs. Classifying on the empty status would have made
  the signal-death test unfailable, and that test is the one whose whole subject is the
  sentinel written for an empty status.
- **The void is asserted, not assumed.** A missing probe marker is only allowed to mean "dead,
  with no status". A pane tmux *destroyed* — because nothing armed `remain-on-exit` — fails,
  naming that. A pane tmux has a status for but ran no hook for fails, naming that. Only the
  measured condition is retried.
- **Not one death, but three, for the capability probe too.** The module's process-wide "does
  a `pane-died` hook fire here?" probe cached its answer off a single death. One void there
  and every hook test in the module skipped for the rest of the run — and a skip reads as
  green, so the module would have been at its most silent exactly on the loaded runners it
  exists to be honest about.

The same discipline already covered two tests here since #409; this extends it to the five
that it did not reach, and moves it into the shared fixture so the next test written against a
dying pane inherits it. A machine that produces three such deaths in a row still skips, saying
which capability it could not measure.

**The second half of the module's flakiness was a different mistake with the same shape.**
Three tests that put a real `charter panel` subprocess in a real pane opened with a bare
`time.sleep(1)` — a guess at how long an interpreter takes to start, import charter and paint
one line — and then asserted on the capture. Under the same one-cpu load they failed every
run on an *empty* capture, and raising the module's whole deadline from 20 seconds to 90
changed nothing, because a fixed sleep does not spend a deadline. A sleep is not a shorter
wait; it is a different thing from a wait. They now poll the pane for its content, through the
helper the rest of the class already used, and pass. Two other fixed sleeps that stood in
for a readiness condition went the same way. Three waits that already polled, but stopped
at `os.path.exists`, now wait for the file's *content*: `echo … > path` creates the file
before it fills it, so existence alone could hand back the blank in between.

Negative waits — "show that this did **not** happen" — deliberately keep their own short
numbers. Raising one of those buys no reliability and spends the time on every single run.

**One case is left open and named:** `MenuClientIntegration` still sleeps 0.8 seconds after
writing a hotkey to a pty, standing in for "the menu is now open on that client", and under
the same load the following keystroke can land before it is. tmux exposes no format that
answers whether a client has a menu open — twelve were probed against a real attached client
before and after `display-menu` and only the activity timestamp moved — so that one needs an
observable found rather than a helper reused. It is tracked as #494.

Nothing to adopt, and no behaviour of charter's own changed — this is the suite. It reaches
you only as a green check that now means something.
