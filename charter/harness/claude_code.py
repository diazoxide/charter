"""Claude Code as one harness among several.

Nothing here is new behaviour — it is the runtime charter was built inside, written down
as a peer of the others so that "which harness am I?" stops being a question every
function answers by sniffing for Claude-Code-shaped variables.
"""

from __future__ import annotations

import os
from pathlib import Path

from .base import Harness

NAME = "claude-code"


class ClaudeCodeHarness(Harness):
    name = NAME

    #: Empty on purpose. Claude Code carries every surface charter has — it is the
    #: runtime charter grew inside, so it is the reference ceiling rather than a harness
    #: that happens to have no gaps recorded yet.
    deficits = ()

    def detect(self) -> bool:
        """``$CLAUDE_PLUGIN_ROOT`` is set for the plugin's own processes.

        The fallback is not tidiness: ``$CHARTER_HARNESS`` reaches Claude Code only once
        `init` has written it into settings, so without this every session already
        running — and every plane not yet reinitialised — would answer "no harness" the
        day the neutral variable ships.
        """
        return bool(os.environ.get("CLAUDE_PLUGIN_ROOT"))

    def wire(self, root: Path) -> list[tuple[str, str]]:
        """One static key in ``.claude/settings.json``'s ``env``.

        Claude Code has no per-shell hook the way opencode does, but ``env`` *"sets
        environment variables that apply to every session"* and this harness's name is a
        constant, so one key does the same job. Its session id needs no wiring: it keeps
        arriving as ``$CLAUDE_CODE_SESSION_ID``, which `session.current` still reads.

        `settings.json` is `commands.py`'s territory — it already owns the status line,
        the guard hook and the ask rules in that same file — so the plumbing stays there
        and this asks for it. Imported inside the method because `commands` imports the
        registry.
        """
        from .. import commands

        status, _path = commands.ensure_env_var(root, "CHARTER_HARNESS", self.name)
        return [(status, ".claude/settings.json (env CHARTER_HARNESS)")]
