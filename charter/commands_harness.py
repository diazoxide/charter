"""`charter harness` — which agent runtimes charter knows, and arming the opt-in one.

Most harness wiring is `init`'s job and happens inside the plane. Codex is the exception,
and the reason it needs a command of its own is scope rather than complexity: it keeps
hooks only in `~/.codex/config.toml`, so arming them reaches every repo on the machine.
Running this command IS the consent (ADR 0003's shape, where the second command is the
consent), and nothing here is ever done as a side effect of something else.
"""

from __future__ import annotations

import sys

from . import config, contain, profiles, tui, util
from .harness import codex, registry


def _list_profiles() -> None:
    """Every harness profile this machine has, the file it came from, and why any was refused.

    There is no `charter harness add` — a chat can run a command as easily as it can edit the
    file, so a command could never stand for the operator's approval — so this is how an
    operator who edited `charter.local.toml` reads back what charter made of it.

    Read through `profiles.current`, what every launch surface reads, so a name that clashes
    with a command is listed as refused here too. Built-ins first in registry order (a
    declared replacement keeps its kind's place), then declared profiles by name. Widths are
    measured from the cells (`tui.column`, `cmd_workspace_list`'s rule), and every cell that
    came out of the file is contained first: a command is text a chat can write, and a
    carriage return in it could otherwise redraw this line (ruling 35).

    The git check runs here because a person typed this command; it never runs on a config
    read. When git would carry the file, every profile it declares is listed refused with
    that state's reason (F1), and one line names the state's fix.
    """
    check = profiles.ignore_check(config.ROOT)
    profile_set = profiles.with_ignore_check(profiles.current(), check)
    order = list(profiles.builtins())
    rows = sorted(profile_set.profiles.values(),
                  key=lambda p: (p.name not in order,
                                 order.index(p.name) if p.name in order else 0, p.name))
    heads = ("NAME", "KIND", "COMMAND")
    body = [(contain.readable(p.name), p.kind, profiles.display(p)) for p in rows]
    widths = [tui.column(h, [row[i] for row in body]) for i, h in enumerate(heads)]

    def line(mark: str, cells, last: str) -> str:
        return (mark + "".join(tui.pad(c, w) for c, w in zip(cells, widths)) + last).rstrip()

    print(line("  ", heads, "FROM"), file=sys.stderr)
    for p, cells in zip(rows, body):
        print(line("* " if p.name == profile_set.default else "  ", cells, p.source),
              file=sys.stderr)
    if profile_set.refused:
        print("refused:", file=sys.stderr)
        for r in profile_set.refused:
            print(f"  {r.name or r.source}: {r.reason}", file=sys.stderr)
    if check.fix:
        util.warn(f"to use the profiles in {profiles.LOCAL_FILE}: {check.fix}")


def cmd_harness_list(args) -> int:
    """Every harness profile, then every registered harness, its ceilings, and which one this
    session is in."""
    _list_profiles()
    print(file=sys.stderr)
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
