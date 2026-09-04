"""Every workspace has a `workspace.json` — from birth, and by backfill (#884).

Measured on the plane this was written for: **5 of 17 workspaces had one.** The file was
written only by `charter workspace snapshot`, so it existed wherever somebody happened to
run that command — while its whole purpose argues the other way. `cmd_workspace_restore`
says it is *"committed precisely so a teammate can restore someone else's"* workspace, and
`workspace._live_block` maintains a `.gitignore` block whose first line un-ignores it. A
file designed to be shared cannot be an opt-in side effect.

Three promises are pinned here, and they pull against each other on purpose:

* **presence is an invariant** — `workspace.ensure` creates the manifest, so a workspace
  has one from birth and `charter workspace reinit` writes the ones that predate this;
* **membership is maintained, branches are not** — a repo cloned into a workspace is
  recorded, with no branch, because a branch in this file carries `snapshot`'s enforce-push
  promise (ADR 0010) and a writer nobody asked for cannot make it;
* **a manifest charter did not write is never overwritten** — the ownership rule the
  harness layer states with `GENERATED_MARKER`, carried here as a digest inside the
  document, because this file is committed and a sidecar beside it would not be.
"""

from __future__ import annotations

import io
import json
import os
import unittest
from contextlib import redirect_stderr
from types import SimpleNamespace
from unittest import mock

from charter import commands, config, contain, workspace
from charter import commands_workspace as cw
from tests._isolation import PersonaIso


def _clone(ws: str, name: str):
    """A directory `workspace.clones` counts as a repo — a `.git` DIRECTORY is git's own
    test (`workspace.is_clone`) and the only one membership depends on. No real repository,
    because nothing here asks a repo a question; the cases that need a branch build one."""
    d = workspace.workspace_dir(ws) / name
    (d / ".git").mkdir(parents=True)
    return d


class ManifestCase(PersonaIso):
    def manifest(self, ws: str) -> dict:
        return json.loads(workspace.manifest_path(ws).read_text())

    def reinit(self, name: str) -> str:
        buf = io.StringIO()
        with redirect_stderr(buf):
            cw.cmd_workspace_reinit(SimpleNamespace(name=name, all=False))
        return buf.getvalue()


class AWorkspaceIsBornWithOne(ManifestCase):
    def test_ensure_writes_the_manifest(self):
        workspace.ensure("alpha")
        self.assertTrue(workspace.manifest_path("alpha").is_file())

    def test_it_carries_the_identity_fields_not_only_repos(self):
        """`repos` is one field among several rather than the point of the file — the
        manifest of a workspace with nothing cloned still says which workspace it is."""
        with mock.patch.dict(os.environ, {"USER": "ada"}):
            workspace.ensure("alpha")
        m = self.manifest("alpha")
        self.assertEqual(m["name"], "alpha")
        self.assertEqual(m["description"], "")
        self.assertEqual(m["repos"], [])
        self.assertEqual(m["updated_by"], "ada")
        self.assertRegex(m["updated_at"], r"^\d{4}-\d\d-\d\dT\d\d:\d\d:\d\d\+00:00$")

    def test_a_shell_with_no_user_records_unknown(self):
        """A `cron` or a container has no `$USER`, and `ensure` runs there too. "unknown"
        rather than an absent field or an empty string, which is `_git_user`'s answer as
        well — a field somebody has to guess the meaning of is worse than one that says
        nobody knows."""
        with mock.patch.dict(os.environ):
            os.environ.pop("USER", None)
            workspace.ensure("alpha")
        self.assertEqual(self.manifest("alpha")["updated_by"], "unknown")

    def test_a_new_workspace_records_no_repos_rather_than_no_manifest(self):
        """*"this workspace exists, is called alpha, and has no repos yet"* is a true and
        useful statement, and it is the one that lets anything else rely on the file."""
        workspace.ensure("alpha")
        self.assertEqual(workspace.read_manifest("alpha")["repos"], [])

    def test_the_manifest_is_charters_to_maintain(self):
        workspace.ensure("alpha")
        self.assertEqual(workspace.manifest_owner("alpha"), "charter")

    def test_a_second_ensure_does_not_rewrite_it(self):
        """Created, never refreshed. `ensure` runs on every launch; a manifest restamped
        each time would put a committed file in every `git status` for the rest of time."""
        workspace.ensure("alpha")
        before = workspace.manifest_path("alpha").read_bytes()
        workspace.ensure("alpha")
        self.assertEqual(workspace.manifest_path("alpha").read_bytes(), before)

    def test_ensure_still_builds_the_workspace_when_the_manifest_cannot_be_written(self):
        """`ensure` is reached from a tab press with no operator waiting — it degrades and
        never raises, and the components after the manifest in `scaffold` still have to
        run. Without the swallow, a `workspaces/` charter cannot write into would take the
        harness layer and the structure marker down with the manifest."""
        wd = workspace.workspace_dir("alpha")
        wd.mkdir(parents=True)
        wd.chmod(0o500)
        self.addCleanup(wd.chmod, 0o700)
        workspace.ensure("alpha")            # must not raise
        wd.chmod(0o700)
        self.assertFalse(workspace.manifest_path("alpha").exists())

    def test_a_manifest_that_could_not_be_written_is_reported_as_missing(self):
        """The honest reading, and what makes `charter workspace reinit` the repair: a
        write that failed leaves the workspace exactly as it was, and `structure_status`
        goes on saying the file is not there."""
        wd = workspace.workspace_dir("alpha")
        wd.mkdir(parents=True)
        wd.chmod(0o500)
        self.addCleanup(wd.chmod, 0o700)
        workspace.ensure("alpha")
        wd.chmod(0o700)
        self.assertIn("workspace.json", workspace.structure_status("alpha")["missing"])


