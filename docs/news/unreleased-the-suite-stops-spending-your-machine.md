---
version: unreleased
headline: charter's own test suite stops spending the machine it runs on — no PyPI request per render, no terminal read, and no tmux server left running after a killed run
---

The last three fixtures a test could inherit without noticing were the plane (0.52.0), the
shell (0.53.0) and the `[update] channel` in your own `charter.toml` (0.53.0). This closes
the three that are not fixtures at all but **costs**: what the suite spends on your
network, what it reads off your terminal, and what it leaves running on your machine after
it is gone. One defect wearing three hats — the suite reading or spending the machine
instead of the repository — and they are fixed together because the deletion sweep runs
the suite once per mutation, so whatever one run spends, a sweep spends again per mutation.

## Ninety children was the wrong number, and the right one was still 66

The suite forked **66 detached charter children per green run**: 27 `charter
_version-check`, each of them a GET to PyPI; 36 `charter gl-refresh`, each running the
forge client over every clone in the workspace; and 3 `charter frame-gather`. Every one of
them correctly pointed at a throwaway plane since #527, and not one of them was waited for
or asserted about. This is *how many* — a different question from #527's *which plane*,
whose own count (131 mis-planed children, earlier in this release) is not this one and must
not be read as it.

The count was taken in-process, by wrapping `subprocess.Popen.__init__` before the test
package is imported. That matters: the first attempt at this number sampled a running
suite with `ps | grep`, and the grep matched its own command line. The figure that
produced — around 90 — was an artefact, and it is the reason the measurement is now
something the repository can repeat rather than something somebody remembers doing.

They fire in a test and almost never on a real machine for one reason: both spawners are
throttled by state in `config.STATE_DIR`, and every test gets a fresh temp one — so the
cache is always absent and the cooldown lock never exists. The throttles that make this
rare in the field are exactly what a throwaway plane removes. Six test modules had stubbed
the spawner by hand because of it, with the same comment twice over. Twelve had not.

**The fix refuses the fork, not the spawner, and the difference is the whole design.**
Stubbing `maybe_spawn` in the shared base class would have been one line and would have
made a test that cannot fail: four modules call those functions directly to assert on the
argv and the cooldown, and two of their cases only assert that nothing raises — so a
base-class stub would have passed them without running anything, and would have done the
same for every case written afterwards. Refusing the `start_new_session=True` `Popen`
instead leaves every one of those cases running its throttle logic and asserting on it,
and refuses only the part nobody was asserting about. A case that never wanted a child
says `no_background_refresh(self)`; a case that is *about* the child — the four-edge frame
integration, which proves the detached `frame-gather` really starts, really gathers and
really writes the cache — says `allow_background_children(self)`.

Turning it on found 43 cases across six modules, and one of them was a module that had
stubbed the version check by hand and named the reason ("a suite that quietly reaches the
network is not hermetic") while forking a real `gl-refresh` straight past it.

**After: zero.**

## `$COLUMNS` was half of it. The other half was the ioctl

`charter/tui.py` reads `$COLUMNS`, which many shells export, and at `COLUMNS=40` with
everything else already scrubbed the suite returned four failures and an error. The
obvious fix is to add `COLUMNS` to the list of names the suite scrubs — and that fix is
wrong, in a way worth writing down, because the same argument produced the hole in the
first place.

With `$COLUMNS` gone, `term_width()` falls through to `os.get_terminal_size()`, an ioctl on
the process's own stdout. Measured on the fixed tree, with both variables unset, running
three of the modules the issue named against a real pty:

| terminal | result |
|---|---|
| 40 columns | `FAILED (failures=3, errors=1)` |
| 200 columns | `OK` |

Same tree, same commit, 172 tests either way. The only difference was how wide the window
was. Scrubbing the variable moved the reading; it did not end it. Nobody had hit it because
`os.get_terminal_size()` raises when stdout is a pipe — which it is under CI, under `| tee`
and under every agent-launched run — so the only person who ever sees it is the one running
the suite in their own narrow terminal, which is the person least likely to look.

Both halves are closed. The two variables are scrubbed, and they are **asked of
`charter.tui`**, the module that reads them, rather than spelled in the test harness: a
third geometry variable is covered on the commit that invents it. The frame's own
"variables that describe the launching terminal" list now asks the same constant instead of
carrying a second copy. And the ioctl answers what a pipe answers, in the same module that
already answers whether those file descriptors are a terminal at all — "is this a terminal"
and "how wide is it" are two questions about the same three streams, and one guard owns
both — so every caller takes the no-tty path it already documents and is already tested on.
A test that wants a size
states one — `mock.patch("os.get_terminal_size", …)`, which fifty-odd cases in this suite
already write.

The exit test is the measurement itself: two suite processes on two real ptys of different
widths, each reporting the width it was handed *and* the width charter answered with, and
the case fails unless the first two differ and the second two agree.

## Fourteen tmux servers, the oldest running for two and a half days

`ps` on the machine this was written on found 14 live tmux servers left over from test
runs, each holding a session and a `cat`, `sleep` or `charter panel` child. The socket
directory held 658 files, 497 of them from one test module, all dated inside the previous
48 hours.

Three things could have caused it, and they were told apart by experiment rather than by
argument. A clean run of that module left the socket directory one file *smaller* than it
found it — teardown works. The same run `kill -9`'d two seconds in left exactly one socket
file and one live tmux server behind.

**So it is not a test bug at all.** It is the signal that skips every `addCleanup` there
is, and the deletion sweep sends one every time a mutation makes the suite hang. 497 files
in two days is what "once per mutation" looks like from the socket directory's side, and no
`tearDown`, `atexit` or exit-time reaper can ever reach it — a run that had no exit has no
exit hook.

The only moment that can is the start of the *next* run, which is where the reaper now is.
It touches a socket only if the name is one this suite hands out (charter's own prefix and
the pid of the process that made it — the operator's own frame runs on `charter`, with no
suffix, and cannot match) and only if that pid is gone, so a concurrent run is never
disturbed. Whether a server is still listening is asked of the socket with one `AF_UNIX`
connect rather than by running `tmux` 497 times, and a live one is killed *before* its file
is unlinked, never after — `kill-server` returns 0 with the socket still bound for up to
1.3 ms, and unlink-then-kill points the kill at a path with no server on it and leaves the
real one running.

The four modules that name a socket now get the name from one helper, so the reaper cannot
fail to recognise one, and a fifth module that spells its own is refused by a test that
parses this directory looking for exactly that.

First run on the machine described above: **658 socket files → 127, and 14 leaked servers →
0.**

None of this reaches you unless you run charter's own test suite. Nothing to adopt.
