"""A plane nested inside another plane's `workspaces/` is NAMED (#140).

`charter.toml` is tracked, so every clone of a plane is itself a plane — and `charter clone`
puts clones at `workspaces/<ws>/<repo>`. Standing in one, resolution stops at the innermost
marker, so the inner plane silently shadows the outer: its own `.charter/` state, its own
vault registry, its own workspace pointers.

The reported incident: `charter vault add reddit …` reported success, and `charter vault
list` from the real plane never showed it. The registration had gone to
`workspaces/showcase/charter/.charter/vaults.json`. Nothing was wrong with the command; the
shell was one directory too deep.

**Named, not resolved.** Changing `ROOT` resolution is the most invasive change available in
this codebase, and it would guess for you — the ambiguity is genuine, because sometimes the
inner plane IS the one you mean (charter's own dogfooding clones charter into a workspace,
and that clone is a plane you might legitimately manage). ADR 0013's second rule covers this
shape exactly: a divergence charter can see, charter names.

The status line already carried this alert. `doctor` did not, and `doctor` is what people
run to be told what is wrong — so the rule now lives in one place and both surfaces read it.
"""

from __future__ import annotations

import unittest
from pathlib import Path

from charter import config, doctor, root, statusline
from tests._isolation import PersonaIso

OK, WARN = doctor.OK, doctor.WARN


class NestedCase(PersonaIso):
    def setUp(self) -> None:
        super().setUp()
        (self.tmp / "charter.toml").write_text("schema = 1\n")
        config.HAS_CONTROL_PLANE = True

    def nest(self, ws: str = "showcase", repo: str = "charter") -> Path:
        """An inner plane exactly where `charter clone` puts one."""
        inner = self.tmp / "workspaces" / ws / repo
        inner.mkdir(parents=True, exist_ok=True)
        (inner / "charter.toml").write_text("schema = 1\n")
        return inner


class TestTheNestingIsDetected(NestedCase):
    def test_a_clone_under_workspaces_is_nested(self):
        inner = self.nest()
        self.assertEqual(root.enclosing_plane(inner), self.tmp.resolve())

    def test_the_outer_plane_itself_is_not_nested(self):
        self.assertIsNone(root.enclosing_plane(self.tmp))

    def test_a_plane_merely_BELOW_another_is_not_nested(self):
        """Only a plane inside `workspaces/` is the trap. A marker further up that does not
        own this path through its workspaces dir is an unrelated plane, and calling it a
        shadow would fire on directory layouts that are perfectly fine."""
        elsewhere = self.tmp / "src" / "other"
        elsewhere.mkdir(parents=True, exist_ok=True)
        (elsewhere / "charter.toml").write_text("schema = 1\n")
        self.assertIsNone(root.enclosing_plane(elsewhere))

    def test_a_deeper_clone_is_still_nested(self):
        inner = self.tmp / "workspaces" / "ws" / "repo" / "sub"
        inner.mkdir(parents=True, exist_ok=True)
        (inner / "charter.toml").write_text("schema = 1\n")
        self.assertEqual(root.enclosing_plane(inner), self.tmp.resolve())


class TestDoctorSaysSo(NestedCase):
    def test_doctor_warns_when_nested(self):
        inner = self.nest()
        config.ROOT = inner
        r = doctor.check_nested_plane()
        self.assertEqual(r.status, WARN)

    def test_the_warning_names_the_outer_plane(self):
        inner = self.nest()
        config.ROOT = inner
        r = doctor.check_nested_plane()
        self.assertIn(str(self.tmp.resolve()), f"{r.detail} {r.hint or ''}")

    def test_the_hint_says_commands_here_do_not_reach_the_outer_plane(self):
        """The incident was a `vault add` that reported success into the wrong plane, so
        the hint has to name the consequence, not just the geometry.

        Asserted on the consequence rather than one phrasing of it: `find_root` now hops
        outward, so reaching this state at all means $CHARTER_ROOT refused the hop, and the
        hint was rewritten to say so. What must survive a rewrite is that it names what
        goes wrong."""
        config.ROOT = self.nest()
        hint = doctor.check_nested_plane().hint or ""
        self.assertIn("never sees", hint)
        self.assertIn("CHARTER_ROOT", hint)

    def test_an_ordinary_plane_is_ok(self):
        self.assertEqual(doctor.check_nested_plane().status, OK)

    def test_doctor_runs_it(self):
        self.assertIn("nested plane", {r.name for r in doctor.run_all()})


class TestBothSurfacesReadOneRule(NestedCase):
    def test_the_status_line_and_doctor_agree(self):
        """They were implemented twice. Two copies of one rule is how the surface nobody is
        looking at ends up disagreeing — so the status line now delegates, and this asserts
        it still answers."""
        inner = self.nest()
        config.ROOT = inner
        self.assertEqual(statusline._nested_under(), root.enclosing_plane(inner))
        self.assertIsNotNone(statusline._nested_under())

    def test_the_status_line_never_raises_on_a_bad_root(self):
        """The render path's contract. `enclosing_plane` resolves paths, which can throw on
        a root that has been deleted underneath a running session."""
        config.ROOT = Path("/nonexistent/\x00bad")
        self.assertIsNone(statusline._nested_under())


if __name__ == "__main__":
    unittest.main()