class TheManifestIsPartOfTheLayout(ManifestCase):
    """The backfill's mechanism: the manifest is a required component, so a workspace made
    before #884 flags itself and one command heals every one of them."""

    def test_a_workspace_without_one_is_stale(self):
        workspace.ensure("alpha")
        workspace.manifest_path("alpha").unlink()
        self.assertIn("workspace.json", workspace.structure_status("alpha")["missing"])
        self.assertTrue(workspace.needs_reinit("alpha"))

    def test_reinit_writes_the_missing_manifest(self):
        workspace.ensure("alpha")
        workspace.manifest_path("alpha").unlink()
        cw.cmd_workspace_reinit(SimpleNamespace(name="alpha", all=False))
        self.assertTrue(workspace.manifest_path("alpha").is_file())
        self.assertFalse(workspace.needs_reinit("alpha"))

    def test_reinit_all_backfills_every_workspace_that_lacks_one(self):
        """The command the operator runs once: 12 of 17 workspaces on the plane this was
        measured on."""
        for n in ("alpha", "beta", "gamma"):
            workspace.ensure(n)
            workspace.manifest_path(n).unlink()
        cw.cmd_workspace_reinit(SimpleNamespace(name=None, all=True))
        for n in ("alpha", "beta", "gamma"):
            self.assertTrue(workspace.manifest_path(n).is_file(), n)

    def test_the_backfill_records_membership(self):
        workspace.ensure("alpha")
        workspace.manifest_path("alpha").unlink()
        _clone("alpha", "svc")
        _clone("alpha", "web")
        cw.cmd_workspace_reinit(SimpleNamespace(name="alpha", all=False))
        self.assertEqual([r["name"] for r in self.manifest("alpha")["repos"]],
                         ["svc", "web"])

    def test_the_backfill_pins_no_branch(self):
        """Backfilling a branch would record whatever is checked out at that instant —
        for a workspace mid-work, a scratch branch. A scratch branch written into a
        teammate's restore target is confidently wrong; an empty one is honestly empty,
        and `snapshot` fills it when the operator means to."""
        workspace.ensure("alpha")
        workspace.manifest_path("alpha").unlink()
        _clone("alpha", "svc")
        cw.cmd_workspace_reinit(SimpleNamespace(name="alpha", all=False))
        self.assertEqual(self.manifest("alpha")["repos"], [{"name": "svc"}])

    def test_reinit_leaves_a_manifest_that_is_already_there_alone(self):
        """Additive, like every other component it heals: *"nothing you wrote is
        touched"*."""
        workspace.ensure("alpha")
        workspace.write_manifest("alpha", {"name": "alpha", "description": "the task",
                                           "repos": [{"name": "svc", "branch": "release"}]})
        before = workspace.manifest_path("alpha").read_bytes()
        cw.cmd_workspace_reinit(SimpleNamespace(name="alpha", all=False))
        self.assertEqual(workspace.manifest_path("alpha").read_bytes(), before)

    def test_reinit_says_so_when_the_manifest_could_not_be_written(self):
        """`reinit` reports what it DID, never what it found. `scaffold_manifest` swallows
        its own failure so the rest of the structure still lands, and without this the
        command would print "added workspace.json" over a file that is not there."""
        workspace.ensure("alpha")
        workspace.manifest_path("alpha").unlink()
        wd = workspace.workspace_dir("alpha")
        wd.chmod(0o500)
        self.addCleanup(wd.chmod, 0o700)
        said = self.reinit("alpha")
        wd.chmod(0o700)
        self.assertIn("workspace.json could not be written", said)
        self.assertNotIn("added workspace.json", said)
        self.assertNotIn("Reinitialized", said,
                         "a workspace whose only gap could not be filled was reported healed")
        self.assertNotIn("Up to date", said,
                         "a repair command that contradicts its own error two lines up")

    def test_a_backfilled_live_workspace_is_told_to_commit_it(self):
        """`workspace.json` is the first path in the managed LIVE block, so a backfill that
        healed it and said nothing would leave the manifest this change exists to share
        sitting uncommitted on one machine."""
        (config.ROOT / ".gitignore").write_text("/workspaces/*/*\n!/workspaces/.gitkeep\n")
        workspace.ensure("alpha")
        workspace.set_live("alpha", True)
        workspace.manifest_path("alpha").unlink()
        self.assertIn("charter workspace save", self.reinit("alpha"))


