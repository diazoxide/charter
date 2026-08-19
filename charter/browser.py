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

#: The CLI's OUTPUT directory — `cliOutputDir` in the vendor's own source, with traces under
#: `.playwright-cli/trace` and snapshots, screenshots and PDFs beside them. Created when
#: something is written there, not by `install`, which is why an operator meets it as a
#: surprise `??` entry with nothing on screen to connect it to the command that caused it.
#:
#: Not the session store, despite the name: a session lives in
#: `~/Library/Caches/ms-playwright/daemon/<hash>/<name>.session`, outside any repo, as the
#: `browser` skill already says. #278 reported this directory as "per-machine session state";
#: it is artifacts, and that turns out to matter MORE rather than less — see `ensure_output_ignored`.
OUTPUT_DIR = Path(".playwright-cli")

#: The workspace directory `install` itself creates (`initWorkspace`), for
#: `.playwright/cli.config.json`. Project configuration, not credentials and not generated
#: output — so ADR 0017 puts it on the "state it, do not decide it" side of the line, and
#: charter says nothing about whether you commit it beyond naming it in `docs/browser.md`.
CONFIG_DIR = Path(".playwright")


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


def ensure_output_ignored(root: Path) -> list[str]:
    """Ignore the CLI's output directory in *root*'s `.gitignore`. Returns what was added.

    The reason is traces, and it is sharper than the "live session" #278 described. A
    Playwright trace records the **network**: requests with their headers and bodies, plus
    DOM snapshots of the pages that produced them. So a trace taken during a `charter secret
    exec` login captures the login POST — the exact thing the credential bridge exists to
    keep out of reach. Charter redacts the value from its own output and Playwright
    substitutes it into the page without printing it; a trace then writes the authenticated
    traffic to disk beside your source, where `git add -A` is waiting.

    That is charter's rule already, not a new opinion: `commands_secrets` refuses a vault
    file git would take, and the `browser` skill's hard rules forbid committing session or
    storage-state files "because they carry live cookies, which are the credential in
    another form". A trace carries more.

    Deliberately silent about `SKILL_DIR` and `CONFIG_DIR`. Generated pages and a project
    config file carry no credential, so whether a plane commits them is a real trade-off with
    two defensible answers — and charter's job there is to state them, not settle them by
    writing a line while nobody is looking (ADR 0017).
    """
    from . import util
    return util.append_gitignore(root, [f"{OUTPUT_DIR}/"],
                                 "added by `charter browser install`")
