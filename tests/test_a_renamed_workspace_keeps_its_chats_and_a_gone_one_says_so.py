"""#795 and #752 — the two halves of "the workspace under this frame changed".

They are one subject with two answers, and #795 says so in as many words: *"a workspace
that is gone is a different answer from one that was renamed, and the frame currently
draws both the same way."*

**Renamed — the chat follows, because nothing moved.** `workspace.rename` already repoints
every per-session and per-terminal pointer whose value is the old name
(`_rename_active_pointers`). Since #794 those pointers are no longer rungs of
`state.own_workspace`, so the two rungs that ARE — the recorded `$CHARTER_WORKSPACE` pin
and `state.record_workspace`'s launch record — kept spelling a name the plane no longer
had. Measured on 0.55.0: after `charter workspace rename alpha alpha2`,
`chats.of_workspace("alpha2")` is empty, every chat in it is invisible to
`commands_frame._plane_session` (which finds a plane's live session BY its chats), and the
frame goes on drawing `⬢ alpha`.

**§4j is not in the way, and the design already said so.** *"A chat belongs to its
workspace for life; `{workspace}-{hash}` is identity, not a property"* forbids **moving a
chat between workspaces**. A rename moves no chat: `alpha.1` has the same clones, the same
cwd and the same siblings before and after, and only the workspace's NAME changed.
`state.new_chat_id`'s own docstring settles the mechanism in advance — *"`frame_workspace`
reads the workspace out of the frame's own `workspace` file, **which can be repointed**,
and the bars show that rather than the prefix of the id"* — and the cosmetic cost it names
(chat `alpha.1` beside `alpha2.3`) is exactly what is kept here: **ids are not rewritten**,
because that would break every `$CHARTER_SESSION_ID` already exported into a live process.

**Gone — the frame says so, because something did move.** `_repos` had three sentences for
a workspace that is present (`_unknown_lines`, `_unreadable_lines`, `_empty_lines`) and no
sentence at all for one that is absent, so a removed workspace was drawn as an empty one:
`0 todos`, no rows, and `⋯ gathering this workspace's repos…` forever, on a pane nothing
was coming to correct. That is #735's shape one noun out, and it is answered the same way:
a FACT asked of the filesystem at the moment the pane is drawn (`workspace.exists`), never
a duration and never an inference from an empty scan.

**Asked where the pane was about to say "nothing here", and not above the cache.** A panel
reads its cache or says it has none, so a renderer that overrode a table it still had would
be re-deriving state the gather owns on every repaint. The boundary is asserted rather than
left implied, along with the convergence that makes it safe.

`os.environ` is cleared in every case: `state.workspace_for` reads `$CHARTER_WORKSPACE` and
`$CHARTER_SESSION_ID`, so a developer running the suite inside a live frame would otherwise
be supplying half of every fixture (#519/#521/#528).
"""

from __future__ import annotations

import os
import shutil
import sys
import unittest
from unittest import mock

from charter import config, statusline as sl, tui, workspace
from charter.frame import chats, gather, slots, state

from tests._isolation import PersonaIso


def _plant(fid: str, *, ws: str, pane: str = "%1", pin: str = "") -> None:
    """Make *fid* look like a chat charter launched into *ws*.

    The production writers and never a hand-written file — `record_workspace` is what
    `frame_workspace` reads back — which is `test_a_chat_belongs_to_its_workspace_for_life`'s
    rule and the reason a fixture that stopped agreeing with the launcher fails here rather
    than passing against itself. *pin* is what `_frame_identity_env` puts on the pane for a
    launch that really was pinned; ``""`` is what it emits for one that was not, which is
    the ordinary case and not a missing field.
    """
    state.frame_dir(fid, create=True)
    state.record_workspace(fid, ws)
    state.record_harness_pane(fid, pane)
    state.record_identity(fid, {"CHARTER_HARNESS": "Claude Code",
                                "CHARTER_WORKSPACE": pin, "CHARTER_PERSONA": ""})


def _seed(fid: str, **overrides) -> None:
    """A cache a renderer can read — the shape `gather.scan` writes."""
    data = {"gathered_at": 0.0, "workspace": "w", "current_repo": None,
            "repos": [], "worktrees": []}
    data.update(overrides)
    gather.save(fid, data)