class MembershipIsMaintained(ManifestCase):
    def test_a_cloned_repo_is_recorded(self):
        workspace.ensure("alpha")
        self.assertEqual(workspace.record_members("alpha", ["svc"]), "recorded")
        self.assertEqual(self.manifest("alpha")["repos"], [{"name": "svc"}])

    def test_no_branch_is_ever_recorded(self):
        """The line ADR 0010 draws. A branch in this file promises `restore` can check it
        out on another machine — `snapshot` refuses while a repo has unpushed work,
        precisely so the promise holds. Nothing that writes without being asked can make
        it, so nothing that writes without being asked writes a branch."""
        workspace.ensure("alpha")
        workspace.record_members("alpha", ["svc"])
        self.assertNotIn("branch", self.manifest("alpha")["repos"][0])

    def test_recording_the_same_repo_twice_rewrites_nothing(self):
        """Idempotent and silent: every `charter clone` into a workspace already holding
        the repo would otherwise restamp `updated_at` and dirty a committed file."""
        workspace.ensure("alpha")
        workspace.record_members("alpha", ["svc"])
        before = workspace.manifest_path("alpha").read_bytes()
        self.assertEqual(workspace.record_members("alpha", ["svc"]), "unchanged")
        self.assertEqual(workspace.manifest_path("alpha").read_bytes(), before)

    def test_a_recorded_branch_survives_a_later_clone(self):
        """The two halves of the file have different lifetimes and neither erases the
        other: `snapshot` pins the branches, a clone adds a member."""
        workspace.ensure("alpha")
        workspace.write_manifest("alpha", {"name": "alpha", "description": "",
                                           "repos": [{"name": "svc", "branch": "release"}]})
        workspace.record_members("alpha", ["web"])
        rows = {r["name"]: r.get("branch") for r in self.manifest("alpha")["repos"]}
        self.assertEqual(rows, {"svc": "release", "web": None})

    def test_a_repo_that_is_not_on_this_machine_is_not_dropped(self):
        """**Absence is not removal**, and that is measured rather than cautious:
        `restore --on-demand` deliberately leaves every recorded repo uncloned, and
        `restore` skips the ones this machine cannot reach. A reconcile against the
        directory would erase the restore target of the teammate this file exists for,
        on their first launch — and their next `charter save` would push the erasure."""
        workspace.ensure("alpha")
        workspace.write_manifest("alpha", {"name": "alpha", "description": "",
                                           "repos": [{"name": "svc", "branch": "release"},
                                                     {"name": "gone", "branch": "main"}]})
        workspace.record_members("alpha", ["web"])
        self.assertEqual([r["name"] for r in self.manifest("alpha")["repos"]],
                         ["gone", "svc", "web"])

    def test_membership_is_restamped_with_who_and_when(self):
        workspace.ensure("alpha")
        workspace.write_manifest("alpha", {"name": "alpha", "repos": [],
                                           "updated_at": "1999-01-01T00:00:00+00:00",
                                           "updated_by": "someone else"})
        with mock.patch.dict(os.environ, {"USER": "ada"}):
            workspace.record_members("alpha", ["svc"])
        self.assertEqual(self.manifest("alpha")["updated_by"], "ada")
        self.assertNotEqual(self.manifest("alpha")["updated_at"], "1999-01-01T00:00:00+00:00")

    def test_a_repo_named_twice_is_recorded_once(self):
        workspace.ensure("alpha")
        workspace.record_members("alpha", ["svc", "svc"])
        self.assertEqual(self.manifest("alpha")["repos"], [{"name": "svc"}])

    def test_a_nameless_repo_is_not_a_member(self):
        """A row with no name is a row `restore` cannot act on and `merge_repo_rows`
        already skips — recording one would put an entry in a committed file that every
        reader has to know to ignore."""
        workspace.ensure("alpha")
        self.assertEqual(workspace.record_members("alpha", [""]), "unchanged")
        self.assertEqual(self.manifest("alpha")["repos"], [])

    def test_the_rows_are_sorted_however_they_arrived(self):
        """One row order for one membership, so a clone, a snapshot and a backfill produce
        the same diff rather than three."""
        workspace.ensure("alpha")
        workspace.record_members("alpha", ["svc"])
        workspace.record_members("alpha", ["api"])
        self.assertEqual([r["name"] for r in self.manifest("alpha")["repos"]],
                         ["api", "svc"])

    def test_rows_that_are_not_rows_are_dropped_rather_than_crashed_on(self):
        """A committed manifest is an untrusted document — `restore` says so in as many
        words — and the stamp is not a secret, so `repos: [1, 2, 3]` carrying a matching
        digest is a file somebody can hand this plane."""
        workspace.ensure("alpha")
        workspace.write_manifest("alpha", {"name": "alpha",
                                           "repos": ["svc", 3, None, {"branch": "main"}]})
        self.assertEqual(workspace.record_members("alpha", ["svc"]), "recorded")
        self.assertEqual(self.manifest("alpha")["repos"], [{"name": "svc"}])

    def test_membership_cannot_be_recorded_where_no_manifest_can_be_created(self):
        """`blocked`, not `recorded`: the caller is told the fact did not land rather than
        being handed a tick over a file that is not there."""
        wd = workspace.workspace_dir("alpha")
        wd.mkdir(parents=True)
        wd.chmod(0o500)
        self.addCleanup(wd.chmod, 0o700)
        self.assertEqual(workspace.record_members("alpha", ["svc"]), "blocked")


