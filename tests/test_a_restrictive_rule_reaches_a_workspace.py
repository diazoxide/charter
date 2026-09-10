"""A `charter guard ask` rule is in force in a chat in a workspace — #942.

`charter guard ask <pattern>` writes the plane's own `.claude/settings.json` and tells the
operator it *"applies to everyone on this repo"*. A framed chat runs with its cwd at
`workspaces/<ws>/`, Claude Code reads the shared project settings from the session's own
directory and does not walk up, and the file charter generates there carried `enabledPlugins`
and `env` and never `permissions`. So a force-prompt rule — a SAFETY rule — was silently not
in force in the chats where the guarded command actually runs, and every `doctor` row about
that chat was green.

**The fix is the restrictive half only.** `permissions.ask` and `permissions.deny` travel;
`permissions.allow` still does not. That keeps `claude_code.WORKSPACE_KEYS`' reasoning
exactly as it was — *"copying a grant sideways into a directory nobody granted it in puts a
permission in force where no one clicked for it"* — because a restrictive rule is the
opposite of a grant: it adds a prompt or a refusal, it never puts one in force.

**Where the machine-local half goes was measured, not assumed** — Claude Code 2.1.267, git
2.50.1, `claude -p` against a `deny` on `mkdir` in throwaway repositories:

    git-root .claude/settings.local.json, session in a subdirectory      blocked
    subdirectory's own .claude/settings.local.json, session there         blocked
    git-root .claude/settings.json, session in a subdirectory              ran
    outer repo's settings.local.json, session in a nested clone            ran
    the clone's own settings.local.json, session in the clone              blocked
    main checkout's settings.local.json, session in a linked worktree      blocked

So the local file is read at the GIT ROOT (the main checkout, for a worktree) as well as in
the starting directory, while the shared file stays cwd-only. A workspace directory sits
inside the plane's repository and already reads the plane's own local file; a clone is a
git root of its own and does not. The generated local file exists for clones only — and in
a clone it is the same file Claude Code saves "Yes, and don't ask again" into, which is the
whole of the co-writing half below.

Every test here writes only into a `PersonaIso` tmp plane — see `_planeguard` for what
touching a real `workspaces/` has cost before.
"""

from __future__ import annotations

import io
import json
import os
import shutil
import subprocess
import unittest
from contextlib import redirect_stderr, redirect_stdout
from pathlib import Path
from types import SimpleNamespace
from unittest import mock

from charter import commands, commands_workspace, config, doctor, workspace
from charter.harness import claude_code, registry

from tests import _isolation
from tests.test_a_clone_gets_the_layer_and_hides_it import _git, _repo
from tests.test_a_workspace_carries_charters_layer import _plane_settings

#: The two generated paths, spelled by hand. Imported from `claude_code` they would agree
#: with whatever value the constants take.
SHARED = ".claude/settings.json"
LOCAL = ".claude/settings.local.json"

_PRETOOLUSE = {"hooks": {"PreToolUse": [{"matcher": "Bash", "hooks": [
    {"type": "command", "command": "charter hook pretooluse"}]}]}}


def _plane_local(root: Path, **buckets) -> Path:
    """The plane's MACHINE-LOCAL settings — what `charter guard ask --local` writes.

    Spelled out here rather than driven through `add_permission_rule`, because half these
    cases are about a plane an operator wrote by hand: charter never writes a `deny` at all
    (`commands.py`'s own note), so a fixture that could only produce what charter writes
    could not state the case at all.
    """
    d = root / ".claude"
    d.mkdir(parents=True, exist_ok=True)
    p = d / "settings.local.json"
    p.write_text(json.dumps({"permissions": buckets}, indent=2) + "\n")
    return p


def _status(tree: Path) -> str:
    """`git status` in *tree* with the machine's global ignore taken out of the answer.

    `core.excludesFile` pointed at nothing, and `test_the_status_cannot_be_fooled_by_a_
    global_ignore` proves it holds. The suite already moves `$XDG_CONFIG_HOME` into a
    sandbox, which is where git looks when that key is unset — but the machine this was
    fixed on lists `**/.claude/settings.local.json` in `~/.config/git/ignore`, which is
    exactly why the live check before this fix could not see the defect. Said at the call
    rather than trusted from the environment.
    """
    return _git(tree, "-c", "core.excludesFile=/dev/null",
                "status", "--porcelain", "-uall").stdout


def _as_the_harness_would(path: Path, rule: str) -> str:
    """Append an `allow` the way "Yes, and don't ask again" does; return the new text.

    Into THIS file because that is where the harness saves a standing approval for a session
    rooted in a checkout: the git root's `.claude/settings.local.json` (documented for
    2.1.211 and later), and a clone's git root is the clone.
    """
    doc = json.loads(path.read_text())
    doc.setdefault("permissions", {}).setdefault("allow", []).append(rule)
    text = json.dumps(doc, indent=2) + "\n"
    path.write_text(text)
    return text


class PlaneWithRestrictions(_isolation.PersonaIso):
    """A real plane whose committed settings hold one of each bucket, and one workspace."""

    def setUp(self) -> None:
        super().setUp()
        # The tripwire the whole layer suite is written under: if `PersonaIso` ever stops
        # repointing derived paths at a throwaway tree, every write below lands in a real
        # plane.
        self.assertIn("edm-test-", str(config.STATE_DIR))
        _isolation.make_plane(self)
        _plane_settings(config.ROOT, permissions={
            "allow": ["Bash(ls:*)"],
            "ask": ["Bash(terraform apply *)"],
            "deny": ["Bash(rm -rf /)"],
        })
        self.ws = "north"
        workspace.ensure(self.ws)

    def generated(self, rel: str = SHARED, name: str | None = None) -> Path:
        return workspace.workspace_dir(name or self.ws) / rel

    def doc(self, rel: str = SHARED, name: str | None = None) -> dict:
        return json.loads(self.generated(rel, name).read_text())

    def checkout(self, name: str = "svc", real: bool = False) -> Path:
        """A checkout inside the workspace. `real` makes a git repository with a commit,
        which `git status` needs; otherwise a bare `.git` directory, which is all the
        layer itself looks at."""
        d = workspace.workspace_dir(self.ws) / name
        if real:
            _repo(d)
        else:
            (d / ".git").mkdir(parents=True)
        return d

    def excludes(self, tree: Path) -> str:
        p = workspace.git_exclude_file(tree)
        return p.read_text() if p is not None and p.is_file() else ""

    def invoke(self, fn, **kw) -> tuple[int, str]:
        out, err = io.StringIO(), io.StringIO()
        with redirect_stdout(out), redirect_stderr(err):
            rc = fn(SimpleNamespace(**kw))
        return rc, out.getvalue() + err.getvalue()

    def at_the_plane(self) -> None:
        self.addCleanup(os.chdir, os.getcwd())
        os.chdir(config.ROOT)

    def reinit_said(self) -> list[str]:
        said: list[str] = []
        with mock.patch.object(commands_workspace.util, "ok", side_effect=said.append), \
             mock.patch.object(commands_workspace.util, "warn", side_effect=said.append), \
             mock.patch.object(commands_workspace.util, "err", side_effect=said.append), \
             mock.patch.object(commands_workspace.util, "info", side_effect=lambda m: None):
            commands_workspace.cmd_workspace_reinit(SimpleNamespace(name=self.ws, all=False))
        return said


