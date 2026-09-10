"""The suite-wide tripwire that refuses writes into the developer's real `.charter/`.

Every case below installs a THROWAWAY directory as "the real plane" and asserts against
that, never against the operator's actual one. The alternative — pointing a case at the
live `config.STATE_DIR` to prove `rmtree` is refused — would delete a machine's vaults and
running frames the first time the guard regressed, which is the exact accident this file
exists to prevent. The fixture tree is populated and asserted intact afterwards, so a
guard that raised *after* delegating would still be caught.
"""

from __future__ import annotations

import builtins
import io
import os
import shutil
import tempfile
import unittest
from pathlib import Path
from unittest import mock

from charter import config, root
from tests import _envguard, _planeguard


class WhatIsGuarded(unittest.TestCase):
    """The two facts that decide whether the guard is pointed at anything at all."""

    def setUp(self) -> None:
        # Outside a frame, with no session id and no pinned workspace: stated here
        # rather than inherited from the shell the suite was launched from
        # (#519, #521, #528).
        _envguard.unset_all()

    def test_the_guarded_directory_is_this_machines_own_plane_state(self):
        """Installed against the plane the test PROCESS resolved, not some later one.

        Recomputed from `root.find_root` rather than read off `config.STATE_DIR`, which
        any `PersonaIso` case may have repointed by the time this runs.
        """
        state = str(config.derive(root.find_root())["STATE_DIR"])
        self.assertIn(os.path.abspath(state), _planeguard._REAL,
                      "the guard is watching a directory that is not this plane's state")

    def test_the_guarded_file_is_this_planes_own_marker(self):
        """The one file outside the state directory the guard refuses, and #726 is the
        measurement: a running plane rewriting `charter.toml` inside a sibling worktree of
        its own clone, the strips gone from a file nobody had opened, and a `git add -A`
        committing the deletion. Both markers when the suite is in a worktree — the two are
        different files, and the wrong one to write is the one nobody is looking at."""
        for real in _planeguard._REAL_ROOT:
            with self.subTest(plane=real):
                self.assertIn(os.path.join(real, root.MARKER), _planeguard._REAL,
                              "a test could rewrite the file that makes that directory a "
                              "control plane")

    def test_the_suite_reads_the_checkout_these_modules_were_loaded_from(self):
        """#785. `root._plane_of` sends a linked worktree's plane back to the tree it was
        cut from, so without the pin in `_planeguard.install` a run in a worktree asserts
        against the operator's own `charter.toml` — their uncommitted edits included —
        while the same tree on CI asserts against the branch.

        Stated as one claim that holds in all three places rather than as a case that
        skips outside a worktree: the checkout these modules were imported from is a root
        the guard treats as real. In the main clone and on CI that is true because the
        checkout IS the plane; in a worktree it is true only because the pin ran.
        """
        tree = Path(_planeguard.__file__).resolve().parents[1]
        if not (tree / root.MARKER).is_file():
            # `_plane_of`'s own condition, read the other way: a checkout with no marker is
            # not this plane seen from a second directory, and there are no committed
            # settings there to pin to. The pin declines, and so does this.
            self.skipTest(f"this checkout carries no committed {root.MARKER}")
        self.assertIn(os.path.abspath(str(tree)), _planeguard._REAL_ROOT,
                      "the suite is asserting against a checkout that is not the one it "
                      "was loaded from")

    def test_the_plane_a_worktree_redirects_to_is_still_one_the_guard_refuses(self):
        """The half the pin must not cost, and #527 is what it costs if it does. A child
        charter resolves its own plane from its own environment, so in a worktree it
        resolves the operator's clone — and if moving `config.ROOT` merely *swapped* which
        root counted as real, every spawned child would land on the operator's live plane
        unrefused. That was 131 detached children in one run, refreshing forge state and
        rewriting caches. The pin widens the refusal; it does not move it."""
        tree = Path(_planeguard.__file__).resolve().parents[1]
        self.assertIn(os.path.abspath(str(root._plane_of(tree))),
                      _planeguard._REAL_ROOT)

    def test_every_write_primitive_is_wrapped_at_package_import(self):
        """No test can opt out by forgetting a base class, because nothing opted IN.

        Named one by one: each of these is a way to create or destroy a file that some
        part of charter actually uses, and a list that drifts short is a guard with a
        door left open. `os.makedirs` is deliberately absent — it calls the module-level
        `mkdir` by name, so it goes through that wrapper.
        """
        guarded = {"tests._planeguard"}
        for owner, name in ((os, "mkdir"), (os, "rmdir"), (os, "remove"), (os, "unlink"),
                            (os, "rename"), (os, "replace"), (os, "symlink"), (os, "link"),
                            (os, "truncate"), (os, "chmod"), (os, "open"),
                            (shutil, "rmtree"), (builtins, "open"), (io, "open")):
            with self.subTest(call=f"{owner.__name__}.{name}"):
                fn = getattr(owner, name)
                self.assertIn(getattr(fn, "__module__", None), guarded,
                              f"{owner.__name__}.{name} is unwrapped — writes through it "
                              f"reach the real plane unseen")


