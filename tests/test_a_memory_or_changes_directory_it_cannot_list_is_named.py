"""A memory directory or a `changes/` charter cannot list is named, in the one sentence #1081 gave
what could not be checked, and never read as empty (#1084, ADR 0009).

`memstore.files` globbed a memory directory, and `Path.glob` answers an empty list for one it may
not read, on every interpreter. So `charter recall` searched a `memory/` at mode 000 as if it held
nothing and said so: "No memories match". `change.all_for` read a `changes/` it could not list as
no changes, and `charter change list` said "No changes in workspace". A search that did not look
read the same as a search that found nothing.

The fixtures are real refusals — a directory at mode 000 or 333 beside a readable one, restored in
cleanup — plus the refusal injected at both calls `Path.iterdir` makes on 3.11–3.14, so each is
pinned on whichever interpreter runs, as root too. Every sentence is spelled out by hand, not
rebuilt from the function that prints it.
"""

from __future__ import annotations

import contextlib
import errno
import io
import json
import os
import unittest
from contextlib import redirect_stderr, redirect_stdout
from pathlib import Path
from types import SimpleNamespace
from unittest import mock

from charter import (change, cli, commands, commands_change, commands_persona, commands_workspace,
                     config, curate, doctor, hooks, memstore, persona, recall, todos, workspace)
from tests._isolation import PersonaIso, make_plane
from tests.test_a_workspace_listing_names_what_it_cannot_look_at import (BOTH_INTERPRETERS,
                                                                         unsearchable)

AS_ROOT = os.geteuid() == 0


def sentence(rel: str) -> str:
    return f"{rel} cannot be checked — restoring read access to it clears this."


def locked(case, d: Path, mode: int = 0o000) -> Path:
    """*d* at *mode*, put back to 0755 in cleanup (before the tmp tree is removed)."""
    d.chmod(mode)
    case.addCleanup(d.chmod, 0o755)
    return d


def refusing_to_list(directory: Path):
    """*directory* refusing to be LISTED at both calls `Path.iterdir` makes on 3.11–3.14, while
    `stat` of it still answers — mode 333's shape, and a refusal root cannot walk past."""
    real_listdir, real_scandir = os.listdir, os.scandir

    def refusing(real):
        def call(p=".", *args, **kwargs):
            if isinstance(p, (str, os.PathLike)) and Path(p) == directory:
                raise PermissionError(errno.EACCES, os.strerror(errno.EACCES), str(p))
            return real(p, *args, **kwargs)
        return call

    stack = contextlib.ExitStack()
    stack.enter_context(mock.patch.object(os, "listdir", refusing(real_listdir)))
    stack.enter_context(mock.patch.object(os, "scandir", refusing(real_scandir)))
    return stack


def run(fn, **kw) -> tuple[int, str, str]:
    out, err = io.StringIO(), io.StringIO()
    with redirect_stdout(out), redirect_stderr(err):
        rc = fn(SimpleNamespace(**kw))
    return rc, out.getvalue(), err.getvalue()


class TwoMemoryDirectories(PersonaIso):
    """`workspaces/alpha/memory` and `workspaces/beta/memory`, one keycloak memory each."""

    def setUp(self) -> None:
        super().setUp()
        make_plane(self)
        for ws in ("alpha", "beta"):
            workspace.ensure(ws)
            workspace.scaffold(ws)
            memstore.write(workspace.memory_dir(ws), f"the keycloak token in {ws} rotates yearly",
                           title="keycloak token policy", timestamped=True)
        self.alpha = workspace.memory_dir("alpha")
        self.beta = workspace.memory_dir("beta")
        self.alpha_file = memstore.files(self.alpha)[0]


