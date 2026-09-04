"""#886 — a `charter` command that wrote plane state reaches every frame that draws it.

Two defects, one subject. `commands.cmd_clone` wrote clones to disk and called neither
`gather.refresh` nor `state.bump`, so the `repos` component stayed empty until charter was
restarted. Behind it sat the wider one: `notify.plane_changed` bumps exactly ONE frame, the
id in `$CHARTER_SESSION_ID`, so an agent cloning inside chat `alpha.1` refreshed `alpha.1`
and left sibling `alpha.2` stale — same workspace, same repo list, nothing coming to
correct it — and a command typed in a plain terminal, which fires no hook at all, reached
neither.

`notify.plane_changed_everywhere` is the answer, and the precedent it generalises is
`state.rename_workspace`: an out-of-frame CLI command that already scans the frame root and
bumps every frame it touched, *"because a panel repaints on a version bump and on nothing
else"*. That one is bump-only; this adds the refresh half, because #512 forbids the
alternative outright — *"a panel does not sweep."*

The properties pinned here are the ones that were settled rather than assumed:

* **one scan per distinct WORKSPACE**, not one per frame — a cold `gather.scan()` is ~35ms
  and three git invocations, and every frame in a workspace gets the same repo list out of
  it (`scan` derives it from `workspace.clones(ws)`);
* **the frame's own workspace decides**, read through `state.own_workspace` and never
  through `state.workspace_for`, whose top rung is `$CHARTER_WORKSPACE` out of *this*
  process's environment — a stranger's, when the command was typed in a terminal;
* **refresh before bump**, `notify.py:20-24`'s order, because a panel's poll reads the
  version first and the cache second;
* **no liveness check**, `rename_workspace`'s deliberate choice: a bump into a dead frame
  is a few bytes `reap` removes, and `state.is_live` costs a tmux subprocess it cannot even
  answer with for a chat;
* **the frame's own `current_repo` survives**, which is the one field of a scan that is a
  fact about a reader rather than about a workspace.

`os.environ` is cleared by `PersonaIso` and re-stated per case where a case is about it:
`state.workspace_for` and `notify.plane_changed` both read `$CHARTER_WORKSPACE` and
`$CHARTER_SESSION_ID`, so a developer running the suite inside a live frame would otherwise
be supplying half of every fixture (#519/#521/#528).
"""

from __future__ import annotations

import json
import os
import subprocess
import unittest
from pathlib import Path
from types import SimpleNamespace
from unittest import mock

from charter import commands, config, inventory, workspace
from charter.frame import gather, notify, state

from tests._isolation import PersonaIso, no_background_refresh


def _plant(fid: str, *, ws: str, pin: str = "") -> None:
    """Make *fid* look like a chat charter launched into *ws*.

    The production writers and never a hand-written file — `record_workspace` is what
    `frame_workspace` reads back — which is the rule
    `test_a_chat_belongs_to_its_workspace_for_life` keeps and the reason a fixture that
    stopped agreeing with the launcher fails here rather than passing against itself.
    *pin* is what `_frame_identity_env` puts on the pane for a launch that really was
    pinned; ``""`` is what it emits for one that was not, which is the ordinary case.
    """
    state.frame_dir(fid, create=True)
    state.record_workspace(fid, ws)
    state.record_harness_pane(fid, "%1")
    state.record_identity(fid, {"CHARTER_HARNESS": "Claude Code",
                                "CHARTER_WORKSPACE": pin, "CHARTER_PERSONA": ""})


def _scan_of(ws: str, *names: str) -> dict:
    """A scan payload for *ws* listing *names* — the shape `gather.scan` returns.

    Spelled out rather than gathered, because every case in the first three classes is
    about WHICH frame gets WHICH payload and not about what a real git sweep says; the
    fields `_shaped_like_a_scan` insists on are present so `gather.cached` reads it back.
    """
    return {"gathered_at": 0.0, "workspace": ws, "current_repo": f"{ws}-cwd",
            "repos": [{"name": n} for n in names], "worktrees": [], "todos": [],
            "todo_count": 0, "changes": []}


