---
version: unreleased
headline: The frame's screenshot tests stop rebuilding the tmux server under themselves
---

`tests/test_frame_tmux_integration.py`'s `ChromeIsOneColour` failed 27 of 135 loaded runs on
`main`, across nine different tests, and never once about colour. Every message was the
same three words from tmux — **`server exited unexpectedly`** — and it is the shape #648
and #694 were both about: a red that names charter for something charter did not do, on a
class that sits on the path of every PR.

**It is not nine bugs. It is one construction, met nine times.**

These are the only tests in this suite that photograph the operator's screen, and that
takes two tmux servers: an inner one holding the frame, and an outer one whose single pane
is a client attached to it. Both were built on demand and both were allowed to go **empty**
— `_screenshot` ends by killing the session it photographed *and* the host session that
photographed it, and the pane-scope probe kills a session of its own. `exit-empty` is `on`
by default and tmux means it: the last session leaving retires the server.

But **tmux does not unlink its socket on the way out**. So the file stays, and the next
`new-session` is a client that finds a socket to `connect` to rather than a path to build a
server at. Which server it reaches depends on where the retiring one has got to: already
closed its listening fd and the client gets `ECONNREFUSED`, unlinks, and starts a fresh one
— the healthy path, and the only one an idle machine ever sees. Still holding it open,
somewhere between "no sessions left" and `exit(0)`, and the `connect` **succeeds**: the
client hands its command to a server that will never run it and is hung up on instead of
answered. `server exited unexpectedly`, rc 1.

That is why it flaked rather than failed. Nothing is wrong with the command or its
arguments; what varies is whether a process that has already decided to exit reaches
`exit(0)` before the next client's `connect` lands, and under load it does not.

**How many draws that is, counted rather than guessed.** A rebuilding client unlinks the
stale socket and binds a new one, so a fresh inode at the socket path is one server birth.
Instrumented, one run of this class on `main`: **61 births** — 41 inner, 20 outer, up to 13
inside a single test.

**The fix is a construction, not a wait** (#650's rule, and no `sleep` was added). Each
server is handed one detached session at `setUp` that nothing in the test kills, so it is
born once and is still the same process at the end. Nothing has to settle, because nothing
is being rebuilt. Births per class run went from 61 to one per server per test.

Measured with both worktrees run at once under one load — three campaigns of 45 runs each,
135 in all, 14-core darwin, tmux 3.7c, `python3.14`. `main` failed **27 of 135** (17, 7 and
3 as the machine's own load varied; most reporting `server exited unexpectedly` directly,
some through an unchecked return code one line later). This branch: **0 of 135**. Idle, all
8932 tests pass, and the module's 8 deterministic tmux 3.2 failures are byte-identical
before and after — three different questions, filed as #716, #717 and #718.

Those runs also turned up a second flake in the same class, which this does not touch and
which is not about servers: `_screenshot` stops at the first capture with any rule in it,
so a loaded machine can photograph a frame whose borders have arrived and whose panes have
not. Filed as #719 rather than bundled here — 5 of the same 135 runs, and on `main` it was
mostly masked by runs that died at the server race first.

Two of tmux's own facts the shape rests on are now measured rather than assumed, in a test
of their own: an emptied server really is retired, its socket file really does outlive it,
and the server that answers afterwards really is a different process — on tmux 3.7c and on
tmux 3.2 alike. And the fix itself is asserted rather than trusted: neither server is ever
rebuilt mid-test, which is red on the version this fixed.

One unchecked return code went with it. `test_the_unset_really_does_take_the_harnesss_
edges_back_off` did not check its own `new-session`, so a failed one let the test run on
with an empty pane id and report `set-option -w -t '' pane-border-style` — a message about
charter's chrome argv, for a session that was never created.

Nothing to adopt — no production behaviour changed.