class TheStoreNamesWhatItCouldNotList(TwoMemoryDirectories):
    @unittest.skipIf(AS_ROOT, "root lists a directory whatever its mode")
    def test_a_real_directory_at_mode_000_or_333_is_unread_not_empty(self):
        for mode in (0o000, 0o333):
            with self.subTest(mode=oct(mode)):
                locked(self, self.alpha, mode)
                got = memstore.read_files(self.alpha)
                self.alpha.chmod(0o755)
                self.assertEqual(got, ([], [(self.alpha, errno.EACCES)]))

    def test_an_injected_refusal_to_list_is_unread_on_either_interpreter(self):
        with refusing_to_list(self.alpha):
            self.assertEqual(memstore.read_files(self.alpha), ([], [(self.alpha, errno.EACCES)]))

    def test_files_refuses_in_the_shared_sentence_rather_than_answering_none(self):
        with refusing_to_list(self.alpha):
            with self.assertRaises(workspace.CannotCheck) as cm:
                memstore.files(self.alpha)
        self.assertIsInstance(cm.exception, OSError)
        self.assertEqual(str(cm.exception), sentence("workspaces/alpha/memory"))
        self.assertEqual(cm.exception.unread, [(self.alpha, errno.EACCES)])

    def test_search_names_it_and_still_searches_the_rest(self):
        unread: list = []
        with refusing_to_list(self.alpha):
            hits = memstore.search([self.alpha, self.beta], "keycloak", unread=unread)
        self.assertEqual([p.parent for p, _t, _s in hits], [self.beta])
        self.assertEqual(unread, [(self.alpha, errno.EACCES)])

    def test_search_with_nowhere_to_name_it_refuses(self):
        with refusing_to_list(self.alpha):
            with self.assertRaises(workspace.CannotCheck):
                memstore.search([self.alpha, self.beta], "keycloak")
            with self.assertRaises(workspace.CannotCheck):
                memstore.duplicates([self.beta, self.alpha])

    def test_duplicates_names_it_and_still_compares_the_rest(self):
        memstore.write(self.beta, "the keycloak token in beta rotates yearly", title="again",
                       timestamped=True)
        unread: list = []
        with refusing_to_list(self.alpha):
            pairs = memstore.duplicates([self.alpha, self.beta], unread=unread)
        self.assertEqual(len(pairs), 1)
        self.assertEqual(unread, [(self.alpha, errno.EACCES)])

    @unittest.skipIf(AS_ROOT, "root searches a directory whatever its mode")
    def test_an_entry_it_cannot_stat_is_unread_not_refused_as_no_memory(self):
        """Mode 666: the names can be listed and none of them `stat`-ed. Each is named."""
        second = memstore.write(self.alpha, "a second fact", title="second", timestamped=True)
        locked(self, self.alpha, 0o666)
        expected = sorted([(self.alpha_file, errno.EACCES), (second, errno.EACCES)])
        self.assertEqual(memstore.read_files(self.alpha), ([], expected))
        with self.assertRaises(workspace.CannotCheck) as cm:
            memstore.files(self.alpha)
        self.assertEqual(str(cm.exception), " ".join(
            sentence(f"workspaces/alpha/memory/{p.name}") for p, _code in expected))

    def test_a_path_outside_the_plane_is_named_whole(self):
        outside = Path("/elsewhere/memory")
        self.assertEqual(str(workspace.CannotCheck([(outside, errno.EACCES)])),
                         sentence("/elsewhere/memory"))

    def test_a_memory_directory_that_is_a_symlink_loop_names_the_loop(self):
        for p in self.alpha.iterdir():
            p.unlink()
        self.alpha.rmdir()
        self.alpha.symlink_to(self.alpha)
        self.assertEqual(memstore.read_files(self.alpha), ([], [(self.alpha, errno.ELOOP)]))
        with self.assertRaises(workspace.CannotCheck) as cm:
            memstore.entries(self.alpha)
        self.assertEqual(str(cm.exception), f"workspaces/alpha/memory cannot be checked — fix "
                                             f"the symlink loop at {self.alpha}.")

    def test_a_readable_or_absent_directory_names_nothing(self):
        """And lists only memories: not the index, not a file that is no `.md`, not a directory."""
        (self.beta / ".gitkeep").write_text("")
        (self.beta / "notes.txt").write_text("x")
        (self.beta / "archive").mkdir()
        self.assertEqual(memstore.read_files(self.beta), (memstore.files(self.beta), []))
        self.assertEqual([p.parent for p in memstore.files(self.beta)], [self.beta])
        self.assertTrue(memstore.files(self.beta)[0].name.endswith("-keycloak-token-policy.md"))
        self.assertEqual(memstore.read_files(self.tmp / "nowhere"), ([], []))

    def test_a_memory_directory_linked_out_of_the_plane_is_refused_not_unread(self):
        """The other side of the line: containment's refusal (#336) stays a refusal, asked before
        the listing. Restoring read access clears nothing there, so it is not named as unread."""
        import shutil
        import tempfile
        outside = Path(tempfile.mkdtemp(prefix="edm-outside-"))
        self.addCleanup(shutil.rmtree, outside, ignore_errors=True)
        (outside / "stolen.md").write_text("# stolen\n\nkeycloak secret\n")
        link = config.PERSONAS_DIR / "dev" / "memory"
        link.parent.mkdir(parents=True)
        link.symlink_to(outside, target_is_directory=True)
        self.assertEqual(memstore.read_files(link), ([], []))
        self.assertEqual(memstore.files(link), [])

    @unittest.skipIf(AS_ROOT, "root searches a directory whatever its mode")
    def test_a_memory_directory_under_a_parent_it_cannot_search_is_named(self):
        locked(self, workspace.workspace_dir("alpha"))
        self.assertEqual(memstore.read_files(self.alpha), ([], [(self.alpha, errno.EACCES)]))

    @unittest.skipIf(AS_ROOT, "root searches a directory whatever its mode")
    def test_resolve_refuses_rather_than_answering_no_such_memory(self):
        """`forget` and `archive` reach a memory through here. On 3.14 a directory at mode 000
        answered "nothing matched"; on 3.11–3.13 `Path.exists` raised past every handler."""
        locked(self, self.alpha)
        with self.assertRaises(workspace.CannotCheck):
            memstore.resolve(self.alpha, self.alpha_file.name)


class ResolveReachesOnlyAMemoryItMayRead(TwoMemoryDirectories):
    """The direct hit in `memstore.resolve` — the short route `forget`, `show` and `archive` take
    to one file — asks `contain` and nothing else (#1084, #336).

    `Path.exists()` in front of it was redundant and cost a raise: `file_refusal` answers a
    refusal for every path a `stat` does not answer for (ENOENT, EACCES, ELOOP, a NUL in the
    name), and answers ``None`` only for a contained regular file, which is there by
    construction. The cases below are the ones that would tell the two apart if they differed.
    """

    def slug(self) -> str:
        return self.alpha_file.name[: -len(".md")]

    def test_a_memory_that_is_there_is_reached(self):
        self.assertEqual(memstore.resolve(self.alpha, self.slug()), self.alpha_file)
        self.assertEqual(memstore.resolve(self.alpha, self.alpha_file.name), self.alpha_file)

    def test_a_name_that_is_a_directory_or_a_dangling_link_reaches_nothing(self):
        (self.alpha / "adir.md").mkdir()
        (self.alpha / "gone.md").symlink_to(self.alpha / "nowhere.md")
        for ident in ("adir", "gone", "never-written"):
            with self.subTest(ident=ident):
                self.assertIsNone(memstore.resolve(self.alpha, ident))

    def test_a_name_holding_a_nul_reaches_nothing_and_does_not_raise(self):
        """`os.stat` raises `ValueError`, not `OSError`, for it — the one input shaped to get
        past a check. `contain` answers for it; `Path.exists()` raised."""
        self.assertIsNone(memstore.resolve(self.alpha, "a\0b"))

    def test_a_memory_directory_linked_out_of_the_plane_reaches_nothing(self):
        """The `dir_refusal` half: inside a linked `memory/` every file is an ordinary regular
        file, so the per-file check has nothing to object to (#336)."""
        import shutil
        import tempfile
        outside = Path(tempfile.mkdtemp(prefix="edm-outside-"))
        self.addCleanup(shutil.rmtree, outside, ignore_errors=True)
        (outside / "stolen.md").write_text("# stolen\n\nkeycloak secret\n")
        link = config.PERSONAS_DIR / "dev" / "memory"
        link.parent.mkdir(parents=True)
        link.symlink_to(outside, target_is_directory=True)
        self.assertIsNone(memstore.resolve(link, "stolen"))


class DoctorsMemoryRow(TwoMemoryDirectories):
    """`check_memory_indexes` lists each base before reading it (#1043), and a base it can list
    whose memories it cannot `stat` (mode 666) is one it could not read either: its drift is
    asked of `memstore.files`, which refuses there, and the row must not end in a traceback."""

    DETAIL = "; workspaces/alpha/memory/{} cannot be checked"

    def test_on_either_interpreter_each_memory_it_cannot_stat_is_named(self):
        for answers in BOTH_INTERPRETERS:
            with self.subTest(answers=answers):
                with unsearchable(self.alpha, answers):
                    r = doctor.check_memory_indexes()
                self.assertEqual(r.status, doctor.WARN, r)
                self.assertTrue(r.detail.endswith(self.DETAIL.format(self.alpha_file.name)),
                                r.detail)

    @unittest.skipIf(AS_ROOT, "root searches a directory whatever its mode")
    def test_a_real_base_at_mode_666(self):
        locked(self, self.alpha, 0o666)
        r = doctor.check_memory_indexes()
        self.assertEqual(r.status, doctor.WARN, r)
        self.assertTrue(r.detail.endswith(self.DETAIL.format(self.alpha_file.name)), r.detail)


