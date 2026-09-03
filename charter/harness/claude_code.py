"""Claude Code as one harness among several.

Nothing here is new behaviour — it is the runtime charter was built inside, written down
as a peer of the others so that "which harness am I?" stops being a question every
function answers by sniffing for Claude-Code-shaped variables.
"""

from __future__ import annotations

import json
import os
from pathlib import Path

from .base import Harness

NAME = "claude-code"

#: The keys charter mirrors from the plane's own `.claude/settings.json` into a workspace
#: directory, and the reason there are three rather than one.
#:
#: Claude Code reads project settings from the session's working directory and **does not
#: walk up**, so a chat whose cwd is `workspaces/<ws>/` reads no settings at all. Agents,
#: skills and CLAUDE.md *do* walk up and stop at a git boundary — which a workspace
#: directory is not, being a plain directory inside the plane's own repo — so those
#: already arrive and are deliberately NOT copied here (#850). A second copy of `skills/`
#: would shadow the plugin's own non-deterministically; Claude Code says so itself, by
#: name: *"is already taken by X, which takes precedence"*.
#:
#: `enabledPlugins` alone is not enough, which is the correction this list records.
#: `statusLine` has no plugin surface at all, and `env` is where `$CHARTER_HARNESS` comes
#: from on a harness with no per-shell hook. A workspace given only the plugin loads
#: charter's skills and hooks and still renders no status line and cannot say which
#: harness it is.
#:
#: Nothing else. `permissions` is the plane's decision about the plane's own root, and
#: copying a grant sideways into a directory nobody granted it in puts a permission in
#: force where no one clicked for it.
WORKSPACE_KEYS = ("enabledPlugins", "statusLine", "env")

#: Where the mirrored document goes, relative to the workspace directory.
WORKSPACE_SETTINGS = ".claude/settings.json"


class ClaudeCodeHarness(Harness):
    name = NAME

    #: Empty on purpose. Claude Code carries every surface charter has — it is the
    #: runtime charter grew inside, so it is the reference ceiling rather than a harness
    #: that happens to have no gaps recorded yet.
    deficits = ()

    cli_name = "claude"
    binary = "claude"

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
        return [(status, ".claude/settings.json (env)")]

    def workspace_files(self) -> dict[str, str]:
        """The plane's own three settings keys, as one document for a workspace to hold.

        **A 1:1 sync, and v1 has no overrides on purpose.** The whole of the requirement
        is that a chat standing in `workspaces/<ws>/` gets the layer a chat standing in
        the plane root gets; a `workspace.json` key that makes one workspace differ is a
        second feature, and building the divergence before anybody has asked for a
        specific one would ship a mechanism nothing exercises — ADR 0007's objection.

        **Read from the plane's committed file rather than composed from charter's own
        constants**, and that is what makes it a sync rather than a second generator.
        `commands._STATUSLINE` is one of the two things that would have to be spelled
        here, and a plane whose operator changed their status line by hand would then get
        charter's default mirrored into every workspace — silently reverting a deliberate
        choice, in a new place, which is the one thing `_ensure_statusline` exists not to
        do.

        Empty for a plane with no settings of its own, and empty for one whose settings
        are not parseable: `_load_settings` returns ``None`` for the second, and charter
        does not guess over a file somebody is holding. Empty here means the workspace
        gets no file and no marker at all, which is the honest rendering of "there is
        nothing to mirror" — writing an empty `{}` would look like a layer.
        """
        from .. import commands, config

        settings, _p = commands._load_settings(config.ROOT)
        if not settings:
            return {}
        doc = {k: settings[k] for k in WORKSPACE_KEYS if k in settings}
        if not doc:
            return {}
        return {WORKSPACE_SETTINGS: json.dumps(doc, indent=2) + "\n"}

    def upgrade(self, root: Path) -> tuple[str, str]:
        """Named, never run — the restraint `cmd_version_sync` already keeps.

        `claude` may be absent, may prompt for a scope, and the command mutates the
        reader's editor install. What changes here is only that this answer is now Claude
        Code's answer rather than the answer charter gave every harness.
        """
        from .. import update

        return "manual", update.PLUGIN_SYNC_CMD

    def ask_rule(self, pattern: str) -> str:
        from .. import commands

        return commands._as_rule(pattern)

    def apply_ask_rule(self, root: Path, pattern: str, local: bool = False,
                       dry_run: bool = False) -> tuple[str, str]:
        from .. import commands

        return commands.add_ask_rule(root, self.ask_rule(pattern), local=local,
                                     dry_run=dry_run)

    def apply_allow_rule(self, root: Path, pattern: str, local: bool = False,
                         dry_run: bool = False) -> tuple[str, str]:
        from .. import commands

        return commands.add_allow_rule(root, self.allow_rule(pattern), local=local,
                                       dry_run=dry_run)
