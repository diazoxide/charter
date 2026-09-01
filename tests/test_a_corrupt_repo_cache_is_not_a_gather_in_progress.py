"""#735 — a `gather.json` that cannot be read says so, and there is a route out.

The repo panel had ONE sentence for four different readings of the cache. `gather.cached`
degrades to ``None`` for a frame directory that does not exist, a directory with no cache
file in it, a file that is not valid JSON, and a file that parses to something that is not
a scan — and `slots._repos` drew `⋯ gathering this workspace's repos…` for every one of
them. The first two are a gather that has not landed yet and the message is right; the
last two are a gather that will never land, and the message is a permanent lie on a pane
with no other route out of it.

**What tells them apart is a fact, not a duration.** The cache file is on disk and cannot
be read as a scan — asked at the moment the pane is drawn, of the same file the renderer
just failed to read. A time-box ("no cache for N seconds") would answer with a probability
instead: a plane with forty clones on a cold NFS mount crosses any N a corrupt file
crosses, and the pane would call a slow gather broken. `gather.save` writes through
``os.replace`` (its own docstring: "a reader must never observe a half-written cache"), so
there is no window in which a real gather looks unreadable either — which is what makes
the fact stable enough to draw.

Both halves of the issue are pinned here: the third message, and the palette row that
re-gathers without the operator having to know `--session`/`--workspace`.
"""

from __future__ import annotations

import os
import sys
import unittest
from unittest import mock

from charter import statusline as sl
from charter import commands_frame, tui, util
from charter.frame import action as faction
from charter.frame import builtin_actions, gather, slots, state

from tests._isolation import PersonaIso


def _seed(fid: str, **overrides) -> dict:
    """A cache a renderer can read — the shape `gather._entry`/`scan` writes."""
    data = {"gathered_at": 0.0, "workspace": "w", "current_repo": None,
            "repos": [], "worktrees": []}
    data.update(overrides)
    gather.save(fid, data)
    return data


def _corrupt(fid: str, text: str):
    """Put *text* where the cache belongs, whatever it says."""
    d = state.frame_dir(fid, create=True)
    assert d is not None
    f = d / "gather.json"
    f.write_text(text)
    return f


#: The two ways a cache file that EXISTS is still unreadable, which are two different
#: paths through `gather.cached` — `json.loads` raising, and `json.loads` returning
#: something `_shaped_like_a_scan` refuses. Spelled as data so every case below runs
#: against both: a fix that caught only the parse error would pass a test written against
#: one of them and leave the other drawing the old lie.
BROKEN = (("invalid json", "not json {{{"),
          ("parses, but is not a scan", '{"foo": "bar"}'),
          ("parses to a bare list", "[1, 2, 3]"),
          ("a dict whose repos is not a list", '{"repos": 7, "worktrees": []}'))


class TheCacheSaysWhichKindOfNothingItIs(PersonaIso, unittest.TestCase):
    """`gather.unreadable` — the fact `cached()`'s ``None`` throws away.

    `cached()` stays exactly as it is (its docstring promises ``None`` for every way
    reading can fail, and every caller depends on that); this is the second question, asked
    only by the caller that needs to say something different about the answer.
    """

    def test_a_cache_file_that_will_not_read_is_unreadable(self):
        for name, text in BROKEN:
            with self.subTest(name):
                fid = f"f-{abs(hash(name))}"
                _corrupt(fid, text)
                self.assertIsNone(gather.cached(fid))
                self.assertTrue(gather.unreadable(fid))

    def test_a_frame_that_has_never_gathered_is_not_unreadable(self):
        """The cold-start pair, and the reason this is asserted beside the case above: a
        function that answered True whenever there was no cache would satisfy every
        assertion above and turn the launch window into a permanent error message."""
        self.assertIsNone(gather.cached("f-never"))
        self.assertFalse(gather.unreadable("f-never"))
        state.frame_dir("f-dir-only", create=True)
        self.assertIsNone(gather.cached("f-dir-only"))
        self.assertFalse(gather.unreadable("f-dir-only"))

    def test_a_cache_that_reads_is_not_unreadable(self):
        _seed("f-good", repos=[{"name": "demo"}])
        self.assertIsNotNone(gather.cached("f-good"))
        self.assertFalse(gather.unreadable("f-good"))

    def test_a_hostile_frame_id_names_no_cache_and_no_error(self):
        """`state.frame_dir` refuses it, so there is no file to call unreadable — the same
        degrade `cached()` makes one line up."""
        self.assertFalse(gather.unreadable("../../etc"))

    def test_the_gather_that_lands_clears_it(self):
        """A statement about now. Same frame id, one real `gather.save` between the two
        readings — which is what a re-gather actually does."""
        _corrupt("f-heals", "not json {{{")
        self.assertTrue(gather.unreadable("f-heals"))
        _seed("f-heals", repos=[{"name": "demo"}])
        self.assertFalse(gather.unreadable("f-heals"))

    def test_it_reads_the_same_file_cached_reads(self):
        """One reader underneath both, so the pane cannot be told "unreadable" about a
        file `cached()` would have accepted. Asserted by moving the file out from under
        the pair and checking they agree, rather than by reading the implementation."""
        f = _corrupt("f-same", '{"repos": [], "worktrees": []}')
        self.assertIsNotNone(gather.cached("f-same"))
        self.assertFalse(gather.unreadable("f-same"))
        f.unlink()
        self.assertIsNone(gather.cached("f-same"))
        self.assertFalse(gather.unreadable("f-same"))


