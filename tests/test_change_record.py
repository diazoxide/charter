"""The cross-repo change record: a closed key set, contained names, derived ordering.

The record holds **intent only** — which repositories, which branch in each, which must
land first, which was excluded and why. Everything a test here pins is one of two claims:

* a value charter does not read must be **named**, not ignored (#503). ``need`` where
  ``needs`` was meant is an ordering constraint that silently ceased to exist, and a record
  charter half-understands is worse than one it refuses;
* a value the record carries reaches somewhere a string is not just a string — argv, a
  report row, a path — and the boundary that reads it is where that is answered.

The ordering half is the same claim about *state*: ``needs`` is declared, "blocked" is
derived, and the round-trip test is what says nobody cached the derivation on the way past.
"""
from __future__ import annotations

import json
import os
import unittest

from charter import change, config, contain, workspace
from tests._isolation import PersonaIso

GOOD = {
    "change": "component-api-2",
    "why": "component.API_VERSION 1 -> 2; providers declare the new integer",
    "created": "2026-08-28T09:14:02+00:00",
    "by": "Aaron Yordanyan",
    "members": [
        {"repo": "charter", "branch": "change/component-api-2", "needs": []},
        {"repo": "charter-metrics", "branch": "change/component-api-2", "needs": ["charter"]},
    ],
    "excluded": [
        {"repo": "charter-slack", "why": "no components; only an action provider",
         "at": "2026-08-28T09:20:11+00:00"},
    ],
}


class ChangeIso(PersonaIso):
    def setUp(self) -> None:
        super().setUp()
        workspace.ensure("ws")

    def put(self, slug: str, text: str) -> None:
        """Write raw bytes as a record, bypassing `change.write` — the hand edit, the older
        charter, the record that arrived in somebody else's commit."""
        d = change.changes_dir("ws")
        d.mkdir(parents=True, exist_ok=True)
        (d / f"{slug}.json").write_text(text)

    def put_record(self, slug: str, rec: dict) -> None:
        self.put(slug, json.dumps(rec))


class TestTheKeySetIsClosed(ChangeIso):
    def test_the_key_set_is_closed_and_an_unknown_key_is_named(self):
        rec = json.loads(json.dumps(GOOD))
        rec["members"][0]["need"] = ["web"]
        self.put_record("component-api-2", rec)
        with self.assertRaises(change.RecordError) as cm:
            change.read("ws", "component-api-2")
        self.assertIn("need", str(cm.exception))

    def test_a_top_level_unknown_key_is_named(self):
        rec = json.loads(json.dumps(GOOD))
        rec["owner"] = "someone"
        self.put_record("component-api-2", rec)
        with self.assertRaises(change.RecordError) as cm:
            change.read("ws", "component-api-2")
        self.assertIn("owner", str(cm.exception))

    def test_a_missing_key_is_named_too(self):
        rec = json.loads(json.dumps(GOOD))
        del rec["excluded"]
        self.put_record("component-api-2", rec)
        with self.assertRaises(change.RecordError) as cm:
            change.read("ws", "component-api-2")
        self.assertIn("excluded", str(cm.exception))

    def test_no_state_field_is_representable(self):
        """`state`, `landed`, `pr` and `ci` are all things git or the forge already knows.
        Each is tested by name rather than as a class, because the closed set is what makes
        them unrepresentable and a test over a loop would pass against a set that happened
        to allow the one nobody listed."""
        for key, value in (("state", "open"), ("landed", True), ("pr", 601), ("ci", "passed")):
            with self.subTest(key=key):
                rec = json.loads(json.dumps(GOOD))
                rec[key] = value
                self.put_record("component-api-2", rec)
                with self.assertRaises(change.RecordError) as cm:
                    change.read("ws", "component-api-2")
                self.assertIn(key, str(cm.exception))

    def test_a_per_member_state_field_is_refused_too(self):
        for key, value in (("state", "open"), ("landed", True), ("pr", 601), ("ci", "passed")):
            with self.subTest(key=key):
                rec = json.loads(json.dumps(GOOD))
                rec["members"][0][key] = value
                self.put_record("component-api-2", rec)
                with self.assertRaises(change.RecordError) as cm:
                    change.read("ws", "component-api-2")
                self.assertIn(key, str(cm.exception))

    def test_a_good_record_reads(self):
        """Positive control. Without it every assertion above would still pass against a
        `read` that refused everything."""
        self.put_record("component-api-2", GOOD)
        self.assertEqual(change.read("ws", "component-api-2"), GOOD)


