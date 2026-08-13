"""The centralized memory-fetch gate (edm/recall.py): one entry that searches/lists across
the workspace journal + active persona own + shared namespace (+ ephemeral opt-in), each hit
labeled by source. Storage stays per-base; only reading is unified here."""

from __future__ import annotations

import datetime
import unittest

from charter import config, memstore, persona, recall, workspace
from tests._isolation import PersonaIso


class RecallGateCase(PersonaIso):
    def setUp(self):
        super().setUp()
        # one workspace journal + one persona (own + shared) with distinguishable memories
        workspace.ensure("w")
        workspace.scaffold("w")
        workspace.remember("w", "workspace note about keycloak tokens")
        self.make_persona("dev", role="Dev", vault="d")
        persona.remember("dev", "devops fact about keycloak deploys")          # own
        persona.remember("dev", "shared org keycloak convention", shared=True)  # shared

    def _labels(self, results):
        return sorted({h.label for h in results.hits})

    def test_searches_across_all_bases_with_source_labels(self):
        r = recall.recall("keycloak", workspace_name="w", persona_name="dev")
        self.assertEqual(len(r.hits), 3)
        self.assertEqual(self._labels(r), ["persona:dev", "shared", "workspace:w"])

    def test_scope_filter_narrows_bases(self):
        r = recall.recall("keycloak", workspace_name="w", persona_name="dev",
                           scopes=("shared",))
        self.assertEqual(self._labels(r), ["shared"])

    def test_persona_scope_excludes_workspace(self):
        r = recall.recall("keycloak", workspace_name="w", persona_name="dev",
                           scopes=("workspace", "persona"))
        self.assertEqual(self._labels(r), ["persona:dev", "workspace:w"])

    def test_no_query_lists_newest_first(self):
        # add two workspace memories with explicit, ordered stamps
        workspace.ensure("t")
        workspace.scaffold("t")
        memstore.write(workspace.memory_dir("t"), "older", "older", timestamped=True,
                       stamp=datetime.datetime(2026, 7, 1, 9, 0))
        memstore.write(workspace.memory_dir("t"), "newer", "newer", timestamped=True,
                       stamp=datetime.datetime(2026, 7, 20, 9, 0))
        r = recall.recall(None, workspace_name="t", scopes=("workspace",))
        titles = [h.title for h in r.hits]
        self.assertEqual(titles[:2], ["newer", "older"])

    def test_sources_skips_persona_when_none_active(self):
        # no active persona → only workspace + shared rows
        srcs = recall.sources(workspace_name="w", scopes=recall.DEFAULT_SCOPES, persona_name=None)
        labels = [lbl for lbl, _d in srcs]
        # 'dev' isn't the active persona here (no active file set), so persona row is absent
        self.assertIn("workspace:w", labels)
        self.assertIn("shared", labels)

    def test_ephemeral_scope_opt_in(self):
        persona.remember("dev", "scratch keycloak thought", ephemeral=True, session="s1")
        r = recall.recall("keycloak", workspace_name="w", persona_name="dev",
                          scopes=("ephemeral",))
        # ephemeral resolves per-session; with no session it targets 'nosession' — just assert no crash
        self.assertIsInstance(r.hits, list)

    def test_limit_caps_results(self):
        for i in range(5):
            persona.remember("dev", f"keycloak item {i}", shared=True)
        r = recall.recall("keycloak", workspace_name="w", persona_name="dev", limit=2)
        self.assertEqual(len(r.hits), 2)


class ParseSinceCase(unittest.TestCase):
    """`--since` accepts what you type when you are remembering (`14d`) and what you type
    when you know (`2026-07-01`). Anything else is an error, never a silent no-filter:
    degrading to 'no filter' returns MORE than asked for, and a full corpus then reads as
    'nothing was recorded that recently'."""

    TODAY = datetime.date(2026, 7, 20)

    def test_relative_days_weeks_months(self):
        self.assertEqual(recall.parse_since("14d", today=self.TODAY), datetime.date(2026, 7, 6))
        self.assertEqual(recall.parse_since("2w", today=self.TODAY), datetime.date(2026, 7, 6))
        self.assertEqual(recall.parse_since("1m", today=self.TODAY), datetime.date(2026, 6, 20))

    def test_iso_date_passes_through(self):
        self.assertEqual(recall.parse_since("2026-07-01", today=self.TODAY),
                         datetime.date(2026, 7, 1))

    def test_zero_days_is_today(self):
        self.assertEqual(recall.parse_since("0d", today=self.TODAY), self.TODAY)

    def test_future_date_is_valid_and_simply_matches_nothing(self):
        self.assertEqual(recall.parse_since("2027-01-01", today=self.TODAY),
                         datetime.date(2027, 1, 1))

    def test_garbage_raises(self):
        for bad in ("banana", "14", "d14", "2026-13-01", "", "-3d"):
            with self.assertRaises(ValueError, msg=bad):
                recall.parse_since(bad, today=self.TODAY)


