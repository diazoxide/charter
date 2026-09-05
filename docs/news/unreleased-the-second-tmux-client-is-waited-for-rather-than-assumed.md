---
version: unreleased
headline: the open-or-focus test waits for the second tmux client to arrive instead of for the first one to still be there, and the module stops claiming CI has no tmux
---

`test_a_second_launch_focuses_instead_of_dragging.TwoPlanesOnOneMachine` went red once on
`test (3.13)` and green on the re-run — `AssertionError: 1 != 2` counting tmux clients
(#910). The property under test was never in doubt. The wait before the measurement was.

## A wait for something already true is not a wait

`_attach` starts a client with `pty.fork()` and then waits for it. The question it asked
was *is there any client at all*:

```python
_await(lambda: _tmux("list-clients", "-t", SHARED,
                     "-F", "#{client_name}").stdout.strip() != "")
```

That is the right question exactly once. On the first attach the list starts empty, so any
client is the new one. `test_a_second_client_attaching_by_session_id_does_not_move_the_first`
is the only test in the module that attaches **twice**, and on the second call the first
client is still attached — so the predicate is already true when `_await` is entered, and
the wait ends on its opening poll, before the forked child has exec'd `tmux`.

**Counted rather than argued.** Instrumenting the predicate: the first attach runs it
**twice** (empty, sleep 50ms, present). The second runs it **once**, every time.

## The margin was under five milliseconds

Between that opening poll and `len(self._clients())` there is exactly one thing: another
`tmux list-clients` subprocess, about 5ms. That is the entire safety margin, and it is
made of nothing but incidental subprocess overhead.

Measured by injecting the delay a loaded runner supplies for free — the forked child sleeps
before `os.execvp`, modelling a child that was descheduled:

```
child delay   old predicate            new predicate
    0 ms      0/10 saw 1 client        0/30
    5 ms     10/10                     0/30
   10 ms      9/10                     0/30
   20 ms     10/10                     0/30
   50 ms     10/10                     0/30
```

Five milliseconds of lateness in the child is enough to turn it from green to *always*
red. That is the whole of what stood between this assertion and the CI failure it produced.

**It would not reproduce on its own here, and that is worth stating plainly.** 200 runs of
the affected test on an idle 14-core machine: **0 failures**. 20 more under 48-way CPU load:
0, because load slows the observing subprocesses just as much as it slows the child, so it
widens nothing. A two-core shared runner losing 5ms in a fork is a likelier thing than this
machine reproducing it, which is why one CI red and no local one is exactly the shape this
defect should have. The mechanism is established by the poll counts and the injection, not
by a local red.

## What it waits for now

The set of clients is taken **before** the fork, and the wait is for one that is not in it.
The second attach now waits as long as the first, and the assertion measures a settled
server. No sleep was lengthened; a longer sleep would have made this rarer and no less real.

It is a set difference rather than `len(now) > len(before)` for two reasons. A count cannot
tell an arrival from a departure-and-arrival. And the count had quietly killed something
else:

**`_TERM_CANDIDATES` had stopped being a fallback.** `_attach` tries `xterm-256color`,
`screen`, `vt100` in turn, and advances only when `_await` returns **False**. Under *any
client* it could never return False after the first attach — so on every later attach a
TERM tmux refused was reported as a successful one, with a dead child's file descriptor
handed back. Waiting for a client that actually arrived restores the fallback, and costs
nothing on a machine where the first candidate works, which is every machine where the
first attach succeeded at all.

## The half of it CI can decide

A real-tmux test can only ever catch this *probabilistically* — both predicates come true a
few milliseconds later, so a red needs the runner to lose a race it usually wins. The
predicate itself is not a timing question. `TheAttachWaitAsksAboutTheClientItStarted` asks
it as four fixed client lists, with one right answer on every machine and no server to
attach to. The first case is the #910 state exactly: a client that was already there does
not count as the new one.

## And the module stops saying CI has no tmux

The docstring closed with the claim that this class is skipped *"where the machine has none
— which per §2.12 is every CI job charter has"*. It is not. `.github/workflows/test.yml`
installs no tmux, but `ubuntu-latest` ships one, so `TwoPlanesOnOneMachine` runs on every
`test (3.x)` job — which the #910 traceback proves, since it has a line number in it.

0.55.0's news had already measured and published this correction against the design's
§2.12. It did not reach this module, and a module written as if CI could only ever skip it
is a module whose timing bugs get found by CI instead of by its author. That is twice now.
