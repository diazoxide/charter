"""Charter generates Playwright's driving surface; it does not carry it.

Vendoring the generated pages would have cost twice. `@playwright/cli` is Apache-2.0 and
charter is MIT, so every wheel and every plugin install would inherit a second licence and
its attribution obligations, for content charter neither wrote nor maintains. And the pin
would move to *charter's* release cadence: a Playwright fix would wait on a charter release
to reach anybody, for a package that is pre-1.0 and publishes often.

Running the vendor's own generator has neither cost. What has to hold instead is that the
invocation is right — a malformed `npx` line fails at the moment somebody needs a browser,
which is the worst time to discover it. The suite must not reach the network, so the
command is asserted rather than run.
"""
from __future__ import annotations

import unittest
from pathlib import Path

from charter import browser

ROOT = Path(__file__).resolve().parents[1]


class TestInstallInvocation(unittest.TestCase):
    def test_it_pins_the_version_it_installs(self):
        """An unpinned `npx @playwright/cli` resolves to whatever is current that day, so
        two engineers on the same plane get different behaviour from the same skill."""
        argv = browser.install_argv("1.2.3")
        self.assertIn("@playwright/cli@1.2.3", argv)
        self.assertNotIn("@playwright/cli", argv, "the bare, unpinned spec must not appear")

    def test_it_asks_for_skills_not_a_browser_install(self):
        """`npx @playwright/cli install` (without --skills) downloads browser binaries —
        hundreds of megabytes, and not what this command is for."""
        argv = browser.install_argv(browser.PINNED)
        self.assertEqual(argv[-2:], ["install", "--skills"])

    def test_the_default_pin_is_a_release_shaped_string(self):
        self.assertRegex(browser.PINNED, r"^\d+\.\d+\.\d+$")

    def test_it_writes_where_claude_code_reads_project_skills(self):
        self.assertEqual(browser.SKILL_DIR, Path(".claude") / "skills" / "playwright-cli")


class TestCharterShipsNoneOfIt(unittest.TestCase):
    def test_the_repo_vendors_no_playwright_pages(self):
        """The licence decision, kept honest. Someone adding the generated skill to the
        repo "just so it is there" would put Apache-2.0 content into an MIT wheel without
        the attribution that requires, and this is the only thing that would notice."""
        vendored = ROOT / "skills" / "playwright-cli"
        self.assertFalse(vendored.exists(),
                         "charter must not vendor Playwright's generated pages — "
                         "`charter browser install` generates them into a plane instead")

    def test_the_browser_skill_points_at_the_generator(self):
        """A skill that describes the credential bridge but never says where the page
        surface comes from leaves the reader to guess, and guessing here means vendoring."""
        body = (ROOT / "skills" / "browser" / "SKILL.md").read_text()
        self.assertIn("charter browser install", body)


class TestBridgeGuidanceSurvives(unittest.TestCase):
    """The two things that silently produce a leaked or bogus login. Both were learned the
    hard way and are worthless if an edit quietly drops them."""

    def _body(self) -> str:
        return (ROOT / "skills" / "browser" / "SKILL.md").read_text()

    def test_it_warns_the_session_must_open_inside_the_bridge(self):
        """`PLAYWRIGHT_MCP_SECRETS_FILE` is read once, when the session daemon starts.
        Set it later and the literal placeholder is typed into the password field with no
        error at all — the failure looks like a wrong password, not a wiring mistake."""
        body = self._body().lower()
        self.assertIn("silently", body)
        self.assertIn("open", body)

    def test_it_forbids_reading_a_filled_secret_back(self):
        body = self._body().lower()
        self.assertIn("never read a filled secret back", body)


class TestTheGeneratorCannotHangOrPromptInvisibly(unittest.TestCase):
    """Two ways the same command stalls with nothing on screen.

    npm's documentation: a prompt "can be suppressed by providing either --yes or --no.
    When standard input is not a TTY or a CI environment is detected, --yes is assumed."
    An agent is therefore fine and a HUMAN is not — run it in a terminal against a cold
    cache and npx asks a question, charter has captured the pipe it was printed to, and
    the operator sees an unexplained pause while something waits on an answer they were
    never shown.

    And the call had no timeout at all, on the one operation in this module that reaches
    the network. `util.run`'s docstring records what that costs: "every un-timeouted path
    could hang indefinitely: a 1Password session needing re-auth stalled the SessionStart
    preflight"."""

    def test_the_prompt_is_suppressed(self):
        self.assertIn("--yes", browser.install_argv("0.1.18"))

    def test_the_package_spec_still_carries_the_version(self):
        argv = browser.install_argv("9.9.9")
        self.assertIn("@playwright/cli@9.9.9", argv)
        self.assertEqual(argv[0], "npx")

    def test_the_run_is_bounded(self):
        self.assertTrue(0 < browser.INSTALL_TIMEOUT <= 600)

    def test_a_timeout_is_reported_as_a_failure_not_a_traceback(self):
        """The caller prints the generator's own words (ADR 0009). A TimeoutExpired
        reaching the user as a traceback would be charter failing to classify its own
        failure."""
        from charter import util
        real = util.run
        util.run = lambda *a, **k: (_ for _ in ()).throw(
            util.ProcTimeout("npx", browser.INSTALL_TIMEOUT))
        self.addCleanup(setattr, util, "run", real)
        code, out = browser.install(Path("."), "0.1.18")
        self.assertNotEqual(code, 0)
        self.assertIn("did not finish", out)
        self.assertIn("npx", out)


