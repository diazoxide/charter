"""`charter init` scaffolds a front door — a generic one, that the plane then owns.

`init` used to create zero personas, so every fresh plane started with no identity, no
routing, and no example of what a persona looks like. The fix is not for charter to know
about a persona called `steward`: it is for `init` to write one FILE the consumer can
rename, rewrite or delete, and to declare it in `charter.toml`. charter's own code still
knows only *that* a plane may declare a default, never which (#255, #256).

The template's prose matters as much as its frontmatter. A fresh plane has nobody to route
to, so a front door that reads as though delegating were optional would ship the exact
failure this design exists to fix — pre-installed, in the file everyone copies from.
"""
from __future__ import annotations

import tempfile
import unittest
from pathlib import Path
from types import SimpleNamespace
from unittest import mock

from charter import commands, config, instance


class InitFrontDoorIso(unittest.TestCase):
    def setUp(self) -> None:
        self.root = Path(tempfile.mkdtemp(prefix="charter-fd-")).resolve()

    def _init(self, **kw):
        args = SimpleNamespace(forge=kw.get("forge", "github"),
                               owner=kw.get("owner", "acme"),
                               host=None,
                               front_door=kw.get("front_door", "steward"))
        with mock.patch.object(config, "ROOT", self.root):
            return commands.cmd_init(args)

    def charter_of(self, name: str) -> str:
        return (self.root / "personas" / name / "persona.md").read_text()

    def declared(self) -> str | None:
        return instance.default_persona_of(instance.load(self.root))


class TestItScaffoldsOne(InitFrontDoorIso):
    def test_a_fresh_plane_gets_a_front_door_persona(self):
        self._init()
        self.assertTrue((self.root / "personas" / "steward" / "persona.md").is_file())

    def test_and_the_plane_declares_it(self):
        """A generated persona nobody declares is a file, not a front door."""
        self._init()
        self.assertEqual(self.declared(), "steward")

    def test_the_name_comes_from_the_flag(self):
        self._init(front_door="ops")
        self.assertTrue((self.root / "personas" / "ops" / "persona.md").is_file())
        self.assertEqual(self.declared(), "ops")

    def test_it_can_be_declined(self):
        self._init(front_door=None)
        self.assertEqual(list((self.root / "personas").glob("*/persona.md")), [])
        self.assertIsNone(self.declared())

    def test_it_carries_the_advise_posture(self):
        """The default posture arrives in a file the consumer owns, never as a constant
        inside charter — that is the whole reason there is no plane-level routing setting."""
        self._init()
        self.assertIn("routing: advise", self.charter_of("steward"))

    def test_it_is_dispatchable_not_a_draft(self):
        """`persona create` marks a stub `draft: true` because it is unfinished. This one
        is complete for its purpose and is adopted by the very next session."""
        self._init()
        self.assertNotIn("draft: true", self.charter_of("steward"))

    def test_it_declares_when_work_should_be_routed_to_it(self):
        self._init()
        self.assertIn("delegate-when:", self.charter_of("steward"))


class TestTheTemplateIsGeneric(InitFrontDoorIso):
    def test_it_names_none_of_charters_own_personas(self):
        """The charter in this repo routes to `statusline`, `release` and `forge`. Shipping
        that into a stranger's plane hands them our furniture."""
        self._init()
        text = self.charter_of("steward").lower()
        for ours in ("statusline", "release", "forge", "plane shape"):
            self.assertNotIn(ours, text)

    def test_it_says_there_is_nobody_to_route_to_yet(self):
        """A fresh plane has a roster of one. The template has to say so, or it reads as a
        persona whose job is to do the work itself."""
        self._init()
        text = self.charter_of("steward").lower()
        self.assertIn("charter persona create", text)

    def test_it_uses_the_name_it_was_given(self):
        self._init(front_door="ops")
        text = self.charter_of("ops")
        self.assertIn("ops", text)
        self.assertNotIn("steward", text)


class TestItStaysAdditive(InitFrontDoorIso):
    def test_an_existing_persona_of_that_name_is_untouched(self):
        d = self.root / "personas" / "steward"
        d.mkdir(parents=True)
        (d / "persona.md").write_text("---\nname: steward\nrole: Mine\n---\n\nMy own words.\n")
        self._init()
        self.assertIn("My own words.", self.charter_of("steward"))

    def test_a_plane_that_already_has_personas_gets_no_new_one(self):
        """`init` creates only what is absent. A roster is not absent because one
        particular name is."""
        d = self.root / "personas" / "ops"
        d.mkdir(parents=True)
        (d / "persona.md").write_text("---\nname: ops\nrole: Ops\n---\n\nbody\n")
        self._init()
        self.assertFalse((self.root / "personas" / "steward").exists())

    def test_an_existing_declaration_is_not_overwritten(self):
        self.root.mkdir(parents=True, exist_ok=True)
        (self.root / "charter.toml").write_text(
            'schema = 1\n\n[persona]\ndefault = "chosen"\n')
        self._init()
        self.assertEqual(self.declared(), "chosen")

    def test_running_it_twice_changes_nothing(self):
        self._init()
        first = self.charter_of("steward")
        self._init()
        self.assertEqual(self.charter_of("steward"), first)
        self.assertEqual(self.declared(), "steward")


if __name__ == "__main__":
    unittest.main()