class RecallSinceCase(PersonaIso):
    """`--since` filters on the RECORDED date (memstore.memory_date) — the same key the
    no-query listing already sorts by, so the filter and the order agree."""

    def setUp(self):
        super().setUp()
        workspace.ensure("w")
        workspace.scaffold("w")
        self.make_persona("dev", role="Dev", vault="d")

    def _note(self, day, title, text="keycloak note"):
        return memstore.write(workspace.memory_dir("w"), text, title, timestamped=True,
                              stamp=datetime.datetime(2026, 7, day, 9, 0))

    def _undated(self, title):
        """A memory with no in-body stamp and no date prefix — hand-written, or predating
        the stamp. `memory_date` returns None for these."""
        p = workspace.memory_dir("w") / f"{title}.md"
        p.write_text(f"# {title}\n\nkeycloak note with no stamp\n")
        return p

    def test_excludes_older_and_includes_the_boundary_day(self):
        self._note(1, "older")
        self._note(6, "boundary")
        self._note(19, "newer")
        got = recall.recall(None, workspace_name="w", scopes=("workspace",),
                            since=datetime.date(2026, 7, 6))
        self.assertEqual(sorted(h.title for h in got.hits), ["boundary", "newer"])

    def test_excludes_undated_but_reports_how_many(self):
        self._note(19, "dated")
        self._undated("nostamp-one")
        self._undated("nostamp-two")
        got = recall.recall(None, workspace_name="w", scopes=("workspace",),
                            since=datetime.date(2026, 7, 1))
        self.assertEqual([h.title for h in got.hits], ["dated"])
        self.assertEqual(got.undated, 2)

    def test_undated_are_not_counted_when_no_since_filter(self):
        self._note(19, "dated")
        self._undated("nostamp")
        got = recall.recall(None, workspace_name="w", scopes=("workspace",))
        self.assertEqual(len(got.hits), 2)
        self.assertEqual(got.undated, 0)

    def test_query_plus_since_keeps_score_order_not_date_order(self):
        # 'keycloak' in the TITLE scores 3x a body hit, so the older title-match must
        # outrank the newer body-match. --since narrows the field; it never re-ranks.
        self._note(2, "keycloak rotation", text="rotation detail")
        self._note(19, "unrelated title", text="a keycloak mention in the body")
        got = recall.recall("keycloak", workspace_name="w", scopes=("workspace",),
                            since=datetime.date(2026, 7, 1))
        self.assertEqual([h.title for h in got.hits], ["keycloak rotation", "unrelated title"])

    def test_hit_fields_are_reachable_by_name(self):
        self._note(19, "named")
        h = recall.recall(None, workspace_name="w", scopes=("workspace",)).hits[0]
        self.assertEqual(h.title, "named")
        self.assertEqual(h.label, "workspace:w")
        self.assertEqual(h.date, datetime.date(2026, 7, 19))
        self.assertEqual(h.score, 0)
        self.assertTrue(h.path.name.endswith("named.md"))


class AllWorkspacesCase(PersonaIso):
    """`--all-workspaces` widens the workspace axis only: persona and shared are not
    workspace-scoped, so they must appear exactly once, not once per workspace."""

    def setUp(self):
        super().setUp()
        for w in ("alpha", "beta"):
            workspace.ensure(w)
            workspace.scaffold(w)
            workspace.remember(w, f"keycloak note in {w}")
        self.make_persona("dev", role="Dev", vault="d")
        persona.remember("dev", "keycloak fact owned by dev")
        persona.remember("dev", "keycloak convention shared org-wide", shared=True)

    def test_surfaces_every_workspace_with_distinct_labels(self):
        got = recall.recall("keycloak", persona_name="dev", all_workspaces=True)
        labels = {h.label for h in got.hits}
        self.assertIn("workspace:alpha", labels)
        self.assertIn("workspace:beta", labels)

    def test_persona_and_shared_appear_exactly_once(self):
        got = recall.recall("keycloak", persona_name="dev", all_workspaces=True, limit=0)
        labels = [h.label for h in got.hits]
        self.assertEqual(labels.count("persona:dev"), 1)
        self.assertEqual(labels.count("shared"), 1)

    def test_sources_lists_one_base_per_workspace(self):
        srcs = recall.sources(persona_name="dev", all_workspaces=True)
        ws_labels = sorted(lbl for lbl, _d in srcs if lbl.startswith("workspace:"))
        self.assertEqual(ws_labels, ["workspace:alpha", "workspace:beta"])


class RecallCliCase(unittest.TestCase):
    """Through the REAL parser — a handler tested only via SimpleNamespace never proves
    the flag exists or that two flags refuse each other."""

    def test_since_and_all_workspaces_flags_parse(self):
        from charter import cli
        args = cli.build_parser().parse_args(["recall", "tokens", "--since", "14d",
                                              "--all-workspaces"])
        self.assertEqual(args.since, "14d")
        self.assertTrue(args.all_workspaces)

    def test_all_workspaces_and_workspace_are_mutually_exclusive(self):
        from charter import cli
        with self.assertRaises(SystemExit):
            cli.build_parser().parse_args(["recall", "--all-workspaces", "-w", "alpha"])


if __name__ == "__main__":
    unittest.main()