class CloningRecordsWhatItCloned(ManifestCase):
    """End to end through `cmd_clone`, which is the structural change: a repo cloned into a
    workspace is a member of it, and a manifest that learns that only when somebody runs
    `snapshot` describes the workspaces nobody shared."""

    def clone(self, ws: str, repo: str, status: str) -> str:
        dest = workspace.workspace_dir(ws) / repo
        r = {"name": repo, "default_branch": "main", "forge": "github",
             "path_with_namespace": f"g/{repo}"}
        result = {"repo": r, "dest": dest, "status": status,
                  "forge": SimpleNamespace(cli="gh"), "stderr": "", "reason": "no"}
        buf = io.StringIO()
        with mock.patch.object(commands, "_clone_one", lambda rr, wd: result), \
                mock.patch.object(commands.workspace, "banner", lambda *a, **k: None), \
                mock.patch.object(commands.inventory, "load", lambda: {}), \
                mock.patch.object(commands.inventory, "repos", lambda d=None: [r]), \
                mock.patch.object(commands, "_resolve_targets", lambda a, d: [r]), \
                redirect_stderr(buf):
            commands.cmd_clone(SimpleNamespace(repos=[repo], workspace=ws))
        return buf.getvalue()

    def test_the_repo_it_cloned_is_a_member(self):
        workspace.ensure("alpha")
        _clone("alpha", "svc")
        self.clone("alpha", "svc", "ok")
        self.assertEqual(self.manifest("alpha")["repos"], [{"name": "svc"}])

    def test_a_repo_that_was_already_there_is_a_member_too(self):
        """`exists` is the ordinary outcome of re-cloning into a workspace, and it is as
        much a statement of membership as a fresh clone — a manifest that only learned
        from first clones would never catch up on a workspace built before this."""
        workspace.ensure("alpha")
        _clone("alpha", "svc")
        self.clone("alpha", "svc", "exists")
        self.assertEqual(self.manifest("alpha")["repos"], [{"name": "svc"}])

    def test_a_clone_that_failed_is_not_a_member(self):
        """Membership is what the workspace HAS. A repo whose clone failed — no access, no
        network — is not in it, and recording it would send `restore` after a repo this
        machine could not reach in the first place."""
        workspace.ensure("alpha")
        self.clone("alpha", "svc", "failed")
        self.assertEqual(self.manifest("alpha")["repos"], [])

    def test_it_says_a_hand_written_manifest_was_left_alone(self):
        """The invariant is quietly untrue there until somebody acts, so the person at the
        keyboard is told rather than left to find out from a restore."""
        workspace.workspace_dir("alpha").mkdir(parents=True)
        workspace.manifest_path("alpha").write_text('{"name": "alpha", "repos": []}\n')
        _clone("alpha", "svc")
        said = self.clone("alpha", "svc", "ok")
        self.assertIn("was not written by charter", said)

    def test_a_clone_that_landed_nothing_says_nothing_about_the_manifest(self):
        """The warning above is about a repo that could not be recorded. With no repo to
        record there is nothing to warn about, and a line that fires on a failed clone
        blames the manifest for the network."""
        workspace.workspace_dir("alpha").mkdir(parents=True)
        workspace.manifest_path("alpha").write_text('{"name": "alpha", "repos": []}\n')
        said = self.clone("alpha", "svc", "failed")
        self.assertNotIn("was not written by charter", said)


