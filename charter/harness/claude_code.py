"""Claude Code as one harness among several.

Nothing here is new behaviour — it is the runtime charter was built inside, written down
as a peer of the others so that "which harness am I?" stops being a question every
function answers by sniffing for Claude-Code-shaped variables.
"""

from __future__ import annotations

import json
import os
from pathlib import Path

from .base import Harness, LayerPart

NAME = "claude-code"

#: The keys charter mirrors from the plane's own `.claude/settings.json` into a workspace
#: directory, and the reason there are two rather than one.
#:
#: Claude Code reads project settings from the session's working directory and **does not
#: walk up**, so a chat whose cwd is `workspaces/<ws>/` reads no settings at all. Agents,
#: skills and CLAUDE.md *do* walk up and stop at a git boundary — which a workspace
#: directory is not, being a plain directory inside the plane's own repo — so those
#: already arrive and are deliberately NOT copied here (#850). A second copy of `skills/`
#: would shadow the plugin's own non-deterministically; Claude Code says so itself, by
#: name: *"is already taken by X, which takes precedence"*.
#:
#: `enabledPlugins` alone is not enough, which is the correction this list records: `env`
#: is where `$CHARTER_HARNESS` comes from on a harness with no per-shell hook, so a
#: workspace given only the plugin loads charter's skills and hooks and still cannot say
#: which harness it is.
#:
#: **`statusLine` was the third and is gone (#895).** Charter no longer writes a status
#: line into Claude Code's settings at all, so there is nothing of charter's under that key
#: to mirror. An operator who wires one by hand owns it the way they own `permissions`:
#: charter neither copies it sideways nor reports on it.
#:
#: Nothing else. `permissions` is the plane's decision about the plane's own root, and
#: copying a grant sideways into a directory nobody granted it in puts a permission in
#: force where no one clicked for it.
WORKSPACE_KEYS = ("enabledPlugins", "env")

#: Where the mirrored document goes, relative to the workspace directory.
WORKSPACE_SETTINGS = ".claude/settings.json"

#: The settings files Claude Code resolves **from the session's own directory**, in the
#: order it reads them. ``~/.claude/settings.json`` is deliberately not among them: it is
#: machine-global state charter never writes, and this list answers what the REPO carries.
_PROJECT_SETTINGS = (".claude/settings.json", ".claude/settings.local.json")

#: Charter's layer, one part per discovery rule — every rule measured on binary 2.1.259.
#:
#: **Two parts and not one, because two rules.** `settings` is cwd-only; `skills+agents`
#: walk up and stop at the git boundary. `CLAUDE.md` is the third rule and is deliberately
#: NOT a part here: it walks up and is **not** git-bounded, so it arrives almost
#: everywhere — which is exactly why the reported chat read as half-configured rather than
#: as absent, and `doctor` says so in the row instead of checking for a file that is always
#: found.
#:
#: **There was a third part, `status line`, and #895 removed it.** It existed because two
#: facts that go missing separately have to be reported separately — but the two stopped
#: being separate the moment charter stopped writing `statusLine`. The part answered
#: *"does this directory carry charter's footer"* against a key charter no longer puts
#: anywhere, so on every plane charter sets up it could only ever answer ✗, and a row that
#: is always red about a surface nobody asked for is the cry-wolf failure `check_harness`
#: records. What a hand-wired footer does is its author's business, the same way
#: `permissions` is.
LAYER = (
    LayerPart(
        "settings", _PROJECT_SETTINGS, False,
        "project settings are read from the session's OWN directory and the host does not "
        "walk up, so nothing above it is in force",
        keys=WORKSPACE_KEYS),
    LayerPart(
        "skills+agents", (".claude/skills", ".claude/agents"), True,
        "skills and agents DO walk up, but the walk stops at the git root — anything "
        "charter wrote above that boundary is out of reach, while CLAUDE.md walks up and "
        "is NOT git-bounded, which is why a session like this reads as half-configured "
        "rather than empty"),
)


#: The plane-root paths a checkout of its own cuts a Claude Code session off from.
#:
#: The same two directories :data:`LAYER`'s walking part is about, said for a different
#: purpose: that part answers whether a session HERE finds them, this one is the spelling
#: charter mirrors into `workspaces/<ws>/<repo>/`. Not derived from `LAYER` because the two
#: only coincide for this harness — opencode's in-repo surface is resolved by a rule
#: `LayerPart` cannot express, so deriving one from the other would have kept opencode's
#: paths out of a clone for as long as its discovery rule went unmeasured.
#:
#: **`.claude/settings.json` is deliberately not here**, and neither is `CLAUDE.md`.
#: Settings arrive by :meth:`ClaudeCodeHarness.workspace_files`, which every checkout gets
#: through `_harness_files`; listing it here as well would mirror the plane's file over the
#: generated one. `CLAUDE.md` is the project-instructions line `Harness.inherited_paths`
#: draws, and it is left behind on purpose — but **not** on the rule the two below follow,
#: which is what this comment used to say. :data:`LAYER` has the measurement: it walks up
#: and is NOT git-bounded, so the plane's own copy already reaches a checkout inside the
#: plane and there is no gap here for a mirror to close. What a mirror would add is the
#: plane's instructions inside somebody else's repository, read there as that repository's.
WALKUP_DIRS = (".claude/agents", ".claude/skills")