class RecallNamesTheBaseItCouldNotSearch(TwoMemoryDirectories):
    def setUp(self) -> None:
        super().setUp()
        self.make_persona("dev", role="Dev")
        persona.remember("dev", "shared keycloak convention", shared=True)

    def test_the_gate_reports_it_beside_the_hits_it_found(self):
        with refusing_to_list(self.alpha):
            got = recall.recall("keycloak", workspace_name="alpha", persona_name="dev")
            listed = recall.recall(None, workspace_name="alpha", persona_name="dev")
        self.assertEqual(sorted({h.label for h in got.hits}), ["shared"])
        self.assertEqual(got.unread, [(self.alpha, errno.EACCES)])
        self.assertEqual(listed.unread, [(self.alpha, errno.EACCES)])
        labels = {h.label for h in listed.hits}
        self.assertIn("shared", labels)
        self.assertNotIn("workspace:alpha", labels)

    def test_a_readable_plane_reports_nothing_unread(self):
        self.assertEqual(recall.recall("keycloak", workspace_name="alpha",
                                       persona_name="dev").unread, [])
        for query in ("keycloak", "absent"):
            with self.subTest(query=query):
                rc, _, err = self.recall_cmd(query)
                self.assertEqual(rc, 0)
                self.assertNotIn("not searched", err)
                self.assertNotIn("cannot be checked", err)

    def recall_cmd(self, query: str) -> tuple[int, str, str]:
        return run(commands.cmd_recall, query=query, scope=None, ephemeral=False, persona="dev",
                   workspace="alpha", all_workspaces=False, since=None, limit=8, full=False)

    @unittest.skipIf(AS_ROOT, "root lists a directory whatever its mode")
    def test_charter_recall_names_it_beside_what_it_found_and_exits_1(self):
        """Everything it could read is printed, and the exit says the search was not whole."""
        locked(self, self.alpha)
        rc, out, err = self.recall_cmd("keycloak")
        self.assertEqual(rc, 1)
        self.assertIn("shared keycloak convention", out)
        self.assertIn(sentence("workspaces/alpha/memory"), err)
        self.assertIn("1 memory(ies) across workspace, persona, shared, refs; 1 base(s) not "
                      "searched — charter could not read them.", err)

    def test_with_no_hit_it_does_not_say_nothing_matches(self):
        with refusing_to_list(self.alpha):
            rc, _, err = self.recall_cmd("rotates")
        self.assertEqual(rc, 1)
        self.assertIn(sentence("workspaces/alpha/memory"), err)
        self.assertNotIn("No memories match 'rotates' across workspace, persona, shared, refs.",
                         err)
        self.assertIn("No memories match 'rotates' across workspace, persona, shared, refs; "
                      "1 base(s) not searched — charter could not read them.", err)


class RecallSaysWhichQuestionItAnswered(TwoMemoryDirectories):
    """Its two empty answers are different answers: a query that matched nothing, and a listing
    of a scope holding nothing. One sentence for both would report a corpus for a search."""

    def empty(self, **kw) -> str:
        workspace.ensure("gamma")
        workspace.scaffold("gamma")
        _, _, err = run(commands.cmd_recall, scope="workspace", ephemeral=False, persona=None,
                        workspace="gamma", all_workspaces=False, since=None, limit=8, full=False,
                        **kw)
        return err

    def test_a_query_that_matched_nothing(self):
        self.assertIn("No memories match 'zebra' across workspace.", self.empty(query="zebra"))

    def test_a_listing_with_nothing_to_list(self):
        self.assertIn("No memories yet across workspace.", self.empty(query=None))


class ALimitCapsAListingAndZeroLiftsTheCap(TwoMemoryDirectories):
    """`recall --limit`, on the no-query path. Capping everything would hide the corpus the
    listing exists to show; capping nothing would put every memory of every base on screen."""

    def hits(self, limit: int) -> int:
        return len(recall.recall(None, workspace_name="beta", scopes=("workspace",),
                                 limit=limit).hits)

    def test_a_limit_truncates_and_zero_returns_everything(self):
        for i in range(4):
            memstore.write(self.beta, f"another fact {i}", title=f"fact {i}", timestamped=True)
        self.assertEqual(self.hits(2), 2)
        self.assertEqual(self.hits(0), 5)


class PersonaRecall(PersonaIso):
    def setUp(self) -> None:
        super().setUp()
        make_plane(self)
        self.make_persona("dev", role="Dev")
        self.own = persona.memory_dir("dev")
        # Written to the store, not through `persona.remember`: that records a trace event, and
        # the listing's activity section would then speak for a store it could not read.
        memstore.write(self.own, "own keycloak deploy fact", title="own keycloak deploy fact")

    def test_a_query_names_the_directory_it_could_not_search(self):
        persona.remember("dev", "shared keycloak convention", shared=True)
        with refusing_to_list(self.own):
            rc, out, err = run(commands_persona.cmd_persona_recall, name="dev", query="keycloak",
                               log=8)
        self.assertEqual(rc, 1)
        self.assertIn("shared keycloak convention", out)
        self.assertIn(sentence("personas/dev/memory"), err)

    def test_a_query_with_no_hit_does_not_say_there_is_no_memory_of_it(self):
        with refusing_to_list(self.own):
            rc, _, err = run(commands_persona.cmd_persona_recall, name="dev", query="keycloak",
                             log=8)
        self.assertEqual(rc, 1)
        self.assertIn(sentence("personas/dev/memory"), err)
        self.assertNotIn("no memory of 'keycloak'", err)

    @unittest.skipIf(AS_ROOT, "root lists a directory whatever its mode")
    def test_the_listing_does_not_say_it_has_no_memories_yet(self):
        locked(self, self.own)
        rc, _, err = run(commands_persona.cmd_persona_recall, name="dev", query=None, log=8)
        self.assertEqual(rc, 1)
        self.assertIn(sentence("personas/dev/memory"), err)
        self.assertNotIn("has no memories yet", err)

    def test_the_listing_names_an_ephemeral_scratch_it_could_not_read(self):
        scratch = persona.ephemeral_dir("dev", False, None)
        scratch.mkdir(parents=True, exist_ok=True)
        with refusing_to_list(scratch):
            _, _, err = run(commands_persona.cmd_persona_recall, name="dev", query=None, log=8)
        self.assertIn(sentence(scratch.relative_to(config.ROOT).as_posix()), err)

    def test_a_persona_with_memories_is_not_told_it_has_none(self):
        _, _, err = run(commands_persona.cmd_persona_recall, name="dev", query=None, log=8)
        self.assertNotIn("has no memories yet", err)

    def test_a_persona_with_nothing_at_all_is_told_so(self):
        """The other side of the line the unread sentence sits on: nothing printed and nothing
        unread is a persona that has recorded nothing, and silence there reads as a broken
        command."""
        self.make_persona("fresh", role="Dev")
        _, _, err = run(commands_persona.cmd_persona_recall, name="fresh", query=None, log=8)
        self.assertIn("persona 'fresh' has no memories yet.", err)

    def test_a_readable_persona_names_nothing_and_exits_0(self):
        for query in ("keycloak", "absent", None):
            with self.subTest(query=query):
                rc, out, err = run(commands_persona.cmd_persona_recall, name="dev", query=query,
                                   log=8)
                self.assertEqual(rc, 0)
                self.assertNotIn("cannot be checked", err)
        _, _, err = run(commands_persona.cmd_persona_recall, name="dev", query="absent", log=8)
        self.assertIn("no memory of 'absent' for 'dev'.", err)


