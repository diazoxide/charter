"""A plane cloned into another plane's ``workspaces/`` resolves to the OUTER one.

`find_root` took the innermost ``charter.toml`` — the git/cargo/npm contract, and right
almost everywhere. But `charter clone` puts clones at ``workspaces/<ws>/<repo>``, and a
cloned repo may carry its own tracked ``charter.toml``. Standing in one, the active plane
silently became a different plane: different personas, no vault, memory landing where
nobody chose.

#140 detected this and deliberately left resolution alone, reasoning that sometimes the
inner plane IS the one you mean — naming charter's own dogfooding as the example. Measured
against that very example, the inner plane holds **no vaults**, a subset of the workspaces,
and 47 **tracked** persona files, so `charter persona remember` there writes into the
charter repo's git index rather than the operator's plane. The justification was
contradicted by the one case it named (#200).

**Scoped to the `workspaces/` relationship, not "outermost wins".** A bare walk to the
topmost marker would let a stray ``charter.toml`` in ``~`` capture every plane beneath it.
The redirect fires only for the structural relationship charter itself creates, which is
exactly what `enclosing_plane` already tested.

**The redirect must remember what it redirected from.** Once `find_root` answers with the
outer plane, ``enclosing_plane(config.ROOT)`` is `None` by construction — the outer is not
nested — so every surface that named the nesting would go quiet, and `charter` would be
silently doing something the operator cannot see. `standing_in_nested_plane` is that fact.
"""

from __future__ import annotations

import os
import unittest
from pathlib import Path

from charter import root, util
from tests._isolation import PersonaIso


class PlaneCase(PersonaIso):
    def plane(self, *parts: str) -> Path:
        d = self.tmp.joinpath(*parts)
        d.mkdir(parents=True, exist_ok=True)
        (d / "charter.toml").write_text("schema = 1\n")
        return d

    def setUp(self) -> None:
        super().setUp()
        # `find_root` consults $CHARTER_ROOT before walking, and the developer running the
        # suite may well have one exported — it would win outright and every walk test
        # would assert against their plane instead of the fixture.
        self._env = os.environ.pop(root.ENV_VAR, None)
        if self._env is not None:
            self.addCleanup(os.environ.__setitem__, root.ENV_VAR, self._env)


class TestTheOuterPlaneWins(PlaneCase):
    def test_a_clone_in_workspaces_resolves_to_the_outer_plane(self):
        outer = self.plane("outer")
        inner = self.plane("outer", "workspaces", "w1", "repo")
        self.assertEqual(root.find_root(inner), outer.resolve())

    def test_a_directory_below_the_clone_resolves_the_same_way(self):
        """The operator is rarely standing exactly on the marker — they are in `charter/`
        or `tests/` inside it."""
        outer = self.plane("outer")
        inner = self.plane("outer", "workspaces", "w1", "repo")
        deep = inner / "charter" / "sub"
        deep.mkdir(parents=True)
        self.assertEqual(root.find_root(deep), outer.resolve())

    def test_a_lone_plane_still_resolves_to_itself(self):
        """The ordinary case, and the one that must not move."""
        solo = self.plane("solo")
        self.assertEqual(root.find_root(solo), solo.resolve())

    def test_a_chain_resolves_to_the_outermost_of_the_chain(self):
        """A plane inside a plane inside a plane. Hopping once would leave the answer in
        the middle — still not the plane holding the vault."""
        top = self.plane("top")
        mid = self.plane("top", "workspaces", "a", "mid")
        bottom = self.plane("top", "workspaces", "a", "mid", "workspaces", "b", "leaf")
        self.assertEqual(root.find_root(bottom), top.resolve())
        self.assertEqual(root.find_root(mid), top.resolve())


class TestTheRedirectIsNarrow(PlaneCase):
    def test_a_marker_above_that_is_not_via_workspaces_does_not_capture(self):
        """The whole reason this is not "outermost wins": a `charter.toml` anywhere up the
        tree — a stray one in `~`, an unrelated plane — would otherwise swallow every plane
        beneath it."""
        self.plane("above")
        inner = self.plane("above", "somewhere", "else", "plane")
        self.assertEqual(root.find_root(inner), inner.resolve())

    def test_a_sibling_workspaces_dir_does_not_capture(self):
        """`workspaces/` has to be the OUTER plane's own, on the path between the two."""
        self.plane("above")
        inner = self.plane("above", "notworkspaces", "w1", "repo")
        self.assertEqual(root.find_root(inner), inner.resolve())

    def test_charter_root_still_wins_outright(self):
        """The escape hatch for anyone who genuinely means the inner plane. `find_root`
        documents $CHARTER_ROOT as winning outright, and that must survive a rule which
        would otherwise make the inner plane unreachable."""
        self.plane("outer")
        inner = self.plane("outer", "workspaces", "w1", "repo")
        os.environ[root.ENV_VAR] = str(inner)
        self.addCleanup(os.environ.pop, root.ENV_VAR, None)
        self.assertEqual(root.find_root(self.tmp), inner.resolve())


class TestTheRedirectRemembersItsOrigin(PlaneCase):
    def test_it_names_the_plane_it_redirected_past(self):
        """Without this the change is invisible: once `find_root` answers with the outer,
        `enclosing_plane(config.ROOT)` is None by construction and every surface that used
        to name the nesting goes quiet."""
        self.plane("outer")
        inner = self.plane("outer", "workspaces", "w1", "repo")
        self.assertEqual(root.standing_in_nested_plane(inner), inner.resolve())

    def test_a_lone_plane_has_no_origin(self):
        solo = self.plane("solo")
        self.assertIsNone(root.standing_in_nested_plane(solo))

    def test_somewhere_with_no_plane_at_all_has_no_origin(self):
        bare = self.tmp / "bare"
        bare.mkdir()
        self.assertIsNone(root.standing_in_nested_plane(bare))

    def test_it_never_raises_on_a_hostile_path(self):
        """It runs on the status line's render path, which must degrade rather than fail."""
        self.assertIsNone(root.standing_in_nested_plane(self.tmp / "nope" / "gone"))


class TestPlanesAreNamedUnambiguously(PlaneCase):
    def test_a_bare_name_is_not_used_to_identify_a_plane(self):
        """The bug this fixes: the inner plane's directory is named `charter` and so is the
        outer's, so the old alert read "memory and vault go to charter, not charter" —
        which is why it told the operator nothing (#200). `.name` is not an identifier; it
        is a coincidence that usually holds."""
        outer = self.plane("charter")
        inner = self.plane("charter", "workspaces", "fleet", "charter")
        a, b = util.short_path(outer), util.short_path(inner)
        self.assertNotEqual(a, b)

    def test_a_path_under_home_is_shortened(self):
        self.assertTrue(util.short_path(Path.home() / "x" / "y").startswith("~/"))

    def test_a_path_outside_home_is_left_absolute(self):
        self.assertEqual(util.short_path(Path("/opt/thing")), "/opt/thing")


class TestTheDuplicateWrapperIsGone(unittest.TestCase):
    def test_the_render_path_has_one_wrapper_over_the_rule(self):
        """`_nested_under`'s own docstring warns that this rule "was implemented twice,
        here and there, which is how two surfaces come to disagree about what nested
        means". 0.36.0 added a third; this collapses it back."""
        from charter import statusline
        self.assertFalse(hasattr(statusline, "_nested_plane_mark"))


if __name__ == "__main__":
    unittest.main()