class TestTheNameIsAName(ChangeIso):
    def test_a_slug_that_is_a_path_is_refused_rather_than_sanitised(self):
        for slug in ("..", "a/b", "a\\b", ".hidden", "-b", "x\x00y", ""):
            with self.subTest(slug=slug):
                with self.assertRaises(change.RecordError):
                    change.path_for("ws", slug)

    def test_a_traversing_slug_never_reaches_the_filesystem(self):
        """The refusal is about the string, not about what is on the disk — asking the
        filesystem would make a traversal succeed exactly when the target happens to
        exist."""
        outside = config.ROOT / "outside.json"
        outside.write_text(json.dumps(GOOD))
        with self.assertRaises(change.RecordError):
            change.read("ws", "../../outside")
        self.assertTrue(outside.exists())

    def test_a_record_that_disagrees_with_its_filename_is_refused(self):
        rec = json.loads(json.dumps(GOOD))
        rec["change"] = "something-else"
        self.put_record("component-api-2", rec)
        with self.assertRaises(change.RecordError) as cm:
            change.read("ws", "component-api-2")
        self.assertIn("something-else", str(cm.exception))


class TestBothRefusalsAreAsked(ChangeIso):
    """#336: `file_refusal` structurally cannot see the variant where the *directory* is
    the link — every file inside it is an ordinary regular file with nothing to object to.
    Only `dir_refusal`, which resolves, catches that one."""

    def test_a_record_that_is_a_link_out_of_the_plane_is_refused(self):
        target = self.tmp / "elsewhere.json"
        target.write_text(json.dumps(GOOD))
        d = change.changes_dir("ws")
        d.mkdir(parents=True, exist_ok=True)
        os.symlink(target, d / "component-api-2.json")
        with self.assertRaises(change.RecordError):
            change.read("ws", "component-api-2")

    def test_a_changes_directory_that_is_a_link_out_of_the_plane_is_refused(self):
        elsewhere = self.tmp / "elsewhere"
        elsewhere.mkdir()
        (elsewhere / "component-api-2.json").write_text(json.dumps(GOOD))
        os.symlink(elsewhere, change.changes_dir("ws"))
        with self.assertRaises(change.RecordError):
            change.read("ws", "component-api-2")

    def test_the_listing_refuses_a_linked_directory_and_says_so(self):
        elsewhere = self.tmp / "elsewhere"
        elsewhere.mkdir()
        (elsewhere / "component-api-2.json").write_text(json.dumps(GOOD))
        os.symlink(elsewhere, change.changes_dir("ws"))
        records, refused = change.all_for("ws")
        self.assertEqual(records, [])
        # The DIRECTORY is what was refused, named as such. Without the directory check the
        # listing still refuses every file inside it one at a time — same exit, same empty
        # `records`, and a report that blames five records for a link on their parent.
        self.assertEqual([slug for slug, _ in refused], [change.DIRNAME])


class TestAFailedReadRaises(ChangeIso):
    """A read that degraded to `{}` would make every read-modify-write a truncation.

    `onepassword._fields` returned `{}` for every non-zero `op item get`, a rate-limited
    vault reported "has no secrets" (#322), and the read-modify-write behind it piped back
    a template holding one key. `add` here is exactly that shape."""

    def test_unparseable_json_raises_rather_than_answering_an_empty_record(self):
        self.put("component-api-2", "{not json")
        with self.assertRaises(change.RecordError):
            change.read("ws", "component-api-2")

    def test_a_missing_record_raises(self):
        with self.assertRaises(change.RecordError):
            change.read("ws", "never-created")

    def test_an_unreadable_record_then_add_writes_nothing(self):
        from charter import commands_change

        self.put("component-api-2", "{not json")
        before = (change.changes_dir("ws") / "component-api-2.json").read_text()
        (workspace.workspace_dir("ws") / "api" / ".git").mkdir(parents=True)
        code = commands_change.cmd_change_add(_args(change="component-api-2", repo="api"))
        self.assertEqual(code, 1)
        after = (change.changes_dir("ws") / "component-api-2.json").read_text()
        self.assertEqual(before, after)


