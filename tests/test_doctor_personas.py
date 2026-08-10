"""`charter doctor` reports roster health.

`persona lint` has always been able to find a dangling `extends:`, an inheritance
cycle, a charter naming a skill no agent can invoke — and nothing ever ran it. It was
absent from `doctor`, absent from every hook, and reported drift only to someone who
already suspected drift. A check nobody runs is close to no check at all.

WARN, never FAIL: doctor's blockers list means "you cannot work", and an untidy
persona does not stop you cloning a repo or reaching the forge. Keeping the roster out
of the exit code is what preserves that meaning.
"""

from __future__ import annotations

import unittest

from charter import config, doctor, persona
from tests._isolation import PersonaIso


class PersonaCheck(PersonaIso):
    def _persona(self, name, body="Real charter.", **meta):
        d = config.PERSONAS_DIR / name
        d.mkdir(parents=True, exist_ok=True)
        fm = "\n".join(f"{k}: {v}" for k, v in {"name": name, **meta}.items())
        (d / "persona.md").write_text(f"---\n{fm}\n---\n\n{body}\n")
        persona.scaffold_memory(name)

    def _ok(self, name):
        self._persona(name, role=name.title(), vault=name,
                      **{"delegate-when": f"{name} work"})

    def test_a_healthy_roster_is_ok(self):
        self._ok("alpha")
        self._ok("beta")
        r = doctor.check_personas()
        self.assertEqual(r.status, doctor.OK)
        self.assertIn("2", r.detail)

    def test_no_personas_is_ok_not_a_problem(self):
        """A control plane that hasn't defined any personas is a normal control
        plane, not a broken one."""
        self.assertEqual(doctor.check_personas().status, doctor.OK)

    def test_a_dangling_extends_warns(self):
        self._ok("alpha")
        self._persona("orphan", role="O", vault="o", extends="ghost",
                      **{"delegate-when": "x"})
        r = doctor.check_personas()
        self.assertEqual(r.status, doctor.WARN)
        self.assertIn("orphan", r.detail)

    def test_a_draft_warns(self):
        self._persona("wip", role="W", vault="w", draft="true",
                      **{"delegate-when": "x"})
        r = doctor.check_personas()
        self.assertEqual(r.status, doctor.WARN)
        self.assertIn("wip", r.detail)

    def test_it_never_fails(self):
        """Every way a persona can be broken, at once — still not a blocker."""
        self._persona("a", role="A", vault="a", extends="ghost")
        self._persona("b", role="B", vault="b", uses="ghost", draft="true")
        self._persona("c", vault="c")
        self.assertNotEqual(doctor.check_personas().status, doctor.FAIL)

    def test_it_points_at_the_command_that_explains(self):
        self._persona("orphan", role="O", vault="o", extends="ghost")
        self.assertIn("charter persona lint", doctor.check_personas().hint)

    def test_a_broken_roster_never_raises(self):
        """Doctor is the command you run when things are wrong; it may not itself
        blow up on a malformed persona."""
        d = config.PERSONAS_DIR / "bad"
        d.mkdir(parents=True, exist_ok=True)
        (d / "persona.md").write_text("---\nthis is not: [valid\n---\n")
        doctor.check_personas()      # must not raise

    def test_it_is_registered_in_run_all(self):
        self._ok("alpha")
        names = [r.name for r in doctor.run_all()]
        self.assertIn("personas", names)


class DoctorAffordsIt(PersonaIso):
    """The check is only viable because the plugin-cache walk is memoised — it used
    to cost 27ms per persona."""

    def test_the_roster_sweep_walks_the_plugin_cache_once(self):
        persona._reset_skill_cache()
        self.addCleanup(persona._reset_skill_cache)
        walks = []
        orig = persona._walk_installed_skills
        persona._walk_installed_skills = lambda: (walks.append(1), orig())[1]
        try:
            for n in ("a", "b", "c", "d", "e"):
                d = config.PERSONAS_DIR / n
                d.mkdir(parents=True, exist_ok=True)
                (d / "persona.md").write_text(
                    f"---\nname: {n}\nrole: R\nvault: {n}\ndelegate-when: x\n---\n\n"
                    "Uses `superpowers:test-driven-development`.\n")
                persona.scaffold_memory(n)
            doctor.check_personas()
        finally:
            persona._walk_installed_skills = orig
        self.assertLessEqual(len(walks), 1)


class MemoryIndexDocstringIsHonest(unittest.TestCase):
    """`check_memory_indexes` justified WARN-never-FAIL with "this runs from the
    SessionStart hook, which must never block a session". It does not: `hooks.py`
    never imports `doctor`, and the function has no caller outside doctor's own
    registry. The conclusion was right; the stated reason was not."""

    def test_it_does_not_claim_to_run_from_a_hook_that_never_calls_it(self):
        doc = doctor.check_memory_indexes.__doc__ or ""
        # The note may still quote the old claim in order to correct it; what it may
        # not do is assert it. Anchor on the correction being present.
        self.assertNotIn("and this runs from the\n    SessionStart hook", doc)
        self.assertIn("never imports this module", doc)

    def test_hooks_really_does_not_import_doctor(self):
        from pathlib import Path
        src = (Path(__file__).resolve().parent.parent / "charter" / "hooks.py").read_text()
        self.assertNotIn("import doctor", src)


if __name__ == "__main__":
    unittest.main()


class RunTakesATimeoutAndDoctorStreams(unittest.TestCase):
    """A 1Password session needing re-auth stalled the SessionStart preflight for its whole
    20s budget and then printed NOTHING — `cmd_doctor` collected every Result before
    showing a line, so a hang and a crash looked identical. Six call sites also bypassed
    `util.run` entirely just to pass their own timeout literal."""

    def test_run_raises_a_charter_error_not_subprocess_timeoutexpired(self):
        """`cli.main` catches `KeyboardInterrupt` and nothing else, so a bare
        `TimeoutExpired` reached the user as a traceback from inside charter."""
        from charter import util
        with self.assertRaises(util.ProcTimeout) as e:
            util.run(["sleep", "5"], timeout=0.2)
        self.assertEqual(e.exception.seconds, 0.2)
        self.assertIn("timed out", str(e.exception))

    def test_a_command_without_a_timeout_is_unbounded_as_before(self):
        from charter import util
        self.assertEqual(util.run(["true"], check=False).returncode, 0)

    def test_doctor_yields_results_one_at_a_time(self):
        """The streaming half: what makes a killed preflight say where it stopped."""
        from charter import doctor
        it = doctor.iter_all()
        first = next(it)
        self.assertIsInstance(first, doctor.Result)
        self.assertTrue(first.name)

    def test_run_all_still_returns_them_all(self):
        from charter import doctor
        self.assertEqual(len(doctor.run_all()), len(list(doctor.iter_all())))

    def test_a_timed_out_vault_is_its_own_state(self):
        """Not "unhealthy" — the vault is fine, the provider CLI did not answer, and the
        fix is unrelated to the vault."""
        import os
        from unittest import mock
        from charter import doctor, util
        from charter.secrets import registry
        with mock.patch.object(registry, "vaults", return_value={"ops": {}}), \
             mock.patch.object(registry, "provider_for",
                               side_effect=util.ProcTimeout(["op"], 5.0)):
            res = doctor.check_vaults()
        self.assertEqual(res.status, doctor.WARN)
        self.assertIn("timed out", res.hint)
