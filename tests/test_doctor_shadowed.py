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

from charter import config, doctor, docsrc  # noqa: F401
from tests._isolation import PersonaIso

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


class TestCharterItselfIsExemptHoweverItWasInstalled(PersonaIso):
    """The exemption used to key on `docsrc.source()`, which prefers the PACKAGED copy — so
    it only matched for someone running `python3 -m charter` from the clone. Every
    contributor also has `uv tool install charter-cp`, and for them doctor reported all
    eight of charter's own pages as shadows of themselves: precisely the noise the
    docstring promises to prevent, on the machine most likely to run the check."""

    def make_checkout(self) -> Path:
        root = Path(self.tmp) / "charter-clone"
        (root / "charter").mkdir(parents=True)
        (root / "charter" / "docsrc.py").write_text("# source\n")
        (root / "pyproject.toml").write_text('[project]\nname = "charter-cp"\n')
        (root / "docs").mkdir()
        for topic in docsrc.topics()[:3] or ["personas"]:
            (root / "docs" / f"{topic}.md").write_text("# ours\n")
        return root

    def test_exempt_when_running_from_a_checkout(self):
        root = self.make_checkout()
        self.assertEqual(doctor.shadowed_knowledge(root), {"skills": [], "docs": []})

    def test_still_exempt_when_the_cli_is_an_installed_wheel(self):
        """The case that was broken. `docsrc.source()` points into site-packages and has
        nothing to do with which plane is being checked."""
        root = self.make_checkout()
        packaged = Path(self.tmp) / "site-packages" / "charter" / "_docs"
        packaged.mkdir(parents=True)
        for p in (root / "docs").glob("*.md"):
            (packaged / p.name).write_text(p.read_text())
        real = docsrc._PACKAGED
        docsrc._PACKAGED = packaged
        self.addCleanup(setattr, docsrc, "_PACKAGED", real)
        self.assertEqual(doctor.shadowed_knowledge(root), {"skills": [], "docs": []})

    def test_an_ordinary_plane_is_still_reported(self):
        """The exemption must not have widened into "never report anything"."""
        root = Path(self.tmp) / "someones-plane"
        (root / "docs").mkdir(parents=True)
        topic = (docsrc.topics() or ["personas"])[0]
        (root / "docs" / f"{topic}.md").write_text("# a local copy\n")
        self.assertIn(topic, doctor.shadowed_knowledge(root)["docs"])


class TestTheCheckItselfIsWired(PersonaIso):
    """The helper was well covered and the check could never run.

    `tests/test_doctor_shadowed.py` exercised only `shadowed_knowledge(root)` with an
    explicit argument, so the one line that resolves the root — the only line that could
    raise — was never executed. 2201 tests passed over a check that raised NameError on
    every invocation, and the `except Exception` around it rendered that as a benign
    "not checked" environment warning.

    A helper-only test cannot catch a wiring bug by construction. This calls the check.
    """

    def test_it_returns_a_real_verdict(self):
        r = doctor.check_shadowed_knowledge()
        self.assertIn(r.status, (doctor.OK, doctor.WARN))
        self.assertNotIn("not checked", r.detail)

    def test_it_reports_a_shadow_when_there_is_one(self):
        """Wired AND correct: the check reaches the helper whose logic the rest of this
        file covers."""
        topic = (docsrc.topics() or ["personas"])[0]
        docs = Path(config.ROOT) / "docs"
        docs.mkdir(parents=True, exist_ok=True)
        (docs / f"{topic}.md").write_text("# a local copy\n")
        r = doctor.check_shadowed_knowledge()
        self.assertEqual(r.status, doctor.WARN)
        self.assertIn(topic, r.detail)


class TestNoCheckHidesAPythonErrorInAWarning(PersonaIso):
    """The class, not the instance.

    Any check whose `not checked` branch can be reached by a NameError or AttributeError
    reports a code defect as a soft environment warning — which reads as "your machine is
    odd" rather than "this is broken", and is why the above shipped green.

    Asserting on the *exception signature* rather than on the words "not checked": a
    genuinely unreadable directory or a timed-out subprocess is entitled to say that, and a
    test forbidding it outright would be wrong the first time someone's disk misbehaved.
    """

    #: What a leaked Python error looks like inside a detail string.
    _SIGNATURES = ("is not defined", "has no attribute", "NameError", "AttributeError",
                   "TypeError", "unexpected keyword", "not subscriptable")

    def test_no_check_reports_a_python_error_as_an_environment_warning(self):
        leaked = []
        for r in doctor.run_all():
            blob = f"{r.detail} {r.hint}"
            for sig in self._SIGNATURES:
                if sig in blob:
                    leaked.append(f"{r.name}: {r.detail}")
                    break
        self.assertEqual(leaked, [], "a code defect is being reported as 'not checked':\n"
                                     + "\n".join(leaked))


if __name__ == "__main__":
    unittest.main()
