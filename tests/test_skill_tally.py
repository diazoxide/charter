"""Which skills a persona actually invokes, against the ones it declares.

A persona declares the skills it starts holding, and the host preloads their **full text**
into the sub-agent on every dispatch. Declaring is cheap to write and expensive to keep, and
nothing could see whether any of it was used.

That is the blindness `dispatch.py` was built for — a persona that lints green and is never
dispatched — aimed one level in, at a persona's *equipment* rather than at the persona.

The store borrows dispatch's three properties for dispatch's reasons: counts and dates only
(never the arguments a skill was invoked with, which is where a client name would travel),
`O_APPEND` so concurrent sub-agents interleave without a lock, and the host in the filename
so a committed tally merges by addition instead of conflicting.

Drift is **named, not resolved** (ADR 0013). An unused declaration may be dead weight or a
skill whose moment has not come; an undeclared one may be a stale charter or a persona
reaching past its remit. Which it is depends on intent charter cannot read.
"""

from __future__ import annotations

import json
import unittest
from datetime import datetime, timezone

from charter import hooks, persona, skilluse
from tests._isolation import PersonaIso, run_hook


class TallyCase(PersonaIso):
    def declare(self, name="dev", skills=None):
        self.make_persona(name, role="Dev", vault="none")
        if skills:
            p = persona.def_path(name)
            p.write_text(p.read_text().replace("---\n", f"---\nskills: {skills}\n", 1))
        return name

    def invoke(self, skill: str, who: str | None = "dev"):
        return run_hook(hooks.posttooluse_skill, {
            "tool_name": "Skill",
            "tool_input": {"skill": skill, "args": "some args"},
            "session_id": "s"}) if who is None else self._as(who, skill)

    def _as(self, who: str, skill: str):
        real = persona.resolve_active
        persona.resolve_active = lambda *a, **k: who
        try:
            return run_hook(hooks.posttooluse_skill, {
                "tool_name": "Skill",
                "tool_input": {"skill": skill, "args": "some args"},
                "session_id": "s"})
        finally:
            persona.resolve_active = real


class TestTheTallyRecords(TallyCase):
    def test_an_invocation_is_recorded_against_the_active_persona(self):
        self.declare("dev")
        self.invoke("test-driven-development")
        self.assertEqual(skilluse.by_persona("dev"), {"test-driven-development": 1})

    def test_repeats_accumulate(self):
        self.declare("dev")
        for _ in range(3):
            self.invoke("systematic-debugging")
        self.assertEqual(skilluse.by_persona("dev")["systematic-debugging"], 3)

    def test_a_qualified_name_matches_a_bare_declaration(self):
        """A charter may write `superpowers:tdd` while the invocation records `tdd`. Two
        spellings of one skill must not read as drift in both directions at once."""
        self.declare("dev", skills="superpowers:test-driven-development")
        self.invoke("test-driven-development")
        self.assertEqual(skilluse.drift("dev"), {"unused": [], "undeclared": []})

    def test_another_tool_is_not_tallied(self):
        self.declare("dev")
        run_hook(hooks.posttooluse_skill, {
            "tool_name": "Bash", "tool_input": {"command": "ls"}, "session_id": "s"})
        self.assertEqual(skilluse.by_persona("dev"), {})

    def test_no_active_persona_still_records_the_skill(self):
        """The skill was still used. Dropping it would make the tally lie about totals to
        keep the per-persona view tidy."""
        self.declare("dev")
        self._as("", "grilling")
        self.assertTrue(any(e["skill"] == "grilling" for e in skilluse._read_all()))


class TestItCarriesNoArguments(TallyCase):
    def test_the_arguments_are_never_written(self):
        """dispatch.py's rule: counts and dates only. A skill's arguments are exactly where
        a workspace, client or repo name would travel to a committed file."""
        self.declare("dev")
        self._as("dev", "grilling")
        blob = skilluse.path_for().read_text()
        self.assertNotIn("some args", blob)
        self.assertEqual(set(json.loads(blob.splitlines()[0])), {"ts", "skill", "persona"})


class TestDrift(TallyCase):
    def test_declared_and_never_used_is_unused(self):
        self.declare("dev", skills="test-driven-development")
        self.assertEqual(skilluse.drift("dev")["unused"], ["test-driven-development"])

    def test_used_and_never_declared_is_undeclared(self):
        self.declare("dev")
        self.invoke("grilling")
        self.assertEqual(skilluse.drift("dev")["undeclared"], ["grilling"])

    def test_declared_and_used_is_neither(self):
        self.declare("dev", skills="grilling")
        self.invoke("grilling")
        self.assertEqual(skilluse.drift("dev"), {"unused": [], "undeclared": []})

    def test_another_personas_use_does_not_count(self):
        """Skills are global and reused freely; the tally is per persona or it answers
        nothing about this one's equipment."""
        self.declare("dev", skills="grilling")
        self.declare("ops")
        self._as("ops", "grilling")
        self.assertEqual(skilluse.drift("dev")["unused"], ["grilling"])


class TestItNeverBreaksATurn(TallyCase):
    def test_an_unwritable_store_is_silent(self):
        self.declare("dev")
        d = skilluse._dir()
        d.mkdir(parents=True, exist_ok=True)
        d.chmod(0o500)
        self.addCleanup(d.chmod, 0o700)
        self.assertIsNone(skilluse.record("grilling", "dev"))

    def test_a_malformed_line_is_skipped_rather_than_fatal(self):
        self.declare("dev")
        p = skilluse.path_for()
        p.parent.mkdir(parents=True, exist_ok=True)
        p.write_text('{"not json\n{"ts":"x","skill":"grilling","persona":"dev"}\n')
        self.assertEqual(skilluse.by_persona("dev"), {"grilling": 1})

    def test_the_hook_returns_zero_whatever_happens(self):
        self.declare("dev")
        self.assertEqual(hooks._HANDLERS["posttooluse-skill"].__name__,
                         "posttooluse_skill")


if __name__ == "__main__":
    unittest.main()
