---
version: unreleased
headline: The tmux integration tests stop waiting for the next look, so the suite's slowest module costs a third less on every run and every mutation
---

`tests/test_frame_tmux_integration.py` is the slowest module in this suite and the most
expensive thing the deletion sweep can select — #843 counted it in 651 of the selection
map's 16,545 entries, so a branch touching the launcher, the layout arithmetic, the frame
state or the plane paths pays it on every mutation, and pays it twice (`sweep.decide`
re-runs a subset that goes red, to confirm the red). Measured on `ubuntu-latest` it was
**21.5 s** of a frame selection's 25.7 s — 84% of it — and on a workstation **28.2 s**.

It is now **19.3 s** on the same workstation, with four more tests than it had.

## Where the time was, measured rather than assumed

Wrapping `subprocess.run` and `time.sleep` for one run of the module:

```
tmux subprocess.run total :   11.58s  (1987 calls)
time.sleep total          :   16.68s  (154 calls, mean 102 ms)
```

**Fifty-seven per cent of the module was `time.sleep`**, at a mean of 102 ms a call, and
the reason is one number repeated twenty times. Every wait here polls — which is right, and
`_DEADLINE`'s own note says why: *"every wait below returns the instant its condition
holds, so the number is what a LOADED runner may take and never what a healthy one does."*
Except that it cannot return the instant its condition holds. It returns at the next look,
and the next look was a flat `time.sleep(0.1)` in every one of them. That tick is a floor
under every wait in the module whether or not anything is actually slow — and on CI, where
57 of the module's 102 tests finish inside a single tick, the floor is most of what the
other 45 pay.

## Two changes, and neither of them shortens a wait

**A wait looks again sooner, and then backs off.** `_Poll` replaces the flat tick at all
twenty polling loops: the first look back is 2 ms, doubling to a 100 ms ceiling. A
condition that holds in 5 ms is now seen at 6 ms instead of at 100. A wait that runs long
costs what it always did — about 205 looks over a 20 s deadline against the 200 the flat
tick took — which is the reason for doubling rather than for a smaller flat number:
`_await_client` runs a `tmux list-clients` per look, and a flat fine tick would have spawned
some four thousand processes for a client that never registers.

Every deadline is the number it was, every condition is the condition it was, and a machine
where a condition never holds spends exactly the time it spent before and fails with the
same message. What changed is how often the loop looks, which is not a fact about the code
under test.

**A wait for absence is bounded by a fact instead of by a stopwatch.** The single most
expensive test in the module — `test_a_panel_dying_here_is_destroyed_unless_charter_arms_
the_window`, 5.0 s of the 21.5 s on CI — is the negative control for charter's `pane-died`
funnel: the pane dies on a server at tmux's own defaults, and nothing must reach a shell.
It bought its right to a short wait by *shortening the deadline to five seconds*, and
`_hook_reaches_a_shim`'s docstring already carried the better argument for it: **once tmux
has destroyed a pane, `pane-died` for it can no longer fire at any deadline.** That is a
fact tmux can be asked for, so it is now asked: the control hands the wait a `settled`
predicate — "has tmux taken the pane away?" — and the wait ends the moment it holds.

That is faster *and* stricter. Five seconds was wrong in both directions: it was paid in
full on every healthy run, and on a machine slow enough to fire the hook at 5.1 s the
control would have PASSED on its timer running out — the "test that cannot fail" shape this
module has shipped several ways. Bounded by the fact, it keeps the full `_DEADLINE` for a
machine that needs it and still answers in milliseconds. Settling ends the *wait*, never
the reading: the marker is read once more on the way out, so an argv that landed in the
same instant is still reported, and a pane tmux KEPT still fails the control by its own
assertion rather than satisfying it.

## What it tolerates: nothing

No `sleep` was added, no retry, no assertion widened, no deadline raised, no return code
stopped being checked, and no test's condition changed. The five remaining literal sleeps in
the module are all waits for something *not* to happen, which cannot be made cheaper by
looking more often because there is nothing to look at — and they are now enumerated by
value in a test, so a flat poll cannot creep back into one of the twenty loops unnoticed.

Four new cases, all four red on the version they fix:

* the first look back is `_POLL_FIRST` and the gap settles at `_POLL_MAX`, read off
  `time.sleep` rather than off a stopwatch;
* a wait told nothing can change asks once and stops, instead of spending 20 s;
* a wait that settles still reports what arrived in the same instant — the half that keeps
  the negative control able to fail at all;
* and the module contains no `time.sleep(<a number>)` outside the five that are listed.

## The measurement

Same 102 tests, all passing, three runs a side on one workstation (tmux 3.7c, darwin):

| | before | after |
|---|---|---|
| module wall | 29.6 / 27.5 / 27.3 s | 17.9 / 19.2 / 19.6 s |
| `time.sleep` | 16.68 s over 154 calls | 5.86 s over 191 calls |

Nothing to adopt — no production behaviour changed, and no production file was touched.