class WhereTheSuiteIsAllowedToBeRun(unittest.TestCase):
    """`_the_tree_the_suite_is_in`, asked of a FIXTURE rather than of this machine.

    The three cases above can only ever report where the suite happens to be standing, and
    that is precisely how #944 survived: the pin was written for a worktree of the plane
    (#785), CONTRIBUTING's own first instruction is to work in a **workspace clone**, and
    nobody ever asked the question one level down. On unmodified code the suite came back
    ``FAILED (failures=2, errors=2)`` in the clone and in a worktree of it, green in the
    plane root, and green on CI — so the four reds looked like the contributor's own
    breakage.

    Every arrangement charter can put a checkout in is built here out of directories, with
    no git: a linked worktree is a ``.git`` FILE naming its main tree, which is all
    `root.main_worktree_of` reads, and a plane is a directory with a ``charter.toml`` in it.
    """

    def setUp(self) -> None:
        # `.resolve()` once, here: on macOS `/tmp` is a symlink to `/private/tmp`, and the
        # gitdir pointer written below has to name the same spelling `main_worktree_of`
        # resolves to or the two never compare equal.
        self.tmp = Path(tempfile.mkdtemp(prefix="guard-nested-")).resolve()
        self.addCleanup(shutil.rmtree, self.tmp, True)
        self.plane = self._plane(self.tmp / "plane")
        self.clone = self._plane(self.plane / "workspaces" / "ws" / "charter", repo=True)
        self.piece = self._worktree(self.clone, self.plane / "workspaces" / "ws"
                                    / ".worktrees" / "charter" / "piece")

    @staticmethod
    def _plane(where: Path, repo: bool = False) -> Path:
        """A control plane: a directory with a marker. ``repo`` gives it a real ``.git``
        directory too, which is what makes it a CLONE rather than a worktree — and what
        makes `root.tree_of` answer ``None`` for it, correctly and permanently."""
        where.mkdir(parents=True)
        (where / root.MARKER).write_text("schema = 1\n")
        if repo:
            (where / ".git").mkdir()
        return where

    @staticmethod
    def _worktree(main: Path, where: Path, marker: bool = True) -> Path:
        """A linked worktree of *main*: the ``.git`` file git writes, plus the tracked
        marker that gets checked out with everything else."""
        where.mkdir(parents=True)
        (where / ".git").write_text(f"gitdir: {main / '.git' / 'worktrees' / where.name}\n")
        if marker:
            (where / root.MARKER).write_text("schema = 1\n")
        return where

    def test_a_worktree_of_the_plane_itself_is_still_what_the_suite_pins_to(self):
        """#785's own arrangement, kept as the control: the first route must still fire,
        and it must not start naming a second plane that is not there."""
        wt = self._worktree(self.plane, self.tmp / "cut-from-the-plane")
        self.assertEqual((wt, (self.plane,)),
                         _planeguard._the_tree_the_suite_is_in(self.plane, wt))

    def test_a_workspace_clone_is_the_checkout_the_suite_must_read(self):
        """CONTRIBUTING's first instruction. `find_root` hops outward through
        ``workspaces/`` (#200), so standing in a clone the suite asserted against the
        OPERATOR's `charter.toml` — uncommitted edits and all — instead of the branch's."""
        self.assertEqual((self.clone, (self.plane,)),
                         _planeguard._the_tree_the_suite_is_in(self.plane, self.clone))

    def test_a_worktree_of_a_workspace_clone_is_too_and_names_both_planes(self):
        """What `charter wt add <repo> <piece> -w <ws>` builds, and the two hops that hid
        it: `main_worktree_of(here)` is the CLONE while `config.ROOT` is one level further
        out, so `tree_of` — narrow on purpose — can never match and answered ``None``.

        Both planes come back, because pinning moves the suite away from both and the rule
        `_pin_the_suite_to_its_own_tree` states for one level holds for two: the guard gets
        wider, never narrower. A child charter forked from here resolves the outer plane;
        `$CHARTER_ROOT=<the clone>` resolves the clone."""
        self.assertEqual((self.piece, (self.plane, self.clone)),
                         _planeguard._the_tree_the_suite_is_in(self.plane, self.piece))

    def test_the_trigger_is_being_cut_from_a_nested_plane_not_a_path_with_workspaces_in_it(self):
        """The control that keeps the fix off path arithmetic. `git worktree add` puts a
        worktree wherever it is told, and one cut from the clone but placed outside the
        plane entirely reads the clone's committed files just the same — so it is pinned
        just the same. `root.nested_plane_in` keys on the chain of enclosing planes, which
        this checkout is in and its directory name says nothing about."""
        off = self._worktree(self.clone, self.tmp / "nowhere-near-the-plane")
        self.assertEqual((off, (self.plane, self.clone)),
                         _planeguard._the_tree_the_suite_is_in(self.plane, off))

    def test_a_checkout_with_nothing_to_do_with_this_plane_is_left_alone(self):
        """The other control, and the reason this is not "pin to wherever you are". A
        charter checkout somewhere else on disk is not this plane seen from a second
        directory; pinning to it would hand the suite settings nobody asked for."""
        other = self._plane(self.tmp / "someone-elses-charter", repo=True)
        stranger = self._worktree(other, self.tmp / "someone-elses-piece")
        self.assertEqual((None, ()),
                         _planeguard._the_tree_the_suite_is_in(self.plane, stranger))
        self.assertEqual((None, ()),
                         _planeguard._the_tree_the_suite_is_in(self.plane, other))

    def test_a_checkout_carrying_no_committed_marker_is_left_alone(self):
        """`_plane_of`'s own condition read the other way round: a branch that predates the
        committed `charter.toml` has no committed settings to pin to, and pinning to it
        would hand the suite a plane-less root.

        Asked on the arrangements where a route actually FINDS the markerless tree, because
        those are the only ones that reach the check. A markerless worktree of the clone
        placed beside the plane is declined earlier by both routes on their own —
        `nested_plane_in` walks up for a marker and finds none — so a case built on that
        alone stayed green with the check deleted, measured by removing it.

        The last two are the ones the check could not see at all, found in review (#949): a
        markerless worktree of the PLANE placed inside the clone's own `.worktrees/`, and one
        placed inside a worktree of the clone. Route 2 walks up to the nearest marker — the
        clone's, or its worktree's — and names THAT, a checkout the suite was not loaded from,
        so the check read the wrong `charter.toml`, passed, and pinned. `main` had left both
        unpinned."""
        for route, bare in (
                ("tree_of — a worktree of the plane itself",
                 self._worktree(self.plane, self.tmp / "before-the-marker", marker=False)),
                ("nested_plane_in — a worktree of the clone, inside the clone",
                 self._worktree(self.clone, self.clone / "sub" / "before-the-marker",
                                marker=False)),
                ("both — a worktree of the plane, inside the clone's .worktrees/",
                 self._worktree(self.plane, self.clone / ".worktrees" / "old-of-plane",
                                marker=False)),
                ("both — a worktree of the plane, inside a worktree of the clone",
                 self._worktree(self.plane, self.piece / "sub" / "old-of-plane-in-a-piece",
                                marker=False))):
            with self.subTest(route=route):
                self.assertEqual((None, ()),
                                 _planeguard._the_tree_the_suite_is_in(self.plane, bare))

    def test_the_answer_is_the_checkout_itself_not_a_worktree_around_it(self):
        """What the pin promises is *the checkout these modules were loaded from*, and the
        two routes can disagree about that. `tree_of` walks up for ANY ancestor whose main
        tree is the plane, so a clone sitting inside a worktree of the plane gets that
        worktree named — a directory the suite was not loaded from. `nested_plane_in` starts
        at the nearest marker, which is the checkout's own, and names the clone.

        Hand-built — charter's own `clone` resolves the outermost plane and never puts a
        clone there — which is why nothing noticed: in every arrangement charter builds, the
        two routes never both answer. Asking the second route only when the first came back
        empty was indistinguishable from asking it always, until this.

        The worktree it overrules stays guarded (#949 review). `main` pinned the suite THERE,
        so that worktree and its `charter.toml` were inside the refusal, and a child handed
        `$CHARTER_ROOT=<that worktree>` still resolves it. Naming the clone instead must not
        quietly drop them: the rule `_pin_the_suite_to_its_own_tree` states is that the guard
        gets wider, never narrower."""
        around = self._worktree(self.plane, self.plane / "workspaces" / "ws"
                                / ".worktrees" / "plane" / "piece")
        inside = self._plane(around / "workspaces" / "ws2" / "charter", repo=True)
        tree, planes = _planeguard._the_tree_the_suite_is_in(self.plane, inside)
        self.assertEqual(inside, tree)
        self.assertIn(self.plane, planes)
        self.assertIn(around, planes, "the worktree `main` pinned to fell out of the guard")

    def test_a_worktree_the_pin_does_not_overrule_is_still_guarded(self):
        """The arrangement above with a MARKERLESS checkout in it: a worktree of that inner
        clone, cut from a branch that predates `charter.toml`. There is nothing committed to
        pin the suite to, so there is nothing to overrule `main`'s answer with — and whatever
        this answers, the worktree `main` guarded must still be guarded. Overruling first
        and letting the marker check discard the result afterwards loses both: the pin, which
        is right, and the worktree's place in the refusal, which is not."""
        around = self._worktree(self.plane, self.plane / "workspaces" / "ws"
                                / ".worktrees" / "plane" / "piece")
        inner = self._plane(around / "workspaces" / "ws2" / "charter", repo=True)
        bare = self._worktree(inner, inner / "sub" / "before-the-marker", marker=False)
        tree, planes = _planeguard._the_tree_the_suite_is_in(self.plane, bare)
        self.assertIn(around, {tree, *planes},
                      "a worktree `main` refused writes and spawns for fell out of the guard")

    def test_a_checkout_named_through_a_symlink_is_the_same_checkout(self):
        """`tree_of` and `nested_plane_in` resolve what they are handed, so what they answer
        is always the resolved spelling, and the overrule compares that answer with the
        checkout. Compared with the spelling as given instead, a clone reached through a
        symlink — macOS's `/tmp`, a symlinked home — never equals it, route 2 never overrules,
        and the suite is left unpinned exactly where #944 found it."""
        link = self.tmp / "a-symlink-to-the-clone"
        link.symlink_to(self.clone, target_is_directory=True)
        self.assertEqual((self.clone, (self.plane,)),
                         _planeguard._the_tree_the_suite_is_in(self.plane, link))


