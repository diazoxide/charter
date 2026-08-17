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
    if status == "present":
        util.info(f"Left alone: {detail}.")
        util.info("  charter appends whole tables or nothing — merging would mean "
                  "rewriting TOML it did not author. Remove or rename yours to let "
                  "charter write its block, or add the hooks by hand.")
        return 0

    util.ok(f"Wired charter into {detail}.")
    util.warn("  This is MACHINE-WIDE. Codex has no project-level config, so these hooks "
              "run in every repo on this machine — charter's guards stay silent outside a "
              "control plane, but the processes do start.")
    util.info("  Codex trusts hooks by hash: the block is INERT until you approve it. "
              "Start Codex and accept the prompt, then `charter doctor` will confirm it.")
    return 0