class APlaneWithThreeFrames(PersonaIso):
    """Two chats in `alpha`, one in `beta`, and no tmux anywhere.

    The strand is a state defect and reproduces on directories: what a fan-out has to get
    right is which cache it writes and which version it moves, and neither question needs a
    live pane to ask.
    """

    def setUp(self) -> None:
        super().setUp()
        # The tripwire every frame-state test is written under: if `PersonaIso` ever stops
        # repointing derived paths at a throwaway tree, these writes land in a real plane's
        # `.charter/frame/`.
        self.assertIn("edm-test-", str(config.STATE_DIR))
        self.enterContext(mock.patch.dict(os.environ, {}, clear=True))
        for n in ("alpha", "beta"):
            (config.WORKSPACES_DIR / n).mkdir(parents=True, exist_ok=True)
        _plant("alpha.1", ws="alpha")
        _plant("alpha.2", ws="alpha")
        _plant("beta.1", ws="beta")

    def versions(self) -> dict:
        return {f: state.version(f) for f in ("alpha.1", "alpha.2", "beta.1")}

    def fake_scan(self, *, fails: str | None = None):
        """`gather.scan` replaced by one that answers per workspace and never shells out.

        *fails* names a workspace whose scan raises, which is the only way to reach the
        one branch in the fan-out that decides what a failed gather costs.
        """
        def scan(*, workspace: str | None = None, cwd: str | None = None) -> dict:
            if workspace == fails:
                raise RuntimeError("this workspace could not be gathered")
            return _scan_of(workspace or "", f"{workspace}-repo")
        return self.enterContext(mock.patch.object(gather, "scan", side_effect=scan))


class EveryFrameHearsAboutIt(APlaneWithThreeFrames, unittest.TestCase):
    """The defect itself, at both of its widths."""

    def test_a_sibling_chat_is_refreshed_too(self):
        """The second gap in #886, stated as the operator sees it: an agent cloning inside
        `alpha.1` leaves `alpha.2` drawing the same workspace and the same repo list. The
        session id is exported here BECAUSE the fan-out must not consult it — a fix that
        still keyed on `$CHARTER_SESSION_ID` would pass every other case in this file."""
        self.fake_scan()
        before = self.versions()
        with mock.patch.dict(os.environ, {"CHARTER_SESSION_ID": "alpha.1"}):
            notify.plane_changed_everywhere()
        self.assertEqual([r["name"] for r in gather.cached("alpha.2")["repos"]],
                         ["alpha-repo"])
        self.assertNotEqual(state.version("alpha.2"), before["alpha.2"])

    def test_every_frame_on_the_plane_is_refreshed_and_bumped(self):
        """Including the one in the other workspace: `charter clone` can be told which
        workspace to write into, and `workspace rename` touches two at once, so "every
        frame that would draw it differently" is the whole frame root."""
        self.fake_scan()
        before = self.versions()
        notify.plane_changed_everywhere()
        for fid in ("alpha.1", "alpha.2", "beta.1"):
            self.assertIsNotNone(gather.cached(fid), fid)
            self.assertNotEqual(state.version(fid), before[fid], fid)

    def test_each_frame_gets_its_own_workspaces_repos(self):
        """A fan-out that wrote one gather into every frame would be #512's defect
        wearing this fix: *"a refresh keyed to a different workspace than the frame it is
        refreshing is the defect, not a stale value"*."""
        self.fake_scan()
        notify.plane_changed_everywhere()
        self.assertEqual(gather.cached("alpha.1")["workspace"], "alpha")
        self.assertEqual(gather.cached("alpha.2")["workspace"], "alpha")
        self.assertEqual(gather.cached("beta.1")["workspace"], "beta")

    def test_the_bump_is_a_real_version_a_panel_can_poll(self):
        """`state.version` answers the sentinel ``"0"`` for a frame that was never bumped,
        and a panel repaints on a version bump and on nothing else — so "the cache was
        written" is only half of the claim this function makes."""
        self.fake_scan()
        self.assertEqual(self.versions(), {"alpha.1": "0", "alpha.2": "0", "beta.1": "0"})
        notify.plane_changed_everywhere()
        for fid, v in self.versions().items():
            self.assertNotEqual(v, "0", fid)


class OneScanPerWorkspace(APlaneWithThreeFrames, unittest.TestCase):
    """The cost decision, measured rather than asserted in a comment."""

    def test_two_workspaces_and_three_frames_cost_two_scans(self):
        """A cold `scan()` is ~35ms and three git invocations. Per-frame would be three
        scans here and N on a plane with N chats open in one workspace, for an answer that
        is identical every time — `scan` builds `repos` from `workspace.clones(ws)`."""
        scanned = self.fake_scan()
        notify.plane_changed_everywhere()
        self.assertEqual(scanned.call_count, 2)
        self.assertEqual(sorted(c.kwargs["workspace"] for c in scanned.call_args_list),
                         ["alpha", "beta"])

    def test_the_workspace_is_always_passed_explicitly(self):
        """#512 again, from the caller's side: `scan` with no workspace falls through to
        `workspace.resolve()`, which answers for whoever typed the command."""
        scanned = self.fake_scan()
        notify.plane_changed_everywhere()
        for call in scanned.call_args_list:
            self.assertTrue(call.kwargs.get("workspace"))


