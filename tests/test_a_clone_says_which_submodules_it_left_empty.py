"""A clone that fetched less than the repo records says so, and a sync stops calling it
up to date.

`git clone` does not fetch submodules unless asked, and `charter clone` never asked. So a
repo whose tooling lives in a submodule arrived with an EMPTY directory where the tooling
should be, and charter's own output was a green tick. Reproduced against the real
`commands._clone_one` — the clone succeeds, `git submodule status` answers a leading `-`,
and the first thing that runs from the submodule dies:

    ✓ super → workspaces/ws/super (main via gh, HTTPS)
    $ git -C .../super submodule status
    -038cbf71a398dadefbaad26e157b70ade2e2f2db dev-scripts
    $ sh dev-scripts/docker-build.sh
    sh: dev-scripts/docker-build.sh: No such file or directory        (rc 127)

**charter says, and does not do.** Initialising means charter fetching a URL it did not
build, out of `.gitmodules` — a file inside the repo it just cloned — over the network,
recursively, with a forge token in the credential helper. `_https_url` already refuses to
hand git a string charter did not build (#335); auto-initialising would put that same
string back one layer down, where the allowlist cannot see it.

It could not even do it under its own rule. Measured on git 2.50.1: **`git clone` does not
read the local config of the repository it is standing in** — only system, global and
`-c`. A submodule fetch IS a nested `git clone`, so `gitpolicy.apply`'s `--local`
`credential.helper` and `url.<https>.insteadOf` never reach it:

    LOCAL protocol.file.allow in the superproject -> submodule init rc = 1  (not read)
    -c on the command line                        -> submodule init rc = 0
    GLOBAL protocol.file.allow                    -> submodule init rc = 0
    LOCAL submodule.<name>.url override           -> submodule init rc = 0  (parent reads it)

The last line is the asymmetry: the PARENT resolves the URL from local config, the CHILD
consumes the transport config and cannot see it. So golden rule 0 does not hold for a
submodule fetch, and a charter that initialised them would be quietly fetching outside
its own credential policy.

`sync` is the same silence with a stronger claim on it. Measured on a clone whose upstream
moved one submodule pointer and added a second submodule, `charter sync` printed

    ✓ ws/sync_clone: up to date on main

with `dev-scripts` still at the old commit and an empty `extra-tools/` beside it. The
branch was up to date; the tree was not, and only the branch was mentioned.

`gl-refresh` does NOT share the gap and is deliberately left alone: it runs no git command
that touches a working tree — it asks each clone's forge for the open change and last CI
and writes a cache the status line reads (`glstate.refresh`).
"""
from __future__ import annotations

import io
import os
import subprocess
import unittest
from contextlib import redirect_stderr, redirect_stdout
from pathlib import Path
from types import SimpleNamespace
from unittest import mock

from charter import commands, config

from ._isolation import PersonaIso


def git(*args, cwd) -> subprocess.CompletedProcess:
    return subprocess.run(["git", *args], cwd=str(cwd), capture_output=True, text=True,
                          check=False)


def make_repo(d: Path) -> Path:
    """A one-commit git repository at *d*."""
    d.mkdir(parents=True, exist_ok=True)
    git("init", "-q", "-b", "main", ".", cwd=d)
    (d / "README.md").write_text("top\n")
    git("add", "README.md", cwd=d)
    git("commit", "-qm", "top", cwd=d)
    return d


#: A gitlink points at a commit; nothing requires that commit to be reachable, or the
#: submodule's repository to exist at all. Planting one by hand is what lets these cases
#: build the state under test — a recorded-but-absent submodule — without a network, a
#: second repository, or `protocol.file.allow`, none of which is the thing being asserted.
UNREACHABLE = "0" * 39 + "1"


def plant_submodule(repo: Path, path: str, url: str = "https://example.invalid/g/s.git",
                    sha: str = UNREACHABLE, name: str | None = None) -> None:
    """Record a submodule at *path* in *repo* and commit it, fetching nothing.

    The empty directory is created because a real `git clone` creates one — measured, and
    it matters: without it `git status --porcelain` answers `` D dev-scripts``, so the
    fixture would be DIRTY and every assertion below would be reading a state no clone
    ever produces (`_sync_one` skips a dirty tree outright)."""
    git("update-index", "--add", "--cacheinfo", f"160000,{sha},{path}", cwd=repo)
    with (repo / ".gitmodules").open("a") as fh:
        fh.write(f'[submodule "{name or path}"]\n\tpath = {path}\n\turl = {url}\n')
    git("add", ".gitmodules", cwd=repo)
    git("commit", "-qm", f"record {path}", cwd=repo)
    (repo / path).mkdir(parents=True, exist_ok=True)