class WorkspaceRecall(TwoMemoryDirectories):
    @unittest.skipIf(AS_ROOT, "root lists a directory whatever its mode")
    def test_the_listing_names_it_and_does_not_say_there_are_none_yet(self):
        locked(self, self.alpha)
        rc, _, err = run(commands_workspace.cmd_workspace_recall, workspace="alpha", query=None)
        self.assertEqual(rc, 1)
        self.assertIn(sentence("workspaces/alpha/memory"), err)
        self.assertNotIn("has no memories yet", err)

    def test_a_query_names_it_and_does_not_say_nothing_matches(self):
        with refusing_to_list(self.alpha):
            rc, _, err = run(commands_workspace.cmd_workspace_recall, workspace="alpha",
                             query="keycloak")
        self.assertEqual(rc, 1)
        self.assertIn(sentence("workspaces/alpha/memory"), err)
        self.assertNotIn("No memories in 'alpha' match", err)

    def test_a_readable_workspace_exits_0(self):
        for query in ("keycloak", "absent", None):
            with self.subTest(query=query):
                rc, _, err = run(commands_workspace.cmd_workspace_recall, workspace="alpha",
                                 query=query)
                self.assertEqual(rc, 0)
                self.assertNotIn("cannot be checked", err)


class AnIndexWriterDoesNotWorkFromAnUnlistedDirectory(TwoMemoryDirectories):
    @unittest.skipIf(AS_ROOT, "root lists a directory whatever its mode")
    def test_apply_safe_refuses_and_leaves_the_store_as_it_was(self):
        """Mode 333: the index is writable, the directory is not listable. Every "safe" op here —
        archive a duplicate, link an unindexed file — is decided from the listing, so one it
        could not make is no basis for touching MEMORY.md."""
        dup = memstore.write(self.alpha, "the keycloak token in alpha rotates yearly",
                             title="unindexed copy", timestamped=True, index=False)
        idx = memstore.index_path(self.alpha)
        before = idx.read_bytes()
        locked(self, self.alpha, 0o333)
        with self.assertRaises(workspace.CannotCheck) as cm:
            curate.apply_safe(self.alpha)
        self.alpha.chmod(0o755)
        self.assertEqual(str(cm.exception), sentence("workspaces/alpha/memory"))
        self.assertEqual(idx.read_bytes(), before)
        self.assertTrue(dup.exists())

    def test_workspace_optimize_names_it_and_goes_on_to_the_next(self):
        memstore.write(self.beta, "the keycloak token in beta rotates yearly", title="again",
                       timestamped=True)
        with refusing_to_list(self.alpha):
            rc, out, err = run(commands_workspace.cmd_workspace_optimize, name=None, all=True,
                               apply=False, stale_days=90)
        self.assertEqual(rc, 1)
        self.assertIn(sentence("workspaces/alpha/memory"), err)
        self.assertIn("◆ beta", out)
        rc, _, _ = run(commands_workspace.cmd_workspace_optimize, name=None, all=True,
                       apply=False, stale_days=90)
        self.assertEqual(rc, 0)

    def test_persona_optimize_names_it_and_goes_on_to_the_next(self):
        self.make_persona("dev", role="Dev")
        persona.remember("dev", "own keycloak deploy fact")
        persona.remember("dev", "shared keycloak convention", shared=True)
        with refusing_to_list(persona.memory_dir("dev")):
            rc, out, err = run(commands_persona.cmd_persona_optimize, name=None, all=True,
                               apply=False, stale_days=90)
        self.assertEqual(rc, 1)
        self.assertIn(sentence("personas/dev/memory"), err)
        self.assertIn("◆ _shared", out)
        rc, _, _ = run(commands_persona.cmd_persona_optimize, name=None, all=True,
                       apply=False, stale_days=90)
        self.assertEqual(rc, 0)


class TheBriefingDigest(PersonaIso):
    def setUp(self) -> None:
        super().setUp()
        make_plane(self)
        self.make_persona("dev", role="Dev")
        persona.ensure_shared()

    def test_a_persona_with_no_memories_and_nothing_unread_has_no_digest(self):
        """The briefing costs every session its context, so a persona with nothing recorded and
        nothing unread contributes no block at all — the case the unread clause is beside."""
        self.assertEqual(hooks._memory_digest("dev"), "")

    def test_it_names_the_store_it_could_not_count_rather_than_counting_none(self):
        persona.remember("dev", "own keycloak deploy fact")
        persona.remember("dev", "shared keycloak convention", shared=True)
        with refusing_to_list(persona.memory_dir("dev")):
            digest = hooks._memory_digest("dev")
        self.assertIn("**own (?)** — not read:\n   ⚠ " + sentence("personas/dev/memory"), digest)
        self.assertIn("## Memory — ? own · 1 shared", digest)
        self.assertIn("shared keycloak convention", digest)

    def test_either_store_alone_unread_is_named_not_an_empty_digest(self):
        """With nothing readable beside it, the digest said nothing at all — the briefing of a
        persona that has recorded nothing."""
        for shared, rel, head in ((False, "personas/dev/memory", "## Memory — ? own · 0 shared"),
                                  (True, "personas/_shared/memory",
                                   "## Memory — 0 own · ? shared")):
            with self.subTest(shared=shared):
                with refusing_to_list(persona.memory_dir("dev", shared=shared)):
                    digest = hooks._memory_digest("dev")
                self.assertIn(sentence(rel), digest)
                self.assertIn(head, digest)
                self.assertNotIn("(0)**", digest)


