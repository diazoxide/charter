"""The status line says so when the plane root is being worked in.

The directory holding `charter.toml` is the control plane — personas, inventory,
workspaces, config — and nothing anyone is meant to edit. Work happens in a workspace's
clones. Since ADR 0007 the root is not drawn as a repo at all, and ADR 0008 is about the
gap that leaves: **not presenting a tree is not the same as preventing work in it**. Two
sessions that both sit in the plane root share one working tree and one HEAD and thrash
each other's branches, while charter reports two different workspaces and lists no tree
that would hint at why. The failure is invisible in exactly the surface a user checks.

So the root gets ONE full-width line, and only when it is dirty or off its default
branch. Silence is the whole point of the design: a warning that renders every turn is
furniture within a day, and then a real one draws no more attention than a zero would.

Real git in a temp dir throughout (the tests/test_statusline_worktree_rows.py pattern).
Every fact under test here — HEAD, `origin/HEAD`, the ref store, dirtiness — is a fact
about a real repository on disk, and a mocked git would prove nothing about any of them.
"""
from __future__ import annotations

import os
import re
import subprocess
import unittest
from pathlib import Path
from unittest import mock

from charter import config, statusline, update
from tests._isolation import PersonaIso


def git(repo: Path, *args: str) -> subprocess.CompletedProcess:
    return subprocess.run(["git", "-C", str(repo), *args],
                          check=True, capture_output=True, text=True)


def _plain(s: str) -> str:
    return re.sub(r"\x1b\[[0-9;]*m", "", s)


def init_repo(path: Path, branch: str = "main") -> Path:
    """A real git repo whose working tree is CLEAN: everything already there is
    committed, so a later `git status` reports only what a test itself changed."""
    path.mkdir(parents=True, exist_ok=True)
    subprocess.run(["git", "init", "-q", "-b", branch, str(path)],
                   check=True, capture_output=True)
    git(path, "config", "user.email", "t@example.com")
    git(path, "config", "user.name", "t")
    # Guarantees there is something to commit: an empty first commit leaves no
    # `refs/heads/<branch>`, and half of what is under test here is read from the ref
    # store. A repo with no content is not the fixture any of these tests mean.
    (path / "README.md").write_text("hello\n")
    git(path, "add", "-A")
    git(path, "-c", "commit.gpgsign=false", "commit", "-qm", "init")
    return path