class AManifestCharterDidNotWriteIsNeverOverwritten(ManifestCase):
    """The ownership rule `GENERATED_MARKER` states for the harness layer, in the one
    spelling a committed JSON document can carry."""

    def hand_written(self, ws: str, text: str = '{"name": "alpha", "repos": []}\n') -> bytes:
        workspace.workspace_dir(ws).mkdir(parents=True, exist_ok=True)
        workspace.manifest_path(ws).write_text(text)
        return text.encode()

    def test_a_hand_written_manifest_is_the_operators(self):
        self.hand_written("alpha")
        self.assertEqual(workspace.manifest_owner("alpha"), "operator")

    def test_ensure_leaves_it_exactly_as_it_found_it(self):
        before = self.hand_written("alpha")
        workspace.ensure("alpha")
        self.assertEqual(workspace.manifest_path("alpha").read_bytes(), before)

    def test_membership_is_not_recorded_into_it(self):
        before = self.hand_written("alpha")
        self.assertEqual(workspace.record_members("alpha", ["svc"]), "operator")
        self.assertEqual(workspace.manifest_path("alpha").read_bytes(), before)

    def test_an_edited_manifest_stops_being_charters(self):
        """A digest rather than a flag, for `_charter_owned`'s reason: a file whose content
        no longer matches what charter wrote is the operator's now, whoever created it."""
        workspace.ensure("alpha")
        doc = workspace.read_manifest("alpha")
        doc["description"] = "mine now"
        workspace.manifest_path("alpha").write_text(json.dumps(doc, indent=2) + "\n")
        self.assertEqual(workspace.manifest_owner("alpha"), "operator")

    def test_the_stamp_travels_in_the_committed_file(self):
        """Why the marker is a key and not a `.charter-generated` sidecar: everything
        beside a manifest is gitignored and the manifest is not, so a teammate who pulls
        the plane would get the file with none of charter's bookkeeping — and every shared
        manifest would read as hand-written on every machine but the one that wrote it."""
        workspace.ensure("alpha")
        raw = workspace.manifest_path("alpha").read_text()
        self.assertIn(workspace.MANIFEST_MARKER, raw)
        pulled = json.loads(raw)                       # what git would hand a teammate
        workspace.manifest_path("beta").parent.mkdir(parents=True, exist_ok=True)
        workspace.manifest_path("beta").write_text(json.dumps(pulled, indent=2) + "\n")
        self.assertEqual(workspace.manifest_owner("beta"), "charter")

    def test_reindenting_a_manifest_does_not_take_it_away_from_charter(self):
        """The digest is over the document, not the bytes: a value charter wrote is still
        charter's however the file is formatted."""
        workspace.ensure("alpha")
        doc = workspace.read_manifest("alpha")
        workspace.manifest_path("alpha").write_text(json.dumps(doc))   # one line, no indent
        self.assertEqual(workspace.manifest_owner("alpha"), "charter")

    def test_a_manifest_that_is_not_even_an_object_is_the_operators(self):
        """`[]` is valid JSON, `read_manifest` hands back whatever the file holds, and the
        digest is not a secret — so a list here has to be told it is somebody else's rather
        than reaching `.get` and taking the command down."""
        before = self.hand_written("alpha", "[]\n")
        self.assertEqual(workspace.manifest_owner("alpha"), "operator")
        workspace.record_members("alpha", ["svc"])
        self.assertEqual(workspace.manifest_path("alpha").read_bytes(), before)

    def test_a_manifest_that_cannot_be_stat_ed_is_the_operators(self):
        """`Path.exists` raises EACCES on Linux, where pathlib does not swallow it, and
        answers False on macOS — `workspace._unreadable`'s divergence, at the one call
        where guessing wrong means writing over somebody's file."""
        from pathlib import Path
        real = Path.exists
        target = workspace.manifest_path("alpha")

        def strict(self_, *a, **kw):
            if self_ == target:
                raise PermissionError(13, "Permission denied", str(self_))
            return real(self_, *a, **kw)

        Path.exists = strict
        self.addCleanup(setattr, Path, "exists", real)
        self.assertEqual(workspace.manifest_owner("alpha"), "operator")

    def test_reordering_the_keys_does_not_take_it_away_from_charter(self):
        """The digest is over the document, not over one serialisation of it. A manifest
        round-tripped through a tool that sorts keys differently is still charter's."""
        workspace.ensure("alpha")
        doc = workspace.read_manifest("alpha")
        flipped = {k: doc[k] for k in reversed(list(doc))}
        workspace.manifest_path("alpha").write_text(json.dumps(flipped, indent=2) + "\n")
        self.assertEqual(workspace.manifest_owner("alpha"), "charter")

    def test_a_manifest_charter_cannot_read_is_not_a_missing_one(self):
        """"Present but unparseable" must answer `operator`, never `absent`: treating it as
        absent would make the automatic writers create a fresh manifest over exactly the
        hand-made file this rule exists to protect."""
        before = self.hand_written("alpha", "this is not json\n")
        self.assertEqual(workspace.manifest_owner("alpha"), "operator")
        workspace.ensure("alpha")
        workspace.record_members("alpha", ["svc"])
        self.assertEqual(workspace.manifest_path("alpha").read_bytes(), before)

    def test_snapshot_still_rewrites_it_because_somebody_asked(self):
        """The rule governs the writes nobody asked for. `charter workspace snapshot` is
        the operator saying *rewrite this file*, and #884 leaves it exactly as it was."""
        self.hand_written("alpha")
        _clone("alpha", "svc")
        with mock.patch.object(cw, "_restore_blockers", lambda n: []), \
                mock.patch.object(cw, "_repo_branch", lambda d: "release"):
            rc = cw.cmd_workspace_snapshot(SimpleNamespace(name="alpha", force=False,
                                                           description=None))
        self.assertEqual(rc, 0)
        self.assertEqual(self.manifest("alpha")["repos"], [{"name": "svc",
                                                            "branch": "release"}])

    def test_a_snapshot_hands_the_file_back_to_charter(self):
        """So the clone AFTER a snapshot is recorded. A deliberate rewrite that left the
        file unstamped would make charter treat its own document as the operator's, and
        membership would silently stop being maintained on exactly the workspaces somebody
        cared enough to snapshot."""
        workspace.ensure("alpha")
        _clone("alpha", "svc")
        with mock.patch.object(cw, "_restore_blockers", lambda n: []), \
                mock.patch.object(cw, "_repo_branch", lambda d: "release"):
            cw.cmd_workspace_snapshot(SimpleNamespace(name="alpha", force=False,
                                                      description=None))
        self.assertEqual(workspace.manifest_owner("alpha"), "charter")
        self.assertEqual(workspace.record_members("alpha", ["web"]), "recorded")