class SubmoduleCase(PersonaIso):
    def setUp(self) -> None:
        super().setUp()
        # Every fixture submodule below has a PATH for its origin, and git has refused the
        # `file` transport for submodules since 2.38 (CVE-2022-39253). Passing
        # `-c protocol.file.allow=always` covers the calls this module makes by hand and
        # NOT the ones charter makes for itself — and `_sync_one`'s own `git fetch --prune`
        # needs it, because git recurses into submodules on demand while fetching
        # (measured: `Fetching submodule dev-scripts` in its stderr). git's
        # `GIT_CONFIG_KEY_n` mechanism reaches every child of this process, charter's
        # included, which is the only spelling that does.
        self.enterContext(mock.patch.dict(os.environ, {
            "GIT_CONFIG_COUNT": "1",
            "GIT_CONFIG_KEY_0": "protocol.file.allow",
            "GIT_CONFIG_VALUE_0": "always",
        }))

    def repo(self, name: str = "super") -> Path:
        return make_repo(config.ROOT / "workspaces" / "ws" / name)

    def said(self, fn, *a, **kw) -> str:
        buf = io.StringIO()
        with redirect_stdout(buf), redirect_stderr(buf):
            fn(*a, **kw)
        return buf.getvalue()


# --------------------------------------------------------------------------- #
# what a tree actually reports                                                 #
# --------------------------------------------------------------------------- #
class TestWhatTheTreeReports(SubmoduleCase):
    def test_a_repo_with_no_submodules_reports_none(self):
        self.assertEqual(commands.submodule_drift(self.repo()), ([], []))

    def test_a_recorded_submodule_with_nothing_checked_out_is_named(self):
        r = self.repo()
        plant_submodule(r, "dev-scripts")
        self.assertEqual(commands.submodule_drift(r), (["dev-scripts"], []))

    def test_every_uninitialised_submodule_is_named_not_just_the_first(self):
        r = self.repo()
        plant_submodule(r, "dev-scripts")
        plant_submodule(r, "extra-tools")
        self.assertEqual(commands.submodule_drift(r), (["dev-scripts", "extra-tools"], []))

    def test_a_submodule_at_the_recorded_commit_is_not_drift(self):
        r, sub = self.repo(), make_repo(self.tmp / "sub")
        git("submodule", "add", "-q", str(sub), "dev-scripts", cwd=r)
        git("commit", "-qm", "add", cwd=r)
        self.assertEqual(commands.submodule_drift(r), ([], []))

    def test_a_submodule_left_behind_the_commit_the_branch_records_is_named(self):
        """The state `sync` itself creates: a fast-forward moves the gitlink and never
        touches the submodule's own checkout."""
        r, sub = self.repo(), make_repo(self.tmp / "sub")
        git("submodule", "add", "-q", str(sub), "dev-scripts", cwd=r)
        git("commit", "-qm", "add", cwd=r)
        git("commit", "-q", "--allow-empty", "-m", "v2", cwd=r / "dev-scripts")
        moved = git("rev-parse", "HEAD", cwd=r / "dev-scripts").stdout.strip()
        git("update-index", "--cacheinfo", f"160000,{moved},dev-scripts", cwd=r)
        git("commit", "-qm", "bump", cwd=r)
        git("checkout", "-q", "HEAD~1", cwd=r / "dev-scripts")
        self.assertEqual(commands.submodule_drift(r), ([], ["dev-scripts"]))

    def test_a_submodule_path_with_a_space_in_it_survives_the_parse(self):
        """`git submodule status` is `<mark><sha> <path>` and, for a checked-out one, a
        trailing ` (<describe>)`. Splitting on every space reads the describe — or the last
        word of the path — as the name. Nothing stops a repo naming a directory this way,
        and the sweep cannot tell a parse that only ever sees single-word paths from a
        correct one."""
        r = self.repo()
        plant_submodule(r, "dev scripts")
        self.assertEqual(commands.submodule_drift(r), (["dev scripts"], []))

    def test_a_checked_out_submodule_is_not_named_after_its_describe(self):
        r, sub = self.repo(), make_repo(self.tmp / "sub")
        git("submodule", "add", "-q", str(sub), "dev scripts", cwd=r)
        git("commit", "-qm", "add", cwd=r)
        git("commit", "-q", "--allow-empty", "-m", "v2", cwd=r / "dev scripts")
        moved = git("rev-parse", "HEAD", cwd=r / "dev scripts").stdout.strip()
        git("update-index", "--cacheinfo", f"160000,{moved},dev scripts", cwd=r)
        git("commit", "-qm", "bump", cwd=r)
        git("checkout", "-q", "HEAD~1", cwd=r / "dev scripts")
        self.assertEqual(commands.submodule_drift(r), ([], ["dev scripts"]))

    def test_a_directory_that_is_not_a_repository_reports_nothing(self):
        """Called from `status` on whatever sits in a workspace — it must not raise."""
        d = config.ROOT / "workspaces" / "ws" / "notarepo"
        d.mkdir(parents=True)
        (d / ".gitmodules").write_text("[submodule \"x\"]\n\tpath = x\n\turl = u\n")
        self.assertEqual(commands.submodule_drift(d), ([], []))

    def test_no_gitmodules_file_costs_no_git_process(self):
        """`status` already runs two git calls per row and the table draws as it goes; a
        third on every clone that has no submodules at all is a cost with no answer."""
        r = self.repo()
        with mock.patch.object(commands, "_git",
                               side_effect=AssertionError("ran git anyway")) as spy:
            self.assertEqual(commands.submodule_drift(r), ([], []))
        spy.assert_not_called()