class TestARepoNameIsASegmentNotAWorkspaceName(ChangeIso):
    def test_dot_github_is_a_real_repository_and_is_accepted(self):
        """`workspace.valid_name` rejects a leading dot; `org/.github` is a real and common
        repository whose name comes from a forge rather than from charter. That distinction
        has already cost this project once."""
        rec = json.loads(json.dumps(GOOD))
        rec["members"] = [{"repo": ".github", "branch": "change/x", "needs": []}]
        rec["excluded"] = []
        self.put_record("component-api-2", rec)
        got = change.read("ws", "component-api-2")
        self.assertEqual(got["members"][0]["repo"], ".github")
        self.assertFalse(workspace.valid_name(".github"))   # the rule that would have failed

    def test_a_member_naming_a_path_is_refused(self):
        for repo in ("..", "a/b", "a\\b", "x\x00y", ""):
            with self.subTest(repo=repo):
                rec = json.loads(json.dumps(GOOD))
                rec["members"] = [{"repo": repo, "branch": "change/x", "needs": []}]
                rec["excluded"] = []
                self.put_record("component-api-2", rec)
                with self.assertRaises(change.RecordError):
                    change.read("ws", "component-api-2")

    def test_the_same_repo_twice_is_refused(self):
        rec = json.loads(json.dumps(GOOD))
        rec["members"] = [{"repo": "api", "branch": "a", "needs": []},
                          {"repo": "api", "branch": "b", "needs": []}]
        rec["excluded"] = []
        self.put_record("component-api-2", rec)
        with self.assertRaises(change.RecordError) as cm:
            change.read("ws", "component-api-2")
        self.assertIn("api", str(cm.exception))

    def test_a_member_that_is_also_excluded_is_refused(self):
        rec = json.loads(json.dumps(GOOD))
        rec["members"] = [{"repo": "api", "branch": "a", "needs": []}]
        rec["excluded"] = [{"repo": "api", "why": "no", "at": "2026-08-28T00:00:00+00:00"}]
        self.put_record("component-api-2", rec)
        with self.assertRaises(change.RecordError):
            change.read("ws", "component-api-2")


class TestABranchNameIsArgv(ChangeIso):
    """`git check-ref-format` **accepts** `refs/heads/-b` — measured on git 2.50.1 — so ref
    grammar proves nothing about the position this value lands in. The record boundary is
    one of the two mechanisms; `--` in the argv is the other, and neither substitutes."""

    def test_git_itself_accepts_the_ref_this_refuses(self):
        """The measurement the refusal rests on, taken rather than asserted. Skipped where
        git is absent, because then there is nothing to measure."""
        import shutil
        import subprocess
        if not shutil.which("git"):
            self.skipTest("no git")
        rc = subprocess.run(["git", "check-ref-format", "refs/heads/-b"],
                            capture_output=True).returncode
        self.assertEqual(rc, 0, "git no longer accepts refs/heads/-b — re-read the guard")

    def test_a_branch_that_reaches_git_as_a_flag_is_refused(self):
        for branch in ("-b", "--upload-pack=touch /tmp/x", "-"):
            with self.subTest(branch=branch):
                self.assertIsNotNone(change.branch_refusal(branch))

    def test_a_branch_that_is_not_one_line_is_refused(self):
        for branch in ("a\nb", "a\u2028b", "a\rb", "a\x1b[31mb",
                       "x" * (change.TEXT_LIMIT + 1)):
            with self.subTest(branch=repr(branch)):
                self.assertIsNotNone(change.branch_refusal(branch))

    def test_a_branch_a_little_longer_than_a_report_row_is_accepted(self):
        """The record's bound is `contain`'s PATH budget, not its ROW budget. Refusing a
        value for being one row long would send somebody to hand-edit the file; the row
        budget still applies where rows are drawn, and is measured there."""
        self.assertIsNone(change.branch_refusal("x" * (contain.DISPLAY_LIMIT + 20)))

    def test_an_ordinary_branch_is_accepted(self):
        self.assertIsNone(change.branch_refusal("change/component-api-2"))

    def test_the_refusal_is_reached_through_the_record(self):
        rec = json.loads(json.dumps(GOOD))
        rec["members"] = [{"repo": "api", "branch": "-b", "needs": []}]
        rec["excluded"] = []
        self.put_record("component-api-2", rec)
        with self.assertRaises(change.RecordError) as cm:
            change.read("ws", "component-api-2")
        self.assertIn("FLAG", str(cm.exception))