#: One repo row, spelled out because `_table_lines` indexes every field of it.
ROW = {"name": "demo", "branch": "main", "dirty": False, "tracked_dirty": False,
       "ahead": 0, "behind": 0, "ci": None, "change": None, "sigil": "",
       "current": False, "worktree_count": 0}


class AWorkspaceIsThereOrItIsNot(PersonaIso, unittest.TestCase):
    """`workspace.exists` — the one predicate three surfaces now ask.

    It was spelled inline twice before this (`frame/leave.plan`'s `homeless`, and the
    `· workspace was missing` note `commands_frame._reopen_one` prints), both of them fed
    from `state.own_workspace`, which name-checks its answer. The pane does NOT get its
    workspace from there — `state.workspace_for`'s last rung is a bare
    `workspace.resolve()`, which hands back `$CHARTER_WORKSPACE` stripped and otherwise
    untouched — so the name check moves INTO the predicate rather than being assumed of
    every caller.
    """

    def setUp(self):
        super().setUp()
        self.assertIn("edm-test-", str(config.STATE_DIR))

    def test_a_workspace_with_a_directory_exists_and_one_without_does_not(self):
        (config.WORKSPACES_DIR / "alpha").mkdir(parents=True, exist_ok=True)
        self.assertTrue(workspace.exists("alpha"))
        self.assertFalse(workspace.exists("beta"))

    def test_it_is_a_fact_about_now(self):
        """The whole reason the pane may ask it on the repaint path: the answer changes
        under a running frame, which is what #752 is."""
        (config.WORKSPACES_DIR / "alpha").mkdir(parents=True, exist_ok=True)
        self.assertTrue(workspace.exists("alpha"))
        shutil.rmtree(config.WORKSPACES_DIR / "alpha")
        self.assertFalse(workspace.exists("alpha"))

    def test_a_name_that_cannot_name_a_workspace_is_not_one_that_exists(self):
        """**The name check is load-bearing, not decoration.** `WORKSPACES_DIR / ".."` is
        the plane root and `WORKSPACES_DIR / "."` is the workspaces directory itself —
        both real directories, so a predicate that only asked the filesystem would answer
        True for two names that are not workspaces and never can be, and the pane would go
        on drawing a workspace for them. Asserted with values that are DIRECTORIES on
        purpose: a name that merely does not exist would pass either way."""
        config.WORKSPACES_DIR.mkdir(parents=True, exist_ok=True)
        self.assertTrue((config.WORKSPACES_DIR / "..").is_dir())
        self.assertFalse(workspace.exists(".."))
        self.assertTrue((config.WORKSPACES_DIR / ".").is_dir())
        self.assertFalse(workspace.exists("."))
        self.assertFalse(workspace.exists(""))

    def test_a_file_where_a_workspace_would_be_is_not_a_workspace(self):
        config.WORKSPACES_DIR.mkdir(parents=True, exist_ok=True)
        (config.WORKSPACES_DIR / "afile").write_text("not a workspace\n")
        self.assertFalse(workspace.exists("afile"))

    def test_a_directory_charter_cannot_look_into_is_not_one_that_exists(self):
        """`_unreadable`'s rule, said for a caller whose failure mode is a dead pane.
        ``pathlib`` does not swallow ``EACCES`` — `is_dir()` RAISES on a parent the process
        cannot enter, on Linux, and returns False on macOS, "which is how a suite green on
        a laptop goes red on CI". A predicate that let it through would take a panel down
        rather than draw a line."""
        blocked = mock.Mock()
        blocked.is_dir.side_effect = PermissionError("EACCES")
        with mock.patch.object(workspace, "workspace_dir", return_value=blocked):
            self.assertFalse(workspace.exists("alpha"))

    def test_asking_creates_nothing(self):
        """A predicate on a repaint path must not be `ensure` in disguise: a renderer that
        made the directory it was asking about would repair #752 by hiding it, and would
        put a write on the path #387 pinned."""
        self.assertFalse(workspace.exists("ghost"))
        self.assertFalse((config.WORKSPACES_DIR / "ghost").exists())


