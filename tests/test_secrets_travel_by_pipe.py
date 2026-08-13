"""A secret reaches a CLI through a pipe — never a file on disk (issue #78).

`util.run(input=...)` is how every credential gets to `op`, `gh` and `vault` without
appearing in argv, where `ps` and shell history can read it. The half that was never
pinned is what `input=` becomes on the way: `subprocess.run` opens a pipe and writes the
bytes into it, so the secret exists only in kernel buffers between two processes.

The plausible edit this guards against is an optimisation, not a mistake. Templates get
large — #78 reports one at 239 KB across 43 fields — and writing a large payload to a
temp file and redirecting it in is the obvious thing to reach for. It would put every
secret charter handles on disk, and it would break `op` besides: `op item create -`
distinguishes a piped stdin from a redirected file and fails on the latter with an error
naming `--category`, which points nowhere near the real cause.

So the guarantee has two independent reasons to hold, and until now no test at all.
"""
from __future__ import annotations

import os
import sys
import unittest

from charter import util

#: Reports what kind of file description the child was handed as stdin.
PROBE = ("import os, stat; m = os.fstat(0).st_mode; "
         "print('FIFO' if stat.S_ISFIFO(m) else "
         "'REGULAR-FILE' if stat.S_ISREG(m) else 'OTHER')")


class TestInputBecomesAPipe(unittest.TestCase):
    def child_stdin_kind(self) -> str:
        return util.run([sys.executable, "-c", PROBE],
                        input='{"category":"SECURE_NOTE"}', check=False).stdout.strip()

    def test_a_template_reaches_the_child_as_a_pipe(self):
        self.assertEqual(self.child_stdin_kind(), "FIFO")

    def test_it_is_still_a_pipe_when_charter_s_own_stdin_is_a_file(self):
        """The case from the report: charter invoked as `charter secret set ... < f`.
        The child's stdin must come from charter's `input=`, not be inherited."""
        path = os.path.join(os.path.dirname(os.path.abspath(__file__)), "_isolation.py")
        saved = os.dup(0)
        fd = os.open(path, os.O_RDONLY)
        try:
            os.dup2(fd, 0)
            self.assertEqual(self.child_stdin_kind(), "FIFO")
        finally:
            os.dup2(saved, 0)
            os.close(saved)
            os.close(fd)

    def test_the_child_actually_receives_the_bytes(self):
        """A pipe nothing was written to would satisfy the check above and deliver no
        template at all."""
        proc = util.run([sys.executable, "-c", "import sys; print(sys.stdin.read())"],
                        input="s3cret-alpha", check=False)
        self.assertEqual(proc.stdout.strip(), "s3cret-alpha")


if __name__ == "__main__":
    unittest.main()
