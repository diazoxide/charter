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

import os
import tomllib
from pathlib import Path

from .base import Deficit, Harness

NAME = "codex"

#: The keys a hook entry must carry. `{command=...}` alone is REJECTED (so `type` is
#: required) and `{type="bogus", command=...}` is REJECTED (so the value is validated) —
#: which is what makes `{type="command", command="…"}` parsing meaningful rather than an
#: artefact of Codex ignoring keys it does not recognise. `timeout` is accepted too.
HOOK_ENTRY_KEYS = ("type", "command")
HOOK_TYPE = "command"

#: How a Codex plugin moves, pinned against codex-cli 0.147.0.
#:
#: `codex plugin update` does NOT exist — the binary answers "error: unrecognized
#: subcommand 'update'", which is the rejection this file treats as the only real
#: evidence. What exists is add / list / marketplace / remove, and the snapshot a plugin
#: installs FROM is refreshed at the marketplace level. So updating is two commands, and
#: it is the same shape Claude Code has (`marketplace update` then the plugin) rather than
#: a quirk worth explaining twice.
#:
#: Named, never run, for the reason every host command is: it mutates an install charter
#: does not own, and `codex` may not be on the reader's PATH at all.
PLUGIN_UPDATE_CMD = ("codex plugin marketplace upgrade charter && "
                     "codex plugin add charter@charter")


def config_path() -> Path:
    """Codex's config file. There is **no project-level one**: a `.codex/config.toml` or
    `codex.toml` planted in a project directory is ignored, checked by putting a
    deliberate type error in each and watching the config load anyway.

    ``$CODEX_HOME`` is honoured — verified by pointing it at a throwaway directory and
    watching `codex mcp list` read that directory's config. Writing to `~/.codex`
    unconditionally would silently miss anyone who sets it.
    """
    home = os.environ.get("CODEX_HOME")
    return (Path(home) if home else Path.home() / ".codex") / "config.toml"


def _block() -> str:
    """The TOML charter appends: the harness name, and nothing else.

    Codex installs charter's **plugin** — the same artifact Claude Code uses — and the
    plugin declares every hook. Declaring them here too ran charter twice on every
    SessionStart, UserPromptSubmit and Bash call, which an earlier version of this file
    did because its survey stopped at `config.toml` and never looked for a marketplace.

    What is left is the one thing the plugin cannot do: tell a Codex shell which harness
    it is, so `harness.current()` has something to read.
    """
    return "\n".join([
        "", "# --- charter (control plane) ---",
        "# Hooks come from the charter@charter plugin; this only names the harness.",
        "[shell_environment_policy]",
        'set = { CHARTER_HARNESS = "codex" }', ""])


def install() -> tuple[str, str]:
    """Arm charter's hooks in Codex's config. ``(status, detail)``.

    Not called by `init`, and that is the decision rather than an oversight: this file is
    machine-wide, so writing it arms `charter hook pretooluse` in every repo on the
    machine. Running this command IS the consent — the two-step shape ADR 0003 uses for
    `charter report`, where "the second command is the consent".

    Charter appends **whole tables or nothing**. A config that already declares `hooks` or
    `shell_environment_policy` is reported and left untouched: merging would mean
    rewriting TOML charter did not author, and `_load_settings` already refuses that for
    the file charter half-owns, let alone this one.

    A hook written here is still **inert until Codex trusts it** (`enabled`/
    `trusted_hash`, and a `--dangerously-bypass-hook-trust` flag exist), so the caller is
    told to approve it rather than left believing the wiring is live.
    """
    p = config_path()
    raw = ""
    if p.exists():
        try:
            raw = p.read_text()
            doc = tomllib.loads(raw)
        except (OSError, UnicodeDecodeError, tomllib.TOMLDecodeError):
            return "malformed", str(p)
        hooks = doc.get("hooks") or {}
        # `[hooks.state]` is Codex's own trust ledger, not a declaration — only actual
        # event tables mean somebody (probably an older charter) declared hooks here.
        declared = [k for k in hooks if k != "state"]
        if declared:
            return "doubled", (f"{p} declares hooks ({', '.join(sorted(declared))}) — the "
                               f"charter plugin declares them too, so charter runs twice "
                               f"per turn. Remove the charter block from that file.")
        if "shell_environment_policy" in doc:
            return "present", str(p)
    p.parent.mkdir(parents=True, exist_ok=True)
    sep = "" if (not raw or raw.endswith("\n")) else "\n"
    p.write_text(raw + sep + _block())
    return "created", str(p)


class CodexHarness(Harness):
    name = NAME

    deficits = (
        Deficit("status-bar",
                "`tui.status_line` takes a list of built-in segments, not a command "
                "(a string or bool is rejected, an array of strings is not) — so charter "
                "cannot render into it, and `/charter` renders on demand instead.",
                "charter statusline --watch"),
        Deficit("session-lock",
                "`shell_environment_policy.set` holds constants, so no per-session "
                "`$CHARTER_SESSION_ID` reaches a shell; the workspace lock falls back to "
                "the terminal-pane key. Hooks are unaffected — their payload carries "
                "`session_id` directly."),
    )

    cli_name = "codex"
    binary = "codex"

    def upgrade(self, root: Path) -> tuple[str, str]:
        """Codex's own config block never needs moving; its PLUGIN does.

        `_block()` writes only `shell_environment_policy` — a constant naming the harness,
        with no version in it — so an upgraded CLI is served by the same block. The hooks
        come from the charter plugin, installed by `codex plugin`, and the command that
        moves THAT is a fact nobody has pinned against a real Codex.

        Every fact in this file was pinned against codex-cli 0.147.0 rather than its
        documentation, and this one the same way: `codex plugin update` — the command
        everyone reaches for, including the first draft of this method — is rejected by the
        binary, and the two-step in :data:`PLUGIN_UPDATE_CMD` is what it actually offers.
        """
        return "manual", PLUGIN_UPDATE_CMD

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