class TheFramesOwnAnswerDecides(APlaneWithThreeFrames, unittest.TestCase):
    """`state.own_workspace`, and the two things that follow from choosing it."""

    def test_the_callers_exported_workspace_does_not_decide(self):
        """`state.workspace_for`'s top rung is `$CHARTER_WORKSPACE` **out of this
        process's environment**. That is the frame's own environment when a hook asks, and
        a stranger's when a terminal does — so a fan-out asking it would file every chat on
        the plane under whatever the operator happened to have exported and then overwrite
        each of their caches with that one workspace's repos."""
        scanned = self.fake_scan()
        with mock.patch.dict(os.environ, {"CHARTER_WORKSPACE": "beta"}):
            notify.plane_changed_everywhere()
        self.assertEqual(sorted(c.kwargs["workspace"] for c in scanned.call_args_list),
                         ["alpha", "beta"])
        self.assertEqual(gather.cached("alpha.1")["workspace"], "alpha")

    def test_a_pinned_chat_is_gathered_for_its_pin(self):
        """The first rung of `own_workspace` is the pin the LAUNCH recorded, which
        outranks the launch's resolved answer — the same order `chats.of_workspace` walks
        for membership, so the roster and the gather cannot disagree."""
        _plant("alpha.3", ws="alpha", pin="beta")
        scanned = self.fake_scan()
        notify.plane_changed_everywhere()
        self.assertEqual(gather.cached("alpha.3")["workspace"], "beta")
        self.assertEqual(scanned.call_count, 2)

    def test_a_frame_that_says_nothing_is_bumped_and_not_refreshed(self):
        """A frame launched by a charter that predates the workspace record. There is no
        rung under `own_workspace` a terminal could honestly stand on — the ones that
        remain answer for whoever typed last — so it gets `rename_workspace`'s bump-only
        outcome, which is no worse than today and is not a cache full of somebody else's
        repos."""
        state.frame_dir("orphan.1", create=True)
        scanned = self.fake_scan()
        notify.plane_changed_everywhere()
        self.assertIsNone(gather.cached("orphan.1"))
        self.assertNotEqual(state.version("orphan.1"), "0")
        self.assertEqual(sorted(c.kwargs["workspace"] for c in scanned.call_args_list),
                         ["alpha", "beta"])

    def test_nothing_asks_whether_a_frame_is_alive(self):
        """Settled, not incidental. `state.is_live` costs a tmux subprocess per frame and
        cannot answer for a chat at all without a pane a terminal command does not have,
        and what a bump into a dead frame costs is a few bytes `state.reap` deletes."""
        self.fake_scan()
        with mock.patch.object(state, "is_live", side_effect=AssertionError) as live:
            notify.plane_changed_everywhere()
        live.assert_not_called()


class RefreshThenBump(APlaneWithThreeFrames, unittest.TestCase):
    """`notify.py:20-24`'s order, pinned as an order and not as an outcome."""

    def test_no_version_moves_before_every_cache_is_written(self):
        """*"A panel's poll loop reads `state.version` first and the cache second, so if
        the version were bumped first there would be a window — however small — where a
        poller sees the new version and still reads the stale cache."* Asserted across the
        WHOLE fan-out rather than per frame: with two workspaces on the plane, bumping the
        first group before scanning the second is a ~35ms window, not a theoretical one."""
        self.fake_scan()
        order: list[str] = []
        real_save, real_bump = gather.save, state.bump

        def save(fid, data):
            order.append("save")
            real_save(fid, data)

        def bump(fid):
            order.append("bump")
            real_bump(fid)

        with mock.patch.object(gather, "save", save), \
                mock.patch.object(state, "bump", bump):
            notify.plane_changed_everywhere()
        self.assertEqual(order, ["save"] * 3 + ["bump"] * 3)