class TheReadmeRoster(PersonaIso):
    def test_it_is_not_rewritten_with_a_count_it_could_not_take_and_says_why(self):
        """`charter docs` refreshes README's persona roster, whose memory column is a count of
        each persona's memory directory. One it could not list read as 0 and was committed so."""
        from charter import render
        make_plane(self)
        self.make_persona("dev", role="Dev")
        memstore.write(persona.memory_dir("dev"), "own keycloak deploy fact", title="own fact")
        readme = config.ROOT / "README.md"
        readme.write_text(f"# plane\n\n{render.PERSONAS_BEGIN}\nold roster\n{render.PERSONAS_END}\n")
        before = readme.read_text()
        err = io.StringIO()
        with refusing_to_list(persona.memory_dir("dev")), redirect_stderr(err):
            changed = commands.refresh_readme_personas()
        self.assertFalse(changed)
        self.assertEqual(readme.read_text(), before)
        self.assertIn(sentence("personas/dev/memory"), err.getvalue())

    def test_charter_docs_still_writes_the_topology_and_exits_1(self):
        from charter import render
        make_plane(self)
        self.make_persona("dev", role="Dev")
        memstore.write(persona.memory_dir("dev"), "own keycloak deploy fact", title="own fact")
        readme = config.ROOT / "README.md"
        readme.write_text(f"# plane\n\n{render.PERSONAS_BEGIN}\nold roster\n{render.PERSONAS_END}\n")
        with mock.patch.object(commands.inventory, "load", return_value={"repos": [{}]}), \
                mock.patch.object(commands.render, "topology_md", return_value="# topology"):
            with refusing_to_list(persona.memory_dir("dev")):
                rc, _, err = run(commands.cmd_docs)
            self.assertEqual(rc, 1)
            self.assertIn(sentence("personas/dev/memory"), err)
            self.assertEqual((config.DOCS_DIR / "topology.md").read_text(), "# topology\n")
            rc, _, err = run(commands.cmd_docs)
        self.assertEqual(rc, 0)
        self.assertNotIn("cannot be checked", err)
        self.assertNotIn("old roster", readme.read_text())


class ACommandWithNoPartialAnswerRefusesCleanly(PersonaIso):
    @unittest.skipIf(AS_ROOT, "root lists a directory whatever its mode")
    def test_it_prints_the_sentence_and_exits_1_rather_than_crashing_or_answering_none(self):
        make_plane(self)
        self.make_persona("dev", role="Dev")
        persona.remember("dev", "own keycloak deploy fact")
        locked(self, persona.memory_dir("dev"))
        err = io.StringIO()
        with redirect_stderr(err), redirect_stdout(io.StringIO()):
            rc = cli.main(["persona", "dedupe", "dev"])
        self.assertEqual(rc, 1)
        self.assertIn(sentence("personas/dev/memory"), err.getvalue())
        self.assertNotIn("no near-duplicate", err.getvalue())
        self.assertNotIn("charter bug", err.getvalue())


class TwoChangeStores(PersonaIso):
    def setUp(self) -> None:
        super().setUp()
        make_plane(self)
        for ws in ("alpha", "beta"):
            workspace.ensure(ws)
            change.write(ws, f"{ws}-api", change.new_record(f"{ws}-api", "API 1 -> 2", "me",
                                                             "2026-09-14T00:00:00+00:00"))
        self.alpha = change.changes_dir("alpha")


class TheChangeStoreNamesWhatItCouldNotList(TwoChangeStores):
    @unittest.skipIf(AS_ROOT, "root lists a directory whatever its mode")
    def test_a_real_changes_directory_at_mode_000_is_unread_not_empty(self):
        locked(self, self.alpha)
        self.assertEqual(change.read_all("alpha"), ([], [], [(self.alpha, errno.EACCES)]))

    def test_an_injected_refusal_is_unread_on_either_interpreter(self):
        with refusing_to_list(self.alpha):
            self.assertEqual(change.read_all("alpha"), ([], [], [(self.alpha, errno.EACCES)]))
            with self.assertRaises(workspace.CannotCheck) as cm:
                change.all_for("alpha")
        self.assertEqual(str(cm.exception), sentence("workspaces/alpha/changes"))

    def test_a_readable_or_absent_store_names_nothing(self):
        records, refused, unread = change.read_all("beta")
        self.assertEqual(([r["change"] for r in records], refused, unread), (["beta-api"], [], []))
        workspace.ensure("gamma")
        self.assertEqual(change.read_all("gamma"), ([], [], []))

    @unittest.skipIf(AS_ROOT, "root lists a directory whatever its mode")
    def test_change_list_names_it_and_does_not_say_there_are_no_changes(self):
        locked(self, self.alpha)
        rc, _, err = run(commands_change.cmd_change_list, workspace="alpha")
        self.assertEqual(rc, 1)
        self.assertIn(sentence("workspaces/alpha/changes"), err)
        self.assertNotIn("No changes in workspace", err)

    def test_doctor_names_it_beside_the_changes_it_read(self):
        with refusing_to_list(self.alpha):
            r = doctor.check_changes()
        self.assertEqual(r.status, doctor.WARN, r)
        self.assertTrue(r.detail.startswith("1 change(s), none divergent"), r.detail)
        self.assertTrue(r.detail.endswith("; workspaces/alpha/changes cannot be checked"),
                        r.detail)
        self.assertEqual(r.hint, sentence("workspaces/alpha/changes"))