class TestWhyIsOneLineAndRequired(ChangeIso):
    def test_an_empty_why_is_refused(self):
        rec = json.loads(json.dumps(GOOD))
        rec["why"] = "   "
        self.put_record("component-api-2", rec)
        with self.assertRaises(change.RecordError) as cm:
            change.read("ws", "component-api-2")
        self.assertIn("why", str(cm.exception))

    def test_a_why_a_little_longer_than_a_report_row_is_kept_whole(self):
        rec = json.loads(json.dumps(GOOD))
        rec["why"] = "w" * (contain.DISPLAY_LIMIT + 40)
        change.write("ws", "component-api-2", rec)
        self.assertEqual(change.read("ws", "component-api-2")["why"], rec["why"])

    def test_a_why_longer_than_a_committed_value_may_be_is_refused(self):
        rec = json.loads(json.dumps(GOOD))
        rec["why"] = "w" * (change.TEXT_LIMIT + 1)
        self.put_record("component-api-2", rec)
        with self.assertRaises(change.RecordError):
            change.read("ws", "component-api-2")

    def test_a_why_that_cannot_be_one_line_is_refused(self):
        """It belongs in `workspace.md`, which is where this plane keeps prose. A newline
        in a value charter repeats back on a row writes a second row that looks exactly as
        much like charter's own output as the first."""
        rec = json.loads(json.dumps(GOOD))
        rec["why"] = "line one\n✓ everything landed"
        self.put_record("component-api-2", rec)
        with self.assertRaises(change.RecordError):
            change.read("ws", "component-api-2")

    def test_an_exclusion_needs_its_reason_too(self):
        rec = json.loads(json.dumps(GOOD))
        rec["excluded"][0]["why"] = ""
        self.put_record("component-api-2", rec)
        with self.assertRaises(change.RecordError):
            change.read("ws", "component-api-2")


class TestTheWriteSideValidatesToo(ChangeIso):
    """Without this, the closed key set is a rule only the reader enforces, and anything
    holding a record in memory can hang a derived value off it and serialise it."""

    def test_write_refuses_an_unknown_key(self):
        rec = json.loads(json.dumps(GOOD))
        rec["blocked"] = ["charter-metrics"]
        with self.assertRaises(change.RecordError) as cm:
            change.write("ws", "component-api-2", rec)
        self.assertIn("blocked", str(cm.exception))
        self.assertFalse(change.exists("ws", "component-api-2"))

    def test_write_refuses_a_derived_value_cached_on_a_member(self):
        rec = json.loads(json.dumps(GOOD))
        rec["members"][1]["blocked"] = True
        with self.assertRaises(change.RecordError) as cm:
            change.write("ws", "component-api-2", rec)
        self.assertIn("blocked", str(cm.exception))

    def test_a_record_read_and_written_back_is_byte_identical(self):
        """The round trip *is* the assertion that ADR 0011 has not been reversed by
        convenience: if anything derived were cached on the record on the way through, the
        bytes would differ."""
        change.write("ws", "component-api-2", json.loads(json.dumps(GOOD)))
        first = change.path_for("ws", "component-api-2").read_bytes()
        rec = change.read("ws", "component-api-2")
        change.blocked_members(rec, [])          # render it — the operation under suspicion
        change.write("ws", "component-api-2", rec)
        self.assertEqual(first, change.path_for("ws", "component-api-2").read_bytes())

    def test_the_bytes_are_two_space_indented_with_a_trailing_newline(self):
        change.write("ws", "component-api-2", json.loads(json.dumps(GOOD)))
        text = change.path_for("ws", "component-api-2").read_text()
        self.assertTrue(text.endswith("}\n"))
        self.assertIn('\n  "why": ', text)


