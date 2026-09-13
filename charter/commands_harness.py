"""`charter harness` — which agent runtimes charter knows, and arming the opt-in one.

Most harness wiring is `init`'s job and happens inside the plane. Codex is the exception,
and the reason it needs a command of its own is scope rather than complexity: it keeps
hooks only in `~/.codex/config.toml`, so arming them reaches every repo on the machine.
Running this command IS the consent (ADR 0003's shape, where the second command is the
consent), and nothing here is ever done as a side effect of something else.
"""

from __future__ import annotations

import sys

from . import config, contain, tui, util
from .harness import registry


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
    # Imported here and not at the top: `cli` imports this module for every command, and
    # ruling 43 keeps `charter.profiles` off every command's import path.
    from . import profiles

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


def _resolve(name: str):
    """*name* as a profile, then as a registry NAME. ``(profile, why it was refused)``.

    Ruling 8, and the order is the whole ruling: a profile named `codex` is what
    `charter codex` runs, so it has to be what `charter harness install codex` wires. The
    registry name (`claude-code`) is looked up second so a plane whose operator typed that
    still reaches the built-in of that kind — nobody's muscle memory is broken by the
    lookup gaining a first half.
    """
    from . import profiles
    from .harness import registry

    read = profiles.current()
    found = read.profiles.get(name)
    if found is not None:
        return found, ""
    h = registry.get(name)
    if h is not None:
        # The BUILT-IN of that kind, and not merely the last profile of it this plane
        # declares: a registry name asks for the harness charter knows, and answering it
        # with somebody's `codex-pinned` would wire a folder they did not name. A built-in
        # is named after its kind, so this is `read.profiles[<kind>]` whenever a declared
        # profile has not replaced it — and a replacement IS what that name runs, so it is
        # the right answer when it has.
        for q in read.profiles.values():
            if q.harness == h.name and q.name == q.kind:
                return q, ""
    shown = contain.readable(name)
    return None, next((r.reason for r in read.refused if r.name == shown), "")


def cmd_harness_install(args) -> int:
    """Wire one profile's own config folder — the command every wiring refusal names.

    Five steps, and the order is what keeps charter from running something nobody approved:
    resolve the name, refuse a declared profile git would carry, ASK before a command the
    operator has not approved, wire it, then ask the harness whether that worked. The last
    step is why this exits non-zero on a Codex profile it has just written to: charter can
    write the `shell_environment_policy` line and nothing else, and a command that reports
    success over a profile that will still refuse to launch is the "remedy that ends the
    investigation" `opencode.unvouched` was written against. What is left for Codex is
    Codex's own commands, and the refusal it ends on names them.
    """
    from . import profiles, profiletrust, wiring
    from .frame import launcher

    name = (getattr(args, "name", "") or "").strip()
    p, refused = _resolve(name)
    if p is None:
        # Quoted by hand and not with `!r`: `contain.readable` has already escaped the name,
        # and `repr` would escape its backslashes a second time (`\\u001b`).
        shown = contain.readable(name)
        if refused:
            util.err(f"profile '{shown}' is refused — {refused}")
        else:
            known = ", ".join(profiles.current().profiles)
            util.err(f"no harness or profile named '{shown}' — have: {known}")
        return 2
    shown = contain.readable(p.name)
    if p.source != profiles.BUILTIN:
        why = profiles.ignored_refusal(config.ROOT)
        if why:
            util.err(f"profile '{shown}' is refused — {why}")
            return 1
    state = profiletrust.approval_needed(p)
    if state:
        # **Installing runs the profile's own command** (`claude plugin install`), so it is
        # asked about exactly as a launch is — Task 3's question and Task 3's sentences.
        # With nobody at a terminal to ask, it is refused the way an open nobody is at is.
        if not profiletrust.can_ask(sys.stdin, sys.stdout):
            util.err(f"charter: {profiletrust.UNATTENDED.format(name=shown, state=state)}")
            return 1
        r = launcher.answered(p, profiletrust.refusal(p, attended=True),
                              again=lambda: profiletrust.refusal(p, attended=True))
        if r is not None:
            # Branched on the KIND (ruling 27). A decline was answered on the terminal it
            # was asked on, and exits with the picker's cancel code; a yes charter could not
            # record (`KIND_RECORD`) or a record that moved under the question refuses.
            if launcher.already_said(r):
                return r.exit
            util.err(f"charter: {r.text}")
            return 1

    for status, detail in wiring.install(p, config.ROOT):
        # Already contained by `wiring.install` — a label is a path built out of the
        # profile's own `env`, which is a file a chat can write (ruling 35).
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
        if status == "refused":
            util.err(f"charter: {detail}")
            return 1
        if status in ("unvouched", "unavailable", "unknown", "failed"):
            util.warn(f"  {detail}")
        else:
            util.info(f"  {status}: {detail}")

    w = wiring.detect(p, cwd=config.ROOT)
    if w.state == wiring.WIRED:
        util.ok(f"profile '{shown}' is wired — {wiring.said(w.detail)}")
        return 0
    util.err(f"charter: {wiring.sentence(p, w)}")
    return 1