class ClaudeCodeHarness(Harness):
    name = NAME

    layer = LAYER

    inherited_paths = WALKUP_DIRS

    #: Measured, and the reason `doctor` names a condition rather than a verdict (#859).
    #: The gate is on the DIRECTORY and is global — it takes no argument saying which
    #: settings source declared the hook — and it is inherited up to the git root. So
    #: `workspaces/<ws>/` rides the plane's acceptance (it is inside the plane's own
    #: repository) while a clone at `workspaces/<ws>/<repo>` or a linked worktree has a
    #: git root of its own and needs its own.
    #:
    #: This read "hooks or the status line" until #895. The binary still gates both — that
    #: measurement is unchanged — but charter now writes only one of them, and naming a
    #: surface charter no longer wires would send an operator looking for a footer that is
    #: not there. **Hooks are the half that still matters and the half that is still
    #: named**: an untrusted directory is a plane-root guard that does not fire, which is
    #: the whole reason this condition is printed at all.
    trust_gate = "hooks"

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

    def provision(self, root: Path) -> list[tuple[str, str]]:
        """Install charter's own Claude Code plugin for the plane at *root* (#881).

        The CLI is the front door: it is what sits on ``PATH``, what every hook in
        ``hooks/hooks.json`` dispatches to, and what already writes `.claude/settings.json`
        — so it exists before a harness session does. The inverse shape, a plugin that
        bootstraps a CLI, has to guess at a Python environment it does not own.

        **Project scope**, from the plane's own directory, for the reason the README states
        as a feature: the plugin is what carries a plane's pinned version, so two planes on
        one laptop can sit on different charters without fighting. A ``user``-scope install
        would collapse that to one version per machine and would put charter's hooks into
        repositories nobody pointed charter at.

        A machine with no `claude` gets **no row at all** rather than a warning. An opencode
        or Codex plane is a supported install and has no Claude Code plugin to be missing;
        `check_harness` already states what each harness cannot carry, and a warning here
        would be the cry-wolf failure `doctor.py` keeps returning to.
        """
        from .. import plugincache

        status, detail = plugincache.install(root)
        if status == "installed":
            # Shaped for `commands._fold_entries`, which splits a label on `" ("`: the
            # head is the thing, the parenthesis is the note. `init` lists one line per
            # FILE and folds several notes onto it, and a label with no fold point makes
            # that line grow instead — the 254-column headline #231 capped.
            return [("created", f"the Claude Code plugin ({plugincache.PLUGIN_ID}, "
                                f"{plugincache.INSTALL_SCOPE} scope)")]
        if status == "present":
            # No parenthetical. Every caller here already says "already present" or
            # "Already there", so `(charter@charter is already installed …)` would be the
            # same word twice in one line.
            return [("present", "the Claude Code plugin")]
        if status == "unavailable":
            return []
        # `unknown` and `failed`. A SENTENCE, in the bucket `init` warns about rather than
        # lists: "the plugin is not installed" is exactly the state a reader must not take
        # away from a line under "already present" (#433, one level up).
        return [("unvouched",
                 f"The Claude Code plugin was not installed — {detail}. Without it charter's "
                 f"hooks do not run in this plane: no session context, no plane-root guard, "
                 f"no auto-save. Retry with `charter doctor --fix`, or install it by hand: "
                 f"`claude plugin marketplace add {plugincache.MARKETPLACE_SOURCE}` then "
                 f"`claude plugin install {plugincache.PLUGIN_ID} --scope "
                 f"{plugincache.INSTALL_SCOPE}`.")]

    def workspace_files(self) -> dict[str, str]:
        """The plane's own settings keys, as one document for a workspace to hold.

        **A 1:1 sync, and v1 has no overrides on purpose.** The whole of the requirement
        is that a chat standing in `workspaces/<ws>/` gets the layer a chat standing in
        the plane root gets; a `workspace.json` key that makes one workspace differ is a
        second feature, and building the divergence before anybody has asked for a
        specific one would ship a mechanism nothing exercises — ADR 0007's objection.

        **Read from the plane's committed file rather than composed from charter's own
        constants**, and that is what makes it a sync rather than a second generator. The
        argument survived #895 unchanged even though the constant it used to name did not:
        a plane whose operator edited one of these keys by hand would otherwise get
        charter's default mirrored into every workspace, silently reverting a deliberate
        choice in a new place. Reading the file cannot do that; composing from constants
        always can.

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