# --------------------------------------------------------------------------- #
# clone says so                                                                #
# --------------------------------------------------------------------------- #
class TestCloneSaysWhatItLeftEmpty(SubmoduleCase):
    def clone_output(self, dest: Path) -> str:
        r = {"name": dest.name, "default_branch": "main", "forge": "github",
             "path_with_namespace": f"g/{dest.name}"}
        out = []
        with mock.patch.object(commands, "_clone_one",
                               lambda rr, wd: {"repo": rr, "dest": dest, "status": "ok",
                                               "forge": SimpleNamespace(cli="gh")}), \
             mock.patch.object(commands.util, "info", lambda m, *a: out.append(m)), \
             mock.patch.object(commands.util, "ok", lambda m, *a: out.append(m)), \
             mock.patch.object(commands.util, "warn", lambda m, *a: out.append(m)), \
             mock.patch.object(commands.util, "err", lambda m, *a: out.append(m)), \
             mock.patch.object(commands.workspace, "resolve", lambda *a, **k: "ws"), \
             mock.patch.object(commands.workspace, "banner", lambda *a, **k: None), \
             mock.patch.object(commands.workspace, "ensure",
                               lambda *a: config.ROOT / "workspaces" / "ws"), \
             mock.patch.object(commands.inventory, "load", lambda: {}), \
             mock.patch.object(commands.inventory, "repos", lambda d=None: [r]), \
             mock.patch.object(commands, "_resolve_targets", lambda a, d: [r]):
            commands.cmd_clone(SimpleNamespace(repos=[r["name"]], workspace="ws"))
        return "\n".join(out)

    def test_a_clone_with_no_submodules_says_nothing_extra(self):
        said = self.clone_output(self.repo())
        self.assertNotIn("submodule", said)

    def test_the_uninitialised_submodule_is_named(self):
        r = self.repo()
        plant_submodule(r, "dev-scripts")
        said = self.clone_output(r)
        self.assertIn("dev-scripts", said)
        # Spelled out by hand, a second time: this sentence is the whole fix, and a test
        # that builds it from the same f-string the code does would pass on any wording.
        self.assertIn("1 submodule(s) recorded but not initialised", said)
        self.assertIn("nothing is checked out there", said)

    def test_it_names_the_command_that_initialises_them(self):
        r = self.repo()
        plant_submodule(r, "dev-scripts")
        said = self.clone_output(r)
        self.assertIn("submodule update --init --recursive", said)
        self.assertIn(f"git -C workspaces/ws/{r.name}", said)

    def test_it_says_charter_will_not_fetch_them_and_why(self):
        """A refusal that does not say why reads as a bug — and this one is a decision."""
        r = self.repo()
        plant_submodule(r, "dev-scripts")
        said = self.clone_output(r)
        self.assertIn("charter does not fetch them", said)
        self.assertIn(".gitmodules", said)

    def test_a_repo_that_was_already_cloned_is_told_about_too(self):
        """`already cloned in 'ws'` is the line an operator gets on every re-run of a
        `workspace restore`, and the submodule is no more initialised for being old. The
        report sits after the whole status branch on purpose, not inside the `ok` arm."""
        r = self.repo()
        plant_submodule(r, "dev-scripts")
        out = []
        rec = {"repo": {"name": r.name}, "dest": r, "status": "exists"}
        with mock.patch.object(commands, "_clone_one", lambda rr, wd: {**rec, "repo": rr}), \
             mock.patch.object(commands.util, "info", lambda m, *a: out.append(m)), \
             mock.patch.object(commands.util, "ok", lambda m, *a: out.append(m)), \
             mock.patch.object(commands.util, "warn", lambda m, *a: out.append(m)), \
             mock.patch.object(commands.workspace, "resolve", lambda *a, **k: "ws"), \
             mock.patch.object(commands.workspace, "banner", lambda *a, **k: None), \
             mock.patch.object(commands.workspace, "ensure",
                               lambda *a: config.ROOT / "workspaces" / "ws"), \
             mock.patch.object(commands.inventory, "load", lambda: {}), \
             mock.patch.object(commands.inventory, "repos",
                               lambda d=None: [{"name": r.name}]), \
             mock.patch.object(commands, "_resolve_targets",
                               lambda a, d: [{"name": r.name}]):
            commands.cmd_clone(SimpleNamespace(repos=[r.name], workspace="ws"))
        said = "\n".join(out)
        self.assertIn("already cloned", said)
        self.assertIn("dev-scripts", said)

    def test_the_clone_still_succeeded(self):
        """This is a report, not a failure: the repo IS cloned, and an exit code that said
        otherwise would break every `workspace restore` that has a submodule in it."""
        r = self.repo()
        plant_submodule(r, "dev-scripts")
        said = self.clone_output(r)
        self.assertIn("→", said)


