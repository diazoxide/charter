"""The only module in charter that runs tmux.

Kept alone so everything else — layout, panels, slots — is testable on a machine with no
tmux installed, and so there is exactly one place where the argv rule can be broken.
"""

from __future__ import annotations

import re
import shutil
import subprocess

#: `display-menu` arrived in 3.0 and `display-popup` in 3.2, and the frame's interaction
#: model uses both. Floor at the higher one and degrade below it rather than refuse.
FLOOR = (3, 2)

_VERSION = re.compile(r"^tmux (\d+)\.(\d+)")


def _probe() -> str | None:
    """`tmux -V`'s output, or ``None`` when there is no tmux to ask."""
    if not shutil.which("tmux"):
        return None
    try:
        out = subprocess.run(["tmux", "-V"], capture_output=True, text=True, timeout=5)
    except (OSError, subprocess.SubprocessError):
        return None
    return out.stdout.strip() or None


def version() -> tuple[int, int] | None:
    """``(major, minor)``, or ``None`` when tmux is absent or unparseable.

    ``None`` is not "version zero": it says charter could not find out, which reads
    differently from "too old" and is answered with a different message.
    """
    raw = _probe()
    if not raw:
        return None
    m = _VERSION.match(raw)
    return (int(m.group(1)), int(m.group(2))) if m else None


def available() -> bool:
    return version() is not None


def meets_floor() -> bool:
    v = version()
    return bool(v and v >= FLOOR)


def absent_message() -> str:
    return ("charter's frame needs tmux, which is not on this machine.\n"
            "  install:  brew install tmux   (or your package manager)\n"
            "  without:  charter <harness> --no-frame  runs the harness bare")


def below_floor_message(v: tuple[int, int]) -> str:
    return (f"tmux {v[0]}.{v[1]} composes the frame, but its menu needs "
            f"tmux {FLOOR[0]}.{FLOOR[1]} — the frame starts with the hotkey disabled.")


def run(cmd: list[str]) -> int:
    """Run one tmux command. *cmd* is a LIST; this module never joins argv."""
    if not isinstance(cmd, list):
        raise TypeError(f"tmux argv must be a list, got {type(cmd).__name__}: {cmd!r} — see frame/layout.py")
    return subprocess.run(cmd).returncode