class _FakePlane(unittest.TestCase):
    """A throwaway directory installed as "the real plane" for the duration of one test."""

    def setUp(self):
        self.plane = Path(tempfile.mkdtemp(prefix="guard-fake-plane-"))
        self.addCleanup(shutil.rmtree, self.plane, True)
        self.elsewhere = Path(tempfile.mkdtemp(prefix="guard-elsewhere-"))
        self.addCleanup(shutil.rmtree, self.elsewhere, True)

        # A populated tree, so "refused" and "deleted, then refused" are distinguishable.
        # The frame is NOT named `<workspace>-<pid>`: nothing here calls `reap`, so the
        # name means nothing, and a name that LOOKS like a frame id would invite a reader
        # to think liveness was being tested. (A `-1` suffix would be worse still — pid 1
        # is launchd, permanently alive, so anything keying off it can never fail.)
        (self.plane / "frame" / "a-frame-still-on-screen").mkdir(parents=True)
        (self.plane / "frame" / "a-frame-still-on-screen" / "exit").write_text("0\n")
        (self.plane / "vaults.json").write_text("{}")
        self.enterContext(mock.patch.object(
            _planeguard, "_REAL", (str(self.plane), str(self.plane.resolve()))))

    def assertPlaneIntact(self):
        exit_file = self.plane / "frame" / "a-frame-still-on-screen" / "exit"
        self.assertEqual(exit_file.read_text(), "0\n")
        self.assertEqual((self.plane / "vaults.json").read_text(), "{}")


