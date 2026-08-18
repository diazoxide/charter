"""`charter harness` — which agent runtimes charter knows, and arming the opt-in one.

Most harness wiring is `init`'s job and happens inside the plane. Codex is the exception,
and the reason it needs a command of its own is scope rather than complexity: it keeps
hooks only in `~/.codex/config.toml`, so arming them reaches every repo on the machine.
Running this command IS the consent (ADR 0003's shape, where the second command is the
consent), and nothing here is ever done as a side effect of something else.
"""

from __future__ import annotations

from . import util
from .harness import codex, registry


def cmd_harness_list(args) -> int:
    """Every registered harness, its ceilings, and which one this session is in."""
    live = registry.current()
    for h in registry.all():
        mark = "*" if h.name == live else " "
        util.info(f"{mark} {h.name}")
        for d in h.deficits:
            util.info(f"      ↳ {d.key}: {d.detail}")
            if d.remedy:
                util.info(f"          → {d.remedy}")
    if live and registry.get(live) is None:
        util.warn(f"  running under '{live}', which charter has no record of — its "
                  f"surfaces are unverified.")
    elif not live:
        util.info("  (not running inside a harness)")
    return 0


def cmd_harness_install(args) -> int:
    """Arm a harness that `init` deliberately will not arm."""
    name = (getattr(args, "name", "") or "").strip()
    if registry.get(name) is None:
        util.err(f"unknown harness {name!r} — known: {', '.join(sorted(registry.KINDS))}")
        return 2
    if name != codex.NAME:
        util.info(f"'{name}' needs no opt-in — `charter init` (or `charter reinit`) writes "
                  f"its wiring into the plane, because it is scoped to the plane.")
        return 0

    status, detail = codex.install()
    if status == "malformed":
        util.err(f"{detail} is not valid TOML — left it completely untouched.")
        util.info("  Fix it by hand, then re-run. charter never repairs this file.")
        return 1
    if status == "doubled":
        util.err(f"{detail}")
        util.info("  Both sets are trusted and both run: charter fires twice on every "
                  "SessionStart, UserPromptSubmit and Bash call. Nothing is wrong; "
                  "everything is doubled, which is harder to notice.")
        return 1
    if status == "present":
        util.ok(f"Already named: {detail}.")
        return 0

    util.ok(f"Named the harness in {detail}.")
    util.info("  Codex's hooks come from the charter plugin — `codex plugin` installs the "
              "same artifact Claude Code uses. This only sets $CHARTER_HARNESS, which the "
              "plugin cannot do, so a Codex shell can say which harness it is.")
    return 0