class TestOrderingIsDeclaredAndDerived(ChangeIso):
    def _with_needs(self, graph: dict[str, list[str]]) -> dict:
        rec = json.loads(json.dumps(GOOD))
        rec["members"] = [{"repo": r, "branch": f"change/{r}", "needs": n}
                          for r, n in graph.items()]
        rec["excluded"] = []
        return rec

    def test_a_cycle_is_refused_at_write_time_naming_both_members(self):
        rec = self._with_needs({"api": ["web"], "web": ["api"]})
        with self.assertRaises(change.RecordError) as cm:
            change.write("ws", "component-api-2", rec)
        self.assertIn("api", str(cm.exception))
        self.assertIn("web", str(cm.exception))
        self.assertIn("cycle", str(cm.exception))

    def test_a_longer_cycle_names_every_member_in_it(self):
        rec = self._with_needs({"a": ["c"], "b": ["a"], "c": ["b"]})
        with self.assertRaises(change.RecordError) as cm:
            change.write("ws", "component-api-2", rec)
        for repo in ("a", "b", "c"):
            self.assertIn(f"'{repo}'", str(cm.exception))

    def test_a_member_cannot_block_itself(self):
        rec = self._with_needs({"api": ["api"]})
        with self.assertRaises(change.RecordError) as cm:
            change.write("ws", "component-api-2", rec)
        self.assertIn("its own blocker", str(cm.exception))

    def test_needs_may_only_name_a_member_of_this_change(self):
        rec = self._with_needs({"api": ["web"]})
        with self.assertRaises(change.RecordError) as cm:
            change.write("ws", "component-api-2", rec)
        self.assertIn("web", str(cm.exception))
        self.assertIn("not a member", str(cm.exception))

    def test_a_diamond_is_not_a_cycle(self):
        """Positive control: without it, "refuse everything" would pass every test above."""
        rec = self._with_needs({"base": [], "left": ["base"], "right": ["base"],
                                "top": ["left", "right"]})
        change.write("ws", "component-api-2", rec)
        self.assertTrue(change.exists("ws", "component-api-2"))

    def test_blocked_is_derived_from_the_declaration_and_a_landing_map(self):
        rec = self._with_needs({"api": [], "web": ["api"], "docs": ["web"]})
        self.assertEqual(change.blocked_members(rec, []), {"web": ["api"], "docs": ["web"]})
        self.assertEqual(change.blocked_members(rec, ["api"]), {"docs": ["web"]})
        self.assertEqual(change.blocked_members(rec, ["api", "web"]), {})

    def test_the_derivation_writes_nothing(self):
        rec = self._with_needs({"api": [], "web": ["api"]})
        change.write("ws", "component-api-2", rec)
        before = change.path_for("ws", "component-api-2").read_bytes()
        change.blocked_members(change.read("ws", "component-api-2"), [])
        self.assertEqual(before, change.path_for("ws", "component-api-2").read_bytes())
        self.assertNotIn("blocked", before.decode())

    def test_a_member_landed_with_an_unlanded_blocker_is_still_reported_blocked(self):
        """§3.2's out-of-order landing, which is `blocked & landed`. Computing `blocked`
        only for members that have NOT landed would make that intersection empty by
        construction and the divergence unnameable."""
        rec = self._with_needs({"api": [], "web": ["api"]})
        blocked = change.blocked_members(rec, ["web"])
        self.assertEqual(set(blocked) & {"web"}, {"web"})

    def test_the_blocker_is_named_not_merely_counted(self):
        rec = self._with_needs({"api": [], "b": [], "web": ["api", "b"]})
        self.assertEqual(change.blocked_members(rec, ["api"]), {"web": ["b"]})


