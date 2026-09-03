---
version: unreleased
headline: The workspace-tab tests stop rebuilding the tmux server between themselves, so a red pull request stops meaning "the runner was busy"
---

`tests/test_a_workspace_tab_opens_what_it_names.py` failed on CI three times in two days,
in three different methods, always in `setUp` and always on the fixture's own
`new-session`:

```
AssertionError: 1 != 0 : server exited unexpectedly
```

Not a word of that is about charter. It is tmux's own sentence for a client whose server
went away mid-command, and the evidence that it was never the code is on the runs
themselves: the same shas that failed on `push` were green on `pull_request`, and
`gh run rerun --failed` turned all four red jobs green with nothing changed.

**It is #713 again, in a file that arrived after #713 was fixed.** That issue measured this
exact mechanism in `test_frame_tmux_integration.py`'s `ChromeIsOneColour` — 27 of 135
loaded runs, nine different tests, every one of them these three words — and settled the
construction that ends it. `test_a_workspace_tab_opens_what_it_names.py` shipped with #811
and never got it.

## The mechanism, and what is new here is that it is now deterministic

`kill-server` ends the server and leaves its socket **file** behind. So the next
`new-session` on that socket is a client that finds a path to `connect` to rather than a
path to build a server at, and which server it reaches depends on where the retiring one
has got to. Past its listening fd: `ECONNREFUSED`, the client unlinks, builds a fresh
server, and everything is fine — the only path an idle machine ever sees. Still holding it,
somewhere between "stopped serving" and `exit(0)`: the `connect` **succeeds**, the command
is handed to a server that will never run it, and the client is hung up on instead of
answered.

Nobody has made #839 fail on demand — it needs a runner loaded enough to be descheduled
inside that window, and on a 14-core darwin box the shape survived 1,040 deliberate
attempts to lose it (200 idle and 840 under load, some of them killing servers holding
seven panes and an attached pty client) without flinching once. But *what the client says
when it happens* is not a race, and
that is now pinned with no server, no signals, no load and no `sleep`: bind a socket where
tmux looks, `accept()` the client's connection — which is the proof it really connected,
rather than a wait — and close it without answering. rc 1, `server exited unexpectedly`,
five times out of five and twenty out of twenty.

**How many draws that is, counted rather than guessed.** A rebuilding client unlinks the
stale socket and binds a new one, so a fresh inode at the socket path is one server birth.
Instrumented on the version this fixed, one run of this module: **12 births** on one
socket — one per test across the class and its subclass, thirteen tests handing each other
a socket with a dying process behind it. It is now **2**, one per class, and both of those
meet no socket file at all, which is a build and cannot be a race.

## The fix is a construction, not a retry

Each class opens one detached `keep` session at `setUp` that nothing ever kills, so the
server is born once and is never a process on its way out. The per-test cleanup kills every
session **but** that one — so the fixed `alpha`/`beta` workspace names are free for the next
test and nothing of this test is left for it to read — and the class teardown is what finally
ends the server, unlinking its socket file behind it so the run leaks nothing.

**What it tolerates, stated rather than left to be found: nothing.** No `sleep` was added,
no retry, no widened assertion, no return code stopped being checked. Every `new-session`
here still asserts its own rc, so a tmux that genuinely refuses one still fails the test
with tmux's own message; a leaked session still fails the next test by name; and the click
deadline that catches a hang is untouched. What changed is that the fixture no longer
*creates* the condition it was reporting.

And the fix is asserted rather than trusted, in a test that is red on the version it fixed:
the cleanup every test ends with leaves the same server process standing, holding the keeper
and nothing else.

Nothing to adopt — no production behaviour changed, and no production file was touched.
