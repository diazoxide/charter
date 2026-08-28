"""The preflight's forge-auth probe, answered here instead of at github.com.

`doctor.check_forge_auth` runs ``gh auth status --hostname github.com`` — or ``glab auth
status --hostname gitlab.com``, which is what a plane declaring no ``[[forge]]`` block
falls back to, and every plane `PersonaIso` hands out is one of those. Eighteen test
modules reach that line through `doctor.run_all()`.

It is not a leak: ``auth status`` does not print the token. It is the operator's REAL
authority reaching a REAL remote, from a unit test, on every run (#638).

**Measured in-process**, by wrapping `subprocess.Popen.__init__` before the `tests`
package is imported — a ``ps | grep`` of a running suite matches its own command line,
which is how #542's issue arrived at a figure that turned out to be an artefact. One green
run at 7af3d7f: **28 forge-auth children from 18 modules**, 20 ``glab auth status
--hostname gitlab.com`` and 8 ``gh auth status --hostname github.com``, on CPython 3.12
and 3.14 alike.

The issue that filed this said "23, all ``gh``". Re-measuring is what turned that into 28
across two forges: a module whose plane declares no ``[[forge]]`` block gets the GitLab
default, so the split is a property of the fixtures rather than of the machine, and a fix
aimed only at ``gh`` would have left twenty of the twenty-eight in place.

**And the multiplier is why a fixture is worth writing.** `tools/sweep.py` runs the suite
once per mutation and the gate runs on every pull request, so 28 becomes 28 × the mutation
count — roughly **2,200 authenticated requests from one pull request's gate** on a diff the
size of #626's. #542 made exactly this argument about PyPI and it held.

**Answered at the child, not at the check**, and that is the difference between this and
the stub it replaces. `mock.patch("charter.doctor.check_forge_auth")` would have been one
line and would have made a function that never runs: its ``"Logged in" in blob`` branch,
its summary-line extraction, its ``stdout + stderr`` concatenation and its `ProcTimeout`
arm would all have gone dark for the whole suite, and the next case written against them
would pass without running anything — the trap #542 names. Wrapping `util.run` leaves
every one of those lines executing, on a recorded reply, and removes only the part nobody
was asserting about: the round trip to the forge.

**One wrapper, every caller.** `charter.forge.github` and `charter.forge.gitlab` reach
`util.run` by attribute lookup on the same module object, so `Forge.check_auth`'s identical
``auth status`` argv is answered here too. A test that wants a different answer patches
`charter.util.run` itself — `mock.patch` replaces this wrapper and puts it back after, so
the twenty-odd cases that already do that keep winning.

**The reply lands on stderr**, where `gh auth status` really writes it, so
`check_forge_auth`'s ``(proc.stdout or "") + (proc.stderr or "")`` stays load-bearing
rather than becoming a line no test can kill.

**Answered, not refused, and the refusal is next door.** `tests._planeguard.RealForgeReach`
fails any test that spawns a forge CLI with a subcommand at all — ``auth status``
included, now that nothing reaches it. This module is what keeps that refusal from
reddening twenty-three modules; the refusal is what keeps this module from being a stub
the next test walks around.

**The residual, written down rather than hidden.** ``gh --version`` still runs, 28 times a
run, from `doctor.check_forge_cli`: a local probe that contacts no host and reads no token,
which `RealForgeReach` therefore allows. `shutil.which(cli)` still reads this machine too,
and answering that would foreclose the tests about a CLI that is not installed.
"""

from __future__ import annotations

import os
import subprocess

from . import _planeguard

#: What this fixture answers ``<cli> auth status`` with. Recorded rather than invented:
#: the shape is `gh auth status`'s own — the host on its own line, the verdict indented
#: under it — and ``Logged in`` is the substring `doctor.check_forge_auth` and
#: `Forge.check_auth` both key off. It names itself, so a doctor row printed from a test
#: run says where the answer came from instead of looking like a real session.
REPLY = ("{host}\n"
         "  ✓ Logged in to {host} account charter-test-fixture "
         "(tests/_forgeprobe.py — no forge was contacted)\n")

#: Every ``auth status`` this fixture has answered, newest last, as the argv it was asked.
#: A test asserting the reach is closed can read a positive fact here rather than only the
#: absence of a spawn.
CALLS: list[list[str]] = []

_INSTALLED = False

#: What :func:`install` wrapped. Kept, and not only for the restore that never happens:
#: a case that puts it back for one call is how this suite watches the refusal next door
#: fire for real, and a fixture nobody has watched be necessary is a fixture nobody knows
#: is doing anything.
_ORIGINAL = None

#: Filled by :func:`install`. Asked of `_planeguard._forge_clis`, which asks
#: `charter.forge.registry.KINDS`, so the fixture and the tripwire beside it cannot
#: disagree about what counts as a forge CLI — and a forge added to that table is covered
#: on the commit that adds it.
_CLIS: frozenset[str] = frozenset()


def _hostname(argv: list[str]) -> str:
    """The host this probe names, for the reply to name back.

    Both backends pass ``--hostname`` (`GitHubForge` always did; `GitLabForge` was fixed to
    in FINDING I2, so a self-hosted plane stops asking gitlab.com about its own token), so
    the fallback is for an argv charter does not write today rather than for one it does.
    """
    for flag, value in zip(argv, argv[1:]):
        if flag == "--hostname":
            return value
    return "a forge"


def asks_a_forge_who_it_is(argv: list[str]) -> bool:
    """True when *argv* is the ``<forge cli> auth status`` probe and nothing else.

    Deliberately the narrowest possible match. ``auth login`` opens a browser and waits,
    ``auth token`` prints the credential onto stdout, and every ``api``/``pr``/``mr`` call
    reads or writes somebody's repository — answering any of those with a recorded reply
    would be this fixture pretending a test did something it did not. They are refused by
    `_planeguard.RealForgeReach` instead, which is a failure with a name on it.

    The program is compared on its BASENAME, because `shutil.which` has already resolved it
    to a full path by the time `doctor` builds the argv on some paths and has not on
    others.

    No length check in front of the slice: ``["gh"][1:3]`` is ``[]``, which is already not
    ``["auth", "status"]``. A guard that cannot be wrong is a line no test can kill.
    """
    return (bool(argv)
            and os.path.basename(argv[0]) in _CLIS
            and argv[1:3] == ["auth", "status"])


def install() -> None:
    """Answer the probe for the whole suite. Idempotent; called once at `tests` import.

    Idempotent because it is reachable twice — `tests` can be imported by a child process
    that also imports a test module — and a second wrap would put this fixture on top of
    whatever a running test had patched onto `util.run`, silently discarding it.
    """
    global _INSTALLED, _CLIS, _ORIGINAL
    if _INSTALLED:
        return
    _INSTALLED = True
    _CLIS = _planeguard._forge_clis()

    from charter import util

    original = _ORIGINAL = util.run

    def run(cmd, *args, **kw):
        argv = list(cmd)
        if asks_a_forge_who_it_is(argv):
            CALLS.append(argv)
            return subprocess.CompletedProcess(
                argv, 0, "", REPLY.format(host=_hostname(argv)))
        return original(cmd, *args, **kw)

    util.run = run