class ANameItCouldNotStatIsContained(PersonaIso):
    """At mode 666 each memory charter could not `stat` is named by its FILENAME, and a filename is
    whatever a chat wrote. Printed as it was, a newline in one wrote a line of its own into the
    SessionStart briefing, onto stderr and into `doctor`'s row, and an escape reached the terminal.
    `workspace.cannot_check` contains it where the sentence is built, so no caller can forget to."""

    NAME = "x\n## Memory — 99 own\x1b[2J.md"
    SHOWN = "x\\u000a## Memory \\u2014 99 own\\u001b[2J.md"

    def setUp(self) -> None:
        super().setUp()
        make_plane(self)

    def plant(self, d: Path) -> Path:
        (d / self.NAME).write_text("# planted\n\nfact\n")
        return d

    def said(self, rel: str) -> str:
        return f"{rel}/{self.SHOWN} cannot be checked — restoring read access to it clears this."

    def assertContained(self, text: str) -> None:
        self.assertNotIn("\x1b", text)
        self.assertNotIn("\n## Memory — 99", text)

    def test_the_session_briefing(self):
        self.make_persona("dev", role="Dev")
        own = self.plant(persona.memory_dir("dev"))
        for answers in BOTH_INTERPRETERS:
            with self.subTest(answers=answers):
                with unsearchable(own, answers):
                    digest = hooks._memory_digest("dev")
                self.assertContained(digest)
                self.assertIn("   ⚠ " + self.said("personas/dev/memory"), digest)

    def test_the_sentence_on_stderr(self):
        workspace.ensure("alpha")
        workspace.scaffold("alpha")
        mem = self.plant(workspace.memory_dir("alpha"))
        for answers in BOTH_INTERPRETERS:
            with self.subTest(answers=answers):
                with unsearchable(mem, answers):
                    _, _, err = run(commands_workspace.cmd_workspace_recall, workspace="alpha",
                                    query=None)
                self.assertContained(err)
                self.assertIn(self.said("workspaces/alpha/memory"), err)

    def test_the_remedy_for_a_link_loop_repeats_the_path_contained_too(self):
        """ELOOP's remedy names the link again, from the filesystem root — the second place the
        same name is printed in one sentence."""
        loop = config.ROOT / "workspaces" / "alpha" / "memory" / self.NAME
        said = workspace.cannot_check(loop, errno.ELOOP)
        self.assertContained(said)
        self.assertNotIn("\n", said)
        self.assertEqual(said, f"workspaces/alpha/memory/{self.SHOWN} cannot be checked — fix the "
                               f"symlink loop at {config.ROOT}/workspaces/alpha/memory/{self.SHOWN}")

    def test_doctors_row(self):
        workspace.ensure("alpha")
        workspace.scaffold("alpha")
        mem = self.plant(workspace.memory_dir("alpha"))
        for answers in BOTH_INTERPRETERS:
            with self.subTest(answers=answers):
                with unsearchable(mem, answers):
                    r = doctor.check_memory_indexes()
                self.assertContained(r.detail + r.hint)
                self.assertNotIn("\n", r.detail + r.hint)
                self.assertTrue(r.detail.endswith(
                    f"; workspaces/alpha/memory/{self.SHOWN} cannot be checked"), r.detail)
                self.assertIn(self.said("workspaces/alpha/memory"), r.hint)


class LiveOffDoesNotGoPrivateAroundAChangesDirectoryItCouldNotList(TwoChangeStores):
    """`_ws_meta_paths` asks `change.has_records` whether `changes/` goes to git. One it could not
    list answered no, so `workspace live --off` untracked the manifest and memory, went LOCAL, and
    left every change record committed on a workspace the operator had just made private."""

    def test_has_records_names_it_rather_than_answering_none(self):
        with refusing_to_list(self.alpha):
            with self.assertRaises(workspace.CannotCheck) as cm:
                change.has_records("alpha")
        self.assertEqual(str(cm.exception), sentence("workspaces/alpha/changes"))

    def test_a_readable_absent_or_emptied_store_still_answers(self):
        self.assertTrue(change.has_records("beta"))
        workspace.ensure("gamma")
        self.assertFalse(change.has_records("gamma"))
        (change.changes_dir("beta") / "notes.txt").write_text("x")
        change.forget("beta", "beta-api")
        self.assertFalse(change.has_records("beta"))

    def test_live_off_untracks_nothing_stays_live_and_exits_1(self):
        workspace.set_live("alpha", True)
        err = io.StringIO()
        with mock.patch.object(commands_workspace, "_git") as git, refusing_to_list(self.alpha), \
                redirect_stderr(err), redirect_stdout(io.StringIO()):
            rc = cli.main(["workspace", "live", "alpha", "--off"])
        self.assertEqual(rc, 1)
        self.assertIn(sentence("workspaces/alpha/changes"), err.getvalue())
        git.assert_not_called()
        self.assertTrue(workspace.is_live("alpha"))


class RefsNameWhatTheyCouldNotRead(PersonaIso):
    """Refs nest, so `recall` offers each directory under `refs/` as a base of its own
    (`recall._ref_dirs`). It found them with `rglob`, which leaves out a directory it cannot tell
    is one, after `base.exists()`, which raises on 3.11–3.13 under a parent charter may not search
    and answers False on 3.14. A runbook in either was a runbook recall said nothing about."""

    def setUp(self) -> None:
        super().setUp()
        make_plane(self)
        self.make_persona("dev", role="Dev")
        self.refs = persona.refs_dir("dev")
        (self.refs / "release").mkdir(parents=True)
        (self.refs / "release" / "keycloak.md").write_text("# keycloak prerequisites\n\nrealm\n")
        (self.refs / "runbook.md").write_text("# runbook\n\nkeycloak restarts\n")

    def recall(self):
        return recall.recall("keycloak", persona_name="dev", scopes=("refs",))

    def test_a_readable_tree_offers_every_directory_and_does_not_follow_a_link(self):
        """A link back up the tree is offered and not walked: `rglob` never descended into one,
        and walking it would never end."""
        self.assertEqual(sorted(h.path.name for h in self.recall().hits),
                         ["keycloak.md", "runbook.md"])
        self.assertEqual(self.recall().unread, [])
        # Walked below the first level: refs nest as deep as the curator likes.
        (self.refs / "release" / "2026").mkdir()
        (self.refs / "release" / "2026" / "rotation.md").write_text("# rotation\n\nkeycloak\n")
        self.assertEqual(sorted(h.path.name for h in self.recall().hits),
                         ["keycloak.md", "rotation.md", "runbook.md"])
        (self.refs / "again").symlink_to(self.refs, target_is_directory=True)
        unread: list = []
        self.assertEqual(recall._ref_dirs(self.refs, unread),
                         [self.refs, self.refs / "again", self.refs / "release",
                          self.refs / "release" / "2026"])
        self.assertEqual(recall._ref_dirs(self.tmp / "nowhere", unread), [])
        self.assertEqual(unread, [])

    def test_a_directory_it_can_tell_is_one_and_cannot_list_is_offered_and_named_once(self):
        """Not raised out of the walk: listing it is `memstore.read_files`' job, and that names it."""
        release = self.refs / "release"
        with refusing_to_list(release):
            got = self.recall()
        self.assertIn("runbook.md", [h.path.name for h in got.hits])
        self.assertEqual(got.unread, [(release, errno.EACCES)])

    def test_a_refs_directory_linked_out_of_the_plane_is_not_walked(self):
        """#336 at the top of the walk: under a `refs/` that is itself a link out, every directory
        is one containment would refuse one at a time, after the walk had listed each of them."""
        import shutil
        import tempfile
        outside = Path(tempfile.mkdtemp(prefix="edm-outside-"))
        self.addCleanup(shutil.rmtree, outside, ignore_errors=True)
        (outside / "nested").mkdir()
        linked = config.PERSONAS_DIR / "reader" / "refs"
        linked.parent.mkdir(parents=True)
        linked.symlink_to(outside, target_is_directory=True)
        unread: list = []
        self.assertEqual(recall._ref_dirs(linked, unread), [])
        self.assertEqual(unread, [])

    def test_an_entry_whose_kind_it_could_not_tell_is_named_once(self):
        for answers in BOTH_INTERPRETERS:
            with self.subTest(answers=answers):
                with unsearchable(self.refs, answers):
                    got = self.recall()
                # `README.md` is the one `persona.scaffold_memory` writes into every `refs/`.
                self.assertEqual(got.unread, [(self.refs / "README.md", errno.EACCES),
                                              (self.refs / "release", errno.EACCES),
                                              (self.refs / "runbook.md", errno.EACCES)])

    def test_refs_under_a_persona_directory_it_cannot_search_are_named(self):
        for answers in BOTH_INTERPRETERS:
            with self.subTest(answers=answers):
                with unsearchable(config.PERSONAS_DIR / "dev", answers):
                    got = self.recall()
                    with self.assertRaises(workspace.CannotCheck):
                        recall.sources(persona_name="dev", scopes=("refs",))
                self.assertEqual(got.unread, [(self.refs, errno.EACCES)])

    @unittest.skipIf(AS_ROOT, "root searches a directory whatever its mode")
    def test_charter_recall_over_a_real_refs_at_mode_666(self):
        locked(self, self.refs, 0o666)
        rc, _, err = run(commands.cmd_recall, query="keycloak", scope="refs", ephemeral=False,
                         persona="dev", workspace=None, all_workspaces=False, since=None,
                         limit=8, full=False)
        self.assertEqual(rc, 1)
        self.assertIn(sentence("personas/dev/refs/release"), err)
        self.assertEqual(err.count(sentence("personas/dev/refs/runbook.md")), 1)