class TestTheListing(ChangeIso):
    def test_records_and_refusals_come_back_separately(self):
        change.write("ws", "good-one", dict(GOOD, change="good-one"))
        self.put("bad-one", "{not json")
        records, refused = change.all_for("ws")
        self.assertEqual([r["change"] for r in records], ["good-one"])
        self.assertEqual([slug for slug, _ in refused], ["bad-one"])

    def test_a_workspace_with_no_changes_directory_lists_nothing_and_refuses_nothing(self):
        self.assertEqual(change.all_for("ws"), ([], []))

    def test_records_come_back_in_slug_order(self):
        """The listing IS the order — there is no rank field and nothing to disagree with
        the name, which is only true while the read is sorted."""
        for slug in ("zeta", "alpha", "middle"):
            change.write("ws", slug, dict(GOOD, change=slug))
        records, _ = change.all_for("ws")
        self.assertEqual([r["change"] for r in records], ["alpha", "middle", "zeta"])

    def test_an_emptied_changes_directory_holds_no_records(self):
        """`any`, not `all`: an empty directory satisfies `all` vacuously, and the answer
        that matters to `_ws_meta_paths` is "is there something git could untrack here"."""
        change.write("ws", "only-one", dict(GOOD, change="only-one"))
        change.forget("ws", "only-one")
        self.assertTrue(change.changes_dir("ws").exists())
        self.assertFalse(change.has_records("ws"))

    def test_a_directory_holding_a_record_and_something_else_still_holds_a_record(self):
        change.write("ws", "only-one", dict(GOOD, change="only-one"))
        (change.changes_dir("ws") / "README.txt").write_text("notes\n")
        self.assertTrue(change.has_records("ws"))

    def test_the_log_directory_is_not_a_record(self):
        change.write("ws", "good-one", dict(GOOD, change="good-one"))
        change.log_dir("ws").mkdir(parents=True)
        (change.log_dir("ws") / "host.jsonl").write_text('{"ts": "x"}\n')
        records, refused = change.all_for("ws")
        self.assertEqual([r["change"] for r in records], ["good-one"])
        self.assertEqual(refused, [])


class TestTheStoreIsCreatedLazily(ChangeIso):
    def test_scaffold_creates_no_changes_directory(self):
        """`_ws_meta_paths` filters by existence and that filter doubles as a
        non-emptiness filter — `git rm --cached` on a path with nothing tracked under it
        fails the whole call. An always-present, always-empty `changes/` breaks
        `charter workspace live --off` whole."""
        workspace.scaffold("ws")
        self.assertFalse(change.changes_dir("ws").exists())

    def test_the_first_write_creates_it(self):
        change.write("ws", "component-api-2", json.loads(json.dumps(GOOD)))
        self.assertTrue(change.changes_dir("ws").exists())

    def test_has_records_is_false_for_a_directory_holding_only_the_log(self):
        change.log_dir("ws").mkdir(parents=True)
        (change.log_dir("ws") / "host.jsonl").write_text("{}\n")
        self.assertTrue(change.changes_dir("ws").exists())
        self.assertFalse(change.has_records("ws"))

    def test_forget_removes_the_record_and_nothing_else(self):
        change.write("ws", "component-api-2", json.loads(json.dumps(GOOD)))
        change.log_dir("ws").mkdir(parents=True)
        log = change.log_dir("ws") / "host.jsonl"
        log.write_text('{"change": "component-api-2"}\n')
        change.forget("ws", "component-api-2")
        self.assertFalse(change.exists("ws", "component-api-2"))
        self.assertTrue(log.exists())