class WritesAreRefused(_FakePlane):
    def test_reap_cannot_delete_a_live_frames_state(self):
        """The #402 accident itself: `frame.state.reap` rmtree's a frame directory the
        tmux server did not report live, and a faked server reports none.

        `shutil.rmtree` is checked at its own front door because its recursion deletes via
        `os.unlink(name, dir_fd=fd)` — a bare filename against an open directory, which no
        path-based check can resolve. Asserting the tree is still there is therefore the
        whole test: a guard that only watched the primitives passes the raise assertion
        and fails this one.
        """
        with self.assertRaises(_planeguard.RealPlaneWrite):
            shutil.rmtree(self.plane / "frame" / "a-frame-still-on-screen")
        self.assertPlaneIntact()

    def test_a_frame_directory_cannot_be_minted(self):
        with self.assertRaises(_planeguard.RealPlaneWrite):
            (self.plane / "frame" / "demo-2").mkdir(parents=True)
        with self.assertRaises(_planeguard.RealPlaneWrite):
            os.makedirs(self.plane / "frame" / "demo-3")
        self.assertFalse((self.plane / "frame" / "demo-2").exists())
        self.assertFalse((self.plane / "frame" / "demo-3").exists())

    def test_a_record_cannot_be_deleted(self):
        """`planegit.record_push` unlinks `plane-push.json` on a successful push — the
        developer's own record of a stranded plane, erased by a test that only meant to
        inspect an argv."""
        rec = self.plane / "vaults.json"
        with self.assertRaises(_planeguard.RealPlaneWrite):
            rec.unlink(missing_ok=True)
        with self.assertRaises(_planeguard.RealPlaneWrite):
            os.remove(rec)
        self.assertPlaneIntact()

    def test_a_file_cannot_be_written_over(self):
        for mode in ("w", "a", "x", "r+"):
            with self.subTest(mode=mode), self.assertRaises(_planeguard.RealPlaneWrite):
                builtins.open(self.plane / "vaults.json", mode)
        with self.assertRaises(_planeguard.RealPlaneWrite):
            (self.plane / "vaults.json").write_text("clobbered")
        with self.assertRaises(_planeguard.RealPlaneWrite):
            os.open(self.plane / "vaults.json", os.O_WRONLY | os.O_TRUNC)
        self.assertPlaneIntact()

    def test_a_file_cannot_be_moved_in_or_out(self):
        """Both ends of a rename: the source vanishes and the destination is overwritten,
        so watching one argument leaves the other as a way in."""
        outside = self.elsewhere / "spare"
        outside.write_text("x")
        with self.assertRaises(_planeguard.RealPlaneWrite):
            os.rename(self.plane / "vaults.json", outside)
        with self.assertRaises(_planeguard.RealPlaneWrite):
            os.replace(outside, self.plane / "vaults.json")
        self.assertPlaneIntact()
        self.assertEqual(outside.read_text(), "x")

    def test_a_link_cannot_be_planted(self):
        """`os.symlink(target, link)` CREATES its second argument and only reads its
        first — the opposite of every other call here."""
        with self.assertRaises(_planeguard.RealPlaneWrite):
            os.symlink(self.elsewhere, self.plane / "planted")
        with self.assertRaises(_planeguard.RealPlaneWrite):
            os.link(self.plane / "vaults.json", self.plane / "hard")
        self.assertFalse((self.plane / "planted").is_symlink())
        self.assertFalse((self.plane / "hard").exists())

    def _a_guarded_marker(self) -> Path:
        """A `charter.toml` in a directory that is otherwise NOT guarded, so the cases below
        can tell a guarded path from a guarded prefix — which is the whole shape of this
        entry: the plane's tracked content stays writable and one file does not."""
        marker = self.elsewhere / root.MARKER
        marker.write_text("schema = 1\n")
        self.enterContext(mock.patch.object(
            _planeguard, "_REAL", (*_planeguard._REAL, str(marker))))
        return marker

    def test_the_planes_own_marker_cannot_be_rewritten(self):
        """#726, contained. Every spelling a rewrite arrives as: `charter.toml` is
        hand-maintained, so a writer that means to keep the comments reads it, edits the
        text and writes it back (`instance._set_key`), and one that does not truncates it or
        renames a new file over it."""
        marker = self._a_guarded_marker()
        for mode in ("w", "a", "r+"):
            with self.subTest(mode=mode), self.assertRaises(_planeguard.RealPlaneWrite):
                builtins.open(marker, mode)
        with self.assertRaises(_planeguard.RealPlaneWrite):
            marker.write_text("")
        with self.assertRaises(_planeguard.RealPlaneWrite):
            os.truncate(marker, 0)
        with self.assertRaises(_planeguard.RealPlaneWrite):
            os.unlink(marker)
        (self.elsewhere / "spare").write_text("new file")
        with self.assertRaises(_planeguard.RealPlaneWrite):
            os.replace(self.elsewhere / "spare", marker)
        self.assertEqual(marker.read_text(), "schema = 1\n")

    def test_it_is_that_one_file_and_not_the_directory_holding_it(self):
        """The boundary this entry has to keep, or it becomes the "stream of false alarms"
        the module docstring refuses: `personas/`, `docs/` and `workspaces/` are committed
        content tests do legitimately generate, and only the marker is refused."""
        marker = self._a_guarded_marker()
        (self.elsewhere / "personas").mkdir()
        (self.elsewhere / "personas" / "x.md").write_text("generated")
        (self.elsewhere / "charter.toml.bak").write_text("a backup is not the marker")
        self.assertEqual(marker.read_text(), "schema = 1\n")

    def test_the_message_for_the_marker_names_the_marker(self):
        """A refusal that read "you are writing into the state directory" while pointing at
        `charter.toml` would send its reader looking in the wrong place — and this refusal
        arrives on a line whose author did not think they were touching a plane at all."""
        with self.assertRaises(_planeguard.RealPlaneWrite) as caught:
            self._a_guarded_marker().write_text("x")
        said = str(caught.exception)
        self.assertIn(root.MARKER, said)
        self.assertIn("#726", said)
        self.assertNotIn("state directory", said)

    def test_a_relative_path_reaches_the_plane_too(self):
        """A test that `chdir`s into the plane writes with no plane prefix in the string
        at all, and a check that only compared prefixes would wave it straight through."""
        here = os.getcwd()
        self.addCleanup(os.chdir, here)
        os.chdir(self.plane)
        with self.assertRaises(_planeguard.RealPlaneWrite):
            os.mkdir("frame/demo-4")
        with self.assertRaises(_planeguard.RealPlaneWrite):
            os.mkdir(os.path.join("frame", os.pardir, "demo-5"))
        os.chdir(here)
        self.assertFalse((self.plane / "frame" / "demo-4").exists())
        self.assertFalse((self.plane / "demo-5").exists())


