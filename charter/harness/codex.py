"""Codex CLI as a harness — the one that turned out to be mostly already compatible.

Every fact here was pinned against **codex-cli 0.147.0**, never its documentation: the
hook payload from a draft-07 JSON schema embedded in the binary, the config shapes by
feeding candidates through ``codex mcp list -c '<toml>'`` and recording which the parser
rejects. Only rejections are evidence — Codex ignores unknown config keys everywhere, so
a shape that loads proves nothing by itself.

**Its hook contract is Claude Code's, near-verbatim.** Events: ``PreToolUse``,
``PermissionRequest``, ``PostToolUse``, ``PreCompact``, ``PostCompact``, ``SessionStart``,
``SessionEnd``, ``UserPromptSubmit``, ``SubagentStart``, ``SubagentStop``, ``Stop``. A
``PostToolUse`` payload requires ``{cwd, hook_event_name, model, permission_mode,
session_id, tool_input, tool_name, tool_response, tool_use_id, transcript_path,
turn_id}`` — Claude Code's field names plus Codex's own ``turn_id``/``agent_id``/
``agent_type`` — and the output wire is ``hookSpecificOutput{hookEventName,
additionalContext, permissionDecision, permissionDecisionReason, updatedInput,
updatedPermissions}``.

So ``charter/hooks.py`` very likely speaks Codex already. What is missing is not a port;
it is the decision about *where* to write the wiring, which is the problem below.
"""

from __future__ import annotations

from pathlib import Path

from .base import Deficit, Harness

NAME = "codex"

#: The keys a hook entry must carry. `{command=...}` alone is REJECTED (so `type` is
#: required) and `{type="bogus", command=...}` is REJECTED (so the value is validated) —
#: which is what makes `{type="command", command="…"}` parsing meaningful rather than an
#: artefact of Codex ignoring keys it does not recognise. `timeout` is accepted too.
HOOK_ENTRY_KEYS = ("type", "command")
HOOK_TYPE = "command"

#: Where a hook's wiring has to go. There is **no project-level config**: a
#: `.codex/config.toml` or `codex.toml` planted in a project directory is ignored, checked
#: by putting a deliberate type error in each and watching the config still load.
CONFIG_PATH = Path("~/.codex/config.toml")


class CodexHarness(Harness):
    name = NAME

    deficits = (
        Deficit("status-bar",
                "`tui.status_line` takes a list of built-in segments, not a command "
                "(a string or bool is rejected, an array of strings is not) — so charter "
                "cannot render into it, and `/charter` renders on demand instead."),
        Deficit("session-lock",
                "`shell_environment_policy.set` holds constants, so no per-session "
                "`$CHARTER_SESSION_ID` reaches a shell; the workspace lock falls back to "
                "the terminal-pane key. Hooks are unaffected — their payload carries "
                "`session_id` directly."),
        Deficit("wiring-scope",
                "no project-level config: hooks live only in `~/.codex/config.toml`, so "
                "charter's wiring here is machine-wide rather than per-plane, and cannot "
                "be committed and shared with the team the way `.claude/settings.json` is."),
    )

    def detect(self) -> bool:
        """Nothing native to detect, and saying so beats guessing.

        Codex hands a shell no identifying variable of its own; ``$CHARTER_HARNESS`` here
        would come from ``shell_environment_policy.set``, which is charter's own wiring.
        An unwired Codex session is therefore genuinely undetectable — and `doctor`'s
        WARN over an unregistered harness is a better answer than a guess from a path.
        """
        return False

    def wire(self, root: Path) -> list[tuple[str, str]]:
        """Deliberately writes nothing. This is a decision, not an omission.

        Claude Code's wiring lands in the plane's own `.claude/settings.json` and
        opencode's in `.opencode/plugin/`; both are per-project, committed or ignorable,
        and scoped to the plane charter was asked about. Codex has neither — writing its
        hooks means editing `~/.codex/config.toml`, which arms `charter hook pretooluse`
        in **every repo on the machine**, including ones with no control plane at all.

        That is the failure ADR 0014 already paid for once: the credential guard "needs
        `config.HAS_CONTROL_PLANE` to stay silent outside a plane — a gate added after it
        fired in unrelated repos and explained a control plane that did not exist there."
        Charter's hooks carry that gate, so the blast radius is survivable — but `init`
        silently reaching outside the plane to arm them is a different act from anything
        charter does today, and it is the operator's call rather than this file's.

        Codex also trusts hooks by hash (`enabled`/`trusted_hash`, and a
        `--dangerously-bypass-hook-trust` flag), so a written entry is inert until it is
        approved — a written-but-untrusted hook is precisely the "looks wired and is not"
        shape this repo keeps paying for (#177, #197).
        """
        return []