class TestTheShapeIsAskedNotAssumed(ChangeIso):
    """Every `isinstance` in `validate` is a guard, and a record arriving from a hand edit
    or an older charter is where the wrong shape comes from. Without these, a JSON list
    where an object belongs reaches the loop below it as an `AttributeError` — a traceback
    where the record was supposed to be named."""

    def test_a_record_that_is_not_an_object_is_refused(self):
        self.put("component-api-2", "[1, 2, 3]")
        with self.assertRaises(change.RecordError) as cm:
            change.read("ws", "component-api-2")
        self.assertIn("not an object", str(cm.exception))

    def test_members_must_be_a_list(self):
        rec = json.loads(json.dumps(GOOD))
        rec["members"] = {"repo": "charter"}
        self.put_record("component-api-2", rec)
        with self.assertRaises(change.RecordError) as cm:
            change.read("ws", "component-api-2")
        self.assertIn("'members' is not a list", str(cm.exception))

    def test_a_member_must_be_an_object(self):
        rec = json.loads(json.dumps(GOOD))
        rec["members"] = ["charter"]
        rec["excluded"] = []
        self.put_record("component-api-2", rec)
        with self.assertRaises(change.RecordError) as cm:
            change.read("ws", "component-api-2")
        self.assertIn("member is not an object", str(cm.exception))

    def test_needs_must_be_a_list(self):
        rec = json.loads(json.dumps(GOOD))
        rec["members"] = [{"repo": "charter", "branch": "b", "needs": "charter-metrics"}]
        rec["excluded"] = []
        self.put_record("component-api-2", rec)
        with self.assertRaises(change.RecordError) as cm:
            change.read("ws", "component-api-2")
        self.assertIn("'needs' is not a list", str(cm.exception))

    def test_excluded_must_be_a_list(self):
        rec = json.loads(json.dumps(GOOD))
        rec["excluded"] = {"repo": "charter-slack"}
        self.put_record("component-api-2", rec)
        with self.assertRaises(change.RecordError) as cm:
            change.read("ws", "component-api-2")
        self.assertIn("'excluded' is not a list", str(cm.exception))

    def test_an_exclusion_must_be_an_object(self):
        rec = json.loads(json.dumps(GOOD))
        rec["excluded"] = ["charter-slack"]
        self.put_record("component-api-2", rec)
        with self.assertRaises(change.RecordError) as cm:
            change.read("ws", "component-api-2")
        self.assertIn("exclusion is not an object", str(cm.exception))

    def test_a_why_that_is_not_a_string_is_refused(self):
        for value in (12, None, ["a"], {"a": 1}):
            with self.subTest(value=value):
                rec = json.loads(json.dumps(GOOD))
                rec["why"] = value
                self.put_record("component-api-2", rec)
                with self.assertRaises(change.RecordError):
                    change.read("ws", "component-api-2")

    def test_a_branch_that_is_not_a_non_empty_string_is_refused(self):
        for branch in (None, "", 3, ["x"], {}):
            with self.subTest(branch=branch):
                self.assertIsNotNone(change.branch_refusal(branch))

    def test_a_record_charter_may_not_open_raises_rather_than_answering_empty(self):
        """`file_refusal` asks one `lstat` — is this a link, is it a regular file, how big
        is it — and a file the process may not READ passes all three. The `OSError` handler
        is what keeps that arriving as a named refusal instead of a traceback, and it is
        the same "never answers `{}`" property one layer down."""
        if os.geteuid() == 0:
            self.skipTest("root reads anything, so there is nothing to measure")
        change.write("ws", "component-api-2", json.loads(json.dumps(GOOD)))
        p = change.path_for("ws", "component-api-2")
        p.chmod(0o000)
        self.addCleanup(p.chmod, 0o600)
        with self.assertRaises(change.RecordError) as cm:
            change.read("ws", "component-api-2")
        self.assertIn("cannot be read", str(cm.exception))