# --------------------------------------------------------------------------- #
# sync stops saying "up to date"                                               #
# --------------------------------------------------------------------------- #
class TestSyncStopsCallingItUpToDate(SubmoduleCase):
    def clone_of(self, seed: Path, name: str = "super") -> Path:
        self.bare = self.tmp / f"{name}.git"
        self.seed = seed
        git("clone", "-q", "--bare", str(seed), str(self.bare), cwd=self.tmp)
        dest = config.ROOT / "workspaces" / "ws" / name
        dest.parent.mkdir(parents=True, exist_ok=True)
        git("clone", "-q", "--branch", "main", "--", str(self.bare), str(dest), cwd=self.tmp)
        return dest

    def publish(self) -> None:
        """Push the seed's `main` to the bare origin the clone fetches from. By path: the
        seed was created with `git init` and has no remote of its own."""
        p = git("push", "-q", str(self.bare), "main", cwd=self.seed)
        self.assertEqual(p.returncode, 0, p.stderr)

    def test_a_clean_repo_still_gets_its_tick(self):
        said = self.said(commands._sync_one, self.clone_of(make_repo(self.tmp / "seed")), "ws")
        self.assertIn("up to date on main", said)
        self.assertNotIn("submodule", said)

    def test_it_does_not_report_a_bare_success_over_an_empty_submodule(self):
        seed = make_repo(self.tmp / "seed")
        plant_submodule(seed, "dev-scripts")
        said = self.said(commands._sync_one, self.clone_of(seed), "ws")
        self.assertIn("dev-scripts", said)
        # By hand, again — the point of the change is that this line replaces the tick.
        self.assertIn("main is up to date, its submodules are not", said)
        self.assertNotIn("up to date on main", said)

    def test_it_names_the_command_there_too(self):
        seed = make_repo(self.tmp / "seed")
        plant_submodule(seed, "dev-scripts")
        said = self.said(commands._sync_one, self.clone_of(seed), "ws")
        self.assertIn("submodule update --init --recursive", said)

    def test_the_skip_over_a_dirty_tree_explains_itself(self):
        """The trap the previous case sets. A submodule left behind is an unstaged change
        to the gitlink, so the FF that produced it makes every later sync take the
        `uncommitted changes` branch — the repo quietly stops being synced, under a
        sentence about work the operator never did."""
        seed, sub = make_repo(self.tmp / "seed"), make_repo(self.tmp / "sub")
        git("submodule", "add", "-q", str(sub), "dev-scripts", cwd=seed)
        git("commit", "-qm", "add", cwd=seed)
        dest = self.clone_of(seed)
        git("submodule", "update", "--init", cwd=dest)
        git("checkout", "-q", "--detach", "HEAD", cwd=dest / "dev-scripts")
        git("commit", "-q", "--allow-empty", "-m", "local", cwd=dest / "dev-scripts")
        said = self.said(commands._sync_one, dest, "ws")
        self.assertIn("uncommitted changes", said)
        self.assertIn("dev-scripts", said)
        self.assertIn("not at the commit this branch records", said)

    def test_a_dirty_tree_with_no_submodules_says_only_what_it_always_said(self):
        dest = self.clone_of(make_repo(self.tmp / "seed"))
        (dest / "README.md").write_text("edited\n")
        said = self.said(commands._sync_one, dest, "ws")
        self.assertIn("uncommitted changes", said)
        self.assertNotIn("submodule", said)

    def test_a_submodule_the_merge_moved_past_is_reported_too(self):
        """The commonest sync case, and the one a fix for `clone` alone would miss: the
        submodule IS initialised, the fast-forward moved the pointer, and the checkout
        stayed where it was."""
        seed, sub = make_repo(self.tmp / "seed"), make_repo(self.tmp / "sub")
        git("submodule", "add", "-q", str(sub), "dev-scripts", cwd=seed)
        git("commit", "-qm", "add", cwd=seed)
        dest = self.clone_of(seed)
        git("submodule", "update", "--init", cwd=dest)
        # The new commit is made in the submodule's ORIGIN, not in the seed's checkout of
        # it: `git fetch` recurses into submodules on demand, so `_sync_one`'s own fetch
        # goes looking for this commit there and a `not our ref` is a fixture that fails
        # for the wrong reason.
        git("commit", "-q", "--allow-empty", "-m", "v2", cwd=sub)
        moved = git("rev-parse", "HEAD", cwd=sub).stdout.strip()
        git("update-index", "--cacheinfo", f"160000,{moved},dev-scripts", cwd=seed)
        git("commit", "-qm", "bump", cwd=seed)
        self.publish()
        said = self.said(commands._sync_one, dest, "ws")
        self.assertIn("dev-scripts", said)
        self.assertIn("not at the commit main records", said)
        self.assertNotIn("up to date on main", said)