class TheWriteIsAtomic(ManifestCase):
    """A reader of this file can be `git add`, so half a manifest is not a glitch somebody
    re-runs past — it is half a manifest a teammate pulls."""

    def test_the_previous_manifest_survives_a_write_that_fails(self):
        workspace.ensure("alpha")
        workspace.write_manifest("alpha", {"name": "alpha", "description": "",
                                           "repos": [{"name": "svc", "branch": "release"}]})
        before = workspace.manifest_path("alpha").read_bytes()
        with mock.patch("os.replace", side_effect=OSError(28, "No space left on device")):
            self.assertEqual(workspace.record_members("alpha", ["web"]), "blocked")
        self.assertEqual(workspace.manifest_path("alpha").read_bytes(), before)

    def test_no_temporary_file_is_left_beside_it(self):
        workspace.ensure("alpha")
        workspace.record_members("alpha", ["svc"])
        litter = [p.name for p in workspace.workspace_dir("alpha").iterdir()
                  if p.name.endswith(".tmp")]
        self.assertEqual(litter, [])

    def test_a_redirected_manifest_path_is_refused_rather_than_followed(self):
        """`contain.writable` first and by itself — the atomic shape must not become a way
        around containment (#328). `os.replace` onto a symlink replaces the link, so a
        refusal here is what keeps the write inside the plane at all."""
        outside = self.tmp / "outside.json"
        outside.write_text("{}\n")
        workspace.workspace_dir("alpha").mkdir(parents=True)
        workspace.manifest_path("alpha").symlink_to(outside)
        with self.assertRaises(contain.Refused):
            workspace.write_manifest("alpha", {"repos": []})
        self.assertEqual(outside.read_text(), "{}\n")