class ARenameTakesItsChatsWithIt(PersonaIso, unittest.TestCase):
    """#795 — the two rungs of `state.own_workspace`, repointed the way the pointers were.

    No tmux here, deliberately: the strand is a state defect and reproduces on two
    directories. What a rename must NOT do is asserted beside what it must.
    """

    def setUp(self):
        super().setUp()
        self.assertIn("edm-test-", str(config.STATE_DIR))
        self.enterContext(mock.patch.dict(os.environ, {}, clear=True))
        for n in ("alpha", "beta"):
            (config.WORKSPACES_DIR / n).mkdir(parents=True, exist_ok=True)
        _plant("alpha.1", ws="alpha", pane="%1")
        _plant("alpha.2", ws="alpha", pane="%2")
        _plant("beta.1", ws="beta", pane="%3")

    def test_every_chat_in_the_renamed_workspace_is_still_in_it(self):
        """The defect itself, asked of the MEMBERSHIP question (`chats.of_workspace`)
        rather than of the files the rename writes. That is the function
        `commands_frame._plane_session` uses to find a plane's live session — matched on
        this plane's own chat directories and never on a tmux session name — so an empty
        roster for `alpha2` is not cosmetic: `charter -w alpha2` opens a SECOND session
        beside the one already running these chats."""
        self.assertEqual(chats.of_workspace("alpha"), ["alpha.1", "alpha.2"])
        workspace.rename("alpha", "alpha2")
        self.assertEqual(chats.of_workspace("alpha2"), ["alpha.1", "alpha.2"])
        self.assertEqual(chats.of_workspace("alpha"), [])

    def test_the_chat_keeps_the_id_it_was_allocated(self):
        """§4j, and the half of it that is NOT relaxed. `state.new_chat_id` says the
        cosmetic mismatch is deliberate: rewriting ids "would break every
        `$CHARTER_SESSION_ID` already exported into a live process". So the id, the frame
        directory and everything keyed on either stay exactly where they were, and the
        rename shows up only in what the frame SAYS."""
        before = sorted(d.name for d in (config.STATE_DIR / "frame").iterdir())
        workspace.rename("alpha", "alpha2")
        self.assertEqual(chats.of_workspace("alpha2"), ["alpha.1", "alpha.2"])
        self.assertEqual(sorted(d.name for d in (config.STATE_DIR / "frame").iterdir()),
                         before)
        self.assertEqual(before, ["alpha.1", "alpha.2", "beta.1"])

    def test_both_rungs_move_and_the_pinned_chat_moves_too(self):
        """`own_workspace` is the recorded pin, then the launch record — and a fix that
        wrote only the record would leave a PINNED chat exactly as orphaned as before,
        because the pin outranks it. Asserted per rung rather than through the ladder,
        which would pass with either one of them repointed."""
        _plant("alpha.3", ws="alpha", pane="%4", pin="alpha")
        self.assertEqual(state.own_workspace("alpha.3"), "alpha")
        workspace.rename("alpha", "alpha2")
        self.assertEqual(state.frame_workspace("alpha.1"), "alpha2")
        self.assertEqual(state.identity("alpha.3").get("CHARTER_WORKSPACE"), "alpha2")
        self.assertEqual(state.own_workspace("alpha.3"), "alpha2")

    def test_a_pin_is_read_exactly_the_way_membership_reads_it(self):
        """**The deletion sweep found this one, as `strip` -> `lstrip` surviving.**
        `state.own_workspace` answers the pin as
        ``identity(fid).get("CHARTER_WORKSPACE", "").strip()``, and `_frame_identity_env`
        copies the launcher's environment verbatim — so a shell that exported
        ``CHARTER_WORKSPACE="alpha "`` produces a chat whose recorded pin carries a
        trailing space and whose MEMBERSHIP is `alpha` regardless. A walk that compared it
        any other way would repoint a different set from the roster it is following, and
        the chats it skipped would be orphaned by the fix for orphaning.

        Asserted as the pair that makes it a property rather than a spelling: the chat is
        in `alpha`'s roster BEFORE (which is `own_workspace`'s reading) and in `alpha2`'s
        after (which is this walk's). One assertion alone would pass on either function
        being wrong in the same direction. Both ends of the value are exercised, because
        `lstrip` and `rstrip` fail on opposite ones."""
        _plant("alpha.4", ws="beta", pane="%5", pin=" alpha ")
        self.assertEqual(state.own_workspace("alpha.4"), "alpha")
        self.assertIn("alpha.4", chats.of_workspace("alpha"))
        workspace.rename("alpha", "alpha2")
        self.assertEqual(state.identity("alpha.4").get("CHARTER_WORKSPACE"), "alpha2")
        self.assertIn("alpha.4", chats.of_workspace("alpha2"))

    def test_the_rest_of_a_chats_identity_is_not_lost_on_the_way(self):
        """The pin is rewritten by re-writing the identity record, which holds three other
        names a frame's new panes are launched with (`_relayout_pane_env`). A rewrite that
        dropped them would pin another plane's harness onto every pane split after a
        rename."""
        _plant("alpha.3", ws="alpha", pane="%4", pin="alpha")
        workspace.rename("alpha", "alpha2")
        self.assertEqual(state.identity("alpha.3").get("CHARTER_HARNESS"), "Claude Code")

    def test_a_chat_in_another_workspace_is_left_alone(self):
        """`_rename_active_pointers`' rule, one noun out: what moves is every record whose
        VALUE is the old name, and nothing else. A walk that repointed every frame it
        found would sweep the whole plane into the renamed workspace."""
        workspace.rename("alpha", "alpha2")
        self.assertEqual(chats.of_workspace("beta"), ["beta.1"])
        self.assertEqual(state.frame_workspace("beta.1"), "beta")

    def test_a_chat_that_says_nothing_is_not_given_an_identity_it_never_had(self):
        """`state.identity` answers `{}` for a frame launched by a charter that predates
        the record, and that absence is read as "do not take this frame's identity from
        here". A rename that wrote one would hand a migration-era frame a pin nobody set,
        which outranks every rung below it."""
        state.frame_dir("alpha.9", create=True)
        state.record_workspace("alpha.9", "alpha")
        workspace.rename("alpha", "alpha2")
        self.assertEqual(state.identity("alpha.9"), {})
        self.assertEqual(state.frame_workspace("alpha.9"), "alpha2")

    def test_the_running_frame_repaints_instead_of_waiting_for_a_hook(self):
        """A panel repaints on a version bump and on nothing else (`frame/panel.py`'s
        contract). `charter workspace rename` is typed in ANOTHER terminal, so no
        `posttooluse` hook fires in the frame's own session and nothing else is coming —
        a repoint written in silence is a frame that goes on drawing the old name for as
        long as it stays idle, which is #795's own screenshot."""
        before = {f: state.version(f) for f in ("alpha.1", "alpha.2", "beta.1")}
        workspace.rename("alpha", "alpha2")
        self.assertNotEqual(state.version("alpha.1"), before["alpha.1"])
        self.assertNotEqual(state.version("alpha.2"), before["alpha.2"])
        self.assertEqual(state.version("beta.1"), before["beta.1"])

    def test_the_session_and_terminal_pointers_still_follow(self):
        """#794's own repoint, asserted here because this change adds a second walk beside
        it and a refactor that folded one into the other could silently drop it. What it
        buys is unchanged and is NOT membership: every `charter` command in that session
        goes on acting on the workspace the operator renamed."""
        config.SESSIONS_DIR.mkdir(parents=True, exist_ok=True)
        (config.SESSIONS_DIR / "sid.workspace").write_text("alpha\n")
        (config.SESSIONS_DIR / "sid.lock").write_text("alpha\n")
        workspace.rename("alpha", "alpha2")
        self.assertEqual((config.SESSIONS_DIR / "sid.workspace").read_text().strip(),
                         "alpha2")
        self.assertEqual((config.SESSIONS_DIR / "sid.lock").read_text().strip(), "alpha2")

    def test_a_frame_root_that_cannot_be_listed_costs_the_rename_nothing(self):
        """Every writer this walk calls is best-effort and never raises, and the walk keeps
        that posture: `charter workspace rename` has ALREADY moved the directory by the
        time it runs, so an exception here would leave a rename that half-happened and a
        traceback where a receipt should be. The chats then say the old name, which is
        exactly the state this function exists to leave behind less often — not a worse
        one."""
        with mock.patch.object(state.os, "scandir", side_effect=OSError("EACCES")):
            self.assertEqual(workspace.rename("alpha", "alpha2"), [])
        self.assertTrue((config.WORKSPACES_DIR / "alpha2").is_dir())
        self.assertEqual(state.frame_workspace("alpha.1"), "alpha")

    def test_the_command_says_how_many_chats_came_with_it(self):
        """A rename that silently re-labels running conversations is one the operator finds
        out about from a panel. Asserted through the real `cmd_workspace_rename`, because
        what is being measured is the receipt an operator reads — and paired with the
        no-chats case, since a line printed unconditionally would say `0 chat(s)` on every
        rename of an empty workspace and mean nothing."""
        import io
        from contextlib import redirect_stderr
        from types import SimpleNamespace
        from charter import commands_workspace as cw

        buf = io.StringIO()
        with redirect_stderr(buf):
            rc = cw.cmd_workspace_rename(SimpleNamespace(old="alpha", new="alpha2",
                                                         message=None))
        self.assertEqual(rc, 0)
        self.assertIn("2 chat(s) in 'alpha' followed the rename", buf.getvalue())
        self.assertIn("alpha2", buf.getvalue())

        (config.WORKSPACES_DIR / "gamma").mkdir(parents=True, exist_ok=True)
        quiet = io.StringIO()
        with redirect_stderr(quiet):
            self.assertEqual(cw.cmd_workspace_rename(
                SimpleNamespace(old="gamma", new="gamma2", message=None)), 0)
        self.assertNotIn("followed the rename", quiet.getvalue())

    def test_a_rename_of_a_workspace_with_no_chats_moves_nothing(self):
        """The ordinary case, and the pair that stops the walk being a no-op test: a
        function that repointed every frame unconditionally would pass every assertion
        above."""
        workspace.rename("beta", "beta2")
        self.assertEqual(state.frame_workspace("alpha.1"), "alpha")
        self.assertEqual(chats.of_workspace("beta2"), ["beta.1"])