class PlaneRootIso(PersonaIso):
    """A control plane whose root is a real git repo — clean, on `main`."""

    def setUp(self) -> None:
        super().setUp()
        (self.tmp / "charter.toml").write_text("schema = 1\n")
        config.HAS_CONTROL_PLANE = True
        # The ignore rules `charter init` writes. Without them this fixture would be
        # dirty for reasons no real plane is: rendering writes its own caches under
        # `.charter/`, and a workspace's clones live under `workspaces/`. Both are
        # ignored in every real plane, so ignoring them here keeps "clean" meaning in
        # the test what it means in the field.
        (self.tmp / ".gitignore").write_text("/workspaces/*/*\n/.charter/\n")
        # Personas exist before the first commit, so the roster the two-column layout
        # needs does not itself leave the root dirty.
        for n in ("alpha", "beta"):
            self.make_persona(n, role=n.title(), **{"delegate-when": f"{n} work"})
        init_repo(self.tmp)
        # `_brand` forks a detached version check. A suite that quietly reaches the
        # network is not hermetic.
        self.enterContext(mock.patch.object(update, "maybe_spawn", lambda: None))

    # -- the two ways a root gets worked in ------------------------------------ #

    def dirty(self) -> None:
        (self.tmp / "charter.toml").write_text("schema = 1\n# edited\n")

    def switch_to(self, branch: str) -> None:
        git(self.tmp, "checkout", "-q", "-b", branch)

    def forget_state(self) -> None:
        """Drop the cached `git status` answers.

        `_repo_states` trusts an entry for `_STATE_TTL` seconds so the status line forks
        at most one `git status` per repo per few seconds however often it renders. That
        is right in the field and a trap in a test: a tree dirtied between two renders
        inside the window would still be reported clean, and the test would be measuring
        the cache rather than the code."""
        (config.STATE_DIR / "cache" / "repostate.json").unlink(missing_ok=True)

    # -- rendering -------------------------------------------------------------- #

    def alerts(self) -> list[str]:
        return [_plain(a) for a in statusline._alerts(config.DEFAULT_WORKSPACE)]

    def raw(self, width: int = 200) -> list[str]:
        """Rendered lines exactly as emitted, frame included, ANSI stripped."""
        old = os.environ.get("COLUMNS")
        os.environ["COLUMNS"] = str(width)
        try:
            return [_plain(ln) for ln in statusline.render({"session_id": "t"}).split("\n")]
        finally:
            if old is None:
                os.environ.pop("COLUMNS", None)
            else:
                os.environ["COLUMNS"] = old

    def lines(self, width: int = 200) -> list[str]:
        """Content lines with the frame stripped — most of these tests are about which
        rows exist and what they say, not about the box drawn around them."""
        out = []
        for ln in self.raw(width):
            if not ln.strip() or set(ln.strip()) <= set("┌─┐└┘├┤"):
                continue                      # border, or a zone divider
            if ln.startswith("│ ") and ln.rstrip().endswith("│"):
                ln = ln[2:].rstrip()[:-1].rstrip()
            out.append(ln)
        return out

    def warning(self, width: int = 200) -> str | None:
        found = [ln for ln in self.lines(width) if "plane root" in ln]
        return found[0] if found else None


class ADirtyRootSpeaks(PlaneRootIso):
    def test_a_dirty_plane_root_is_warned_about_in_the_status_line(self):
        """The observed failure: work happening in the one tree charter does not draw."""
        self.dirty()
        self.assertIsNotNone(self.warning(), self.lines())

    def test_the_warning_says_dirty_in_words(self):
        """`*` is the repo table's dirty marker and reads as a marker only next to a
        column of them. This line has no siblings, so it says the word."""
        self.dirty()
        self.assertIn("dirty", self.warning())

    def test_the_warning_names_the_plane_root(self):
        """It cannot be mistaken for a workspace repo: the root is the one tree that is
        never a row, so a line about it with no subject would name nothing on screen."""
        self.dirty()
        line = self.warning()
        self.assertIn("plane root", line)
        self.assertIn(config.ROOT.name, line)

    def test_the_root_is_still_not_counted_as_one_of_your_repos(self):
        """Warning about it must not smuggle it back into `repo_trees`: a plane's repos
        are its clones and nothing else (ADR 0007), and the whole reason this warning
        exists is that the root is NOT among them."""
        clone = config.WORKSPACES_DIR / config.DEFAULT_WORKSPACE / "demo"
        init_repo(clone)
        self.dirty()
        joined = "\n".join(self.lines())
        self.assertIn("repos 1", joined)
        self.assertIsNotNone(self.warning())
        self.assertFalse([ln for ln in self.lines()
                          if re.search(rf"[├└╰]─ {re.escape(config.ROOT.name)}\b", ln)],
                         "the plane root was drawn as a repo row")