class TheRepoPaneSaysWhichStateItIsIn(PersonaIso, unittest.TestCase):
    """The renderer's half — through the real `slots.render("repos", …)`, because what
    #735 is about is what an operator READS, not what a helper returns."""

    def _render(self, fid, *, cols=200, rows=24) -> str:
        with mock.patch("os.get_terminal_size",
                        return_value=os.terminal_size((cols, rows))), \
             mock.patch.object(sys.stdout, "fileno", return_value=1, create=True):
            return slots.render("repos", fid)

    def test_a_corrupt_cache_is_not_drawn_as_a_gather_in_progress(self):
        """The defect, at the pane. Asserted as a PAIR against the cold-start frame in the
        same test: "says something else" alone would pass on a renderer that had simply
        stopped saying `gathering` at all, which would make the launch window silent."""
        for name, text in BROKEN:
            with self.subTest(name):
                fid = f"f-r-{abs(hash(name))}"
                _corrupt(fid, text)
                broken = tui.strip_ansi(self._render(fid))
                cold = tui.strip_ansi(self._render("f-cold"))
                self.assertIn("gathering", cold)
                self.assertNotIn("gathering", broken)
                self.assertIn("unreadable", broken)

    def test_the_line_names_the_command_that_fixes_it(self):
        """The shape `_empty_lines` already has: a line that names a problem and not its
        fix costs a row and settles nothing. Both flags, both filled in from the frame the
        pane is drawing — an operator inside a stuck frame can discover neither."""
        _corrupt("f-remedy", "not json {{{")
        out = tui.strip_ansi(self._render("f-remedy"))
        self.assertIn("charter frame-gather", out)
        self.assertIn("--session f-remedy", out)
        self.assertIn(f"--workspace {state.workspace_for('f-remedy')}", out)

    def test_it_is_one_line_and_it_fits_the_pane(self):
        """Every other line in this pane is one row bounded by `tui.truncate`, and a
        `repos` pane that quietly became two rows tall pushes the attention strip off the
        bottom of the window. Measured at `statusline._LEFT_W`, the narrowest width at
        which this pane draws anything at all.

        **The names are long on purpose, and the first version of this test was vacuous
        without them.** `f-width` plus `default` composes to about 80 cells, which fits a
        95-column pane untruncated — so the bound passed identically on a renderer with no
        `tui.truncate` in it at all. A workspace name is arbitrary length (`_empty_lines`
        says so at length about the same pane) and so is a frame id, so the line is
        measured with names that make the unbounded version overflow: the assertion is red
        the moment the truncate goes."""
        fid = "f-" + "w" * 60
        ws = "a" * 60
        _corrupt(fid, "not json {{{")
        with mock.patch.dict(os.environ, {"CHARTER_WORKSPACE": ws}, clear=True):
            self.assertEqual(state.workspace_for(fid), ws)
            out = self._render(fid, cols=sl._LEFT_W)
        self.assertEqual(len(out.split("\n")), 1, out)
        for line in out.split("\n"):
            self.assertLessEqual(tui.width(tui.strip_ansi(line)), sl._LEFT_W, line)

    def test_a_workspace_name_the_rungs_never_checked_is_still_one_line(self):
        """`_empty_lines`' hostile-name case, for the line beside it. `state.workspace_for`
        rung 0 name-checks the pin, but its LAST rung is `workspace.resolve()`, which hands
        back `$CHARTER_WORKSPACE` stripped and otherwise untouched — so a name with a
        newline in it arrives at this renderer verbatim. `tui.truncate` runs `tui.sanitize`
        first, which is what keeps a `repos` pane one row tall.

        The hostile value is asserted to have got THROUGH as well as to have been
        contained: containment alone would pass just as well on a build where a rung had
        rejected it and the line said `default`."""
        hostile = "ev\nil\x1b[31m;rm -rf /"
        with mock.patch.dict(os.environ, {"CHARTER_WORKSPACE": hostile}, clear=True):
            self.assertEqual(state.workspace_for("f-hostile"), hostile)
            _corrupt("f-hostile", "not json {{{")
            out = self._render("f-hostile")
        self.assertEqual(len(out.split("\n")), 1, repr(out))
        self.assertIn("rm -rf /", tui.strip_ansi(out))

    def test_a_gathered_cache_still_draws_its_table(self):
        """The other three states are untouched — the new branch is reached only from
        `cached() is None`, and a renderer that reached it otherwise would replace every
        pane on the plane."""
        _seed("f-full", repos=[{"name": "demo", "branch": "main", "dirty": False,
                                "tracked_dirty": False, "ahead": 0, "behind": 0,
                                "ci": None, "change": None, "sigil": "",
                                "current": False, "worktree_count": 0}])
        out = tui.strip_ansi(self._render("f-full"))
        self.assertIn("demo", out)
        self.assertNotIn("unreadable", out)
        _seed("f-none")
        empty = tui.strip_ansi(self._render("f-none"))
        self.assertIn("no clones", empty)
        self.assertNotIn("unreadable", empty)

    def test_a_pane_too_narrow_for_the_table_says_nothing_new_either(self):
        """`_table_cap` answers 0 below `statusline._LEFT_W` and `_repos` composes nothing
        on a budget of 0, so this line cannot appear where a table never will — the same
        rule the gathering line keeps, asserted for the new one because it is a new
        early-return past the same cap."""
        _corrupt("f-narrow", "not json {{{")
        out = tui.strip_ansi(self._render("f-narrow", cols=90))
        self.assertNotIn("unreadable", out)
        self.assertIn("too narrow", out)

    def test_drawing_the_line_repairs_nothing(self):
        """A renderer does not write. Deleting the operator's file would remove the
        evidence and put a filesystem write on the repaint path that #387 pinned at one
        `stat` — so the file is exactly where it was after the pane has said so."""
        f = _corrupt("f-intact", "not json {{{")
        self._render("f-intact")
        self.assertEqual(f.read_text(), "not json {{{")

    def test_a_repaint_still_runs_no_gather_of_its_own(self):
        """`gather.read`'s live-`scan` fallback is what `_repos` refuses (#512), and a new
        branch that reached for it to "just fix it" would put a git sweep on every repaint
        of a broken frame."""
        _corrupt("f-noscan", "not json {{{")
        with mock.patch.object(gather, "scan",
                               side_effect=AssertionError("a repaint gathered")):
            self.assertIn("unreadable", tui.strip_ansi(self._render("f-noscan")))