class ARenamedWorkspaceIsDrawnUnderItsNewName(PersonaIso, unittest.TestCase):
    """The two halves composed, which is the thing #795 asked to have settled: the frame
    must tell a rename apart from a removal, and it now does — one follows, one is said."""

    def setUp(self):
        super().setUp()
        self.assertIn("edm-test-", str(config.STATE_DIR))
        self.enterContext(mock.patch.dict(os.environ, {}, clear=True))
        (config.WORKSPACES_DIR / "alpha").mkdir(parents=True, exist_ok=True)
        _plant("alpha.1", ws="alpha", pane="%1")
        slots.VIEWPORT.forget()

    def _render(self, slot, fid, *, cols=200, rows=24) -> str:
        with mock.patch("os.get_terminal_size",
                        return_value=os.terminal_size((cols, rows))), \
             mock.patch.object(sys.stdout, "fileno", return_value=1, create=True):
            return tui.strip_ansi(slots.render(slot, fid))

    def test_the_top_bar_names_the_workspace_the_rename_produced(self):
        workspace.rename("alpha", "alpha2")
        out = self._render("top", "alpha.1")
        self.assertIn("alpha2", out)

    def test_the_repo_pane_does_not_call_a_renamed_workspace_gone(self):
        """The line between the two issues, drawn at the pane. A rename leaves a workspace
        that is THERE, so the pane goes on saying what it said about it — and a fix for
        #752 alone, with no repoint, would have every rename produce this sentence."""
        _seed("alpha.1", repos=[ROW])
        workspace.rename("alpha", "alpha2")
        out = self._render("repos", "alpha.1")
        self.assertIn("demo", out)
        self.assertNotIn("no workspace", out)