class WhatIsStillAllowed(_FakePlane):
    """A guard that refused everything would pass every case above and stop the suite
    dead. These are the calls that must keep working."""

    def test_reading_the_real_plane_is_untouched(self):
        self.assertEqual((self.plane / "vaults.json").read_text(), "{}")
        self.assertIn("vaults.json", os.listdir(self.plane))
        with builtins.open(self.plane / "vaults.json") as fh:
            self.assertEqual(fh.read(), "{}")
        self.assertEqual(os.stat(self.plane / "vaults.json").st_size, 2)

    def test_writing_anywhere_else_is_untouched(self):
        (self.elsewhere / "d").mkdir()
        (self.elsewhere / "d" / "f").write_text("ok")
        os.replace(self.elsewhere / "d" / "f", self.elsewhere / "g")
        self.assertEqual((self.elsewhere / "g").read_text(), "ok")
        shutil.rmtree(self.elsewhere / "d")
        self.assertFalse((self.elsewhere / "d").exists())

    def test_a_path_that_merely_starts_with_the_planes_name_is_not_inside_it(self):
        """`<plane>-sibling` shares the plane's string prefix and is a different
        directory; a `startswith` with no separator would refuse it."""
        sibling = Path(str(self.plane) + "-sibling")
        self.addCleanup(shutil.rmtree, sibling, True)
        sibling.mkdir()
        (sibling / "f").write_text("ok")
        self.assertEqual((sibling / "f").read_text(), "ok")


class TheRefusalCannotBeSwallowed(_FakePlane):
    def test_it_is_not_an_exception(self):
        """charter's write paths are wrapped in `except OSError` / `except Exception`
        fallbacks that exist so a degraded environment cannot break a command —
        `record_push` ends in `except OSError: pass`. A tripwire those can catch reports
        nothing and the test goes green over a deleted plane."""
        self.assertFalse(issubclass(_planeguard.RealPlaneWrite, Exception))
        try:
            os.mkdir(self.plane / "swallowed")
        except Exception:                                  # noqa: BLE001 — the point
            self.fail("`except Exception` swallowed the tripwire")
        except _planeguard.RealPlaneWrite:
            pass
        self.assertFalse((self.plane / "swallowed").exists())

    def test_it_names_the_path_and_the_way_out(self):
        with self.assertRaises(_planeguard.RealPlaneWrite) as caught:
            os.mkdir(self.plane / "frame" / "demo-5")
        msg = str(caught.exception)
        self.assertIn(str(self.plane / "frame" / "demo-5"), msg)
        self.assertIn("PersonaIso", msg)
        self.assertIn("CHARTER_ROOT", msg)      # the half PersonaIso cannot fix


if __name__ == "__main__":
    unittest.main()