class PersonaShow(PersonaIso):
    """`persona show` counts each memory quadrant and the refs beside the charter. A memory it could
    not list raised out of the command before the charter body was printed; refs it could not list
    were `glob`bed as none."""

    def setUp(self) -> None:
        super().setUp()
        make_plane(self)
        self.make_persona("dev", role="Dev")
        memstore.write(persona.memory_dir("dev"), "own keycloak deploy fact", title="own fact")
        self.refs = persona.refs_dir("dev")
        self.refs.mkdir(parents=True, exist_ok=True)

    def show(self):
        return run(commands_persona.cmd_persona_show, name="dev")

    def test_a_memory_it_could_not_list_is_a_question_mark_and_the_body_still_prints(self):
        with refusing_to_list(persona.memory_dir("dev")):
            rc, out, err = self.show()
        self.assertEqual(rc, 1)
        self.assertIn("memory:  ? own · 0 shared (persistent) · 0 ephemeral · 0 refs", out)
        self.assertIn("charter body", out)
        self.assertIn(sentence("personas/dev/memory"), err)

    def test_refs_it_could_not_list_are_not_counted_as_none(self):
        with refusing_to_list(self.refs):
            rc, out, err = self.show()
        self.assertEqual(rc, 1)
        self.assertIn("memory:  1 own · 0 shared (persistent) · 0 ephemeral · ? refs", out)
        self.assertIn("charter body", out)
        self.assertIn(sentence("personas/dev/refs"), err)

    def test_a_readable_persona_is_counted_and_exits_0(self):
        (self.refs / "README.md").write_text("# refs\n")
        (self.refs / "runbook.md").write_text("# runbook\n")
        rc, out, err = self.show()
        self.assertEqual(rc, 0)
        self.assertIn("memory:  1 own · 0 shared (persistent) · 0 ephemeral · 1 refs", out)
        self.assertNotIn("cannot be checked", err)


class TheLandingLogNamesWhatItCouldNotRead(TwoChangeStores):
    """`changes/log/` holds what charter declared it landed. Both of its readers globbed it, and a
    glob of a directory charter may not list is empty: `change land` let a dependent through over a
    blocker whose revert only the log could show, `change revert` said charter had landed nothing,
    `change show` read every member as landed outside charter, and `doctor` checked no landing and
    said so as OK."""

    def setUp(self) -> None:
        super().setUp()
        self.log = change.log_dir("alpha")
        self.log.mkdir(parents=True)
        (self.log / "h.jsonl").write_text(json.dumps(
            {"ts": "t", "change": "alpha-api", "repo": "r", "number": 1, "merge": "e0c9d13",
             "head": "h"}) + "\n")

    def test_both_readers_name_a_log_directory_they_could_not_list(self):
        with refusing_to_list(self.log):
            self.assertEqual(change.read_landings("alpha"), ([], [(self.log, errno.EACCES)]))
            for read in (lambda: change.landings("alpha"),
                         lambda: commands_change.landings("alpha", "alpha-api")):
                with self.assertRaises(workspace.CannotCheck) as cm:
                    read()
                self.assertEqual(str(cm.exception), sentence("workspaces/alpha/changes/log"))

    @unittest.skipIf(AS_ROOT, "root reads a file whatever its mode")
    def test_a_log_file_it_could_not_open_is_named_not_skipped(self):
        f = locked(self, self.log / "h.jsonl")
        self.assertEqual(change.read_landings("alpha"), ([], [(f, errno.EACCES)]))
        with self.assertRaises(workspace.CannotCheck) as cm:
            commands_change.landings("alpha", "alpha-api")
        self.assertEqual(str(cm.exception), sentence("workspaces/alpha/changes/log/h.jsonl"))

    def test_a_readable_log_names_nothing(self):
        (self.log / "notes.txt").write_text("not a log\n")
        (self.log / "old.jsonl").mkdir()
        lines, unread = change.read_landings("alpha")
        self.assertEqual(([line["merge"] for line in lines], unread), (["e0c9d13"], []))
        self.assertEqual(list(commands_change.landings("alpha", "alpha-api")), ["r"])
        self.assertEqual(change.read_landings("beta"), ([], []))

    def test_change_show_prints_the_record_names_the_log_and_exits_1(self):
        with refusing_to_list(self.log):
            rc, out, err = run(commands_change.cmd_change_show, workspace="alpha",
                               change="alpha-api")
        self.assertEqual(rc, 1)
        self.assertIn("why: API 1 -> 2", out)
        self.assertIn(sentence("workspaces/alpha/changes/log"), err)
        rc, _, err = run(commands_change.cmd_change_show, workspace="alpha", change="alpha-api")
        self.assertEqual(rc, 0)
        self.assertNotIn("cannot be checked", err)

    def test_change_revert_names_it_rather_than_saying_nothing_landed(self):
        err = io.StringIO()
        with refusing_to_list(self.log), redirect_stderr(err), redirect_stdout(io.StringIO()):
            rc = cli.main(["change", "revert", "alpha-api", "-w", "alpha"])
        self.assertEqual(rc, 1)
        self.assertIn(sentence("workspaces/alpha/changes/log"), err.getvalue())
        self.assertNotIn("nothing to revert", err.getvalue())

    def test_doctor_names_it_beside_the_verdict(self):
        with refusing_to_list(self.log):
            r = doctor.check_changes()
        self.assertEqual(r.status, doctor.WARN, r)
        self.assertTrue(r.detail.endswith("; workspaces/alpha/changes/log cannot be checked"),
                        r.detail)
        self.assertEqual(r.hint, sentence("workspaces/alpha/changes/log"))


