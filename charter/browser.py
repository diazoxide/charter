"""Materialising the browser driving surface into a plane, without charter carrying it.

A plane that drives a browser needs two different things, and only one of them is
charter's:

* **How to drive a page** — the command surface, snapshots, network mocking, tracing. That
  is Playwright's, it is long, and its own CLI already generates it as a skill.
* **Where credentials come from, and how parallel workers stay isolated.** That is
  charter's, and it is the `browser` skill this ships.

Charter deliberately does not vendor the first. `@playwright/cli` is Apache-2.0 and charter
is MIT: redistributing the generated pages would put a second licence, with its attribution
obligations, into every wheel and every plugin install — for content charter neither wrote
nor maintains. Worse, it would pin a pre-1.0 package that publishes frequently to *charter's*
release cadence, so a Playwright fix would wait on a charter release to reach anyone.

Running the vendor's own generator instead keeps both problems out of the repo: the plane
gets the pages from the tool that owns them, at the version it asks for, and a bump is one
command rather than a re-vendor and a release. This is the same shape as ADR 0014 — charter
writes the host's files and keeps no copy of its own.
"""
from __future__ import annotations

import shutil
from pathlib import Path

#: The version installed when none is asked for. A default, not a lock: `@playwright/cli`
#: is pre-1.0 and publishes often, so this is "known to work with the `browser` skill"
#: rather than a claim about what is current. `--version` exists because it will age.
PINNED = "0.1.18"

#: Where the generator puts its pages. Claude Code reads project skills from here, so it is
#: the vendor's choice as much as ours — named so the caller can be told what to expect.
SKILL_DIR = Path(".claude") / "skills" / "playwright-cli"


#: How long the generator may take before charter stops waiting on it.
#:
#: Generous, because a cold npm cache genuinely fetches a package. Bounded, because
#: `util.run`'s own docstring records why: "every un-timeouted path could hang
#: indefinitely: a 1Password session needing re-auth stalled the SessionStart preflight".
#: A network fetch is the last place to make an exception to that.
INSTALL_TIMEOUT = 300.0


def install_argv(version: str) -> list[str]:
    """The generator invocation. Split out so a test can assert the command without a
    network round trip — the failure worth catching is a malformed `npx` line, and running
    it for real would make the suite depend on npm being reachable.

    `--yes` is not optional. npm's own documentation: a prompt "can be suppressed by
    providing either --yes or --no. When standard input is not a TTY or a CI environment is
    detected, --yes is assumed." So an agent is fine and a HUMAN is not: run this in a
    terminal with a cold cache and npx asks a question, charter has captured the pipe it
    was printed to, and the operator sees an unexplained pause while something waits on an
    answer they were never shown.
    """
    return ["npx", "--yes", f"@playwright/cli@{version}", "install", "--skills"]


def npx_available() -> bool:
    return shutil.which("npx") is not None


def install(root: Path, version: str) -> tuple[int, str]:
    """Run the generator in `root`. Returns (exit status, combined output).

    The output is handed back rather than streamed so the caller can report what landed
    without the vendor's own progress noise becoming charter's.

    Through `util.run` rather than `subprocess.run`, for the timeout: a bare call had no
    bound at all, on the one operation here that reaches the network. A timeout is reported
    as a normal failure with its own explanation rather than a traceback, which is what the
    caller already knows how to print.
    """
    from . import util
    try:
        proc = util.run(install_argv(version), cwd=str(root), check=False,
                        timeout=INSTALL_TIMEOUT)
    except util.ProcTimeout:
        return 1, (f"the generator did not finish within {INSTALL_TIMEOUT:g}s and was "
                   f"stopped. npm may be unreachable, or a registry auth prompt may be "
                   f"waiting; try the command by hand:\n"
                   f"  {' '.join(install_argv(version))}")
    return proc.returncode, (proc.stdout or "") + (proc.stderr or "")