class AnOffDefaultRootSpeaks(PlaneRootIso):
    def test_a_non_default_branch_produces_a_warning_on_a_clean_tree(self):
        """The sharpest form of the observed failure — six branches cut in the root,
        and a `git checkout main` that reverted in-flight work out of the tree."""
        self.switch_to("feat/x")
        self.assertIsNotNone(self.warning(), self.lines())

    def test_the_warning_names_both_the_branch_and_the_default(self):
        """"Off default" is unactionable without both halves: which branch you are on,
        and which one this tree is supposed to sit on."""
        self.switch_to("feat/x")
        line = self.warning()
        self.assertIn("feat/x", line)
        self.assertIn("main", line)

    def test_a_detached_head_counts_as_off_the_default(self):
        """A detached HEAD in the plane root is the same accident wearing a sha."""
        git(self.tmp, "checkout", "-q", "--detach")
        self.assertIsNotNone(self.warning(), self.lines())

    def test_origin_head_decides_what_the_default_is(self):
        """Not a hardcoded `main`: the repo already states its own default, and a plane
        living on `trunk` must not be told every turn that it is off `main`."""
        head = Path(self.tmp / ".git" / "refs" / "remotes" / "origin")
        head.mkdir(parents=True, exist_ok=True)
        (head / "HEAD").write_text("ref: refs/remotes/origin/trunk\n")
        line = self.warning()               # still on `main`, which is now NOT the default
        self.assertIsNotNone(line, self.lines())
        self.assertIn("trunk", line)

    def test_no_branch_claim_when_the_repo_never_says_what_its_default_is(self):
        """No `origin/HEAD`, no `main`, no `master` — nothing here knows which branch is
        home, and a warning invented from a guess is worse than the silence it replaced.
        Dirtiness still speaks; only the branch half goes quiet."""
        git(self.tmp, "branch", "-m", "main", "trunk")
        git(self.tmp, "checkout", "-q", "-b", "other")
        self.assertIsNone(statusline._plane_root_alert())

    def test_both_findings_share_one_line(self):
        """One line, not two rows. Every row is spent on every turn, and "dirty AND off
        main" is one situation with two symptoms."""
        self.dirty()
        self.switch_to("feat/x")
        self.assertEqual(len([ln for ln in self.lines() if "plane root" in ln]), 1)
        line = self.warning()
        self.assertIn("dirty", line)
        self.assertIn("feat/x", line)


class ACleanRootSaysNothing(PlaneRootIso):
    def test_a_clean_root_on_its_default_branch_renders_nothing_at_all(self):
        """No row, no zero, no badge. Presence IS the signal — the same discipline
        `_session_news` and the todo count keep."""
        self.assertIsNone(statusline._plane_root_alert())
        self.assertIsNone(self.warning(), self.lines())

    def test_a_clean_root_costs_the_layout_no_row(self):
        """Not merely an empty string: an all-clean plane must render the same number of
        rows it rendered before this warning existed."""
        before = len(self.lines())
        self.dirty()
        self.forget_state()
        self.assertEqual(len(self.lines()), before + 1)

    def test_a_plane_root_that_is_not_a_git_repo_is_silent(self):
        """`charter init` in a fresh directory does not run `git init`, and that is the
        README's 60-second path. A plane with no repo cannot be off a branch, and
        warning it about one would be the first thing a new user ever saw."""
        import shutil
        shutil.rmtree(self.tmp / ".git")
        self.assertIsNone(statusline._plane_root_alert())

    def test_a_dirty_tree_around_a_plane_that_is_not_a_repo_is_not_borrowed(self):
        """`charter init` inside a subdirectory of some other repo leaves ROOT without a
        `.git` of its own. Reporting the surrounding repo's dirtiness as the plane
        root's would be a warning about a tree the user never named."""
        import shutil
        outer, inner = self.tmp, self.tmp / "plane"
        shutil.rmtree(outer / ".git")
        init_repo(outer)
        inner.mkdir()
        (inner / "charter.toml").write_text("schema = 1\n")
        (outer / "scratch.txt").write_text("uncommitted\n")   # the OUTER tree is dirty
        self._orig_root = config.ROOT
        config.use(inner)
        self.addCleanup(config.use, outer)
        self.assertIsNone(statusline._plane_root_alert())


