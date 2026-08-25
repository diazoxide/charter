"""#324 — `util.run` redirects stdout and stderr, and leaves stdin INHERITED.

`test_clone_parallel` already wrote the diagnosis down, one defect early: "`util.run`
captures stdout and stderr but leaves stdin INHERITED, so when git falls back to
prompting for credentials the prompt is invisible and the call waits forever." That was
answered for git alone, with `GIT_TERMINAL_PROMPT=0`. Every other CLI charter runs kept
the descriptor — and `gh api` will read stdin for a field value naming standard input
(#323), which is how `glstate.state_for_repo` was observed still running after 10s.

**Never reproduced in this process.** A test that blocks on its own stdin does not fail,
it hangs, and a suite with nobody at the keyboard hangs with it. Everything below runs in
a child under an explicit timeout, and the hostile stdin is a pipe this test constructs
rather than whatever the machine happened to hand the runner — the "your machine is not
the runner" rule in CONTRIBUTING, applied to a file descriptor.
"""

from __future__ import annotations

import json
import os
import subprocess
import sys
import unittest
from pathlib import Path

import charter
from tests._isolation import child_plane_env

#: So the helper below can `from charter import util` whatever the cwd is.
_REPO_ROOT = Path(charter.__file__).resolve().parent.parent

#: Reads stdin to EOF. Handed a descriptor that never reaches EOF, it blocks until
#: something kills it — which is the whole point.
_READER = "import sys; sys.stdout.write(sys.stdin.read())"

_INNER_BOUND = 5.0    # what the helper gives the child before calling it stuck
_OUTER_BOUND = 60.0   # the backstop on the helper itself; never reached in practice

#: Runs `_READER` through `util.run` and reports the outcome as JSON. Run as a child so
#: that if `util.run` does block, it blocks *here* and this process reaps it.
_HELPER = f"""
import json, sys
from charter import util
try:
    p = util.run([sys.executable, "-c", {_READER!r}], check=False,
                 timeout={_INNER_BOUND})
    print(json.dumps({{"outcome": "returned", "rc": p.returncode, "out": p.stdout}}))
except util.ProcTimeout as e:
    print(json.dumps({{"outcome": "blocked", "seconds": e.seconds}}))
"""


class _NeverEofStdin:
    """A pipe whose write end nobody ever writes to and this process holds open, so a
    reader of the read end waits forever. Closed on exit."""

    def __enter__(self):
        self.r, self.w = os.pipe()
        return self.r

    def __exit__(self, *exc):
        os.close(self.r)
        os.close(self.w)
        return False


class TheFixtureIsRealBeforeAnythingIsConcludedFromIt(unittest.TestCase):
    """Assert the precondition rather than assume it.

    If this pipe were at EOF — because a descriptor got closed, or because the runner's
    stdin was already `/dev/null` — the test below would pass with `util.run` having
    done nothing at all. That is the vacuous shape this audit produced five of.
    """

    def test_an_inheriting_child_really_does_block_on_it(self):
        with _NeverEofStdin() as stdin:
            with self.assertRaises(subprocess.TimeoutExpired):
                subprocess.run([sys.executable, "-c", _READER], stdin=stdin,
                               capture_output=True, timeout=3)


class AChildNeverInheritsCharterSStdin(unittest.TestCase):
    def _report(self) -> dict:
        # `_HELPER` opens with ``from charter import util``, and a charter import resolves
        # a plane from the importing process's own cwd — which here is the checkout, i.e.
        # the developer's live plane. The helper never writes to it, but nothing said it
        # could not, and that is the shape #527 was: a module-level charter import in a
        # child nobody handed a plane. `child_plane_env` hands it one.
        _, env = child_plane_env(self, PYTHONPATH=str(_REPO_ROOT))
        with _NeverEofStdin() as stdin:
            proc = subprocess.run(
                [sys.executable, "-c", _HELPER], stdin=stdin,
                capture_output=True, text=True, timeout=_OUTER_BOUND, env=env,
            )
        # Precondition: the helper ran and reported. An ImportError or a traceback here
        # must be loud, not silently read as "did not block".
        self.assertEqual(proc.returncode, 0,
                         f"helper failed:\n{proc.stdout}\n{proc.stderr}")
        return json.loads(proc.stdout)

    def test_a_cli_waiting_on_stdin_gets_eof_instead_of_charter_s_descriptor(self):
        got = self._report()
        self.assertEqual(got["outcome"], "returned",
                         f"util.run blocked on inherited stdin: {got}")
        self.assertEqual(got["rc"], 0)
        self.assertEqual(got["out"], "", "the child read something — stdin was inherited")


class WritingToTheChildStillWorks(unittest.TestCase):
    """The half that must NOT change. `input=` is how every credential reaches `op`,
    `gh` and `vault` without appearing in argv, and `subprocess.run` opens the pipe for
    it itself — handing it `stdin=` as well is a `ValueError`, so the guard has to be
    conditional. `test_secrets_travel_by_pipe` owns this contract; asserted here too so
    a change to the stdin line fails next to the line that caused it.
    """

    def test_input_still_reaches_the_child(self):
        proc = util_run_reader(input="s3cret-alpha")
        self.assertEqual(proc.stdout, "s3cret-alpha")

    def test_passing_input_does_not_raise_value_error(self):
        self.assertEqual(util_run_reader(input="").returncode, 0)


def util_run_reader(**kw):
    from charter import util
    return util.run([sys.executable, "-c", _READER], check=False, **kw)


if __name__ == "__main__":
    unittest.main()