class AWorkspaceThatIsGoneIsNotAnEmptyOne(PersonaIso, unittest.TestCase):
    """#752, through the real `slots.render("repos", …)` — what an operator READS.

    The pane's three existing sentences are all about a workspace that is present. Absent
    is a fourth state and it is asked FIRST, above the cache, because a cache outlives the
    directory it was gathered from: the worst reading is not the empty one the issue
    reports, it is a full table of clones that are no longer on disk.
    """

    def setUp(self):
        super().setUp()
        self.assertIn("edm-test-", str(config.STATE_DIR))
        self.enterContext(mock.patch.dict(os.environ, {}, clear=True))
        (config.WORKSPACES_DIR / "alpha").mkdir(parents=True, exist_ok=True)
        _plant("alpha.1", ws="alpha", pane="%1")
        slots.VIEWPORT.forget()
        self.addCleanup(slots.VIEWPORT.forget)

    def _render(self, fid, *, cols=200, rows=24) -> str:
        with mock.patch("os.get_terminal_size",
                        return_value=os.terminal_size((cols, rows))), \
             mock.patch.object(sys.stdout, "fileno", return_value=1, create=True):
            return tui.strip_ansi(slots.render("repos", fid))

    def _remove(self):
        shutil.rmtree(config.WORKSPACES_DIR / "alpha")

    def test_a_gather_that_never_landed_stops_being_the_answer(self):
        """#752's own screenshot: the pane said `⋯ gathering this workspace's repos…` for
        a workspace that is not there, and nothing was coming. Asserted as a PAIR against
        the same frame with its directory intact — "says something else" alone would pass
        on a renderer that had stopped saying `gathering` at all, which would make the
        launch window silent."""
        cold = self._render("alpha.1")
        self.assertIn("gathering", cold)
        self._remove()
        gone = self._render("alpha.1")
        self.assertNotIn("gathering", gone)
        self.assertIn("no workspace alpha", gone)

    def test_an_empty_scan_is_not_how_absence_is_reported(self):
        """`no clones in alpha` is a claim about a workspace that HAS no clones, and
        drawing it for one that is not there is #512's mistake in a new noun. The pair is
        the assertion: an empty workspace that exists still says the old sentence."""
        _seed("alpha.1")
        present = self._render("alpha.1")
        self.assertIn("no clones in alpha", present)
        self._remove()
        gone = self._render("alpha.1")
        self.assertNotIn("no clones", gone)
        self.assertIn("no workspace alpha", gone)

    def test_a_cache_with_rows_in_it_is_still_what_the_pane_draws(self):
        """**The boundary of the fix, asserted so it is a decision and not an oversight.**
        `gather.json` is under `.charter/`, not under `workspaces/`, so removing a
        workspace leaves its last scan exactly where it was — and the pane goes on drawing
        it. The question is asked where the pane was about to say "nothing here", and NOT
        above the cache, because a panel "never gathers on its own — it reads the cache or
        says it has none" (docs/frame.md): a renderer that contradicted its own cache from
        a `stat` would be re-deriving, every repaint of every frame, state the gather owns.

        The residual is a table that is stale rather than a table that is wrong about which
        state it is in, and it converges — `gather.scan` reads `workspace.clones`, which
        answers `[]` for a workspace with no directory, so the first refresh after the
        removal empties the cache and the pane says so. That is measured below rather than
        asserted in prose."""
        _seed("alpha.1", repos=[ROW])
        self.assertIn("demo", self._render("alpha.1"))
        self._remove()
        self.assertIn("demo", self._render("alpha.1"))

    def test_the_first_gather_after_the_removal_empties_the_cache_and_the_pane_says_so(self):
        """The convergence the test above leaves to this one, through the real
        `gather.refresh` — the call every `posttooluse` hook makes (`notify.plane_changed`)
        — rather than by writing an empty cache by hand, which would prove nothing about
        what a real gather of an absent workspace produces."""
        _seed("alpha.1", repos=[ROW])
        self._remove()
        gather.refresh("alpha.1", workspace="alpha", cwd=str(config.ROOT))
        self.assertEqual(gather.cached("alpha.1").get("repos"), [])
        self.assertIn("no workspace alpha", self._render("alpha.1"))

    def test_a_cache_that_cannot_be_read_is_still_reported_as_such(self):
        """#735's sentence is about a file, and it survives — but only where the workspace
        is there to gather. Both orders asserted, because "absent" is checked above the
        cache and a pane that checked it below would answer this one wrongly."""
        d = state.frame_dir("alpha.1", create=True)
        assert d is not None
        (d / "gather.json").write_text("not json {{{")
        self.assertIn("unreadable repo cache", self._render("alpha.1"))
        self._remove()
        self.assertIn("no workspace alpha", self._render("alpha.1"))

    def test_the_line_names_the_workspace_and_the_command_that_makes_it_exist(self):
        """`_empty_lines`' shape, and its reason: a line that names a problem and not its
        fix costs a row and settles nothing. The name is the FRAME's workspace, so the
        command names the workspace the pane is about rather than whatever this process
        would resolve for itself (#512). No key is named, for `_unreadable_lines`' reason
        — a frame running as a window in an operator's own tmux has no `F2` of charter's
        to offer."""
        self._remove()
        out = self._render("alpha.1")
        self.assertIn("charter workspace create alpha", out)
        self.assertNotIn("F2", out)

    def test_it_is_one_line_and_it_fits_the_pane(self):
        """Every line in this pane is one row bounded by `tui.truncate`, and a `repos`
        pane that quietly became two rows tall pushes the attention strip off the bottom
        of the window. Measured at `statusline._LEFT_W` with a name long enough that the
        unbounded version overflows — `_unreadable_lines`' first version was vacuous
        without exactly that, because a short name composes to well under the width."""
        long = "a" * 60
        with mock.patch.dict(os.environ, {"CHARTER_WORKSPACE": long}, clear=True):
            self.assertEqual(state.workspace_for("alpha.1"), long)
            out = self._render("alpha.1", cols=sl._LEFT_W)
        self.assertEqual(len(out.split("\n")), 1, out)
        self.assertLessEqual(tui.width(out), sl._LEFT_W, out)

    def test_a_name_no_rung_ever_checked_is_absent_rather_than_stat_ed(self):
        """**The reason `workspace.exists` name-checks before it joins.**
        `state.workspace_for`'s last rung is a bare `workspace.resolve()`, which hands back
        `$CHARTER_WORKSPACE` stripped and otherwise untouched — so a value that escapes the
        workspaces directory reaches this pane verbatim. `..` is the plane root and it IS a
        directory, so a predicate that only asked the filesystem would answer "present" and
        the pane would go on drawing a workspace for a name that can never be one.

        The hostile value is asserted to have got THROUGH as well as to have been contained:
        containment alone would pass on a build where a rung had rejected it. It reaches
        the pane on ONE frame — one that has recorded nothing, so `workspace_for` falls
        past its own name-checked rungs to `workspace.resolve()`; `_plant`ed chats have a
        record and never see it."""
        for hostile in ("..", "ev\nil\x1b[31m;rm -rf /"):
            with self.subTest(hostile), \
                 mock.patch.dict(os.environ, {"CHARTER_WORKSPACE": hostile}, clear=True):
                self.assertEqual(state.workspace_for("f-nothing-recorded"), hostile)
                out = self._render("f-nothing-recorded")
            self.assertEqual(len(out.split("\n")), 1, repr(out))
            self.assertIn("no workspace", out)

    def test_a_pane_too_narrow_for_the_table_says_that_instead(self):
        """`_table_cap` answers 0 below `statusline._LEFT_W` and `_repos` composes nothing
        on a budget of 0, so this line cannot appear where a table never will — the rule
        every other line in the pane keeps, asserted for the new one because it is a new
        early return past the same cap."""
        self._remove()
        out = self._render("alpha.1", cols=90)
        self.assertNotIn("no workspace", out)
        self.assertIn("too narrow", out)

    def test_drawing_the_line_repairs_nothing(self):
        """A renderer does not write. Re-creating the directory would put a write on the
        path #387 pinned at one `stat` per idle tick, would hide whatever removed it, and
        would hand back a workspace with none of the clones it had."""
        self._remove()
        self._render("alpha.1")
        self.assertFalse((config.WORKSPACES_DIR / "alpha").exists())

    def test_a_repaint_still_runs_no_gather_of_its_own(self):
        """`gather.read`'s live-`scan` fallback is what `_repos` refuses (#512), and a new
        branch that reached for it to "just find out" would put a git sweep on every
        repaint of a frame whose workspace is gone."""
        self._remove()
        with mock.patch.object(gather, "scan",
                               side_effect=AssertionError("a repaint gathered")):
            self.assertIn("no workspace alpha", self._render("alpha.1"))

    def test_a_click_or_a_wheel_notch_on_the_line_is_not_a_selection(self):
        """The three existing one-line answers go through `_Viewport.blank`, which clears
        the click map AND the scroll bound together — a paint that cleared one and left
        the other is the defect that method exists to make unwriteable. A fourth early
        return that skipped it would leave the bound the last TABLE wrote, and every wheel
        notch over one static sentence would answer truthy and repaint it."""
        _seed("alpha.1", repos=[ROW] * 12)
        self._render("alpha.1", rows=6)
        self._remove()
        _seed("alpha.1")
        self._render("alpha.1", rows=6)
        self.assertEqual(slots.VIEWPORT.limit, 0)
        self.assertFalse(slots.VIEWPORT.move(1))
        self.assertIsNone(slots.VIEWPORT.repo_at(0))


if __name__ == "__main__":  # pragma: no cover
    unittest.main()
