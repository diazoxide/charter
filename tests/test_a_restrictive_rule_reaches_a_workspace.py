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

import errno
import fnmatch
import io
import json
import os
import shutil
import stat
import subprocess
import sys
import unittest
from contextlib import ExitStack, contextmanager, redirect_stderr, redirect_stdout
from pathlib import Path
from types import SimpleNamespace
from unittest import mock

from charter import commands, commands_workspace, config, doctor, gitpolicy, hooks, util, workspace
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


@contextmanager
def _refused(files=(), inside=(), calls=("lstat",)):
    """Filesystem questions about *files*, or about anything strictly *inside* the given
    directories, refused with EACCES — the way an unreadable directory or a flaky network
    mount refuses them — while every other path is answered for real.

    **Refused by mock, not by a permission bit** (review round 3). An unreadable directory
    makes `Path.exists` RAISE on 3.11–3.13 and answer False on 3.14, so `chmod` reproduces a
    different failure on every interpreter CI runs. The refused question is the same one on
    all of them, and each of *calls* (`os.lstat`, `os.stat`, `io.open`) is one a caller can
    make."""
    exact = {os.fspath(f) for f in files}
    under = [os.fspath(d) + os.sep for d in inside]

    def refusing(real):
        def call(path, *args, **kwargs):
            try:
                spelled = os.fspath(path)
            except TypeError:
                spelled = None
            if spelled is not None and (spelled in exact
                                        or any(spelled.startswith(u) for u in under)):
                raise PermissionError(13, "Permission denied", spelled)
            return real(path, *args, **kwargs)
        return call

    with ExitStack() as stack:
        for name in calls:
            owner = io if name == "open" else os
            stack.enter_context(mock.patch.object(owner, name, refusing(getattr(owner, name))))
        yield


class _Killed(BaseException):
    """A process killed at an injected point: nothing after it runs, and no `except Exception`
    or cleanup that a real SIGKILL would skip gets to run either (review round 4)."""


#: Charter's temp-file name, spelled by hand: imported, it would agree with any value it takes.
_TEMP_GLOB = ".charter-generated.*.tmp"


