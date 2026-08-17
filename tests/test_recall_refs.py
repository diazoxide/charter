"""`recall` searches `refs/` — the curated docs a persona collects (#178).

`recall` calls itself the one memory gate, "search/list across ALL bases". It searched
memory and not `refs/`, which on the reporting plane was **the larger half of the corpus**:
46 docs and 6,153 lines of refs against 148 files and 4,792 lines of memory. An agent asking
charter for a runbook that exists was told, with a straight face, that it does not.

Two shape differences make this more than wiring, and both are covered here.

**Refs nest.** The reporter's own example is `refs/release/keycloak-prerequisites.md`, while
`memstore.files` is a flat `*.md` glob because memory is flat by construction. Rather than
teach the memory engine to recurse — and change what every other base searches — each
subdirectory is offered as its own source, since `memstore.search` already takes a list.

**Refs carry no date.** `--since` filters on a recorded date, so it can only exclude them.
That exclusion is reported *separately* from undated memories: an undated memory lost a stamp
it was meant to carry, a refs document never had one, and blaming a runbook for a missing
date would read as corruption rather than as the filter not applying.
"""

from __future__ import annotations

import datetime
import unittest

from charter import config, persona, recall, workspace
from tests._isolation import PersonaIso


class RefsCase(PersonaIso):
    def setUp(self) -> None:
        super().setUp()
        workspace.ensure("w")
        workspace.scaffold("w")
        self.make_persona("dev", role="Dev", vault="none")

    def ref(self, rel: str, body: str, name="dev", shared=False):
        base = persona.refs_dir(config.SHARED_PERSONA if shared else name, shared=shared)
        p = base / rel
        p.parent.mkdir(parents=True, exist_ok=True)
        p.write_text(body)
        return p

    def find(self, query, **kw):
        return recall.recall(query, persona_name="dev", workspace_name="w", **kw)


class TestRefsAreSearched(RefsCase):
    def test_a_ref_doc_is_found(self):
        self.ref("keycloak.md", "# Keycloak\n\nGate 5: the realm-management roles.\n")
        hits = self.find("keycloak")
        self.assertTrue(any("keycloak" in h.path.name for h in hits.hits), hits.hits)

    def test_a_NESTED_ref_doc_is_found(self):
        """The reported path shape. A flat glob would miss it, and missing it is the whole
        issue — the runbook that answers the question lives in a subdirectory."""
        self.ref("release/keycloak-prerequisites.md",
                 "# Prerequisites\n\nGate 5 records the realm-management roles fix.\n")
        hits = self.find("keycloak prerequisites")
        self.assertTrue(any("keycloak-prerequisites" in h.path.name for h in hits.hits),
                        hits.hits)

    def test_shared_refs_are_searched_too(self):
        self.ref("conventions.md", "# Conventions\n\nthe canonical grafana dashboard.\n",
                 shared=True)
        self.assertTrue(any("conventions" in h.path.name for h in self.find("grafana").hits))

    def test_a_refs_hit_is_labelled_as_refs(self):
        """Long documents can outrank short memories on raw term frequency, so a reader has
        to be able to see WHY something is at the top."""
        self.ref("keycloak.md", "# Keycloak\n\nrealm gates.\n")
        hits = [h for h in self.find("keycloak").hits if "keycloak" in h.path.name]
        self.assertTrue(hits)
        self.assertTrue(hits[0].label.startswith("refs"), hits[0].label)


class TestItIsOnByDefault(RefsCase):
    def test_refs_is_in_the_default_scopes(self):
        """Opt-in would fix the letter of the issue and not its substance: an agent that has
        to know to ask for refs first is in exactly the position the report describes."""
        self.assertIn("refs", recall.DEFAULT_SCOPES)

    def test_ephemeral_is_still_opt_in(self):
        """The distinction is standing, not size — refs are committed and shared, scratch is
        not."""
        self.assertNotIn("ephemeral", recall.DEFAULT_SCOPES)

    def test_narrowing_to_persona_excludes_refs(self):
        """The reason refs got their own scope rather than folding into `persona`: someone
        narrowing scope usually wants exactly this."""
        self.ref("keycloak.md", "# Keycloak\n\nrealm gates.\n")
        hits = self.find("keycloak", scopes=("persona",))
        self.assertFalse(any(h.label.startswith("refs") for h in hits.hits))


class TestTheSinceFilter(RefsCase):
    def test_since_excludes_refs(self):
        self.ref("keycloak.md", "# Keycloak\n\nrealm gates.\n")
        hits = self.find("keycloak", since=datetime.date(2000, 1, 1))
        self.assertFalse(any(h.label.startswith("refs") for h in hits.hits))

    def test_the_exclusion_is_counted_separately_from_undated_memories(self):
        """An undated memory lost a stamp it was meant to carry; a refs doc never had one.
        Reporting them as one number would blame the runbook for corruption."""
        self.ref("keycloak.md", "# Keycloak\n\nrealm gates.\n")
        got = self.find("keycloak", since=datetime.date(2000, 1, 1))
        self.assertEqual(got.undated_refs, 1)
        self.assertEqual(got.undated, 0)

    def test_positional_unpacking_of_the_first_two_still_works(self):
        """`Recalled`'s own docstring records what happened the last time a field was added
        — every positional unpack in the codebase broke at once."""
        hits, undated = self.find("keycloak")[:2]
        self.assertIsInstance(undated, int)


class TestListingWithoutAQuery(RefsCase):
    def test_refs_are_listed_too(self):
        self.ref("keycloak.md", "# Keycloak\n\nrealm gates.\n")
        got = recall.recall(None, persona_name="dev", workspace_name="w", limit=0)
        self.assertTrue(any(h.label.startswith("refs") for h in got.hits))

    def test_undated_refs_sort_after_dated_memories(self):
        """"List everything" must not silently omit the larger half — but a document with no
        date has not earned a place above a memory that has one."""
        persona.remember("dev", "a dated memory about keycloak")
        self.ref("keycloak.md", "# Keycloak\n\nrealm gates.\n")
        got = recall.recall(None, persona_name="dev", workspace_name="w", limit=0)
        labels = [h.label for h in got.hits]
        self.assertTrue(labels)
        self.assertTrue(labels[-1].startswith("refs"), labels)


if __name__ == "__main__":
    unittest.main()