class WhatTravelsIntoAWorkspace(PlaneWithRestrictions):
    def test_the_planes_ask_rule_is_in_force_in_the_workspace(self):
        """The whole of #942: this is the rule the operator was told applies to everyone."""
        self.assertEqual(self.doc()["permissions"]["ask"], ["Bash(terraform apply *)"])

    def test_a_hand_written_deny_travels_too(self):
        """Charter never writes a `deny` itself, so every one of them is an operator's own
        deliberate choice — and it was being dropped by the same key filter."""
        self.assertEqual(self.doc()["permissions"]["deny"], ["Bash(rm -rf /)"])

    def test_a_grant_still_never_travels(self):
        """`WORKSPACE_KEYS`' reasoning, unchanged: nothing is put in force in a directory
        nobody clicked for it in."""
        self.assertNotIn("allow", self.doc()["permissions"])
        self.assertNotIn("Bash(ls:*)", self.generated().read_text())

    def test_the_two_mirrored_keys_are_still_there_beside_it(self):
        self.assertEqual(sorted(self.doc()), ["enabledPlugins", "env", "permissions"])

    def test_a_plane_whose_permissions_are_all_grants_declares_none_at_all(self):
        """An empty `permissions` block in the generated file would look like policy."""
        _plane_settings(config.ROOT, permissions={"allow": ["Bash(ls:*)"]})
        workspace.wire_harnesses(self.ws)
        self.assertNotIn("permissions", self.doc())

    def test_a_bucket_that_is_not_a_list_is_read_past_rather_than_mirrored(self):
        """A `permissions` block of the wrong shape is somebody's deliberate structure.
        `add_permission_rule` refuses to WRITE into one; a mirror has less standing still,
        and must not put a shape the host cannot read into a second file."""
        _plane_settings(config.ROOT, permissions={"ask": "Bash(terraform apply *)",
                                                  "deny": ["Bash(rm -rf /)"]})
        workspace.wire_harnesses(self.ws)
        self.assertEqual(self.doc()["permissions"], {"deny": ["Bash(rm -rf /)"]})

    def test_the_rules_arrive_at_the_paths_the_host_actually_reads(self):
        """Not a charter-shaped sidecar: the files Claude Code resolves for a session, which
        is what makes the rule in force rather than merely recorded."""
        self.assertEqual(claude_code.WORKSPACE_SETTINGS, SHARED)
        self.assertEqual(claude_code.CHECKOUT_LOCAL_SETTINGS, LOCAL)
        self.assertEqual(claude_code._PROJECT_SETTINGS, (SHARED, LOCAL))


class AWorkspaceDirectoryReadsThePlanesOwnLocalFile(PlaneWithRestrictions):
    """Measured: the local file is read at the git root, and `workspaces/<ws>/` is inside the
    plane's repository. So the plane's `--local` rules are already in force there, and a
    generated copy would be at best a leftover — and at worst a second file doctor warns
    about for no reason."""

    def setUp(self) -> None:
        super().setUp()
        _plane_local(config.ROOT, ask=["Bash(charter change land *)"])
        workspace.wire_harnesses(self.ws)

    def test_no_local_file_is_generated_in_a_workspace_directory(self):
        self.assertFalse(self.generated(LOCAL).exists())

    def test_the_local_rule_does_not_leak_into_the_generated_shared_file(self):
        """The two plane files differ in blast radius, and folding one into the other would
        publish a personal decision to every reader of the generated file."""
        self.assertNotIn("charter change land", self.generated().read_text())

    def test_doctor_does_not_warn_about_a_local_rule_a_workspace_already_reads(self):
        r = doctor.check_workspace_harness()
        self.assertEqual(r.status, doctor.OK, f"{r.detail} {r.hint}")

    def test_a_local_file_generated_by_the_first_version_of_this_fix_is_withdrawn(self):
        """The first commit of #942 wrote one here. It never shipped, but a plane that ran
        the branch holds one, and charter withdraws a file it wrote and no longer wants."""
        text = json.dumps({"permissions": {"ask": ["Bash(charter change land *)"]}},
                          indent=2) + "\n"
        self.generated(LOCAL).write_text(text)
        marker_path = workspace.workspace_dir(self.ws) / workspace.GENERATED_MARKER
        marker = json.loads(marker_path.read_text())
        marker[LOCAL] = workspace.content_digest(text)
        marker_path.write_text(json.dumps(marker))
        rows = dict(workspace.wire_harnesses(self.ws))
        self.assertEqual(rows[LOCAL], "removed")
        self.assertFalse(self.generated(LOCAL).exists())


