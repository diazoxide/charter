#!/usr/bin/env python3
"""Run a command with its output attached to a pseudo-terminal, and print what it wrote.

`script -q /dev/null <cmd>` does the same job in one word, but it needs the *parent* to
already own a terminal — under CI, a hook, or an agent's shell it dies with
"tcgetattr/ioctl: Operation not supported on socket" and takes the capture with it. This
allocates the pty itself, so it works with no terminal anywhere in sight.

The pty matters because charter decides whether to emit colour from
``sys.stderr.isatty()``, evaluated at import time with no environment override. Capture it
down a plain pipe and every escape sequence is gone — the SVG comes out monochrome and the
whole point of the screenshot with it.

    COLUMNS=88 python3 ptyrun.py charter status > capture.ansi
"""

from __future__ import annotations

import fcntl
import os
import pty
import struct
import subprocess
import sys
import termios


def main() -> int:
    cmd = sys.argv[1:]
    if not cmd:
        print(__doc__, file=sys.stderr)
        return 2

    master, slave = pty.openpty()

    # Without an explicit size the pty reports 0x0 and anything that lays out to the
    # terminal width collapses to a single column.
    cols = int(os.environ.get("COLUMNS") or 88)
    rows = int(os.environ.get("LINES") or 50)
    fcntl.ioctl(slave, termios.TIOCSWINSZ, struct.pack("HHHH", rows, cols, 0, 0))

    proc = subprocess.Popen(
        cmd, stdout=slave, stderr=slave, stdin=subprocess.DEVNULL, close_fds=True,
    )
    # The parent must drop its copy of the slave, or the read below never sees EOF.
    os.close(slave)

    chunks = []
    while True:
        try:
            data = os.read(master, 65536)
        except OSError:      # EIO — the child exited and the pty tore down
            break
        if not data:
            break
        chunks.append(data)
    os.close(master)

    sys.stdout.buffer.write(b"".join(chunks))
    sys.stdout.buffer.flush()
    return proc.wait()


if __name__ == "__main__":
    raise SystemExit(main())