@contextmanager
def _torn_write_of(target: Path):
    """Every write into *target*, or into a charter temp file beside it, is killed HALFWAY: half
    the text reaches the disk, and nothing after it runs (review round 5, R1).

    Written in place, that half IS the file — 68 or 0 bytes of a `settings.local.json` that
    parses as nothing and read as harness-edited, so the plane's new `deny` was never written
    and doctor said all current. Written to a temp that is then renamed, the half is the temp's,
    and the file stays whole."""
    real_open = io.open
    wanted = os.fspath(target)
    beside = os.fspath(target.parent)

    class _Torn:
        def __init__(self, f):
            self._f = f

        def write(self, text):
            self._f.write(text[: len(text) // 2])
            self._f.flush()
            raise _Killed()

        def __enter__(self):
            return self

        def __exit__(self, *exc):
            self._f.close()
            return False

        def __getattr__(self, name):
            return getattr(self._f, name)

    def opening(path, mode="r", *args, **kwargs):
        f = real_open(path, mode, *args, **kwargs)
        try:
            spelled = os.fspath(path)
        except TypeError:
            return f
        ours = spelled == wanted or (os.path.dirname(spelled) == beside and fnmatch.fnmatchcase(
            os.path.basename(spelled), _TEMP_GLOB))
        return _Torn(f) if ours and any(c in mode for c in "wxa") else f

    with mock.patch("io.open", opening), mock.patch("builtins.open", opening):
        yield


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


def _briefing(case, cwd: Path) -> str:
    """What SessionStart hands a chat rooted in *cwd* — the real handler, driven with the payload
    the harness sends, and its background refreshers held (review round 5, R4)."""
    _isolation.no_background_refresh(case)
    out = _isolation.run_hook(hooks.sessionstart, {"session_id": "r5", "cwd": str(cwd)})
    return ((out or {}).get("hookSpecificOutput") or {}).get("additionalContext", "")


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
        self.assertIn("the harness saves its approvals into that file", r.hint)
        self.assertNotIn("Remove", r.hint)

    def test_guard_ask_says_the_harness_keeps_it_rather_than_that_charter_did_not_write_it(self):
        _as_the_harness_would(self.local, "Bash(npm test *)")
        self.at_the_plane()
        _, said = self.invoke(commands.cmd_guard_ask, pattern="kubectl *", local=True)
        self.assertIn(f"{self.ws}/api/{LOCAL}", said)
        self.assertIn("the harness saves its approvals into that file", said)
        self.assertNotIn("charter did not write", said)
        self.assertNotIn("Remove", said)

    def test_reinit_says_the_same(self):
        _as_the_harness_would(self.local, "Bash(npm test *)")
        self.plane_moves()
        said = " ".join(self.reinit_said())
        self.assertIn(f"api/{LOCAL}", said)
        self.assertIn("the harness saves its approvals into that file", said)
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
        """The block's order, settled in ONE place since review round 2 — `_shared_rels`' `sorted`;
        the first pass carried its own until the sweep showed it deciding nothing after that.
        The order never shows in the end state, only in whether an idempotent wire rewrote the
        block at all.
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


class TheSharedExcludeHoldsWhatEveryTreeNeeds(PlaneWithRestrictions):
    """Review round 2's critical finding: ONE `info/exclude` serves a clone and every linked
    worktree of it — the common git directory's — and each tree rewrote charter's block there
    from its own marker alone.

    So a worktree `api-wt`, wired after `api` because it sorts after it, withdrew its own
    untouched local file and wrote the block without the local line, and the clone's
    harness-edited copy — the plane's private rules plus the operator's grants — showed in
    `git status` after every launch. Removing a workspace that held a worktree of another
    workspace's clone emptied that clone's block outright. Real git throughout, with the
    machine's global ignore taken out of every answer (`_status`).
    """

    def setUp(self) -> None:
        super().setUp()
        _plane_local(config.ROOT, ask=["Bash(charter change land *)"])
        self.clone = self.checkout("api", real=True)
        self.local = self.clone / LOCAL

    def worktree(self, name: str, ws: str | None = None) -> Path:
        path = workspace.workspace_dir(ws or self.ws) / name
        path.parent.mkdir(parents=True, exist_ok=True)
        _git(self.clone, "worktree", "add", "-q", "-b", f"wt-{name}-{ws or self.ws}", str(path))
        return path

    def git_asked(self) -> list[list[str]]:
        """Every `util.run` argv for the rest of the case, each still run for real."""
        calls: list[list[str]] = []
        real = workspace.util.run

        def _record(cmd, *args, **kwargs):
            calls.append(list(cmd))
            return real(cmd, *args, **kwargs)

        self.enterContext(mock.patch.object(workspace.util, "run", _record))
        return calls

    def edited_clone_and_a_withdrawn_worktree(self) -> Path:
        """The W1 shape: the clone's local file holds an approval, the plane then drops its
        local rules, and the worktree's untouched copy is withdrawn by the next wire."""
        wt = self.worktree("api-wt")
        self.assertEqual([t.name for t in workspace.guest_trees(self.ws)], ["api", "api-wt"],
                         "fixture: the worktree must be wired AFTER the clone")
        workspace.wire_harnesses(self.ws)
        _as_the_harness_would(self.local, "Bash(npm test *)")
        (config.ROOT / LOCAL).unlink()
        return wt

    def test_a_worktree_wired_after_its_clone_does_not_unhide_the_clones_local_file(self):
        wt = self.edited_clone_and_a_withdrawn_worktree()
        workspace.wire_harnesses(self.ws)
        self.assertFalse((wt / LOCAL).exists(), "fixture: the worktree's copy was not withdrawn")
        self.assertEqual(_status(self.clone), "")
        workspace.wire_harnesses(self.ws)
        self.assertEqual(_status(self.clone), "")

    def test_a_line_stays_while_any_tree_of_the_repository_still_holds_the_file(self):
        """Keep-on-existence across trees, not only in the tree being wired. After
        `unwire_guest` the clone has no marker to speak for its harness-edited file, so only
        the file's existence keeps its line when the worktree next settles the block."""
        wt = self.edited_clone_and_a_withdrawn_worktree()
        workspace.unwire_guest(self.clone)
        self.assertTrue(self.local.is_file(), "fixture: the harness-edited file must stay")
        workspace.wire_guest(wt)
        self.assertNotIn(LOCAL, _status(self.clone))

    def test_removing_a_workspace_holding_a_worktree_of_this_clone_leaves_the_clone_hidden(self):
        workspace.ensure("other")
        self.worktree("api", ws="other")
        workspace.wire_harnesses(self.ws)
        workspace.wire_harnesses("other")
        self.assertEqual(_status(self.clone), "", "fixture: the clone was not hidden to begin with")
        rc, said = self.invoke(commands_workspace.cmd_workspace_remove, name="other", force=True)
        self.assertEqual(rc, 0, said)
        self.assertEqual(_status(self.clone), "")

    def test_doctor_reads_every_tree_sharing_the_block_as_current(self):
        """`guest_layer` asks the same question the write answers. Compared against one tree's
        own list, a block that correctly holds its sibling's line reads `stale` for ever."""
        wt = self.edited_clone_and_a_withdrawn_worktree()
        workspace.wire_harnesses(self.ws)
        for tree in (self.clone, wt):
            self.assertEqual(dict(workspace.guest_layer(tree))[".git/info/exclude"], "ok",
                             tree.name)

    def test_a_clone_without_a_linked_worktree_asks_git_nothing(self):
        """A launch wires every checkout in the workspace, and a git spawn costs ~7 ms
        (measured). A common directory with no `worktrees/` has no tree but its own."""
        calls = self.git_asked()
        workspace.wire_harnesses(self.ws)
        self.assertEqual([c for c in calls if "worktree" in c], [])
        self.worktree("api-wt")
        workspace.wire_harnesses(self.ws)
        self.assertTrue([c for c in calls if "worktree" in c],
                        "fixture: git was never asked, so the first half proves nothing")

    def wire_the_worktree_while_git(self, answer) -> tuple[str, str]:
        """Wire the worktree while git answers *answer*; return the exclude file before it,
        and what `workspace.unaccounted` says about the worktree while git still answers so.

        **Byte for byte, not only "the local line is still there."** Nothing leaves the block
        and nothing new arrives, so the file must come back exactly as it was. The looser
        assertion let the block's own comment lines and its end marker be read back as paths:
        `/ <<< charter <<<` written into somebody's exclude file, one more on every wire —
        which is what the deletion sweep showed the `startswith("/")` filter deciding."""
        wt = self.edited_clone_and_a_withdrawn_worktree()
        before = self.excludes(self.clone)
        self.assertIn(f"/{LOCAL}", before, "fixture: the local line was not in the block")
        real = workspace.util.run

        def _git_says(cmd, *args, **kwargs):
            if "worktree" in cmd:
                return answer()
            return real(cmd, *args, **kwargs)

        with mock.patch.object(workspace.util, "run", _git_says):
            workspace.wire_guest(wt)
        self.assertFalse((wt / LOCAL).exists(), "fixture: the worktree's copy was not withdrawn")
        self.assertEqual(self.excludes(self.clone), before)
        with mock.patch.object(workspace.util, "run", _git_says):
            said = " ".join(workspace.unaccounted(wt))
        return before, said

    def test_git_that_refuses_to_list_the_worktrees_takes_no_line_away(self):
        """Nobody can say what the other trees need, so nothing leaves the block."""
        before, said = self.wire_the_worktree_while_git(
            lambda: subprocess.CompletedProcess([], 128, stdout="", stderr="fatal"))
        self.assertEqual(self.excludes(self.clone), before)
        self.assertIn("exited 128", said)

    def test_git_that_cannot_be_run_takes_no_line_away(self):
        def _missing():
            raise FileNotFoundError("git")

        before, said = self.wire_the_worktree_while_git(_missing)
        self.assertEqual(self.excludes(self.clone), before)
        self.assertIn("could not be run", said)

    def test_git_that_takes_too_long_takes_no_line_away(self):
        """Review round 3, N2: a 2 s hang made a launch take 5 s, and doctor with it, which runs
        from SessionStart. A timeout is `util.ProcTimeout` — a `RuntimeError`, which no
        `except OSError` catches — and it is one more way of not knowing."""
        def _hangs():
            raise util.ProcTimeout(["git", "worktree", "list"], doctor.CHECK_TIMEOUT)

        before, said = self.wire_the_worktree_while_git(_hangs)
        self.assertEqual(self.excludes(self.clone), before)
        self.assertIn("timed out", said)

    def test_git_is_given_doctors_check_timeout(self):
        """`doctor.CHECK_TIMEOUT`, the budget every read-only git question here already has."""
        wt = self.worktree("api-wt")
        given: list = []
        real = workspace.util.run

        def _record(cmd, *args, **kwargs):
            if "worktree" in cmd:
                given.append(kwargs.get("timeout"))
            return real(cmd, *args, **kwargs)

        with mock.patch.object(workspace.util, "run", _record):
            workspace.wire_guest(wt)
        self.assertTrue(given, "fixture: git was never asked")
        self.assertEqual(set(given), {doctor.CHECK_TIMEOUT})

    def test_a_worktrees_directory_that_cannot_be_checked_takes_no_line_away(self):
        """Review round 4, N1's class: `os.path.isdir(<common>/worktrees)` answered False for an
        EIO as well, so charter skipped git, took this checkout for the only tree there is, and
        dropped the sibling's line."""
        wt = self.edited_clone_and_a_withdrawn_worktree()
        admin = self.clone / ".git" / "worktrees"
        with _refused(files=[admin, Path(os.path.realpath(admin))],
                      calls=("lstat", "stat", "scandir")):
            workspace.wire_guest(wt)
        self.assertFalse((wt / LOCAL).exists(), "fixture: the worktree's copy was not withdrawn")
        self.assertEqual(_status(self.clone), "")

    def test_every_variable_git_treats_as_repository_local_is_withheld(self):
        """Review round 4: `GIT_DIR` and `GIT_WORK_TREE` one by one missed `GIT_COMMON_DIR`. The
        list is git's own answer to "which variables name a repository", measured on 2.50.1."""
        git = shutil.which("git")
        if git is None:
            self.skipTest("git is not installed")
        listed = subprocess.run([git, "rev-parse", "--local-env-vars"], capture_output=True,
                                text=True, check=True).stdout.split()
        self.assertTrue(listed, "fixture: git printed no repository-local variables")
        self.assertLessEqual(set(listed), set(workspace._GIT_ENV))

    def test_an_exported_git_common_dir_does_not_decide_which_trees_share_the_file(self):
        wt = self.edited_clone_and_a_withdrawn_worktree()
        unrelated = _repo(self.tmp / "unrelated")
        with mock.patch.dict(os.environ, {"GIT_COMMON_DIR": str(unrelated / ".git")}):
            workspace.wire_guest(wt)
        self.assertFalse((wt / LOCAL).exists(), "fixture: the worktree's copy was not withdrawn")
        self.assertEqual(_status(self.clone), "")

    def test_three_checkouts_of_one_repository_ask_git_once_per_wire(self):
        """Review round 4: the 5 s budget is per call. With git hung for 8 s and three checkouts
        sharing one common directory, a launch asked four times and took 20 s."""
        self.worktree("api-wt")
        self.worktree("api-wt2")
        calls = self.git_asked()
        workspace.wire_harnesses(self.ws)
        self.assertEqual(len([c for c in calls if "worktree" in c]), 1)
        workspace.wire_harnesses(self.ws)
        self.assertEqual(len([c for c in calls if "worktree" in c]), 2,
                         "a second wire must ask git again, not reuse the first wire's answer")

    def test_doctor_asks_git_once_per_repository(self):
        """Across workspaces too: `other` holds a worktree of this same clone."""
        self.worktree("api-wt")
        workspace.ensure("other")
        self.worktree("api", ws="other")
        workspace.wire_harnesses(self.ws)
        workspace.wire_harnesses("other")
        calls = self.git_asked()
        doctor.check_workspace_harness()
        self.assertEqual(len([c for c in calls if "worktree" in c]), 1)

    def test_guard_ask_asks_git_once_per_repository_across_workspaces(self):
        workspace.ensure("other")
        self.worktree("api", ws="other")
        self.worktree("api-wt")
        workspace.wire_harnesses(self.ws)
        workspace.wire_harnesses("other")
        calls = self.git_asked()
        self.at_the_plane()
        self.invoke(commands.cmd_guard_ask, pattern="kubectl delete *", local=False)
        self.assertEqual(len([c for c in calls if "worktree" in c]), 1)

    def test_an_exported_git_dir_does_not_decide_which_trees_share_the_file(self):
        """Review round 3: a launch with `GIT_DIR` exported — charter run from a git hook —
        asked THAT repository which worktrees there are, and dropped the sibling's line."""
        wt = self.edited_clone_and_a_withdrawn_worktree()
        unrelated = _repo(self.tmp / "unrelated")
        with mock.patch.dict(os.environ, {"GIT_DIR": str(unrelated / ".git"),
                                          "GIT_WORK_TREE": str(unrelated)}):
            workspace.wire_guest(wt)
        self.assertFalse((wt / LOCAL).exists(), "fixture: the worktree's copy was not withdrawn")
        self.assertEqual(_status(self.clone), "")


class ALostMarkerCannotUnhideTheLocalFile(PlaneWithRestrictions):
    """Review round 2: with no marker the harness-edited local file reads `harness-behind`,
    the first pass does not name it, `_charter_owned` returns nothing — and the next wire
    dropped its line. Measured three ways: `unwire_guest` then a launch, the marker deleted by
    hand, an empty marker. The marker write also truncated in place, so a concurrent reader
    could take a half-written marker for an empty one."""

    def setUp(self) -> None:
        super().setUp()
        _plane_local(config.ROOT, ask=["Bash(charter change land *)"])
        self.clone = self.checkout("api", real=True)
        workspace.wire_harnesses(self.ws)
        self.local = self.clone / LOCAL
        self.marker = self.clone / workspace.GENERATED_MARKER
        _as_the_harness_would(self.local, "Bash(npm test *)")

    def launch(self) -> None:
        """What a framed chat's launch runs for its workspace (`_launch_root` → `ensure`)."""
        workspace.ensure(self.ws)

    def assert_still_hidden(self) -> None:
        """EVERYTHING, not only the local file (review round 3, concern 1): main's stale block
        kept hiding `settings.json` after a lost marker, and round 2 dropped it — a file that
        now carries the plane's ask and deny rules."""
        self.assertEqual(_status(self.clone), "")
        self.assertIn(f"/{LOCAL}", self.excludes(self.clone))

    def test_unwiring_and_then_launching_keeps_it_hidden(self):
        workspace.unwire_guest(self.clone)
        self.launch()
        self.assertEqual(_status(self.clone), "")

    def test_a_marker_deleted_by_hand_does_not_unhide_it(self):
        self.marker.unlink()
        self.launch()
        self.assert_still_hidden()

    def test_an_empty_marker_does_not_unhide_it(self):
        self.marker.write_text("")
        self.launch()
        self.assert_still_hidden()
        # Review round 5: hiding follows the files that are there (R2), so an unreadable marker
        # leaves nothing unaccounted — and its old advice, "delete it with the files it named",
        # deleted the harness's approvals when followed (R5).
        self.assertEqual(workspace.unaccounted(self.clone), [])
        self.assertNotIn("delet", doctor.check_workspace_harness().hint)

    def test_a_lost_record_stays_hidden_after_charter_writes_a_new_one(self):
        """The record a later wire writes names only what that wire wrote. Read as the whole
        truth, it would unhide `settings.json` one launch later instead of at once."""
        self.marker.unlink()
        self.launch()
        agent = config.ROOT / ".claude" / "agents" / "steward.md"
        agent.parent.mkdir(parents=True, exist_ok=True)
        agent.write_text("# steward\n")
        self.launch()
        self.assertIn(".claude/agents/steward.md", json.loads(self.marker.read_text()),
                      "fixture: charter wrote no new record")
        self.assertEqual(_status(self.clone), "")

    def test_a_lost_marker_leaves_nothing_unaccounted_and_no_advice_to_delete(self):
        """Review round 5: a file that is there keeps its line by being there (R2), so a lost
        record is nothing doctor has to explain — and "delete it and the next launch writes and
        records charter's own" is advice that destroys what it names (R5)."""
        self.marker.unlink()
        self.launch()
        r = doctor.check_workspace_harness()
        self.assertEqual(_status(self.clone), "")
        self.assertNotIn("(unaccounted)", r.detail)
        self.assertNotIn("delet", r.hint)

    def test_what_could_not_be_accounted_for_comes_back_in_path_order(self):
        """Doctor prints these, and a report that reshuffles from run to run cannot be diffed.
        Eight kept paths, so a set iterating in path order by luck is a 1-in-40,320 accident
        rather than a coin toss — the round-3 sweep charged `sorted` here."""
        agents = config.ROOT / ".claude" / "agents"
        agents.mkdir(parents=True, exist_ok=True)
        for name in ("zulu", "yankee", "xray", "whiskey", "victor", "uniform", "tango"):
            (agents / f"{name}.md").write_text(f"# {name}\n")
        self.launch()
        self.marker.unlink()
        self.launch()
        kept = [self.clone / ".claude" / "agents" / f"{name}.md"
                for name in ("zulu", "yankee", "xray", "whiskey", "victor", "uniform", "tango")]
        kept.append(self.clone / SHARED)
        # Refused lstats, since review round 5: a path that is there keeps its line without a
        # reason, so the reasons left to order are the paths charter cannot check (R2).
        with _refused(files=kept, calls=("lstat",)):
            said = workspace.unaccounted(self.clone)
        self.assertEqual(len(said), 8, said)
        self.assertTrue(all("cannot be checked" in reason for reason in said), said)
        self.assertEqual(said, sorted(said))

    def test_a_path_that_cannot_be_checked_keeps_its_line_and_says_so(self):
        """Ruling G(e): absence is proved by `FileNotFoundError` or `NotADirectoryError`, and
        a refused `lstat` proves nothing."""
        self.marker.unlink()
        with _refused(files=[self.clone / SHARED]):
            workspace.wire_guest(self.clone)
        self.assertIn(f"/{SHARED}\n", self.excludes(self.clone))
        with _refused(files=[self.clone / SHARED]):
            said = " ".join(workspace.unaccounted(self.clone))
        self.assertIn(f"{SHARED} cannot be checked", said)

    def test_a_marker_that_is_not_an_object_is_no_record(self):
        self.marker.write_text("[]\n")
        self.launch()
        self.assert_still_hidden()

    def test_a_marker_that_cannot_be_opened_is_no_record(self):
        with _refused(files=[self.marker], calls=("open",)):
            self.launch()
        self.assert_still_hidden()

    def test_a_temp_file_left_by_an_interrupted_marker_write_does_not_show(self):
        """Review round 3: the marker is published through `config.replace_for`, whose temp
        file sits beside it — and a process killed between the write and the rename leaves
        that file in somebody's repository."""
        config.temp_beside(self.marker).write_text("{")
        self.launch()
        self.assertEqual(_status(self.clone), "")

    def test_a_reader_never_sees_a_half_written_marker(self):
        """Published whole, by rename. A reader that opened the marker before a wire reads
        the whole marker it opened; truncated in place, the same reader read whatever the
        writer had got to."""
        before = self.marker.read_text()
        agent = config.ROOT / ".claude" / "agents" / "steward.md"
        agent.parent.mkdir(parents=True, exist_ok=True)
        agent.write_text("# steward\n")
        with self.marker.open() as reader:
            workspace.wire_harnesses(self.ws)
            self.assertEqual(reader.read(), before)
        self.assertIn(".claude/agents/steward.md", json.loads(self.marker.read_text()),
                      "fixture: the wire did not rewrite the marker")


class AnUnreadableLocalFileIsNotCalledForeign(PlaneWithRestrictions):
    """Review round 2: a co-written file charter cannot read was folded into `foreign`, and
    `guard ask` and `reinit` then advised removing it — a file that holds the harness's
    approvals. It is said to be unreadable, left exactly as it is, and kept hidden."""

    def setUp(self) -> None:
        super().setUp()
        _plane_local(config.ROOT, ask=["Bash(charter change land *)"])
        self.clone = self.checkout("api", real=True)
        workspace.wire_harnesses(self.ws)
        self.local = self.clone / LOCAL
        self.theirs = _as_the_harness_would(self.local, "Bash(npm test *)")
        self.local.chmod(0o000)
        self.addCleanup(self.local.chmod, 0o600)
        with self.assertRaises(PermissionError, msg="fixture: this user can read a 000 file"):
            self.local.read_text()
        _plane_local(config.ROOT, ask=["Bash(charter change land *)", "Bash(kubectl *)"])

    def test_the_row_says_it_cannot_be_read(self):
        self.assertEqual(dict(workspace.wire_guest(self.clone))[LOCAL], "unreadable")

    def test_guard_ask_says_it_cannot_be_read_and_never_advises_removal(self):
        self.at_the_plane()
        _, said = self.invoke(commands.cmd_guard_ask, pattern="helm uninstall *", local=True)
        self.assertIn(f"{self.ws}/api/{LOCAL} cannot be read", said)
        self.assertNotIn("Remove", said)
        self.assertNotIn("charter did not write", said)
        # Review round 3, N3: "until that file can be read" was false of a harness-edited file,
        # which reads `harness-behind` once it can be read and never gets the rule.
        self.assertNotIn("until that file can be read", said)
        self.assertIn("only if that file turns out to be exactly what charter last wrote", said)

    def test_reinit_says_it_cannot_be_read_and_leaves_it_as_it_is(self):
        said = " ".join(self.reinit_said())
        self.assertIn(f"api/{LOCAL} cannot be read", said)
        self.assertNotIn("Remove", said)
        self.assertNotIn("not written by charter", said)
        self.assertNotIn("until it can be", said)
        self.assertIn("only if that file turns out to be exactly what charter last wrote", said)
        self.local.chmod(0o600)
        self.assertEqual(self.local.read_text(), self.theirs)
        self.assertEqual(_status(self.clone), "")


class AGoneLocalFileLetsItsLineGo(PlaneWithRestrictions):
    """Review round 2: a co-written file that was gone and no longer wanted kept its marker
    entry and its exclude line for ever — a line hiding nothing, naming a path charter no
    longer has. Once it is gone from every tree and unwanted, both go."""

    def setUp(self) -> None:
        super().setUp()
        _plane_local(config.ROOT, ask=["Bash(charter change land *)"])
        self.clone = self.checkout("api", real=True)
        workspace.wire_harnesses(self.ws)
        _as_the_harness_would(self.clone / LOCAL, "Bash(npm test *)")
        (config.ROOT / LOCAL).unlink()
        workspace.wire_harnesses(self.ws)
        self.assertIn(f"/{LOCAL}", self.excludes(self.clone),
                      "fixture: the harness-edited file should still be hidden")

    def test_once_it_is_gone_its_marker_entry_and_its_line_go(self):
        (self.clone / LOCAL).unlink()
        workspace.wire_harnesses(self.ws)
        self.assertNotIn(LOCAL, json.loads(
            (self.clone / workspace.GENERATED_MARKER).read_text()))
        self.assertNotIn(f"/{LOCAL}", self.excludes(self.clone))
        self.assertEqual(_status(self.clone), "")


class HarnessBehindIsTrueOfAFileCharterNeverWrote(PlaneWithRestrictions):
    """Review round 2: every `harness-behind` sentence said charter "no longer rewrites" the
    file. For a local file that was there before charter wired the clone, charter never
    rewrote it at all — the sentence has to be true of both."""

    def setUp(self) -> None:
        super().setUp()
        _plane_local(config.ROOT, ask=["Bash(charter change land *)"])
        self.fresh = _repo(workspace.workspace_dir(self.ws) / "fresh")
        (self.fresh / ".claude").mkdir()
        (self.fresh / LOCAL).write_text(
            json.dumps({"permissions": {"allow": ["Bash(make *)"]}}, indent=2) + "\n")

    def test_doctor(self):
        workspace.wire_harnesses(self.ws)
        r = doctor.check_workspace_harness()
        self.assertIn(f"{self.ws}/fresh/{LOCAL} (harness-behind)", r.detail)
        self.assertIn("the harness saves its approvals into that file", r.hint)
        self.assertNotIn("no longer", r.hint)

    def test_guard_ask(self):
        self.at_the_plane()
        _, said = self.invoke(commands.cmd_guard_ask, pattern="kubectl *", local=True)
        self.assertIn(f"{self.ws}/fresh/{LOCAL}", said)
        self.assertIn("the harness saves its approvals into that file", said)
        self.assertNotIn("no longer", said)

    def test_reinit(self):
        said = " ".join(self.reinit_said())
        self.assertIn(f"fresh/{LOCAL}", said)
        self.assertIn("the harness saves its approvals into that file", said)
        self.assertNotIn("no longer", said)


class SilenceIsNotAVerdict(PlaneWithRestrictions):
    """Review round 4, ruling H: a line for a path that EXISTS leaves charter's block only when a
    record saying charter does not own that path was both PUBLISHED and READ.

    A generated file charter had written, whose new digest never reached the marker, read as
    somebody else's file: its old digest no longer matched. It happened on a SIGKILL just before
    the marker was published (on main too) and, since round 2 publishes through a temp file, on
    every launch into a checkout whose root is not writable — `?? .claude/settings.json`, with
    doctor saying "all current". Kills are injected at a point rather than sent: nothing after
    the point runs, as after a real SIGKILL."""

    def setUp(self) -> None:
        super().setUp()
        _plane_local(config.ROOT, ask=["Bash(charter change land *)"])
        self.clone = self.checkout("api", real=True)
        workspace.wire_harnesses(self.ws)
        self.assertEqual(_status(self.clone), "", "fixture: the clone was not hidden to begin with")
        self.shared = self.clone / SHARED
        self.marker = self.clone / workspace.GENERATED_MARKER

    def plane_moves(self) -> str:
        """The plane gains a rule, so the next wire rewrites the clone's shared settings."""
        _plane_settings(config.ROOT, permissions={
            "ask": ["Bash(terraform apply *)", "Bash(kubectl *)"], "deny": ["Bash(rm -rf /)"]})
        new = workspace._guest_files(self.clone)[SHARED]
        self.assertNotEqual(self.shared.read_text(), new, "fixture: the plane did not move")
        return new

    def test_a_kill_after_writing_a_file_and_before_recording_it_leaves_it_hidden(self):
        new = self.plane_moves()
        real = os.replace

        # At the rename that publishes the marker, whichever writer makes it (round 5 moved the
        # marker onto `_write_whole`): a temp left behind, and no record published.
        def _killed_after_the_write(src, dst, *args, **kwargs):
            if os.fspath(dst) == os.fspath(self.marker) and self.shared.read_text() == new:
                shutil.copyfile(src, config.temp_beside(self.marker))
                raise _Killed()
            return real(src, dst, *args, **kwargs)

        with mock.patch("os.replace", _killed_after_the_write):
            with self.assertRaises(_Killed):
                workspace.wire_harnesses(self.ws)
        self.assertEqual(self.shared.read_text(), new, "fixture: the kill came before the write")
        workspace.ensure(self.ws)
        self.assertEqual(_status(self.clone), "")
        self.assertEqual(json.loads(self.marker.read_text())[SHARED],
                         workspace.content_digest(new))

    def test_a_kill_halfway_through_writing_a_file_leaves_it_hidden_and_rewrites_it(self):
        new = self.plane_moves()
        with _torn_write_of(self.shared):
            with self.assertRaises(_Killed):
                workspace.wire_harnesses(self.ws)
        workspace.ensure(self.ws)
        self.assertEqual(_status(self.clone), "")
        self.assertEqual(self.shared.read_text(), new)

    def test_a_checkout_root_charter_cannot_write_keeps_every_line_and_doctor_says_so(self):
        new = self.plane_moves()
        self.clone.chmod(0o555)
        self.addCleanup(self.clone.chmod, 0o755)
        with self.assertRaises(PermissionError, msg="fixture: this user can write a 555 directory"):
            (self.clone / "probe").write_text("x")
        workspace.ensure(self.ws)
        self.assertEqual(_status(self.clone), "")
        r = doctor.check_workspace_harness()
        self.assertIn(f"{self.ws}/api/{workspace.GENERATED_MARKER} (unrecorded)", r.detail)
        # Review round 5: the errno of the publish that failed, not a guess from a mode bit
        # (R5), and a hint that leads with what clears it rather than with `reinit` (R4).
        self.assertIn("EACCES", r.hint)
        self.assertTrue(r.hint.startswith("An 'unrecorded' marker"), r.hint)
        told = _briefing(self, self.clone)
        self.assertIn("Bash(kubectl *)", told)
        self.assertIn("restore write access to this checkout", told)
        self.assertIn("could not publish its record there first", " ".join(self.reinit_said()))
        self.at_the_plane()
        _, said = self.invoke(commands.cmd_guard_ask, pattern="helm uninstall *", local=False)
        self.assertIn("could not publish its record there first", said)
        self.assertEqual(_status(self.clone), "")
        self.assertNotEqual(self.shared.read_text(), new,
                            "fixture: charter wrote a file it could not record first")
        self.clone.chmod(0o755)
        workspace.ensure(self.ws)
        self.assertEqual(_status(self.clone), "")
        # What the plane wants NOW: the `guard ask` above moved it again while the root was
        # read-only, and the first launch that can record its write brings the file all the way.
        self.assertEqual(self.shared.read_text(), workspace._guest_files(self.clone)[SHARED])

    def test_a_checkout_root_charter_cannot_write_is_no_finding_while_nothing_needs_writing(self):
        """`unrecorded` is about a record a launch needs to publish, not about a mode bit."""
        self.clone.chmod(0o555)
        self.addCleanup(self.clone.chmod, 0o755)
        r = doctor.check_workspace_harness()
        self.assertNotIn("unrecorded", f"{r.detail} {r.hint}")

    def test_a_kill_during_a_first_wire_leaves_no_temp_file_showing(self):
        """Minor 1: the pattern line is in the block before the first temp file exists — the
        first pass names the marker before anything is published."""
        fresh = _repo(workspace.workspace_dir(self.ws) / "fresh")
        marker = fresh / workspace.GENERATED_MARKER
        real = os.replace

        def _killed_inside_the_publish(src, dst, *args, **kwargs):
            if os.fspath(dst) == os.fspath(marker):
                shutil.copyfile(src, config.temp_beside(marker))
                raise _Killed()
            return real(src, dst, *args, **kwargs)

        with mock.patch("os.replace", _killed_inside_the_publish):
            with self.assertRaises(_Killed):
                workspace.wire_harnesses(self.ws)
        workspace.ensure(self.ws)
        self.assertEqual(_status(fresh), "")

    def test_a_stale_temp_file_keeps_its_line_while_it_is_there(self):
        """…and it stays while such a file is there, whatever became of the marker's own line."""
        config.temp_beside(self.marker).write_text("{")
        self.marker.unlink()
        workspace.ensure(self.ws)
        self.assertEqual(_status(self.clone), "")


class ClassHitsInReinitSayWhatTheyCouldNotCheck(PlaneWithRestrictions):
    """Review round 4, minor 2's class, in `cmd_workspace_reinit`: `Path.exists` read an error as
    absence — "no workspace", "workspace.json could not be written" — and raised on 3.11–3.13."""

    def test_a_workspace_directory_that_cannot_be_checked_is_not_called_missing(self):
        wd = workspace.workspace_dir(self.ws)
        with _refused(files=[wd], calls=("lstat", "stat")):
            said = " ".join(self.reinit_said())
        self.assertIn(f"workspace '{self.ws}' cannot be checked", said)
        self.assertNotIn("no workspace", said)

    def test_a_manifest_that_cannot_be_checked_is_not_called_unwritten(self):
        manifest = workspace.manifest_path(self.ws)
        with _refused(files=[manifest], calls=("lstat", "stat")):
            said = " ".join(self.reinit_said())
        self.assertIn("workspace.json cannot be checked", said)
        self.assertNotIn("could not be written", said)


class RoundFiveCheckout(PlaneWithRestrictions):
    """One real clone, wired, with the plane's local rule in it — review round 5's fixture."""

    def setUp(self) -> None:
        super().setUp()
        _plane_local(config.ROOT, ask=["Bash(charter change land *)"])
        self.clone = self.checkout("api", real=True)
        workspace.wire_harnesses(self.ws)
        self.shared = self.clone / SHARED
        self.local = self.clone / LOCAL
        self.marker = self.clone / workspace.GENERATED_MARKER
        self.exclude = workspace.git_exclude_file(self.clone)
        self.assertEqual(_status(self.clone), "", "fixture: the clone was not hidden to begin with")

    def plane_moves_shared(self, *ask: str) -> str:
        _plane_settings(config.ROOT, permissions={
            "ask": ["Bash(terraform apply *)", *(ask or ("Bash(kubectl *)",))],
            "deny": ["Bash(rm -rf /)"]})
        new = workspace._guest_files(self.clone)[SHARED]
        self.assertNotEqual(self.shared.read_text(), new, "fixture: the plane did not move")
        return new

    def plane_moves_local(self) -> str:
        _plane_local(config.ROOT, ask=["Bash(charter change land *)"],
                     deny=["Bash(git push --force *)"])
        return workspace._guest_files(self.clone)[LOCAL]

    def killed_after_the_pending_record(self):
        """SIGKILL right after the PENDING record is published and before the file is written."""
        real = os.replace

        def after(src, dst, *args, **kwargs):
            result = real(src, dst, *args, **kwargs)
            if os.fspath(dst) == os.fspath(self.marker) and any(
                    isinstance(v, list) for v in json.loads(Path(dst).read_text()).values()):
                raise _Killed()
            return result

        return mock.patch("os.replace", after)


class R1AFileCharterWritesIsOldOrNewNeverTorn(RoundFiveCheckout):
    """Review round 5, R1: every file charter writes in a checkout or its common git directory
    is written to a temp beside it, fsynced, then renamed over it — so it only ever holds the
    old content or the new. Written in place, a kill left `settings.local.json` at 68 or 0 bytes
    that read as harness-edited (the plane's new `deny` never arrived, doctor said all current),
    and an `info/exclude` killed after the truncate lost the operator's own lines."""

    def assert_a_halfway_kill_leaves_it_whole(self, target: Path, old: str, new: str) -> None:
        with _torn_write_of(target), self.assertRaises(_Killed):
            workspace.wire_harnesses(self.ws)
        self.assertIn(target.read_text(), (old, new))

    def test_settings_json_is_old_or_new(self):
        old = self.shared.read_text()
        self.assert_a_halfway_kill_leaves_it_whole(self.shared, old, self.plane_moves_shared())

    def test_settings_local_json_is_old_or_new(self):
        old = self.local.read_text()
        self.assert_a_halfway_kill_leaves_it_whole(self.local, old, self.plane_moves_local())

    def test_info_exclude_keeps_the_operators_own_line_after_the_block(self):
        self.exclude.write_text(self.exclude.read_text() + "their-own-line-after\n")
        old = self.exclude.read_text()
        agent = config.ROOT / ".claude" / "agents" / "steward.md"
        agent.parent.mkdir(parents=True, exist_ok=True)
        agent.write_text("# steward\n")
        with _torn_write_of(self.exclude), self.assertRaises(_Killed):
            workspace.wire_harnesses(self.ws)
        self.assertEqual(self.exclude.read_text(), old)

    def test_a_file_is_fsynced_before_it_replaces_the_old_one(self):
        new = self.plane_moves_shared()
        events: list[tuple[str, str]] = []
        real_fsync, real_replace = os.fsync, os.replace

        def fsync(fd):
            events.append(("fsync", ""))
            return real_fsync(fd)

        def replace(src, dst, *args, **kwargs):
            events.append(("replace", os.fspath(dst)))
            return real_replace(src, dst, *args, **kwargs)

        with mock.patch("os.fsync", fsync), mock.patch("os.replace", replace):
            workspace.wire_harnesses(self.ws)
        self.assertEqual(self.shared.read_text(), new)
        renames = [i for i, (kind, dst) in enumerate(events)
                   if kind == "replace" and dst == os.fspath(self.shared)]
        self.assertTrue(renames, "settings.json was not published by rename")
        for i in renames:
            self.assertEqual(events[i - 1][0], "fsync", events)

    def test_a_temp_file_a_kill_left_beside_a_generated_file_stays_hidden(self):
        (self.clone / ".claude" / ".charter-generated.4242.0123456789ab.tmp").write_text("{")
        workspace.ensure(self.ws)
        self.assertEqual(_status(self.clone), "")

    def test_an_exclude_file_keeps_its_own_mode_through_a_rewrite(self):
        self.exclude.chmod(0o600)
        agent = config.ROOT / ".claude" / "agents" / "steward.md"
        agent.parent.mkdir(parents=True, exist_ok=True)
        agent.write_text("# steward\n")
        workspace.wire_harnesses(self.ws)
        self.assertIn("/.claude/agents/steward.md", self.excludes(self.clone),
                      "fixture: the block did not change")
        self.assertEqual(stat.S_IMODE(self.exclude.stat().st_mode), 0o600)

    def test_an_exclude_file_that_is_a_symlink_stays_one(self):
        """`Path.write_text` wrote through a link. A rename over one would replace somebody's
        shared exclude file with a private copy of it."""
        shared = self.tmp / "shared-exclude"
        shared.write_text(self.exclude.read_text())
        self.exclude.unlink()
        self.exclude.symlink_to(shared)
        agent = config.ROOT / ".claude" / "agents" / "steward.md"
        agent.parent.mkdir(parents=True, exist_ok=True)
        agent.write_text("# steward\n")
        workspace.wire_harnesses(self.ws)
        self.assertTrue(self.exclude.is_symlink())
        self.assertIn("/.claude/agents/steward.md", shared.read_text())


class R2AFileStaysHiddenWhileItIsThere(RoundFiveCheckout):
    """Review round 5, R2: in a checkout charter wires, a line for a charter path stays while
    that path exists. A record — pending, settled, lost, raced — never takes one out."""

    def test_two_launches_racing_to_settle_never_unhide_the_file(self):
        """Wire A is held before its settle; wire B rewrites and settles; A settles LAST, with a
        digest the file no longer has. The file then read as somebody else's and its line left,
        surviving the next launch and `reinit`."""
        first = workspace.content_digest(self.plane_moves_shared("Bash(kubectl *)"))
        real = os.replace
        raced: list[bool] = []

        def racing(src, dst, *args, **kwargs):
            if not raced and os.fspath(dst) == os.fspath(self.marker):
                if json.loads(Path(src).read_text()).get(SHARED) == first:
                    raced.append(True)
                    self.plane_moves_shared("Bash(helm uninstall *)")
                    with mock.patch("os.replace", real):
                        workspace.wire_harnesses(self.ws)
            return real(src, dst, *args, **kwargs)

        with mock.patch("os.replace", racing):
            workspace.wire_harnesses(self.ws)
        self.assertTrue(raced, "fixture: wire A never reached its settle")
        workspace.ensure(self.ws)
        workspace.ensure(self.ws)
        self.assertEqual(_status(self.clone), "")
        # And the file is still charter's to bring up to date: A's settle recorded text the file
        # no longer held, and the plane's next move would have called charter's own file foreign.
        self.plane_moves_shared("Bash(argocd app delete *)")
        workspace.ensure(self.ws)
        self.assertEqual(self.shared.read_text(), workspace._guest_files(self.clone)[SHARED])


class R3OnlyContentCharterKnowsIsOverwritten(RoundFiveCheckout):
    """Review round 5, R3: a record licenses overwriting only content whose digest it lists."""

    def test_a_hand_edit_made_while_a_write_was_pending_is_never_overwritten(self):
        self.plane_moves_shared()
        with self.killed_after_the_pending_record(), self.assertRaises(_Killed):
            workspace.wire_harnesses(self.ws)
        theirs = '{"env": {"THEIRS": "1"}}\n'
        self.shared.write_text(theirs)
        workspace.ensure(self.ws)
        self.assertEqual(self.shared.read_text(), theirs)
        self.assertEqual(_status(self.clone), "")
        self.assertIn(f"{self.ws}/api/{SHARED} (foreign)", doctor.check_workspace_harness().detail)

    def test_an_approval_saved_after_an_interrupted_local_write_reads_behind(self):
        self.plane_moves_local()
        with self.killed_after_the_pending_record(), self.assertRaises(_Killed):
            workspace.wire_harnesses(self.ws)
        _as_the_harness_would(self.local, "Bash(npm test *)")
        workspace.ensure(self.ws)
        self.assertEqual(dict(workspace.guest_layer(self.clone))[LOCAL], "harness-behind")


class R4TheChatIsToldWhatIsNotInForce(RoundFiveCheckout):
    """Review round 5, R4: where the plane's ask/deny rules are not in force in the checkout a
    chat is rooted in, that chat's SessionStart context says which, and what fixes it — and
    `reinit` never calls such a checkout up to date."""

    def setUp(self) -> None:
        super().setUp()
        _isolation.no_background_refresh(self)
        _as_the_harness_would(self.local, "Bash(npm test *)")
        self.plane_moves_local()
        workspace.ensure(self.ws)
        self.assertEqual(dict(workspace.guest_layer(self.clone))[LOCAL], "harness-behind",
                         "fixture: the local file is not the harness's")

    def test_the_chat_is_told_the_rule_and_the_fix(self):
        told = _briefing(self, self.clone)
        self.assertIn("Bash(git push --force *)", told)
        self.assertIn(f"add them to {LOCAL} by hand", told)
        # The rule the file already holds is not named as missing: the line reads the file.
        self.assertNotIn("Bash(charter change land *)", told)

    def test_a_chat_where_every_rule_is_in_force_is_told_nothing(self):
        fresh = self.checkout("fresh", real=True)
        workspace.wire_harnesses(self.ws)
        self.assertNotIn("NOT in force", _briefing(self, fresh))
        self.assertNotIn("NOT in force", _briefing(self, workspace.workspace_dir(self.ws)))

    def test_reinit_never_says_up_to_date_while_a_rule_is_not_in_force(self):
        said = " ".join(self.reinit_said())
        self.assertNotIn("Up to date", said)
        self.assertIn("not in force", said)


class R5TrueReasonsAndNoDestructiveAdvice(PlaneWithRestrictions):
    """Review round 5, R5: `git worktree prune` only for a path charter itself found absent, and
    `unrecorded` names the errno of the publish that failed."""

    def setUp(self) -> None:
        super().setUp()
        _plane_local(config.ROOT, ask=["Bash(charter change land *)"])

    def test_a_worktree_git_calls_prunable_that_is_still_there_is_not_advised_pruned(self):
        clone = self.checkout("api", real=True)
        workspace.ensure("other")
        other = workspace.workspace_dir("other") / "api"
        _git(clone, "worktree", "add", "-q", "-b", "wt-other", str(other))
        workspace.wire_harnesses(self.ws)
        workspace.wire_harnesses("other")
        _as_the_harness_would(other / LOCAL, "Bash(npm test *)")
        (config.ROOT / LOCAL).unlink()
        other.chmod(0o000)
        self.addCleanup(other.chmod, 0o755)
        self.assertIn("prunable", _git(clone, "worktree", "list", "--porcelain").stdout,
                      "fixture: git does not call an unreadable worktree prunable")
        workspace.wire_harnesses(self.ws)
        said = " ".join(workspace.unaccounted(clone))
        self.assertIn("unreadable: restore access", said)
        self.assertNotIn("prune", said)
        other.chmod(0o755)
        self.assertEqual(_status(other), "")

    def test_a_record_that_cannot_be_published_names_its_errno(self):
        clone = self.checkout("api", real=True)
        workspace.wire_harnesses(self.ws)
        _plane_settings(config.ROOT, permissions={
            "ask": ["Bash(terraform apply *)", "Bash(kubectl *)"], "deny": ["Bash(rm -rf /)"]})
        marker = clone / workspace.GENERATED_MARKER
        real = os.replace

        def refusing(src, dst, *args, **kwargs):
            if os.fspath(dst) == os.fspath(marker):
                raise OSError(errno.EROFS, "Read-only file system", os.fspath(dst))
            return real(src, dst, *args, **kwargs)

        with mock.patch("os.replace", refusing):
            said = " ".join(self.reinit_said())
            r = doctor.check_workspace_harness()
        self.assertIn("EROFS", said)
        self.assertIn("EROFS", f"{r.detail} {r.hint}")
        self.assertEqual(_status(clone), "")


class AFileCharterWritesAgainIsSettled(RoundFiveCheckout):
    """Final review (#942), a regression against efdd827: a launch that wrote a deleted file again
    published its pending intent and then skipped the settle, because the record it ended with
    equalled the one it READ at the start. The entry stayed pending for good, and since R3 a
    pending entry never reads `harness-edited` — so the harness's first approval left the file
    `harness-behind` for ever, with every rule in it and nothing that cleared the warning."""

    def test_a_local_file_written_again_takes_an_approval_as_harness_edited(self):
        self.local.unlink()
        workspace.ensure(self.ws)
        self.assertEqual(self.local.read_text(), workspace._guest_files(self.clone)[LOCAL],
                         "fixture: the launch did not write the file again")
        self.assertEqual(json.loads(self.marker.read_text())[LOCAL],
                         workspace.content_digest(self.local.read_text()))
        _as_the_harness_would(self.local, "Bash(npm test *)")
        workspace.ensure(self.ws)
        self.assertEqual(dict(workspace.guest_layer(self.clone))[LOCAL], "harness-edited")
        self.assertNotIn("harness-behind", doctor.check_workspace_harness().detail)


class AWorktreeListGitCannotBackUpKeepsEveryLine(RoundFiveCheckout):
    """#942 final review, ruling G's own class: `git worktree list` exiting 0 is not a whole list.

    Measured on git 2.50.1. With `<common>/worktrees` unreadable, or one worktree's directory in
    it, or only that worktree's `gitdir` file, git lists the rest, exits 0, and says nothing of the
    one it could not read. Charter took that short list for the whole one and dropped the line a
    live sibling still needed: its `.claude/settings.local.json`, the harness's approvals. With only
    `gitdir` unreadable, that sibling still works, and its `git status` showed the file at once."""

    def setUp(self) -> None:
        super().setUp()
        workspace.ensure("other")
        self.other = workspace.workspace_dir("other") / "api"
        _git(self.clone, "worktree", "add", "-q", "-b", "wt-other", str(self.other))
        workspace.wire_harnesses(self.ws)
        workspace.wire_harnesses("other")
        _as_the_harness_would(self.other / LOCAL, "Bash(npm test *)")
        (config.ROOT / LOCAL).unlink()
        self.admin = workspace.git_exclude_file(self.clone).parent.parent / "worktrees"
        self.entry = next(
            p for p in self.admin.iterdir()
            if os.path.realpath(Path((p / "gitdir").read_text().strip()).parent)
            == os.path.realpath(self.other))

    def wired_while_unreadable(self, locked: Path, mode: int) -> str:
        """Wire from the main checkout with *locked* at mode 000; doctor's hint, asked then."""
        locked.chmod(0o000)
        self.addCleanup(locked.chmod, mode)
        workspace.wire_harnesses(self.ws)
        hint = doctor.check_workspace_harness().hint
        locked.chmod(mode)
        return hint

    def assert_every_line_stays(self, hint: str, named: Path) -> None:
        self.assertFalse(self.local.exists(), "fixture: the clone's own copy was not withdrawn")
        self.assertIn(f"/{LOCAL}", self.excludes(self.clone).splitlines())
        self.assertEqual(_status(self.other), "")
        self.assertIn(f"{named} cannot be checked — restoring read access clears this", hint)

    def test_the_worktrees_directory_unreadable(self):
        self.assert_every_line_stays(self.wired_while_unreadable(self.admin, 0o755), self.admin)

    def test_one_worktrees_directory_unreadable(self):
        self.assert_every_line_stays(self.wired_while_unreadable(self.entry, 0o755),
                                     self.entry / "gitdir")

    def test_a_stray_file_among_the_worktrees_takes_no_trust_away(self):
        """Git passes over a file in `worktrees/` that is no worktree's directory (measured), and
        so does charter: the line still leaves once no checkout holds the file."""
        (self.admin / "README").write_text("stray\n")
        (self.other / LOCAL).unlink()
        workspace.wire_harnesses(self.ws)
        self.assertFalse(self.local.exists(), "fixture: the clone's own copy was not withdrawn")
        self.assertNotIn(f"/{LOCAL}", self.excludes(self.clone).splitlines())

    def test_one_worktrees_gitdir_missing(self):
        """Git leaves a worktree whose `gitdir` file is gone out of its list, exits 0, and marks
        nothing prunable (measured)."""
        gitdir = self.entry / "gitdir"
        saved = gitdir.read_bytes()

        def put_back() -> None:
            if not gitdir.exists():
                gitdir.write_bytes(saved)

        gitdir.unlink()
        self.addCleanup(put_back)
        workspace.wire_harnesses(self.ws)
        hint = doctor.check_workspace_harness().hint
        put_back()
        self.assertFalse(self.local.exists(), "fixture: the clone's own copy was not withdrawn")
        self.assertIn(f"/{LOCAL}", self.excludes(self.clone).splitlines())
        self.assertEqual(_status(self.other), "")
        self.assertIn(f"{gitdir} is missing", hint)
        self.assertIn("git worktree repair", hint)

    def test_one_worktrees_gitdir_unreadable_while_that_worktree_still_works(self):
        gitdir = self.entry / "gitdir"
        gitdir.chmod(0o000)
        self.addCleanup(gitdir.chmod, 0o644)
        workspace.wire_harnesses(self.ws)
        self.assertEqual(_status(self.other), "", "the sibling's local settings showed while it worked")
        hint = doctor.check_workspace_harness().hint
        gitdir.chmod(0o644)
        self.assert_every_line_stays(hint, gitdir)


class ARefsDirectoryAtModeZeroIsNamedNotRaised(RoundFiveCheckout):
    """Final review addendum (#942): a `refs` directory charter cannot read, and `charter workspace
    reinit` and `charter doctor`.

    Measured before the fix, at the CLI in a throwaway plane, on 3.14 and 3.11, on this branch
    AND on main (3286a4f): a clone's `.git/refs` at mode 000 never raised anywhere — reinit rc 0.
    The WORKSPACE's own `refs/` at mode 000, which is what the round-4 probe locked, raised a
    PermissionError out of `reinit` everywhere (`scaffold`'s `Path.exists` on 3.11–3.13, its
    write on 3.14) and out of `doctor` on 3.11 (`gitpolicy.repos`). ADR 0009: name the path
    charter could not check and what clears it, and change nothing it cannot see."""

    def reinit(self) -> tuple[int, str]:
        said: list[str] = []
        with mock.patch.object(commands_workspace.util, "ok", side_effect=said.append), \
             mock.patch.object(commands_workspace.util, "warn", side_effect=said.append), \
             mock.patch.object(commands_workspace.util, "err", side_effect=said.append), \
             mock.patch.object(commands_workspace.util, "info", side_effect=lambda m: None):
            rc = commands_workspace.cmd_workspace_reinit(SimpleNamespace(name=self.ws, all=False))
        return rc, " ".join(said)

    def locked(self, path: Path) -> None:
        path.chmod(0o000)
        self.addCleanup(path.chmod, 0o755)

    def test_a_clones_unreadable_git_refs_costs_nothing(self):
        refs = self.clone / ".git" / "refs"
        self.locked(refs)
        rc, said = self.reinit()
        self.assertEqual(rc, 0, said)
        self.assertNotIn("cannot be checked", f"{said} {doctor.check_ssh().hint}")
        doctor.check_workspace_harness()
        refs.chmod(0o755)
        self.assertEqual(_status(self.clone), "")

    def test_a_workspace_refs_directory_is_named_by_reinit_and_doctor(self):
        self.locked(workspace.refs_dir(self.ws))
        rc, said = self.reinit()
        self.assertEqual(rc, 0, said)
        self.assertIn("refs/README.md cannot be checked", said)
        self.assertIn("restoring read access", said)
        self.assertNotIn("Up to date", said)
        r = doctor.check_ssh()
        self.assertEqual(r.status, doctor.WARN)
        self.assertIn(f"{self.ws}/refs", f"{r.detail} {r.hint}")
        self.assertIn("read access", r.hint)
        workspace.refs_dir(self.ws).chmod(0o755)
        self.assertEqual(_status(self.clone), "")

    def test_doctor_names_a_directory_it_cannot_look_into_where_every_repo_is_token_only(self):
        """doctor.py `check_ssh`: the row goes WARN for it alone, and leads with the path."""
        self.locked(workspace.refs_dir(self.ws))
        with mock.patch.object(gitpolicy, "check", return_value=[]):
            r = doctor.check_ssh()
        self.assertEqual(r.status, doctor.WARN)
        self.assertTrue(r.hint.startswith(f"{self.ws}/refs cannot be checked"), r.hint)


class TheFixForAnUnpublishedRecordMatchesItsErrno(RoundFiveCheckout):
    """Final review (#942): every failed publish was told to "restore write access", and a full
    disk or a read-only mount has no write access to restore. The advice follows the errno."""

    def refused_with(self, code: int, says: str) -> dict[str, str]:
        """What the chat, doctor, reinit and `guard ask` say once a launch's record publish failed
        with *code*."""
        self.plane_moves_shared()
        real = os.replace

        def refusing(src, dst, *args, **kwargs):
            if os.fspath(dst) == os.fspath(self.marker):
                raise OSError(code, says, os.fspath(dst))
            return real(src, dst, *args, **kwargs)

        with mock.patch("os.replace", refusing):
            workspace.ensure(self.ws)
            reinit = " ".join(self.reinit_said())
            self.at_the_plane()
            _, guard = self.invoke(commands.cmd_guard_ask, pattern="helm uninstall *", local=False)
        self.assertEqual(_status(self.clone), "")
        return {"chat": _briefing(self, self.clone), "doctor": doctor.check_workspace_harness().hint,
                "reinit": reinit, "guard ask": guard}

    def test_a_full_disk_is_told_to_free_space(self):
        for who, said in self.refused_with(errno.ENOSPC, "No space left on device").items():
            self.assertIn("free space", said, who)
            self.assertNotIn("write access", said, who)

    def test_a_read_only_mount_is_told_to_remount_it(self):
        for who, said in self.refused_with(errno.EROFS, "Read-only file system").items():
            self.assertIn("read-only", said, who)
            self.assertNotIn("write access", said, who)

    def test_a_refused_permission_is_told_to_restore_write_access(self):
        for who, said in self.refused_with(errno.EACCES, "Permission denied").items():
            self.assertIn("write access", said, who)
            self.assertNotIn("free space", said, who)

    def test_any_other_refusal_names_its_errno_and_claims_no_cause(self):
        for who, said in self.refused_with(errno.EIO, "Input/output error").items():
            self.assertIn("EIO", said, who)
            self.assertIn("what stops writes", said, who)
            for claim in ("write access", "free space", "read-only"):
                self.assertNotIn(claim, said, who)


@contextmanager
def _publishes_refused(*markers: Path, code: int = errno.EACCES, says: str = "Permission denied"):
    """Every publish of *markers* refused with *code* — the way a root without write access, a full
    disk or a read-only mount refuses one — while every other rename is made."""
    real = os.replace
    wanted = {os.fspath(m) for m in markers}

    def refusing(src, dst, *args, **kwargs):
        if os.fspath(dst) in wanted:
            raise OSError(code, says, os.fspath(dst))
        return real(src, dst, *args, **kwargs)

    with mock.patch("os.replace", refusing):
        yield


def _no_note_can_be_kept() -> None:
    """A file where the directory of failed-publish notes goes, so no note is ever kept."""
    config.STATE_DIR.mkdir(parents=True, exist_ok=True)
    (config.STATE_DIR / "unrecorded").write_text("not a directory\n")


class SweepOfRoundFiveGuardAsk(RoundFiveCheckout):
    """Survivors of the deletion sweep of 98a1999 in `commands._mirror_into_workspaces` (#942)."""

    def guard_ask(self) -> str:
        self.at_the_plane()
        _, said = self.invoke(commands.cmd_guard_ask, pattern="helm uninstall *", local=False)
        return said

    def test_no_errno_on_record_offers_no_remedy_and_costs_no_launch(self):
        """commands.py:1843, the empty branch; workspace.py:2306, the note's narrowed catch."""
        _no_note_can_be_kept()
        self.plane_moves_shared()
        with _publishes_refused(self.marker):
            workspace.ensure(self.ws)
            said = self.guard_ask()
        self.assertIn("could not publish its record there first", said)
        self.assertNotIn("until you", said)

    def test_two_unpublished_records_list_only_the_remedy_on_record(self):
        """commands.py:1807: `api`'s note cannot be kept and `web`'s can — one remedy, no blank."""
        web = self.checkout("web", real=True)
        workspace.wire_harnesses(self.ws)
        note = workspace._unrecorded_note(self.clone)
        note.mkdir(parents=True)
        self.plane_moves_shared()
        with _publishes_refused(self.marker, web / workspace.GENERATED_MARKER):
            said = self.guard_ask()
        self.assertIn("until you restore write access to that checkout (EACCES", said)


class SweepOfRoundFiveReinit(RoundFiveCheckout):
    """Survivors in `cmd_workspace_reinit` (#942): a workspace holding a state `reinit` cannot clear
    is never called up to date, and each state is worded as what it is."""

    def said(self) -> str:
        return " ".join(self.reinit_said())

    def assert_not_up_to_date(self, expected: str) -> None:
        said = self.said()
        self.assertIn(expected, said)
        self.assertNotIn("Up to date", said)

    def test_a_foreign_file_in_the_workspace_directory(self):
        self.generated().write_text("{}\n")
        self.assert_not_up_to_date("was not written by charter")

    def test_a_blocked_write(self):
        fresh = _repo(workspace.workspace_dir(self.ws) / "fresh")
        (fresh / ".claude").write_text("in the way\n")
        self.said()                                  # the first pass writes the block: a repair
        self.assert_not_up_to_date("could not be written")

    def test_an_unreadable_file(self):
        self.generated().unlink()
        self.generated().mkdir()
        self.assert_not_up_to_date("cannot be read")

    def test_an_unpublished_record(self):
        self.plane_moves_shared()
        with _publishes_refused(self.marker):
            self.said()                              # the workspace directory's refresh: a repair
            self.assert_not_up_to_date("could not publish its record there first")

    def test_a_checkout_file_somebody_rewrote_is_offered_the_commit(self):
        """commands_workspace.py:1512."""
        self.shared.write_text('{"env": {"THEIRS": "1"}}\n')
        said = self.said()
        self.assertIn("if this is your own file and you mean to commit it: "
                      "git add -f .claude/settings.json", said)
        self.assertNotIn("Remove", said)

    def test_no_errno_on_record_leaves_no_empty_remedy(self):
        """commands_workspace.py:1562."""
        _no_note_can_be_kept()
        self.plane_moves_shared()
        with _publishes_refused(self.marker):
            said = self.said()
        self.assertIn("kept every exclude line it had.", said)

    def test_all_counts_a_workspace_it_could_not_bring_up_to_date(self):
        """commands_workspace.py:1620."""
        workspace.ensure("south")
        self.generated().write_text("{}\n")
        said: list[str] = []
        with mock.patch.object(commands_workspace.util, "ok", side_effect=said.append), \
             mock.patch.object(commands_workspace.util, "warn", side_effect=said.append), \
             mock.patch.object(commands_workspace.util, "err", side_effect=said.append), \
             mock.patch.object(commands_workspace.util, "info", side_effect=said.append):
            commands_workspace.cmd_workspace_reinit(SimpleNamespace(name=None, all=True))
        self.assertIn("1 still hold what the rows above name", " ".join(said))


class SweepOfRoundFiveDoctor(RoundFiveCheckout):
    """Survivors in doctor's `workspace layer` hint (#942): it leads with what clears each state."""

    def hint(self) -> str:
        return doctor.check_workspace_harness().hint

    def test_a_missing_file_alone_is_led_by_reinit(self):
        self.generated().unlink()
        self.assertTrue(self.hint().startswith("charter workspace reinit --all"), self.hint())

    def test_an_unwanted_file_alone_is_led_by_reinit(self):
        (config.ROOT / LOCAL).unlink()
        self.assertTrue(self.hint().startswith("charter workspace reinit --all"), self.hint())

    def test_a_stale_file_beside_a_harness_kept_one_is_led_by_reinit(self):
        _as_the_harness_would(self.local, "Bash(npm test *)")
        self.plane_moves_local()
        self.plane_moves_shared()
        self.assertTrue(self.hint().startswith("charter workspace reinit --all"), self.hint())

    def test_a_stale_file_alone_leaves_no_rest_to_clear(self):
        self.plane_moves_shared()
        hint = self.hint()
        self.assertTrue(hint.startswith("charter workspace reinit --all"), hint)
        self.assertNotIn("clears the rest", hint)

    def test_a_harness_kept_file_alone_is_not_sent_to_reinit(self):
        _as_the_harness_would(self.local, "Bash(npm test *)")
        self.plane_moves_local()
        self.assertNotIn("charter workspace reinit", self.hint())

    def test_an_unreadable_file_leads_and_reinit_clears_the_rest(self):
        self.generated().unlink()
        self.generated().mkdir()
        self.plane_moves_shared()
        hint = self.hint()
        self.assertTrue(hint.startswith("An 'unreadable' path"), hint)
        self.assertIn("restoring read access to it clears this", hint)
        self.assertIn("charter workspace reinit --all clears the rest.", hint)

    def test_an_unrecorded_checkout_is_never_sent_to_reinit(self):
        self.plane_moves_shared()
        with _publishes_refused(self.marker):
            workspace.ensure(self.ws)
        hint = self.hint()
        self.assertTrue(hint.startswith("An 'unrecorded' marker"), hint)
        self.assertNotIn("charter workspace reinit", hint)

    def test_the_unrecorded_sentence_names_only_unrecorded_checkouts(self):
        web = self.checkout("web", real=True)
        workspace.wire_harnesses(self.ws)
        self.plane_moves_shared()
        with _publishes_refused(self.marker):
            workspace.ensure(self.ws)
        (web / SHARED).write_text("{}\n")
        hint = self.hint()
        self.assertIn(f"{self.ws}/api:", hint)
        self.assertNotIn(f"{self.ws}/web", hint)

    def test_only_a_foreign_file_is_offered_the_commit(self):
        self.shared.write_text('{"env": {"THEIRS": "1"}}\n')
        _as_the_harness_would(self.local, "Bash(npm test *)")
        self.plane_moves_local()
        hint = self.hint()
        self.assertIn("git add -f .claude/settings.json", hint)
        self.assertNotIn("git add -f .claude/settings.local.json", hint)

    def test_the_commit_offered_is_the_path_and_nothing_after_it(self):
        self.shared.write_text('{"env": {"THEIRS": "1"}}\n')
        self.assertNotIn("git add -f .claude/settings.json.", self.hint())

    def test_a_foreign_file_in_the_workspace_directory_ends_its_sentence(self):
        self.generated().write_text("{}\n")
        self.assertIn("stays hidden while it is there.", self.hint())


class SweepOfRoundFiveTheChatsLine(RoundFiveCheckout):
    """Survivors in `workspace.rules_not_in_force`, `hooks._rules_gap` and `rules_held` (#942)."""

    def setUp(self) -> None:
        super().setUp()
        _isolation.no_background_refresh(self)

    def test_text_that_is_not_json_holds_no_rules(self):
        self.assertEqual(claude_code.ClaudeCodeHarness().rules_held("{"), frozenset())

    def test_a_harness_that_sends_no_cwd_is_told_about_the_directory_it_runs_in(self):
        _as_the_harness_would(self.local, "Bash(npm test *)")
        self.plane_moves_local()
        workspace.ensure(self.ws)
        self.addCleanup(os.chdir, os.getcwd())
        os.chdir(self.clone)
        out = _isolation.run_hook(hooks.sessionstart, {"session_id": "r5"})
        told = ((out or {}).get("hookSpecificOutput") or {}).get("additionalContext", "")
        self.assertIn("Bash(git push --force *)", told)

    def test_a_check_that_raises_costs_its_line_and_not_the_briefing(self):
        with mock.patch.object(workspace, "rules_not_in_force", side_effect=RuntimeError("boom")):
            told = _briefing(self, self.clone)
        self.assertTrue(told.strip(), "the whole briefing was lost")

    def test_a_directory_outside_the_workspaces_is_answered_not_raised(self):
        self.assertEqual(workspace.rules_not_in_force(config.ROOT), "")

    def test_a_workspace_directory_holding_somebodys_copy_is_told(self):
        self.generated().write_text("{}\n")
        self.assertIn("in this workspace",
                      workspace.rules_not_in_force(workspace.workspace_dir(self.ws)))

    def test_a_directory_in_a_workspace_that_is_no_checkout_is_told_nothing(self):
        self.generated().write_text("{}\n")
        memory = workspace.memory_dir(self.ws)
        self.assertTrue(memory.is_dir(), "fixture: the workspace has no memory directory")
        self.assertEqual(workspace.rules_not_in_force(memory), "")

    def test_a_file_no_rule_rides_in_is_passed_over(self):
        agent = config.ROOT / ".claude" / "agents" / "steward.md"
        agent.parent.mkdir(parents=True, exist_ok=True)
        agent.write_text("# steward\n")
        workspace.wire_harnesses(self.ws)
        self.assertEqual(workspace.rules_not_in_force(self.clone), "")

    def test_each_file_gets_its_own_fix_beside_an_unpublished_record(self):
        _as_the_harness_would(self.local, "Bash(npm test *)")
        self.plane_moves_local()
        self.plane_moves_shared()
        with _publishes_refused(self.marker):
            workspace.ensure(self.ws)
        told = workspace.rules_not_in_force(self.clone)
        self.assertIn("restore write access to this checkout", told)
        self.assertIn(f"add them to {LOCAL} by hand", told)

    def test_a_missing_file_an_unpublished_record_kept_out_is_told_to_restore_the_record(self):
        self.local.unlink()
        with _publishes_refused(self.marker):
            workspace.ensure(self.ws)
        self.assertFalse(self.local.exists(), "fixture: the local file was written anyway")
        told = workspace.rules_not_in_force(self.clone)
        self.assertIn("restore write access to this checkout", told)
        self.assertNotIn("reinit", told)

    def test_a_stale_file_is_sent_to_reinit(self):
        self.plane_moves_shared()
        self.assertIn(f"`charter workspace reinit {self.ws}` writes them",
                      workspace.rules_not_in_force(self.clone))

    def test_a_missing_file_is_sent_to_reinit(self):
        self.local.unlink()
        self.assertIn(f"`charter workspace reinit {self.ws}` writes them",
                      workspace.rules_not_in_force(self.clone))

    def test_an_unreadable_file_is_told_to_restore_read_access(self):
        self.shared.unlink()
        self.shared.mkdir()
        self.assertIn(f"restore read access to {SHARED}", workspace.rules_not_in_force(self.clone))


class SweepOfRoundFiveTheRecord(RoundFiveCheckout):
    """Survivors in the marker, its note and the exclude block (#942)."""

    def test_a_hand_edited_pending_entry_holding_an_object_is_passed_over(self):
        """workspace.py:1906."""
        doc = json.loads(self.marker.read_text())
        doc[SHARED] = [{"hand": "edited"}, doc[SHARED]]
        self.marker.write_text(json.dumps(doc))
        new = self.plane_moves_shared()
        workspace.ensure(self.ws)
        self.assertEqual(self.shared.read_text(), new)

    def test_a_checkout_that_cannot_be_listed_keeps_the_temp_line_on_the_way_out(self):
        """workspace.py:1926 and :1928 — a refused `scandir`, which chmod reaches on every
        platform charter supports (macOS and Linux) and a mock reaches the same way on each."""
        with _refused(files=[self.clone], calls=("scandir",)):
            workspace.unwire_guest(self.clone)
        self.assertIn(".charter-generated.*.tmp", self.exclude.read_text().splitlines())

    def test_a_record_that_cannot_be_settled_is_reported(self):
        """workspace.py:2212."""
        self.plane_moves_shared()
        real = os.replace
        publishes: list[str] = []

        def settle_refused(src, dst, *args, **kwargs):
            if os.fspath(dst) == os.fspath(self.marker):
                publishes.append(os.fspath(dst))
                if len(publishes) == 2:
                    raise OSError(errno.ENOSPC, "No space left on device", os.fspath(dst))
            return real(src, dst, *args, **kwargs)

        with mock.patch("os.replace", settle_refused):
            rows = dict(workspace.wire_harnesses(self.ws))
        self.assertEqual(len(publishes), 2, "fixture: no settle was published")
        self.assertEqual(rows.get(f"api/{workspace.GENERATED_MARKER}"), "unrecorded")

    def test_a_write_that_fails_leaves_no_temp_file(self):
        """workspace.py:2253."""
        self.plane_moves_shared()
        with _publishes_refused(self.shared, code=errno.ENOSPC, says="No space left on device"):
            rows = dict(workspace.wire_harnesses(self.ws))
        self.assertEqual(rows[f"api/{SHARED}"], "blocked")
        self.assertEqual([p.name for p in (self.clone / ".claude").iterdir()
                          if fnmatch.fnmatchcase(p.name, _TEMP_GLOB)], [])

    def test_an_errno_python_has_no_name_for_is_kept_as_its_number(self):
        """workspace.py:2304."""
        self.plane_moves_shared()
        with _publishes_refused(self.marker, code=99999, says="strange"):
            workspace.ensure(self.ws)
        self.assertEqual(workspace.unrecorded_reason(self.clone), "99999: strange")

    def test_a_line_the_operator_removed_for_their_own_file_is_not_put_back(self):
        """workspace.py:2392."""
        self.shared.write_text('{"env": {"THEIRS": "1"}}\n')
        lines = self.exclude.read_text().splitlines(keepends=True)
        self.exclude.write_text("".join(ln for ln in lines if ln.strip() != f"/{SHARED}"))
        workspace.ensure(self.ws)
        self.assertNotIn(f"/{SHARED}", self.exclude.read_text().splitlines())
        self.assertIn(SHARED, _status(self.clone))

    def test_a_note_with_nothing_left_to_write_is_no_finding(self):
        """workspace.py:2781, the first conjunct."""
        plane = (config.ROOT / SHARED).read_text()
        self.plane_moves_shared()
        with _publishes_refused(self.marker):
            workspace.ensure(self.ws)
        self.assertTrue(workspace.unrecorded_reason(self.clone), "fixture: no note was kept")
        (config.ROOT / SHARED).write_text(plane)
        self.assertNotIn(workspace.GENERATED_MARKER, dict(workspace.guest_layer(self.clone)))

    def test_a_missing_file_behind_an_unpublished_record_is_reported_unrecorded(self):
        """workspace.py:2781, "missing"."""
        agent = config.ROOT / ".claude" / "agents" / "steward.md"
        agent.parent.mkdir(parents=True, exist_ok=True)
        agent.write_text("# steward\n")
        with _publishes_refused(self.marker):
            workspace.ensure(self.ws)
        self.assertFalse((self.clone / ".claude" / "agents" / "steward.md").exists(),
                         "fixture: the agent file was written anyway")
        self.assertEqual(dict(workspace.guest_layer(self.clone))[workspace.GENERATED_MARKER],
                         "unrecorded")


class SweepOfRoundFiveTheBlock(RoundFiveCheckout):
    """Mutations CI's sweep of 98a1999 ran out of time on, pinned ahead of the local sweep (#942)."""

    def test_an_unaccounted_block_beside_a_workspace_directory_finding_is_reported_not_raised(self):
        """workspace.py `_exclude_state`'s `if p is None`: doctor asks every finding's directory
        for its unaccounted reasons, and a workspace directory has no exclude file."""
        self.shared.write_text('{"env": {"THEIRS": "1"}}\n')
        self.generated().unlink()
        with _refused(files=[self.shared], calls=("lstat",)):
            r = doctor.check_workspace_harness()
        self.assertIn(f"{self.ws}/api/.git/info/exclude (unaccounted)", r.detail)
        self.assertIn("cannot be checked", r.hint)

    def test_a_lost_marker_under_a_listing_that_cannot_be_trusted_still_reports_the_block(self):
        """workspace.py `guest_layer`'s `status == "unaccounted"`: charter owns nothing there now,
        and the block it keeps is reported all the same."""
        wt = workspace.workspace_dir(self.ws) / "api-wt"
        _git(self.clone, "worktree", "add", "-q", "-b", "wt1", str(wt))
        workspace.wire_harnesses(self.ws)
        _git(self.clone, "worktree", "lock", str(wt))
        (wt / ".git").unlink()
        self.marker.unlink()
        self.assertEqual(dict(workspace.guest_layer(self.clone)).get(".git/info/exclude"),
                         "unaccounted")

    def test_the_workspaces_directory_itself_is_answered_not_raised(self):
        """workspace.py `rules_not_in_force`'s `if not parts`."""
        _isolation.no_background_refresh(self)
        self.assertEqual(workspace.rules_not_in_force(config.WORKSPACES_DIR), "")

    def test_a_worktrees_directory_that_cannot_be_checked_distrusts_what_git_lists_anyway(self):
        """workspace.py `_live_trees`' `if admin is None`. Measured on git 2.50.1: with a
        repository's `worktrees/` unreadable, `git worktree list --porcelain` exits 0 and lists the
        main worktree alone. So what git answers there cannot be backed up, and is not trusted."""
        workspace.ensure("other")
        other = workspace.workspace_dir("other") / "api"
        _git(self.clone, "worktree", "add", "-q", "-b", "wt-other", str(other))
        workspace.wire_harnesses(self.ws)
        workspace.wire_harnesses("other")
        _as_the_harness_would(other / LOCAL, "Bash(npm test *)")
        (config.ROOT / LOCAL).unlink()
        admin = workspace.git_exclude_file(self.clone).parent.parent / "worktrees"
        main_alone = (f"worktree {os.path.realpath(self.clone)}\nHEAD {'0' * 40}\n"
                      f"branch refs/heads/main\n\n")
        real_run = util.run

        def git_as_it_answers_there(cmd, *args, **kwargs):
            if "worktree" in cmd and "list" in cmd:
                return SimpleNamespace(returncode=0, stdout=main_alone, stderr="")
            return real_run(cmd, *args, **kwargs)

        with _refused(files=[admin], calls=("lstat", "scandir")), \
             mock.patch.object(workspace.util, "run", side_effect=git_as_it_answers_there):
            workspace.wire_harnesses(self.ws)
        self.assertFalse(self.local.exists(), "fixture: the clone's own copy was not withdrawn")
        self.assertEqual(_status(other), "")

    def test_a_temp_file_at_the_root_keeps_the_temp_line_when_that_line_is_all_that_is_left(self):
        """workspace.py `_shared_rels`: a temp file's pattern names no directory of its own —
        `Path(".charter-generated.*.tmp").parent` is the checkout root — so leaving that line out
        of the directories to scan left the root unscanned whenever it was the block's last line,
        and a temp file there lost its line."""
        (self.clone / ".charter-generated.4242.0123456789ab.tmp").write_text("{")
        with _refused(files=[self.clone], calls=("scandir",)):
            workspace.unwire_guest(self.clone)
        left = [ln for ln in self.exclude.read_text().splitlines() if ln and not ln.startswith("#")]
        self.assertEqual(left, [".charter-generated.*.tmp"], "fixture: more than the temp line is left")
        workspace.unwire_guest(self.clone)
        self.assertIn(".charter-generated.*.tmp", self.exclude.read_text().splitlines())
        self.assertEqual(_status(self.clone), "")

    def test_a_worktree_in_another_workspace_that_lost_its_marker_still_keeps_its_line(self):
        """workspace.py `_wired`'s location half: a checkout inside this plane's workspaces is one
        charter wires, marker or none, so the file there keeps its line."""
        workspace.ensure("other")
        other = workspace.workspace_dir("other") / "api"
        _git(self.clone, "worktree", "add", "-q", "-b", "wt-other", str(other))
        workspace.wire_harnesses("other")
        _as_the_harness_would(other / LOCAL, "Bash(npm test *)")
        (other / workspace.GENERATED_MARKER).unlink()
        (config.ROOT / LOCAL).unlink()
        workspace.wire_harnesses(self.ws)
        self.assertFalse(self.local.exists(), "fixture: the clone's own copy was not withdrawn")
        self.assertEqual(_status(other), "")


class UtilRunCanWithholdAVariable(unittest.TestCase):
    """`util.run(..., unset=...)` (#942 review round 3). `env` is an overlay and can only ADD to
    what a child inherits; `GIT_DIR` exported by a git hook has to be taken away, or every git
    the child runs answers about a different repository."""

    def test_a_named_variable_does_not_reach_the_child(self):
        code = "import os; print(os.environ.get('PROBE_942_WITHHELD', 'absent'))"
        with mock.patch.dict(os.environ, {"PROBE_942_WITHHELD": "inherited"}):
            kept = util.run([sys.executable, "-c", code]).stdout.strip()
            gone = util.run([sys.executable, "-c", code],
                            unset=["PROBE_942_WITHHELD"]).stdout.strip()
        self.assertEqual((kept, gone), ("inherited", "absent"))


class HarnessBehindIsTrueWhenCharterLostItsRecord(PlaneWithRestrictions):
    """Review round 3, N4: a local file charter DID write, byte for byte, whose marker entry is
    gone, read as holding "settings charter did not put there". The sentence has to be true
    whether or not charter's record survives."""

    def setUp(self) -> None:
        super().setUp()
        _plane_local(config.ROOT, ask=["Bash(charter change land *)"])
        self.clone = self.checkout("api", real=True)
        workspace.wire_harnesses(self.ws)
        marker = self.clone / workspace.GENERATED_MARKER
        doc = json.loads(marker.read_text())
        del doc[LOCAL]
        marker.write_text(json.dumps(doc, indent=2) + "\n")
        _plane_local(config.ROOT, ask=["Bash(charter change land *)", "Bash(kubectl *)"])

    def test_doctor(self):
        r = doctor.check_workspace_harness()
        self.assertIn(f"{self.ws}/api/{LOCAL} (harness-behind)", r.detail)
        self.assertIn("charter cannot vouch for as its own write", r.hint)
        self.assertNotIn("did not put there", r.hint)

    def test_guard_ask(self):
        self.at_the_plane()
        _, said = self.invoke(commands.cmd_guard_ask, pattern="helm uninstall *", local=True)
        self.assertIn(f"{self.ws}/api/{LOCAL}", said)
        self.assertIn("charter cannot vouch for as its own write", said)
        self.assertNotIn("did not put there", said)

    def test_reinit(self):
        said = " ".join(self.reinit_said())
        self.assertIn(f"api/{LOCAL}", said)
        self.assertIn("charter cannot vouch for as its own write", said)
        self.assertNotIn("did not put there", said)


class AFailedReadNeverUnhidesALine(PlaneWithRestrictions):
    """Review round 3, N1 — a regression round 2 introduced. "Gone" was `os.path.lexists`,
    which answers False for ANY lstat error: one refused `lstat` on `settings.json` during a
    launch forgot its marker entry for good, the file showed in `git status`, and doctor said
    "all current". An unreadable `.claude/` for one reinit forgot every entry and took the
    whole block (and on 3.13 the same reinit crashed instead). Ruling G(e): only
    `FileNotFoundError` or `NotADirectoryError` proves a path is gone."""

    def setUp(self) -> None:
        super().setUp()
        _plane_local(config.ROOT, ask=["Bash(charter change land *)"])
        self.clone = self.checkout("api", real=True)
        workspace.wire_harnesses(self.ws)
        self.local = self.clone / LOCAL
        self.approved = _as_the_harness_would(self.local, "Bash(npm test *)")
        self.marker = self.clone / workspace.GENERATED_MARKER
        self.recorded = json.loads(self.marker.read_text())

    def test_one_refused_lstat_during_a_launch_forgets_nothing(self):
        with _refused(files=[self.clone / SHARED]):
            workspace.ensure(self.ws)
        self.assertEqual(json.loads(self.marker.read_text()), self.recorded)
        self.assertEqual(_status(self.clone), "")

    def test_a_claude_directory_refused_for_one_reinit_costs_nothing(self):
        with _refused(inside=[self.clone / ".claude"], calls=("lstat", "stat", "open")):
            said = " ".join(self.reinit_said())
        self.assertEqual(json.loads(self.marker.read_text()), self.recorded)
        self.assertEqual(self.local.read_text(), self.approved)
        self.assertEqual(_status(self.clone), "")
        self.assertIn(f"api/{SHARED} cannot be read", said)


class AnUnreadableSharedFileIsCalledUnreadableEverywhere(PlaneWithRestrictions):
    """Review round 3, ruling D extended: an unreadable generated `settings.json` was `foreign`
    to `guard ask` and `reinit` — "not written by charter … Remove it" — while doctor called
    it `unreadable`. One state, one name, and no removal advice."""

    def setUp(self) -> None:
        super().setUp()
        self.clone = self.checkout("api", real=True)
        workspace.wire_harnesses(self.ws)
        self.shared = self.clone / SHARED
        self.theirs = self.shared.read_text()
        self.shared.chmod(0o000)
        self.addCleanup(self.shared.chmod, 0o644)
        with self.assertRaises(PermissionError, msg="fixture: this user can read a 000 file"):
            self.shared.read_text()

    def plane_moves(self) -> None:
        _plane_settings(config.ROOT, permissions={
            "ask": ["Bash(terraform apply *)", "Bash(kubectl *)"], "deny": ["Bash(rm -rf /)"]})

    def test_the_row_says_it_cannot_be_read(self):
        self.plane_moves()
        self.assertEqual(dict(workspace.wire_guest(self.clone))[SHARED], "unreadable")

    def test_guard_ask_never_calls_it_foreign(self):
        self.at_the_plane()
        _, said = self.invoke(commands.cmd_guard_ask, pattern="kubectl delete *", local=False)
        self.assertIn(f"{self.ws}/api/{SHARED} cannot be read", said)
        self.assertNotIn("Remove", said)
        self.assertNotIn("charter did not write", said)

    def test_reinit_never_calls_it_foreign_and_it_stays_hidden(self):
        self.plane_moves()
        said = " ".join(self.reinit_said())
        self.assertIn(f"api/{SHARED} cannot be read", said)
        self.assertNotIn("not written by charter", said)
        self.assertNotIn("Remove", said)
        self.shared.chmod(0o644)
        self.assertEqual(self.shared.read_text(), self.theirs)
        self.assertEqual(_status(self.clone), "")


class WhatGitListsDecidesNothingItCannotBackUp(PlaneWithRestrictions):
    """Review round 3, ruling G (b) and (c). A line leaves only when every tree git lists is a
    checkout charter can look into and none is marked prunable. Git 2.50.1 lists a
    `--separate-git-dir` clone's git directory as its main worktree, so the clone never
    entered the union; and `charter workspace rename` leaves git listing the old path as
    prunable, so every launch of the other workspace unhid the moved worktree's local file."""

    def setUp(self) -> None:
        super().setUp()
        _plane_local(config.ROOT, ask=["Bash(charter change land *)"])

    def test_a_clone_with_a_separate_git_dir_keeps_its_local_line(self):
        clone = workspace.workspace_dir(self.ws) / "api"
        separate = self.tmp / "separate" / "api.git"
        separate.parent.mkdir(parents=True)
        _git(clone.parent, "init", "-q", f"--separate-git-dir={separate}", clone.name)
        (clone / "README.md").write_text("theirs\n")
        _git(clone, "add", "-A")
        _git(clone, "commit", "-qm", "init")
        wt = workspace.workspace_dir(self.ws) / "api-wt"
        _git(clone, "worktree", "add", "-q", "-b", "wt1", str(wt))
        self.assertIn(f"worktree {separate.resolve()}\n",
                      _git(clone, "worktree", "list", "--porcelain").stdout,
                      "fixture: git no longer lists the git directory as the main worktree")
        workspace.wire_harnesses(self.ws)
        _as_the_harness_would(clone / LOCAL, "Bash(npm test *)")
        (config.ROOT / LOCAL).unlink()
        workspace.wire_harnesses(self.ws)
        self.assertFalse((wt / LOCAL).exists(), "fixture: the worktree's copy was not withdrawn")
        self.assertEqual(_status(clone), "")
        self.assertIn("not a checkout", " ".join(workspace.unaccounted(wt)))
        # The clone's own record names every line in the block, so git's doubt keeps nothing
        # there and doctor has nothing to say about it — a reason printed over a block that is
        # fully accounted for is the cry-wolf the round-3 sweep found in `current - need`.
        self.assertEqual(workspace.unaccounted(clone), [])
        # Review round 4: `reinit` cannot clear this, and neither can anything else — say so.
        r = doctor.check_workspace_harness()
        self.assertIn("a separate git dir keeps this line by design; nothing to do", r.hint)
        self.assertFalse(r.hint.startswith("charter workspace reinit"), r.hint)

    def test_a_worktree_git_lists_as_prunable_takes_no_line_away(self):
        clone = self.checkout("api", real=True)
        workspace.ensure("other")
        _git(clone, "worktree", "add", "-q", "-b", "wt-other",
             str(workspace.workspace_dir("other") / "api"))
        workspace.wire_harnesses(self.ws)
        workspace.wire_harnesses("other")
        _as_the_harness_would(workspace.workspace_dir("other") / "api" / LOCAL,
                              "Bash(npm test *)")
        (config.ROOT / LOCAL).unlink()
        rc, said = self.invoke(commands_workspace.cmd_workspace_rename, old="other", new="other2")
        self.assertEqual(rc, 0, said)
        moved = workspace.workspace_dir("other2") / "api"
        self.assertIn("prunable", _git(clone, "worktree", "list", "--porcelain").stdout,
                      "fixture: git no longer lists the old path as prunable")
        workspace.wire_harnesses(self.ws)
        self.assertNotIn(LOCAL, _status(moved))
        workspace.wire_harnesses("other2")
        workspace.wire_harnesses(self.ws)
        self.assertNotIn(LOCAL, _status(moved))
        said = " ".join(workspace.unaccounted(clone))
        self.assertIn("prunable", said)
        # Git's own reason, not charter's guess (review round 4): "moved or deleted" was said of
        # a worktree that was only unreadable.
        self.assertIn("gitdir file points to non-existent location", said)
        self.assertNotIn("moved or deleted", said)
        self.assertIn("git worktree repair", said)

    def test_a_listed_checkout_that_is_not_there_says_what_clears_it(self):
        """A LOCKED worktree whose `.git` is gone: git lists it, not as prunable, and it is no
        separate git dir — so "by design" would be false of it."""
        clone = self.checkout("api", real=True)
        wt = workspace.workspace_dir(self.ws) / "api-wt"
        _git(clone, "worktree", "add", "-q", "-b", "wt1", str(wt))
        workspace.wire_harnesses(self.ws)
        _as_the_harness_would(wt / LOCAL, "Bash(npm test *)")
        (config.ROOT / LOCAL).unlink()
        _git(clone, "worktree", "lock", str(wt))
        (wt / ".git").unlink()
        workspace.wire_guest(clone)
        said = " ".join(workspace.unaccounted(clone))
        self.assertIn("not a checkout", said)
        self.assertIn("restoring that checkout", said)
        self.assertNotIn("by design", said)

    def test_a_reason_two_checkouts_share_is_printed_once(self):
        """Review round 4: two checkouts reading one exclude reported git's one doubt twice."""
        clone = self.checkout("api", real=True)
        wt = workspace.workspace_dir(self.ws) / "api-wt"
        _git(clone, "worktree", "add", "-q", "-b", "wt1", str(wt))
        workspace.ensure("other")
        gone = workspace.workspace_dir("other") / "api"
        _git(clone, "worktree", "add", "-q", "-b", "wt-other", str(gone))
        workspace.wire_harnesses(self.ws)
        workspace.wire_harnesses("other")
        _as_the_harness_would(gone / LOCAL, "Bash(npm test *)")
        (config.ROOT / LOCAL).unlink()
        workspace.wire_harnesses(self.ws)
        self.assertIn(f"/{LOCAL}\n", self.excludes(clone),
                      "fixture: the approving worktree's line did not stay")
        shutil.rmtree(gone)
        r = doctor.check_workspace_harness()
        self.assertIn(f"{self.ws}/api/.git/info/exclude (unaccounted)", r.detail)
        self.assertIn(f"{self.ws}/api-wt/.git/info/exclude (unaccounted)", r.detail)
        self.assertEqual(r.hint.count("as prunable"), 1, r.hint)

    def test_a_bare_repositorys_worktrees_still_let_an_unneeded_line_go(self):
        """The `bare` entry names a git directory, not a checkout, and says so — so it is
        passed over rather than read as a tree charter cannot look into, which would keep
        every line of every bare repository's worktrees for ever."""
        seed = _repo(self.tmp / "seed")
        bare = self.tmp / "bare" / "api.git"
        bare.parent.mkdir(parents=True)
        _git(self.tmp, "clone", "-q", "--bare", str(seed), str(bare))
        api = workspace.workspace_dir(self.ws) / "api"
        wt = workspace.workspace_dir(self.ws) / "api-wt"
        _git(bare, "worktree", "add", "-q", "-b", "wmain", str(api))
        _git(bare, "worktree", "add", "-q", "-b", "wt1", str(wt))
        workspace.wire_harnesses(self.ws)
        self.assertIn(f"/{LOCAL}\n", self.excludes(api), "fixture: the local line never arrived")
        (config.ROOT / LOCAL).unlink()
        workspace.wire_harnesses(self.ws)
        self.assertNotIn(f"/{LOCAL}\n", self.excludes(api))
        self.assertIn(f"/{SHARED}\n", self.excludes(api))


if __name__ == "__main__":  # pragma: no cover
    unittest.main()