class WhatBelongsToTheFrameStays(APlaneWithThreeFrames, unittest.TestCase):
    """`current_repo` — the one field of a scan that answers for a reader.

    `scan` derives `repos` from `workspace.clones(ws)`, which is why one scan can be
    written into every frame of a workspace. It derives `current_repo` from a **cwd**, and
    the cwd a fan-out has is whichever directory the operator typed the command in.
    """

    def test_the_frame_keeps_the_repo_it_was_standing_in(self):
        """Otherwise the fix introduces its own staleness: a chat whose harness sits in
        `api` loses the marked row on its repo table because somebody ran `charter clone`
        from their home directory, and gets it back only on that chat's next tool call."""
        gather.save("alpha.1", _scan_of("alpha", "api") | {"current_repo": "api"})
        self.fake_scan()
        notify.plane_changed_everywhere()
        self.assertEqual(gather.cached("alpha.1")["current_repo"], "api")
        self.assertEqual([r["name"] for r in gather.cached("alpha.1")["repos"]],
                         ["alpha-repo"])

    def test_a_frame_with_no_cache_is_not_given_a_location(self):
        """The other half of the same rule. The fan-out never DECIDES which repo a frame
        is in, so a frame that has recorded nothing gets ``None`` — what it already draws —
        rather than the caller's own cwd."""
        self.fake_scan()
        notify.plane_changed_everywhere()
        self.assertIsNone(gather.cached("beta.1")["current_repo"])


class NothingHereRaises(APlaneWithThreeFrames, unittest.TestCase):
    """The module's one absolute rule, extended to a caller that is a command rather than
    a hook: a `charter clone` that cloned every repo must not exit non-zero because a frame
    directory could not be written."""

    def test_a_frame_root_that_cannot_be_listed_is_not_an_exception(self):
        with mock.patch.object(state, "_root", return_value=self.tmp / "not-a-frame-root"):
            notify.plane_changed_everywhere()

    def test_a_writer_that_throws_is_swallowed(self):
        """`state.bump` already swallows its own `OSError`s; this covers the step after
        any future change to either writer, which is what the outer `try` is for."""
        self.fake_scan()
        with mock.patch.object(state, "bump", side_effect=RuntimeError("disk")):
            notify.plane_changed_everywhere()

    def test_one_workspace_that_cannot_be_gathered_costs_only_its_own_facts(self):
        """Two properties in one, and both were choices. The other workspace is still
        scanned and written — a failure inside one group must not end the fan-out — and the
        failed group keeps the facts it had, where `gather.refresh`'s own `_empty` fallback
        would blank a whole workspace's tables at once. Every frame is still bumped: the
        plane really did change, whatever this process managed to gather about it."""
        gather.save("alpha.1", _scan_of("alpha", "api"))
        self.fake_scan(fails="alpha")
        before = self.versions()
        notify.plane_changed_everywhere()
        self.assertEqual([r["name"] for r in gather.cached("alpha.1")["repos"]], ["api"])
        self.assertEqual([r["name"] for r in gather.cached("beta.1")["repos"]],
                         ["beta-repo"])
        for fid in ("alpha.1", "alpha.2", "beta.1"):
            self.assertNotEqual(state.version(fid), before[fid], fid)


def _git(where: Path, *args: str) -> None:
    """`git` in *where*, inheriting this process's environment ON PURPOSE.

    `tests/_gitguard` has already pointed `$GIT_CONFIG_GLOBAL` at a file this package
    writes, so identity and `commit.gpgsign=false` are answered for every child. Building a
    private env here would drop that redirect, which is the shape that hangs a suite on
    somebody's fingerprint reader (#641).
    """
    subprocess.run(["git", "-C", str(where), *args], check=True, capture_output=True)