class TheLineIsFullWidthFurniture(PlaneRootIso):
    """It sits with the other control-plane alerts: not a property of the active
    workspace (the top line) and not a property of this session (the bottom strip), but
    an actionable problem about the plane itself, on its own full-width row."""

    def test_the_warning_is_not_inside_the_width_critical_repo_column(self):
        """The left column is padded to `_LEFT_W` with `tui.width`; anything drawn
        inside it that a font renders wider than the Unicode tables claim shifts the
        divider on that row alone. A full-width row has nothing to its right to shear."""
        self.dirty()
        self.switch_to("feat/x")
        line = self.warning()
        self.assertNotIn("│", line, "the warning was laid out inside the two columns")

    def test_column_alignment_holds_at_every_pane_width(self):
        """The frame is the ruler: a row that renders wider than counted pushes its own
        `│` past the others, so drift is visible rather than mysterious."""
        self.dirty()
        self.switch_to("feat/x")
        for w in (40, 60, 80, 120, 200):
            with self.subTest(width=w):
                rows = [ln for ln in self.raw(w) if ln.strip()]
                self.assertEqual(len({statusline.tui.width(ln) for ln in rows}), 1,
                                 f"ragged frame at {w}: {rows}")

    def test_the_divider_still_sits_in_one_column_with_the_warning_present(self):
        self.dirty()
        cols = {ln.find("│", 40) for ln in self.lines() if ln.find("│", 40) > 0}
        self.assertEqual(len(cols), 1, f"divider wanders between columns: {sorted(cols)}")

    def test_no_line_exceeds_a_narrow_pane(self):
        self.dirty()
        self.switch_to("feat/some-very-long-branch-name-that-will-not-fit")
        for w in (24, 30, 40, 80):
            with self.subTest(width=w):
                for ln in self.raw(w):
                    self.assertLessEqual(statusline.tui.width(ln), w)

    def test_the_identity_survives_truncation_on_a_narrow_pane(self):
        """This row's order is its truncation order: on a pane with room for a few words
        the ones that must survive are the ones naming what the line is about."""
        self.dirty()
        self.switch_to("feat/some-very-long-branch-name-that-will-not-fit")
        self.assertIsNotNone(self.warning(40), self.lines(40))


class ItNeverBreaksTheStatusLine(PlaneRootIso):
    """The module's one hard contract: `render` must never raise, and everything added
    to it must fall back to silence rather than take the footer down."""

    def test_an_unreadable_root_yields_no_line_and_no_exception(self):
        config.use(Path("/nonexistent-control-plane-xyz"))
        self.addCleanup(config.use, self.tmp)
        # Forced past the "is there a plane at all" guard, so this exercises the
        # filesystem probe itself against a directory that cannot be read.
        config.HAS_CONTROL_PLANE = True
        self.assertIsNone(statusline._plane_root_alert())

    def test_a_failing_git_status_yields_no_line_and_no_exception(self):
        """`git` missing, a corrupt index, a timeout — the state probe is the one part
        of this that shells out, so it is the part that can fail in the field."""
        self.dirty()
        with mock.patch.object(statusline, "_repo_states", side_effect=RuntimeError("boom")):
            self.assertIsNone(statusline._plane_root_alert())

    def test_render_survives_the_warning_exploding(self):
        with mock.patch.object(statusline, "_plane_root_alert",
                               side_effect=RuntimeError("boom")):
            self.assertTrue(statusline.render({"session_id": "t"}).strip())

    def test_the_other_alerts_survive_the_warning_exploding(self):
        """It is checked LAST inside `_alerts`, whose guard returns what it has already
        collected — so a failure here costs its own line and not the pinned-version
        warning above it, which carries the command that fixes a different problem."""
        from charter import instance
        instance.set_locked_version(config.ROOT, "99.0.0")
        with mock.patch.object(statusline, "_plane_root_alert",
                               side_effect=RuntimeError("boom")):
            self.assertTrue(any("pinned" in a for a in self.alerts()), self.alerts())


if __name__ == "__main__":
    unittest.main()
