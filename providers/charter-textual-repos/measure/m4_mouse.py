"""M4 — the mouse measurement, against a real tmux client on a real pty.

§4i measured that **tmux enables mouse reporting on the outer terminal from the ACTIVE
pane's mode alone**, and it measured that with charter's own panels, none of which ask for
the mouse. It could not measure the other half — what happens when a pane genuinely wants
it — because charter had no such program. `textual.live` is one: `App.run(mouse=True)`
writes ``?1000h ?1003h ?1015h ?1006h`` to its pane the moment it starts.

So this asks, of tmux 3.7c, with tmux's own ``mouse`` **off** (charter's default):

1. Does the client get mouse reporting turned on when the Textual pane is active, and
   turned off again when it is not?
2. Does a mouse report injected into the client reach the Textual app — and does it reach
   it only while that pane is active?
3. Does the harness pane keep its scrollback throughout, i.e. is the operator's own
   text-selection preserved while the Textual pane is *not* the one they are in?

The client runs on a pty this script owns, so every byte tmux writes to a terminal is
readable here. Nothing is inferred from tmux's own reporting of its state.

Cleanup: the caller (`m4_mouse.sh`) owns the socket and kills the server before unlinking
it — see `rig.sh`.
"""

from __future__ import annotations

import os
import pty
import re
import select
import struct
import subprocess
import sys
import termios
import time

SOCK = sys.argv[1]
LIVE = sys.argv[2]          # pane id of the textual.live pane
HARNESS = sys.argv[3]       # pane id of the harness pane
COLS, ROWS = 150, 40

#: The private modes that mean "report the mouse". 1000 = press/release, 1002 = drag,
#: 1003 = any motion, 1006 = SGR encoding. tmux writes them to the CLIENT when it decides
#: the terminal should report; that decision is what is being measured.
MODE = re.compile(rb"\x1b\[\?(1000|1002|1003|1005|1006|1015)([hl])")


def tmux(*args: str) -> str:
    return subprocess.run(["tmux", "-L", SOCK, *args], capture_output=True,
                          text=True).stdout.strip()


def drain(fd, seconds: float = 0.6) -> bytes:
    """Everything the client writes to its terminal in *seconds*."""
    out = b""
    end = time.monotonic() + seconds
    while time.monotonic() < end:
        r, _, _ = select.select([fd], [], [], 0.05)
        if r:
            try:
                out += os.read(fd, 65536)
            except OSError:
                break
    return out


def modes(blob: bytes) -> str:
    """The mouse modes in *blob*, in order, as ``1006h 1000l`` style text."""
    seen = [f"{m.group(1).decode()}{m.group(2).decode()}" for m in MODE.finditer(blob)]
    return " ".join(seen) if seen else "(none)"


def counters(pane: str) -> str:
    """The Textual app's own footer, which counts what it received."""
    text = tmux("capture-pane", "-p", "-t", pane)
    for line in reversed(text.split("\n")):
        if "clicks" in line:
            return line.strip()
    return "(no footer)"


def main() -> int:
    master, slave = pty.openpty()
    # The pty must be the session's size or tmux resizes the window under the experiment.
    termios.tcsetwinsize(slave, (ROWS, COLS)) if hasattr(termios, "tcsetwinsize") else None
    import fcntl
    fcntl.ioctl(slave, termios.TIOCSWINSZ, struct.pack("HHHH", ROWS, COLS, 0, 0))

    client = subprocess.Popen(
        ["tmux", "-L", SOCK, "attach", "-t", "exp"],
        stdin=slave, stdout=slave, stderr=slave, close_fds=True,
        env=dict(os.environ, TERM="xterm-256color"))
    os.close(slave)
    print(f"tmux {tmux('-V')}  ·  session mouse = {tmux('show', '-t', 'exp', '-v', 'mouse')}")
    drain(master, 1.5)

    print("\n== 1. what tmux writes to the terminal as the active pane changes ==")
    tmux("select-pane", "-t", HARNESS)
    drain(master, 0.5)
    tmux("select-pane", "-t", LIVE)
    print(f"  select-pane -> textual.live : {modes(drain(master))}")
    tmux("select-pane", "-t", HARNESS)
    print(f"  select-pane -> harness      : {modes(drain(master))}")
    tmux("select-pane", "-t", LIVE)
    print(f"  select-pane -> textual.live : {modes(drain(master))}")

    print("\n== 1b. and the keyboard, which charter has no path for either ==")
    tmux("select-pane", "-t", LIVE)
    time.sleep(0.3)
    for key in ("j", "j", "k"):
        tmux("send-keys", "-t", LIVE, key)
        time.sleep(0.2)
    print(f"  after j j k                 : {counters(LIVE)}")
    print("  ^ the app reads its own tty. `Component.events` declaring `key` has nothing")
    print("    to do with it: charter validates that tuple and never reads it again.")

    print("\n== 2. does an injected mouse report reach the app? ==")
    live_top = int(tmux("display", "-p", "-t", LIVE, "#{pane_top}"))
    row = live_top + 4                       # a row inside the Textual pane, 0-based
    col = 12

    def send(seq: bytes) -> None:
        os.write(master, seq)
        time.sleep(0.35)

    print(f"  before                      : {counters(LIVE)}")
    tmux("select-pane", "-t", LIVE)
    drain(master, 0.4)
    send(f"\x1b[<0;{col + 1};{row + 1}M".encode())      # press
    send(f"\x1b[<0;{col + 1};{row + 1}m".encode())      # release
    print(f"  click, textual.live active  : {counters(LIVE)}")
    send(f"\x1b[<64;{col + 1};{row + 1}M".encode())     # wheel up
    send(f"\x1b[<65;{col + 1};{row + 1}M".encode())     # wheel down
    print(f"  wheel, textual.live active  : {counters(LIVE)}")

    tmux("select-pane", "-t", HARNESS)
    drain(master, 0.5)
    send(f"\x1b[<0;{col + 1};{row + 1}M".encode())
    send(f"\x1b[<0;{col + 1};{row + 1}m".encode())
    send(f"\x1b[<64;{col + 1};{row + 1}M".encode())
    print(f"  same clicks, harness active : {counters(LIVE)}")
    print(f"  active pane after that click: "
          f"{tmux('display', '-p', '#{pane_id}')} (harness is {HARNESS})")
    print("  ^ tmux routes a report by POSITION, not by which pane is active. The modal")
    print("    part is the TERMINAL's reporting mode, which the active pane alone sets.")

    print("\n== 2b. and with the harness pane itself asking for the mouse ==")
    # What Claude Code actually does. The harness stand-in requests SGR reporting on its
    # own pane; tmux then enables reporting on the terminal while the HARNESS is active,
    # and every click anywhere in the window becomes bytes — including clicks that land on
    # charter's panels.
    tmux("send-keys", "-t", HARNESS, r"printf '\033[?1000h\033[?1006h'", "Enter")
    time.sleep(0.4)
    tmux("select-pane", "-t", HARNESS)
    print(f"  select-pane -> harness      : {modes(drain(master))}")

    print("\n== 3. the harness pane's scrollback, throughout ==")
    print(f"  history_size = {tmux('display', '-p', '-t', HARNESS, '#{history_size}')}"
          f"  ·  last line = "
          f"{tmux('capture-pane', '-p', '-S', '-5', '-t', HARNESS).splitlines()[-1]!r}")

    client.terminate()
    try:
        client.wait(timeout=3)
    except subprocess.TimeoutExpired:
        client.kill()
    os.close(master)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