class ACloneTellsEveryFrame(PersonaIso, unittest.TestCase):
    """The reported symptom, end to end through the real `commands.cmd_clone`.

    Only the network step is replaced — `_clone_one` makes a real git repo where `git
    clone` would have left one — so everything after it is production code: the result
    rendering, `_wire_clones`, and the fan-out that was missing. `gather.scan` runs for
    real here, which is what makes this a test of the reported defect (*"the `repos`
    component stays empty until charter restarts"*) rather than of a call being made.
    """

    def setUp(self) -> None:
        super().setUp()
        self.assertIn("edm-test-", str(config.STATE_DIR))
        # `PersonaIso`'s `_envguard.unset_all()` and NOT a `clear=True` patch of
        # `os.environ`, unlike the classes above: this one spawns `git`, and clearing the
        # environment would drop `tests/_gitguard`'s `$GIT_CONFIG_GLOBAL` redirect —
        # `_planeguard` refuses the `Popen` for it, and the reason it refuses is #641's
        # hang on somebody's fingerprint reader.
        (self.tmp / "charter.toml").write_text("schema = 1\n")
        config.HAS_CONTROL_PLANE = True
        # A plane made a moment ago has no cache to be fresh and no cooldown lock to hold,
        # so `gather.scan`'s `glstate.maybe_spawn` would fork a real detached
        # `charter gl-refresh` that nothing here waits for (#542).
        no_background_refresh(self)
        self.ws = "alpha"
        workspace.ensure(self.ws)
        (config.WORKSPACES_DIR / "beta").mkdir(parents=True, exist_ok=True)
        _plant("alpha.1", ws="alpha")
        _plant("alpha.2", ws="alpha")
        _plant("beta.1", ws="beta")
        config.INVENTORY.parent.mkdir(parents=True, exist_ok=True)
        config.INVENTORY.write_text(json.dumps({
            "group": "g", "count": 2,
            "repos": [{"name": n, "path_with_namespace": f"g/{n}",
                       "default_branch": "main",
                       "ssh_url_to_repo": f"git@example.com:g/{n}.git",
                       "http_url_to_repo": f"https://example.com/g/{n}.git"}
                      for n in ("svc", "web")]}) + "\n")
        # The plane root is not a git checkout in this fixture, so `inventory.plane_repo`
        # contributes nothing and the inventory above is the whole list.
        self.enterContext(mock.patch.object(inventory, "plane_repo", return_value=None))
        self.enterContext(mock.patch.object(commands, "_clone_one", self._clone))

    def _clone(self, r: dict, wd) -> dict:
        """What `_clone_one` returns for a repo that really did arrive — with a real git
        repo left behind, because a `repos` row is `_repo_states`/`_branch` reading one."""
        dest = Path(wd) / r["name"]
        if dest.exists():
            # The real one's own answer for a repo already in the workspace, which is what
            # the second of two identical targets is.
            return {"repo": r, "dest": dest, "status": "exists"}
        dest.mkdir(parents=True, exist_ok=True)
        subprocess.run(["git", "init", "-q", "-b", "main", str(dest)],
                       check=True, capture_output=True)
        _git(dest, "config", "user.email", "t@example.com")
        _git(dest, "config", "user.name", "t")
        (dest / "README.md").write_text("hello\n")
        _git(dest, "add", "README.md")
        _git(dest, "-c", "commit.gpgsign=false", "commit", "-qm", "init")
        return {"repo": r, "dest": dest, "status": "ok",
                "forge": SimpleNamespace(cli="gh"), "stderr": ""}

    def _clone_svc(self) -> int:
        return commands.cmd_clone(SimpleNamespace(repos=["svc"], workspace=self.ws))

    def test_the_repos_component_is_not_empty_until_a_restart(self):
        """#886's opening sentence. The cache is what every panel draws — *"a panel never
        gathers on its own, it reads the cache or says it has none"* — so a clone that
        wrote no cache was a `repos` pane with nothing in it and nothing coming."""
        self.assertEqual(self._clone_svc(), 0)
        self.assertEqual([r["name"] for r in gather.cached("alpha.1")["repos"]], ["svc"])

    def test_the_sibling_chat_sees_it_too(self):
        """Both chats in `alpha` draw the same workspace and the same repo list, and one
        clone is one fan-out."""
        self.assertEqual(self._clone_svc(), 0)
        self.assertEqual([r["name"] for r in gather.cached("alpha.2")["repos"]], ["svc"])

    def test_every_frame_is_bumped_so_a_panel_repaints(self):
        """A cache nothing repaints over is the same as no cache: a panel repaints on a
        version bump and on nothing else."""
        before = {f: state.version(f) for f in ("alpha.1", "alpha.2", "beta.1")}
        self.assertEqual(self._clone_svc(), 0)
        for fid in ("alpha.1", "alpha.2", "beta.1"):
            self.assertNotEqual(state.version(fid), before[fid], fid)

    def test_the_frame_in_another_workspace_is_not_given_this_ones_repos(self):
        """`beta.1` is bumped and refreshed for `beta`, which has no clones — the fan-out
        reaches every frame and still keys each one to its own workspace."""
        self.assertEqual(self._clone_svc(), 0)
        self.assertEqual(gather.cached("beta.1")["workspace"], "beta")
        self.assertEqual(gather.cached("beta.1")["repos"], [])

    def test_many_repos_are_one_fan_out(self):
        """*"Cloning ten repos is one fan-out, not ten."* `notify.DEBOUNCE` is per hook
        process and cannot debounce a CLI command against itself, so the discipline is at
        the call site: notify at completion, outside the loop that renders the results. It
        is also what the operator sees — the list arriving complete, once, rather than
        growing a row at a time — and the cost is what it saves, one ~35ms scan per
        workspace for the whole command instead of one per repo."""
        with mock.patch.object(notify, "plane_changed_everywhere") as fan:
            self.assertEqual(
                commands.cmd_clone(SimpleNamespace(repos=["svc", "web"],
                                                   workspace=self.ws)), 0)
        self.assertEqual(fan.call_count, 1)
        self.assertEqual(sorted(p.name for p in workspace.clones(self.ws)),
                         ["svc", "web"])


if __name__ == "__main__":
    unittest.main()