class ThePaletteCanReGather(PersonaIso, unittest.TestCase):
    """The route out. `charter frame-gather` already exists and already fixes this
    instantly; what it wants is two flags an operator inside a stuck frame has no way to
    learn. The palette knows both."""

    FID = "f-palette"

    def setUp(self) -> None:
        super().setUp()
        state.frame_dir(self.FID, create=True)

    def _reg(self):
        return builtin_actions.build(self.FID, current_density="normal",
                                     current_chrome="off")

    def test_the_palette_offers_a_re_gather(self):
        a = self._reg().get("frame.gather")
        self.assertIsNotNone(a)
        self.assertIn("gather", a.title)

    def test_it_is_offered_on_a_frame_whose_cache_is_corrupt(self):
        """Availability is the point: this is the ONE row that has to be there on exactly
        the frame every other route has failed on."""
        _corrupt(self.FID, "not json {{{")
        offered = [r.id for r in self._reg().offers(fid=self.FID, snapshot={})
                   if r.available]
        self.assertIn("frame.gather", offered)

    def test_running_it_starts_the_command_with_both_flags_filled_in(self):
        """The whole value of the row. Asserted against `builtin_actions._spawn`, the one
        place a palette row becomes a second charter."""
        a = self._reg().get("frame.gather")
        with mock.patch.object(builtin_actions, "_spawn") as spawn:
            said = a.run(faction.build(a.touches, fid=self.FID, snapshot={}))
        spawn.assert_called_once()
        argv = spawn.call_args.args[0]
        ws = state.workspace_for(self.FID)
        self.assertEqual(argv, util.self_relaunch_argv(
            "frame-gather", "--session", self.FID, "--workspace", ws))
        self.assertEqual(spawn.call_args.kwargs, {"fid": self.FID})
        self.assertTrue(said)

    def test_it_is_the_same_command_the_launch_already_detaches(self):
        """Two ways to start the same gather, and the docstrings say they must not drift —
        so the drift is measured rather than asserted in prose. `commands_frame._spawn_gather`
        is what the LAUNCH fires (#512); this row is what the palette fires. The
        `charter`-side arguments are compared, not the interpreter prefix: `_spawn_gather`
        goes through `util.detach_self` and this goes through `util.self_relaunch_argv`,
        which is a difference about how a child is started rather than about what it is
        asked to do."""
        with mock.patch.object(commands_frame.util, "detach_self",
                               return_value=True) as detach:
            commands_frame._spawn_gather(self.FID, "alpha")
        a = self._reg().get("frame.gather")
        with mock.patch.dict(os.environ, {"CHARTER_WORKSPACE": "alpha"}, clear=True), \
             mock.patch.object(builtin_actions, "_spawn") as spawn:
            a.run(faction.build(a.touches, fid=self.FID, snapshot={}))
        launch = detach.call_args.args[0]
        row = spawn.call_args.args[0]
        self.assertEqual(launch, ["frame-gather", "--session", self.FID,
                                  "--workspace", "alpha"])
        self.assertEqual(row[row.index("frame-gather"):], launch)

    def test_it_names_the_frames_workspace_and_not_this_process_s(self):
        """#512's rule, on the one path that would otherwise re-resolve it: a detached
        child has no terminal of the operator's, so a gather that guessed its own
        workspace would refill the cache from `default` on a plane that is not on it."""
        state.record_workspace(self.FID, "alpha")
        a = self._reg().get("frame.gather")
        with mock.patch.object(builtin_actions, "_spawn") as spawn:
            a.run(faction.build(a.touches, fid=self.FID, snapshot={}))
        self.assertIn("alpha", spawn.call_args.args[0])

    def test_the_receipt_is_one_line_whatever_the_workspace_is_called(self):
        """`_regather` hands back `gathering <ws>…` with no `contain.one_line` of its own,
        and this is the measurement that says it does not need one. `state.workspace_for`'s
        last rung returns `$CHARTER_WORKSPACE` stripped and otherwise unchecked, so a name
        with a newline in it really does reach the receipt — and `actions.Invocation._work`
        contains every receipt on the way to the surface, which is why `_select` hands back
        a bare repo name too.

        Asserted through the real `registry.invoke`, not by calling `run` directly: the
        containment lives on the path between the two, so a test that stopped at `run`
        would be measuring the string this function composes rather than the line an
        operator reads. The hostile value is asserted to have got THROUGH as well as to
        have been contained — containment alone would pass on a build where a rung had
        rejected the name."""
        hostile = "ev\nil-ws"
        with mock.patch.dict(os.environ, {"CHARTER_WORKSPACE": hostile}, clear=True), \
             mock.patch.object(builtin_actions, "_spawn"):
            self.assertEqual(state.workspace_for(self.FID), hostile)
            inv = self._reg().invoke("frame.gather", fid=self.FID, snapshot={})
            self.assertTrue(inv.join(5))
        self.assertEqual(inv.error, "")
        self.assertIn("il-ws", inv.note)
        self.assertEqual(len(inv.note.split("\n")), 1, repr(inv.note))

    def test_listing_the_row_starts_nothing(self):
        """`Action.run` is what spawns. Building the registry and reading the row must
        not — a palette that gathered on open would sweep git every `F2`."""
        with mock.patch.object(builtin_actions, "_spawn") as spawn:
            self._reg().offers(fid=self.FID, snapshot={})
        spawn.assert_not_called()


if __name__ == "__main__":  # pragma: no cover
    unittest.main()