class AForkSaysWhatItDidNotCarry(TwoMemoryDirectories):
    """`workspace fork` copies the source's charter, memory and todos with `shutil.copytree`. One it
    could not read raised out of the command as a charter bug — after `copytree` had already given
    a directory at mode 666 its mode, so the fork's own memory was no longer writable. Now each
    piece is carried or named, and the sentence that says it forked says what it did not carry."""

    def fork(self, live: bool = False, new: str = "gamma"):
        return run(commands_workspace.cmd_workspace_fork, src="alpha", new=new, live=live,
                   restore=False)

    def test_either_sentence_says_a_live_fork_is_live(self):
        """What the fork is — LIVE or LOCAL — is in the sentence whether or not it carried
        everything, and a LIVE one is not reported as LOCAL."""
        rc, _, err = self.fork(live=True)
        self.assertEqual(rc, 0)
        self.assertIn("Forked 'alpha' → 'gamma' — charter + context + memo copied (LIVE).", err)
        self.assertIn("Share the fork: charter workspace save gamma", err)
        with refusing_to_list(self.alpha):
            rc, _, err = self.fork(live=True, new="delta")
        self.assertEqual(rc, 1)
        self.assertIn("Forked 'alpha' → 'delta' (LIVE) without the memory charter could not read "
                      "in 'alpha':", err)

    def fork_note(self) -> str:
        return "\n".join(p.read_text() for p in memstore.files(workspace.memory_dir("gamma")))

    def test_a_memory_directory_it_could_not_list_is_named_and_the_rest_is_carried(self):
        todos.add("alpha", "finish the keycloak rotation")
        with refusing_to_list(self.alpha):
            rc, _, err = self.fork()
        self.assertEqual(rc, 1)
        self.assertIn("Forked 'alpha' → 'gamma' (LOCAL) without the memory charter could not read "
                      "in 'alpha':", err)
        self.assertIn(sentence("workspaces/alpha/memory"), err)
        self.assertNotIn("charter + context + memo copied", err)
        self.assertNotIn("charter bug", err)
        self.assertEqual(todos.count_open("gamma"), 1)
        self.assertIn("without the memory charter could not read there", self.fork_note())

    @unittest.skipIf(AS_ROOT, "root searches a directory whatever its mode")
    def test_todos_at_mode_666_are_each_named_and_the_fork_stays_usable(self):
        todos.add("alpha", "finish the keycloak rotation")
        names = sorted(p.name for p in todos.todos_dir("alpha").iterdir())
        locked(self, todos.todos_dir("alpha"), 0o666)
        rc, _, err = self.fork()
        self.assertEqual(rc, 1)
        self.assertIn("Forked 'alpha' → 'gamma' (LOCAL) without the todos charter could not read "
                      "in 'alpha':", err)
        for name in names:
            self.assertIn(sentence(f"workspaces/alpha/todos/{name}"), err)
        self.assertEqual(len(memstore.files(workspace.memory_dir("gamma"))), 2)
        self.assertIn("keycloak token policy", self.fork_note())
        # The fork's own todo list takes a todo: nothing gave it the source's mode 666.
        todos.add("gamma", "pick the rotation up in the fork")
        self.assertEqual(todos.count_open("gamma"), 1)

    @unittest.skipIf(AS_ROOT, "root reads a file whatever its mode")
    def test_every_piece_it_could_not_read_is_listed_in_the_one_sentence(self):
        """Three pieces, so the list is spelled out whole: the first two joined by a comma, the
        last by "and"."""
        todos.add("alpha", "finish the keycloak rotation")
        locked(self, workspace.charter_file("alpha"))
        locked(self, todos.todos_dir("alpha"))
        with refusing_to_list(self.alpha):
            rc, _, err = self.fork()
        self.assertEqual(rc, 1)
        self.assertIn("Forked 'alpha' → 'gamma' (LOCAL) without the workspace.md, memory and todos "
                      "charter could not read in 'alpha':", err)
        self.assertIn(sentence("workspaces/alpha/workspace.md"), err)
        self.assertIn(sentence("workspaces/alpha/todos"), err)

    @unittest.skipIf(AS_ROOT, "root reads a file whatever its mode")
    def test_a_memory_it_could_not_open_is_named_and_its_neighbours_are_carried(self):
        second = memstore.write(self.alpha, "a second fact", title="second", timestamped=True)
        locked(self, self.alpha_file)
        rc, _, err = self.fork()
        self.assertEqual(rc, 1)
        self.assertIn(sentence(f"workspaces/alpha/memory/{self.alpha_file.name}"), err)
        self.assertIn("without the memory charter could not read in 'alpha':", err)
        self.assertTrue((workspace.memory_dir("gamma") / second.name).exists())

    def test_a_readable_source_is_carried_whole_and_exits_0(self):
        todos.add("alpha", "finish the keycloak rotation")
        rc, _, err = self.fork()
        self.assertEqual(rc, 0)
        self.assertIn("Forked 'alpha' → 'gamma' — charter + context + memo copied (LOCAL).", err)
        self.assertNotIn("could not read", err)
        self.assertNotIn("Share the fork", err)
        self.assertIn("inherited its vision, context, glossary, and memo", self.fork_note())
        self.assertIn("keycloak token policy", self.fork_note())
        self.assertEqual(todos.count_open("gamma"), 1)


if __name__ == "__main__":
    unittest.main()
