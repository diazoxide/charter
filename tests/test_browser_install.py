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


if __name__ == "__main__":
    unittest.main()