# --------------------------------------------------------------------------- #
# status says so                                                               #
# --------------------------------------------------------------------------- #
class TestStatusRowSaysSo(SubmoduleCase):
    def test_a_clean_row_is_unchanged(self):
        self.assertEqual(commands._clone_note(self.repo()), "main · clean")

    def test_the_row_names_the_uninitialised_count(self):
        r = self.repo()
        plant_submodule(r, "dev-scripts")
        self.assertEqual(commands._clone_note(r),
                         "main · clean · 1 submodule(s) not initialised")

    def test_two_uninitialised_submodules_are_counted_not_just_named(self):
        """The count is what makes the row worth a column, and `(s)` is this repo's form
        for a number it does not know at write time (`{len(targets)} repo(s)`). Pinned at
        two because one reads correctly under either spelling."""
        r = self.repo()
        plant_submodule(r, "dev-scripts")
        plant_submodule(r, "extra-tools")
        self.assertEqual(commands._clone_note(r),
                         "main · clean · 2 submodule(s) not initialised")

    def test_both_kinds_of_drift_fit_in_one_note(self):
        """`dirty` here is not incidental — a submodule left behind IS an unstaged change
        to the gitlink, so this row already said `dirty` and said nothing about why. It is
        also why the row matters: `_sync_one` skips a dirty tree, so the next `charter
        sync` stops syncing this repo at all, over a submodule nobody was told about."""
        r, sub = self.repo(), make_repo(self.tmp / "sub")
        plant_submodule(r, "dev-scripts")
        git("submodule", "add", "-q", str(sub), "extra-tools", cwd=r)
        git("commit", "-qm", "add", cwd=r)
        git("commit", "-q", "--allow-empty", "-m", "v2", cwd=r / "extra-tools")
        moved = git("rev-parse", "HEAD", cwd=r / "extra-tools").stdout.strip()
        git("update-index", "--cacheinfo", f"160000,{moved},extra-tools", cwd=r)
        git("commit", "-qm", "bump", cwd=r)
        git("checkout", "-q", "HEAD~1", cwd=r / "extra-tools")
        self.assertEqual(commands._clone_note(r),
                         "main · dirty · 1 submodule(s) not initialised · 1 out of date")


if __name__ == "__main__":
    unittest.main()