class TestTheOutputDirectoryIsIgnored(unittest.TestCase):
    """`.playwright-cli/` is the CLI's OUTPUT directory — `cliOutputDir` in the vendor's
    source, holding traces under `trace/` plus snapshots and screenshots. #278 called it
    "per-machine session state"; it is not (a session lives under
    `~/Library/Caches/ms-playwright/daemon/`), and the truth is worse. A trace records
    network requests with their headers and bodies, so a trace taken during a
    `charter secret exec` login holds the login POST — the one thing the credential bridge
    exists to keep out of reach.

    So the ignore line is charter enforcing a rule it had already written down, not a new
    opinion: `commands_secrets` already refuses a vault file git would take.

    The generated skill and `.playwright/` are deliberately NOT covered. Neither carries a
    credential, so committing them is a plane's call, and charter states the trade-off
    instead of quietly deciding it (ADR 0017).
    """

    def setUp(self) -> None:
        import tempfile
        self.root = Path(tempfile.mkdtemp(prefix="charter-browser-"))

    def test_the_output_dir_is_the_vendors_cli_output_dir(self):
        """Pinned against the vendor's `cliOutputDir`, not against the name in the report
        that prompted this — the two disagreed, and the source won."""
        self.assertEqual(browser.OUTPUT_DIR, Path(".playwright-cli"))

    def test_the_config_dir_is_known_but_not_ignored(self):
        """`install` creates it (`initWorkspace`, for cli.config.json). Charter names it so
        the operator is not surprised, and ignores nothing about it."""
        self.assertEqual(browser.CONFIG_DIR, Path(".playwright"))

    def test_it_ignores_the_output_directory(self):
        added = browser.ensure_output_ignored(self.root)
        body = (self.root / ".gitignore").read_text()
        self.assertIn(".playwright-cli/", body.splitlines())
        self.assertEqual(added, [".playwright-cli/"])

    def test_it_ignores_neither_the_generated_skill_nor_the_config_dir(self):
        """The opinionated half, pinned so a later "while we're here" cannot quietly
        take either decision away from the plane."""
        browser.ensure_output_ignored(self.root)
        body = (self.root / ".gitignore").read_text()
        self.assertNotIn("playwright-cli/", body.replace(".playwright-cli/", ""))
        self.assertNotIn(str(browser.SKILL_DIR), body)
        self.assertNotIn(f"{browser.CONFIG_DIR}/", body)

    def test_running_it_twice_changes_nothing(self):
        browser.ensure_output_ignored(self.root)
        self.assertEqual(browser.ensure_output_ignored(self.root), [])


class TestTheInstallStatesBothPostures(unittest.TestCase):
    """The whole complaint in #278 is that the command said nothing about either path, so
    every plane re-decided both in silence. Silence is the regression to guard against."""

    def _run_install(self):
        import io
        from contextlib import redirect_stderr, redirect_stdout
        from unittest import mock
        import tempfile
        from charter import commands, config

        root = Path(tempfile.mkdtemp(prefix="charter-install-"))
        (root / browser.SKILL_DIR).mkdir(parents=True)
        (root / browser.SKILL_DIR / "page.md").write_text("x")
        out, err = io.StringIO(), io.StringIO()
        with mock.patch.object(config, "ROOT", root), \
             mock.patch.object(config, "HAS_CONTROL_PLANE", True), \
             mock.patch.object(browser, "npx_available", lambda: True), \
             mock.patch.object(browser, "install", lambda r, v: (0, "")), \
             redirect_stdout(out), redirect_stderr(err):
            code = commands.cmd_browser_install(mock.Mock(version=None))
        return code, out.getvalue() + err.getvalue(), root

    def test_it_reports_the_ignore_line_it_wrote(self):
        code, said, root = self._run_install()
        self.assertEqual(code, 0)
        self.assertIn(".playwright-cli/", said)
        self.assertIn(".playwright-cli/", (root / ".gitignore").read_text().splitlines())

    def test_it_states_the_undecided_paths_are_the_planes_call(self):
        """Not "commit it" and not "do not" — the costs, and whose decision it is."""
        _, said, _ = self._run_install()
        self.assertIn(str(browser.SKILL_DIR), said)
        self.assertIn(str(browser.CONFIG_DIR), said)
        self.assertIn("docs/browser.md", said)


if __name__ == "__main__":
    unittest.main()
