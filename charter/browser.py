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
import subprocess
from pathlib import Path

#: The version installed when none is asked for. A default, not a lock: `@playwright/cli`
#: is pre-1.0 and publishes often, so this is "known to work with the `browser` skill"
#: rather than a claim about what is current. `--version` exists because it will age.
PINNED = "0.1.18"

#: Where the generator puts its pages. Claude Code reads project skills from here, so it is
#: the vendor's choice as much as ours — named so the caller can be told what to expect.
SKILL_DIR = Path(".claude") / "skills" / "playwright-cli"


def install_argv(version: str) -> list[str]:
    """The generator invocation. Split out so a test can assert the command without a
    network round trip — the failure worth catching is a malformed `npx` line, and running
    it for real would make the suite depend on npm being reachable."""
    return ["npx", f"@playwright/cli@{version}", "install", "--skills"]


def npx_available() -> bool:
    return shutil.which("npx") is not None


def install(root: Path, version: str) -> tuple[int, str]:
    """Run the generator in `root`. Returns (exit status, combined output).

    The output is handed back rather than streamed so the caller can report what landed
    without the vendor's own progress noise becoming charter's.
    """
    proc = subprocess.run(install_argv(version), cwd=str(root),
                          capture_output=True, text=True)
    return proc.returncode, (proc.stdout or "") + (proc.stderr or "")
