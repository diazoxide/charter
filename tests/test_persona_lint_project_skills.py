"""Two charter commands disagreeing about what a skill is (#286).

`charter browser install` writes `.claude/skills/playwright-cli/` — that path chosen, in its
own words, "because Claude Code reads project skills from here". `charter persona lint` then
walked only `~/.claude/plugins` and rejected the result as *"not installed here … Remove it
or install the plugin"*.

That remedy cannot be followed. There is no plugin to install: charter deliberately does not
vendor Playwright's pages (Apache-2.0 into an MIT wheel, and a pre-1.0 pin moving to
charter's release cadence), and `browser install` exists precisely so the plane gets them
from the vendor at a version it picks. A reader who trusts the message hunts for a package
that does not exist; a reader who drops the declaration loses the preload for the one skill
charter told them to install.

The lint's own justification is that a dead entry is paid forever at dispatch for nothing —
which argues for accuracy about what EXISTS, not about what happens to be packaged.
"""
from __future__ import annotations

import tempfile
import unittest
from pathlib import Path
from unittest import mock

from charter import config, persona


def _skill(root: Path, name: str, *, human_only: bool = False) -> None:
    d = root / name
    d.mkdir(parents=True, exist_ok=True)
    dmi = "\ndisable-model-invocation: true" if human_only else ""
    (d / "SKILL.md").write_text(f"---\nname: {name}\ndescription: x{dmi}\n---\n\nbody\n")


class ProjectSkillsAreVisible(unittest.TestCase):
    def setUp(self):
        self.home = Path(tempfile.mkdtemp(prefix="charter-home-")).resolve()
        self.root = Path(tempfile.mkdtemp(prefix="charter-plane-")).resolve()
        # A plugin cache must exist, or the walk skips entirely by design.
        _skill(self.home / ".claude" / "plugins" / "charter" / "skills", "browser")
        self.enterContext(mock.patch.object(Path, "home", staticmethod(lambda: self.home)))
        self.enterContext(mock.patch.object(config, "ROOT", self.root))
        persona._reset_skill_cache()
        self.addCleanup(persona._reset_skill_cache)

    def test_a_project_skill_is_found(self):
        """The exact shape `charter browser install` leaves behind."""
        _skill(self.root / ".claude" / "skills", "playwright-cli")
        self.assertIn("playwright-cli", persona._walk_installed_skills())

    def test_a_personal_skill_is_found(self):
        """`~/.claude/skills/` is the third place Claude Code reads from, and a persona
        declaring one is in exactly the same position."""
        _skill(self.home / ".claude" / "skills", "my-own-thing")
        self.assertIn("my-own-thing", persona._walk_installed_skills())

    def test_plugin_skills_still_resolve(self):
        self.assertIn("browser", persona._walk_installed_skills())

    def test_declaring_a_project_skill_no_longer_errors(self):
        """The end-to-end case from the report."""
        _skill(self.root / ".claude" / "skills", "playwright-cli")
        self.assertEqual(persona._declared_skill_issues(["playwright-cli"]), [])

    def test_a_human_only_project_skill_is_still_warned_about(self):
        """Visibility must not cost the check its other half."""
        _skill(self.root / ".claude" / "skills", "handbook", human_only=True)
        issues = persona._declared_skill_issues(["handbook"])
        self.assertTrue(any("human-only" in m for _l, m in issues), issues)

    def test_a_genuinely_absent_skill_is_still_an_error(self):
        issues = persona._declared_skill_issues(["nope"])
        self.assertTrue(any(l == "error" for l, _m in issues), issues)

    def test_the_error_names_every_place_that_was_searched(self):
        """The old remedy — "install the plugin" — was impossible for a generated skill.
        Whatever the message says now, it must not send the reader after a package that
        does not exist."""
        issues = persona._declared_skill_issues(["nope"])
        said = " ".join(m for _l, m in issues)
        self.assertIn(".claude/skills", said)


class NoPluginCacheStillSkips(unittest.TestCase):
    """Unchanged on purpose. Without the plugin cache charter cannot see plugin-provided
    skills, so checking would flag every one of them as missing — the check has to stay
    silent rather than become confidently wrong on a fresh clone or in CI."""

    def setUp(self):
        self.home = Path(tempfile.mkdtemp(prefix="charter-nohome-")).resolve()
        self.root = Path(tempfile.mkdtemp(prefix="charter-plane2-")).resolve()
        self.enterContext(mock.patch.object(Path, "home", staticmethod(lambda: self.home)))
        self.enterContext(mock.patch.object(config, "ROOT", self.root))
        persona._reset_skill_cache()
        self.addCleanup(persona._reset_skill_cache)

    def test_no_plugin_cache_returns_none_even_with_project_skills(self):
        _skill(self.root / ".claude" / "skills", "playwright-cli")
        self.assertIsNone(persona._walk_installed_skills())


if __name__ == "__main__":
    unittest.main()
