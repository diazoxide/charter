"""charter's own frame: the harness runs inside it, charter draws around it.

tmux composes the rectangles and owns every part of terminal emulation — alt-screen,
resize, scrollback, the lot. Charter fills the edges with its own processes and never
parses or draws the harness's pane. ADR 0018.
"""

from __future__ import annotations

from . import layout
from . import tmuxctl

__all__ = ["layout", "tmuxctl"]