class TestEveryRefusalIsOneLineWhateverItNames(ChangeIso):
    """A refusal is a report line **about** a value charter would not accept — and that
    value is exactly the one that must not be able to write a second line into the report.

    Asked as a property over every refusing entry point rather than site by site, because
    the containment call at each site is invisible in its own output: a test that only
    checks `assertRaises` stays green with every `contain.readable` deleted, which is how
    ninety of them come to be a comment with a runtime cost.
    """

    ESC = "\x1b"
    HOSTILE = "ok\n  charter  branch main  landed"

    def _refusal(self, fn, *a) -> str:
        with self.assertRaises(change.RecordError) as cm:
            fn(*a)
        return str(cm.exception)

    def _messages(self) -> list[str]:
        out = [
            self._refusal(change.path_for, "ws", self.HOSTILE),
            self._refusal(change.path_for, "ws", f"api{self.ESC}[2K"),
            # `write` validates BEFORE it resolves the path, so the slug reaching these
            # messages has been through nothing at all.
            self._refusal(change.write, "ws", self.HOSTILE, json.loads(json.dumps(GOOD))),
            self._refusal(change.validate, json.loads(json.dumps(GOOD)), self.HOSTILE),
        ]
        hostile_name = json.loads(json.dumps(GOOD))
        hostile_name["change"] = self.HOSTILE
        out.append(self._refusal(change.validate, hostile_name, "component-api-2"))
        for field, value in (("repo", f"api{self.ESC}[2K/../x"),
                             ("branch", f"b{self.ESC}[2K"),
                             ("needs", [f"api{self.ESC}[2K/x"])):
            rec = json.loads(json.dumps(GOOD))
            rec["members"] = [{"repo": "api", "branch": "b", "needs": []}]
            rec["excluded"] = []
            rec["members"][0][field] = value
            out.append(self._refusal(change.validate, rec, "component-api-2"))
        rec = json.loads(json.dumps(GOOD))
        rec["excluded"] = [{"repo": f"x{self.ESC}[2K/y", "why": "w", "at": "t"}]
        out.append(self._refusal(change.validate, rec, "component-api-2"))
        rec = json.loads(json.dumps(GOOD))
        rec[f"sneaky{self.ESC}[2K"] = 1
        out.append(self._refusal(change.validate, rec, "component-api-2"))
        for key in ("why", "created", "by"):
            rec = json.loads(json.dumps(GOOD))
            rec[key] = self.HOSTILE
            out.append(self._refusal(change.validate, rec, "component-api-2"))
        rec = json.loads(json.dumps(GOOD))
        rec["excluded"] = [{"repo": "x", "why": self.HOSTILE, "at": "t"}]
        out.append(self._refusal(change.validate, rec, "component-api-2"))
        return out

    def test_a_branch_refusal_cannot_forge_a_second_line_either(self):
        """`branch_refusal` answers a sentence rather than raising, and the leading-dash
        arm is reached BEFORE the one-line arm — so that message carries a value nothing
        else has looked at yet."""
        for branch in (f"-b\n  charter  branch main  landed", f"-{self.ESC}[2K"):
            with self.subTest(branch=branch):
                msg = change.branch_refusal(branch)
                self.assertEqual(len(msg.splitlines()), 1, msg)
                self.assertNotIn(self.ESC, msg)

    def test_the_slug_is_asked_about_before_anything_else_is(self):
        """The words, not the exit. With `validate`'s own slug check deleted a hostile slug
        is still refused — by `path_for`, one line later in `write` — but only after every
        sentence in `validate` has already named it, and those sentences name it plainly."""
        with self.assertRaises(change.RecordError) as cm:
            change.write("ws", self.HOSTILE, json.loads(json.dumps(GOOD)))
        self.assertIn("is not a change name", str(cm.exception))
        self.assertNotIn("calls itself", str(cm.exception))

    def test_there_are_refusals_to_measure(self):
        """The control. A helper that raised on its own first line would make every
        assertion below pass over an empty list."""
        self.assertGreaterEqual(len(self._messages()), 8)

    def test_no_refusal_can_forge_a_second_line(self):
        for msg in self._messages():
            with self.subTest(msg=msg[:60]):
                self.assertEqual(len(msg.splitlines()), 1, msg)

    def test_no_refusal_carries_an_escape_through(self):
        for msg in self._messages():
            with self.subTest(msg=msg[:60]):
                self.assertNotIn(self.ESC, msg)


def _args(**kw):
    from types import SimpleNamespace
    kw.setdefault("workspace", "ws")
    kw.setdefault("branch", None)
    kw.setdefault("needs", None)
    kw.setdefault("why", None)
    return SimpleNamespace(**kw)


if __name__ == "__main__":
    unittest.main()
