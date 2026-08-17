"""A plane keeping its own copy of something charter ships.

The failure this reports was found in a real control plane: a `setup` skill instructing
engineers to authenticate over SSH and add an SSH key, months after the rule became
token-only-over-HTTPS and charter's own PreToolUse guard began denying exactly that. Nine
skills there were in some stage of the same rot. Every one looked wired. Nothing compared
any of them to the CLI they described.

The check reports and never resolves, because drift runs both ways — the same plane's
persona page had grown sections upstream never received. A copy is not automatically wrong;
it is automatically *unwatched*.

The subtle case, and the reason this file exists rather than one assertion: charter's own
checkout **is** a control plane, and its `docs/personas.md` is the page `docs show personas`
serves. A naive check flags it as shadowing itself, on the one machine most likely to run
`doctor`.
"""
from __future__ import annotations

import tempfile
import unittest
from pathlib import Path

from charter import doctor

REPO_ROOT = Path(__file__).resolve().parents[1]


def _plane(tmp: Path, *, skills=(), docs=()) -> Path:
    """A throwaway plane carrying the named skills and doc pages."""
    for s in skills:
        d = tmp / ".claude" / "skills" / s
        d.mkdir(parents=True, exist_ok=True)
        (d / "SKILL.md").write_text(f"---\nname: {s}\n---\n")
    for name in docs:
        d = tmp / "docs"
        d.mkdir(parents=True, exist_ok=True)
        (d / f"{name}.md").write_text("# page\n")
    return tmp


class TestShadowDetection(unittest.TestCase):
    def test_a_plane_copy_of_a_shipped_skill_is_named(self):
        with tempfile.TemporaryDirectory() as t:
            root = _plane(Path(t), skills=("secrets", "browser"))
            hit = doctor.shadowed_knowledge(root)
            self.assertEqual(hit["skills"], ["browser", "secrets"])

    def test_a_plane_copy_of_a_shipped_page_is_named(self):
        with tempfile.TemporaryDirectory() as t:
            root = _plane(Path(t), docs=("personas", "secrets"))
            hit = doctor.shadowed_knowledge(root)
            self.assertEqual(hit["docs"], ["personas", "secrets"])

    def test_a_planes_own_skills_and_pages_are_left_alone(self):
        """The check must not object to a plane having knowledge of its own — only to it
        re-stating charter's. A false positive here would train operators to ignore it."""
        with tempfile.TemporaryDirectory() as t:
            root = _plane(Path(t),
                          skills=("incident-investigation", "deploy-runbook"),
                          docs=("topology", "conventions", "onboarding"))
            hit = doctor.shadowed_knowledge(root)
            self.assertEqual(hit, {"skills": [], "docs": []})

    def test_topology_is_not_a_shadow(self):
        """`docs/topology.md` is written *into* a plane by `charter docs`. Reporting
        charter's own generated output as a duplicate of charter would be absurd, and it is
        the page most certain to be present."""
        with tempfile.TemporaryDirectory() as t:
            root = _plane(Path(t), docs=("topology",))
            self.assertEqual(doctor.shadowed_knowledge(root)["docs"], [])

    def test_a_directory_without_a_skill_file_is_not_a_skill(self):
        with tempfile.TemporaryDirectory() as t:
            (Path(t) / ".claude" / "skills" / "secrets").mkdir(parents=True)
            self.assertEqual(doctor.shadowed_knowledge(Path(t))["skills"], [])

    def test_an_empty_plane_is_quiet(self):
        with tempfile.TemporaryDirectory() as t:
            self.assertEqual(doctor.shadowed_knowledge(Path(t)),
                             {"skills": [], "docs": []})


class TestCharterDoesNotShadowItself(unittest.TestCase):
    def test_this_repo_reports_nothing(self):
        """charter's checkout is a plane whose docs/ *is* the served source. Without the
        guard, `doctor` here would report seven pages shadowing themselves."""
        hit = doctor.shadowed_knowledge(REPO_ROOT)
        self.assertEqual(hit, {"skills": [], "docs": []})


class TestShippedSkillsStayInSync(unittest.TestCase):
    def test_the_constant_matches_the_shipped_directory(self):
        """The CLI ships in a wheel with no `skills/` — that directory belongs to the
        plugin — so the check carries a constant. Adding a skill without updating it would
        leave the new skill shadowable and unreported, which is this check's own failure
        mode turned inward."""
        on_disk = {d.name for d in (REPO_ROOT / "skills").iterdir()
                   if d.is_dir() and (d / "SKILL.md").is_file()}
        self.assertEqual(set(doctor.SHIPPED_SKILLS), on_disk)


if __name__ == "__main__":
    unittest.main()