class RestoringAWorkspaceWhoseBranchesAreUnpinned(ManifestCase):
    """The backfill's output is a manifest with membership and no branches, and `restore`
    is what reads it. Without this it asked `git checkout ''` for every repo and reported
    a workspace it had in fact restored as one it could not."""

    def test_an_unpinned_repo_is_restored_by_being_there(self):
        workspace.ensure("alpha")
        _clone("alpha", "svc")
        workspace.write_manifest("alpha", {"name": "alpha", "repos": [{"name": "svc"}]})
        buf = io.StringIO()
        with redirect_stderr(buf):
            rc = cw.cmd_workspace_restore(SimpleNamespace(name="alpha", on_demand=False))
        self.assertEqual(rc, 0)
        self.assertIn("Restored 1/1", buf.getvalue())

    def test_it_is_named_as_unpinned_rather_than_as_a_branch(self):
        workspace.ensure("alpha")
        _clone("alpha", "svc")
        workspace.write_manifest("alpha", {"name": "alpha", "repos": [{"name": "svc"}]})
        buf = io.StringIO()
        with redirect_stderr(buf):
            cw.cmd_workspace_restore(SimpleNamespace(name="alpha", on_demand=False))
        self.assertIn("no branch recorded", buf.getvalue())

    def test_a_branch_that_is_an_option_in_disguise_is_still_refused(self):
        """#334's guard, asked of the value git is actually handed. `" -b"` does not
        `startswith("-")` and `git checkout` reads the argument it is given, so the reading
        and the guard have to be the same string."""
        workspace.ensure("alpha")
        _clone("alpha", "svc")
        workspace.write_manifest("alpha", {"name": "alpha",
                                           "repos": [{"name": "svc", "branch": " -b"}]})
        buf = io.StringIO()
        with redirect_stderr(buf):
            cw.cmd_workspace_restore(SimpleNamespace(name="alpha", on_demand=False))
        self.assertIn("may not begin with '-'", buf.getvalue())

    def test_the_on_demand_listing_survives_a_row_with_no_branch(self):
        """`r['branch']` was a KeyError waiting for the first manifest charter wrote
        itself."""
        workspace.ensure("alpha")
        workspace.write_manifest("alpha", {"name": "alpha", "repos": [{"name": "svc"}]})
        buf = io.StringIO()
        with redirect_stderr(buf):
            rc = cw.cmd_workspace_restore(SimpleNamespace(name="alpha", on_demand=True))
        self.assertEqual(rc, 0)
        self.assertIn("svc", buf.getvalue())


if __name__ == "__main__":
    unittest.main()