class TheHostReadsTheLocalFileAtTheGitRoot(PlaneWithRestrictions):
    """`doctor._settings_files` — the list every "is it wired" row reads — as measured.

    It listed `.claude/settings.local.json` from the session's own directory only, which
    has been wrong since Claude Code 2.1.211 and predates #942: in a workspace chat a hook or
    a plugin declared only in the plane's local file went uncounted.
    """

    def setUp(self) -> None:
        super().setUp()
        self.plane = Path(config.ROOT).resolve()
        subprocess.run(["git", "init", "-q", str(self.plane)], check=True,
                       stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
        self.here = workspace.workspace_dir(self.ws).resolve()
        self.addCleanup(os.chdir, os.getcwd())
        home = self.tmp / "home"
        (home / ".claude").mkdir(parents=True, exist_ok=True)
        self.enterContext(mock.patch.dict(os.environ, {"HOME": str(home)}))
        os.environ.pop("CLAUDE_PLUGIN_ROOT", None)

    def listed(self) -> list[str]:
        return [str(p) for p in doctor._settings_files()]

    def test_a_session_in_a_workspace_directory_reads_the_planes_local_file(self):
        os.chdir(self.here)
        self.assertIn(str(self.plane / LOCAL), self.listed())

    def test_the_file_in_its_own_directory_is_still_read(self):
        """Measured as well: a local file in the starting directory still applies — the
        docs call it what an earlier version left there."""
        os.chdir(self.here)
        self.assertIn(str(self.here / LOCAL), self.listed())

    def test_a_clone_with_its_own_git_root_does_not_read_the_planes(self):
        clone = self.checkout("api", real=True).resolve()
        os.chdir(clone)
        listed = self.listed()
        self.assertNotIn(str(self.plane / LOCAL), listed)
        self.assertEqual(listed.count(str(clone / LOCAL)), 1)

    def test_the_plane_root_is_not_listed_twice(self):
        """At the git root the two rules name one file, and `check_guard_wired` counts
        declarations — the same file twice would read as the guard declared twice."""
        os.chdir(self.plane)
        self.assertEqual(self.listed().count(str(self.plane / LOCAL)), 1)

    def test_outside_any_repository_the_list_is_what_it_was(self):
        shutil.rmtree(self.plane / ".git")
        os.chdir(self.here)
        self.assertNotIn(str(self.plane / LOCAL), self.listed())

    def test_a_guard_declared_only_in_the_planes_local_file_counts_in_a_workspace(self):
        """The pre-existing false warning, closed: the host runs this hook in a workspace
        chat, and the row said nothing wired it."""
        (self.plane / LOCAL).write_text(json.dumps(_PRETOOLUSE))
        os.chdir(self.here)
        self.assertEqual(doctor.check_guard_wired().status, doctor.OK)


class ACheckoutGetsTheLocalFile(PlaneWithRestrictions):
    """`workspaces/<ws>/<repo>/` is a git root of its own, so the plane's local file stops
    reaching a session there — measured. This is where the generated local file lives."""

    def setUp(self) -> None:
        super().setUp()
        _plane_local(config.ROOT, ask=["Bash(charter change land *)"])
        self.clone = self.checkout()
        workspace.wire_harnesses(self.ws)

    def test_the_committed_restrictions_reach_the_checkout(self):
        doc = json.loads((self.clone / SHARED).read_text())
        self.assertEqual(doc["permissions"], {"ask": ["Bash(terraform apply *)"],
                                              "deny": ["Bash(rm -rf /)"]})

    def test_the_machine_local_ones_reach_it_in_a_machine_local_file(self):
        doc = json.loads((self.clone / LOCAL).read_text())
        self.assertEqual(doc, {"permissions": {"ask": ["Bash(charter change land *)"]}})

    def test_no_grant_reaches_a_repository_nobody_granted_it_in(self):
        self.assertNotIn("Bash(ls:*)", (self.clone / SHARED).read_text())

    def test_a_local_grant_does_not_travel_either(self):
        _plane_local(config.ROOT, allow=["Bash(gh pr merge *)"])
        workspace.wire_harnesses(self.ws)
        self.assertFalse((self.clone / LOCAL).exists())

    def test_a_plane_with_only_a_local_rule_still_reaches_a_checkout(self):
        (config.ROOT / SHARED).unlink()
        other = self.checkout("other")
        workspace.wire_harnesses(self.ws)
        self.assertFalse((other / SHARED).exists())
        self.assertIn("charter change land", (other / LOCAL).read_text())

    def test_both_files_are_hidden_from_the_checkouts_own_git_status(self):
        body = self.excludes(self.clone)
        self.assertIn(f"/{SHARED}", body)
        self.assertIn(f"/{LOCAL}", body)

    def test_removing_the_layer_takes_the_local_file_with_it(self):
        workspace.unwire_guest(self.clone)
        self.assertFalse((self.clone / LOCAL).exists())

    def test_the_harness_declares_that_it_writes_into_that_file_itself(self):
        self.assertEqual(claude_code.ClaudeCodeHarness().cowritten, (LOCAL,))


class ARuleThePlaneDropsIsWithdrawn(PlaneWithRestrictions):
    """The mirror runs both ways, and it has to.

    `charter guard` has no remove verb: a rule goes by editing the plane's settings. A plane
    that drops its last `--local` rule stops wanting a whole generated file, and a mirror
    that only ever adds would put a restriction in force in every checkout with no way to
    lift it.
    """

    def setUp(self) -> None:
        super().setUp()
        _plane_local(config.ROOT, ask=["Bash(charter change land *)"])
        self.clone = self.checkout()
        workspace.wire_harnesses(self.ws)
        self.assertTrue((self.clone / LOCAL).is_file(),
                        "fixture never mirrored the file this case is about")

    def test_deleting_the_planes_local_file_lifts_the_rule_from_the_checkout(self):
        (config.ROOT / LOCAL).unlink()
        rows = dict(workspace.wire_harnesses(self.ws))
        self.assertEqual(rows[f"svc/{LOCAL}"], "removed")
        self.assertFalse((self.clone / LOCAL).exists())

    def test_dropping_the_last_restrictive_rule_lifts_it_too(self):
        """The bucket emptied rather than the file deleted — the ordinary edit."""
        _plane_local(config.ROOT, allow=["Bash(gh pr merge *)"])
        workspace.wire_harnesses(self.ws)
        self.assertFalse((self.clone / LOCAL).exists())

    def test_a_generated_file_somebody_edited_is_never_deleted(self):
        """The destructive direction. `unwire_guest`'s rule verbatim: charter withdraws
        only a file whose content still matches the digest it recorded."""
        (config.ROOT / SHARED).unlink()
        mine = '{"env": {"MINE": "1"}}\n'
        self.generated().write_text(mine)
        rows = dict(workspace.wire_harnesses(self.ws))
        self.assertEqual(self.generated().read_text(), mine)
        self.assertNotIn(SHARED, rows)

    def test_bytes_charter_cannot_read_are_left_exactly_where_they_are(self):
        (config.ROOT / SHARED).unlink()
        self.generated().write_bytes(b"\xff\xfe not utf-8")
        workspace.wire_harnesses(self.ws)
        self.assertEqual(self.generated().read_bytes(), b"\xff\xfe not utf-8")

    def test_a_marker_entry_whose_file_is_already_gone_costs_nothing(self):
        """`wire_harnesses` runs on every launch, so a file somebody deleted by hand is an
        ordinary state and must not raise on the way past it."""
        (config.ROOT / LOCAL).unlink()
        (self.clone / LOCAL).unlink()
        rows = dict(workspace.wire_harnesses(self.ws))
        self.assertEqual(rows[f"svc/{SHARED}"], "present")

    def test_two_withdrawals_come_back_in_path_order(self):
        """Path order, not whatever a set happens to iterate in — `cmd_workspace_reinit`
        prints one line per row, and a repair whose report reshuffles cannot be diffed.

        **The marker is written in REVERSE path order, and that is the whole test.** Left
        as charter writes it — already sorted — dropping `sorted` changes nothing a case
        can see, which is how CI's sweep found this line unpinned."""
        (config.ROOT / SHARED).unlink()
        (config.ROOT / LOCAL).unlink()
        marker_path = self.clone / workspace.GENERATED_MARKER
        marker = json.loads(marker_path.read_text())
        marker_path.write_text(json.dumps(dict(reversed(list(marker.items())))))
        self.assertEqual(list(json.loads(marker_path.read_text())),
                         sorted(marker, reverse=True),
                         "fixture did not reverse the marker — this case would pin nothing")
        rows = workspace.wire_guest(self.clone)
        self.assertEqual(rows[:2], [(SHARED, "removed"), (LOCAL, "removed")])

    def test_the_marker_stops_vouching_for_what_was_withdrawn(self):
        (config.ROOT / LOCAL).unlink()
        workspace.wire_harnesses(self.ws)
        marker = json.loads((self.clone / workspace.GENERATED_MARKER).read_text())
        self.assertNotIn(LOCAL, marker)
        self.assertIn(SHARED, marker)

    def test_an_emptied_claude_directory_does_not_stay_behind(self):
        """Charter still visible in a directory it has nothing in."""
        (config.ROOT / SHARED).unlink()
        workspace.wire_harnesses(self.ws)
        self.assertFalse((workspace.workspace_dir(self.ws) / ".claude").exists())

    def test_a_full_withdrawal_in_a_checkout_leaves_no_block_and_no_marker(self):
        """With nothing left, an empty marker made `_charter_owned` answer `[]` and the
        exclude rewrite was skipped — the block kept naming three paths charter no longer
        owned, and the marker itself sat unhidden in somebody's repository."""
        real = self.checkout("real", real=True)
        workspace.wire_harnesses(self.ws)
        self.assertIn(workspace._EXCLUDE_BEGIN, self.excludes(real))
        (config.ROOT / SHARED).unlink()
        (config.ROOT / LOCAL).unlink()
        workspace.wire_harnesses(self.ws)
        self.assertFalse((real / ".claude").exists())
        self.assertFalse((real / workspace.GENERATED_MARKER).exists())
        self.assertNotIn(workspace._EXCLUDE_BEGIN, self.excludes(real))
        self.assertEqual(_status(real), "")

    def test_reinit_says_it_removed_rather_than_calling_it_refreshed(self):
        (config.ROOT / LOCAL).unlink()
        self.assertIn(f"Reinitialized '{self.ws}' → removed svc/{LOCAL} — the plane no "
                      f"longer declares it (charter's harness layer).", self.reinit_said())

    def test_a_withdrawal_is_hidden_no_longer_in_a_checkout(self):
        (config.ROOT / LOCAL).unlink()
        workspace.wire_harnesses(self.ws)
        self.assertNotIn(f"/{LOCAL}", self.excludes(self.clone))
        self.assertFalse((self.clone / LOCAL).exists())


class AMalformedPlaneFileKeepsTheLastGoodCopy(PlaneWithRestrictions):
    """Charter cannot read what the plane now says — which is not the plane saying nothing.

    The first version of this fix read an unparseable plane file as "nothing to mirror" and
    WITHDREW the generated one, so the next launch took `enabledPlugins`, `env` and the
    committed `deny` out of every workspace over a typo, and doctor then said the plane
    declared none of them. Before #942 the last good file stayed, and it does again.
    """

    def setUp(self) -> None:
        super().setUp()
        _plane_local(config.ROOT, ask=["Bash(charter change land *)"])
        self.clone = self.checkout()
        workspace.wire_harnesses(self.ws)

    def test_the_workspace_keeps_its_shared_settings(self):
        before = self.generated().read_text()
        (config.ROOT / SHARED).write_text("{ not json")
        workspace.wire_harnesses(self.ws)
        self.assertEqual(self.generated().read_text(), before)
        self.assertEqual(self.doc()["permissions"]["deny"], ["Bash(rm -rf /)"])

    def test_the_checkout_keeps_its_local_file(self):
        before = (self.clone / LOCAL).read_text()
        (config.ROOT / LOCAL).write_text("{ not json")
        workspace.wire_harnesses(self.ws)
        self.assertEqual((self.clone / LOCAL).read_text(), before)

    def test_the_harness_names_what_it_is_holding_and_why(self):
        (config.ROOT / SHARED).write_text("{ not json")
        held = claude_code.ClaudeCodeHarness().held_files()
        self.assertEqual(list(held), [SHARED])
        self.assertIn(SHARED, held[SHARED])

    def test_doctor_names_the_plane_file_that_cannot_be_read(self):
        (config.ROOT / SHARED).write_text("{ not json")
        r = doctor.check_workspace_harness()
        self.assertEqual(r.status, doctor.WARN)
        text = f"{r.detail} {r.hint}"
        self.assertIn(str(Path(config.ROOT) / SHARED), text)
        self.assertIn("not valid JSON", text)
        self.assertNotIn("nothing to mirror", text)

    def test_doctor_names_an_unreadable_local_file_too(self):
        (config.ROOT / LOCAL).write_text("{ not json")
        r = doctor.check_workspace_harness()
        self.assertEqual(r.status, doctor.WARN)
        self.assertIn(str(Path(config.ROOT) / LOCAL), f"{r.detail} {r.hint}")

    def test_what_is_held_is_not_reported_as_unwanted(self):
        (config.ROOT / SHARED).write_text("{ not json")
        self.assertNotIn("unwanted", [st for _rel, st in workspace.harness_layer(self.ws)])


class AHarnessEditedLocalFileStaysHidden(PlaneWithRestrictions):
    """The critical one: a machine-local rule must never become committable in a clone.

    In a checkout the generated `.claude/settings.local.json` is the same file Claude Code
    saves "Yes, and don't ask again" into. Claude Code adds its global exclude only in a
    repository that does not already ignore the file — and charter's exclude already did.
    So once the harness appended an approval, the digest stopped matching, the next wire
    called the file foreign, dropped it from the exclude block, and the plane's private
    rules plus the operator's grants sat in someone else's `git status`, one `git add -A`
    away. The machine the first version was checked on ignores that path globally, which is
    why nobody saw it; `_status` takes that ignore out of the answer.
    """

    def setUp(self) -> None:
        super().setUp()
        _plane_local(config.ROOT, ask=["Bash(charter change land *)"])
        self.clone = self.checkout("api", real=True)
        workspace.wire_harnesses(self.ws)
        self.local = self.clone / LOCAL
        self.assertTrue(self.local.is_file())

    def plane_moves(self) -> None:
        _plane_local(config.ROOT, ask=["Bash(charter change land *)", "Bash(kubectl *)"])

    def test_the_status_cannot_be_fooled_by_a_global_ignore(self):
        """The fixture's own proof. Without it every clean status below could be this
        machine's `~/.config/git/ignore` speaking."""
        probe = _repo(self.tmp / "probe")
        (probe / ".claude").mkdir()
        (probe / LOCAL).write_text("{}\n")
        self.assertIn(f"?? {LOCAL}", _status(probe))

    def test_a_harness_approval_does_not_make_the_file_committable(self):
        _as_the_harness_would(self.local, "Bash(npm test *)")
        workspace.wire_harnesses(self.ws)
        self.assertEqual(_status(self.clone), "")
        self.assertIn(f"/{LOCAL}", self.excludes(self.clone))

    def test_charter_neither_rewrites_nor_merges_into_it(self):
        edited = _as_the_harness_would(self.local, "Bash(npm test *)")
        self.plane_moves()
        workspace.wire_harnesses(self.ws)
        self.assertEqual(self.local.read_text(), edited)

    def test_while_the_plane_has_not_moved_nothing_is_wrong(self):
        """Every rule charter mirrored is still in the file — the harness only added to it —
        so a warning here would fire on every clone anybody ever approved anything in."""
        _as_the_harness_would(self.local, "Bash(npm test *)")
        self.assertEqual(dict(workspace.guest_layer(self.clone))[LOCAL], "harness-edited")
        r = doctor.check_workspace_harness()
        self.assertEqual(r.status, doctor.OK, f"{r.detail} {r.hint}")

    def test_once_the_plane_moves_doctor_names_it_and_never_advises_removal(self):
        _as_the_harness_would(self.local, "Bash(npm test *)")
        self.plane_moves()
        self.assertEqual(dict(workspace.guest_layer(self.clone))[LOCAL], "harness-behind")
        r = doctor.check_workspace_harness()
        self.assertEqual(r.status, doctor.WARN)
        self.assertIn(f"{self.ws}/api/{LOCAL} (harness-behind)", r.detail)
        self.assertIn("the harness has added its own approvals", r.hint)
        self.assertNotIn("Remove", r.hint)

    def test_guard_ask_says_the_harness_keeps_it_rather_than_that_charter_did_not_write_it(self):
        _as_the_harness_would(self.local, "Bash(npm test *)")
        self.at_the_plane()
        _, said = self.invoke(commands.cmd_guard_ask, pattern="kubectl *", local=True)
        self.assertIn(f"{self.ws}/api/{LOCAL}", said)
        self.assertIn("the harness has added its own approvals", said)
        self.assertNotIn("charter did not write", said)
        self.assertNotIn("Remove", said)

    def test_reinit_says_the_same(self):
        _as_the_harness_would(self.local, "Bash(npm test *)")
        self.plane_moves()
        said = " ".join(self.reinit_said())
        self.assertIn(f"api/{LOCAL}", said)
        self.assertIn("the harness has added its own approvals", said)
        self.assertNotIn("Remove", said)

    def test_removing_the_workspace_keeps_it_hidden(self):
        """`unwire_guest` takes charter's own files and its block. This file is no longer
        only charter's, so it stays — and a line removed from the block over a file that
        stays is the same leak one verb on."""
        _as_the_harness_would(self.local, "Bash(npm test *)")
        workspace.unwire_guest(self.clone)
        self.assertTrue(self.local.is_file())
        self.assertIn(f"/{LOCAL}", self.excludes(self.clone))
        self.assertEqual(_status(self.clone), "")

    def test_a_local_file_charter_never_wrote_is_neither_hidden_nor_rewritten(self):
        """The operator ran a session in the clone before charter wired it. That file is
        theirs from the start: charter does not merge into it and does not hide it from
        their own status, and it says the plane's rules are not in it."""
        fresh = _repo(workspace.workspace_dir(self.ws) / "fresh")
        (fresh / ".claude").mkdir()
        theirs = json.dumps({"permissions": {"allow": ["Bash(make *)"]}}, indent=2) + "\n"
        (fresh / LOCAL).write_text(theirs)
        rows = dict(workspace.wire_guest(fresh))
        self.assertEqual((fresh / LOCAL).read_text(), theirs)
        self.assertEqual(rows[LOCAL], "harness-behind")
        self.assertNotIn(f"/{LOCAL}", self.excludes(fresh))


class TheExcludeIsWrittenBeforeTheLocalFile(PlaneWithRestrictions):
    """A machine-local file charter cannot hide is never written.

    The first version wrote the files, then the block. When the block could not be written
    the local file was on disk anyway, and `guard ask` reported the exclude path as a rule
    "NOT in force" — backwards: the rule WAS in force, and the problem was that it was
    committable.
    """

    def setUp(self) -> None:
        super().setUp()
        _plane_local(config.ROOT, ask=["Bash(charter change land *)"])
        self.clone = self.checkout("api", real=True)
        # A DIRECTORY where the exclude file goes: the one refusal no permission bit decides,
        # so the case means the same thing when the suite runs as root.
        self.exclude = workspace.git_exclude_file(self.clone)
        if self.exclude.exists():
            self.exclude.unlink()
        self.exclude.mkdir(parents=True)

    def test_the_local_file_is_withheld_when_it_cannot_be_hidden(self):
        rows = dict(workspace.wire_guest(self.clone))
        self.assertEqual(rows[LOCAL], "withheld")
        self.assertEqual(rows[".git/info/exclude"], "blocked")
        self.assertFalse((self.clone / LOCAL).exists())

    def test_the_shared_settings_still_arrive(self):
        """Unchanged, and deliberately so: they hold nothing machine-local, and the
        exclude row already says they show in that repository's status."""
        workspace.wire_guest(self.clone)
        self.assertTrue((self.clone / SHARED).is_file())

    def test_guard_ask_says_it_in_a_sentence_of_its_own(self):
        self.at_the_plane()
        _, said = self.invoke(commands.cmd_guard_ask, pattern="kubectl delete *",
                              local=False)
        self.assertIn(f"{self.ws}/api/{LOCAL} was not written", said)
        self.assertIn("could not hide it in that checkout's .git/info/exclude", said)

    def test_a_blocked_exclude_is_not_reported_as_a_rule_out_of_force(self):
        self.at_the_plane()
        _, said = self.invoke(commands.cmd_guard_ask, pattern="kubectl delete *",
                              local=False)
        self.assertNotIn(f"NOT in force in {self.ws}/api/.git/info/exclude", said)
        self.assertIn(f"Could not update {self.ws}/api/.git/info/exclude", said)

    def test_reinit_says_it_too(self):
        said = " ".join(self.reinit_said())
        self.assertIn(f"api/{LOCAL} was not written — charter could not hide it", said)

    def test_once_the_exclude_can_be_written_the_file_arrives_hidden(self):
        workspace.wire_guest(self.clone)
        self.exclude.rmdir()
        workspace.wire_guest(self.clone)
        self.assertTrue((self.clone / LOCAL).is_file())
        self.assertIn(f"/{LOCAL}", self.excludes(self.clone))
        self.assertEqual(_status(self.clone), "")


class CloningSaysWhatItCouldNotHide(PlaneWithRestrictions):
    """`charter clone` announces what it wrote and that `git status` there is unaffected.
    With the checkout's exclude unwritable, neither half is true of the local file."""

    def setUp(self) -> None:
        super().setUp()
        _plane_local(config.ROOT, ask=["Bash(charter change land *)"])
        self.clone = self.checkout("api", real=True)
        exclude = workspace.git_exclude_file(self.clone)
        if exclude.exists():
            exclude.unlink()
        exclude.mkdir(parents=True)

    def announced(self) -> str:
        err = io.StringIO()
        with redirect_stderr(err), redirect_stdout(io.StringIO()):
            commands._wire_clones(self.ws)
        return err.getvalue()

    def test_the_withheld_file_gets_a_sentence_of_its_own(self):
        said = self.announced()
        self.assertIn(f"api/{LOCAL} was not written", said)
        self.assertIn("could not hide it in that checkout's .git/info/exclude", said)

    def test_it_does_not_claim_that_nothing_it_wrote_can_be_committed(self):
        self.assertNotIn("nothing charter wrote can be committed", self.announced())


class TheLocalFileRootIsAskedOfGitCarefully(PlaneWithRestrictions):
    """`doctor._local_settings_root`'s edges — each a place the answer must degrade to "the
    session's own directory" rather than to a wrong root or a traceback."""

    def setUp(self) -> None:
        super().setUp()
        self.plane = Path(config.ROOT).resolve()
        self.here = workspace.workspace_dir(self.ws).resolve()
        self.addCleanup(os.chdir, os.getcwd())
        home = self.tmp / "home"
        (home / ".claude").mkdir(parents=True, exist_ok=True)
        self.enterContext(mock.patch.dict(os.environ, {"HOME": str(home)}))

    def listed(self) -> list[str]:
        return [str(p) for p in doctor._settings_files()]

    def test_a_repository_whose_git_dir_lives_elsewhere_still_names_its_root(self):
        """`git init --separate-git-dir`: the common directory is not a `.git` inside the
        checkout, so its parent is not the root — `--show-toplevel` is."""
        import tempfile

        elsewhere = Path(tempfile.mkdtemp(prefix="charter-942-gitdir-"))
        self.addCleanup(shutil.rmtree, elsewhere, True)
        subprocess.run(["git", "init", "-q", "--separate-git-dir", str(elsewhere / "repo.git"),
                        str(self.plane)], check=True,
                       stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
        os.chdir(self.here)
        listed = self.listed()
        self.assertIn(str(self.plane / LOCAL), listed)
        self.assertNotIn(str(elsewhere.resolve() / LOCAL), listed)

    def test_a_repository_rooted_at_home_keeps_the_file_beside_the_shared_one(self):
        """The docs' own exception: with the repository root at the home directory the local
        file stays with the shared one, in the session's directory."""
        subprocess.run(["git", "init", "-q", str(self.plane)], check=True,
                       stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
        self.enterContext(mock.patch.dict(os.environ, {"HOME": str(self.plane)}))
        os.chdir(self.here)
        self.assertNotIn(str(self.plane / LOCAL), self.listed())

    def test_git_that_cannot_answer_costs_only_the_extra_entry(self):
        """Read from the SessionStart hook: a git that raises must leave the list as it
        was, never take the row down."""
        os.chdir(self.here)
        with mock.patch.object(doctor, "_git_in", side_effect=OSError("no git here")):
            listed = self.listed()
        self.assertEqual(listed[:2], [str(self.here / SHARED), str(self.here / LOCAL)])
        self.assertNotIn(str(self.plane / LOCAL), listed)


class WhatTheWriteLoopLeavesAlone(PlaneWithRestrictions):
    """`_materialise`'s answers for the states that must NOT end in a write."""

    def setUp(self) -> None:
        super().setUp()
        _plane_local(config.ROOT, ask=["Bash(charter change land *)"])
        self.clone = self.checkout()
        workspace.wire_harnesses(self.ws)

    def test_a_harness_edit_while_the_plane_is_unmoved_is_not_rewritten(self):
        """`harness-edited` is current, and writing charter's text over it would throw away
        the approval the harness has just saved."""
        edited = _as_the_harness_would(self.clone / LOCAL, "Bash(npm test *)")
        rows = dict(workspace.wire_harnesses(self.ws))
        self.assertEqual((self.clone / LOCAL).read_text(), edited)
        self.assertEqual(rows[f"svc/{LOCAL}"], "present")

    def test_a_file_that_cannot_be_withdrawn_stays_named_and_nothing_raises(self):
        """`wire_harnesses` runs on a launch. A withdrawal that fails costs that one file,
        stays in the marker, and is still reported — it does not raise out of the launch."""
        (config.ROOT / LOCAL).unlink()
        real_unlink = Path.unlink

        def _refuse(path, *args, **kwargs):
            if path.name == "settings.local.json":
                raise OSError("a directory that will not let go")
            return real_unlink(path, *args, **kwargs)

        with mock.patch.object(Path, "unlink", _refuse):
            rows = dict(workspace.wire_harnesses(self.ws))
        self.assertNotIn(f"svc/{LOCAL}", rows)
        self.assertTrue((self.clone / LOCAL).is_file())
        self.assertIn(LOCAL, json.loads(
            (self.clone / workspace.GENERATED_MARKER).read_text()))
        self.assertIn((f"svc/{LOCAL}", "unwanted"), workspace.harness_layer(self.ws))

    def test_a_plane_with_no_local_rules_has_no_local_entry_in_its_rules(self):
        """A file declaring none is absent, not present and empty — a zero is not a count
        worth a sentence."""
        (config.ROOT / LOCAL).unlink()
        self.assertNotIn(LOCAL, claude_code.ClaudeCodeHarness().restrictive_rules())


class TheGuardsOnTheCoWrittenFileEachDecideSomething(PlaneWithRestrictions):
    """One case per guard added for #942's fix round whose deletion no other case sees."""

    def setUp(self) -> None:
        super().setUp()
        _plane_local(config.ROOT, ask=["Bash(charter change land *)"])
        self.clone = self.checkout("api", real=True)
        workspace.wire_harnesses(self.ws)

    def test_outside_a_repository_nothing_is_added_wherever_the_process_stands(self):
        """`if res.returncode != 0`. Without it an empty answer becomes `Path("")`, which
        resolves against the PROCESS's directory rather than the one asked about — so a
        caller naming a root (`_ensure_guard_hook` names the plane) from inside some other
        repository would be handed that repository's local file."""
        outside = self.tmp / "nowhere"
        outside.mkdir()
        self.addCleanup(os.chdir, os.getcwd())
        os.chdir(self.clone)
        listed = [str(p) for p in doctor._settings_files(outside)]
        self.assertEqual(listed[:2], [str(outside / SHARED), str(outside / LOCAL)])
        self.assertEqual(len(listed), 3, listed)

    def test_the_first_pass_never_names_a_file_charter_did_not_write(self):
        """The block is written before the files, and for that one moment it must not hide
        somebody's own local settings — a process killed between the two passes would leave
        their file invisible in their own `git status`."""
        fresh = _repo(workspace.workspace_dir(self.ws) / "fresh")
        (fresh / ".claude").mkdir()
        (fresh / LOCAL).write_text(
            json.dumps({"permissions": {"allow": ["Bash(make *)"]}}) + "\n")
        named: list[list[str]] = []
        real = workspace._register_excludes

        def _record(tree, rels):
            named.append(list(rels))
            return real(tree, rels)

        with mock.patch.object(workspace, "_register_excludes", side_effect=_record):
            workspace.wire_guest(fresh)
        self.assertTrue(named, "nothing was registered — this case would assert nothing")
        for rels in named:
            self.assertNotIn(LOCAL, rels)

    def test_an_operator_edited_shared_file_that_stays_does_not_keep_the_block(self):
        """Only a CO-WRITTEN file keeps its line when the rest goes. The operator's own
        rewrite of the shared file is theirs, and charter does not hide it on the way out."""
        (self.clone / SHARED).write_text('{"env": {"theirs": "1"}}\n')
        workspace.unwire_guest(self.clone)
        self.assertTrue((self.clone / SHARED).is_file())
        self.assertNotIn(f"/{SHARED}", self.excludes(self.clone))

    def test_a_co_written_file_already_gone_does_not_keep_a_line(self):
        """A line kept for a file that is not there hides nothing and names a path charter
        no longer has — the block would read stale for ever."""
        (self.clone / LOCAL).unlink()
        workspace.unwire_guest(self.clone)
        self.assertNotIn(f"/{LOCAL}", self.excludes(self.clone))


class WhatTheSweepFoundUnpinned(PlaneWithRestrictions):
    """One case per survivor the deletion sweep reported on the fix round's commit that turned
    out to decide something. The survivors that decided nothing were deleted instead."""

    def setUp(self) -> None:
        super().setUp()
        _plane_local(config.ROOT, ask=["Bash(charter change land *)"])

    def announced(self) -> str:
        err = io.StringIO()
        with redirect_stderr(err), redirect_stdout(io.StringIO()):
            commands._wire_clones(self.ws)
        return err.getvalue()

    def test_an_ordinary_clone_is_announced_as_hidden(self):
        """`_wire_clones`' first branch. With the exclude written, the announcement makes both
        of its claims; the warning that withdraws them belongs to the other case only."""
        self.checkout("fresh", real=True)
        said = self.announced()
        self.assertIn("fresh: charter's layer written", said)
        self.assertIn("nothing charter wrote can be committed", said)
        self.assertNotIn("could not be updated", said)

    def test_a_linked_worktree_reads_the_main_checkouts_local_file(self):
        """Measured on 2.1.267: a `deny` in the main checkout's local file applies to a session
        in its linked worktree. `--show-toplevel` answers with the worktree, so the common
        directory's parent is the only question here that names the main checkout — and the
        one layout where the two answers differ."""
        main = _repo(self.tmp / "main-checkout")
        tree = self.tmp / "linked"
        _git(main, "worktree", "add", "-q", "-b", "probe-942", str(tree))
        self.addCleanup(os.chdir, os.getcwd())
        os.chdir(tree)
        listed = [str(p) for p in doctor._settings_files()]
        self.assertIn(str(main.resolve() / LOCAL), listed)
        self.assertIn(str(tree.resolve() / LOCAL), listed)

    def test_both_unreadable_plane_files_are_named_in_the_harnesss_order(self):
        workspace.wire_harnesses(self.ws)
        (config.ROOT / SHARED).write_text("{ not json")
        (config.ROOT / LOCAL).write_text("{ not json")
        r = doctor.check_workspace_harness()
        shared, local = str(Path(config.ROOT) / SHARED), str(Path(config.ROOT) / LOCAL)
        self.assertIn(f"not valid JSON: {shared}, {local}", r.detail)

    def test_the_sentence_survives_beside_a_row_that_carries_no_rules(self):
        """`any`, not `all`: one named row carrying rules is enough for the consequence, and a
        missing agent file beside it must not talk the sentence away."""
        agent = config.ROOT / ".claude" / "agents" / "steward.md"
        agent.parent.mkdir(parents=True, exist_ok=True)
        agent.write_text("# steward\n")
        clone = self.checkout()
        workspace.wire_harnesses(self.ws)
        (clone / ".claude" / "agents" / "steward.md").unlink()
        _plane_settings(config.ROOT, permissions={"ask": ["Bash(kubectl delete *)"]})
        r = doctor.check_workspace_harness()
        self.assertIn("svc/.claude/agents/steward.md (missing)", r.detail)
        self.assertIn("prompted or refused by them", r.hint)

    def test_a_wire_that_changes_nothing_leaves_the_marker_alone(self):
        """`if not wrote: return rows`. A launch runs this every time, and a marker rewritten on
        every one moves mtimes in every directory charter owns for a call that did nothing."""
        workspace.wire_harnesses(self.ws)
        marker = workspace.workspace_dir(self.ws) / workspace.GENERATED_MARKER
        os.utime(marker, (1, 1))
        workspace.wire_harnesses(self.ws)
        self.assertEqual(marker.stat().st_mtime_ns, 1_000_000_000)

    def test_a_plane_with_only_a_local_rule_withholds_it_where_it_cannot_be_hidden(self):
        """The first pass names a MISSING co-written file itself. When the local file is the
        only thing a checkout wants, nothing else puts a path in the block, no write would be
        attempted, the refusal would never be seen — and the file would land unhidden."""
        (config.ROOT / SHARED).unlink()
        clone = self.checkout("api", real=True)
        exclude = workspace.git_exclude_file(clone)
        if exclude.exists():
            exclude.unlink()
        exclude.mkdir(parents=True)
        rows = dict(workspace.wire_guest(clone))
        self.assertEqual(rows[LOCAL], "withheld")
        self.assertFalse((clone / LOCAL).exists())

    def test_a_second_wire_of_a_clone_with_many_files_leaves_the_block_as_it_was(self):
        """`sorted` in the first pass. The second pass re-sorts, so the first pass's order never
        shows in the end state — only in whether an idempotent wire rewrote the block at all.
        Nine files, so an unsorted set agreeing with path order by luck is a 1-in-362,880
        accident rather than the coin toss a two-file fixture is."""
        agents = config.ROOT / ".claude" / "agents"
        agents.mkdir(parents=True, exist_ok=True)
        for name in ("zulu", "yankee", "xray", "whiskey", "victor", "uniform", "tango"):
            (agents / f"{name}.md").write_text(f"# {name}\n")
        clone = self.checkout("api", real=True)
        workspace.wire_guest(clone)
        self.assertEqual(dict(workspace.wire_guest(clone))[".git/info/exclude"], "present")


class GuardAskKeepsTheMirrorInStep(PlaneWithRestrictions):
    """The command that writes the rule refreshes the generated files it has just made
    stale. Without it the rule reaches a workspace chat at the NEXT launch, and the operator
    is told it applies now."""

    def setUp(self) -> None:
        super().setUp()
        self.at_the_plane()

    def test_the_rule_is_in_the_workspace_the_moment_the_command_returns(self):
        self.invoke(commands.cmd_guard_ask, pattern="kubectl delete *", local=False)
        self.assertIn("Bash(kubectl delete *)", self.doc()["permissions"]["ask"])

    def test_a_local_rule_reaches_a_checkout_and_not_the_workspace_directory(self):
        clone = self.checkout()
        self.invoke(commands.cmd_guard_ask, pattern="charter change land *", local=True)
        self.assertIn("Bash(charter change land *)",
                      json.loads((clone / LOCAL).read_text())["permissions"]["ask"])
        self.assertFalse(self.generated(LOCAL).exists())

    def test_the_landing_prompt_charter_itself_recommends_reaches_a_checkout_chat(self):
        """`doctor.LANDING_PROMPT` verbatim — the rule charter names and does not run."""
        clone = self.checkout()
        self.assertIn("--local", doctor.LANDING_PROMPT)
        pattern = doctor.LANDING_PROMPT.split("--local ", 1)[1].strip().strip("'")
        self.invoke(commands.cmd_guard_ask, pattern=pattern, local=True)
        self.assertIn(commands._as_rule(pattern),
                      json.loads((clone / LOCAL).read_text())["permissions"]["ask"])

    def test_it_says_the_rule_travelled_rather_than_writing_in_silence(self):
        _, said = self.invoke(commands.cmd_guard_ask, pattern="kubectl delete *",
                              local=False)
        self.assertIn("workspaces/", said)

    def test_a_plane_with_no_workspaces_says_nothing_about_mirroring(self):
        shutil.rmtree(config.WORKSPACES_DIR / self.ws)
        _, said = self.invoke(commands.cmd_guard_ask, pattern="kubectl delete *",
                              local=False)
        self.assertNotIn("brought into step", said)

    def test_a_generated_file_somebody_edited_is_named_rather_than_repaired(self):
        self.generated().write_text('{"env": {"MINE": "1"}}\n')
        _, said = self.invoke(commands.cmd_guard_ask, pattern="kubectl delete *",
                              local=False)
        self.assertIn("NOT in force", said)
        self.assertIn(f"{self.ws}/{SHARED}", said)
        self.assertEqual(self.generated().read_text(), '{"env": {"MINE": "1"}}\n')

    def test_nothing_is_named_as_out_of_reach_when_everything_took_the_rule(self):
        _, said = self.invoke(commands.cmd_guard_ask, pattern="kubectl delete *",
                              local=False)
        self.assertNotIn("NOT in force", said)

    def test_a_run_that_only_creates_a_file_is_counted(self):
        """`"created"`, alone in a run — one of the three status literals CI's sweep read
        as a masked cluster."""
        self.generated().unlink()
        _, said = self.invoke(commands.cmd_guard_ask, pattern="kubectl delete *",
                              local=False)
        self.assertIn("1 generated file(s) under workspaces/ brought into step", said)
        self.assertIn("Bash(kubectl delete *)", self.doc()["permissions"]["ask"])

    def test_a_run_that_only_withdraws_a_file_is_counted(self):
        """`"removed"`, alone in a run. The exclude block that changes with it is not a
        generated file and is not counted as one."""
        _plane_local(config.ROOT, ask=["Bash(charter change land *)"])
        clone = self.checkout()
        workspace.wire_harnesses(self.ws)
        (config.ROOT / LOCAL).unlink()
        _, said = self.invoke(commands.cmd_guard_ask, pattern="terraform apply *",
                              local=False)
        self.assertIn("1 generated file(s) under workspaces/ brought into step", said)
        self.assertFalse((clone / LOCAL).exists())

    def test_a_path_charter_cannot_write_is_named_as_out_of_reach(self):
        """`"blocked"`, alone in a run. A `.claude` that is a FILE cannot be made into a
        directory; a file rather than a `chmod`, because permission bits do not refuse
        root."""
        shutil.rmtree(workspace.workspace_dir(self.ws) / ".claude")
        (workspace.workspace_dir(self.ws) / ".claude").write_text("not a directory\n")
        _, said = self.invoke(commands.cmd_guard_ask, pattern="kubectl delete *",
                              local=False)
        self.assertIn(f"NOT in force in {self.ws}/{SHARED}", said)
        self.assertEqual((workspace.workspace_dir(self.ws) / ".claude").read_text(),
                         "not a directory\n")

    def test_an_allow_rule_does_not_refresh_anything_sideways(self):
        """`cmd_guard_allow` deliberately does not mirror: a grant is never carried, so a
        refresh there could only push UNRELATED drift into every workspace."""
        _plane_settings(config.ROOT, enabledPlugins={"charter@charter": True,
                                                     "later@market": True})
        self.invoke(commands.cmd_guard_allow, pattern="npm test *", local=False)
        self.assertNotIn("later@market", self.generated().read_text())
        self.assertNotIn("npm test", self.generated().read_text())

    def test_a_workspaces_directory_that_cannot_be_read_does_not_fail_the_command(self):
        with mock.patch.object(workspace, "list_workspaces",
                               side_effect=OSError("workspaces/ is unreadable")):
            rc, _said = self.invoke(commands.cmd_guard_ask, pattern="kubectl delete *",
                                    local=False)
        self.assertEqual(rc, 0)
        self.assertIn("Bash(kubectl delete *)", json.loads(
            (config.ROOT / SHARED).read_text())["permissions"]["ask"])

    def test_one_workspace_that_cannot_be_wired_does_not_cost_the_others(self):
        workspace.ensure("south")
        real = workspace.wire_harnesses

        def _one_fails(name):
            if name == "north":
                raise OSError("this one is unwireable")
            return real(name)

        with mock.patch.object(workspace, "wire_harnesses", side_effect=_one_fails):
            self.invoke(commands.cmd_guard_ask, pattern="kubectl delete *", local=False)
        self.assertIn("Bash(kubectl delete *)",
                      self.doc(name="south")["permissions"]["ask"])


class DoctorNamesTheLagRatherThanTicking(PlaneWithRestrictions):
    """Three rows were green over a chat with none of the plane's rules. The one that can
    name it is `workspace layer`, which regenerates and compares — so it sees the gap the
    moment the generator emits the buckets, and what it has to ADD is the consequence."""

    def test_a_current_workspace_still_reads_ok(self):
        self.assertEqual(doctor.check_workspace_harness().status, doctor.OK)

    def test_a_workspace_behind_the_planes_restrictions_reads_as_a_problem(self):
        _plane_settings(config.ROOT, permissions={"ask": ["Bash(terraform apply *)",
                                                          "Bash(kubectl delete *)"]})
        r = doctor.check_workspace_harness()
        self.assertEqual(r.status, doctor.WARN)
        self.assertIn(f"{self.ws}/{SHARED} (stale)", r.detail)
        self.assertIn("prompted or refused by them", r.hint)
        self.assertIn("charter workspace reinit", r.hint)

    def test_a_foreign_generated_file_never_receives_them_and_the_row_says_so(self):
        self.generated().write_text('{"env": {"MINE": "1"}}\n')
        r = doctor.check_workspace_harness()
        self.assertEqual(r.status, doctor.WARN)
        self.assertIn("charter did not write", r.hint)
        self.assertIn("prompted or refused by them", r.hint)

    def test_a_plane_with_no_restrictions_at_all_gets_no_such_sentence(self):
        _plane_settings(config.ROOT)
        self.generated().unlink()
        r = doctor.check_workspace_harness()
        self.assertEqual(r.status, doctor.WARN)
        self.assertNotIn("prompted or refused by them", r.hint)

    def test_the_sentence_is_only_about_rows_that_carry_rules(self):
        """A missing agent file in a checkout is a finding, and no rule rides in it."""
        agent = config.ROOT / ".claude" / "agents" / "steward.md"
        agent.parent.mkdir(parents=True, exist_ok=True)
        agent.write_text("# steward\n")
        clone = self.checkout()
        workspace.wire_harnesses(self.ws)
        (clone / ".claude" / "agents" / "steward.md").unlink()
        r = doctor.check_workspace_harness()
        self.assertEqual(r.status, doctor.WARN)
        self.assertIn(f"svc/.claude/agents/steward.md (missing)", r.detail)
        self.assertNotIn("prompted or refused by them", r.hint)

    def test_the_count_is_of_the_rules_riding_in_the_rows_it_names(self):
        """One local rule behind, not the plane's three: the committed two are current."""
        _plane_local(config.ROOT, ask=["Bash(charter change land *)"])
        clone = self.checkout()
        workspace.wire_harnesses(self.ws)
        (clone / LOCAL).unlink()
        r = doctor.check_workspace_harness()
        self.assertIn("The plane's 1 ask/deny rule(s)", r.hint)

    def test_a_generated_file_the_plane_no_longer_wants_is_named(self):
        """It still matches what charter wrote, so the next launch withdraws it — and until
        then a withdrawn restriction is still prompting in that checkout."""
        _plane_local(config.ROOT, ask=["Bash(charter change land *)"])
        self.checkout()
        workspace.wire_harnesses(self.ws)
        (config.ROOT / LOCAL).unlink()
        r = doctor.check_workspace_harness()
        self.assertEqual(r.status, doctor.WARN)
        self.assertIn(f"svc/{LOCAL} (unwanted)", r.detail)

    def test_a_harness_that_cannot_answer_costs_the_sentence_and_not_the_row(self):
        class _Mute(claude_code.ClaudeCodeHarness):
            def restrictive_rules(self):
                raise ValueError("this harness cannot say")

        self.generated().unlink()
        with mock.patch.object(registry, "all", return_value=[_Mute()]):
            r = doctor.check_workspace_harness()
        self.assertEqual(r.status, doctor.WARN)
        self.assertIn(f"{self.ws}/{SHARED} (missing)", r.detail)
        self.assertNotIn("prompted or refused by them", r.hint)

    def test_a_harness_answering_none_costs_the_sentence_and_not_the_row(self):
        class _Null(claude_code.ClaudeCodeHarness):
            def restrictive_rules(self):
                return None

        self.generated().unlink()
        with mock.patch.object(registry, "all", return_value=[_Null()]):
            r = doctor.check_workspace_harness()
        self.assertEqual(r.status, doctor.WARN)
        self.assertIn(f"{self.ws}/{SHARED} (missing)", r.detail)


class TheAskRulesRowSeesThemInAWorkspaceChat(PlaneWithRestrictions):
    """`check_ask_rules` reads the settings THIS SESSION reads (#855). In a workspace chat
    that is the generated file, which is why the row said `none` while the plane held
    rules — and why it now says what it says."""

    def test_a_chat_in_a_workspace_reports_the_planes_rule_rather_than_none(self):
        self.addCleanup(os.chdir, os.getcwd())
        os.chdir(workspace.workspace_dir(self.ws))
        r = doctor.check_ask_rules()
        self.assertEqual(r.status, doctor.OK)
        self.assertNotEqual(r.detail, "none")
        self.assertIn("1 rule(s)", r.detail)

    def test_a_chat_in_a_workspace_can_now_see_a_rule_shadowing_a_persona_tool(self):
        self.make_persona("ops", role="Ops", vault="none", tools="kubectl")
        from charter import persona

        persona.set_active("ops")
        _plane_settings(config.ROOT, permissions={"ask": ["Bash(kubectl *)"]})
        workspace.wire_harnesses(self.ws)
        self.addCleanup(os.chdir, os.getcwd())
        os.chdir(workspace.workspace_dir(self.ws))
        r = doctor.check_ask_rules()
        self.assertEqual(r.status, doctor.WARN)
        self.assertIn("kubectl", f"{r.detail} {r.hint}")


class TheBucketsAreNamedAndAllowIsNotAmongThem(PlaneWithRestrictions):
    def test_only_the_restrictive_buckets_are_mirrored(self):
        """A literal, spelled here by hand — a test that reads the constant agrees with any
        value it takes."""
        self.assertEqual(claude_code.RESTRICTIVE_BUCKETS, ("ask", "deny"))

    def test_the_harness_answers_which_rules_ride_in_which_generated_file(self):
        """Keyed by the generated path, so doctor can count only the rules riding in the
        rows it names."""
        _plane_local(config.ROOT, ask=["Bash(charter change land *)"])
        got = claude_code.ClaudeCodeHarness().restrictive_rules()
        self.assertEqual({k: sorted(v) for k, v in got.items()},
                         {SHARED: ["Bash(rm -rf /)", "Bash(terraform apply *)"],
                          LOCAL: ["Bash(charter change land *)"]})

    def test_a_harness_that_generates_nothing_carries_no_rules(self):
        for h in registry.all():
            if not h.workspace_files() and not h.checkout_files():
                self.assertEqual(h.restrictive_rules(), {},
                                 f"{h.name} claims rules ride in files it does not write")


if __name__ == "__main__":  # pragma: no cover
    unittest.main()
